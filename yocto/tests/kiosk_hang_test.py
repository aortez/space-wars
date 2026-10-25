"""Host control-channel tests; these never contact a device."""
import importlib.util
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/qualify-kiosk-hang.py'
spec = importlib.util.spec_from_file_location('hang_runner', SCRIPT)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class Fake:
    helper = '/data/spacewars-recovery-tools/' + 'a' * 64 + '/kiosk_hang_trial'

    def __init__(self, failure=None):
        self.calls, self.failure = [], failure

    def verify(self):
        self.calls.append('verify')

    def remote(self, args, data=None):
        op = args[1]
        self.calls.append(op)
        value = json.loads(data)
        if op == 'inspect':
            plan = dict(expectation=value, selected_ms=30000, health={})
            if self.failure == 'wrong-transaction':
                plan['expectation'] = dict(value, transaction='b'*64)
            result = dict(plan=plan, plan_sha256=runner.digest(plan))
            if self.failure == 'digest':
                result['plan_sha256'] = '0'*64
        else:
            if self.failure == 'lost-reply':
                raise subprocess.TimeoutExpired(args, 12)
            result = dict(plan_sha256=runner.digest(value),
                transaction=value['expectation']['transaction'],
                boot_id=value['expectation']['trial_boot_id'], signal='SIGSTOP',
                stopped_process_observed=True,automatic_recovery_qualified=False)
        return subprocess.CompletedProcess(args, 0, json.dumps(result).encode(), b'')


class TestRunner(unittest.TestCase):
    def test_expectation_rejects_old_transactions_and_wrong_candidate(self):
        good = dict(slot='b', build_id='old', generation=12, payload='old-payload',
                    expanded='old-root', data={'writes': 1})
        binding = {'device_id': 'fixture-board'}
        image = dict(build_id='new', source={'commit': 'a'*40})
        selected = dict(boot_id='old-boot', binding=binding, recovery='checksum',
            candidate=dict(build_id='new',generation=13,compressed={'sha256':'new-payload'},
                expanded={'sha256':'new-root'},data={'writes':1},source_commit='a'*40))
        before = dict(boot=dict(kernel_build_id='kernel', observation=dict(boot_id='old-boot',
            binding=binding, tryboot=False, actual_slot='b', default_slot='b')),
            recovery=dict(checksum='checksum',record=dict(good=good)))
        candidate = dict(slot='a',build_id='new',generation=13,payload='new-payload',
                         expanded='new-root',data={'writes':1})
        trial = dict(boot=dict(image=image,kernel_build_id='kernel',observation=dict(
            boot_id='new-boot',tryboot=True,binding=binding,uptime_ms=30000,
            trial_id='new-transaction',actual_slot='a',default_slot='b')),
            recovery=dict(record=dict(good=good,transaction=dict(id='new-transaction',
                phase='observing',origin_boot_id='old-boot',trial_boot_id='new-boot',
                previous_good=good,candidate=candidate))))
        result = runner.expectation_for_trial(selected,before,trial,image)
        self.assertEqual(result['transaction'],'new-transaction')
        for field, value in [('id','old-transaction'),('phase','rolled-back'),
                             ('origin_boot_id','another-origin'),('trial_boot_id','old-boot')]:
            changed=copy.deepcopy(trial)
            changed['recovery']['record']['transaction'][field]=value
            with self.subTest(field=field), self.assertRaises(ValueError):
                runner.expectation_for_trial(selected,before,changed,image)
        changed=copy.deepcopy(trial)
        changed['recovery']['record']['transaction']['candidate']['payload']='another-payload'
        with self.assertRaises(ValueError):
            runner.expectation_for_trial(selected,before,changed,image)

    def test_one_stop_and_no_retry_after_uncertain_reply(self):
        for failure in [None, 'lost-reply', 'wrong-transaction', 'digest']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                fake = Fake(failure)
                expected = dict(transaction='a'*64,trial_boot_id='22222222-2222-2222-2222-222222222222')
                if failure:
                    with self.assertRaises((ValueError,subprocess.TimeoutExpired)):
                        runner.inject(fake, expected, output)
                else:
                    runner.inject(fake, expected, output)
                self.assertEqual(fake.calls.count('stop'),
                                 0 if failure in ['wrong-transaction','digest'] else 1)
                if failure == 'lost-reply':
                    self.assertTrue((output/'stop-intent.json').exists())
                    self.assertTrue((output/'stop-uncertain.json').exists())
                # A repeated call cannot invoke the device again, even after
                # an incomplete prior result, because the first intent exists.
                calls = list(fake.calls)
                with self.assertRaises(FileExistsError):
                    runner.inject(fake, expected, output)
                self.assertEqual(fake.calls, calls + ['verify'])

    def test_master_loss_never_falls_back_to_a_fresh_login(self):
        connection=runner.SharedConnection('owner@fixture.local','/tmp/existing-control','a'*64)
        with patch.object(runner.subprocess,'run',
                return_value=subprocess.CompletedProcess([],255,b'',b'lost')) as call:
            with self.assertRaisesRegex(ValueError,'no fresh login'):
                connection.remote(['true'])
            self.assertEqual(call.call_count,1)
            self.assertIn('check',call.call_args.args[0])

    def test_remote_requires_master_and_disables_proxy_fallback(self):
        connection=runner.SharedConnection('owner@fixture.local','/tmp/existing-control','a'*64)
        with patch.object(runner.subprocess,'run',
                return_value=subprocess.CompletedProcess([],0,b'',b'')) as call:
            connection.remote(['true'])
            self.assertEqual(call.call_count,2)
            command=call.call_args.args[0]
            self.assertIn('ControlMaster=no',command)
            self.assertIn('ProxyCommand=false',command)
            self.assertNotIn('-M',command)


if __name__ == '__main__':
    unittest.main()
