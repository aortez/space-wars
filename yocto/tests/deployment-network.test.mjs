import assert from 'node:assert/strict';
import test from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { NETWORK_PROBE, run, withDeploymentHold } from '../scripts/deployment-network.mjs';
import { checkRemoteOtaPrivilege, transferAndFlash } from '../scripts/yolo-update.mjs';

const target = 'root@test.local';
const reply = (availability = 'wireless-ready', partial = true) => JSON.stringify({
  schema: 1, error: null, availability, status: { policy: { policy: { partial_offline: partial } } },
});

test('one capability covers command gaps and reboot, with no token in arguments or results', async () => {
  const operations = [];
  const tokens = new Set();
  const argsSeen = [];
  const execute = async (command, args, options) => {
    assert.equal(command, 'ssh');
    argsSeen.push(args);
    if (args.at(-1) === NETWORK_PROBE) return reply();
    const operation = args.at(-1).split(' ').at(-1);
    operations.push(operation);
    assert.match(options.input, /^[a-f0-9]{64}\n$/);
    tokens.add(options.input.trim());
    assert.equal(options.capture, true);
    assert.equal(options.timeoutMs, 60_000);
    return reply();
  };
  const result = await withDeploymentHold({ target, execute, intervalMs: 10 }, async hold => {
    await hold.ensure();
    await hold.expectReboot();
    const before = operations.length;
    await delay(35);
    assert.ok(operations.length > before, 'renewal continues during a potentially long flash');
    assert.ok(operations.slice(before).every(operation => operation === 'renew'));
    await assert.rejects(hold.ensure(), /waiting for reboot/);
    await hold.reconnected();
    return 'verified';
  });
  assert.equal(result, 'verified');
  assert.equal(tokens.size, 1);
  assert.ok(!JSON.stringify(argsSeen).includes([...tokens][0]));
  assert.equal(operations[0], 'admit');
  assert.equal(operations.at(-1), 'release');
  assert.ok(operations.slice(1, -1).every(operation => operation === 'renew'));
});

test('older images keep their path, then acquire protection if the service appears after reboot', async () => {
  let managed = false;
  const operations = [];
  const execute = async (_, args) => {
    if (args.at(-1) === NETWORK_PROBE) return managed ? reply() : 'spacewars-network-legacy-v1';
    operations.push(args.at(-1).split(' ').at(-1));
    return reply('deployment-protected', false);
  };
  await withDeploymentHold({ target, execute }, async hold => {
    await hold.ensure();
    await hold.expectReboot();
    assert.equal(operations.length, 0);
    managed = true;
    await hold.reconnected();
  });
  assert.deepEqual(operations, ['admit', 'release']);
});

test('unavailable or malformed policy is never treated as a legacy image', async () => {
  for (const response of ['', 'null', '{}', 'not-json', reply('wireless-ready', null)]) {
    let worked = false;
    await assert.rejects(withDeploymentHold({ target, execute: async () => response }, async () => { worked = true; }));
    assert.equal(worked, false);
  }
  await assert.rejects(withDeploymentHold({ target, execute: async () => { throw new Error('SSH failed'); } }, async () => {
    assert.fail('work must not start');
  }), /Cannot determine/);
});

test('lost admission response still releases the same capability without echoing an error containing it', async () => {
  let secret;
  const operations = [];
  const execute = async (_, args, options) => {
    if (args.at(-1) === NETWORK_PROBE) return reply();
    secret ??= options.input;
    assert.equal(options.input, secret);
    const operation = args.at(-1).split(' ').at(-1);
    operations.push(operation);
    if (operation === 'admit') throw new Error(secret);
    return reply();
  };
  await assert.rejects(withDeploymentHold({ target, execute }, async () => assert.fail('must not transfer')), error => {
    assert.ok(!error.message.includes(secret.trim()));
    return /admit failed/.test(error.message);
  });
  assert.deepEqual(operations, ['admit', 'release']);
});

test('heartbeat failure prevents the next step but does not interrupt active work', async () => {
  let failed;
  const failure = new Promise(resolve => { failed = resolve; });
  const warnings = [];
  const operations = [];
  const execute = async (_, args) => {
    if (args.at(-1) === NETWORK_PROBE) return reply();
    const operation = args.at(-1).split(' ').at(-1);
    operations.push(operation);
    if (operation === 'renew') { failed(); throw new Error('disconnected'); }
    return reply();
  };
  await assert.rejects(withDeploymentHold({ target, execute, intervalMs: 5, warn: message => warnings.push(message) }, async hold => {
    await delay(20); // Keep the test alive while the production timer is unref'ed.
    await failure;
    await hold.ensure();
    assert.fail('a new update step must not start');
  }), /renew failed/);
  assert.equal(operations.at(-1), 'release');
  assert.equal(warnings.length, 1);
});

test('expected reboot tolerates unavailable heartbeats but requires acknowledged reconnection', async () => {
  let offline = false;
  let rejected = 0;
  const warnings = [];
  const execute = async (_, args) => {
    if (offline && args.at(-1).endsWith('renew')) { rejected++; throw new Error('rebooting'); }
    return reply();
  };
  await withDeploymentHold({ target, execute, intervalMs: 5, reconnectDelayMs: 1, warn: message => warnings.push(message) }, async hold => {
    await hold.expectReboot();
    offline = true;
    await delay(25);
    assert.ok(rejected > 0);
    assert.equal(warnings.length, 0);
    await assert.rejects(hold.reconnected(), /renew failed/);
    await assert.rejects(hold.ensure(), /waiting for reboot/);
    offline = false;
    await hold.reconnected();
    await hold.ensure();
  });
});

