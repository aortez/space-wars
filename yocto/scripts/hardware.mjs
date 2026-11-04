import { execFileSync } from 'child_process';
import { existsSync, mkdtempSync, readFileSync, rmdirSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

export const HARDWARE_PROFILES = ['hdmi', 'hyperpixel', 'picade'];
// Fresh images name USB slot A by its kernel name, and the fresh bootstrap
// reads that default as the factory selection. A card names its own slot A.
export const BOOT_MEDIA = Object.freeze({ usb: '/dev/sda2', sd: '/dev/mmcblk0p2' });

export function validateProfile(profile) {
  if (!HARDWARE_PROFILES.includes(profile)) {
    throw new Error(`Select --profile ${HARDWARE_PROFILES.join('|')}; hardware selection is required for flashing.`);
  }
  return profile;
}

export function validateBootMedium(medium) {
  if (!Object.hasOwn(BOOT_MEDIA, medium)) {
    throw new Error(`Select --boot-medium ${Object.keys(BOOT_MEDIA).join('|')}; it names the Pi's root device.`);
  }
  return medium;
}

export function bootMediumCommandLine(text, medium) {
  validateBootMedium(medium);
  const roots = text.split(/\s+/).filter(word => word.startsWith('root='));
  if (text.indexOf('\n') !== text.length - 1 || roots.length !== 1 || roots[0] !== `root=${BOOT_MEDIA.usb}`) {
    throw new Error('The flashed command line is not a fresh image default; its root cannot be selected.');
  }
  return text.replace(/(^|\s)root=\S+/, `$1root=${BOOT_MEDIA[medium]}`);
}

export function deviceProfileText(profile) {
  validateProfile(profile);
  return `# Space-Wars hardware selection; preserved by rootfs-only A/B updates.\n[all]\ninclude spacewars-profiles/${profile}.txt\n[all]\n`;
}

export function readBootIdentity(imagePath) {
  const sidecar = `${imagePath}.boot-id`;
  if (!existsSync(sidecar)) {
    throw new Error(`Missing ${sidecar}. Build a unified image with boot-compatibility metadata first.`);
  }
  const identity = readFileSync(sidecar, 'utf8').trim();
  if (!/^[a-f0-9]{64}$/.test(identity)) {
    throw new Error(`Invalid boot identity in ${sidecar}`);
  }
  return identity;
}

export function assertBootCompatibility(expected, actual) {
  if (!/^[a-f0-9]{64}$/.test(expected) || typeof actual !== 'string' || actual.trim() !== expected) {
    throw new Error(
      'The target boot partition does not match this rootfs image. ' +
      'Rootfs-only A/B updates cannot update the kernel, device trees or hardware profiles. ' +
      'Flash the full .wic.gz image first (with the correct --profile); do not deploy this rootfs alone.'
    );
  }
}

function withBootPartition(partition, run, work) {
  const mountPoint = mkdtempSync(join(tmpdir(), 'spacewars-hardware-'));
  let mounted = false;
  try {
    run('sudo', ['mount', '-t', 'vfat', '-o', 'rw,nosuid,nodev,noexec', partition, mountPoint], { stdio: 'pipe' });
    mounted = true;
    return work(mountPoint);
  } finally {
    // If unmount fails, leave the mount point intact. Never recursively remove it.
    if (mounted) {
      run('sudo', ['umount', mountPoint], { stdio: 'pipe' });
    }
    rmdirSync(mountPoint);
  }
}

export function setBootMedium(utils, device, medium, dryRun, { run = execFileSync } = {}) {
  validateBootMedium(medium);
  if (medium === 'usb') return;
  const partition = utils.getPartitionDevice(device, 1);
  if (dryRun) {
    utils.info(`Would select root ${BOOT_MEDIA[medium]} in ${partition}:/cmdline.txt`);
    return;
  }
  withBootPartition(partition, run, mountPoint => {
    // Read and write through sudo, as for the profile: the FAT mount is private.
    const path = join(mountPoint, 'cmdline.txt');
    const contents = bootMediumCommandLine(run('sudo', ['cat', path], { encoding: 'utf8', stdio: 'pipe' }), medium);
    run('sudo', ['tee', path], { input: contents, stdio: ['pipe', 'ignore', 'pipe'] });
    run('sudo', ['sync', '-f', path], { stdio: 'pipe' });
    if (run('sudo', ['cat', path], { encoding: 'utf8', stdio: 'pipe' }) !== contents) {
      throw new Error('Boot medium readback differs from the selected root.');
    }
    utils.success(`Selected root: ${BOOT_MEDIA[medium]}`);
  });
}

export function setHardwareProfile(utils, device, profile, bootIdentity, dryRun, { run = execFileSync } = {}) {
  const contents = deviceProfileText(profile);
  const partition = utils.getPartitionDevice(device, 1);
  if (dryRun) {
    utils.info(`Would select ${profile} in ${partition}:/spacewars-device.txt`);
    return;
  }

  withBootPartition(partition, run, mountPoint => {
    // FAT inherits the mounting process's umask. Keep these reads privileged:
    // existsSync would report a root-only mount as if the files were missing.
    const bootIdPath = join(mountPoint, 'spacewars-boot-id');
    const presence = run('sudo', ['/bin/sh', '-c',
      'if test -f "$1" && test -f "$2"; then printf present; else printf absent; fi',
      'spacewars-hardware', join(mountPoint, 'spacewars-profiles', `${profile}.txt`), bootIdPath],
    { encoding: 'utf8', stdio: 'pipe' });
    if (presence === 'absent') {
      throw new Error('Flashed image does not contain unified hardware profiles and boot metadata.');
    }
    if (presence !== 'present') throw new Error('Could not inspect the flashed boot filesystem.');
    if (run('sudo', ['cat', bootIdPath], { encoding: 'utf8', stdio: 'pipe' }).trim() !== bootIdentity) {
      throw new Error('Flashed boot identity does not match the image sidecar. Check the selected artifacts.');
    }
    const selectionPath = join(mountPoint, 'spacewars-device.txt');
    run('sudo', ['tee', selectionPath], { input: contents, stdio: ['pipe', 'ignore', 'pipe'] });
    run('sudo', ['sync', '-f', selectionPath], { stdio: 'pipe' });
    if (run('sudo', ['cat', selectionPath], { encoding: 'utf8', stdio: 'pipe' }) !== contents) {
      throw new Error('Hardware profile readback differs from the selected profile.');
    }
    utils.success(`Selected hardware profile: ${profile}`);
  });
}
