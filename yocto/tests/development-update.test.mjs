import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { developmentUpdate, parseArgs, parseUnitStatus, readPackage, trialOutcome } from '../scripts/development-update.mjs';

const BUILD = `rootfs-${'c'.repeat(64)}`;
const PREVIOUS = `rootfs-${'a'.repeat(64)}`;
const STAGE = '/tmp/spacewars-development.A123456789';
const BOOTS = ['11111111-1111-4111-8111-111111111111', '22222222-2222-4222-8222-222222222222', '33333333-3333-4333-8333-333333333333'];

function fixture(t) {
  const directory = mkdtempSync(join(tmpdir(), 'spacewars-development-test-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const payload = Buffer.from('fixture development payload');
  writeFileSync(join(directory, 'image.ext4.zst'), payload);
  const image = {
    schema: 1, build_id: BUILD, source_dirty: true,
    compressed: { bytes: payload.length, sha256: createHash('sha256').update(payload).digest('hex') },
    expanded: { bytes: 4096, sha256: 'b'.repeat(64) },
  };
  const save = () => writeFileSync(join(directory, 'development.json'), JSON.stringify(image));
  save();
  return { directory, image, payload, save };
}

function report(stage, phase, good, candidate) {
  return JSON.stringify({ recovery: { report: { progress: { stage, phase, good_build_id: good, candidate_build_id: candidate, detail: null } }, fresh: true, error: null } });
}

// A unit reachable only through the shared connection, as the script promises.
function unit({ enabled = true, normalBoot = true, outcome = 'committed', refuse = false } = {}) {
  const state = { boot: 0, master: false, logins: 0, polls: 0, commands: [] };
  const execute = async (program, args, settings = {}) => {
    if (program !== 'ssh') {
      state.commands.push({ program, args });
      return '';
    }
    if (args.includes('-O')) {
      state.master = false;
      return '';
    }
    if (args.includes('ControlMaster=yes')) {
      state.logins++;
      state.master = true;
      return '';
    }
    assert.ok(args.includes('ProxyCommand=false'), 'a command could log in without the shared connection');
    if (!state.master) throw new Error('ssh exited with code 255');
    const command = args.at(-1);
    assert.ok(command.startsWith("sudo -n /bin/sh -c '"));
    state.commands.push({ command, settings });
    if (command.includes('boot_id')) return BOOTS[state.boot];
    if (command.includes('maintenance development status')) {
      return JSON.stringify({ enabled, normal_boot: normalBoot, good_generation: 21, good_build_id: PREVIOUS, highest_attempted_generation: 21 });
    }
    if (command.includes('network status') || command.includes('network deployment')) {
      return JSON.stringify({ schema: 1, error: null, availability: 'wireless-ready', status: { policy: { policy: { partial_offline: true } } } });
    }
    if (command.includes('systemctl reboot')) {
      state.boot++;
      state.master = false;
      normalBoot = true;
      throw new Error('ssh exited with code 255');
    }
    if (command.includes('mktemp -d')) return STAGE;
    if (command.includes('df -B1')) return String(1024 ** 3);
    if (command.includes('maintenance development install')) {
      if (refuse) throw new Error('ssh exited with code 1');
      state.boot++;
      state.master = false;
      throw new Error('ssh exited with code 255');
    }
    if (command.includes('/usr/bin/engine-os-manager status')) {
      state.polls++;
      if (state.polls === 1) return report('observing', 'observing', PREVIOUS, BUILD);
      if (outcome === 'committed') return report('healthy', 'committed', BUILD, BUILD);
      if (state.polls === 2) {
        state.boot++;
        state.master = false;
        return report('reboot-requested', 'rollback-pending', PREVIOUS, BUILD);
      }
      return report('healthy', 'rolled-back', PREVIOUS, BUILD);
    }
    return '';
  };
  return { state, execute };
}

function run(t, scenario, extra = ['--skip-build']) {
  const f = fixture(t);
  const args = ['--target', 'sw-picade-2.local', '--image', join(f.directory, 'image.ext4.zst'), ...extra];
  const device = unit(scenario);
  const lines = [];
  const packed = [];
  let clock = 0;
  const done = developmentUpdate(parseArgs(args), {
    execute: device.execute,
    pack: async source => { packed.push(source); return { work: null, ...readPackage(f.directory) }; },
    wait: async () => {},
    now: () => (clock += 1000),
    log: line => lines.push(line),
    warn: line => lines.push(line),
  });
  return { f, device, lines, packed, done };
}

test('a committed trial is reported from one login per boot', async t => {
  const { f, device, lines, packed, done } = run(t, {});
  await done;
  assert.equal(packed.length, 1);
  assert.equal(device.state.logins, 3); // Check, install, trial boot.
  const name = `${STAGE}/${f.image.compressed.sha256}.ext4.zst`;
  const transfer = device.state.commands.find(call => call.command?.includes(`cat > ${name}`));
  assert.ok(transfer.command.includes('umask 077'));
  assert.deepEqual(transfer.settings.input, f.payload);
  assert.ok(device.state.commands.some(call => call.command?.includes(`maintenance development install ${STAGE} ${f.image.compressed.sha256}`)));
  assert.ok(!device.state.commands.some(call => call.command?.includes('rm -f')));
  assert.match(device.state.commands.at(-1).command, /network deployment release/);
  assert.match(lines.at(-1), new RegExp(`^Committed: sw-picade-2.local now runs ${BUILD}`));
});

test('a failed trial reports the return to the previous image as an error', async t => {
  const { device, lines, done } = run(t, { outcome: 'returned' });
  await assert.rejects(done, new RegExp(`returned to ${PREVIOUS}`));
  assert.equal(device.state.logins, 4); // The return adds one boot.
  assert.ok(lines.includes('The trial failed; the unit is returning to the previous image.'));
});

test('a unit without the opt-in is refused before any build', async t => {
  const { device, packed, done } = run(t, { enabled: false }, []);
  await assert.rejects(done, /Development installs are off on sw-picade-2.local/);
  assert.equal(packed.length, 0);
  assert.ok(!device.state.commands.some(call => call.program === process.execPath));
});

test('an image the unit refuses leaves no staged files', async t => {
  const { device, done } = run(t, { refuse: true });
  await assert.rejects(done, /code 1/);
  assert.equal(device.state.logins, 2);
  assert.ok(device.state.commands.some(call => call.command?.includes(`rm -f -- ${STAGE}/development.json`)));
});

test('a unit still on its last trial boot restarts normally before the install', async t => {
  const { device, lines, done } = run(t, { normalBoot: false });
  await done;
  assert.ok(lines.some(line => line.includes('Rebooting it normally first')));
  assert.equal(device.state.logins, 4);
  const reboot = device.state.commands.findIndex(call => call.command?.includes('systemctl reboot'));
  const install = device.state.commands.findIndex(call => call.command?.includes('maintenance development install'));
  assert.ok(reboot >= 0 && reboot < install);
});

test('arguments need an explicit target and an operator account', () => {
  assert.deepEqual(parseArgs(['--development', '--target', 'sw-picade-2.local']), {
    host: 'sw-picade-2.local', user: 'swadmin', image: null, skipBuild: false, dryRun: false, prompt: false,
  });
  assert.equal(parseArgs(['--target', 'unit', '--user', 'root']).user, 'root');
  for (const args of [[], ['--target'], ['--target', 'host;reboot'], ['--target', '-oProxyCommand=x'], ['--target', 'unit', '--user', 'spacewars'], ['--target', 'unit', '--fast'], ['--target', 'unit', '--ssh-key', 'key.pub']]) {
    assert.throws(() => parseArgs(args), undefined, args.join(' '));
  }
});

test('packages need a supported descriptor that matches the payload length', t => {
  const f = fixture(t);
  assert.equal(readPackage(f.directory).image.build_id, BUILD);
  for (const change of [image => { image.schema = 2; }, image => { image.build_id = 'two words'; }, image => { image.compressed.sha256 = 'A'.repeat(64); }, image => { image.compressed.bytes += 1; }]) {
    const original = structuredClone(f.image);
    change(f.image);
    f.save();
    assert.throws(() => readPackage(f.directory));
    Object.assign(f.image, original);
    f.save();
  }
});

test('trial outcomes come only from this build in the recovery report', () => {
  assert.deepEqual(trialOutcome(report('healthy', 'committed', BUILD, BUILD), BUILD), { state: 'committed' });
  assert.deepEqual(trialOutcome(report('healthy', 'rolled-back', PREVIOUS, BUILD), BUILD), { state: 'returned', good: PREVIOUS });
  assert.deepEqual(trialOutcome(report('reboot-requested', 'rollback-pending', PREVIOUS, BUILD), BUILD), { state: 'rebooting' });
  assert.deepEqual(trialOutcome(report('observing', 'observing', PREVIOUS, BUILD), BUILD), { state: 'waiting', stage: 'observing' });
  assert.equal(trialOutcome(report('blocked', 'observing', PREVIOUS, BUILD), BUILD).state, 'blocked');
  assert.equal(trialOutcome(report('healthy', 'committed', PREVIOUS, PREVIOUS), BUILD).state, 'unexpected');
  assert.equal(trialOutcome('not json', BUILD).state, 'waiting');
  assert.throws(() => parseUnitStatus('{"enabled":true}'));
});
