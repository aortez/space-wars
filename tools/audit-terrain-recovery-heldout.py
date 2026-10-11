#!/usr/bin/env python3
"""Audit every frozen held-out recovery arm without rerunning simulation."""
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tarfile


def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


TOOLS = Path(__file__).resolve().parent
common = module(TOOLS / 'audit-terrain-recovery-missions.py', 'recovery_audit')
runner = module(TOOLS / 'run-terrain-recovery-heldout.py', 'heldout_runner')


def outcome_score(outcome, seat):
    if outcome == 'draw':
        return 0
    assert isinstance(outcome, dict) and outcome.get('winner') in ('player_1', 'player_2')
    return 1 if outcome['winner'] == f'player_{seat+1}' else -1


def seat_metrics(summary, seat):
    native = summary['native_recovery'][seat]
    return {'recoveries': summary['brains'][seat]['completed_recoveries'],
            'sorties': summary['brains'][seat]['completed_sorties'],
            'blocked_ticks': summary['metrics'][seat]['recovery_blocked_ticks'],
            'longest_block': summary['metrics'][seat]['longest_recovery_block'],
            'mission_blocked_ticks': summary['metrics'][seat]['mission_blocked_ticks'],
            'ships_lost': native.get('ships_lost') if native else None,
            'rebuilds': native.get('rebuilds') if native else None,
            'death_tick': summary['match']['pilots'][seat]['death_tick'],
            'owned_planets': summary['match']['owned_planets'][seat],
            'outcome_score': outcome_score(summary['match']['outcome'], seat)}


def regression_flags(control, candidate):
    flags = []
    for key in ('recoveries', 'sorties', 'owned_planets', 'outcome_score'):
        if candidate[key] < control[key]:
            flags.append(f'less_{key}')
    for key in ('blocked_ticks', 'longest_block', 'mission_blocked_ticks', 'ships_lost'):
        if control[key] is not None and candidate[key] is not None and candidate[key] > control[key]:
            flags.append(f'more_{key}')
    if control['death_tick'] is None and candidate['death_tick'] is not None:
        flags.append('new_pilot_death')
    return flags


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    runtime = common.read(root / 'runtime.json')
    inputs = common.read(root / 'source-inputs.json')
    assert common.digest(root / 'recovery-heldout') == runtime['binary_sha256']
    assert common.digest(root / 'source-inputs.json') == runtime['source_inputs_sha256']
    assert common.digest(root / 'matrix.json') == runtime['matrix_sha256']
    assert common.digest(Path(__file__)) == runtime['auditor_sha256']
    for name in ('run-terrain-recovery-heldout.py', 'audit-terrain-recovery-missions.py'):
        assert common.digest(TOOLS / name) == inputs['tools/' + name]
    checked = set()
    process = subprocess.Popen(['git', 'archive', runtime['source_commit']], cwd=TOOLS.parent,
                               stdout=subprocess.PIPE)
    with tarfile.open(fileobj=process.stdout, mode='r|') as archive:
        for member in archive:
            if member.name in inputs:
                payload = member.linkname.encode() if member.issym() else archive.extractfile(member).read()
                assert hashlib.sha256(payload).hexdigest() == inputs[member.name], member.name
                checked.add(member.name)
    assert process.wait() == 0 and checked == set(inputs)
    matrix = common.read(root / 'matrix.json')
    assert matrix == runner.matrix()
    results = common.read(root / 'results.json')
    assert len(matrix) == len(results) == 48
    settings, cases, comparisons = {}, [], []
    for item, result in zip(matrix, results, strict=True):
        assert item['name'] == result['name']
        assert result['source_commit'] == runtime['source_commit']
        assert result['binary_sha256'] == runtime['binary_sha256']
        receipt, _ = common.audit_case(root, item, result)
        path = root / 'cases' / item['name'] / 'output'
        summary = receipt['summary']
        assert not any(summary[k] for k in ('controller_reset', 'input_override', 'physics_edited'))
        profiles = [runner.PROFILE if item['case']['candidate_seat'] == s else 'standard' for s in (0, 1)]
        assert summary['profiles'] == common.read(path / 'profiles.json') == profiles
        controllers = (path / 'initial-controllers.txt').read_text()
        if item['arm'] == 'control':
            assert 'terrain_recovery: true' not in controllers
            assert 'hatch_supported_arrival: true' not in controllers
            settings[item['setting']] = (path, receipt)
        else:
            control_path, control_receipt = settings[item['setting']]
            assert (path / 'initial.json').read_bytes() == (control_path / 'initial.json').read_bytes()
            for flag in ('terrain_recovery', 'hatch_supported_arrival'):
                assert controllers.count(flag + ': true') == 1
                controllers = controllers.replace(flag + ': true', flag + ': false')
            assert controllers == (control_path / 'initial-controllers.txt').read_text()
            comparison = common.compare(control_path / 'actions.jsonl', path / 'actions.jsonl')
            seat = item['case']['candidate_seat']
            first_recovery = next((r['tick'] for r in common.rows(path / 'actions.jsonl')
                                   if r['recovery_active'][seat]), None)
            first_input = comparison['first_difference']['actions']
            if first_input is not None:
                assert first_recovery is not None and first_recovery <= first_input
            old, new = seat_metrics(control_receipt['summary'], seat), seat_metrics(summary, seat)
            pair = {'name': item['name'], 'setting': item['setting'], 'seat': seat,
                    'inputs': comparison, 'first_recovery_tick': first_recovery,
                    'same_match_result': summary['match'] == control_receipt['summary']['match'],
                    'control': old, 'candidate': new, 'regression_flags': regression_flags(old, new)}
            comparisons.append(pair)
            receipt['comparison'] = pair
        cases.append(receipt)
        print(json.dumps({'name': item['name'], 'ticks': summary['end_tick'],
                          'recoveries': [b['completed_recoveries'] for b in summary['brains']],
                          'regression_flags': receipt.get('comparison', {}).get('regression_flags')}), flush=True)
    assert len(settings) == 16 and len(comparisons) == 32
    artifacts = {str(p.relative_to(root)): {'sha256': common.digest(p), 'bytes': p.stat().st_size}
                 for p in sorted((root / 'cases').rglob('*')) if p.is_file()}
    totals = {'matches': len(cases), 'settings': len(settings), 'paired_candidates': len(comparisons),
              'decisions': sum(c['summary']['end_tick'] for c in cases),
              'audit_samples': sum(c['audit_samples'] for c in cases),
              'comparisons_with_changed_inputs': sum(p['inputs']['first_difference']['actions'] is not None for p in comparisons),
              'comparisons_with_additional_recoveries': sum(p['candidate']['recoveries'] > p['control']['recoveries'] for p in comparisons),
              'comparisons_with_regression_flags': sum(bool(p['regression_flags']) for p in comparisons),
              'regression_flags': dict(Counter(flag for p in comparisons for flag in p['regression_flags']))}
    report = {'schema': 1, 'runtime': runtime, 'totals': totals, 'cases': cases,
              'comparisons': comparisons, 'source_files_verified': len(checked),
              'artifact_receipts': artifacts, 'auditor_sha256': common.digest(Path(__file__))}
    (root / 'audit.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(totals), flush=True)


if __name__ == '__main__':
    main()
