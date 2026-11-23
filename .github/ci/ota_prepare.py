"""Builds one unsigned Nightly candidate from main and compares it with the newest published generation.

  ota_release.py candidate [SOURCE] [--accept SHA256] [--baseline GENERATION] [--previous-bundle DIR]

SOURCE is a commit on main whose CI push run passed, by default the newest
such commit. The pinned build driver builds it in the configured build
checkout, which may hold nothing but that commit and what builds leave, and
the pinned image_candidate packages it. The baseline is the newest generation
the journal records as published on Nightly, or the one --baseline names, and
its payload comes from the published bundle. Compared with the baseline, these
must not change:

- anything under yocto/, which holds every build input except the app's own
  source;
- the data range and the protocols the image needs;
- any entry of the expanded root, in type, mode, owner, flags, extended
  attributes, content or link target, except the content of the app's files
  and NSS's self-check files.

A change to any of them stops preparation and leaves differences.json in the
candidate directory, with its digest and the command that accepts it. That
command reuses the build and records the acceptance, and the next generation
is compared with this one. A change to the boot set, machine, architecture or
layout, or modules for another kernel, stops preparation with no acceptance,
since such changes take the supervised path (#268).

Every image must also pass the content checks: the packaged image record, the
packaged service and journal files against their sources, the device
service's block device rules, an fstab that mounts nothing by device name, the
app's two main binaries against the image record, and none of the owner's
fault helpers packaged.

The candidate directory is the workspace's <source[:8]>/, which ota_release.py
nightly then plans. Its code.json names the code that started it, and only
that code resumes it. A directory compared with an older generation than the
newest is compared again. This never signs, publishes, takes the publisher
lock or contacts a device, and it only reads GitHub.
"""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

import ota_build_inputs as build_inputs
import ota_candidate as packager
import ota_image_policy as image_policy
from ota_delivery import REPOSITORY

