#!/usr/bin/env node

import { existsSync, statSync } from 'fs';
import { basename, isAbsolute, join, resolve } from 'path';
import { pathToFileURL } from 'url';
import { defaultImageDir, preferredImages, YOCTO_DIR } from './paths.mjs';
import { readBootIdentity, setHardwareProfile, validateProfile } from './hardware.mjs';
import { ADMIN_USER, backupData, restoreData, readPublicKey, provisionAccess, preserveLegacyHostKeys } from './flash-storage.mjs';

const DEFAULT_IMAGE_DIR = defaultImageDir();
const CONFIG_FILE = join(YOCTO_DIR, '.flash-config.json');
const WIFI_CREDS_FILE = join(YOCTO_DIR, 'wifi-creds.local');
const DEFAULT_HOSTNAME = 'spacewars';
const DEFAULT_USER = 'spacewars';
const DATA_FREE_PERCENT = 10;
const IMAGE_SUFFIX = '.wic.gz';
const PREFERRED_IMAGES = preferredImages('wic.gz');

function usage() {
  console.log(`
Space-Wars Yocto flash tool

Usage:
  npm run flash [options]

Options:
  --device <dev>       Flash directly to device, still with confirmation
  --image <path>       Use a specific .wic.gz image
  --profile <name>     Required: hdmi, hyperpixel, or picade
  --hostname <name>    Write /boot/hostname.txt after flashing [default: spacewars]
  --ssh-key <path>     Runtime public SSH key (also the default owner key)
  --admin-ssh-key <path> Separate owner key for swadmin, with passwordless sudo
  --replace-admin-key  Explicitly replace a restored swadmin key/revocation
  --access-only        Finish SSH key provisioning on an already-flashed image
  --expected-serial <serial> Require this drive identity (required with --access-only)
  --discard-data       Explicitly skip the existing /data backup and restore
  --interactive        Force interactive prompts instead of saved config
  --reconfigure        Re-select and save SSH key
  --list               List candidate devices and exit
  --dry-run            Print actions without changing the target
  -h, --help           Show this help

Examples:
  npm run flash -- --list
  npm run flash -- --dry-run --profile picade --hostname picade --device <verified-device>
  npm run flash -- --profile picade --hostname picade --device <verified-device>

Wi-Fi:
  Create yocto/wifi-creds.local to inject Wi-Fi credentials on flash:
    { "ssid": "MyNetwork", "password": "MySecretPassword" }
`);
}

function argValue(args, name) {
  const index = args.indexOf(name);
  if (index < 0) {
    return null;
  }
  if (index === args.length - 1) {
    throw new Error(`Missing value for ${name}`);
  }
  return args[index + 1];
}

async function loadPiBase() {
  const modulePath = join(YOCTO_DIR, 'meta-pi-base/scripts/lib/index.mjs');
  if (!existsSync(modulePath)) {
    throw new Error(
      `Missing pi-base flash utilities at ${modulePath}. Run "npm run build" once so KAS checks out meta-pi-base.`
    );
  }

  return await import(pathToFileURL(modulePath).href);
}

function resolvePath(path) {
  return isAbsolute(path) ? path : resolve(process.cwd(), path);
}

function imageFromPath(path) {
  const resolved = resolvePath(path);
  if (!existsSync(resolved)) {
    throw new Error(`Image not found: ${resolved}`);
  }

  return {
    name: basename(resolved),
    path: resolved,
    stat: statSync(resolved),
  };
}

function findImage(utils, imageArg) {
  if (imageArg) {
    return imageFromPath(imageArg);
  }

  return utils.findLatestImage(DEFAULT_IMAGE_DIR, IMAGE_SUFFIX, PREFERRED_IMAGES);
}

function findDefaultSshKey() {
  const home = process.env.HOME;
  if (!home) {
    return null;
  }

  for (const name of ['id_ed25519.pub', 'id_rsa.pub']) {
    const path = join(home, '.ssh', name);
    if (existsSync(path)) {
      return path;
    }
  }

  return null;
}

