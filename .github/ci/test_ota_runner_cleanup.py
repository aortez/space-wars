"""Cleanup guards and resource checks; never remove a real SDK during tests."""
import contextlib
import io
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import ota_runner_cleanup as helper


class CleanupTests(unittest.TestCase):
    def setUp(self):
        self.env = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='github-hosted',
                        RUNNER_OS='Linux', GITHUB_REPOSITORY='aortez/space-wars',
                        GITHUB_REF='refs/heads/main', GITHUB_EVENT_NAME='schedule')

    def test_workstation_self_hosted_fork_branch_and_pr_stop_before_any_command(self):
        for field, value in (('GITHUB_ACTIONS', 'false'), ('RUNNER_ENVIRONMENT', 'self-hosted'),
                             ('RUNNER_OS', 'Windows'), ('GITHUB_REPOSITORY', 'fork/space-wars'),
                             ('GITHUB_REF', 'refs/heads/unreviewed'), ('GITHUB_EVENT_NAME', 'pull_request')):
            with patch.object(helper.subprocess, 'run') as run, \
                    patch.object(helper.subprocess, 'check_output') as query:
                with self.assertRaises(ValueError):
                    helper.cleanup(self.env | {field: value})
                run.assert_not_called()
                query.assert_not_called()

    def invoke(self, code=0, free_bytes=70 * helper.GIB, free_inodes=1_000_000):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'docker').mkdir()
            paths = (root, root / 'docker')
            before = {str(p): dict(free_bytes=14 * helper.GIB, free_inodes=free_inodes) for p in paths}
            after = {str(p): dict(free_bytes=free_bytes, free_inodes=free_inodes) for p in paths}
            with patch.object(helper.subprocess, 'run', return_value=subprocess.CompletedProcess([], code)) as run, \
                    patch.object(helper.subprocess, 'check_output', return_value=str(root / 'docker')), \
                    patch.object(helper, 'filesystem_space', side_effect=[before, after]), \
                    contextlib.redirect_stdout(io.StringIO()):
                try:
                    return helper.cleanup(self.env | {'RUNNER_TEMP': str(root)})
                finally:
                    run.assert_called_once_with(helper.CLEANUP, check=False, timeout=75)

    def test_only_fixed_sdk_paths_with_root_timeout_and_no_broad_cleanup(self):
        command = helper.CLEANUP
        self.assertEqual(command[:6], ('sudo', '-n', 'timeout', '--signal=TERM', '--kill-after=5s', '60s'))
        self.assertIn('--one-file-system', command)
        self.assertIn('--preserve-root=all', command)
        self.assertEqual(command[command.index('--') + 1:], helper.SDK_PATHS)
        self.assertFalse(set(command) & {'apt', 'apt-get', 'docker', 'prune', 'swapoff'})
        for path in helper.SDK_PATHS:
            self.assertTrue(path.startswith(('/usr/', '/opt/')))
            self.assertNotIn(path, ('/usr', '/opt', '/opt/hostedtoolcache'))
        report = self.invoke()
        self.assertEqual(report['cleanup_exit'], 0)

    def test_deadline_is_allowed_only_if_enough_space_and_inodes_remain(self):
        for code in (124, 137):
            self.assertEqual(self.invoke(code=code)['cleanup_exit'], code)
            with self.assertRaisesRegex(ValueError, 'refusing to start'):
                self.invoke(code=code, free_bytes=helper.MINIMUM_FREE - 1)
        with self.assertRaisesRegex(ValueError, 'refusing to start'):
            self.invoke(free_inodes=499_999)
        with self.assertRaisesRegex(ValueError, 'cleanup failed'):
            self.invoke(code=1)


if __name__ == '__main__':
    unittest.main()
