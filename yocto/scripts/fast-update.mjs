#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline/promises';
import { defaultImageDir, MACHINE, YOCTO_DIR } from './paths.mjs';

const NAMES = ['engine-client', 'spacewars-cli'];
const HELPER = '/usr/sbin/spacewars-fast-update';
const SSH_OPTIONS = ['-o', 'BatchMode=yes', '-o', 'ConnectTimeout=5', '-o', 'ServerAliveInterval=10', '-o', 'ServerAliveCountMax=3'];
const HASH = /^[a-f0-9]{64}$/;

export function parseArgs(args) {
  const options = { host: 'spacewars.local', user: 'spacewars', skipBuild: false, dryRun: false, prompt: false };
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (['--target', '--host', '--user'].includes(arg)) {
      const value = args[++i];
      if (!value || value.startsWith('-')) throw new Error(`Missing value for ${arg}`);
      options[arg === '--user' ? 'user' : 'host'] = value;
    } else if (arg === '--skip-build') options.skipBuild = true;
    else if (arg === '--dry-run') options.dryRun = true;
    else if (arg === '--prompt') options.prompt = true;
    else if (!['--fast', '--yes', '--hold-my-mead'].includes(arg)) {
      throw new Error(`Unsupported fast-update option: ${arg}. Image/key/staging options are for full updates only.`);
    }
  }
  if (!/^[a-zA-Z0-9][a-zA-Z0-9._-]*$/.test(options.host)) throw new Error('Invalid target hostname or IPv4 address.');
  if (options.user !== 'spacewars') throw new Error('Fast updates use the image’s restricted spacewars account.');
  return options;
}

export function readBundle(directory) {
  let manifest;
  try { manifest = JSON.parse(readFileSync(join(directory, 'manifest.json'), 'utf8')); }
  catch (error) { throw new Error(`Missing/invalid fast-update bundle; run npm run build -- --target spacewars. ${error.message}`); }
  if (manifest.format !== 1 || manifest.machine !== MACHINE || manifest.target_arch !== 'aarch64'
      || !HASH.test(manifest.compatibility)) throw new Error('Incompatible fast-update manifest.');
  if (Object.keys(manifest.binaries ?? {}).sort().join(',') !== [...NAMES].sort().join(',')) {
    throw new Error('Fast update requires exactly the matching engine-client and spacewars-cli pair.');
  }
  for (const name of NAMES) {
    const file = readFileSync(join(directory, name));
    const entry = manifest.binaries[name];
    if (file.length < 64 || file.subarray(0, 7).toString('hex') !== '7f454c46020101' || file.readUInt16LE(18) !== 183) {
      throw new Error(`${name} is not an AArch64 ELF executable.`);
    }
    if (file.length !== entry.size || createHash('sha256').update(file).digest('hex') !== entry.sha256) {
      throw new Error(`${name} size/checksum mismatch; rebuild the bundle.`);
    }
  }
  return manifest;
}

export function checkRemoteIdentity(reply, expected) {
  if (reply.trim() !== `spacewars-fast-update-v1 ${expected}`) {
    throw new Error('Fast-update helper/runtime does not match this build. Perform a normal full update first (without --fast).');
  }
}

export function run(command, args, { capture = false, ...options } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: capture ? ['ignore', 'pipe', 'inherit'] : 'inherit', ...options });
    let output = '';
    if (capture) child.stdout.on('data', chunk => { output += chunk; });
    child.on('error', reject);
    child.on('close', code => code === 0 ? resolve(output.trim()) : reject(new Error(`${command} exited with code ${code}`)));
  });
}

export async function fastUpdate(options, { execute = run, bundleDir = join(defaultImageDir(), 'spacewars-fast'), loadBundle = readBundle } = {}) {
  const target = `${options.user}@${options.host}`;
  console.log(`Fast update: ${target} (client + CLI, application restart, no reboot)`);
  if (options.dryRun) {
    console.log(`${options.skipBuild ? 'Use existing' : 'Build --target spacewars, then use'} bundle: ${bundleDir}`);
    console.log('Check restricted helper/runtime compatibility; verify hashes; copy pair; restart and health-check; rollback on failure.');
    console.log('Dry run: no build, SSH connection, files transferred, or remote changes.');
    return;
  }

  const ssh = (command, capture = false) => execute('ssh', [...SSH_OPTIONS, target, command], { capture });
  // Fail before an expensive build if this image has not been bootstrapped.
  let identity;
  try { identity = await ssh(`sudo -n ${HELPER} --check`, true); }
  catch (error) { throw new Error(`Cannot use fast updates on ${target}: ${error.message}. Install the helper with one normal full update first.`); }
  if (!/^spacewars-fast-update-v1 [a-f0-9]{64}$/.test(identity)) {
    throw new Error('Missing/unsupported fast-update helper; perform a normal full update first.');
  }
  if (!options.skipBuild) {
    await execute(process.execPath, [join(YOCTO_DIR, 'scripts/build.mjs'), '--target', 'spacewars'], { cwd: YOCTO_DIR });
  }
  const manifest = loadBundle(bundleDir);
  checkRemoteIdentity(identity, manifest.compatibility);
  for (const name of NAMES) console.log(`${name}: ${manifest.binaries[name].size} bytes, sha256=${manifest.binaries[name].sha256}`);
  if (options.prompt) {
    const prompt = createInterface({ input: process.stdin, output: process.stdout });
    try {
      if ((await prompt.question(`Restart the application on ${target}? Type "yes": `)).trim() !== 'yes') {
        throw new Error('Update cancelled.');
      }
    } finally { prompt.close(); }
  }

  const stage = await ssh('mktemp -d /tmp/spacewars-fast.XXXXXXXXXX', true);
  if (!/^\/tmp\/spacewars-fast\.[a-zA-Z0-9]{10}$/.test(stage)) throw new Error('Unexpected remote staging directory.');
  try {
    // No remote shell interpolation of local paths, and only a validated fixed
    // staging path on the remote side. The helper checks hashes again after copy.
    await execute('scp', [...SSH_OPTIONS, ...NAMES.map(name => join(bundleDir, name)), `${target}:${stage}/`]);
    await ssh(`sudo -n ${HELPER} ${stage} ${manifest.compatibility} ${NAMES.map(name => manifest.binaries[name].sha256).join(' ')}`);
  } finally {
    try { await ssh(`rm -f -- ${stage}/engine-client ${stage}/spacewars-cli && rmdir -- ${stage}`); }
    catch (error) { console.warn(`Could not clean staging directory ${stage}: ${error.message}`); }
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { await fastUpdate(parseArgs(process.argv.slice(2))); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
