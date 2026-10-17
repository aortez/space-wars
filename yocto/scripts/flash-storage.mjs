// Workstation operations on an explicitly selected, offline flash device.
// Keep real numeric owners, ACLs and xattrs; --fake-super is not a restore.
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, rmdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { YOCTO_DIR } from './paths.mjs';

export const ADMIN_USER = 'swadmin';
export const ADMIN_UID = 1001;
const COPY = ['-aHAX', '--numeric-ids', '--one-file-system', '--exclude=/lost+found'];

export function execute(program, args, options = {}) {
  return execFileSync(program, args, {
    encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, LC_ALL: 'C' }, ...options,
  }).trim();
}

export function requireGnuInstall(run = execute) {
  // Some install implementations resolve numeric owners through the host's
  // account database. Offline image UIDs need not exist on the workstation.
  for (const path of ['/usr/bin/gnuinstall', '/bin/gnuinstall', '/usr/bin/install', '/bin/install']) {
    try {
      if (/^install \(GNU coreutils\) /m.test(run(path, ['--version']))) return path;
    } catch { /* Try the next system location. */ }
  }
  throw new Error('GNU install is required to preserve offline image ownership. Install GNU coreutils (gnu-coreutils on Ubuntu systems using uutils) before flashing.');
}

function partition(device, number) {
  if (!/^\/dev\/(sd[a-z]+|mmcblk\d+|nvme\d+n\d+)$/.test(device)) {
    throw new Error('Expected a selected whole-disk device.');
  }
  return `${device}${/\d$/.test(device) ? 'p' : ''}${number}`;
}

function sudo(run, ...args) { return run('sudo', args); }
function exists(run, path) {
  // A failed sudo invocation must not masquerade as a missing path.
  const result = sudo(run, '/bin/sh', '-c',
    'if test -e "$1" || test -L "$1"; then printf present; else printf absent; fi', 'flash-path', path);
  if (!['present', 'absent'].includes(result)) throw new Error('Could not inspect the flash filesystem.');
  return result === 'present';
}

export function assertCleanFilesystem(report) {
  const state = /^Filesystem state:\s*(.+)$/m.exec(report)?.[1].trim();
  const features = /^Filesystem features:\s*(.+)$/m.exec(report)?.[1].split(/\s+/);
  if (state !== 'clean' || !features || features.includes('needs_recovery') || features.includes('orphan_present')) {
    throw new Error('The source filesystem needs recovery before backup. Shut the Pi down cleanly or repair the offline filesystem, then retry; nothing will be flashed.');
  }
}

function mounted(device, number, writable, run, work) {
  const source = partition(device, number);
  const report = JSON.parse(run('lsblk', ['--json', '--output', 'TYPE,FSTYPE,LABEL,MOUNTPOINTS', source]));
  const entries = report.blockdevices;
  const filesystem = number === 1 ? 'vfat' : 'ext4';
  if (entries?.length !== 1 || entries[0].type !== 'part' || entries[0].fstype !== filesystem
      || (number === 4 && entries[0].label !== 'data')
      || !Array.isArray(entries[0].mountpoints) || entries[0].mountpoints.some(Boolean)) {
    throw new Error(`${source} must be an unmounted ${number === 4 ? 'data-labelled ' : ''}${filesystem} partition.`);
  }
  if (!writable && filesystem === 'ext4') assertCleanFilesystem(sudo(run, 'dumpe2fs', '-h', source));
  const target = mkdtempSync(join(tmpdir(), 'spacewars-flash-mount-'));
  let attached = false;
  try {
    const options = writable ? 'rw,nosuid,nodev,noexec'
      : number === 1 ? 'ro,nosuid,nodev,noexec' : 'ro,noload,nosuid,nodev,noexec';
    sudo(run, 'mount', '-o', options, source, target);
    attached = true;
    const result = work(target);
    if (writable) sudo(run, 'sync', '-f', target);
    return result;
  } finally {
    // Never hide an unmount failure or remove a directory still holding a mount.
    if (attached) sudo(run, 'umount', target);
    rmdirSync(target);
  }
}

export function copyTree(source, destination, run = execute) {
  sudo(run, 'rsync', ...COPY, `${source}/`, `${destination}/`);
  const difference = sudo(run, 'rsync', ...COPY, '--checksum', '--dry-run', '--delete',
    '--itemize-changes', `${source}/`, `${destination}/`);
  if (difference) throw new Error('Data copy verification failed; keep the backup and do not flash.');
  sudo(run, 'sync', '-f', destination);
}