async function ensureSshKeyConfig(utils, sshKeyArg, forceReconfigure, dryRun) {
  const existing = utils.loadConfig(CONFIG_FILE);

  if (sshKeyArg) {
    const sshKeyPath = resolvePath(sshKeyArg);
    if (!existsSync(sshKeyPath)) {
      throw new Error(`SSH key not found: ${sshKeyPath}`);
    }
    const config = { ...(existing ?? {}), ssh_key_path: sshKeyPath };
    if (!dryRun) utils.saveConfig(CONFIG_FILE, config);
    utils.info(`Using SSH key from CLI: ${basename(sshKeyPath)}`);
    return config;
  }

  if (forceReconfigure) {
    if (dryRun) throw new Error('Cannot reconfigure saved settings in --dry-run mode.');
    return await utils.configureSSHKey(CONFIG_FILE);
  }

  if (existing) {
    utils.info(`Using SSH key: ${basename(existing.ssh_key_path)}`);
    return existing;
  }

  const defaultKey = findDefaultSshKey();
  if (defaultKey) {
    const config = { ssh_key_path: defaultKey };
    if (!dryRun) utils.saveConfig(CONFIG_FILE, config);
    utils.info(`Using SSH key: ${basename(defaultKey)}`);
    if (!dryRun) utils.info(`Config saved to: ${basename(CONFIG_FILE)}`);
    return config;
  }

  utils.info('No SSH key configured yet.');
  if (dryRun) throw new Error('Pass --ssh-key <public-key> to preview flashing without saved settings.');
  return await utils.configureSSHKey(CONFIG_FILE);
}

function validateHostname(hostname, utils) {
  const cleaned = hostname.trim();
  if (!/^[a-zA-Z0-9][a-zA-Z0-9-]*$/.test(cleaned)) {
    utils.warn(`Invalid hostname "${cleaned}", using default: ${DEFAULT_HOSTNAME}`);
    return DEFAULT_HOSTNAME;
  }
  return cleaned;
}

async function selectTargetDevice(utils, devices, config, specifiedDevice, interactive) {
  if (specifiedDevice) {
    const found = devices.find(dev => dev.device === specifiedDevice);
    if (!found) {
      throw new Error(`Device ${specifiedDevice} not found or not suitable for flashing.`);
    }
    return { targetDevice: specifiedDevice, selectedDeviceInfo: found };
  }

  if (!interactive && config.device) {
    const found = devices.find(dev => dev.device === config.device);
    if (!found) {
      utils.warn(`Configured device ${config.device} not found, falling back to interactive selection.`);
    } else {
      const validation = utils.validateDeviceIdentity(found, config);
      if (!validation.valid) {
        utils.log('');
        utils.warn('Device identity check failed.');
        utils.warn(validation.reason);
        utils.log('');

        const confirmSerial = await utils.prompt('Type "YES" to confirm this is the correct device: ');
        if (confirmSerial !== 'YES') {
          throw new Error('Aborted. Device identity not confirmed.');
        }
      }

      utils.info(`Using device from config: ${config.device}`);
      return { targetDevice: config.device, selectedDeviceInfo: found };
    }
  }

  const targetDevice = await utils.selectDevice(devices);
  if (!targetDevice) {
    throw new Error('Aborted.');
  }

  const selectedDeviceInfo = devices.find(dev => dev.device === targetDevice);
  return { targetDevice, selectedDeviceInfo };
}

async function chooseHostname(utils, config, hostnameArg, interactive, specifiedDevice, dryRun) {
  let hostname = hostnameArg
    ? validateHostname(hostnameArg, utils)
    : (!interactive && config.hostname) ? config.hostname : DEFAULT_HOSTNAME;

  if (!hostnameArg && (interactive || !config.hostname) && !specifiedDevice && !dryRun) {
    utils.log('');
    const hostnameInput = await utils.prompt(`Device hostname (default: ${hostname}): `);
    if (hostnameInput && hostnameInput.trim()) {
      hostname = validateHostname(hostnameInput, utils);
    }
  } else if (!hostnameArg && !interactive && config.hostname) {
    utils.info(`Using hostname from config: ${hostname}`);
  }

  return hostname;
}

function getBmapPath(imagePath) {
  const bmapPath = imagePath.replace(/\.wic\.gz$/, '.wic.bmap');
  return existsSync(bmapPath) ? bmapPath : null;
}