test('reconnection retries a briefly unavailable device service with the same capability', async () => {
  let recover = false;
  let attempts = 0;
  const tokens = new Set();
  const execute = async (_, args, options) => {
    if (args.at(-1) === NETWORK_PROBE) return reply();
    tokens.add(options.input);
    if (recover && args.at(-1).endsWith('renew') && ++attempts < 3) throw new Error('service starting');
    return reply();
  };
  await withDeploymentHold({ target, execute, reconnectDelayMs: 1 }, async hold => {
    await hold.expectReboot();
    recover = true;
    await hold.reconnected();
    assert.equal(attempts, 3);
  });
  assert.equal(tokens.size, 1);
});

test('cleanup failure does not replace the install failure and dry runs do nothing', async () => {
  const installError = new Error('fixture install failed');
  const warnings = [];
  const execute = async (_, args) => {
    if (args.at(-1).endsWith('release')) throw new Error('offline');
    return reply();
  };
  await assert.rejects(withDeploymentHold({ target, execute, warn: message => warnings.push(message) }, async () => { throw installError; }), error => error === installError);
  assert.match(warnings[0], /bounded lease/);
  await withDeploymentHold({ target, dryRun: true, execute: async () => assert.fail('dry run connected') }, async hold => {
    await hold.ensure(); await hold.expectReboot(); await hold.reconnected();
  });
});

test('command runner bounds output/time and supplies secret input without logging remote stderr', async () => {
  const secret = 'fixture-secret-not-in-arguments';
  const length = await run(process.execPath, ['-e', 'let n=0; process.stdin.on("data", b=>n+=b.length); process.stdin.on("end",()=>console.log(n))'], { capture: true, input: secret, timeoutMs: 2000 });
  assert.equal(Number(length), secret.length);
  await assert.rejects(run(process.execPath, ['-e', 'process.stdin.on("data",b=>process.stderr.write(b)); process.stdin.on("end",()=>process.exit(7))'], { capture: true, input: secret, timeoutMs: 2000 }), error => !error.message.includes(secret) && /code 7/.test(error.message));
  await assert.rejects(run(process.execPath, ['-e', 'console.log("x".repeat(1000))'], { capture: true, outputLimit: 32, timeoutMs: 2000 }), /output limit/);
  await assert.rejects(run(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { capture: true, timeoutMs: 50 }), /time limit/);
});

function abFixture(failure = '') {
  const events = [];
  const utils = {
    banner() {}, info() {}, success(message) { events.push(message); },
    async transferImage() { events.push('transfer'); if (failure === 'transfer') throw new Error('transfer failed'); return { remoteImagePath: '/tmp/image', remoteChecksumPath: '/tmp/hash' }; },
    verifyRemoteChecksum() { events.push('checksum'); return failure !== 'checksum'; },
    async run() { events.push('key'); },
    getRemoteBootTime() { events.push('boot-id'); return 'original'; },
    async remoteFlashWithKey(...args) { events.push('flash'); assert.equal(args[5], true); if (failure === 'flash') throw new Error('flash failed'); },
    async waitForReboot() { events.push('wait-reboot'); return failure !== 'reboot'; },
  };
  const protect = async (_, work) => {
    events.push('admit');
    try {
      return await work({ ensure: async () => events.push('renew'), expectReboot: async () => events.push('protect-reboot'), reconnected: async () => events.push('renew-after-reboot') });
    } finally { events.push('release'); }
  };
  const options = { utils, image: { path: '/fixture/image' }, checksum: 'fixture', remoteTarget: target, remoteTmp: '/tmp', sshKeyPath: '/fixture/public-key', remoteUser: 'root', targetHost: 'test.local', dryRun: false, skipConfirm: true };
  return { events, options, protect };
}

test('A/B transport retains protection through transfer, key, reboot and reconnect validation', async () => {
  const f = abFixture();
  await transferAndFlash(f.options, f);
  assert.deepEqual(f.events, ['admit', 'renew', 'transfer', 'checksum', 'renew', 'key', 'SSH key transferred', 'boot-id', 'protect-reboot', 'flash', 'wait-reboot', 'renew-after-reboot', 'OTA update complete.', 'release']);
  for (const failure of ['transfer', 'checksum', 'flash', 'reboot']) {
    const f = abFixture(failure);
    await assert.rejects(transferAndFlash(f.options, f));
    assert.equal(f.events.at(-1), 'release');
    assert.ok(!f.events.includes('OTA update complete.'));
  }
  assert.throws(() => checkRemoteOtaPrivilege({ ssh: () => 'managed' }, target), /unsigned A\/B installation is disabled/);
  assert.equal(checkRemoteOtaPrivilege({ ssh: () => 'legacy' }, target), true);
});

test('A/B confirmation happens before the final protection check and cancellation releases the hold', async () => {
  for (const answer of ['no', 'yolo']) {
    const f = abFixture();
    f.options.skipConfirm = false;
    f.options.utils.prompt = async () => { f.events.push('prompt'); return answer; };
    if (answer === 'no') {
      await assert.rejects(transferAndFlash(f.options, f), /cancelled/);
      assert.ok(!f.events.includes('flash'));
    } else {
      await transferAndFlash(f.options, f);
      assert.ok(f.events.indexOf('prompt') < f.events.indexOf('protect-reboot'));
    }
    assert.equal(f.events.at(-1), 'release');
  }
});
