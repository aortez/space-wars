import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { checkRemoteIdentity, fastUpdate, parseArgs as parseOriginalArgs, readBundle } from '../scripts/fast-update.mjs';
import { MACHINE, YOCTO_DIR } from '../scripts/paths.mjs';
import { NETWORK_PROBE } from '../scripts/deployment-network.mjs';

// Keep the existing direct-root transport controls; admin transport has its own test.
const parseArgs = args => parseOriginalArgs(['--user', 'root', ...args]);

const compat = 'a'.repeat(64);
const identity = `spacewars-fast-update-v2 ${compat}`;

test('default owner transport uses sudo for protection and private root-owned staging', async t => {
  assert.equal(parseOriginalArgs([]).user, 'swadmin');
  const f = fixture(t);
  const calls = [];
  const execute = async (program, args, settings) => {
    assert.equal(program, 'ssh');
    assert.ok(args.includes('swadmin@spacewars.local'));
    const command = args.at(-1);
    assert.ok(command.startsWith('sudo -n /bin/sh -c '));
    calls.push({ command, settings });
    if (command.includes('--check')) return identity;
    if (command.includes('network status') || command.includes('network deployment')) {
      return JSON.stringify({ schema: 1, error: null, availability: 'wireless-ready', status: { policy: { policy: { partial_offline: true } } } });
    }
    if (command.includes('mktemp -d')) return '/tmp/spacewars-fast.A123456789';
    return '';
  };
  await fastUpdate(parseOriginalArgs(['--skip-build']), { execute, bundleDir: f.directory });
  const transfers = calls.filter(call => call.settings?.input instanceof Buffer);
  assert.equal(transfers.length, 2);
  for (const call of transfers) assert.match(call.command, /umask 077; cat > \/tmp\/spacewars-fast\.A123456789\/(engine-client|spacewars-cli)'$/);
  assert.match(calls.at(-2).command, /rm -f -- .*rmdir/);
  assert.match(calls.at(-1).command, /network deployment release/);
});

function fixture(t) {
  const directory = mkdtempSync(join(tmpdir(), 'spacewars-fast-test-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const manifest = { format: 1, machine: MACHINE, target_arch: 'aarch64', compatibility: compat, binaries: {} };
  for (const name of ['engine-client', 'spacewars-cli']) {
    const file = Buffer.alloc(64);
    Buffer.from('7f454c46020101', 'hex').copy(file);
    file.writeUInt16LE(183, 18);
    writeFileSync(join(directory, name), file);
    manifest.binaries[name] = { sha256: createHash('sha256').update(file).digest('hex'), size: file.length };
  }
  const save = () => writeFileSync(join(directory, 'manifest.json'), JSON.stringify(manifest));
  save();
  return { directory, manifest, save };
}

test('fast arguments are explicit and reject full-image flags/remote shell syntax', () => {
  assert.deepEqual(parseArgs(['--fast', '--target', 'picade.local', '--skip-build']), {
    host: 'picade.local', user: 'root', skipBuild: true, dryRun: false, prompt: false,
  });
  assert.equal(parseArgs(['--host', '192.168.1.10']).host, '192.168.1.10');
  for (const args of [['--image', 'image.gz'], ['--ssh-key', 'key.pub'], ['--remote-tmp', '/data'], ['--target'], ['--target', '--dry-run'], ['--target', 'host;reboot'], ['--target', '-oProxyCommand=x'], ['--user', 'spacewars'], ['--typo']]) {
    assert.throws(() => parseArgs(args));
  }
});

test('bundle requires matching identities, complete binary pair, architecture and checksums', t => {
  const f = fixture(t);
  assert.deepEqual(readBundle(f.directory), f.manifest);
  const bytes = readFileSync(join(f.directory, 'engine-client'));
  bytes.writeUInt16LE(62, 18);
  writeFileSync(join(f.directory, 'engine-client'), bytes);
  assert.throws(() => readBundle(f.directory), /AArch64/);
  bytes.writeUInt16LE(183, 18);
  bytes[30] = 1;
  writeFileSync(join(f.directory, 'engine-client'), bytes);
  assert.throws(() => readBundle(f.directory), /checksum mismatch/);
  f.manifest.machine = 'raspberrypi5'; f.save();
  assert.throws(() => readBundle(f.directory), /manifest/);
  f.manifest.machine = MACHINE;
  delete f.manifest.binaries['spacewars-cli']; f.save();
  assert.throws(() => readBundle(f.directory), /matching.*pair/);
});

test('missing/mismatched remote helper requires a full update', () => {
  checkRemoteIdentity(identity, compat);
  for (const actual of ['', `spacewars-fast-update-v1 ${compat}`, `spacewars-fast-update-v2 ${'b'.repeat(64)}`]) {
    assert.throws(() => checkRemoteIdentity(actual, compat), /supervised root bootstrap/);
  }
});

function executor(reply = identity, failInstall = false, failNetwork = '') {
  const calls = [];
  return {
    calls,
    async execute(command, args) {
      calls.push([command, args]);
      const remote = args.at(-1);
      if (remote.endsWith('--check')) return reply;
      if (remote === NETWORK_PROBE || remote.startsWith('/usr/bin/engine-os-manager network deployment ')) {
        if (failNetwork && remote.endsWith(failNetwork)) throw new Error('network request failed');
        return JSON.stringify({ schema: 1, error: null, availability: 'wireless-ready', status: { policy: { policy: { partial_offline: true } } } });
      }
      if (remote.startsWith('mktemp')) return '/tmp/spacewars-fast.A123456789';
      if (failInstall && remote.startsWith('/usr/sbin/spacewars-fast-update /tmp/')) throw new Error('install failed');
      return '';
    },
  };
}

test('dry run has no build, network or artifact reads', async () => {
  const never = () => { throw new Error('Unexpected side effect'); };
  await fastUpdate(parseArgs(['--dry-run']), { execute: never, loadBundle: never });
});

test('runtime mismatch aborts before copying or stopping the app', async t => {
  const f = fixture(t);
  const e = executor(`spacewars-fast-update-v2 ${'b'.repeat(64)}`);
  await assert.rejects(fastUpdate(parseArgs(['--skip-build']), { ...e, bundleDir: f.directory }), /supervised root bootstrap/);
  assert.equal(e.calls.length, 1);
});

test('fast update builds only the recipe, transfers a pair, calls fixed helper, cleans staging', async t => {
  const f = fixture(t);
  const e = executor();
  await fastUpdate(parseArgs(['--target', 'picade.local']), { ...e, bundleDir: f.directory });
  assert.equal(e.calls.length, 11);
  assert.deepEqual(e.calls[1][1].slice(-2), ['--target', 'spacewars']);
  const transfer = e.calls.findIndex(([command]) => command === 'scp');
  assert.ok(e.calls[transfer][1].includes(join(f.directory, 'spacewars-cli')));
  assert.match(e.calls[transfer - 1][1].at(-1), /network deployment renew$/);
  assert.match(e.calls[transfer + 1][1].at(-1), /network deployment renew$/);
  assert.match(e.calls[transfer + 2][1].at(-1), /^\/usr\/sbin\/spacewars-fast-update \/tmp\/spacewars-fast\.A123456789 /);
  assert.match(e.calls.at(-2)[1].at(-1), /^rm -f -- .* && rmdir -- /);
  assert.match(e.calls.at(-1)[1].at(-1), /network deployment release$/);
  assert.doesNotMatch(JSON.stringify(e.calls), /reboot|ab-update|rootfs/);
});

test('failed install still cleans only its own staged pair and reports failure', async t => {
  const f = fixture(t);
  const e = executor(identity, true);
  await assert.rejects(fastUpdate(parseArgs(['--skip-build']), { ...e, bundleDir: f.directory }), /install failed/);
  assert.match(e.calls.at(-2)[1].at(-1), /^rm -f -- \/tmp\/spacewars-fast\.A123456789\/engine-client/);
  assert.match(e.calls.at(-1)[1].at(-1), /network deployment release$/);
});

test('failed deployment admission or renewal stops before transfer/install and still releases', async t => {
  const f = fixture(t);
  for (const operation of ['admit', 'renew']) {
    const e = executor(identity, false, operation);
    await assert.rejects(fastUpdate(parseArgs(['--skip-build']), { ...e, bundleDir: f.directory }), /protection/);
    assert.ok(!e.calls.some(([command]) => command === 'scp'));
    assert.ok(!e.calls.some(([, args]) => args.at(-1).startsWith('/usr/sbin/spacewars-fast-update /tmp/')));
    assert.match(e.calls.at(-1)[1].at(-1), /network deployment release$/);
  }
});

test('renewal lost after transfer prevents installation and retains cleanup/release order', async t => {
  const f = fixture(t);
  const e = executor();
  let renewals = 0;
  const execute = async (command, args, options) => {
    if (args.at(-1).endsWith('network deployment renew') && ++renewals === 2) {
      e.calls.push([command, args]);
      throw new Error('renewal unavailable after transfer');
    }
    return e.execute(command, args, options);
  };
  await assert.rejects(fastUpdate(parseArgs(['--skip-build']), { execute, bundleDir: f.directory }), /renew failed/);
  assert.ok(e.calls.some(([command]) => command === 'scp'));
  assert.ok(!e.calls.some(([, args]) => args.at(-1).startsWith('/usr/sbin/spacewars-fast-update /tmp/')));
  assert.match(e.calls.at(-2)[1].at(-1), /^rm -f --/);
  assert.match(e.calls.at(-1)[1].at(-1), /network deployment release$/);
});

test('wrapper routes --fast and legacy yolo refuses it instead of flashing', () => {
  const dry = spawnSync(process.execPath, [join(YOCTO_DIR, 'scripts/update.mjs'), '--fast', '--dry-run'], { encoding: 'utf8' });
  assert.equal(dry.status, 0, dry.stderr);
  assert.match(dry.stdout, /Dry run: no build, SSH/);
  const unsafe = spawnSync(process.execPath, [join(YOCTO_DIR, 'scripts/yolo-update.mjs'), '--fast', '--yes'], { encoding: 'utf8' });
  assert.equal(unsafe.status, 1);
  assert.match(unsafe.stderr, /application-only/);
});
