"""Checks of ota_prepare.py: the root inventory and comparison on real ext4 images, and whole preparations.

The build and the image_candidate packaging are replaced here; test_ota_build_inputs.py and
test_ota_candidate.py cover them, and replaying a published generation exercises the real ones.
"""
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import ota_prepare as prepare
import ota_release as release
from ota_delivery import digest

MKE2FS = '/usr/sbin/mke2fs'
URL = 'https://github.com/aortez/space-wars/actions/runs/{}'
BOOT = 'b' * 64
DATA = dict(reads=dict(min=1, max=1), writes=1)
GIT = ('git', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false',
       '-c', 'init.defaultBranch=main')
RULES = prepare.RECIPE + 'spacewars-system.service'
SOURCES = {source: f'{source} fixture\n'.encode() for _, source, _ in prepare.PACKAGED}
SOURCES[RULES] = ('[Service]\n' + '\n'.join(sorted(prepare.DEVICE_RULES)) + '\n').encode()
BLUEZ = 'yocto/meta-spacewars/recipes-connectivity/bluez5/bluez5_%.bbappend'


def blob(raw):
    return dict(bytes=len(raw), sha256=digest(raw))


def run(*args, cwd=None):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True).stdout


def ext4(tree, image, *commands):
    """An ext4 image of tree, then debugfs commands that change it, such as owners, never set on host files."""
    run(MKE2FS, '-q', '-t', 'ext4', '-b', '1024', '-E', 'lazy_itable_init=0,lazy_journal_init=0', '-d', str(tree),
        str(image), '16384')
    if commands:
        subprocess.run([prepare.DEBUGFS, '-w', '-f', '-', str(image)], check=True, capture_output=True,
                       input=''.join(command + '\n' for command in commands).encode())
    return image


def write(tree, files):
    """Lays out files given as content and mode, or as a str naming a link's target."""
    for path, value in files.items():
        target = tree / path.lstrip('/')
        target.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(value, str):
            target.symlink_to(value)
        else:
            target.write_bytes(value[0])
            target.chmod(value[1])
    return tree


def identity(commit, client, manager):
    return dict(schema=1, source=dict(schema=1, commit=commit, dirty=False, revision=commit[:12]),
                build_id='rootfs-' + commit[:8], rootfs_tree=commit[:8], machine='raspberrypi-spacewars',
                architecture='aarch64', layout='spacewars-ab-v1', boot_identity=BOOT, data=DATA, updater_protocol=1,
                recovery_protocol=2, binaries=dict(client=blob(client), os_manager=blob(manager)))


def root(image, client, manager, extra=None):
    """A root with everything check_contents() needs, then extra entries."""
    files = {installed: (SOURCES[source], int(mode, 8)) for installed, source, mode in prepare.PACKAGED}
    files.update({
        '/bin': 'usr/bin',
        '/etc/fstab': (b'# Mounted by label.\nLABEL=data /data ext4 defaults 0 2\n', 0o644),
        '/etc/spacewars/image.json': (json.dumps(image).encode(), 0o644),
        '/usr/bin/engine-client': (client, 0o755),
        '/usr/bin/engine-os-manager': (manager, 0o755),
        '/usr/lib/libfreebl3.chk': (os.urandom(8), 0o755),
        '/usr/lib/modules/6.12.47-v8-16k/modules.dep': (b'kernel/drivers/net/wireless/brcmfmac.ko:\n', 0o644),
        '/usr/libexec/bluetooth/bluetoothd': (b'BlueZ 5.72\n', 0o755),
    })
    files.update(extra or {})
    return files


class Scratch(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)