async function maybeBackupData(utils, targetDevice, discardData, dryRun) {
  if (discardData || !utils.hasDataPartition(targetDevice)) return null;
  if (dryRun) {
    utils.info('Would back up /data with numeric ownership, ACLs and xattrs, then verify every copied file before flashing.');
    return null;
  }
  utils.info('Backing up /data before flashing. A failed or unverified backup stops the flash.');
  return backupData(targetDevice);
}

async function getWifiForFlash(utils, willRestoreBackup, dryRun) {
  if (willRestoreBackup) {
    return null;
  }

  if (dryRun) {
    return utils.loadWifiCredsFile(WIFI_CREDS_FILE);
  }

  return await utils.getWifiCredentials(WIFI_CREDS_FILE);
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes('-h') || args.includes('--help')) {
    usage();
    return;
  }

  const dryRun = args.includes('--dry-run');
  const listOnly = args.includes('--list');
  const reconfigure = args.includes('--reconfigure');
  const interactive = args.includes('--interactive');
  const specifiedDevice = argValue(args, '--device');
  const specifiedImage = argValue(args, '--image');
  const specifiedHostname = argValue(args, '--hostname');
  const specifiedSshKey = argValue(args, '--ssh-key');
  const specifiedAdminKey = argValue(args, '--admin-ssh-key');
  const accessOnly = args.includes('--access-only');
  const expectedSerial = argValue(args, '--expected-serial');
  if (accessOnly && (!specifiedDevice || !expectedSerial)) {
    throw new Error('--access-only requires --device and --expected-serial.');
  }
  const profile = listOnly ? null : validateProfile(argValue(args, '--profile'));

  const utils = await loadPiBase();

  utils.log('');
  utils.log(`${utils.colors.bold}${utils.colors.cyan}Space-Wars Yocto Flash Tool${utils.colors.reset}`);
  if (dryRun) {
    utils.log(`${utils.colors.yellow}(dry-run mode - no changes will be made)${utils.colors.reset}`);
  }
  utils.log('');

  if (listOnly) {
    utils.displayDevices(utils.getBlockDevices());
    return;
  }

  const config = await ensureSshKeyConfig(utils, specifiedSshKey, reconfigure, dryRun);
  const runtimeKey = readPublicKey(config.ssh_key_path);
  const adminKey = readPublicKey(resolvePath(specifiedAdminKey ?? config.ssh_key_path));
  utils.info(`Owner login: ${ADMIN_USER} (SSH public key, passwordless sudo).`);
  const image = findImage(utils, specifiedImage);
  if (!image) {
    throw new Error('No image found. Run "npm run build" first, or pass --image <path>.');
  }
  // Reject older Pi 5 images before any backup, mount or destructive write.
  const bootIdentity = readBootIdentity(image.path);

  utils.log('');
  utils.info(`Image: ${image.name}`);
  utils.info(`Hardware profile: ${profile}`);
  utils.info(`Boot assets: ${bootIdentity.substring(0, 16)}...`);
  utils.info(`Size: ${utils.formatBytes(image.stat.size)}`);
  utils.info(`Built: ${image.stat.mtime.toLocaleString()}`);
  if (getBmapPath(image.path)) {
    utils.info('Bmap: available (faster flashing)');
  }
  utils.log('');

  const devices = utils.getBlockDevices();
  if (devices.length === 0) {
    throw new Error('No suitable removable USB/block devices found.');
  }

  utils.displayDevices(devices);
  const { targetDevice, selectedDeviceInfo } = await selectTargetDevice(
    utils,
    devices,
    config,
    specifiedDevice,
    interactive,
  );

  if (expectedSerial && selectedDeviceInfo.serial !== expectedSerial) {
    throw new Error('The selected drive serial does not match --expected-serial.');
  }
  if (accessOnly) {
    if (dryRun) {
      utils.info(`Would finish SSH key provisioning on ${targetDevice} after checking the installed root identity.`);
    } else {
      const access = provisionAccess(targetDevice, runtimeKey, adminKey, {
        replaceAdmin: args.includes('--replace-admin-key'), bootIdentity,
      });
      utils.info(`Administrator SSH access: ${access}.`);
      if (access === 'preserved') utils.info('Use the existing administrator key. Empty/revoked access stays revoked; --replace-admin-key explicitly provisions the selected key.');
      utils.success('SSH access provisioning complete. Keys verified and filesystems unmounted.');
    }
    return;
  }

  if (utils.isLargeDevice(selectedDeviceInfo)) {
    utils.log('');
    utils.warn('Large device detected.');
    utils.warn(`${selectedDeviceInfo.device}: ${selectedDeviceInfo.size} (${selectedDeviceInfo.model})`);
    const confirmLarge = await utils.prompt('Type "YES" to confirm flashing this large device: ');
    if (confirmLarge !== 'YES') {
      throw new Error('Aborted. Large device not confirmed.');
    }
  }

  if (!dryRun) {
    utils.log('');
    utils.info('Checking host tools required for data partition resize...');
    utils.ensureGrowDataPartitionDependencies();
  }

  const hostname = await chooseHostname(
    utils,
    config,
    specifiedHostname ?? (profile === 'picade' ? 'picade' : null),
    interactive,
    specifiedDevice,
    dryRun,
  );

  let backupDir = null;
  let restoredData = false;
  try {
    backupDir = await maybeBackupData(utils, targetDevice, args.includes('--discard-data'), dryRun);
    if (!dryRun) backupDir = preserveLegacyHostKeys(targetDevice, backupDir);
    const backupContainsWifi = backupDir?.hasWifi ?? false;
    if (backupDir && !backupContainsWifi && !dryRun) {
      utils.info('/data backup does not contain Wi-Fi credentials; configuring Wi-Fi for this flash.');
    }
    const wifiCredentials = await getWifiForFlash(utils, backupContainsWifi, dryRun);

    await utils.flashImage(image.path, targetDevice, {
      dryRun,
      bmapPath: getBmapPath(image.path),
      skipConfirm: (!interactive && config.skip_confirmation) || false,
    });

    utils.growDataPartition(targetDevice, DATA_FREE_PERCENT, dryRun);
    await utils.setHostname(targetDevice, hostname, dryRun);
    setHardwareProfile(utils, targetDevice, profile, bootIdentity, dryRun);

    if (backupDir && !dryRun) {
      restoreData(targetDevice, backupDir);
      restoredData = true;
      utils.success(`Data restored and verified. Backup retained: ${backupDir.directory}`);
    }

    if (wifiCredentials) {
      await utils.injectWifiCredentials(
        targetDevice,
        wifiCredentials.ssid,
        wifiCredentials.password,
        dryRun,
      );
    }

    if (dryRun) {
      utils.info('Would restore and verify /data, provision the selected owner key, and preserve existing admin keys unless --replace-admin-key is selected.');
    } else {
      const access = provisionAccess(targetDevice, runtimeKey, adminKey, { replaceAdmin: args.includes('--replace-admin-key'), bootIdentity });
      utils.info(`Administrator SSH access: ${access}.`);
      if (access === 'preserved') utils.info('Use the existing administrator key. Empty/revoked access stays revoked; --replace-admin-key explicitly provisions the selected key.');
    }

    if (!dryRun) {
      config.device = targetDevice;
      config.device_serial = selectedDeviceInfo.serial;
      config.device_model = selectedDeviceInfo.model;
      config.device_size = selectedDeviceInfo.size;
      config.hostname = hostname;
      utils.saveConfig(CONFIG_FILE, config);
    }

    utils.log('');
    if (dryRun) {
      utils.success('Dry run complete.');
      utils.info('Run without --dry-run to actually flash.');
    } else {
      utils.success('Flash complete.');
      if (restoredData) {
        utils.success('/data restored. Wi-Fi credentials were preserved if present.');
      } else if (wifiCredentials) {
        utils.success(`Wi-Fi "${wifiCredentials.ssid}" configured.`);
      } else {
        utils.warn('No Wi-Fi credentials were injected.');
      }
      utils.info(`Boot the Pi and try: ssh ${DEFAULT_USER}@${hostname}.local`);
      utils.info(`Administration: ssh ${ADMIN_USER}@${hostname}.local, then sudo -i`);
    }
  } catch (err) {
    if (backupDir) {
      utils.warn(`Backup retained for recovery: ${backupDir.directory}`);
    }
    throw err;
  }
}

main().catch(err => {
  console.error(err.message);
  process.exit(1);
});
