#!/usr/bin/env python3
"""One selected recovery-service failure on an existing owner SSH connection.

No install, reboot, service restart, reconnect or watchdog access. Preserve a
lost request as consumed. Later read-only observations cannot resend the fault.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path

_spec = importlib.util.spec_from_file_location('recovery_failure_transport',
    Path(__file__).with_name('qualify-kiosk-hang.py'))
transport = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(transport)
expectation_for_trial = transport.expectation_for_trial


class SharedConnection(transport.SharedConnection):
    def __init__(self, host, control, helper_sha):
        super().__init__(host, control, helper_sha)
        self.helper = '/data/spacewars-recovery-tools/' + self.sha + '/fail_recovery_trial'

    def remote(self, args, data=None):
        # Hashing/rechecking the selected service and collecting its failure
        # can take longer than a read-only observation. This cannot reconnect.
        timeout = 25 if len(args) == 3 and args[:2] == [self.helper, 'kill'] else 12
        return super().remote(args, data, timeout=timeout)


def failed_unit(unit, before):
    transport.require(unit['active'] == unit['sub'] == 'failed' and unit['pid'] == 0
        and unit['exec_pid'] == before['pid'] and unit['exec_start_us'] == before['exec_start_us']
        and unit['invocation'] == before['invocation'] and unit['restarts'] == before['restarts'] == 0
        and unit['result'] == 'signal' and unit['exec_code'] == 2 and unit['exec_status'] == 9,
        'Recovery failure result does not identify the selected SIGKILL exit')


def verify_receipt(result, plan, selected):
    expected = plan['expectation']
    transport.require(result.get('schema') == 1 and result.get('case') == 'recovery-service-failure'
        and result.get('action') == 'kill-recovery-observer' and result.get('signal') == 'SIGKILL'
        and result.get('transaction') == expected['transaction'] and result.get('boot_id') == expected['trial_boot_id']
        and result.get('plan_sha256') == selected and result.get('instance_before') == plan['instance']
        and result.get('trial') == plan['trial'] and result.get('pinned_exit_observed') is True
        and result.get('automatic_recovery_qualified') is False, 'Recovery failure receipt differs; never replay')
    at, exited, failed = (result[k] for k in ['signal_requested_ms', 'exit_observed_ms', 'failed_observed_ms'])
    transport.require(plan['selected_ms'] <= at < min(45000, plan['selected_ms'] + 10000)
        and at <= exited < at + 5000 and exited <= failed < 45000,
        'Recovery failure missed the selected early window or bounded exit')
    failed_unit(result['unit_after'], plan['instance']['unit'])


def request_failure(connection, expectation, output):
    connection.verify()
    inspected = transport.invoke(connection, output, 'inspect', ['inspect'], expectation)
    transport.require(set(inspected) == {'plan', 'plan_sha256'}, 'Unexpected recovery plan envelope')
    plan, selected = inspected['plan'], inspected['plan_sha256']
    transport.require(plan.get('schema') == 1 and plan.get('case') == 'recovery-service-failure'
        and plan.get('action') == 'kill-recovery-observer' and plan['expectation'] == expectation
        and plan['instance']['boot_id'] == expectation['trial_boot_id']
        and plan['trial']['watchdog_timeout_seconds'] == 210 and transport.digest(plan) == selected,
        'Native recovery plan changed selection')
    transport.save(output, 'plan.json', plan)
    connection.verify()
    result = transport.invoke(connection, output, 'kill', ['kill', selected], plan)
    verify_receipt(result, plan, selected)
    transport.save(output, 'result.json', result)
    return result


def observe_failure(connection, plan, selected, output, label):
    transport.require(transport.digest(plan) == selected, 'Observation plan changed')
    result = transport.invoke(connection, output, label, ['observe', selected], plan)
    expected = plan['expectation']
    transport.require(result.get('schema') == 1 and result.get('case') == 'recovery-service-failure'
        and result.get('action') == 'observe-failed-recovery' and result.get('plan_sha256') == selected
        and result.get('transaction') == expected['transaction'] and result.get('boot_id') == expected['trial_boot_id']
        and result.get('trial') == plan['trial'] and plan['selected_ms'] <= result['uptime_ms'] < 300000,
        'Read-only recovery observation differs')
    failed_unit(result['unit'], plan['instance']['unit'])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True)
    parser.add_argument('--control-path', required=True)
    parser.add_argument('--helper-sha256', required=True)
    parser.add_argument('--expectation', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    transport.require(args.expectation.stat().st_size <= 32 * 1024, 'Oversized expectation')
    expectation = json.loads(args.expectation.read_bytes())
    args.output.mkdir(mode=0o700)
    result = request_failure(SharedConnection(args.host, args.control_path, args.helper_sha256), expectation, args.output)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
