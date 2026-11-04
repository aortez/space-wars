#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createInterface } from 'node:readline/promises';
import { defaultImageDir, MACHINE, YOCTO_DIR } from './paths.mjs';
import { run, SSH_OPTIONS, withDeploymentHold } from './deployment-network.mjs';
export { run } from './deployment-network.mjs';

const NAMES = ['engine-client', 'spacewars-cli'];
const HELPER = '/usr/sbin/spacewars-fast-update';
const HASH = /^[a-f0-9]{64}$/;

export function parseArgs(args) {
  const options = { host: 'spacewars.local', user: 'swadmin', skipBuild: false, dryRun: false, prompt: false };
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
  if (!['root', 'swadmin'].includes(options.user)) throw new Error('Fast updates require swadmin with passwordless sudo, or separately provisioned root access. The kiosk account cannot install unsigned binaries.');
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
  if (reply.trim() !== `spacewars-fast-update-v2 ${expected}`) {
    throw new Error('Fast-update helper/runtime does not match this build. Use supervised root bootstrap to install the matching image and maintenance guard.');
  }
}

export async function fastUpdate(options, { execute = run, bundleDir = join(defaultImageDir(), 'spacewars-fast'), loadBundle = readBundle } = {}) {
  const target = `${options.user}@${options.host}`;
  const privileged = command => options.user === 'root' ? command
    : `sudo -n /bin/sh -c '${command.replaceAll("'", "'\\''")}'`;
  const operatorExecute = (program, args, settings) => execute(program,
    program === 'ssh' ? [...args.slice(0, -1), privileged(args.at(-1))] : args, settings);
  console.log(`Fast update: ${target} (client + CLI, application restart, no reboot)`);
  if (options.dryRun) {
    console.log(`${options.skipBuild ? 'Use existing' : 'Build --target spacewars, then use'} bundle: ${bundleDir}`);
    console.log('Check root operator helper/runtime compatibility; acquire network protection; verify hashes; copy pair; restart and health-check; rollback on failure; release protection after cleanup.');
    console.log('Dry run: no build, SSH connection, files transferred, or remote changes.');
    return;
  }

  const ssh = (command, capture = false) => operatorExecute('ssh', [...SSH_OPTIONS, target, command], { capture });
  // Fail before an expensive build if this image has not been bootstrapped.
  let identity;
  try { identity = await ssh(`${HELPER} --check`, true); }
  catch (error) { throw new Error(`Cannot use fast updates on ${target}: ${error.message}. Provision owner access and the matching maintenance guard through a current image or supervised bootstrap.`); }
  if (!/^spacewars-fast-update-v2 [a-f0-9]{64}$/.test(identity)) {
    throw new Error('Missing/unsupported operator helper; use supervised root bootstrap first.');
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

  await withDeploymentHold({ target, execute: operatorExecute }, async hold => {
    const stage = await ssh('mktemp -d /tmp/spacewars-fast.XXXXXXXXXX', true);
    if (!/^\/tmp\/spacewars-fast\.[a-zA-Z0-9]{10}$/.test(stage)) throw new Error('Unexpected remote staging directory.');
    try {
      // Only a validated fixed staging path reaches the remote shell. The
      // installer checks hashes again and retains its lock through health/rollback.
      await hold.ensure();
      if (options.user === 'root') {
        await execute('scp', [...SSH_OPTIONS, ...NAMES.map(name => join(bundleDir, name)), `${target}:${stage}/`]);
      } else {
        // The existing installer requires a root-owned private stage. Stream
        // into it through the authenticated admin's sudo, without granting the
        // runtime account access or weakening the installer's ownership checks.
        for (const name of NAMES) {
          await hold.ensure();
          await operatorExecute('ssh', [...SSH_OPTIONS, target, `umask 077; cat > ${stage}/${name}`],
            { input: readFileSync(join(bundleDir, name)), capture: true });
        }
      }
      await hold.ensure();
      await ssh(`${HELPER} ${stage} ${manifest.compatibility} ${NAMES.map(name => manifest.binaries[name].sha256).join(' ')}`);
    } finally {
      try { await ssh(`rm -f -- ${stage}/engine-client ${stage}/spacewars-cli && rmdir -- ${stage}`); }
      catch (error) { console.warn(`Could not clean staging directory ${stage}: ${error.message}`); }
    }
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { await fastUpdate(parseArgs(process.argv.slice(2))); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
