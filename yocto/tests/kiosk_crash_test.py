"""Bounded crash transport fixtures; no device or service operations."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('crash_runner',
    Path(__file__).resolve().parents[1] / 'scripts/qualify-kiosk-crash.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def expectation():
    return dict(transaction='a'*64, trial_boot_id='selected-boot', image=dict(build_id='selected-image'))


def plan():
    expected = expectation()
    return dict(schema=1, case='kiosk-crash', action='bounded-sigkill-series', expectation=expected,
        policy=dict(runner.POLICY), selected_ms=21000,
        health=dict(boot_id=expected['trial_boot_id'], build_id=expected['image']['build_id'],
            process_id=123, start_ticks=100, invocation_id='a'*32, uptime_ms=20000))


def receipt(selected_plan):
    receipts = []
    for step, at in enumerate([21000, 51010, 81020, 111030, 141040], 1):
        health = dict(selected_plan['health'])
        if step > 1:
            health.update(process_id=123+step, start_ticks=100+step,
                          invocation_id=f'{step:032x}', uptime_ms=at-100)
        receipts.append(dict(step=step, health_before=health, signal_requested_ms=at, exit_observed_ms=at+10))
    return dict(schema=1, case='kiosk-crash', action='bounded-sigkill-series', signal='SIGKILL',
        transaction=selected_plan['expectation']['transaction'], boot_id=selected_plan['expectation']['trial_boot_id'],
        plan_sha256=runner.transport.digest(selected_plan), policy=dict(runner.POLICY),
        series_completed=True, automatic_recovery_qualified=False, receipts=receipts)


class Fake:
    helper = '/data/spacewars-recovery-tools/' + 'a'*64 + '/crash_kiosk_trial'

    def __init__(self, failure=None):
        self.failure, self.calls = failure, []

    def verify(self): self.calls.append('verify')

    def remote(self, args, data=None):
        op = args[1]; self.calls.append(op); value = json.loads(data)
        if op == 'inspect':
            selected_plan = plan()
            if self.failure == 'wrong-policy': selected_plan['policy']['max_crashes'] += 1
            if self.failure == 'wrong-action': selected_plan['action'] = 'unbounded'
            if self.failure == 'old-transaction': selected_plan['expectation']['transaction'] = 'b'*64
            result = dict(plan=selected_plan, plan_sha256=runner.transport.digest(selected_plan))
            if self.failure == 'wrong-digest': result['plan_sha256'] = '0'*64
        else:
            if self.failure == 'lost-reply': raise subprocess.TimeoutExpired(args,190)
            result = receipt(value)
            if self.failure == 'short-series': result['receipts'].pop()
        return subprocess.CompletedProcess(args,0,json.dumps(result).encode(),b'')


class TestCrash(unittest.TestCase):
    def test_selection_refusals_and_lost_replies_never_repeat_a_series(self):
        for failure in [None,'wrong-policy','wrong-action','old-transaction','wrong-digest','lost-reply','short-series']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                fake=Fake(failure); output=Path(directory)
                if failure:
                    with self.assertRaises((ValueError,subprocess.TimeoutExpired)):
                        runner.request_crashes(fake,expectation(),output)
                else: runner.request_crashes(fake,expectation(),output)
                self.assertEqual(fake.calls.count('crash'),0 if failure in ['wrong-policy','wrong-action','old-transaction','wrong-digest'] else 1)
                if failure=='lost-reply':
                    self.assertTrue((output/'crash-intent.json').exists())
                    self.assertTrue((output/'crash-uncertain.json').exists())
                before=list(fake.calls)
                with self.assertRaises(FileExistsError): runner.request_crashes(fake,expectation(),output)
                self.assertEqual(fake.calls,before+['verify'])

    def test_each_signal_requires_new_identity_and_bounded_timing(self):
        selected_plan=plan(); good=receipt(selected_plan); selected=runner.transport.digest(selected_plan)
        runner.verify_receipt(good,selected_plan,selected)
        for fault in ['boot','image','repeat','late-first','wide-gap','short-gap','old-health','slow-exit','missing-exit','count','step','transaction','after-completion','absolute-deadline']:
            with self.subTest(fault=fault):
                bad=copy.deepcopy(good); step=bad['receipts'][1]
                if fault=='boot': step['health_before']['boot_id']='old'
                elif fault=='image': step['health_before']['build_id']='other'
                elif fault=='repeat': step['health_before']=dict(bad['receipts'][0]['health_before'])
                elif fault=='late-first': bad['receipts'][0]['signal_requested_ms']=45000
                elif fault=='wide-gap': step['signal_requested_ms']=66000; step['exit_observed_ms']=66010
                elif fault=='short-gap': step['signal_requested_ms']-=1
                elif fault=='old-health': step['health_before']['uptime_ms']=step['signal_requested_ms']-5000
                elif fault=='slow-exit': step['exit_observed_ms']=step['signal_requested_ms']+5000
                elif fault=='missing-exit': step['exit_observed_ms']=step['signal_requested_ms']-1
                elif fault=='count': bad['receipts']*=2
                elif fault=='step': step['step']=1
                elif fault=='transaction': bad['transaction']='b'*64
                elif fault=='absolute-deadline': bad['receipts'][-1]['signal_requested_ms']=170000
                elif fault=='after-completion':
                    bad['receipts'][-2].update(signal_requested_ms=125000,exit_observed_ms=125010)
                    bad['receipts'][-2]['health_before']['uptime_ms']=124900
                    bad['receipts'][-1].update(signal_requested_ms=155010,exit_observed_ms=155020)
                    bad['receipts'][-1]['health_before']['uptime_ms']=154910
                with self.assertRaises(ValueError): runner.verify_receipt(bad,selected_plan,selected)

    def test_long_timeout_only_applies_to_fixed_series_on_existing_connection(self):
        connection=runner.SharedConnection('owner@fixture.local','/tmp/existing','a'*64)
        with patch.object(runner.transport.subprocess,'run',return_value=subprocess.CompletedProcess([],255,b'',b'lost')) as call:
            with self.assertRaisesRegex(ValueError,'no fresh login'): connection.remote([connection.helper,'crash','a'*64])
            self.assertEqual(call.call_count,1)
        for args,timeout in [([connection.helper,'crash','a'*64],190),([connection.helper,'inspect'],12),(['stat','path'],12),(['arbitrary','crash','a'*64],12)]:
            with self.subTest(args=args), patch.object(runner.transport.subprocess,'run',return_value=subprocess.CompletedProcess([],0,b'',b'')) as call:
                connection.remote(args)
                self.assertEqual(call.call_count,2)
                self.assertEqual(call.call_args.kwargs['timeout'],timeout)
                self.assertIn('ProxyCommand=false',call.call_args.args[0])
                self.assertIn('ControlMaster=no',call.call_args.args[0])


if __name__=='__main__': unittest.main()
