import { execFileSync } from 'child_process';
import { existsSync, mkdtempSync, readFileSync, rmdirSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

export const HARDWARE_PROFILES = ['hdmi', 'hyperpixel', 'picade'];

export function validateProfile(profile) {
  if (!HARDWARE_PROFILES.includes(profile)) {
    throw new Error(`Select --profile ${HARDWARE_PROFILES.join('|')}; hardware selection is required for flashing.`);
  }
  return profile;
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

export function setHardwareProfile(utils, device, profile, bootIdentity, dryRun) {
  const contents = deviceProfileText(profile);
  const partition = utils.getPartitionDevice(device, 1);
  if (dryRun) {
    utils.info(`Would select ${profile} in ${partition}:/spacewars-device.txt`);
    return;
  }

  const mountPoint = mkdtempSync(join(tmpdir(), 'spacewars-hardware-'));
  let mounted = false;
  try {
    execFileSync('sudo', ['mount', partition, mountPoint], { stdio: 'pipe' });
    mounted = true;
    if (!existsSync(join(mountPoint, 'spacewars-profiles', `${profile}.txt`)) ||
        !existsSync(join(mountPoint, 'spacewars-boot-id'))) {
      throw new Error('Flashed image does not contain unified hardware profiles and boot metadata.');
    }
    if (readFileSync(join(mountPoint, 'spacewars-boot-id'), 'utf8').trim() !== bootIdentity) {
      throw new Error('Flashed boot identity does not match the image sidecar. Check the selected artifacts.');
    }
    const selectionPath = join(mountPoint, 'spacewars-device.txt');
    execFileSync('sudo', ['tee', selectionPath], { input: contents, stdio: ['pipe', 'ignore', 'pipe'] });
    execFileSync('sudo', ['sync', '-f', selectionPath], { stdio: 'pipe' });
    utils.success(`Selected hardware profile: ${profile}`);
  } finally {
    // If unmount fails, leave the mount point intact. Never recursively remove it.
    if (mounted) {
      execFileSync('sudo', ['umount', mountPoint], { stdio: 'pipe' });
    }
    rmdirSync(mountPoint);
  }
}
