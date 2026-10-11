import { basename, dirname, join } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const SCRIPTS_DIR = dirname(__filename);

export const YOCTO_DIR = dirname(SCRIPTS_DIR);
export const REPO_DIR = dirname(YOCTO_DIR);
export const WORKSPACE_DIR = dirname(REPO_DIR);
export const MACHINE = 'raspberrypi-spacewars';

export function gpuBuildEnabled(args = [], env = process.env) {
  if (args.includes('--gpu') && args.includes('--software')) {
    throw new Error('Choose either --gpu or --software, not both.');
  }
  if (args.includes('--gpu')) return true;
  if (args.includes('--software')) return false;
  return env.SPACEWARS_GPU !== '0';
}

export function defaultBuildDir({ gpu, env = process.env } = {}) {
  // Separate both machines and checkouts; share download/sstate caches only.
  gpu ??= gpuBuildEnabled([], env);
  return env.KAS_BUILD_DIR || join(WORKSPACE_DIR, `.${basename(REPO_DIR)}-yocto-build-${MACHINE}${gpu ? '-gpu' : ''}`);
}

export function defaultImageDir() {
  return join(defaultBuildDir(), 'tmp/deploy/images', MACHINE);
}

export function preferredImages(suffix) {
  return [`spacewars-image-${MACHINE}.rootfs.${suffix}`, `spacewars-image.rootfs.${suffix}`];
}
