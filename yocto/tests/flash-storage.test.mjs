import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { chmodSync, chownSync, existsSync, linkSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readlinkSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { assertCleanFilesystem, backupData, copyTree, execute, installKey, legacyRootArgument, preserveLegacyHostKeys, provisionAccess, readPublicKey, requireGnuInstall, restoreData } from '../scripts/flash-storage.mjs';

const GNU_INSTALL = '/usr/bin/gnuinstall';
const isInstall = command => /\/(gnuinstall|install)$/.test(command);

function temporary(t) {
  const directory = mkdtempSync(join(tmpdir(), 'spacewars-flash-test-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  return directory;
}

function model({ store = false, admin = false, sudoFailure = false, mismatch = false, mounted = false, recovery = false } = {}) {
  const calls = [];
  const mounts = new Map();
  const run = (program, args, options = {}) => {
    const call = { program, args, options };
    calls.push(call);
    if (program === GNU_INSTALL && args[0] === '--version') return 'install (GNU coreutils) 9.7';
    if (program === 'lsblk') {
      const number = Number(args.at(-1).at(-1));
      return JSON.stringify({ blockdevices: [{ type: 'part', fstype: number === 1 ? 'vfat' : 'ext4', label: number === 4 ? 'data' : 'rootfs_a', mountpoints: [mounted ? '/in-use' : null] }] });
    }
    assert.equal(program, 'sudo');
    if (sudoFailure) throw Object.assign(new Error('sudo denied'), { status: 1 });
    const [command, ...rest] = args;
    if (isInstall(command) && !rest.includes('-d')) call.key = readFileSync(rest.at(-2));
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
      if (rest[0].endsWith('/etc/spacewars/boot-id')) return 'a'.repeat(64);
    }
    if (command === 'stat') {
      const path = rest.at(-1);
      if (path.endsWith('/.ssh')) {
        const uid = path.includes('/home/swadmin/') ? 1001 : 1000;
        return `${uid}:${uid}:41c0`;
      }
      if (path.endsWith('/authorized_keys')) {
        const uid = path.includes('/home/swadmin/') ? 1001 : 1000;
        return `${uid}:${uid}:8180:1`;
      }
      if (path.endsWith('/spacewars-access')) return '0:0:41ed';
      return `0:0:${path.endsWith('/spacewars-admin') ? '8120' : path.endsWith('/root') ? '8180' : '81a4'}:1`;
    }
    return '';
  };
  return { calls, mounts, run };
}

test('GNU ownership tool selection accepts standard paths and rejects incompatible tools before mounting', () => {
  for (const available of [GNU_INSTALL, '/bin/gnuinstall', '/usr/bin/install', '/bin/install']) {
    const run = (program, args) => {
      assert.deepEqual(args, ['--version']);
      if (program === available) return 'install (GNU coreutils) 9.7\n';
      if (program.endsWith('/gnuinstall')) throw Object.assign(new Error('not found'), { code: 'ENOENT' });
      return 'install (uutils coreutils) 0.8.0';
    };
    assert.equal(requireGnuInstall(run), available);
  }
  for (const missing of [false, true]) {
    const run = (program, args) => {
      assert.ok(isInstall(program), 'must fail before sudo, mounting or disk inspection');
      assert.deepEqual(args, ['--version']);
      if (missing) throw Object.assign(new Error('not found'), { code: 'ENOENT' });
      return 'install (uutils coreutils) 0.8.0';
    };
    assert.throws(() => provisionAccess('/dev/sdz', Buffer.from('a'), Buffer.from('b'), { run }), /GNU install is required/);
    assert.throws(() => preserveLegacyHostKeys('/dev/sdz', { hasHostKeys: false }, { run }), /GNU install is required/);
  }
});

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
  const writes = m.calls.filter(call => call.key);
  assert.equal(writes.length, 2);
  assert.match(writes[0].args.at(-1), /home\/spacewars\/\.ssh\/authorized_keys$/);
  assert.deepEqual(writes[0].key, runtime);
  assert.match(writes[1].args.at(-1), /home\/swadmin\/\.ssh\/authorized_keys$/);
  assert.deepEqual(writes[1].key, admin);
  assert.ok(writes[1].args.includes('1001'));
  assert.ok(m.calls.filter(call => isInstall(call.args[0])).every(call => call.args[0] === GNU_INSTALL));
});