export function backupData(device, { run = execute, base = join(YOCTO_DIR, '../target/flash-backups') } = {}) {
  mkdirSync(base, { recursive: true, mode: 0o700 });
  const directory = mkdtempSync(join(resolve(base), 'spacewars-'));
  const tree = join(directory, 'data');
  mkdirSync(tree, { mode: 0o700 });
  try {
    const summary = mounted(device, 4, false, run, source => {
      // Recovery transactions are tied to the old disk/slot identities. Keep
      // them for recovery, but do not silently reuse them after repartitioning.
      for (const retained of ['spacewars-bootstrap/active', 'spacewars-system/recovery']) {
        if (exists(run, join(source, retained))) {
          throw new Error('This disk has OTA recovery state. Use its supervised migration or explicitly discard data; a full reflash cannot reuse its old slot journal.');
        }
      }
      copyTree(source, tree, run);
      const wifi = join(tree, 'NetworkManager/system-connections');
      let hasWifi = false;
      if (exists(run, wifi)) {
        hasWifi = !!sudo(run, 'find', wifi, '-maxdepth', '1', '-type', 'f', '-name', '*.nmconnection', '-print', '-quit');
      }
      const hasHostKeys = exists(run, join(tree, 'ssh/ssh_host_ed25519_key'));
      return { directory, tree, hasWifi, hasHostKeys };
    });
    writeFileSync(join(directory, 'backup.json'), JSON.stringify({ schema: 1, device, ...summary }, null, 2) + '\n', { mode: 0o600 });
    sudo(run, 'sync', '-f', directory);
    return summary;
  } catch (error) {
    throw new Error(`${error.message}\nBackup retained for inspection: ${directory}`, { cause: error });
  }
}

export function restoreData(device, backup, { run = execute } = {}) {
  mounted(device, 4, true, run, target => copyTree(backup.tree, target, run));
}

export function preserveLegacyHostKeys(device, backup, { run = execute } = {}) {
  if (!backup || backup.hasHostKeys) return backup;
  const install = requireGnuInstall(run);
  const rootArgument = mounted(device, 1, false, run, boot => {
    if (exists(run, join(boot, 'tryboot.txt')) || exists(run, join(boot, 'autoboot.txt'))) {
      throw new Error('Cannot select legacy SSH host keys from this boot configuration. Review the boot selection before flashing.');
    }
    return legacyRootArgument(name => sudo(run, 'head', '-c', '65537', join(boot, name)));
  });
  const selected = [2, 3].filter(number => {
    const source = partition(device, number);
    if (rootArgument.startsWith('/dev/')) return rootArgument === `/dev/sda${number}`;
    const [field, value] = rootArgument.split('=');
    return run('blkid', ['-s', field, '-o', 'value', source]) === value;
  });
  if (selected.length !== 1) throw new Error('The legacy boot root does not match exactly one root partition.');
  mounted(device, selected[0], false, run, root => {
    const source = join(root, 'etc/ssh');
    const names = [];
    for (const algorithm of ['rsa', 'ecdsa', 'ed25519']) {
      for (const suffix of ['', '.pub']) {
        const file = `ssh_host_${algorithm}_key${suffix}`;
        if (exists(run, join(source, file))) names.push(file);
      }
    }
    if (!names.some(name => !name.endsWith('.pub'))) return;
    const target = join(backup.tree, 'ssh');
    sudo(run, install, '-d', '-o', '0', '-g', '0', '-m', '700', target);
    // Verify the copied files, including numeric owners and modes, before
    // allowing the old root to be overwritten. Key bytes never enter logs.
    sudo(run, 'rsync', '-aHAX', '--numeric-ids', ...names.map(name => join(source, name)), `${target}/`);
    const difference = sudo(run, 'rsync', '-aHAX', '--numeric-ids', '--checksum', '--dry-run',
      '--itemize-changes', ...names.map(name => join(source, name)), `${target}/`);
    if (difference) throw new Error('Legacy SSH host key backup failed verification.');
    sudo(run, 'sync', '-f', target);
    backup.hasHostKeys = true;
  });
  return backup;
}

export function legacyRootArgument(read) {
  const visited = new Set();
  const commandLines = new Set();
  let section = '[all]';
  const bounded = name => {
    if (!/^[a-zA-Z0-9_./-]{1,255}$/.test(name) || name.startsWith('/')
        || name.split('/').some(part => ['.', '..', ''].includes(part))) throw new Error('Unsafe boot include path.');
    const text = read(name);
    if (Buffer.byteLength(text) > 65536) throw new Error('Boot configuration exceeds its size limit.');
    return text;
  };
  const visit = (name, depth) => {
    if (visited.has(name) || visited.size >= 32 || depth > 8) throw new Error('Repeated or excessive boot includes.');
    visited.add(name);
    for (const line of bounded(name).split('\n').map(line => line.split('#')[0].trim())) {
      if (line.startsWith('[')) section = line;
      if (line.startsWith('include ')) visit(line.slice(8).trim(), depth + 1);
      if (line.startsWith('cmdline=')) {
        if (section !== '[all]') throw new Error('Conditional boot command line; review the active root before flashing.');
        commandLines.add(line.slice(8).trim());
      }
    }
  };
  visit('config.txt', 0);
  if (commandLines.size > 1) throw new Error('Ambiguous boot command lines; review the active root before flashing.');
  const args = bounded(commandLines.size ? [...commandLines][0] : 'cmdline.txt').split(/\s+/).filter(word => word.startsWith('root='));
  if (args.length !== 1 || !/^root=(PARTUUID=[a-fA-F0-9-]+|LABEL=rootfs_[ab]|\/dev\/sda[23])$/.test(args[0])) {
    throw new Error('Unsupported legacy root selection; cannot preserve the active SSH host identity.');
  }
  return args[0].slice(5);
}

