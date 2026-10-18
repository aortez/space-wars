"""Exercise workstation interruption handling without SSH or device writes."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / 'scripts/qualify-recovery.py'
SPEC = importlib.util.spec_from_file_location('qualify_recovery', SOURCE)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)
HELPER = '/data/spacewars-recovery-tools/' + 'a' * 64 + '/recovery_qualify'


def result(code=0, value=None):
    return subprocess.CompletedProcess([], code, json.dumps(value).encode(), b'')


class Clock:
    def __init__(self):
        self.now = 0

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class QualificationRunnerTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.runner = RUNNER.Runner(self.directory, 'test.local', HELPER)
        self.metadata = dict(id='b' * 64, helper_sha256='a' * 64,
                             host_boot_id=Path('/proc/sys/kernel/random/boot_id').read_text().strip())
        self.plan = dict(origin_boot_id='original')
        self.clock = Clock()
        self.addCleanup(patch.stopall)
        patch.object(RUNNER, 'time', self.clock).start()
        patch('builtins.print').start()

    def start_observation(self):
        RUNNER.save(self.directory / 'activation-start.json',
                    dict(monotonic=0, host_boot_id=self.metadata['host_boot_id']))

    def test_observer_starts_offline_and_accepts_only_new_normal_boot(self):
        self.start_observation()
        replies = iter([(45, result(255)), (10, result(value=dict(current_boot_id='original'))),
                        (45, result(255)), (45, result(255)), (45, result(255)),
                        (11, result(value=dict(current_boot_id='normal',
                                               state=dict(request_consumed=True, phase='requested'))))])

        def native(operation, identity, timeout):
            self.assertEqual((operation, identity), ('status', self.metadata['id']))
            delay, reply = next(replies)
            self.assertLessEqual(delay, timeout)
            self.clock.sleep(delay)
            return reply

        with patch.object(self.runner, 'native', side_effect=native) as calls, \
                patch.object(self.runner, 'helper_identity') as identity:
            RUNNER.observe(self.runner, self.metadata, self.plan)
        self.assertEqual(calls.call_count, 6)
        identity.assert_called_once_with('a' * 64)
        returned = RUNNER.read(self.directory / 'returned.json')
        self.assertTrue(returned['connection_loss_observed'])
        self.assertTrue(returned['watchdog_interval_consistent'])
        self.assertFalse(returned['trial_userspace_observed'])
        self.assertFalse(returned['runner_requested_additional_reboot'])
        self.assertEqual(returned['elapsed_seconds'], 216)

    def test_timeout_budget_includes_remote_calls_and_never_reboots(self):
        self.start_observation()

        def offline(operation, identity, timeout):
            self.assertEqual(operation, 'status')
            self.assertLessEqual(self.clock.now + timeout, 420)
            self.clock.sleep(timeout)
            raise subprocess.TimeoutExpired('ssh', timeout)

        with patch.object(self.runner, 'native', side_effect=offline), \
                patch.object(self.runner, 'helper_identity') as identity:
            with self.assertRaisesRegex(RuntimeError, 'No verified normal return'):
                RUNNER.observe(self.runner, self.metadata, self.plan)
        self.assertEqual(self.clock.now, 420)
        identity.assert_not_called()
        self.assertFalse((self.directory / 'returned.json').exists())

    def test_late_reply_is_not_reported_as_bounded_success(self):
        self.start_observation()
        self.clock.now = 399

        def late(*args, **kwargs):
            self.clock.sleep(22)
            return result(value=dict(current_boot_id='normal', state=dict(request_consumed=True, phase='requested')))

        with patch.object(self.runner, 'native', side_effect=late):
            with self.assertRaisesRegex(ValueError, 'after the observation deadline'):
                RUNNER.observe(self.runner, self.metadata, self.plan)
        self.assertFalse((self.directory / 'returned.json').exists())

    def test_lost_activation_reply_cannot_be_repeated_locally(self):
        RUNNER.save(self.directory / 'prepare.json', dict(state=dict(phase='prepared')))
        with patch.object(self.runner, 'native', side_effect=subprocess.TimeoutExpired('ssh', 90)) as call:
            with self.assertRaises(subprocess.TimeoutExpired):
                RUNNER.activate(self.runner, self.metadata, self.plan)
            with self.assertRaises(FileExistsError):
                RUNNER.activate(self.runner, self.metadata, self.plan)
        self.assertEqual(call.call_count, 1)
        self.assertEqual(call.call_args.args, ('activate', self.metadata['id']))
        self.assertEqual((self.directory / 'activate.json').read_bytes(), b'')

    def test_existing_operation_evidence_prevents_another_remote_mutation(self):
        (self.directory / 'prepare.json').touch()
        with patch.object(self.runner, 'native') as call:
            with self.assertRaises(FileExistsError):
                self.runner.operation('prepare', 'digest')
        call.assert_not_called()

    def test_restore_can_resume_after_postcheck_failure_without_overwriting_evidence(self):
        RUNNER.save(self.directory / 'stores-before.json', ['original'])
        calls = []

        def remote_native(runner, operation, identity, **kwargs):
            calls.append(operation)
            return result(value=dict(state=dict(phase='restored'), current_boot_id='normal'))

        with patch.object(RUNNER.Runner, 'native', new=remote_native), \
                patch.object(RUNNER.Runner, 'query', return_value=json.dumps(
                    dict(boot_id='normal', progress=dict(stage='healthy')))), \
                patch.object(RUNNER.Runner, 'stores', return_value=['changed']):
            for _ in range(2):
                with self.assertRaisesRegex(ValueError, 'Persistent store inventory changed'):
                    RUNNER.restore(self.runner, self.metadata, self.plan)
        self.assertEqual(calls, ['restore', 'restore'])
        attempts = list((self.directory / 'restorations').iterdir())
        self.assertEqual(len(attempts), 2)
        for attempt in attempts:
            self.assertEqual(RUNNER.read(attempt / 'restore.json')['state']['phase'], 'restored')
            self.assertEqual(RUNNER.read(attempt / 'stores-after.json'), ['changed'])

    def test_gatt_cache_changes_are_reported_without_exempting_pairings_or_metadata(self):
        adapter = '/data/bluetooth/00:11:22:33:44:55'
        cache = adapter + '/attributes'
        peer = adapter + '/66:77:88:99:AA:BB'
        before = sorted(['a' * 64 + '  ' + cache,
                         'b' * 64 + '  ' + peer + '/info',
                         'c' * 64 + '  ' + peer + '/attributes',
                         'regular file:0:0:600:' + cache])
        after = sorted(line.replace('a' * 64, 'd' * 64) for line in before)
        comparison = RUNNER.compare_stores(before, after)
        self.assertFalse(comparison['all_bytes_unchanged'])
        self.assertTrue(comparison['durable_stores_unchanged'])
        self.assertEqual(comparison['regenerated_local_gatt_databases'],
                         [dict(path=cache, before='a' * 64, after='d' * 64)])
        for changed in [
            [line.replace('b' * 64, 'e' * 64) for line in after],
            [line.replace('c' * 64, 'e' * 64) for line in after],
            [line.replace('0:0:600:', '1000:0:600:') for line in after],
            [line for line in after if not line.startswith('d' * 64)],
            after + ['f' * 64 + '  /data/bluetooth/11:22:33:44:55:66/attributes'],
        ]:
            self.assertFalse(RUNNER.compare_stores(before, sorted(changed))['durable_stores_unchanged'])


if __name__ == '__main__':
    unittest.main()