test('restoring an old store provisions only the new owner role and preserves existing roles', () => {
  const m = model({ store: true });
  assert.equal(provisionAccess('/dev/sdz', Buffer.from('runtime'), Buffer.from('owner'), m), 'provisioned');
  const writes = m.calls.filter(call => call.key);
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
    assert.equal(m.calls.filter(call => call.key).length, replaceAdmin ? 3 : 2);
  }
});

test('unsafe admin key targets and unavailable sudo never become a first-boot fallback', () => {
  const m = model({ store: true, admin: true });
  // Symlinks, directories, unsafe permissions, wrong owners and hard links
  // remain invalid even when empty regular key files are accepted.
  for (const observed of ['0:0:a1ff:1', '0:0:41a4:1', '0:0:81b6:1', '1000:0:81a4:1', '0:1000:81a4:1', '0:0:81a4:2']) {
    const run = (program, args, options) => args[0] === 'stat' && args.at(-1).endsWith('/swadmin')
      ? observed : m.run(program, args, options);
    assert.throws(() => provisionAccess('/dev/sdz', Buffer.from('a'), Buffer.from('b'), { run }), /Unsafe SSH/);
  }
  assert.throws(() => provisionAccess('/dev/sdz', Buffer.from('a'), Buffer.from('b'), model({ sudoFailure: true })), /sudo denied/);
});

test('real key installation preserves exact bytes and permissions when invoked through Node', t => {
  const base = temporary(t);
  const destination = join(base, 'authorized_keys');
  const bytes = Buffer.from('# owner key\nssh-ed25519 fixture-public-bytes comment\n');
  const owners = process.getuid() === 0 ? [0, 1000, 1001] : [process.getuid()];
  const sources = [];
  const run = (program, args, options = {}) => {
    if (program !== 'sudo') return execute(program, args, options);
    if (isInstall(args[0])) sources.push(args.at(-2));
    return execute(args[0], args.slice(1), options);
  };
  for (const uid of owners) {
    for (const mode of ['600', '644']) {
      installKey(run, destination, bytes, uid, mode);
      assert.deepEqual(readFileSync(destination), bytes);
      const metadata = lstatSync(destination);
      assert.deepEqual([metadata.uid, metadata.gid, metadata.mode & 0o777], [uid, uid, parseInt(mode, 8)]);
    }
  }
  assert.ok(sources.every(path => !existsSync(path)));
});

test('real empty key files retain valid permissions and represent disabled or revoked access', t => {
  const destination = join(temporary(t), 'root');
  const run = (program, args, options) => program === 'sudo'
    ? execute(args[0], args.slice(1), options) : execute(program, args, options);
  for (const mode of ['600', '644']) {
    installKey(run, destination, Buffer.alloc(0), process.getuid(), mode);
    const metadata = lstatSync(destination);
    assert.equal(metadata.size, 0);
    assert.equal(metadata.mode & 0o777, parseInt(mode, 8));
    assert.equal(metadata.nlink, 1);
  }
});

