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

export function setHardwareProfile(utils, device, profile, bootIdentity, dryRun, { run = execFileSync } = {}) {
  const contents = deviceProfileText(profile);
  const partition = utils.getPartitionDevice(device, 1);
  if (dryRun) {
    utils.info(`Would select ${profile} in ${partition}:/spacewars-device.txt`);
    return;
  }

  const mountPoint = mkdtempSync(join(tmpdir(), 'spacewars-hardware-'));
  let mounted = false;
  try {
    run('sudo', ['mount', '-t', 'vfat', '-o', 'rw,nosuid,nodev,noexec', partition, mountPoint], { stdio: 'pipe' });
    mounted = true;
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
  } finally {
    // If unmount fails, leave the mount point intact. Never recursively remove it.
    if (mounted) {
      run('sudo', ['umount', mountPoint], { stdio: 'pipe' });
    }
    rmdirSync(mountPoint);
  }
}