class RootTests(Scratch):
    def inventory(self, name, files, *commands):
        work = self.base / name
        image = ext4(write(work / 'source', files), work / 'root.ext4', *commands)
        (work / 'inspected').mkdir()
        return prepare.inspect(image, work / 'inspected')[0]

    def test_inventory_records_type_mode_owner_and_content(self):
        entries = self.inventory('one', {'/etc/motd': (b'hello\n', 0o640), '/bin': 'usr/bin',
                                         '/usr/bin/tool': (b'#!/bin/sh\n', 0o4755)}, 'sif /etc/motd uid 0')
        self.assertEqual(entries['/etc/motd'], dict(type='file', mode='0640', uid=0, gid=os.getgid(), size=6,
                                                    sha256=digest(b'hello\n')))
        self.assertEqual(entries['/bin'], dict(type='link', mode='0777', uid=os.getuid(), gid=os.getgid(), size=7,
                                               target='usr/bin'))
        self.assertEqual(entries['/usr/bin/tool']['mode'], '4755')
        self.assertEqual((entries['/']['type'], entries['/lost+found']['type']), ('dir', 'dir'))

    def test_inventory_records_flags_and_extended_attributes(self):
        # stat prints only the length of a value of 40 bytes or more, so the note needs ea_get.
        capability, note = bytes.fromhex('0100000200200000' + '00' * 12), bytes(range(64))
        (self.base / 'capability').write_bytes(capability)
        (self.base / 'note').write_bytes(note)
        entries = self.inventory('one', {'/usr/bin/ping': (b'ping', 0o755), '/usr/bin/plain': (b'plain', 0o755)},
                                 f"ea_set -f {self.base / 'capability'} /usr/bin/ping security.capability",
                                 f"ea_set -f {self.base / 'note'} /usr/bin/ping user.note",
                                 'set_inode_field /usr/bin/plain flags 0x80010')
        self.assertEqual(entries['/usr/bin/ping']['xattrs'],
                         {'security.capability': capability.hex(), 'user.note': note.hex()})
        self.assertNotIn('flags', entries['/usr/bin/ping'])
        self.assertEqual((entries['/usr/bin/plain']['flags'], 'xattrs' in entries['/usr/bin/plain']), ('0x10', False))
        self.assertFalse({'flags', 'xattrs'} & set(entries['/usr/bin']))

    def test_only_app_and_self_check_content_may_change(self):
        files = {'/usr/bin/engine-client': (b'old client', 0o755), '/usr/lib/libfreebl3.chk': (b'one', 0o755),
                 '/etc/fstab': (b'old\n', 0o644), '/bin': 'usr/bin', '/usr/share/gone': (b'x', 0o644)}
        old = self.inventory('old', files)
        allowed = self.inventory('allowed', dict(files, **{'/usr/bin/engine-client': (b'new client!', 0o755),
                                                           '/usr/lib/libfreebl3.chk': (b'two', 0o755)}))
        self.assertEqual(prepare.root_differences(old, allowed), [])
        client = '/usr/bin/engine-client'
        cases = {
            'an app file mode': (dict(files, **{client: (b'new client!', 0o700)}), (), [client]),
            'an app file attribute': (files, (f'ea_set {client} user.note x',), [client]),
            'an app file flag': (files, (f'set_inode_field {client} flags 0x80010',), [client]),
            'other content': (dict(files, **{'/etc/fstab': (b'new\n', 0o644)}), (), ['/etc/fstab']),
            'an owner': (files, ('sif /etc/fstab uid 0',), ['/etc/fstab']),
            'a group': (files, ('sif /etc/fstab gid 0',), ['/etc/fstab']),
            'a directory mode': (files, ('sif /usr/share mode 040700',), ['/usr/share']),
            'a type': (dict(files, **{'/usr/share/gone': 'x'}), (), ['/usr/share/gone']),
            'a link target': (dict(files, **{'/bin': 'usr/sbin'}), (), ['/bin']),
            'an added and a removed file': ({**{k: v for k, v in files.items() if k != '/usr/share/gone'},
                                             '/usr/share/new': (b'x', 0o644)}, (),
                                            ['/usr/share/gone', '/usr/share/new']),
        }
        for number, (case, (changed, commands, paths)) in enumerate(cases.items()):
            with self.subTest(case):
                found = prepare.root_differences(old, self.inventory(f'case{number}', changed, *commands))
                self.assertEqual([difference['path'] for difference in found], paths)

    def test_special_files_are_refused(self):
        tree = write(self.base / 'source', {'/etc/motd': (b'hello\n', 0o644)})
        os.mkfifo(tree / 'etc/pipe')
        image = ext4(tree, self.base / 'root.ext4')
        (self.base / 'inspected').mkdir()
        with self.assertRaisesRegex(ValueError, 'device node, pipe or socket: /etc/pipe'):
            prepare.inspect(image, self.base / 'inspected')


