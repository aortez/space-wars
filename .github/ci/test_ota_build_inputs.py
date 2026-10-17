import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import ota_build_inputs as helper


def git(path, *args):
    return subprocess.check_output(['git', '-C', str(path), *args], text=True,
                                   stderr=subprocess.DEVNULL).strip()


def commit(path):
    git(path, 'add', '.')
    git(path, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
        '-c', 'commit.gpgsign=false', 'commit', '-m', 'fixture')
    return git(path, 'rev-parse', 'HEAD')


class InputsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        git(self.root, 'init', '-b', 'fixture')
        for name in helper.CONFIGURATION_FILES:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('committed configuration\n')
        (self.root / '.gitignore').write_text(''.join(f'/yocto/{name}/\n' for name in helper.REPOSITORIES))
        self.source = commit(self.root)
        self.configuration = dict(machine=helper.MACHINE, target='spacewars-image', distro='poky',
                                  repos={'meta-spacewars': {'path': 'meta-spacewars'}})
        for name, url in helper.REPOSITORIES.items():
            layer = self.root / 'yocto' / name
            layer.mkdir()
            git(layer, 'init', '-b', 'fixture')
            git(layer, 'remote', 'add', 'origin', url)
            (layer / 'recipe').write_text('pinned recipe\n')
            self.configuration['repos'][name] = dict(url=url, commit=commit(layer), branch='fixture')

    def snapshot(self):
        return helper.snapshot(self.root, self.source, self.configuration)

    def test_snapshot_binds_clean_source_all_layers_and_configuration_bytes(self):
        result = self.snapshot()
        self.assertEqual(result, self.snapshot())
        self.assertEqual(result['source_commit'], self.source)
        self.assertEqual(set(result['layers']), set(helper.REPOSITORIES))
        self.assertEqual(result['configuration']['Cargo.lock'], helper.blob(self.root / 'Cargo.lock'))
        self.assertEqual(result['local_layer']['source_commit'], self.source)

    def test_dirty_or_untracked_source_and_layers_fail_without_resetting_them(self):
        for path in (self.root / 'Cargo.lock', self.root / 'new-source',
                     self.root / 'yocto/poky/recipe', self.root / 'yocto/poky/new-recipe'):
            with self.subTest(path=path):
                original = path.read_bytes() if path.exists() else None
                path.write_text('edited\n')
                with self.assertRaises(ValueError):
                    self.snapshot()
                self.assertEqual(path.read_text(), 'edited\n')
                if original is None:
                    path.unlink()
                else:
                    path.write_bytes(original)

    def test_wrong_source_pin_or_origin_rejects(self):
        with self.assertRaises(ValueError):
            helper.snapshot(self.root, 'f' * 40, self.configuration)
        pin = self.configuration['repos']['poky']['commit']
        self.configuration['repos']['poky']['commit'] = 'f' * 40
        with self.assertRaises(ValueError):
            self.snapshot()
        self.configuration['repos']['poky']['commit'] = pin
        git(self.root / 'yocto/poky', 'remote', 'set-url', 'origin', 'https://example.invalid/other')
        with self.assertRaises(ValueError):
            self.snapshot()

    def test_unpinned_added_missing_or_overridden_layers_are_rejected(self):
        original = copy.deepcopy(self.configuration)
        for edit in (lambda c: c['repos']['poky'].pop('commit'),
                     lambda c: c['repos']['poky'].update(commit='main'),
                     lambda c: c['repos']['poky'].update(patches={'other': {}}),
                     lambda c: c['repos']['poky'].update(path='../other'),
                     lambda c: c['repos'].update(extra={}),
                     lambda c: c['repos'].pop('meta-pi-base'),
                     lambda c: c.update(machine='other'),
                     lambda c: c.update(target='other')):
            self.configuration = copy.deepcopy(original)
            edit(self.configuration)
            with self.assertRaises(ValueError):
                self.snapshot()

    def test_changed_layer_commit_is_observed_without_checkout(self):
        layer = self.root / 'yocto/poky'
        (layer / 'recipe').write_text('different committed recipe\n')
        changed = commit(layer)
        with self.assertRaises(ValueError):
            self.snapshot()
        self.assertEqual(git(layer, 'rev-parse', 'HEAD'), changed)

    def test_build_environment_drops_credentials_and_implicit_overrides(self):
        with patch.dict(os.environ, {'PATH': '/tools', 'HOME': '/home/builder', 'GH_TOKEN': 'secret',
                                     'SSH_AUTH_SOCK': '/agent', 'KAS_MACHINE': 'wrong',
                                     'BB_ENV_PASSTHROUGH_ADDITIONS': 'SECRET', 'SPACEWARS_GPU': '0'}, clear=True):
            result = helper.environment(self.root, self.root / 'build', self.root / 'cache')
        self.assertNotIn('GH_TOKEN', result)
        self.assertNotIn('SSH_AUTH_SOCK', result)
        self.assertNotIn('KAS_MACHINE', result)
        self.assertNotIn('BB_ENV_PASSTHROUGH_ADDITIONS', result)
        self.assertEqual(result['KAS_WORK_DIR'], str(self.root / 'yocto'))
        self.assertEqual(result['SPACEWARS_GPU'], '1')
        self.assertEqual(result['SPACEWARS_CACHE_ROOT'], str(self.root / 'cache'))

    def test_extra_build_configuration_and_linked_inputs_reject(self):
        directory = self.root / 'build'
        conf = directory / 'conf'
        conf.mkdir(parents=True)
        helper.check_build_directory(directory)
        extra = conf / 'auto.conf'
        extra.write_text('unreviewed overrides')
        with self.assertRaises(ValueError):
            helper.check_build_directory(directory)
        extra.unlink()
        (conf / 'local.conf').symlink_to(self.root / 'Cargo.lock')
        with self.assertRaises(ValueError):
            helper.check_build_directory(directory)
        with self.assertRaises(ValueError):
            helper.blob(conf / 'local.conf')
        with self.assertRaises(ValueError):
            helper.blob(self.root / 'Cargo.lock', maximum=1)

    def run_builder(self, driver, during_build):
        expected = self.snapshot()
        original_command = helper.command
        original_run = subprocess.run
        with tempfile.TemporaryDirectory() as temporary:
            build = Path(temporary)
            output = build / 'build.json'
            def command(args, **kwargs):
                if args == ['kas', '--version']:
                    return 'kas 5.4 (fixture)'
                if args == ['kas', 'dump', '--format', 'json', helper.CONFIG]:
                    return json.dumps(self.configuration)
                return original_command(args, **kwargs)
            def run(args, **kwargs):
                if args != driver:
                    return original_run(args, **kwargs)
                env = kwargs['env']
                selected = Path(env['SPACEWARS_OTA_BUILD_INPUTS'])
                raw = selected.read_bytes()
                self.assertEqual(json.loads(raw), expected)
                self.assertEqual(hashlib.sha256(raw).hexdigest(), env['SPACEWARS_OTA_BUILD_INPUTS_SHA256'])
                self.assertNotIn('GH_TOKEN', env)
                self.assertEqual(kwargs['cwd'], self.root / 'yocto')
                during_build()
                conf = build / 'conf'
                conf.mkdir()
                for name in ('local.conf', 'bblayers.conf'):
                    (conf / name).write_text('generated configuration')
                deploy = build / 'tmp/deploy/images' / helper.MACHINE
                deploy.mkdir(parents=True)
                image = deploy / f'spacewars-image-{helper.MACHINE}.rootfs-fixture.ext4.gz'
                image.write_bytes(b'fixture image (not qualified by this build record)')
                image.with_name(f'spacewars-image-{helper.MACHINE}.rootfs.ext4.gz').symlink_to(image.name)
                Path(str(image) + '.image.json').write_text(json.dumps(dict(source=dict(
                    schema=1, commit=self.source, dirty=False, revision=self.source[:12]))))
                return subprocess.CompletedProcess(args, 0)
            try:
                with patch.object(helper, 'command', side_effect=command), patch.object(subprocess, 'run', side_effect=run):
                    helper.build(self.root, self.source, build, output, build / 'cache', driver[0] == 'kas')
                result = json.loads(output.read_text())
                raw = json.dumps(expected, sort_keys=True, separators=(',', ':')).encode()
                self.assertEqual(result['embedded_inputs'], dict(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest()))
            finally:
                self.assertEqual(list(build.glob('ota-build-inputs-*.json')), [])

    def test_builder_passes_exact_inputs_to_local_and_container_drivers(self):
        for driver in (['node', 'scripts/build.mjs', '--gpu'], ['kas', 'build', helper.CONFIG]):
            with self.subTest(driver=driver):
                self.run_builder(driver, lambda: None)

    def test_build_failure_or_source_change_leaves_no_success_record(self):
        def failure():
            raise subprocess.CalledProcessError(1, 'fixture kas build')
        with self.assertRaises(subprocess.CalledProcessError):
            self.run_builder(['kas', 'build', helper.CONFIG], failure)
        with self.assertRaises(ValueError):
            self.run_builder(['kas', 'build', helper.CONFIG], lambda: (self.root / 'Cargo.lock').write_text('changed during build'))


if __name__ == '__main__':
    unittest.main()
