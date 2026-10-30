#!/usr/bin/env node

// Opt-in development installs; see docs/ota-development-installs.md. The unit
// checks the image, writes its inactive slot and boots it as an OTA trial.
import { createReadStream, createWriteStream, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createInterface } from 'node:readline/promises';
import { pipeline } from 'node:stream/promises';
import { setTimeout as sleep } from 'node:timers/promises';
import { createGunzip } from 'node:zlib';
import { defaultImageDir, preferredImages, REPO_DIR, YOCTO_DIR } from './paths.mjs';
import { run, SSH_OPTIONS, withDeploymentHold } from './deployment-network.mjs';

const BINARY = '/usr/bin/engine-os-manager';
const HASH = /^[a-f0-9]{64}$/;
const IDENTIFIER = /^[a-zA-Z0-9._-]{1,128}$/;
const BOOT_ID = /^[a-f0-9-]{36}$/;
const STAGE = /^\/tmp\/spacewars-development\.[a-zA-Z0-9]{10}$/;
// Free space left beside the payload, as the unit's own update staging keeps.
const HEADROOM = 256 * 1024 * 1024;
const VALIDATOR = join(REPO_DIR, 'target/release/examples/image_candidate');

export function parseArgs(args) {
  const options = { host: null, user: 'swadmin', image: null, skipBuild: false, dryRun: false, prompt: false };
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (['--target', '--host', '--user', '--image'].includes(arg)) {
      const value = args[++i];
      if (!value || value.startsWith('-')) throw new Error(`Missing value for ${arg}`);
      if (arg === '--user') options.user = value;
      else if (arg === '--image') options.image = value;
      else options.host = value;
    } else if (arg === '--skip-build') options.skipBuild = true;
    else if (arg === '--dry-run') options.dryRun = true;
    else if (arg === '--prompt') options.prompt = true;
    else if (!['--development', '--yes', '--hold-my-mead'].includes(arg)) {
      throw new Error(`Unsupported development-install option: ${arg}.`);
    }
  }
  if (!options.host) throw new Error('Development installs need an explicit --target. Only units that enabled them accept one.');
  if (!/^[a-zA-Z0-9][a-zA-Z0-9._-]*$/.test(options.host)) throw new Error('Invalid target hostname or IPv4 address.');
  if (!['root', 'swadmin'].includes(options.user)) {
    throw new Error('Development installs require swadmin with passwordless sudo, or separately provisioned root access.');
  }
  return options;
}

export function findImage(path, directory = defaultImageDir()) {
  const candidates = path ? [path] : preferredImages('ext4.gz').map(name => join(directory, name));
  const found = candidates.find(candidate => existsSync(candidate));
  if (!found) throw new Error(path ? `Image not found: ${path}` : `No rootfs image in ${directory}. Build one, or pass --image <path>.`);
  return found;
}

export function readPackage(directory) {
  const descriptor = join(directory, 'development.json');
  let image;
  try { image = JSON.parse(readFileSync(descriptor, 'utf8')); }
  catch (error) { throw new Error(`Missing/invalid development descriptor: ${error.message}`); }
  const blob = value => Number.isSafeInteger(value?.bytes) && value.bytes > 0 && HASH.test(value?.sha256 ?? '');
  if (image?.schema !== 1 || !blob(image.compressed) || !blob(image.expanded) || !IDENTIFIER.test(image.build_id ?? '')) {
    throw new Error('Unsupported development descriptor.');
  }
  const payload = join(directory, 'image.ext4.zst');
  if (statSync(payload).size !== image.compressed.bytes) throw new Error('Development payload length differs from its descriptor.');
  return { image, descriptor, payload };
}

/** Packs the build's rootfs with the same structural checks the unit relies on. */
export async function packImage(source, { execute = run } = {}) {
  await execute('cargo', ['build', '--locked', '--release', '-p', 'engine-ota', '--features', 'publisher', '--example', 'image_candidate'], { cwd: REPO_DIR });
  mkdirSync(join(REPO_DIR, 'target'), { recursive: true });
  const work = mkdtempSync(join(REPO_DIR, 'target', 'development-image-'));
  try {
    const rootfs = join(work, 'rootfs.ext4');
    await pipeline(createReadStream(source), createGunzip(), createWriteStream(rootfs, { mode: 0o600 }));
    await execute(VALIDATOR, ['pack-development', rootfs, join(work, 'package')], { cwd: REPO_DIR });
    rmSync(rootfs, { force: true });
    return { work, ...readPackage(join(work, 'package')) };
  } catch (error) {
    rmSync(work, { recursive: true, force: true });
    throw error;
  }
}

export function parseUnitStatus(text) {
  let status = null;
  try { status = JSON.parse(text); } catch {}
  if (typeof status?.enabled !== 'boolean' || typeof status.normal_boot !== 'boolean'
      || !Number.isSafeInteger(status.good_generation) || typeof status.good_build_id !== 'string') {
    throw new Error('Invalid development status from the unit.');
  }
  return status;
}

