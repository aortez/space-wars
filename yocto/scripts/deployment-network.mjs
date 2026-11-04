import { randomBytes } from 'node:crypto';
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

export const SSH_OPTIONS = ['-o', 'BatchMode=yes', '-o', 'ConnectTimeout=5', '-o', 'ServerAliveInterval=10', '-o', 'ServerAliveCountMax=3'];
export const NETWORK_PROBE = 'if test -x /usr/bin/engine-os-manager; then exec /usr/bin/engine-os-manager network status; else printf "%s\\n" spacewars-network-legacy-v1; fi';
const PREFIX = '/usr/bin/engine-os-manager network deployment ';

// Capture suppresses remote stderr too: unexpected peers must not put a secret
// stdin capability into workstation diagnostics. Errors contain no input/output.
export function run(command, args, { capture = false, input, timeoutMs = 0, outputLimit = 64 * 1024, ...options } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      ...options,
      stdio: [input === undefined ? (capture ? 'ignore' : 'inherit') : 'pipe', capture ? 'pipe' : 'inherit', capture ? 'pipe' : 'inherit'],
    });
    let output = '';
    let bytes = 0;
    let failure;
    let killTimer;
    const stop = message => {
      if (failure) return;
      failure = new Error(message);
      child.kill('SIGTERM');
      killTimer = setTimeout(() => child.kill('SIGKILL'), 1000);
      killTimer.unref();
    };
    const timer = timeoutMs ? setTimeout(() => stop(`${command} exceeded its time limit`), timeoutMs) : null;
    const collect = (chunk, stdout) => {
      bytes += chunk.length;
      if (bytes > outputLimit) stop(`${command} exceeded its output limit`);
      else if (stdout) output += chunk;
    };
    if (capture) {
      child.stdout.on('data', chunk => collect(chunk, true));
      child.stderr.on('data', chunk => collect(chunk, false));
    }
    if (input !== undefined) {
      child.stdin.on('error', () => {}); // The exit status handles early EOF/EPIPE.
      child.stdin.end(input);
    }
    child.on('error', () => { failure ??= new Error(`Could not start ${command}`); });
    child.on('close', code => {
      clearTimeout(timer);
      clearTimeout(killTimer);
      if (failure) reject(failure);
      else if (code !== 0) reject(new Error(`${command} exited with code ${code}`));
      else resolve(output.trim());
    });
  });
}

function accepted(text, operation) {
  let reply;
  try { reply = JSON.parse(text); } catch { throw new Error('Invalid deployment protection response.'); }
  const partial = reply?.status?.policy?.policy?.partial_offline;
  if (reply?.schema !== 1 || reply.error !== null || typeof partial !== 'boolean'
      || (operation !== 'status' && operation !== 'release'
          && reply.availability !== 'wireless-ready'
          && !(reply.availability === 'deployment-protected' && partial === false))) {
    throw new Error('Deployment network protection was not acknowledged. No new update step may start.');
  }
}

/** One persisted, account-owned capability spans the entire workstation task.
 * A failed heartbeat never kills an installer mid-write. Each subsequent step
 * must obtain a fresh acknowledgement; SSH/install observations protect work
 * already in progress. Reboot gaps use the server's bounded reconnect grace.
 */
export async function withDeploymentHold({ target, dryRun = false, execute = run, intervalMs = 60_000, reconnectDelayMs = 5000, warn = console.warn }, work) {
  if (!/^[a-z_][a-z0-9_-]*@[a-zA-Z0-9][a-zA-Z0-9._-]*$/.test(target)) throw new Error('Invalid deployment SSH target.');
  if (dryRun) return work({ ensure: async () => {}, expectReboot: async () => {}, reconnected: async () => {} });
  const token = randomBytes(32).toString('hex');
  let managed = false;
  let paused = false;
  let reconnecting = false;
  let warned = false;
  let serial = Promise.resolve();
  let timer;
  const ssh = (command, input) => execute('ssh', [...SSH_OPTIONS, target, command], { capture: true, input, timeoutMs: 60_000, outputLimit: 64 * 1024 });
  const queued = action => {
    const pending = serial.catch(() => {}).then(action);
    serial = pending;
    return pending;
  };
  const operation = async command => {
    let reply;
    try { reply = await ssh(PREFIX + command, token + '\n'); }
    catch { throw new Error(`Deployment protection ${command} failed. Check the device service and connection.`); }
    accepted(reply, command);
  };
  const ensure = () => queued(async () => {
    if (!managed) {
      let reply;
      try { reply = await ssh(NETWORK_PROBE); }
      catch { throw new Error('Cannot determine deployment network policy. No new update step may start.'); }
      if (reply.trim() === 'spacewars-network-legacy-v1') return;
      accepted(reply, 'status');
      managed = true; // Even a lost Admit response can leave a reservation.
      await operation('admit');
    } else {
      await operation('renew');
    }
    warned = false;
  });
  const lease = {
    async ensure() {
      if (paused) throw new Error('Deployment is waiting for reboot/reconnection.');
      await ensure();
    },
    async expectReboot() {
      await lease.ensure();
      paused = true;
    },
    async reconnected() {
      reconnecting = true;
      try {
        for (let attempt = 0; ; attempt++) {
          try {
            await ensure();
            paused = false;
            return;
          } catch (error) {
            if (attempt === 2) throw error;
            await delay(reconnectDelayMs);
          }
        }
      } finally { reconnecting = false; }
    },
  };
  try {
    await ensure();
    let heartbeat = false;
    timer = setInterval(() => {
      if (!managed || heartbeat || reconnecting) return;
      heartbeat = true;
      ensure().catch(() => {
        if (!paused && !warned) warn('Deployment protection renewal failed; the next update step requires a fresh acknowledgement.');
        if (!paused) warned = true;
      }).finally(() => { heartbeat = false; });
    }, intervalMs);
    timer.unref();
    return await work(lease);
  } finally {
    clearInterval(timer);
    await serial.catch(() => {});
    if (managed) {
      try { await operation('release'); }
      catch { warn('Could not release deployment network protection. Its bounded lease remains until recovery checks permit expiry.'); }
    }
  }
}
