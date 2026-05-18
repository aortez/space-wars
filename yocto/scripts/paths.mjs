import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const SCRIPTS_DIR = dirname(__filename);

export const YOCTO_DIR = dirname(SCRIPTS_DIR);
export const REPO_DIR = dirname(YOCTO_DIR);
export const WORKSPACE_DIR = dirname(REPO_DIR);

export function defaultBuildDir() {
  return process.env.KAS_BUILD_DIR || join(WORKSPACE_DIR, '.space-wars-yocto-build');
}
