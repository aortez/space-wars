#!/usr/bin/env node

import { spawn } from 'child_process';
import { accessSync, constants, existsSync, lstatSync, mkdirSync, readlinkSync, rmSync, symlinkSync } from 'fs';
import { delimiter, join } from 'path';
import { defaultBuildDir, YOCTO_DIR } from './paths.mjs';

const BUILD_DIR = defaultBuildDir();

function findExecutable(name) {
  for (const dir of (process.env.PATH ?? '').split(delimiter)) {
    const candidate = join(dir, name);
    try {
      accessSync(candidate, constants.X_OK);
      return candidate;
    } catch {
      // Keep looking.
    }
  }

  return null;
}

function linkExecutable(link, target) {
  if (existsSync(link)) {
    const stat = lstatSync(link);
    if (stat.isSymbolicLink() && readlinkSync(link) === target) {
      return;
    }

    rmSync(link, { force: true });
  }

  symlinkSync(target, link);
}

function prepareHosttools() {
  const gnuInstall = findExecutable('gnuinstall');
  const env = {
    ...process.env,
    KAS_BUILD_DIR: BUILD_DIR,
  };

  console.log(`Using Yocto build directory: ${BUILD_DIR}`);
  if (gnuInstall === null) {
    return env;
  }

  const hosttoolsDir = join(YOCTO_DIR, '.hosttools');
  mkdirSync(hosttoolsDir, { recursive: true });

  const installLink = join(hosttoolsDir, 'install');
  linkExecutable(installLink, gnuInstall);

  const bitbakeHosttoolsDir = join(BUILD_DIR, 'tmp', 'hosttools');
  if (existsSync(bitbakeHosttoolsDir)) {
    linkExecutable(join(bitbakeHosttoolsDir, 'install'), installLink);
  }

  console.log(`Using GNU install for Yocto hosttools: ${gnuInstall}`);
  return {
    ...env,
    PATH: `${hosttoolsDir}${delimiter}${process.env.PATH ?? ''}`,
  };
}

function showHelp() {
  console.log(`
Space-Wars Yocto build

Usage:
  npm run build [-- kas args...]

Examples:
  npm run build
  npm run build -- --target spacewars-image
`);
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes('-h') || args.includes('--help')) {
    showHelp();
    return;
  }

  const config = join(YOCTO_DIR, 'kas-spacewars.yml');
  const env = prepareHosttools();
  const child = spawn('kas', ['build', config, ...args], {
    cwd: YOCTO_DIR,
    env,
    stdio: 'inherit',
  });

  const exitCode = await new Promise((resolve, reject) => {
    child.on('close', resolve);
    child.on('error', reject);
  });

  if (exitCode !== 0) {
    throw new Error(`kas exited with code ${exitCode}`);
  }
}

main().catch(err => {
  console.error(err.message);
  process.exit(1);
});
