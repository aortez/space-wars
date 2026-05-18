#!/usr/bin/env node

import { existsSync, statSync } from 'fs';
import { basename, isAbsolute, join, resolve } from 'path';
import { pathToFileURL } from 'url';
import { spawn } from 'child_process';
import { defaultBuildDir, YOCTO_DIR } from './paths.mjs';

const DEFAULT_IMAGE_DIR = join(defaultBuildDir(), 'tmp/deploy/images/raspberrypi5');
const CONFIG_FILE = join(YOCTO_DIR, '.flash-config.json');
const DEFAULT_HOST = 'spacewars.local';
const DEFAULT_USER = 'spacewars';
const DEFAULT_REMOTE_TMP = '/tmp';
const IMAGE_SUFFIX = '.ext4.gz';
const PREFERRED_IMAGES = [
  'spacewars-image-raspberrypi5.rootfs.ext4.gz',
  'spacewars-image.rootfs.ext4.gz',
];

function usage() {
  console.log(`
Space-Wars Yocto OTA update

Usage:
  npm run yolo [options]

Options:
  --target <host>       Target host [default: spacewars.local]
  --user <user>         SSH user [default: spacewars]
  --remote-tmp <path>   Remote staging directory [default: /tmp]
  --image <path>        Use a specific rootfs .ext4.gz image
  --ssh-key <path>      Public SSH key to inject into the updated slot
  --skip-build          Push the existing image without rebuilding
  --dry-run             Print the update actions without changing the Pi
  --yes                 Skip the final yolo confirmation
  --hold-my-mead        Alias for --yes
  -h, --help            Show this help

Examples:
  npm run yolo -- --skip-build
  npm run yolo -- --target spacewars.local --yes
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

function resolvePath(path) {
  return isAbsolute(path) ? path : resolve(process.cwd(), path);
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

async function runCommand(cmd, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { stdio: 'inherit', ...options });
    child.on('close', code => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`${cmd} exited with code ${code}`));
      }
    });
    child.on('error', reject);
  });
}

async function loadPiBase() {
  const modulePath = join(YOCTO_DIR, 'meta-pi-base/scripts/lib/index.mjs');
  if (!existsSync(modulePath)) {
    throw new Error(
      `Missing pi-base update utilities at ${modulePath}. Run "npm run build" once so KAS checks out meta-pi-base.`
    );
  }

  return await import(pathToFileURL(modulePath).href);
}

function resolveSshKey(utils, sshKeyArg) {
  if (sshKeyArg) {
    const sshKeyPath = resolvePath(sshKeyArg);
    if (!existsSync(sshKeyPath)) {
      throw new Error(`SSH key not found: ${sshKeyPath}`);
    }
    return sshKeyPath;
  }

  const config = utils.loadConfig(CONFIG_FILE);
  if (config?.ssh_key_path && existsSync(config.ssh_key_path)) {
    return config.ssh_key_path;
  }

  return findDefaultSshKey();
}

function findImage(utils, imageArg) {
  if (imageArg) {
    return imageFromPath(imageArg);
  }

  return utils.findLatestImage(DEFAULT_IMAGE_DIR, IMAGE_SUFFIX, PREFERRED_IMAGES);
}

function checkRemoteOtaPrivilege(utils, remoteTarget) {
  const probe = [
    'rm -f /tmp/spacewars-ota-sudo-check',
    'sudo -n /usr/sbin/ab-update-with-key --spacewars-sudo-probe >/tmp/spacewars-ota-sudo-check 2>&1',
    "if grep -Eqi 'password|not allowed|may not run sudo' /tmp/spacewars-ota-sudo-check; then echo blocked; else echo ok; fi",
    'rm -f /tmp/spacewars-ota-sudo-check',
  ].join('; ');

  return utils.ssh(remoteTarget, probe) === 'ok';
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes('-h') || args.includes('--help')) {
    usage();
    return;
  }

  const targetHost = argValue(args, '--target') ?? argValue(args, '--host') ?? DEFAULT_HOST;
  const remoteUser = argValue(args, '--user') ?? DEFAULT_USER;
  const remoteTmp = argValue(args, '--remote-tmp') ?? DEFAULT_REMOTE_TMP;
  const imageArg = argValue(args, '--image');
  const sshKeyArg = argValue(args, '--ssh-key');
  const skipBuild = args.includes('--skip-build');
  const dryRun = args.includes('--dry-run');
  const skipConfirm = args.includes('--yes') || args.includes('--hold-my-mead');
  const remoteTarget = `${remoteUser}@${targetHost}`;

  if (!skipBuild) {
    console.log('');
    console.log('Building Space-Wars image...');
    await runCommand(process.execPath, [join(YOCTO_DIR, 'scripts/build.mjs')], { cwd: YOCTO_DIR });
  }

  const utils = await loadPiBase();

  utils.log('');
  utils.log(`${utils.colors.bold}${utils.colors.cyan}Space-Wars OTA Update${utils.colors.reset}`);
  if (dryRun) {
    utils.log(`${utils.colors.yellow}(dry-run mode - no changes will be made)${utils.colors.reset}`);
  }
  utils.log('');

  const image = findImage(utils, imageArg);
  if (!image) {
    throw new Error('No rootfs image found. Run "npm run build" first, or pass --image <path>.');
  }

  utils.info(`Image: ${image.name}`);
  utils.info(`Size: ${utils.formatBytes(image.stat.size)}`);
  utils.info(`Built: ${image.stat.mtime.toLocaleString()}`);
  utils.info(`Target: ${remoteTarget}`);
  utils.info(`Remote staging: ${remoteTmp}`);

  const sshKeyPath = resolveSshKey(utils, sshKeyArg);
  if (sshKeyPath) {
    utils.info(`SSH key: ${basename(sshKeyPath)}`);
  } else {
    utils.warn('No SSH key found. The updated slot will not receive an injected authorized_keys file.');
  }

  const remoteReachable = utils.checkRemoteReachable(targetHost, remoteTarget);
  if (!remoteReachable) {
    if (!dryRun) {
      throw new Error(`Cannot reach ${remoteTarget} over SSH.`);
    }
    utils.warn(`Cannot reach ${remoteTarget} over SSH; continuing because this is a dry run.`);
  } else {
    utils.success(`${targetHost} is reachable`);
  }

  if (!dryRun && !checkRemoteOtaPrivilege(utils, remoteTarget)) {
    throw new Error(
      [
        'Remote image is not OTA-ready for the spacewars user.',
        'The current Pi image needs a sudoers rule allowing:',
        '  sudo /usr/sbin/ab-update-with-key ...',
        '  sudo /usr/bin/systemctl reboot',
        'Flash one OTA-capable image first, then future updates can use npm run update.',
      ].join('\n')
    );
  }

  if (remoteReachable) {
    const remoteSpace = utils.getRemoteTmpSpace(remoteTarget, remoteTmp);
    if (remoteSpace < image.stat.size) {
      throw new Error(
        `Not enough space in ${remoteTmp} on ${targetHost}. Need ${utils.formatBytes(image.stat.size)}, have ${utils.formatBytes(remoteSpace)}.`
      );
    }
    utils.success(`Remote staging has enough space (${utils.formatBytes(remoteSpace)} available)`);
  } else {
    utils.warn(`Skipping ${remoteTmp} free-space check because SSH is unavailable.`);
  }

  utils.info('Calculating checksum...');
  const checksum = await utils.calculateChecksum(image.path);
  utils.success(`Checksum: ${checksum.substring(0, 16)}...`);

  utils.banner('Transferring rootfs to Pi');
  const { remoteImagePath, remoteChecksumPath } = await utils.transferImage(
    image.path,
    checksum,
    remoteTarget,
    remoteTmp,
    dryRun,
  );

  if (!dryRun && !utils.verifyRemoteChecksum(remoteImagePath, remoteChecksumPath, remoteTarget)) {
    throw new Error('Transfer checksum verification failed.');
  }

  let remoteKeyPath = null;
  if (sshKeyPath) {
    remoteKeyPath = `${remoteTmp}/spacewars-authorized_keys.pub`;
    if (dryRun) {
      utils.info(`Would transfer SSH key to ${remoteTarget}:${remoteKeyPath}`);
    } else {
      utils.info('Transferring SSH key...');
      await utils.run('scp', [
        '-o', 'ConnectTimeout=10',
        '-o', 'BatchMode=yes',
        sshKeyPath,
        `${remoteTarget}:${remoteKeyPath}`,
      ]);
      utils.success('SSH key transferred');
    }
  }

  const originalBootTime = utils.getRemoteBootTime(remoteTarget);

  utils.banner('Flashing inactive slot on Pi');
  await utils.remoteFlashWithKey(
    remoteImagePath,
    remoteKeyPath,
    remoteUser,
    remoteTarget,
    dryRun,
    skipConfirm,
    'sudo /usr/sbin/ab-update-with-key',
  );

  if (dryRun) {
    utils.success('Dry run complete.');
    return;
  }

  utils.banner('Waiting for Pi to reboot');
  const online = await utils.waitForReboot(remoteTarget, targetHost, originalBootTime, 180);
  if (!online) {
    utils.warn('The Pi did not come back online within the timeout.');
    return;
  }

  utils.success('OTA update complete.');
  utils.info(`Connect with: ssh ${remoteTarget}`);
}

main().catch(err => {
  console.error(err.message);
  process.exit(1);
});
