#!/usr/bin/env node

import { spawn } from 'child_process';
import { join } from 'path';
import { YOCTO_DIR } from './paths.mjs';

function usage() {
  console.log(`
Space-Wars update

Usage:
  npm run update [options]

Options:
  --target <host>       Target host [default: spacewars.local]
  --user <user>         SSH user [default: spacewars]
  --remote-tmp <path>   Remote staging directory [default: /tmp]
  --image <path>        Use a specific rootfs .ext4.gz image
  --ssh-key <path>      Public SSH key to inject into the updated slot
  --skip-build          Push the existing image without rebuilding
  --dry-run             Print the update actions without changing the Pi
  --prompt              Ask for the final "yolo" confirmation
  -h, --help            Show this help

Examples:
  npm run update
  npm run update -- --skip-build
  npm run update -- --target spacewars.local --dry-run
`);
}

async function run(cmd, args, options = {}) {
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

async function main() {
  const args = process.argv.slice(2);
  if (args.includes('-h') || args.includes('--help')) {
    usage();
    return;
  }

  const yoloArgs = args.filter(arg => arg !== '--prompt');
  if (!args.includes('--prompt') && !args.includes('--yes') && !args.includes('--hold-my-mead')) {
    yoloArgs.push('--yes');
  }

  await run(process.execPath, [join(YOCTO_DIR, 'scripts/yolo-update.mjs'), ...yoloArgs], {
    cwd: YOCTO_DIR,
  });
}

main().catch(err => {
  console.error(err.message);
  process.exit(1);
});