test('real provisioning creates runtime and admin directories and keys with offline numeric owners', {
  skip: process.getuid() !== 0 ? 'Run in a disposable root test container to check UIDs 1000 and 1001' : false,
}, t => {
  const roots = [];
  const writes = [];
  const results = [];
  const runtime = Buffer.from('runtime fixture key\n');
  const admin = Buffer.from('admin fixture key\n');
  const bootIdentity = 'a'.repeat(64);
  t.after(() => roots.forEach(root => rmSync(root, { recursive: true, force: true })));
  const file = (root, path, bytes, mode) => {
    const target = join(root, path);
    mkdirSync(join(target, '..'), { recursive: true });
    writeFileSync(target, bytes, { mode });
  };
  const metadata = path => {
    const stat = lstatSync(path);
    return [stat.uid, stat.gid, stat.mode & 0o777, stat.isDirectory() ? 'directory' : 'file'];
  };
  const run = (program, args, options) => {
    if (program === 'lsblk') return model().run(program, args, options);
    if (program !== 'sudo') return execute(program, args, options);
    const [command, ...rest] = args;
    if (command === 'mount') {
      const root = rest.at(-1);
      roots.push(root);
      if (rest.at(-2).endsWith('2')) {
        file(root, 'etc/passwd', 'spacewars:x:1000:1000::/home/spacewars:/bin/bash\nswadmin:x:1001:1001::/home/swadmin:/bin/bash\n', 0o644);
        file(root, 'etc/spacewars/boot-id', bootIdentity + '\n', 0o644);
        file(root, 'etc/sudoers.d/spacewars-admin', 'swadmin ALL=(root) NOPASSWD: ALL\n', 0o440);
        for (const [user, uid] of [['spacewars', 1000], ['swadmin', 1001]]) {
          const home = join(root, 'home', user);
          mkdirSync(home, { recursive: true });
          chownSync(home, uid, uid);
        }
      } else {
        file(root, 'spacewars-access/root', '', 0o600);
        file(root, 'spacewars-access/spacewars', 'retained runtime key\n', 0o644);
      }
      return '';
    }
    if (command === 'umount') {
      const root = rest[0];
      assert.ok(roots.includes(root));
      const entries = existsSync(join(root, 'home'))
        ? ['home/spacewars/.ssh', 'home/spacewars/.ssh/authorized_keys', 'home/swadmin/.ssh', 'home/swadmin/.ssh/authorized_keys']
        : ['spacewars-access/root', 'spacewars-access/spacewars', 'spacewars-access/swadmin'];
      for (const path of entries) {
        const target = join(root, path);
        results.push({ path, metadata: metadata(target), bytes: lstatSync(target).isFile() ? readFileSync(target) : null });
      }
      for (const path of ['etc', 'home', 'spacewars-access']) rmSync(join(root, path), { recursive: true, force: true });
      return '';
    }
    if (isInstall(command)) writes.push(command);
    return execute(command, rest, options);
  };
  assert.equal(provisionAccess('/dev/sdz', runtime, admin, { run, bootIdentity }), 'provisioned');
  assert.deepEqual(results, [
    { path: 'home/spacewars/.ssh', metadata: [1000, 1000, 0o700, 'directory'], bytes: null },
    { path: 'home/spacewars/.ssh/authorized_keys', metadata: [1000, 1000, 0o600, 'file'], bytes: runtime },
    { path: 'home/swadmin/.ssh', metadata: [1001, 1001, 0o700, 'directory'], bytes: null },
    { path: 'home/swadmin/.ssh/authorized_keys', metadata: [1001, 1001, 0o600, 'file'], bytes: admin },
    { path: 'spacewars-access/root', metadata: [0, 0, 0o600, 'file'], bytes: Buffer.alloc(0) },
    { path: 'spacewars-access/spacewars', metadata: [0, 0, 0o644, 'file'], bytes: Buffer.from('retained runtime key\n') },
    { path: 'spacewars-access/swadmin', metadata: [0, 0, 0o644, 'file'], bytes: admin },
  ]);
  assert.deepEqual(writes, Array(5).fill(requireGnuInstall()));
  assert.ok(roots.every(root => !existsSync(root)));
});

test('key provisioning rejects the wrong boot image and unverified key copies', () => {
  const m = model();
  assert.throws(() => provisionAccess('/dev/sdz', Buffer.from('a'), Buffer.from('b'), {
    ...m, bootIdentity: 'b'.repeat(64),
  }), /boot identity/);
  assert.ok(!m.calls.some(call => call.key));
  let snapshot;
  const run = (program, args, options) => {
    if (args[0] === 'cmp') {
      snapshot = args.at(-2);
      throw new Error('key copy mismatch');
    }
    return m.run(program, args, options);
  };
  assert.throws(() => provisionAccess('/dev/sdz', Buffer.from('a'), Buffer.from('b'), { run }), /key copy mismatch/);
  assert.ok(!existsSync(snapshot));
  assert.equal(m.mounts.size, 0);
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