export function readPublicKey(path, run = execute) {
  const bytes = readFileSync(path);
  const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  if (!bytes.length || bytes.length > 64 * 1024 || bytes.includes('PRIVATE KEY')
      || /[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/.test(text)) {
    throw new Error('Expected a bounded public SSH key file.');
  }
  const lines = text.split(/\r?\n/).filter(line => line.trim() && !line.trimStart().startsWith('#'));
  const snapshot = mkdtempSync(join(tmpdir(), 'spacewars-public-key-'));
  let fingerprints;
  try {
    writeFileSync(join(snapshot, 'key.pub'), bytes, { mode: 0o600 });
    fingerprints = run('ssh-keygen', ['-l', '-E', 'sha256', '-f', join(snapshot, 'key.pub')]).split('\n');
  } finally { rmSync(snapshot, { recursive: true, force: true }); }
  if (lines.length === 0 || lines.length > 64 || lines.some(line => line.length > 8192)
      || fingerprints.length !== lines.length || fingerprints.some(line => !/^\d+ SHA256:/.test(line))) {
    throw new Error('One or more SSH public key lines were rejected by OpenSSH.');
  }
  return bytes;
}

export function installKey(run, path, bytes, uid, mode, install = requireGnuInstall(run)) {
  // Node's child stdin can be a socket that install cannot reopen through
  // /dev/stdin. Use a private regular file and verify the installed copy.
  const directory = mkdtempSync(join(tmpdir(), 'spacewars-key-install-'));
  const source = join(directory, 'key.pub');
  try {
    writeFileSync(source, bytes, { mode: 0o600 });
    sudo(run, install, '-o', String(uid), '-g', String(uid), '-m', mode, source, path);
    sudo(run, 'cmp', '--', source, path);
    checkPath(run, path, mode, 'regular file', uid);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

function checkPath(run, path, mode, type, uid = 0) {
  const format = `%u:%g:%a:%F${type === 'regular file' ? ':%h' : ''}`;
  if (sudo(run, 'stat', '-c', format, '--', path) !== `${uid}:${uid}:${mode}:${type}${type === 'regular file' ? ':1' : ''}`) {
    throw new Error(`Unsafe SSH access path: ${path}`);
  }
}

export function provisionAccess(device, runtimeKey, adminKey, { run = execute, replaceAdmin = false, bootIdentity } = {}) {
  const install = requireGnuInstall(run);
  mounted(device, 2, true, run, root => {
    if (bootIdentity !== undefined
        && (!/^[a-f0-9]{64}$/.test(bootIdentity) || sudo(run, 'cat', join(root, 'etc/spacewars/boot-id')) !== bootIdentity)) {
      throw new Error('The flashed root does not match the selected image boot identity.');
    }
    const passwd = sudo(run, 'cat', join(root, 'etc/passwd'));
    const accounts = passwd.split('\n').map(line => line.split(':'));
    for (const [user, uid] of [['spacewars', 1000], [ADMIN_USER, ADMIN_UID]]) {
      const matches = accounts.filter(fields => fields[0] === user);
      if (matches.length !== 1 || matches[0][2] !== String(uid) || matches[0][3] !== String(uid)
          || matches[0][5] !== `/home/${user}` || matches[0][6] !== '/bin/bash') {
        throw new Error(`The selected image does not contain the expected ${user} account. Build a current image.`);
      }
    }
    const rule = join(root, 'etc/sudoers.d/spacewars-admin');
    checkPath(run, rule, '440', 'regular file');
    if (sudo(run, 'cat', rule).trim() !== 'swadmin ALL=(root) NOPASSWD: ALL') {
      throw new Error('The selected image has an unsupported admin sudo policy.');
    }
    for (const [user, uid, bytes] of [['spacewars', 1000, runtimeKey], [ADMIN_USER, ADMIN_UID, adminKey]]) {
      const directory = join(root, 'home', user, '.ssh');
      sudo(run, install, '-d', '-o', String(uid), '-g', String(uid), '-m', '700', directory);
      checkPath(run, directory, '700', 'directory', uid);
      installKey(run, join(directory, 'authorized_keys'), bytes, uid, '600', install);
    }
  });
  return mounted(device, 4, true, run, data => {
    const directory = join(data, 'spacewars-access');
    if (!exists(run, directory)) return 'provisioned-on-first-boot';
    checkPath(run, directory, '755', 'directory');
    for (const [role, mode] of [['root', '600'], ['spacewars', '644']]) {
      checkPath(run, join(directory, role), mode, 'regular file');
    }
    const path = join(directory, ADMIN_USER);
    if (exists(run, path)) {
      checkPath(run, path, '644', 'regular file');
      if (!replaceAdmin) return 'preserved';
    }
    // An older restored store has no admin role. Physical flashing explicitly
    // provisions the selected owner key, without promoting runtime credentials.
    installKey(run, path, adminKey, 0, '644', install);
    return replaceAdmin ? 'replaced' : 'provisioned';
  });
}