HERE = Path(__file__).resolve().parent
TOOL_ENV = {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'}
DEBUGFS = '/usr/sbin/debugfs'
# A cold build takes hours; a warm one about eleven minutes.
BUILD_TIMEOUT = 12 * 60 * 60
COMMIT = re.compile(r'[0-9a-f]{40}\Z')
REQUESTED = re.compile(r'[0-9a-f]{7,40}\Z')
DIGEST = re.compile(r'[0-9a-f]{64}\Z')
# The app's own files, whose content new app source changes; their type, mode and owner still may not.
APP_FILES = frozenset({
    '/etc/spacewars/image.json',
    '/usr/bin/engine-agent',
    '/usr/bin/engine-client',
    '/usr/bin/engine-os-manager',
    '/usr/bin/falling-benchmark',
    '/usr/bin/spacewars-cli',
    '/usr/share/spacewars/build-inputs.json',
    '/usr/share/spacewars/build.json',
    '/usr/share/spacewars/fast-update-compat',
    '/usr/share/spacewars/source.json',
})
# NSS's package script signs its libraries again whenever an image is assembled, and the signatures differ each time.
SELF_CHECKS = frozenset({'/usr/lib/libfreebl3.chk', '/usr/lib/libnssdbm3.chk', '/usr/lib/libsoftokn3.chk'})
VARIABLE = APP_FILES | SELF_CHECKS
# What a Nightly may not change, as a label, the image record's name and the journal's.
FIXED = (('architecture', 'architecture', 'architecture'), ('boot set', 'boot_identity', 'boot_identity'),
         ('layout', 'layout', 'layout'), ('machine', 'machine', 'machine'))
# What a unit needs to install the image, whose change the owner reviews: the image record's name, then the journal's.
COMPATIBILITY = (('data', 'data'), ('recovery_protocol', 'min_recovery_protocol'),
                 ('updater_protocol', 'min_updater_protocol'))
RECIPE = 'yocto/meta-spacewars/recipes-spacewars/spacewars/files/'
JOURNAL = 'yocto/meta-spacewars/recipes-support/spacewars-journal/files/'
# Packaged copies of source files, with their modes.
PACKAGED = (
    ('/etc/spacewars/development-runtime.json', RECIPE + 'development-runtime.json', '0644'),
    ('/etc/systemd/journald.conf.d/spacewars-journal.conf', JOURNAL + 'spacewars-journal.conf', '0644'),
    ('/usr/lib/systemd/system/spacewars-journal.service', JOURNAL + 'spacewars-journal.service', '0644'),
    ('/usr/lib/systemd/system/spacewars-kiosk.service', RECIPE + 'spacewars-kiosk.service', '0644'),
    ('/usr/lib/systemd/system/spacewars-recovery.service', RECIPE + 'spacewars-recovery.service', '0644'),
    ('/usr/lib/systemd/system/spacewars-system.service', RECIPE + 'spacewars-system.service', '0644'),
    ('/usr/lib/systemd/system/sshd@.service.d/spacewars-network.conf', RECIPE + 'sshd-spacewars-network.conf',
     '0644'),
    ('/usr/lib/systemd/system/systemd-journal-flush.service.d/spacewars-persistent-journal.conf',
     JOURNAL + 'persistent-journal.conf', '0644'),
    ('/usr/lib/tmpfiles.d/spacewars-install.conf', RECIPE + 'spacewars-install.conf', '0644'),
    ('/usr/sbin/spacewars-journal-prepare', JOURNAL + 'spacewars-journal-prepare.sh', '0755'),
)
SYSTEM_UNIT = '/usr/lib/systemd/system/spacewars-system.service'
# The device service writes the inactive slot on a USB disk or an SD card.
DEVICE_RULES = frozenset({'DevicePolicy=closed', 'DeviceAllow=block-sd rw', 'DeviceAllow=block-mmc rw'})
# The owner's fault and approval helpers, which must never ship.
OWNER_HELPERS = ('bootstrap_retire', 'crash_kiosk_trial', 'fail_recovery_trial', 'fail_root_startup',
                 'hold_promotion_trial', 'install_signed_trial', 'kiosk_hang_trial', 'recovery_approval',
                 'reset_trial')
BINARIES = (('client', '/usr/bin/engine-client'), ('os_manager', '/usr/bin/engine-os-manager'))
# What ota_release.py nightly checks, byte for byte, before it plans.
ARTIFACTS = ('build.json', 'build.log', 'candidate/build.json', 'candidate/candidate.json',
             'candidate/image.ext4.zst', 'candidate/qualification.json', 'changes.json', 'code.json',
             'differences.json', 'image-verification.json', 'main-ci-run.json', 'policy.json')
KINDS = {stat.S_IFREG: 'file', stat.S_IFLNK: 'link', stat.S_IFDIR: 'dir'}
MAX_ENTRIES = 500_000
ENTRY = re.compile(rb'/(\d+)/([0-7]+)/(\d+)/(\d+)/([^/]*)/(\d*)/')
ECHO = re.compile(rb'debugfs: ls -p <(\d+)>')
BANNER = re.compile(rb'debugfs [0-9.]+ \([^)]*\)')
# rdump keeps each entry's mode, but only root may give it the image's owner; listing() reads the owners.
OWNERSHIP = re.compile(rb'(rdump|dump_file): Operation not permitted while changing ownership of .*')
PLAIN_PATH = re.compile(r'/[A-Za-z0-9._/+-]+\Z')
# Kernel modules sit in a directory per kernel version, under either spelling of the library directory.
MODULES = ('/lib/modules', '/usr/lib/modules')
STAT = re.compile(rb'debugfs: stat <(\d+)>')
HEADER = re.compile(rb'Inode: (\d+) +Type: [A-Za-z ]+ +Mode: +[0-7]+ +Flags: 0x([0-9a-f]+)')
NAMED = re.compile(rb'  ([A-Za-z0-9._-]+) \((\d+)\)( = .*)?')
GOT = re.compile(rb'debugfs: ea_get -x <(\d+)> ([A-Za-z0-9._-]+)')
VALUE = re.compile(rb'([A-Za-z0-9._-]+) \((\d+)\)(?: = ((?:[0-9a-f]{2} )*))?')
# How ext4 stores an inode rather than anything set on it: hashed directories, huge files, extents, attribute
# inodes and inline data.
STORAGE_FLAGS = 0x1000 | 0x40000 | 0x80000 | 0x200000 | 0x10000000
# What git ignores in a build checkout after builds: the layers kas checks out, the host tool wrappers,
# externalsrc's links into the build directory and Python's caches.
LEFTOVERS = re.compile('(yocto/(' + '|'.join(map(re.escape, build_inputs.REPOSITORIES)) + r'|\.hosttools)/'
                       r'|oe-workdir|oe-logs|(.*/)?__pycache__/.*)\Z')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def progress(message):
    print(message, file=sys.stderr, flush=True)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def encode(value):
    return (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


def blob(path):
    with open(os.open(path, os.O_RDONLY | os.O_NOFOLLOW), 'rb') as stream:
        return dict(bytes=os.fstat(stream.fileno()).st_size, sha256=hashlib.file_digest(stream, 'sha256').hexdigest())


def save(path, raw, exclusive=False):
    """Writes one preparation file atomically and durably; an exclusive save never replaces one."""
    with tempfile.NamedTemporaryFile(prefix='.' + path.name + '-', dir=path.parent, delete=False) as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    try:
        (os.link if exclusive else os.replace)(stream.name, path)
    finally:
        if os.path.lexists(stream.name):
            os.unlink(stream.name)
    directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)
    return raw


def git(checkout, *args, timeout=600):
    result = subprocess.run(['git', '-C', str(checkout), *args], capture_output=True, timeout=timeout)
    require(result.returncode == 0, f'git {args[0]} failed in {checkout}: '
            + result.stderr.decode(errors='replace').strip())
    return result.stdout


def head(checkout):
    return git(checkout, 'rev-parse', 'HEAD').decode().strip()


def known(checkout, commit):
    return subprocess.run(['git', '-C', str(checkout), 'cat-file', '-e', commit + '^{commit}'],
                          capture_output=True, timeout=120).returncode == 0


def ancestor(checkout, older, newer):
    result = subprocess.run(['git', '-C', str(checkout), 'merge-base', '--is-ancestor', older, newer],
                            capture_output=True, timeout=120)
    require(result.returncode in (0, 1), 'git merge-base failed: ' + result.stderr.decode(errors='replace').strip())
    return result.returncode == 0


def ci_runs(publisher, commit=None):
    args = ['run', 'list', '--repo', REPOSITORY, '--workflow', 'ci.yml', '--branch', 'main', '--event', 'push',
            '--limit', '30', '--json', 'databaseId,headSha,status,conclusion,url']
    return publisher.gh_json(args + (['--commit', commit] if commit else []))


def select_source(publisher, checkout, requested):
    """The commit to build, on main, and the CI push run that passed it."""
    git(checkout, 'fetch', '--quiet', 'origin', '+refs/heads/main:refs/remotes/origin/main')
    main = git(checkout, 'rev-parse', '--verify', 'origin/main^{commit}').decode().strip()
    if requested is None:
        passed = [run['headSha'] for run in ci_runs(publisher) if run.get('conclusion') == 'success']
        source = next((commit for commit in passed if isinstance(commit, str) and COMMIT.fullmatch(commit)
                       and known(checkout, commit) and ancestor(checkout, commit, main)), None)
        require(source, 'No recent commit on main has passed CI')
    else:
        require(isinstance(requested, str) and REQUESTED.fullmatch(requested), 'Name the source by its commit hash')
        require(known(checkout, requested), f'{requested} is not a commit the build checkout has fetched')
        source = git(checkout, 'rev-parse', '--verify', requested + '^{commit}').decode().strip()
        require(ancestor(checkout, source, main), f'{source} is not on main')
    runs = [run for run in ci_runs(publisher, source) if run.get('headSha') == source]
    passed = [run for run in runs if run.get('status') == 'completed' and run.get('conclusion') == 'success']
    if not passed:
        running = [run for run in runs if run.get('status') != 'completed']
        raise ValueError(f"CI is still running for {source}: {running[0]['url']}" if running
                         else f'{source} has not passed CI on main')
    return source, publisher.passed_ci(passed[0]['databaseId'], source)


def baseline(state, generation=None):
    """The published Nightly generation a candidate is compared with: the newest, unless one is named."""
    published = sorted((entry for entry in state['images'].values() if entry['nightly'] is not None),
                       key=lambda entry: entry['generation'])
    require(published, 'No generation is published yet, so a candidate has nothing to be compared with')
    if generation is None:
        return published[-1]
    chosen = [entry for entry in published if entry['generation'] == generation]
    require(chosen, f'Generation {generation} was never published on Nightly')
    return chosen[0]


def move(checkout, source):
    """Puts the build checkout at the source. Builds read its working tree, so it may hold no local changes and,
    of what git ignores, only what builds leave."""
    def unexpected():
        status = git(checkout, 'status', '--porcelain=v1', '-z', '--ignored=matching', '--untracked-files=all')
        return [os.fsdecode(entry[3:]) for entry in status.split(b'\0')
                if entry and not (entry.startswith(b'!! ') and LEFTOVERS.fullmatch(os.fsdecode(entry[3:])))]

    found = unexpected()
    require(not found, f'The build checkout {checkout} has local changes or ignored files: ' + ', '.join(found[:5]))
    if head(checkout) != source:
        git(checkout, 'checkout', '--quiet', '--detach', source)
    require(head(checkout) == source and not unexpected(), f'The build checkout did not move cleanly to {source}')


def yocto_changes(checkout, old, new):
    """Every path under yocto/ that differs between two commits, with the new content's digest."""
    fields = git(checkout, 'diff', '--no-renames', '--name-status', '-z', old, new, '--', 'yocto/').split(b'\0')
    changes = []
    for status, path in zip(fields[0:-1:2], fields[1::2]):
        name = os.fsdecode(path)
        content = None if status == b'D' else digest(git(checkout, 'show', f'{new}:{name}'))
        changes.append(dict(path=name, status=status.decode(), sha256=content))
    return changes


def changes(checkout, old, new):
    """The commits since the baseline's source, for the release record and notes."""
    commits = []
    for item in filter(None, git(checkout, 'log', '--reverse', '-z', '--format=%H%x1f%s', f'{old}..{new}')
                       .decode().split('\0')):
        commit, subject = item.split('\x1f', 1)
        files = git(checkout, 'diff-tree', '--no-commit-id', '--name-only', '-r', '-z', commit).decode().split('\0')
        number = re.search(r'\(#(\d+)\)\Z', subject)
        commits.append(dict(commit=commit, subject=subject, pull_request=int(number.group(1)) if number else None,
                            areas=sorted({'/'.join(name.split('/')[:2]) for name in files if name})))
    return commits


def build(config, source, directory):
    """Runs the pinned build driver against the build checkout; a recorded build is reused."""
    record, log = directory / 'build.json', directory / 'build.log'
    if not record.exists():
        progress(f'Building {source[:12]}; the log is {log}')
        with log.open('ab') as stream:
            result = subprocess.run(
                [sys.executable, str(HERE / 'ota_build_inputs.py'), '--source-checkout', str(config.build['checkout']),
                 '--source', source, '--build-directory', str(config.build['directory']), '--output', str(record),
                 '--cache', str(config.build['cache'])], stdout=stream, stderr=subprocess.STDOUT,
                timeout=BUILD_TIMEOUT)
        if result.returncode:
            tail = log.read_bytes()[-2000:].decode(errors='replace')
            raise ValueError(f'The build failed; its log is {log}:\n{tail}')
    value = json.loads(record.read_bytes())
    require(value.get('source_commit') == source, f'{record} records the build of another source')
    return value


def package(publisher, directory, record, policy):
    """Packages the recorded build output with the pinned image_candidate; a finished package is reused."""
    candidate = directory / 'candidate'
    if not candidate.exists():
        output = (publisher.config.build['directory'] / 'tmp/deploy/images' / build_inputs.MACHINE
                  / record['output']['name'])
        require(output.is_file(), f"The build output is gone; delete {directory / 'build.json'} to build again")
        progress('Packaging with the pinned image_candidate')
        packager.assemble(directory / 'policy.json', directory / 'build.json', output,
                          Path(publisher.tool('image_candidate')), candidate)
    require((candidate / 'build.json').read_bytes() == (directory / 'build.json').read_bytes(),
            f'{candidate} was packaged from another build')
    descriptor = json.loads((candidate / 'candidate.json').read_bytes())
    qualified = json.loads((candidate / 'qualification.json').read_bytes())
    require(qualified.get('build_inputs_sha256') == policy['build_inputs_sha256']
            and qualified.get('expanded') == descriptor.get('expanded'),
            f'{candidate} was packaged under another policy; delete it to package again')
    return descriptor, qualified


def expand(payload, compressed, expanded, destination):
    """Decompresses one payload of known digest; the expanded bytes must have theirs too."""
    require(blob(payload) == compressed, f'{payload} differs from its recorded digest')
    subprocess.run(['zstd', '--quiet', '--decompress', '--no-progress', '-o', str(destination), str(payload)],
                   env=TOOL_ENV, check=True, timeout=600)
    require(blob(destination) == expanded, f'{payload} expands to other bytes than its record names')
    return destination


def debugfs(image, requests):
    """Runs read-only debugfs requests and returns their output and any error lines."""
    result = subprocess.run([DEBUGFS, '-f', '-', str(image)], input=''.join(r + '\n' for r in requests).encode(),
                            capture_output=True, env=TOOL_ENV, timeout=600, check=True)
    return result.stdout, [line for line in result.stderr.splitlines() if line and not BANNER.fullmatch(line)]


def listing(image):
    """Every entry's type, mode and owner, and its inode, by path, from debugfs's listings of each directory."""
    entries, inodes, queue = {}, {}, {2: ''}
    while queue:
        output, errors = debugfs(image, [f'ls -p <{inode}>' for inode in queue])
        if errors:
            raise ValueError('debugfs could not list the image: ' + os.fsdecode(errors[0]))
        following, directory = {}, None
        for line in output.split(b'\n'):
            echo = ECHO.fullmatch(line)
            if echo:
                require(int(echo.group(1)) in queue, 'Unexpected debugfs listing')
                directory = queue[int(echo.group(1))]
                continue
            if not line:
                continue
            match = ENTRY.fullmatch(line)
            require(match and directory is not None, 'Unexpected debugfs listing: ' + os.fsdecode(line[:200]))
            inode, mode, uid, gid, name, size = match.groups()
            if directory == '' and name == b'.':
                name = b''  # The root itself, recorded as '/'.
            elif inode == b'0' or name in (b'.', b'..'):
                continue  # An empty slot, or a directory itself and its parent.
            path = directory + '/' + os.fsdecode(name)
            require(path not in entries and len(entries) < MAX_ENTRIES, 'Unexpected image tree at ' + path)
            mode = int(mode, 8)
            kind = KINDS.get(stat.S_IFMT(mode))
            require(kind, 'The image holds a device node, pipe or socket: ' + path)
            entries[path] = dict(type=kind, mode=f'{stat.S_IMODE(mode):04o}', uid=int(uid), gid=int(gid))
            inodes[path] = int(inode)
            if kind == 'dir':
                if path != '/':
                    following[int(inode)] = path
            else:
                entries[path]['size'] = int(size or 0)
        queue = following
    return entries, inodes


def attributes(image, entries, inodes):
    """Adds the inode flags and extended attributes of each entry that has any; inline data is content."""
    numbers = sorted(set(inodes.values()))
    output, errors = debugfs(image, [f'stat <{number}>' for number in numbers])
    if errors:
        raise ValueError('debugfs could not read the inodes: ' + os.fsdecode(errors[0]))
    flags, names, current, listed = {}, {}, None, False
    for line in output.split(b'\n'):
        echo, header = STAT.fullmatch(line), HEADER.fullmatch(line)
        if echo:
            current, listed = int(echo.group(1)), False
        elif header:
            require(int(header.group(1)) == current and current not in flags, 'Unexpected debugfs inode record')
            flags[current] = int(header.group(2), 16) & ~STORAGE_FLAGS
        elif line == b'Extended attributes:':
            listed = True
        elif listed and line.startswith(b'  '):
            named = NAMED.fullmatch(line)
            require(named, f'Unexpected extended attribute on inode {current}: ' + os.fsdecode(line[:200]))
            names.setdefault(current, []).append(named.group(1).decode())
        else:
            listed = False
    require(set(flags) == set(numbers), 'debugfs did not read every inode')
    wanted = [(number, name) for number, found in sorted(names.items()) for name in found if name != 'system.data']
    values, asked = {}, None
    if wanted:
        # stat leaves out values of 40 bytes or more, so each value is read in full.
        output, errors = debugfs(image, [f'ea_get -x <{number}> {name}' for number, name in wanted])
        if errors:
            raise ValueError('debugfs could not read an extended attribute: ' + os.fsdecode(errors[0]))
        for line in output.split(b'\n'):
            got, value = GOT.fullmatch(line), VALUE.fullmatch(line)
            if got:
                asked = (int(got.group(1)), got.group(2).decode())
            elif value and asked and value.group(1).decode() == asked[1]:
                raw = bytes.fromhex((value.group(3) or b'').decode())
                require(len(raw) == int(value.group(2)) and asked not in values,
                        f'Unexpected extended attribute value on inode {asked[0]}')
                values[asked], asked = raw.hex(), None
        require(set(values) == set(wanted), 'debugfs did not read every extended attribute')
    for path, number in inodes.items():
        if flags[number]:
            entries[path]['flags'] = f'{flags[number]:#x}'
        held = {name: value for (owner, name), value in values.items() if owner == number}
        if held:
            entries[path]['xattrs'] = held


def extracted(tree):
    """Every path rdump wrote below tree, making each directory searchable first, since rdump keeps image modes."""
    found = {'/': str(tree)}

    def fail(error):
        raise error

    os.chmod(tree, 0o700)
    for directory, names, files in os.walk(tree, onerror=fail):
        for name in names + files:
            path = os.path.join(directory, name)
            if stat.S_ISDIR(os.lstat(path).st_mode):
                os.chmod(path, 0o700)
            found['/' + os.path.relpath(path, tree)] = path
    return found


def inspect(image, scratch):
    """Lists and extracts one expanded root without mounting it; returns its entries and their local copies."""
    entries, inodes = listing(image)
    attributes(image, entries, inodes)
    tree = scratch / 'tree'
    tree.mkdir()
    require(PLAIN_PATH.fullmatch(str(tree)), f'debugfs needs a plain path to extract into, not {tree}')
    _, errors = debugfs(image, [f'rdump / {tree}'])
    unexpected = [line for line in errors if not OWNERSHIP.fullmatch(line)]
    if unexpected:
        raise ValueError('debugfs could not extract the image: ' + os.fsdecode(unexpected[0]))
    found = extracted(tree)
    require(set(found) == set(entries), 'debugfs extracted another tree than it listed')
    for path, entry in entries.items():
        local = found[path]
        info = os.lstat(local)
        if entry['type'] == 'file':
            require(stat.S_ISREG(info.st_mode) and info.st_size == entry['size'], 'Extracted file differs: ' + path)
            os.chmod(local, 0o600)
            entry['sha256'] = blob(local)['sha256']
        elif entry['type'] == 'link':
            require(stat.S_ISLNK(info.st_mode), 'Extracted link differs: ' + path)
            entry['target'] = os.readlink(local)
            require(len(os.fsencode(entry['target'])) == entry['size'], 'Extracted link differs: ' + path)
        else:
            require(stat.S_ISDIR(info.st_mode), 'Extracted directory differs: ' + path)
    return entries, found


def root_differences(old, new):
    """Entries that differ in any way beyond the content of the app's files and NSS's self-checks."""
    def fixed(entry):
        return {key: value for key, value in entry.items() if key not in ('sha256', 'size')}

    found = []
    for path in sorted(old.keys() | new.keys()):
        before, after = old.get(path), new.get(path)
        if before == after:
            continue
        if (path in VARIABLE and before and after and before['type'] == after['type'] == 'file'
                and fixed(before) == fixed(after)):
            continue
        found.append(dict(path=path, baseline=before, candidate=after))
    return found


def kernels(entries):
    """The kernel versions a root carries modules for."""
    return sorted(path.rsplit('/', 1)[1] for path, entry in entries.items()
                  if entry['type'] == 'dir' and path.rsplit('/', 1)[0] in MODULES)


def compatibility(image, base, generation):
    """The reviewed fields that differ from the baseline's; the boot set, machine, architecture and layout may not."""
    fixed = [label for label, name, field in FIXED if image[name] != base[field]]
    require(not fixed, f"The candidate's {' and '.join(fixed)} {'differs' if len(fixed) == 1 else 'differ'} from "
            f"generation {generation}'s; a Nightly keeps the boot set, machine, architecture and layout, whose "
            'changes take the supervised path (#268)')
    return {name: dict(baseline=base[field], candidate=image[name])
            for name, field in COMPATIBILITY if image[name] != base[field]}


def check_contents(entries, found, image, source_file):
    """The checks every image must pass, whatever changed; returns the packaged files it compared with sources."""
    def read(path):
        entry = entries.get(path)
        require(entry is not None and entry['type'] == 'file', 'Not packaged: ' + path)
        with open(os.open(found[path], os.O_RDONLY | os.O_NOFOLLOW), 'rb') as stream:
            return stream.read()

    require(json.loads(read('/etc/spacewars/image.json')) == image,
            'The packaged image record is not the qualified one')
    for installed, source, mode in PACKAGED:
        require(read(installed) == source_file(source), 'Packaged file differs from its source: ' + installed)
        require(entries[installed]['mode'] == mode, 'Packaged file has another mode: ' + installed)
    require(DEVICE_RULES <= set(read(SYSTEM_UNIT).decode().splitlines()),
            'The packaged device service lacks its block device rules')
    # A card has no /dev/sdX, so the installed root must mount nothing by device name (#577).
    mounts = [line.split() for line in read('/etc/fstab').decode().splitlines()
              if line.strip() and not line.lstrip().startswith('#')]
    require(mounts and all(not fields[0].startswith(('/dev/sd', '/dev/mmcblk')) for fields in mounts),
            'The packaged fstab names a disk device')
    shipped = [f'{directory}/{name}' for directory in ('/usr/bin', '/usr/sbin') for name in OWNER_HELPERS
               if f'{directory}/{name}' in entries]
    require(not shipped, 'An owner helper is packaged: ' + ', '.join(shipped))
    for key, path in BINARIES:
        entry = entries.get(path)
        require(entry is not None and entry['type'] == 'file'
                and dict(bytes=entry['size'], sha256=entry['sha256']) == image['binaries'][key],
                'Packaged executable differs from the image record: ' + path)
    return [installed for installed, _, _ in PACKAGED]


def refusal(directory, report, differences, command):
    lines = [f"{report['source'][:12]} differs from generation {report['baseline']['generation']} beyond the app:"]
    if report['yocto']:
        lines.append(f"  yocto/: {len(report['yocto'])} changed, " + ', '.join(c['path'] for c in report['yocto'][:5]))
    if report['compatibility']:
        lines.append('  compatibility: ' + ', '.join(sorted(report['compatibility'])))
    if report['root']:
        lines.append(f"  root: {len(report['root'])} entries, " + ', '.join(c['path'] for c in report['root'][:5]))
    lines.append(f"Review {directory / 'differences.json'}, then accept it with: {command} --accept {differences}")
    return '\n'.join(lines)


def summary(directory, report, already=False):
    return dict(prepared=True, already_prepared=already, candidate=str(directory), source=report['source'],
                ci=report['ci'], baseline=report['baseline'], payload=report['descriptor']['compressed']['sha256'],
                accepted=report['accepted'], next=f'ota_release.py nightly {directory}')


def prepare(publisher, code, requested=None, accept=None, generation=None, previous_override=None):
    """Builds, packages and compares one candidate; see this module's description."""
    config = publisher.config
    checkout = config.build['checkout']
    require(not HERE.is_relative_to(checkout.resolve()),
            f'The build checkout {checkout} holds the code that runs, which building would move; give it its own')
    require(accept is None or DIGEST.fullmatch(accept), 'Accept the differences by their full SHA-256')
    config.workspace.mkdir(mode=0o700, parents=True, exist_ok=True)
    lock = os.open(config.workspace / '.candidate.lock', os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    try:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError('Another candidate is being prepared') from None
        progress('Fetching main and its CI runs')
        source, ci = select_source(publisher, checkout, requested)
        entry = baseline(publisher.state, generation)
        base, number = entry['image'], entry['generation']
        require(source != base['source_commit'], f'{source[:12]} is already generation {number}')
        require(ancestor(checkout, base['source_commit'], source),
                f"{source[:12]} does not descend from generation {number}'s source")
        directory = config.workspace / source[:8]
        result, started = directory / 'preparation-result.json', directory / 'code.json'
        pins = encode(dict(code=code, image_candidate=config.tools['image_candidate']))
        if directory.exists():
            require(started.exists() or not any(directory.iterdir()),
                    f'{directory} was not started by ota_release.py candidate; delete it to prepare {source[:12]}')
            require(not started.exists() or started.read_bytes() == pins,
                    f'{directory} was started by other code; delete it to prepare {source[:12]} again')
        if result.exists():
            report = json.loads(result.read_bytes())
            require(report.get('source') == source, f'{directory} holds another candidate')
            compared = report['baseline']['generation']
            if compared == number:
                return summary(directory, report, already=True)
            # nightly takes only a comparison with the newest generation; the build and package are reused.
            progress(f'{directory} was compared with generation {compared}; comparing it with generation {number}')
            result.unlink()
        directory.mkdir(mode=0o700, exist_ok=True)
        if not started.exists():
            save(started, pins)
        move(checkout, source)
        policy = image_policy.from_source(checkout, source)
        raw = encode(policy)
        path = directory / 'policy.json'
        require(not path.exists() or path.read_bytes() == raw,
                f'The derived policy differs from the saved one; delete {directory} to start again')
        save(path, raw)
        record = build(config, source, directory)
        descriptor, qualified = package(publisher, directory, record, policy)
        image = qualified['image']
        require(image['source'] == dict(schema=1, commit=source, dirty=False, revision=source[:12]),
                'Wrong embedded source')
        changed = compatibility(image, base, number)
        progress(f'Comparing with generation {number}')
        bundle, _ = publisher.previous_bundle(previous_override)
        payload = base['compressed']['sha256']
        with tempfile.TemporaryDirectory(prefix='.candidate-', dir=config.workspace) as scratch:
            scratch = Path(scratch)
            roots = {}
            payloads = dict(baseline=(base, bundle / f'{payload}.nightly.{payload}.ext4.zst'),
                            candidate=(descriptor, directory / 'candidate/image.ext4.zst'))
            for name, (named, compressed) in payloads.items():
                work = scratch / name
                work.mkdir()
                ext4 = expand(compressed, named['compressed'], named['expanded'], work / 'root.ext4')
                roots[name] = inspect(ext4, work)
                ext4.unlink()
            entries, found = roots['candidate']
            packaged = check_contents(entries, found, image, lambda name: git(checkout, 'show', f'{source}:{name}'))
            # The boot set holds the kernel, so its modules may not change version either (#587).
            carried, kept = kernels(entries), kernels(roots['baseline'][0])
            require(carried and carried == kept, f"The candidate carries kernel modules for {carried}, but "
                    f"generation {number}'s boot set, which it keeps, runs {kept}")
            root = root_differences(roots['baseline'][0], entries)
            app = sorted(path for path in VARIABLE if roots['baseline'][0].get(path) != entries.get(path))
        report = dict(schema=1, source=source, baseline=dict(generation=number, source_commit=base['source_commit'],
                      payload_sha256=payload), yocto=yocto_changes(checkout, base['source_commit'], source),
                      compatibility=changed, root=root)
        differences = digest(save(directory / 'differences.json', encode(report)))
        differs = bool(report['yocto'] or report['compatibility'] or report['root'])
        command = ' '.join(['ota_release.py candidate', source]
                           + ([f'--baseline {generation}'] if generation is not None else [])
                           + ([f'--previous-bundle {previous_override}'] if previous_override else []))
        require(not differs or accept == differences, refusal(directory, report, differences, command))
        save(directory / 'main-ci-run.json', encode(ci))
        save(directory / 'changes.json', encode(changes(checkout, base['source_commit'], source)))
        save(directory / 'image-verification.json', encode(dict(
            schema=2, passed=True, source=source, image=image, descriptor=descriptor,
            validator=dict(sha256=publisher.config.tools['image_candidate']), baseline=report['baseline'],
            root=dict(entries=len(entries), changed_app_files=app, differences=len(root), kernel_modules=carried),
            packaged=packaged,
            differences_sha256=differences, accepted=differs, signed=False, published=False, installed=False,
            device_accessed=False, scope='Unsigned artifact verification only; does not establish physical '
                                         'recovery or grant installation approval')))
        preparation = dict(
            schema=2, prepared_by=dict(command='ota_release.py candidate', code=code),
            prepared_at=publisher.now().strftime('%Y-%m-%dT%H:%M:%SZ'), source=source, ci=ci['url'], image=image,
            descriptor=descriptor, baseline=report['baseline'], differences_sha256=differences,
            accepted=differences if differs else None,
            artifacts={name: blob(directory / name) for name in ARTIFACTS},
            signed=False, published=False, installed=False, device_accessed=False)
        save(result, encode(preparation), exclusive=True)
        return summary(directory, preparation)
    finally:
        os.close(lock)
