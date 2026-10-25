"""Recovery-observer fault transport fixtures; no device or service operations."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('failure_runner',
    Path(__file__).resolve().parents[1] / 'scripts/qualify-recovery-failure.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def expectation():
    return dict(transaction='a'*64, trial_boot_id='selected-boot')


def plan():
    return dict(schema=1, case='recovery-service-failure', action='kill-recovery-observer',
        expectation=expectation(), selected_ms=21000,
        instance=dict(start_ticks=10, boot_id='selected-boot', unit=dict(active='activating', sub='start',
            invocation='a'*32, pid=123, exec_pid=123, exec_start_us=200000, exec_code=0,
            exec_status=0, result='success', restarts=0)),
        trial=dict(recovery='b'*64, watchdog_armed_ms=1400, watchdog_timeout_seconds=210))


def receipt(p):
    unit=dict(p['instance']['unit'])
    unit.update(active='failed', sub='failed', pid=0, exec_code=2, exec_status=9, result='signal')
    return dict(schema=1, case='recovery-service-failure', action='kill-recovery-observer',
        signal='SIGKILL', transaction=p['expectation']['transaction'], boot_id=p['expectation']['trial_boot_id'],
        plan_sha256=runner.transport.digest(p), instance_before=p['instance'], trial=p['trial'],
        signal_requested_ms=21000, exit_observed_ms=21010, failed_observed_ms=21020,
        pinned_exit_observed=True, unit_after=unit, automatic_recovery_qualified=False)


class Fake:
    helper = '/data/spacewars-recovery-tools/' + 'a'*64 + '/fail_recovery_trial'

    def __init__(self, failure=None): self.failure, self.calls = failure, []
    def verify(self): self.calls.append('verify')
    def remote(self, args, data=None):
        op=args[1]; self.calls.append(op); p=json.loads(data)
        if op=='inspect':
            p=plan()
            if self.failure=='wrong-action': p['action']='arbitrary'
            if self.failure=='old-transaction': p['expectation']['transaction']='b'*64
            if self.failure=='watchdog': p['trial']['watchdog_timeout_seconds']=211
            result=dict(plan=p,plan_sha256=runner.transport.digest(p))
            if self.failure=='wrong-digest': result['plan_sha256']='0'*64
        elif op=='kill':
            if self.failure=='lost-reply': raise subprocess.TimeoutExpired(args,25)
            result=receipt(p)
            if self.failure=='restarted': result['unit_after']['restarts']=1
        elif op=='observe':
            result=dict(schema=1,case='recovery-service-failure',action='observe-failed-recovery',
                transaction=p['expectation']['transaction'],boot_id=p['expectation']['trial_boot_id'],
                plan_sha256=runner.transport.digest(p),trial=p['trial'],unit=receipt(p)['unit_after'],uptime_ms=185000)
            if self.failure=='changed-journal': result['trial']=dict(p['trial'],recovery='c'*64)
            if self.failure=='late': result['uptime_ms']=300000
            if self.failure=='restarted': result['unit']['restarts']=1
        else: raise AssertionError(op)
        return subprocess.CompletedProcess(args,0,json.dumps(result).encode(),b'')


class TestRecoveryFailure(unittest.TestCase):
    def test_selection_refusals_and_lost_reply_never_repeat_a_signal(self):
        selection_failures=['wrong-action','old-transaction','watchdog','wrong-digest']
        for failure in [None,*selection_failures,'lost-reply','restarted']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as d:
                fake=Fake(failure); out=Path(d)
                if failure:
                    with self.assertRaises((ValueError,subprocess.TimeoutExpired)):
                        runner.request_failure(fake,expectation(),out)
                else: runner.request_failure(fake,expectation(),out)
                self.assertEqual(fake.calls.count('kill'),0 if failure in selection_failures else 1)
                if failure=='lost-reply':
                    self.assertTrue((out/'kill-intent.json').exists())
                    self.assertTrue((out/'kill-uncertain.json').exists())
                before=list(fake.calls)
                with self.assertRaises(FileExistsError): runner.request_failure(fake,expectation(),out)
                self.assertEqual(fake.calls,before+['verify'])

    def test_receipt_requires_exact_process_exit_journal_and_times(self):
        p=plan(); selected=runner.transport.digest(p); good=receipt(p)
        runner.verify_receipt(good,p,selected)
        mutations=[('boot_id','old'),('transaction','b'*64),('pinned_exit_observed',False),
            ('automatic_recovery_qualified',True),('signal_requested_ms',31000),
            ('exit_observed_ms',20999),('exit_observed_ms',26000),('failed_observed_ms',21000),
            ('failed_observed_ms',45000)]
        for key,value in mutations:
            bad=copy.deepcopy(good);bad[key]=value
            with self.subTest(key=key,value=value), self.assertRaises(ValueError): runner.verify_receipt(bad,p,selected)
        for key,value in [('pid',321),('exec_pid',321),('exec_start_us',200001),('invocation','b'*32),
            ('result','timeout'),('exec_status',15),('exec_code',1),('restarts',1),('active','inactive')]:
            bad=copy.deepcopy(good);bad['unit_after'][key]=value
            with self.subTest(key=key), self.assertRaises(ValueError): runner.verify_receipt(bad,p,selected)

    def test_after_deadline_observation_cannot_inject_again(self):
        for failure in [None,'changed-journal','late','restarted']:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as d:
                p=plan();fake=Fake(failure)
                if failure:
                    with self.assertRaises(ValueError): runner.observe_failure(fake,p,runner.transport.digest(p),Path(d),'after-deadline')
                else: runner.observe_failure(fake,p,runner.transport.digest(p),Path(d),'after-deadline')
                self.assertEqual(fake.calls,['observe'])

    def test_fixed_timeout_and_existing_connection_only(self):
        connection=runner.SharedConnection('owner@fixture.local','/tmp/existing','a'*64)
        with patch.object(runner.transport.subprocess,'run',return_value=subprocess.CompletedProcess([],255,b'',b'lost')) as call:
            with self.assertRaisesRegex(ValueError,'no fresh login'): connection.remote([connection.helper,'kill','a'*64])
            self.assertEqual(call.call_count,1)
        for args,timeout in [([connection.helper,'kill','a'*64],25),([connection.helper,'inspect'],12),
            ([connection.helper,'observe','a'*64],12),(['arbitrary','kill','a'*64],12)]:
            with self.subTest(args=args), patch.object(runner.transport.subprocess,'run',return_value=subprocess.CompletedProcess([],0,b'',b'')) as call:
                connection.remote(args)
                self.assertEqual(call.call_count,2)
                self.assertEqual(call.call_args.kwargs['timeout'],timeout)
                self.assertIn('ProxyCommand=false',call.call_args.args[0])
                self.assertIn('ControlMaster=no',call.call_args.args[0])


if __name__=='__main__': unittest.main()
