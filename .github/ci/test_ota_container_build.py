"""Exercise source isolation and build-result collection without a Docker daemon."""
import copy
import gzip
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import ota_container_build as helper
from ota_candidate import canonical
from ota_delivery import digest

CONTAINER = 'd' * 64


def blob(raw):
    return dict(bytes=len(raw), sha256=digest(raw))


class ContainerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.output = self.directory / 'result'
        self.gz = gzip.compress(b'fixture image')
        self.inputs = dict(schema=1, source_commit='a' * 40, kas_version='5.4',
                           machine='raspberrypi-spacewars', target='spacewars-image',
                           layers={}, local_layer={}, configuration={}, expanded_kas_sha256='b' * 64)
        inputs = canonical(self.inputs)
        self.policy = dict(schema=1, source_commit='a' * 40, build_inputs_sha256=digest(inputs))
        self.record = self.inputs | dict(embedded_inputs=blob(inputs), generated_configuration={
            'local.conf': blob(b'configuration'), 'bblayers.conf': blob(b'layers')}, output=dict(
                name='spacewars-image-raspberrypi-spacewars.rootfs-20000101000000.ext4.gz',
                blob=blob(self.gz), image_identity=dict(source=dict(
                    schema=1, commit='a' * 40, dirty=False, revision='a' * 12))))
        self.state = dict(Status='exited', ExitCode=0, OOMKilled=False)
        self.configuration = dict(Mounts=[], Config=dict(User='builder'),
                                  HostConfig=dict(Privileged=False))
        self.calls = []
        self.start_error = None
        self.commands = SimpleNamespace(query=self.query, run=self.run_command)

    def query(self, args, **kwargs):
        self.calls.append((args, kwargs))
        if args[:2] == ['docker', 'create']:
            return CONTAINER
        if args == ['docker', 'inspect', CONTAINER]:
            return json.dumps([self.configuration])
        if args == ['docker', 'inspect', CONTAINER, '--format', '{{json .State}}']:
            return json.dumps(self.state)
        self.fail('Unexpected command: ' + repr(args))

    def run_command(self, args, **kwargs):
        self.calls.append((args, kwargs))
        if args[:2] == ['docker', 'cp']:
            if args[2] == CONTAINER + ':/builder/out/build-inputs.json':
                Path(args[3]).write_bytes(canonical(self.record))
            elif args[2] == CONTAINER + ':' + helper.DEPLOY + self.record['output']['name']:
                Path(args[3]).write_bytes(self.gz)
            else:
                self.assertEqual(args[3], CONTAINER + ':/tmp/ota-source.tar.gz')
        if args[:2] == ['docker', 'start'] and self.start_error:
            raise self.start_error

    def collect(self):
        return helper.collect(CONTAINER, self.policy, self.output, self.commands)

    def test_only_successful_exact_output_is_collected(self):
        self.assertEqual(self.collect(), self.record)
        self.assertEqual(set(p.name for p in self.output.iterdir()),
                         {'build.json', 'rootfs.ext4.gz', 'policy.json'})
        self.assertEqual((self.output / 'rootfs.ext4.gz').read_bytes(), self.gz)
        self.assertEqual(json.loads((self.output / 'policy.json').read_bytes()), self.policy)
        with self.assertRaises(ValueError):
            self.collect()
        self.assertEqual(list(self.directory.glob('.ota-build-result-*')), [])

    def test_failed_running_oom_or_wrong_container_cannot_be_collected(self):
        for state in (dict(Status='running', ExitCode=0, OOMKilled=False),
                      dict(Status='exited', ExitCode=1, OOMKilled=False),
                      dict(Status='exited', ExitCode=0, OOMKilled=True)):
            self.state = state
            self.calls.clear()
            with self.assertRaises(ValueError):
                self.collect()
            self.assertEqual(len(self.calls), 1)
            self.assertFalse(self.output.exists())
        for container in ('user-container', CONTAINER + ':/other', 'a' * 12):
            self.calls.clear()
            with self.assertRaises(ValueError):
                helper.collect(container, self.policy, self.output, self.commands)
            self.assertEqual(self.calls, [])

    def test_wrong_source_or_untrusted_image_path_never_reaches_image_copy(self):
        original = copy.deepcopy(self.record)
        for change in (lambda r: r.update(source_commit='b' * 40),
                       lambda r: r['output'].update(name='../../../secret'),
                       lambda r: r.update(layers={'unexpected': {}})):
            self.record = copy.deepcopy(original)
            change(self.record)
            self.calls.clear()
            with self.assertRaises(ValueError):
                self.collect()
            self.assertEqual(len(self.calls), 2)
            self.assertFalse(self.output.exists())
        self.record = original
        self.gz += b'changed image'
        with self.assertRaises(ValueError):
            self.collect()
        self.assertFalse(self.output.exists())
        self.assertEqual(list(self.directory.glob('.ota-build-result-*')), [])

    def build(self):
        with patch.object(helper, 'from_source', return_value=self.policy), \
                patch.object(helper, 'source_archive'), \
                patch.object(helper, 'Commands', return_value=self.commands), \
                patch.object(helper.os, 'getuid', return_value=1000), \
                patch.object(helper.os, 'sched_getaffinity', return_value={2, 4, 6, 8, 10}):
            return helper.build(self.directory, 'a' * 40, self.output)

    def test_build_uses_no_mounts_credentials_or_privilege_and_removes_own_container(self):
        self.assertEqual(self.build(), self.record)
        create = next(args for args, _ in self.calls if args[:2] == ['docker', 'create'])
        self.assertIn(helper.KAS_IMAGE, create)
        self.assertEqual(create[create.index('--cpuset-cpus') + 1], '2,4,6,8')
        self.assertEqual(create[create.index('--memory') + 1], '12g')
        self.assertEqual(create[create.index('--memory-swap') + 1], '12g')
        self.assertIn('no-new-privileges', create)
        self.assertEqual(create[create.index('--cap-drop') + 1], 'ALL')
        self.assertFalse(set(create) & {'--mount', '--volume', '-v', '--privileged', '--env', '-e'})
        start = next(kwargs for args, kwargs in self.calls if args[:2] == ['docker', 'start'])
        self.assertEqual(start['timeout'], 330 * 60)
        self.assertEqual(self.calls[-1][0], ['docker', 'rm', '--force', CONTAINER])

    def test_timeout_or_unexpected_isolation_cleans_up_without_output(self):
        self.start_error = subprocess.TimeoutExpired('docker start', 1)
        with self.assertRaises(subprocess.TimeoutExpired):
            self.build()
        self.assertEqual(self.calls[-1][0], ['docker', 'rm', '--force', CONTAINER])
        self.assertFalse(self.output.exists())
        for change in (lambda c: c.update(Mounts=[{'Source': '/host'}]),
                       lambda c: c['Config'].update(User='root'),
                       lambda c: c['HostConfig'].update(Privileged=True)):
            self.configuration = dict(Mounts=[], Config=dict(User='builder'), HostConfig=dict(Privileged=False))
            change(self.configuration)
            self.calls.clear()
            with self.assertRaises(ValueError):
                self.build()
            self.assertFalse(any(args[:2] == ['docker', 'start'] for args, _ in self.calls))
            self.assertEqual(self.calls[-1][0], ['docker', 'rm', '--force', CONTAINER])

    def test_private_command_environment_excludes_caller_secrets(self):
        with patch.dict(os.environ, {'GH_TOKEN': 'secret', 'SSH_AUTH_SOCK': '/secret',
                                    'DOCKER_HOST': 'tcp://untrusted', 'AWS_SECRET_ACCESS_KEY': 'secret'}):
            commands = helper.Commands(self.directory / 'private-home')
        self.assertFalse(set(commands.env) & {'GH_TOKEN', 'SSH_AUTH_SOCK', 'DOCKER_HOST', 'AWS_SECRET_ACCESS_KEY'})
        self.assertEqual(commands.env['GIT_CONFIG_GLOBAL'], '/dev/null')
        self.assertEqual(Path(commands.env['HOME']), self.directory / 'private-home')