/** Reads `engine-os-manager status`. The build ID identifies this install's transaction. */
export function trialOutcome(text, build) {
  let status = null;
  try { status = JSON.parse(text); } catch {}
  const progress = status?.recovery?.report?.progress;
  if (!progress) return { state: 'waiting', stage: null };
  if (progress.stage === 'reboot-requested') return { state: 'rebooting' };
  if (progress.stage === 'blocked') return { state: 'blocked', detail: progress.detail ?? null };
  if (progress.stage !== 'healthy') return { state: 'waiting', stage: progress.stage };
  if (progress.phase === 'committed' && progress.good_build_id === build) return { state: 'committed' };
  if (progress.phase === 'rolled-back' && progress.candidate_build_id === build) {
    return { state: 'returned', good: progress.good_build_id };
  }
  return { state: 'unexpected', phase: progress.phase ?? null, good: progress.good_build_id ?? null };
}

function running(status) {
  return `${status.good_build_id} (generation ${status.good_generation}${status.good_generation === 0 ? ', unpublished' : ''})`;
}

export async function developmentUpdate(options, {
  execute = run, pack = packImage, protect = withDeploymentHold, wait = sleep, now = Date.now, log = console.log, warn = console.warn,
} = {}) {
  const target = `${options.user}@${options.host}`;
  log(`Development install: ${target} (full image, trial boot, automatic return on failure)`);
  if (options.dryRun) {
    log(`${options.skipBuild ? 'Use the existing' : 'Build, then use the'} rootfs image: ${options.image ?? defaultImageDir()}`);
    log('Check the unit allows development installs; pack and check the image; acquire network protection; stage it privately; write the inactive slot; reboot into the trial; report whether it committed or returned.');
    log('Dry run: no build, SSH connection, files transferred, or remote changes.');
    return;
  }

  const control = mkdtempSync(join(tmpdir(), 'spacewars-ssh-'));
  const socket = ['-o', `ControlPath=${join(control, 'master')}`];
  const privileged = command => options.user === 'root' ? command
    : `sudo -n /bin/sh -c '${command.replaceAll("'", "'\\''")}'`;
  // One login per boot carries every command, including the network hold's.
  // Each fresh login takes a network admission that a slow USB stick can stall,
  // so without the shared connection a command fails instead of logging in.
  const shared = [...socket, '-o', 'ControlMaster=no', '-o', 'ProxyCommand=false'];
  const operatorExecute = (program, args, settings) => execute(program,
    program === 'ssh' ? [...shared, ...args.slice(0, -1), privileged(args.at(-1))] : args, settings);
  const ssh = (command, settings = {}) => operatorExecute('ssh', [...SSH_OPTIONS, target, command], { capture: true, timeoutMs: 60_000, ...settings });
  const close = () => execute('ssh', [...socket, '-O', 'exit', target], { capture: true, timeoutMs: 10_000 }).catch(() => {});
  const open = async () => {
    await close();
    await execute('ssh', [...SSH_OPTIONS, ...socket, '-o', 'ControlMaster=yes', '-o', 'ControlPersist=yes', '-f', '-N', target], { capture: true, timeoutMs: 30_000 });
  };
  const bootId = async () => {
    const boot = await ssh('cat /proc/sys/kernel/random/boot_id');
    if (!BOOT_ID.test(boot)) throw new Error('Unexpected boot identity from the unit.');
    return boot;
  };
  const unitStatus = async () => parseUnitStatus(await ssh(`${BINARY} maintenance development status`));
  // Waits for any boot other than `previous` (null accepts the current one).
  // While the old boot is still up, it polls over one connection to it.
  const reconnect = async (previous, minutes) => {
    const deadline = now() + minutes * 60_000;
    let connected = false;
    while (now() < deadline) {
      try {
        if (!connected) {
          await open();
          connected = true;
        }
        const boot = await bootId();
        if (boot !== previous) return boot;
      } catch {
        connected = false;
        await close();
      }
      await wait(connected ? 5_000 : 10_000);
    }
    throw new Error(`${options.host} did not come back within ${minutes} minutes.`);
  };

  let work = null;
  try {
    try { await open(); }
    catch (error) { throw new Error(`Cannot connect to ${target}: ${error.message}`); }
    let status;
    try { status = await unitStatus(); }
    catch (error) {
      throw new Error(`${options.host} can't take development installs (${error.message}). It needs an installed generation that has them.`);
    }
    if (!status.enabled) {
      throw new Error(`Development installs are off on ${options.host}. To allow them, run on the unit: sudo ${BINARY} maintenance development enable`);
    }
    log(`${options.host} runs ${running(status)}.`);
    // A build can take an hour; don't hold a login through it.
    await close();
    if (!options.skipBuild) {
      await execute(process.execPath, [join(YOCTO_DIR, 'scripts/build.mjs')], { cwd: YOCTO_DIR });
    }
    const packaged = await pack(findImage(options.image), { execute });
    work = packaged.work;
    const { image, descriptor, payload } = packaged;
    log(`Image ${image.build_id}: ${image.compressed.bytes} bytes compressed, ${image.expanded.bytes} expanded${image.source_dirty ? ', from a dirty source' : ''}.`);
    if (options.prompt) {
      const prompt = createInterface({ input: process.stdin, output: process.stdout });
      try {
        if ((await prompt.question(`Install it on ${target} and reboot into a trial? Type "yes": `)).trim() !== 'yes') {
          throw new Error('Update cancelled.');
        }
      } finally { prompt.close(); }
    }

    await open();
    let boot = await bootId();
    await protect({ target, execute: operatorExecute }, async hold => {
      status = await unitStatus();
      if (!status.enabled) throw new Error(`Development installs were turned off on ${options.host}.`);
      if (!status.normal_boot) {
        log(`${options.host} is still on the trial boot of its last install. Rebooting it normally first, as official installs require.`);
        await hold.expectReboot();
        await ssh('systemctl reboot').catch(() => {});
        await close();
        boot = await reconnect(boot, 10);
        await hold.reconnected();
        status = await unitStatus();
        if (!status.normal_boot) throw new Error(`${options.host} came back on a trial boot.`);
      }
      await hold.ensure();
      const stage = await ssh('mktemp -d /tmp/spacewars-development.XXXXXXXXXX');
      if (!STAGE.test(stage)) throw new Error('Unexpected remote staging directory.');
      const name = `${image.compressed.sha256}.ext4.zst`;
      let started = false;
      try {
        const available = Number(await ssh(`df -B1 --output=avail ${stage} | tail -n 1`));
        if (!Number.isSafeInteger(available) || available < image.compressed.bytes + HEADROOM) {
          throw new Error(`Not enough space in /tmp on ${options.host}: need ${image.compressed.bytes + HEADROOM} bytes, have ${available}.`);
        }
        // Only a validated fixed staging path and hash reach the remote shell.
        // The unit checks the bytes again under its install lock.
        await hold.ensure();
        await ssh(`umask 077; cat > ${stage}/development.json`, { input: readFileSync(descriptor) });
        await hold.ensure();
        log(`Copying ${image.compressed.bytes} bytes to ${options.host}...`);
        await ssh(`umask 077; cat > ${stage}/${name}`, { input: readFileSync(payload), timeoutMs: 0 });
        await hold.ensure();
        await hold.expectReboot();
        started = true;
        log('Writing the inactive slot. The unit then reboots into the trial.');
        try {
          await operatorExecute('ssh', [...SSH_OPTIONS, target, `${BINARY} maintenance development install ${stage} ${image.compressed.sha256}`]);
        } catch (error) {
          // 255 is ssh's own failure: the reboot, or a dropped connection the
          // install outlives. Anything else is the unit refusing the image.
          if (!/code 255$/.test(error.message)) {
            started = false;
            throw error;
          }
          log('The connection closed; waiting for the unit.');
        }
      } finally {
        // A trial reboot clears /tmp; until one is requested, clean up here.
        if (!started) {
          await ssh(`rm -f -- ${stage}/development.json ${stage}/${name} ${stage}/lock && rmdir -- ${stage}`)
            .catch(error => warn(`Could not clean staging directory ${stage}: ${error.message}`));
        }
      }

      await close();
      log('Waiting for the trial boot...');
      boot = await reconnect(boot, 20);
      await hold.reconnected().catch(() => warn('Could not renew network protection after the reboot.'));
      const deadline = now() + 15 * 60_000;
      let shown = null;
      while (now() < deadline) {
        let result;
        try { result = trialOutcome(await ssh(`${BINARY} status`), image.build_id); }
        catch { result = { state: 'disconnected' }; }
        if (result.state === 'committed') {
          log(`Committed: ${options.host} now runs ${image.build_id} (generation 0, unpublished). The next official generation installs over it.`);
          return;
        }
        if (result.state === 'returned') {
          throw new Error(`The trial did not pass, and ${options.host} returned to ${result.good}. Its journal from the trial boot shows why.`);
        }
        if (result.state === 'blocked') throw new Error(`Recovery is blocked on ${options.host}: ${result.detail ?? 'no detail'}.`);
        if (result.state === 'unexpected') {
          throw new Error(`${options.host} reports phase ${result.phase} with good image ${result.good}, which is not this install's outcome.`);
        }
        if (result.state === 'rebooting' || result.state === 'disconnected') {
          if (result.state === 'rebooting') log('The trial failed; the unit is returning to the previous image.');
          await close();
          boot = await reconnect(result.state === 'rebooting' ? boot : null, 10);
          continue;
        }
        if (result.stage !== shown) {
          shown = result.stage;
          if (shown) log(`Recovery: ${shown}`);
        }
        await wait(5_000);
      }
      throw new Error(`No trial outcome from ${options.host} within 15 minutes. Check: sudo ${BINARY} status`);
    });
  } finally {
    await close();
    rmSync(control, { recursive: true, force: true });
    if (work) rmSync(work, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { await developmentUpdate(parseArgs(process.argv.slice(2))); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
