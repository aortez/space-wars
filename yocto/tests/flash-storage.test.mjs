import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { chmodSync, chownSync, linkSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readlinkSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { assertCleanFilesystem, backupData, copyTree, legacyRootArgument, preserveLegacyHostKeys, provisionAccess, readPublicKey, restoreData } from '../scripts/flash-storage.mjs';

function temporary(t) {
  const directory = mkdtempSync(join(tmpdir(), 'spacewars-flash-test-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  return directory;
}

function model({ store = false, admin = false, sudoFailure = false, mismatch = false, mounted = false, recovery = false } = {}) {
  const calls = [];
  const mounts = new Map();
  const run = (program, args, options = {}) => {
    calls.push({ program, args, options });
    if (program === 'lsblk') {
      const number = Number(args.at(-1).at(-1));
      return JSON.stringify({ blockdevices: [{ type: 'part', fstype: number === 1 ? 'vfat' : 'ext4', label: number === 4 ? 'data' : 'rootfs_a', mountpoints: [mounted ? '/in-use' : null] }] });
    }
    assert.equal(program, 'sudo');
    if (sudoFailure) throw Object.assign(new Error('sudo denied'), { status: 1 });
    const [command, ...rest] = args;
    if (command === 'dumpe2fs') return 'Filesystem features: has_journal extent\nFilesystem state: clean';
    if (command === 'mount') mounts.set(rest.at(-1), Number(rest.at(-2).at(-1)));
    if (command === 'umount') mounts.delete(rest[0]);
    if (command === 'rsync') return rest.includes('--dry-run') && mismatch ? '>fcs....... damaged\n' : '';
    if (command === '/bin/sh') {
      const path = rest.at(-1);
      const present = path.endsWith('/spacewars-access') ? store
        : path.endsWith('/spacewars-access/swadmin') ? admin
          : path.endsWith('/spacewars-system/recovery') ? recovery : false;
      return present ? 'present' : 'absent';
    }
    if (command === 'cat') {
      if (rest[0].endsWith('/etc/passwd')) return 'spacewars:x:1000:1000::/home/spacewars:/bin/bash\nswadmin:x:1001:1001::/home/swadmin:/bin/bash';
      if (rest[0].endsWith('/spacewars-admin')) return 'swadmin ALL=(root) NOPASSWD: ALL';
    }
    if (command === 'stat') {
      const path = rest.at(-1);
      if (path.endsWith('/spacewars-access')) return '0:0:755:directory';
      return `0:0:${path.endsWith('/spacewars-admin') ? '440' : path.endsWith('/root') ? '600' : '644'}:regular file:1`;
    }
    return '';
  };
  return { calls, mounts, run };
}

test('backup and restore preserve real numeric owners and verify before success', t => {
  const m = model();
  const backup = backupData('/dev/sdz', { ...m, base: temporary(t) });
  restoreData('/dev/sdz', backup, m);
  const copies = m.calls.filter(call => call.args[0] === 'rsync');
  assert.equal(copies.length, 4);
  for (const call of copies) {
    assert.ok(call.args.includes('-aHAX'));
    assert.ok(call.args.includes('--numeric-ids'));
    assert.ok(!call.args.includes('--fake-super'));
  }
  assert.ok(copies[1].args.includes('--checksum'));
  assert.ok(copies[3].args.includes('--checksum'));
  assert.equal(m.mounts.size, 0);
  assert.equal(JSON.parse(readFileSync(join(backup.directory, 'backup.json'))).device, '/dev/sdz');
});

test('damaged copies, mounted disks, recovery journals and denied sudo stop backup', t => {
  for (const fault of ['mismatch', 'mounted', 'recovery', 'sudoFailure']) {
    const m = model({ [fault]: true });
    assert.throws(() => backupData('/dev/sdz', { ...m, base: temporary(t) }), /Backup retained/);
    assert.ok(!m.calls.some(call => call.args.includes('rw,nosuid,nodev,noexec')));
    if (!m.mounts.size) continue;
    assert.fail('backup left a mount attached');
  }
  assert.throws(() => backupData('/dev/sdz4', { ...model(), base: temporary(t) }), /whole-disk/);
});

test('new image receives explicit runtime and admin keys without writing a root key', () => {
  const m = model();
  const runtime = Buffer.from('runtime public bytes');
  const admin = Buffer.from('owner public bytes');
  assert.equal(provisionAccess('/dev/sdz', runtime, admin, m), 'provisioned-on-first-boot');
  const writes = m.calls.filter(call => call.options.input);
  assert.equal(writes.length, 2);
  assert.match(writes[0].args.at(-1), /home\/spacewars\/\.ssh\/authorized_keys$/);
  assert.equal(writes[0].options.input, runtime);
  assert.match(writes[1].args.at(-1), /home\/swadmin\/\.ssh\/authorized_keys$/);
  assert.equal(writes[1].options.input, admin);
  assert.ok(writes[1].args.includes('1001'));
});

test('restoring an old store provisions only the new owner role and preserves existing roles', () => {
  const m = model({ store: true });
  assert.equal(provisionAccess('/dev/sdz', Buffer.from('runtime'), Buffer.from('owner'), m), 'provisioned');
  const writes = m.calls.filter(call => call.options.input);
  assert.equal(writes.length, 3);
  assert.match(writes[2].args.at(-1), /spacewars-access\/swadmin$/);
  assert.ok(writes[2].args.includes('644'));
  assert.ok(writes[2].args.includes('0'));
});

test('existing administrator keys and deliberate revocations survive unless explicitly replaced', () => {
  for (const replaceAdmin of [false, true]) {
    const m = model({ store: true, admin: true });
    const result = provisionAccess('/dev/sdz', Buffer.from('runtime'), Buffer.from('owner'), { ...m, replaceAdmin });
    assert.equal(result, replaceAdmin ? 'replaced' : 'preserved');
    assert.equal(m.calls.filter(call => call.options.input).length, replaceAdmin ? 3 : 2);
  }
});

test('unsafe admin key targets and unavailable sudo never become a first-boot fallback', () => {
  const m = model({ store: true, admin: true });
  const run = (program, args, options) => args[0] === 'stat' && args.at(-1).endsWith('/swadmin')
    ? '0:0:777:symbolic link:1' : m.run(program, args, options);
  assert.throws(() => provisionAccess('/dev/sdz', Buffer.from('a'), Buffer.from('b'), { run }), /Unsafe persistent/);
  assert.throws(() => provisionAccess('/dev/sdz', Buffer.from('a'), Buffer.from('b'), model({ sudoFailure: true })), /sudo denied/);
});

test('preserved host keys avoid reading an unrelated root slot', () => {
  const backup = { hasHostKeys: true };
  assert.equal(preserveLegacyHostKeys('/dev/sdz', backup, { run: () => assert.fail('unexpected read') }), backup);
});

test('legacy host identity follows the boot root through hardware includes and rejects ambiguity', () => {
  const files = { 'config.txt': 'include hardware.txt\n', 'hardware.txt': 'dtoverlay=vc4-kms-v3d\n', 'cmdline.txt': 'console=tty1 root=PARTUUID=12345678-03 rootwait' };
  const read = name => { if (!(name in files)) throw new Error('missing file'); return files[name]; };
  assert.equal(legacyRootArgument(read), 'PARTUUID=12345678-03');
  files['hardware.txt'] += 'cmdline=custom.txt\n';
  files['custom.txt'] = 'root=LABEL=rootfs_b';
  assert.equal(legacyRootArgument(read), 'LABEL=rootfs_b');
  files['config.txt'] += 'cmdline=cmdline.txt\n';
  assert.throws(() => legacyRootArgument(read), /Ambiguous/);
  files['config.txt'] = '[pi4]\ncmdline=custom.txt\n';
  assert.throws(() => legacyRootArgument(read), /Conditional/);
  files['config.txt'] = 'include ../outside';
  assert.throws(() => legacyRootArgument(read), /Unsafe/);
  files['config.txt'] = 'include config.txt';
  assert.throws(() => legacyRootArgument(read), /Repeated/);
});

test('real rsync round trip retains content, modes, hardlinks, symlinks and numeric owners', t => {
  const base = temporary(t);
  const source = join(base, 'original');
  const backup = join(base, 'backup');
  const restored = join(base, 'restored');
  for (const path of [source, backup, restored]) mkdirSync(path, { mode: 0o700 });
  writeFileSync(join(source, 'save'), 'saved game\n');
  chmodSync(join(source, 'save'), 0o640);
  if (process.getuid() === 0) chownSync(join(source, 'save'), 1000, 1000);
  execFileSync('python3', ['-c',
    'import os,struct,sys; p=sys.argv[1]; os.setxattr(p,"user.spacewars-test",b"saved metadata"); os.setxattr(p,"system.posix_acl_access",struct.pack("<I",2)+b"".join(struct.pack("<HHI",*e) for e in [(1,6,0xffffffff),(2,4,1001),(4,4,0xffffffff),(16,4,0xffffffff),(32,0,0xffffffff)]))',
    join(source, 'save')]);
  linkSync(join(source, 'save'), join(source, 'same-save'));
  symlinkSync('save', join(source, 'latest'));
  const run = (program, args, options) => {
    assert.equal(program, 'sudo');
    return execFileSync(args[0], args.slice(1), { encoding: 'utf8', ...options }).trim();
  };
  copyTree(source, backup, run);
  copyTree(backup, restored, run);
  assert.equal(readFileSync(join(restored, 'save'), 'utf8'), 'saved game\n');
  const a = lstatSync(join(source, 'save'));
  const b = lstatSync(join(restored, 'save'));
  assert.deepEqual([b.uid, b.gid, b.mode & 0o777], [a.uid, a.gid, 0o640]);
  assert.equal(b.ino, lstatSync(join(restored, 'same-save')).ino);
  assert.equal(readlinkSync(join(restored, 'latest')), 'save');
  execFileSync('python3', ['-c',
    'import os,sys; a,b=sys.argv[1:]; assert {k:os.getxattr(a,k) for k in os.listxattr(a)} == {k:os.getxattr(b,k) for k in os.listxattr(b)}',
    join(source, 'save'), join(restored, 'save')]);
});

test('public key preflight rejects private files and mixed malformed public lines', t => {
  const base = temporary(t);
  const key = join(base, 'fixture');
  execFileSync('ssh-keygen', ['-q', '-t', 'ed25519', '-N', '', '-f', key]);
  assert.deepEqual(readPublicKey(key + '.pub'), readFileSync(key + '.pub'));
  assert.throws(() => readPublicKey(key), /public SSH key/);
  writeFileSync(key + '.pub', Buffer.concat([readFileSync(key + '.pub'), Buffer.from('invalid\n')]));
  assert.throws(() => readPublicKey(key + '.pub'), /rejected/);
});

test('read-only backups reject unreplayed journals before mounting or copying', t => {
  const base = temporary(t);
  const filesystem = join(base, 'fixture.ext4');
  execFileSync('truncate', ['-s', '16M', filesystem]);
  execFileSync('mkfs.ext4', ['-q', '-F', filesystem]);
  const header = () => execFileSync('dumpe2fs', ['-h', filesystem], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  assert.doesNotThrow(() => assertCleanFilesystem(header()));
  execFileSync('debugfs', ['-w', '-R', 'feature needs_recovery', filesystem], { stdio: 'pipe' });
  const dirty = header();
  assert.throws(() => assertCleanFilesystem(dirty), /needs recovery/);
  assert.throws(() => assertCleanFilesystem('Filesystem state: clean'), /needs recovery/);
  const m = model();
  const run = (program, args, options) => args[0] === 'dumpe2fs' ? dirty : m.run(program, args, options);
  assert.throws(() => backupData('/dev/sdz', { run, base }), /needs recovery/);
  assert.ok(!m.calls.some(call => ['mount', 'rsync'].includes(call.args[0])));
});