class PreparationTests(Scratch):
    """Whole preparations against a published generation, with the build and packaging replaced."""

    def setUp(self):
        super().setUp()
        self.origin, self.checkout = self.base / 'origin.git', self.base / 'checkout'
        run(*GIT, 'init', '-q', '--bare', str(self.origin))
        run(*GIT, 'clone', '-q', str(self.origin), str(self.checkout))
        self.first = self.commit({**SOURCES, 'yocto/kas-spacewars.yml': b'header: {version: 14}\n',
                                  'crates/app.rs': b'first\n'}, 'Clock: first (#1)')
        self.bundle = self.base / 'bundle'
        self.bundle.mkdir()
        self.state = dict(generation=0, images={})
        self.publish(1, self.first, b'client one', b'manager one')
        self.second = self.commit({'crates/app.rs': b'second\n'}, 'Scorched: second (#2)')
        self.runs = [dict(databaseId=2, headSha=self.second, status='completed', conclusion='success'),
                     dict(databaseId=1, headSha=self.first, status='completed', conclusion='success')]
        self.config = release.Config(dict(
            schema=1, authority=str(self.base / 'authority'), workspace=str(self.base / 'workspace'),
            root_sha256='f' * 64, tools_directory=str(self.base / 'tools'),
            tools={name: 'f' * 64 for name in release.TOOLS}, code={name: 'c' * 64 for name in release.CODE},
            build=dict(cache=str(self.base / 'cache'), checkout=str(self.checkout),
                       directory=str(self.base / 'build'))))
        self.publisher = release.Publisher(self.config, exclusive=False)
        self.publisher.state = self.state
        self.publisher.gh_json = self.gh
        self.bundles = []
        self.publisher.previous_bundle = lambda override=None: (self.bundles.append(override), (self.bundle, b'{}'))[1]
        self.publisher.now = lambda: datetime(2026, 11, 23, 18, tzinfo=timezone.utc)
        self.policy = dict(schema=1, source_commit=self.second, build_inputs_sha256='a' * 64)
        self.candidate_root, self.builds, self.packages = None, 0, 0
        self.candidate(b'client two', b'manager two')

    def commit(self, files, message):
        for path, content in files.items():
            (self.checkout / path).parent.mkdir(parents=True, exist_ok=True)
            (self.checkout / path).write_bytes(content)
        run(*GIT, 'add', '-A', cwd=self.checkout)
        run(*GIT, 'commit', '-q', '-m', message, cwd=self.checkout)
        run(*GIT, 'push', '-q', 'origin', 'HEAD:main', cwd=self.checkout)
        return run('git', 'rev-parse', 'HEAD', cwd=self.checkout).decode().strip()

    def side(self):
        """A commit that never reached main."""
        run(*GIT, 'checkout', '-q', '--detach', cwd=self.checkout)
        (self.checkout / 'crates/app.rs').write_bytes(b'unmerged\n')
        run(*GIT, 'commit', '-q', '-am', 'Unmerged', cwd=self.checkout)
        return run('git', 'rev-parse', 'HEAD', cwd=self.checkout).decode().strip()

    def publish(self, generation, commit, client, manager):
        """Publishes an image of commit as a Nightly generation: its payload in the bundle, its entry in the journal."""
        named = identity(commit, client, manager)
        work = self.base / f'generation{generation}'
        image = ext4(write(work / 'tree', root(named, client, manager)), work / 'root.ext4')
        payload = work / 'root.ext4.zst'
        run('zstd', '-q', '-o', str(payload), str(image))
        compressed = prepare.blob(payload)
        os.link(payload, self.bundle / f"{compressed['sha256']}.nightly.{compressed['sha256']}.ext4.zst")
        published = dict(schema=1, source_commit=commit, build_id=named['build_id'], machine=named['machine'],
                         architecture='aarch64', layout='spacewars-ab-v1', format='rootfs-ext4-zstd',
                         compressed=compressed, expanded=prepare.blob(image), boot_identity=BOOT,
                         min_updater_protocol=1, min_recovery_protocol=2, data=DATA)
        self.state['generation'] = generation
        self.state['images'][compressed['sha256']] = dict(
            generation=generation, image=published, evidence_sha256='e' * 64, nightly=dict(at='2026-11-22T20:57:53Z'),
            release=None)

    def candidate(self, client, manager, extra=None, **changed):
        """Sets up the root the next packaging packs: from self.second, with extra entries and changed identity."""
        self.candidate_identity = dict(identity(self.second, client, manager), **changed)
        self.candidate_root = root(self.candidate_identity, client, manager, extra)

    def discard(self):
        """Removes the packaged candidate, so the next preparation packages the root set up last."""
        shutil.rmtree(self.config.workspace / self.second[:8] / 'candidate')

    def gh(self, args):
        if args[:2] == ['run', 'list']:
            commit = args[args.index('--commit') + 1] if '--commit' in args else None
            return [dict(run, url=URL.format(run['databaseId'])) for run in self.runs
                    if commit in (None, run['headSha'])]
        chosen = next(run for run in self.runs if str(run['databaseId']) == args[2])
        return dict(chosen, event='push', headBranch='main', url=URL.format(chosen['databaseId']), workflowName='CI')

    def build(self, config, source, directory):
        if not (directory / 'build.json').exists():
            (directory / 'build.json').write_text(json.dumps(dict(source_commit=source, output=dict(name='x.gz'))))
            (directory / 'build.log').write_text('fixture build\n')
            self.builds += 1
        return json.loads((directory / 'build.json').read_bytes())

    def package(self, publisher, directory, record, policy):
        candidate = directory / 'candidate'
        if not candidate.exists():
            work = self.base / f'packaged{self.packages}'
            image = ext4(write(work / 'tree', self.candidate_root), work / 'root.ext4')
            candidate.mkdir()
            run('zstd', '-q', '-o', str(candidate / 'image.ext4.zst'), str(image))
            descriptor = dict(schema=1, format='rootfs-ext4-zstd', expanded=prepare.blob(image),
                              compressed=prepare.blob(candidate / 'image.ext4.zst'))
            qualified = dict(schema=1, image=self.candidate_identity, expanded=descriptor['expanded'],
                             build_inputs_sha256=policy['build_inputs_sha256'])
            (candidate / 'candidate.json').write_bytes(prepare.encode(descriptor))
            (candidate / 'qualification.json').write_bytes(prepare.encode(qualified))
            (candidate / 'build.json').write_bytes((directory / 'build.json').read_bytes())
            self.packages += 1
        return tuple(json.loads((candidate / name).read_bytes()) for name in ('candidate.json', 'qualification.json'))

    def prepare(self, code=None, **arguments):
        with patch.object(prepare, 'build', self.build), patch.object(prepare, 'package', self.package), \
                patch.object(prepare.image_policy, 'from_source', lambda checkout, source: self.policy), \
                patch.object(prepare, 'progress', lambda message: None):
            return prepare.prepare(self.publisher, code or self.config.code, **arguments)

    def planned(self, directory):
        """What ota_release.py nightly selects from the directory, with the verifier replaced."""
        evidence = (directory / 'candidate/qualification.json').read_bytes()
        planner = release.Publisher(self.config)
        planner.state, planner.gh_json = self.state, self.gh
        planner.run = lambda name, *args, timeout=120: evidence
        return planner.select_nightly(directory)

    def test_a_candidate_that_changes_only_the_app_is_prepared_for_nightly(self):
        result = self.prepare()
        directory = Path(result['candidate'])
        self.assertEqual((result['source'], result['already_prepared'], result['accepted']), (self.second, False, None))
        self.assertEqual(result['next'], f'ota_release.py nightly {directory}')
        differences = json.loads((directory / 'differences.json').read_bytes())
        self.assertEqual((differences['yocto'], differences['compatibility'], differences['root']), ([], {}, []))
        verification = json.loads((directory / 'image-verification.json').read_bytes())
        self.assertEqual(verification['root']['changed_app_files'],
                         ['/etc/spacewars/image.json', '/usr/bin/engine-client', '/usr/bin/engine-os-manager',
                          '/usr/lib/libfreebl3.chk'])
        changes = json.loads((directory / 'changes.json').read_bytes())
        self.assertEqual([(c['subject'], c['pull_request'], c['areas']) for c in changes],
                         [('Scorched: second (#2)', 2, ['crates/app.rs'])])
        intent, selection, payload = self.planned(directory)
        self.assertEqual((intent['image']['source_commit'], selection['ci']['workflowName']), (self.second, 'CI'))
        self.assertEqual(payload, directory / 'candidate/image.ext4.zst')
        again = self.prepare()
        self.assertEqual((again['already_prepared'], again['payload'], self.builds), (True, result['payload'], 1))

    def test_differences_beyond_the_app_wait_for_the_owners_acceptance(self):
        self.second = self.commit({BLUEZ: b'SRC_URI += "file://fix.patch"\n', 'crates/app.rs': b'third\n'},
                                  'BlueZ: a fix (#3)')
        self.runs.insert(0, dict(databaseId=3, headSha=self.second, status='completed', conclusion='success'))
        self.candidate(b'client two', b'manager two', {'/usr/libexec/bluetooth/bluetoothd': (b'BlueZ 5.72+\n', 0o755)})
        given = self.base / 'given-bundle'
        with self.assertRaises(ValueError) as refused:
            self.prepare(previous_override=given)
        directory = self.config.workspace / self.second[:8]
        accepted = digest((directory / 'differences.json').read_bytes())
        message = str(refused.exception)
        self.assertIn(f'yocto/: 1 changed, {BLUEZ}', message)
        self.assertIn('root: 1 entries, /usr/libexec/bluetooth/bluetoothd', message)
        # The command it prints builds the same source even after main moves on, from the bundle it was given.
        self.assertIn(f'accept it with: ota_release.py candidate {self.second} --previous-bundle {given} '
                      f'--accept {accepted}', message)
        self.assertEqual(self.bundles, [given])
        with self.assertRaisesRegex(ValueError, '--accept ' + accepted):
            self.prepare(accept='0' * 64)
        self.assertFalse((directory / 'preparation-result.json').exists())
        result = self.prepare(requested=self.second, accept=accepted)
        self.assertEqual((result['accepted'], self.builds, self.packages), (accepted, 1, 1))
        report = json.loads((directory / 'differences.json').read_bytes())
        self.assertEqual([change['path'] for change in report['yocto']], [BLUEZ])
        self.planned(directory)

    def test_a_result_compared_with_an_older_generation_is_compared_again(self):
        middle = self.second
        self.second = self.commit({'crates/app.rs': b'third\n'}, 'Scorched: third (#3)')
        self.runs.insert(0, dict(databaseId=3, headSha=self.second, status='completed', conclusion='success'))
        self.candidate(b'client three', b'manager three')
        directory = Path(self.prepare()['candidate'])
        self.publish(2, middle, b'client two', b'manager two')
        with self.assertRaisesRegex(ValueError, 'compared with generation 1, but the newest published generation is 2'):
            self.planned(directory)
        again = self.prepare()
        self.assertEqual((again['already_prepared'], again['baseline']['generation'], self.builds, self.packages),
                         (False, 2, 1, 1))
        changes = json.loads((directory / 'changes.json').read_bytes())
        self.assertEqual([change['commit'] for change in changes], [self.second])
        self.planned(directory)

    def test_only_the_code_that_started_a_directory_resumes_it(self):
        directory = Path(self.prepare()['candidate'])
        self.assertEqual(json.loads((directory / 'code.json').read_bytes()),
                         dict(code=self.config.code, image_candidate='f' * 64))
        self.assertIn('code.json', json.loads((directory / 'preparation-result.json').read_bytes())['artifacts'])
        with self.assertRaisesRegex(ValueError, 'was started by other code; delete it to prepare'):
            self.prepare(code=dict(self.config.code, **{'ota_prepare.py': 'd' * 64}))
        shutil.rmtree(directory)
        directory.mkdir()
        (directory / 'preparation-result.json').write_text(json.dumps(dict(schema=1, source=self.second)))
        with self.assertRaisesRegex(ValueError, 'was not started by ota_release.py candidate; delete it'):
            self.prepare()

    def test_the_build_checkout_holds_only_the_commit_and_what_builds_leave(self):
        (self.checkout / '.git/info/exclude').write_text('__pycache__/\n*.bbappend\n')
        cache = self.checkout / 'yocto/meta-spacewars/lib/spacewars/__pycache__'
        cache.mkdir(parents=True)
        (cache / 'layer.cpython-312.pyc').write_bytes(b'')
        stray = self.checkout / 'yocto/meta-spacewars/recipes-local/local.bbappend'
        stray.parent.mkdir(parents=True)
        stray.write_bytes(b'SRC_URI += "file://local.patch"\n')
        with self.assertRaisesRegex(ValueError, 'has local changes or ignored files: '
                                                'yocto/meta-spacewars/recipes-local/local.bbappend'):
            self.prepare()
        stray.unlink()
        self.assertFalse(self.prepare()['already_prepared'])
        self.config.build['checkout'] = prepare.HERE.parents[1]
        with self.assertRaisesRegex(ValueError, 'holds the code that runs'):
            self.prepare()

    def test_a_new_boot_set_layout_or_kernel_is_refused_even_when_accepted(self):
        for message, changed in (("boot set differs from generation 1's", dict(boot_identity='d' * 64)),
                                 ("layout differs from generation 1's", dict(layout='spacewars-ab-v2'))):
            with self.subTest(message):
                self.candidate(b'client two', b'manager two', **changed)
                with self.assertRaisesRegex(ValueError, message):
                    self.prepare(accept='0' * 64)
                self.discard()
        self.candidate(b'client two', b'manager two', {'/usr/lib/modules/6.13.0-v8-16k/modules.dep': (b'', 0o644)})
        with self.assertRaisesRegex(ValueError, r"modules for \['6.12.47-v8-16k', '6.13.0-v8-16k'\], but "
                                                r"generation 1's boot set, which it keeps, runs \['6.12.47-v8-16k'\]"):
            self.prepare(accept='0' * 64)

    def test_every_image_must_pass_the_content_checks(self):
        unit, client = '/usr/lib/systemd/system/spacewars-kiosk.service', '/usr/bin/engine-client'
        cases = {
            'The packaged image record is not the qualified one': {'/etc/spacewars/image.json': (b'{}', 0o644)},
            'Packaged file differs from its source: ' + unit: {unit: (b'edited\n', 0o644)},
            'Packaged file has another mode: ' + unit: {unit: (SOURCES[prepare.RECIPE + 'spacewars-kiosk.service'],
                                                               0o600)},
            'fstab names a disk device': {'/etc/fstab': (b'/dev/sda1 /data ext4 defaults 0 2\n', 0o644)},
            'An owner helper is packaged: /usr/bin/reset_trial': {'/usr/bin/reset_trial': (b'x', 0o755)},
            'Packaged executable differs from the image record: ' + client: {client: (b'another client', 0o755)},
        }
        for message, extra in cases.items():
            with self.subTest(message):
                self.candidate(b'client two', b'manager two', extra)
                with self.assertRaisesRegex(ValueError, message):
                    self.prepare()
                self.discard()
        self.second = self.commit({RULES: b'[Service]\n'}, 'Device service: drop its rules (#4)')
        self.runs.insert(0, dict(databaseId=4, headSha=self.second, status='completed', conclusion='success'))
        self.candidate(b'client two', b'manager two', {prepare.SYSTEM_UNIT: (b'[Service]\n', 0o644)})
        with self.assertRaisesRegex(ValueError, 'lacks its block device rules'):
            self.prepare()

    def test_the_newest_commit_on_main_that_passed_ci_is_built(self):
        newest = self.commit({'crates/app.rs': b'third\n'}, 'Scorched: third (#3)')
        side = self.side()
        self.runs[:0] = [dict(databaseId=5, headSha=side, status='completed', conclusion='success'),
                         dict(databaseId=4, headSha='e' * 40, status='completed', conclusion='success'),
                         dict(databaseId=3, headSha=newest, status='completed', conclusion='failure')]
        self.assertEqual(self.prepare()['source'], self.second)

    def test_the_source_must_be_on_main_with_passing_ci(self):
        self.runs[0]['status'], self.runs[0]['conclusion'] = 'in_progress', None
        with self.assertRaisesRegex(ValueError, 'CI is still running for ' + self.second):
            self.prepare(requested=self.second[:10])
        self.runs[0]['status'], self.runs[0]['conclusion'] = 'completed', 'success'
        side = self.side()
        for requested, message in ((side, 'is not on main'), ('main', 'by its commit hash'),
                                   ('0' * 40, 'not a commit the build checkout has fetched')):
            with self.subTest(requested), self.assertRaisesRegex(ValueError, message):
                self.prepare(requested=requested)
        published = next(iter(self.state['images'].values()))['image']
        published['source_commit'] = side
        with self.assertRaisesRegex(ValueError, f"{self.second[:12]} does not descend from generation 1's source"):
            self.prepare(requested=self.second)
        published['source_commit'] = self.first
        with self.assertRaisesRegex(ValueError, f'{self.first[:12]} is already generation 1'):
            self.prepare(requested=self.first)
        self.runs = []
        with self.assertRaisesRegex(ValueError, 'No recent commit on main has passed CI'):
            self.prepare()


if __name__ == '__main__':
    unittest.main()
