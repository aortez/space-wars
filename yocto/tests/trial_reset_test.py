"""One-use reset transport fixtures; no device or real reboot operations."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('reset_runner',
    Path(__file__).resolve().parents[1] / 'scripts/qualify-trial-reset.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class Fake:
    helper = '/data/spacewars-recovery-tools/' + 'a' * 64 + '/reset_trial'

    def __init__(self, failure=None):
        self.failure, self.calls = failure, []

    def verify(self):
        self.calls.append('verify')

    def remote(self, args, data=None):
        op = args[1]
        self.calls.append(op)
        value = json.loads(data)
        if op == 'inspect':
            plan = dict(schema=1, case='trial-reset', action='normal-reboot',
                        expectation=value, selected_ms=30000, health={})
            if self.failure == 'wrong-action':
                plan['action'] = 'tryboot'
            if self.failure == 'old-transaction':
                plan['expectation'] = dict(value, transaction='b' * 64)
            result = dict(plan=plan, plan_sha256=runner.transport.digest(plan))
            if self.failure == 'wrong-digest':
                result['plan_sha256'] = '0' * 64
        else:
            if self.failure == 'lost-reply':
                raise subprocess.TimeoutExpired(args, 12)
            result = dict(schema=1, case='trial-reset', action='normal-reboot', reboot_queued=True,
                plan_sha256=runner.transport.digest(value),
                transaction=value['expectation']['transaction'],
                boot_id=value['expectation']['trial_boot_id'], automatic_recovery_qualified=False)
            if self.failure == 'wrong-receipt':
                result['transaction'] = 'b' * 64
        return subprocess.CompletedProcess(args, 0, json.dumps(result).encode(), b'')


class TestReset(unittest.TestCase):
    def test_selected_reset_is_once_even_when_reply_is_uncertain(self):
        for failure in [None, 'wrong-action', 'old-transaction', 'wrong-digest',
                        'lost-reply', 'wrong-receipt']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                fake = Fake(failure)
                expected = dict(transaction='a' * 64, trial_boot_id='selected-boot')
                if failure:
                    with self.assertRaises((ValueError, subprocess.TimeoutExpired)):
                        runner.request_reset(fake, expected, Path(directory))
                else:
                    runner.request_reset(fake, expected, Path(directory))
                self.assertEqual(fake.calls.count('reset'),
                    0 if failure in ['wrong-action', 'old-transaction', 'wrong-digest'] else 1)
                if failure == 'lost-reply':
                    self.assertTrue((Path(directory) / 'reset-intent.json').exists())
                    self.assertTrue((Path(directory) / 'reset-uncertain.json').exists())
                before = list(fake.calls)
                with self.assertRaises(FileExistsError):
                    runner.request_reset(fake, expected, Path(directory))
                self.assertEqual(fake.calls, before + ['verify'])

    def test_reset_uses_only_the_existing_connection_and_fixed_helper(self):
        connection = runner.SharedConnection('owner@fixture.local', '/tmp/existing', 'a' * 64)
        self.assertEqual(connection.helper, Fake.helper)
        with patch.object(runner.transport.subprocess, 'run',
                return_value=subprocess.CompletedProcess([], 255, b'', b'lost')) as call:
            with self.assertRaisesRegex(ValueError, 'no fresh login'):
                connection.remote(['true'])
            self.assertEqual(call.call_count, 1)
            self.assertIn('check', call.call_args.args[0])
        with patch.object(runner.transport.subprocess, 'run',
                return_value=subprocess.CompletedProcess([], 0, b'', b'')) as call:
            connection.remote(['true'])
            self.assertEqual(call.call_count, 2)
            self.assertIn('ProxyCommand=false', call.call_args.args[0])
            self.assertIn('ControlMaster=no', call.call_args.args[0])


if __name__ == '__main__':
    unittest.main()
