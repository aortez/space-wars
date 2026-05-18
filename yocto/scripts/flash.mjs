#!/usr/bin/env node

import { execSync } from 'child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, statSync } from 'fs';
import { tmpdir } from 'os';
import { basename, isAbsolute, join, resolve } from 'path';
import { pathToFileURL } from 'url';
import { defaultBuildDir, YOCTO_DIR } from './paths.mjs';

const DEFAULT_IMAGE_DIR = join(defaultBuildDir(), 'tmp/deploy/images/raspberrypi5');
const CONFIG_FILE = join(YOCTO_DIR, '.flash-config.json');
const WIFI_CREDS_FILE = join(YOCTO_DIR, 'wifi-creds.local');
const DEFAULT_HOSTNAME = 'spacewars';
const DEFAULT_USER = 'spacewars';
const DEFAULT_UID = 1000;
const DATA_FREE_PERCENT = 10;
const IMAGE_SUFFIX = '.wic.gz';
const PREFERRED_IMAGES = [
  'spacewars-image-raspberrypi5.rootfs.wic.gz',
  'spacewars-image.rootfs.wic.gz',
];
const SSH_HOST_KEY_PATTERN = /^ssh_host_.*_key(\.pub)?$/;

function usage() {
  console.log(`
Space-Wars Yocto flash tool

Usage:
  npm run flash [options]

Options:
  --device <dev>       Flash directly to device, still with confirmation
  --image <path>       Use a specific .wic.gz image
  --hostname <name>    Write /boot/hostname.txt after flashing [default: spacewars]
  --ssh-key <path>     Public SSH key to inject
  --interactive        Force interactive prompts instead of saved config
  --reconfigure        Re-select and save SSH key
  --list               List candidate devices and exit
  --dry-run            Print actions without changing the target
  -h, --help           Show this help

Examples:
  npm run flash -- --list
  npm run flash -- --dry-run --device /dev/sdb
  npm run flash -- --device /dev/sdb

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

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\''")}'`;
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

async function ensureSshKeyConfig(utils, sshKeyArg, forceReconfigure) {
  const existing = utils.loadConfig(CONFIG_FILE);

  if (sshKeyArg) {
    const sshKeyPath = resolvePath(sshKeyArg);
    if (!existsSync(sshKeyPath)) {
      throw new Error(`SSH key not found: ${sshKeyPath}`);
    }
    const config = { ...(existing ?? {}), ssh_key_path: sshKeyPath };
    utils.saveConfig(CONFIG_FILE, config);
    utils.info(`Using SSH key from CLI: ${basename(sshKeyPath)}`);
    return config;
  }

  if (forceReconfigure) {
    return await utils.configureSSHKey(CONFIG_FILE);
  }

  if (existing) {
    utils.info(`Using SSH key: ${basename(existing.ssh_key_path)}`);
    return existing;
  }

  const defaultKey = findDefaultSshKey();
  if (defaultKey) {
    const config = { ssh_key_path: defaultKey };
    utils.saveConfig(CONFIG_FILE, config);
    utils.info(`Using SSH key: ${basename(defaultKey)}`);
    utils.info(`Config saved to: ${basename(CONFIG_FILE)}`);
    return config;
  }

  utils.info('No SSH key configured yet.');
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

async function maybeBackupData(utils, targetDevice, config, interactive, dryRun) {
  if (dryRun || !utils.hasDataPartition(targetDevice)) {
    return null;
  }

  utils.log('');
  utils.info(`Found existing data partition on ${utils.getPartitionDevice(targetDevice, 4)}`);

  let shouldBackup = (!interactive && config.backup_data !== undefined) ? config.backup_data : null;
  if (shouldBackup === null) {
    const doBackup = await utils.prompt('Backup /data before flashing? (Y/n): ');
    shouldBackup = doBackup.toLowerCase() !== 'n';
  } else {
    utils.info(`Using backup setting from config: ${shouldBackup ? 'yes' : 'no'}`);
  }

  if (!shouldBackup) {
    return null;
  }

  const backupDir = utils.backupDataPartition(targetDevice);
  if (backupDir) {
    return backupDir;
  }

  const continueAnyway = await utils.prompt('Continue without backup? (y/N): ');
  if (continueAnyway.toLowerCase() !== 'y') {
    throw new Error('Aborted.');
  }

  return null;
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

function backupHasWifiConfig(backupDir) {
  if (!backupDir) {
    return false;
  }

  try {
    const connectionsDir = join(backupDir, 'NetworkManager/system-connections');
    return readdirSync(connectionsDir).some(name => name.endsWith('.nmconnection'));
  } catch {
    return false;
  }
}

function backupHasSshHostKeys(backupDir) {
  if (!backupDir) {
    return false;
  }

  try {
    const sshDir = join(backupDir, 'ssh');
    return readdirSync(sshDir).some(name => SSH_HOST_KEY_PATTERN.test(name) && !name.endsWith('.pub'));
  } catch {
    return false;
  }
}

function listRootfsHostKeys(mountPoint) {
  const sshDir = join(mountPoint, 'etc/ssh');
  try {
    return readdirSync(sshDir)
      .filter(name => SSH_HOST_KEY_PATTERN.test(name))
      .map(name => join(sshDir, name));
  } catch {
    return [];
  }
}

function copySshHostKeysToBackup(utils, keyPaths, backupDir) {
  const sshBackupDir = join(backupDir, 'ssh');
  mkdirSync(sshBackupDir, { recursive: true });

  const quotedKeys = keyPaths.map(shellQuote).join(' ');
  execSync(`sudo cp -a ${quotedKeys} ${shellQuote(sshBackupDir)}/`, { stdio: 'pipe' });

  const uid = process.getuid?.() ?? Number(execSync('id -u', { encoding: 'utf-8' }).trim());
  const gid = process.getgid?.() ?? Number(execSync('id -g', { encoding: 'utf-8' }).trim());
  execSync(`sudo chown -R ${uid}:${gid} ${shellQuote(sshBackupDir)}`, { stdio: 'pipe' });

  utils.success('Preserved SSH host keys for /data restore.');
}

function preserveSshHostKeysFromRootfs(utils, targetDevice, backupDir, dryRun) {
  if (dryRun) {
    utils.info('Would preserve existing SSH host keys under /data/ssh if present.');
    return backupDir;
  }

  if (backupHasSshHostKeys(backupDir)) {
    utils.info('/data backup already contains SSH host keys.');
    return backupDir;
  }

  for (const partitionNumber of [2, 3]) {
    const rootfsPartition = utils.getPartitionDevice(targetDevice, partitionNumber);
    const mountPoint = mkdtempSync(join(tmpdir(), 'spacewars-rootfs-keys-'));
    let mounted = false;

    try {
      execSync(`test -b ${shellQuote(rootfsPartition)}`, { stdio: 'pipe' });
      execSync(`sudo mount -o ro ${shellQuote(rootfsPartition)} ${shellQuote(mountPoint)}`, { stdio: 'pipe' });
      mounted = true;

      const keyPaths = listRootfsHostKeys(mountPoint);
      const privateKeys = keyPaths.filter(path => !path.endsWith('.pub'));
      if (privateKeys.length === 0) {
        continue;
      }

      const effectiveBackupDir = backupDir ?? mkdtempSync(join(tmpdir(), `${utils.TEMP_PREFIX ?? 'spacewars-'}data-backup-`));
      copySshHostKeysToBackup(utils, keyPaths, effectiveBackupDir);
      return effectiveBackupDir;
    } catch {
      // The target may not have both rootfs slots yet, or the previous image may
      // not have generated host keys. In either case first boot will generate
      // stable keys in /data/ssh.
    } finally {
      if (mounted) {
        try {
          execSync(`sudo umount ${shellQuote(mountPoint)}`, { stdio: 'pipe' });
        } catch {
          // Ignore cleanup errors.
        }
      }
      rmSync(mountPoint, { recursive: true, force: true });
    }
  }

  return backupDir;
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

  const utils = await loadPiBase();

  utils.log('');
  utils.log(`${utils.colors.bold}${utils.colors.cyan}Space-Wars Yocto Flash Tool${utils.colors.reset}`);
  if (dryRun) {
    utils.log(`${utils.colors.yellow}(dry-run mode - no changes will be made)${utils.colors.reset}`);
  }
  utils.log('');

  const config = await ensureSshKeyConfig(utils, specifiedSshKey, reconfigure);
  const image = findImage(utils, specifiedImage);
  if (!image) {
    throw new Error('No image found. Run "npm run build" first, or pass --image <path>.');
  }

  utils.log('');
  utils.info(`Image: ${image.name}`);
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
  if (listOnly) {
    return;
  }

  const { targetDevice, selectedDeviceInfo } = await selectTargetDevice(
    utils,
    devices,
    config,
    specifiedDevice,
    interactive,
  );

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
    specifiedHostname,
    interactive,
    specifiedDevice,
    dryRun,
  );

  let backupDir = null;
  let restoredData = false;
  try {
    backupDir = await maybeBackupData(utils, targetDevice, config, interactive, dryRun);
    backupDir = preserveSshHostKeysFromRootfs(utils, targetDevice, backupDir, dryRun);
    const backupContainsWifi = backupHasWifiConfig(backupDir);
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
    await utils.injectSSHKey(targetDevice, config.ssh_key_path, DEFAULT_USER, DEFAULT_UID, dryRun);
    await utils.setHostname(targetDevice, hostname, dryRun);

    if (wifiCredentials) {
      await utils.injectWifiCredentials(
        targetDevice,
        wifiCredentials.ssid,
        wifiCredentials.password,
        dryRun,
      );
    }

    if (backupDir) {
      const restoreOkay = utils.restoreDataPartition(targetDevice, backupDir, dryRun);
      if (!restoreOkay) {
        throw new Error(`Failed to restore /data backup. Backup retained at: ${backupDir}`);
      }
      restoredData = true;
      utils.cleanupBackup(backupDir);
      backupDir = null;
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
    }
  } catch (err) {
    if (backupDir) {
      utils.warn(`Backup retained for recovery: ${backupDir}`);
    }
    throw err;
  }
}

main().catch(err => {
  console.error(err.message);
  process.exit(1);
});
