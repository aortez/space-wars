#!/usr/bin/env python3
"""Request one selected trial reset over an existing owner SSH connection.

Installation, WAN isolation and the separately planned return observation belong
to the surrounding qualification procedure. This never opens a fresh login.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path

_spec = importlib.util.spec_from_file_location(
    'trial_fault_transport', Path(__file__).with_name('qualify-kiosk-hang.py'))
transport = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(transport)
expectation_for_trial = transport.expectation_for_trial


class SharedConnection(transport.SharedConnection):
    def __init__(self, host, control, helper_sha):
        super().__init__(host, control, helper_sha)
        self.helper = '/data/spacewars-recovery-tools/' + self.sha + '/reset_trial'


def request_reset(connection, expectation, output):
    connection.verify()
    inspected = transport.invoke(connection, output, 'inspect', ['inspect'], expectation)
    transport.require(set(inspected) == {'plan', 'plan_sha256'}, 'Unexpected reset plan envelope')
    plan, selected = inspected['plan'], inspected['plan_sha256']
    transport.require(plan['expectation'] == expectation and plan.get('schema') == 1
                      and plan.get('case') == 'trial-reset' and plan.get('action') == 'normal-reboot',
                      'Native reset plan changed the selected trial or action')
    transport.require(transport.digest(plan) == selected, 'Native reset plan digest mismatch')
    transport.save(output, 'plan.json', plan)
    connection.verify()
    result = transport.invoke(connection, output, 'reset', ['reset', selected], plan)
    transport.require(result.get('schema') == 1 and result.get('plan_sha256') == selected
                      and result.get('transaction') == expectation['transaction']
                      and result.get('boot_id') == expectation['trial_boot_id']
                      and result.get('case') == 'trial-reset'
                      and result.get('action') == 'normal-reboot'
                      and result.get('reboot_queued') is True
                      and result.get('automatic_recovery_qualified') is False,
                      'Reset receipt differs; observe without replay')
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
    result = request_reset(SharedConnection(args.host, args.control_path, args.helper_sha256),
                           expectation, args.output)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