class SourceArchiveTests(unittest.TestCase):
    def test_detached_selected_source_excludes_ignored_files_and_host_git_settings(self):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            repo = work / 'repository'
            commands = helper.Commands(work / 'private-home')
            commands.query(['git', '-c', 'init.templateDir=', 'init', str(repo)])
            (repo / 'selected.txt').write_text('exact source\n')
            (repo / '.gitignore').write_text('ignored-secret\n')
            commands.query(['git', '-C', str(repo), 'add', '.'])
            commands.query(['git', '-C', str(repo), '-c', 'user.name=Fixture',
                            '-c', 'user.email=fixture@example.invalid', 'commit', '-m', 'fixture'])
            source = commands.query(['git', '-C', str(repo), 'rev-parse', 'HEAD'])
            commands.query(['git', '-C', str(repo), 'checkout', '--detach', source])
            commands.query(['git', '-C', str(repo), 'config', 'credential.helper', 'caller-secret'])
            (repo / 'ignored-secret').write_text('must not enter builder')
            (repo / '.git/hooks').mkdir(exist_ok=True)
            (repo / '.git/hooks/host-hook').write_text('host private hook')
            archive_path = work / 'source.tar.gz'
            helper.source_archive(repo, source, archive_path, commands)
            with tarfile.open(archive_path, 'r:gz') as archive:
                names = set(archive.getnames())
                self.assertIn('./selected.txt', names)
                self.assertNotIn('./ignored-secret', names)
                self.assertNotIn('./.git/hooks/host-hook', names)
                self.assertNotIn(b'caller-secret', archive.extractfile('./.git/config').read())
            self.assertEqual(helper.clean_repository(work / 'source', source), source)
            (repo / 'selected.txt').write_text('changed source')
            with self.assertRaises(ValueError):
                helper.source_archive(repo, source, work / 'dirty.tar.gz', commands)
            self.assertFalse((work / 'dirty.tar.gz').exists())


if __name__ == '__main__':
    unittest.main()
