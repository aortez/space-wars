import { basename, dirname, join } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const SCRIPTS_DIR = dirname(__filename);

export const YOCTO_DIR = dirname(SCRIPTS_DIR);
export const REPO_DIR = dirname(YOCTO_DIR);
export const WORKSPACE_DIR = dirname(REPO_DIR);
export const MACHINE = 'raspberrypi-spacewars';

export function defaultBuildDir() {
  // Separate both machines and checkouts; share download/sstate caches only.
  return process.env.KAS_BUILD_DIR || join(WORKSPACE_DIR, `.${basename(REPO_DIR)}-yocto-build-${MACHINE}`);
}

export function defaultImageDir() {
  return join(defaultBuildDir(), 'tmp/deploy/images', MACHINE);
}

export function preferredImages(suffix) {
  return [`spacewars-image-${MACHINE}.rootfs.${suffix}`, `spacewars-image.rootfs.${suffix}`];
}
