#!/usr/bin/env python3
"""One bounded kiosk-crash series over an existing owner SSH connection.

No install, reboot, reconnect, recovery stop or watchdog access. Lost replies
leave the series consumed; collect durable receipts without replaying it.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path

_spec = importlib.util.spec_from_file_location(
    'crash_transport', Path(__file__).with_name('qualify-kiosk-hang.py'))
transport = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(transport)
expectation_for_trial = transport.expectation_for_trial
POLICY = dict(max_crashes=6, interval_ms=30000, max_gap_ms=45000,
              last_at_least_ms=125000, latest_ms=170000)


class SharedConnection(transport.SharedConnection):
    def __init__(self, host, control, helper_sha):
        super().__init__(host, control, helper_sha)
        self.helper = '/data/spacewars-recovery-tools/' + self.sha + '/crash_kiosk_trial'

    def remote(self, args, data=None):
        # Only the fixed bounded-series operation gets the longer timeout.
        timeout = 190 if len(args) == 3 and args[:2] == [self.helper, 'crash'] else 12
        return super().remote(args, data, timeout=timeout)


def verify_receipt(result, plan, selected):
    expected = plan['expectation']
    transport.require(result.get('schema') == 1 and result.get('case') == 'kiosk-crash'
        and result.get('action') == 'bounded-sigkill-series' and result.get('signal') == 'SIGKILL'
        and result.get('plan_sha256') == selected and result.get('transaction') == expected['transaction']
        and result.get('boot_id') == expected['trial_boot_id'] and result.get('policy') == POLICY
        and result.get('series_completed') is True and result.get('automatic_recovery_qualified') is False,
        'Crash receipt differs; observe without replay')
    receipts = result.get('receipts', [])
    transport.require(1 <= len(receipts) <= POLICY['max_crashes'], 'Wrong crash count')
    identities = set()
    previous = None
    for step, receipt in enumerate(receipts, 1):
        health = receipt['health_before']
        requested, exited = receipt['signal_requested_ms'], receipt['exit_observed_ms']
        identity = (health['process_id'], health['start_ticks'], health['invocation_id'])
        transport.require(receipt['step'] == step and identity not in identities
            and health['boot_id'] == expected['trial_boot_id']
            and health['build_id'] == expected['image']['build_id']
            and 0 <= health['uptime_ms'] <= requested < POLICY['latest_ms']
            and requested <= exited < requested + 5000, 'Crash step identity or time differs')
        if previous is None:
            transport.require(health == plan['health'] and requested < 45000
                and plan['selected_ms'] <= requested < plan['selected_ms'] + 10000,
                'First crash missed its selected early window')
        else:
            transport.require(previous['exit_observed_ms'] + POLICY['interval_ms'] <= requested
                < previous['signal_requested_ms'] + POLICY['max_gap_ms']
                and requested - health['uptime_ms'] < 5000
                and previous['signal_requested_ms'] < POLICY['last_at_least_ms'],
                'Crash spacing was unsafe or continued after completion')
        identities.add(identity)
        previous = receipt
    transport.require(receipts[-1]['signal_requested_ms'] >= POLICY['last_at_least_ms'],
                      'Crash series ended before its final window')


def request_crashes(connection, expectation, output):
    connection.verify()
    inspected = transport.invoke(connection, output, 'inspect', ['inspect'], expectation)
    transport.require(set(inspected) == {'plan', 'plan_sha256'}, 'Unexpected crash plan envelope')
    plan, selected = inspected['plan'], inspected['plan_sha256']
    transport.require(plan['expectation'] == expectation and plan.get('schema') == 1
        and plan.get('case') == 'kiosk-crash' and plan.get('action') == 'bounded-sigkill-series'
        and plan.get('policy') == POLICY, 'Native crash plan changed selection or bounds')
    transport.require(transport.digest(plan) == selected, 'Native crash plan digest mismatch')
    transport.save(output, 'plan.json', plan)
    connection.verify()
    result = transport.invoke(connection, output, 'crash', ['crash', selected], plan)
    verify_receipt(result, plan, selected)
    transport.save(output, 'result.json', result)
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
    result = request_crashes(SharedConnection(args.host, args.control_path, args.helper_sha256),
                             expectation, args.output)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
