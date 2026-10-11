#!/usr/bin/env python3
"""Summarize #155 native traces and compare the predeclared paired screen."""
import argparse
from collections import Counter
import hashlib
import itertools
import json
from pathlib import Path


def read(path):
    return json.loads(path.read_text())


def rows(path):
    with path.open() as file:
        for line in file:
            yield json.loads(line)


def digest(path):
    with path.open('rb') as file:
        return hashlib.file_digest(file, 'sha256').hexdigest()


def mode(row):
    # The mission's waiting-for-a-site patrol delegates to the same combat pilot
    # but does not populate MissionTelemetry.combat. Its label is current there.
    combat_called = row['combat_active'] or row['goal'] == 'patrol'
    if row['reason'] == 'climbing for a firing pass' or (
            combat_called and row['combat_goal'] == 'climb clear of ground'):
        return 'climb'
    if combat_called and row['combat_goal'] == 'engage ship':
        return 'engage'
    if combat_called and row['break_until'] is not None and row['tick'] < row['break_until']:
        return 'break'
    return row['goal']


def opportunities(row):
    target, aim = row['target'], row['aim']
    usable = bool(target and aim and target['visible'] and not target['ground_occluded']
                  and row['queries_ready'] and row['laser_available']
                  and aim['direct_window'])
    breaking = row['break_until'] is not None and row['tick'] < row['break_until']
    eligible = usable and mode(row) == 'engage' and not row['cannon'] and not breaking
    return usable, eligible


def analyze_case(case_dir):
    out = case_dir / 'output'
    summary = read(out / 'summary.json')
    assert summary['audits_passed']
    counters = [Counter(), Counter()]
    damage = [Counter(), Counter()]
    modes = [Counter(), Counter()]
    declined = [Counter(), Counter()]
    segments = [[], []]
    energy_sum = [0.0, 0.0]
    for row in rows(out / 'combat.jsonl'):
        seat = row['seat']
        assert seat in (0, 1)
        count = counters[seat]
        assert row['tick'] == count['ticks'], 'missing, repeated or unordered decision row'
        count['ticks'] += 1
        state = mode(row)
        modes[seat][state] += 1
        usable, eligible = opportunities(row)
        count['ready_visible_direct_window_ticks'] += usable
        count['eligible_direct_window_ticks'] += eligible
        count['taken_direct_window_ticks'] += eligible and row['laser']
        count['declined_direct_window_ticks'] += eligible and not row['laser']
        if usable and not row['laser'] and not row['cannon']:
            declined[seat][state] += 1
        count['laser_request_ticks'] += row['laser']
        count['laser_without_cannon_ticks'] += row['laser'] and not row['cannon']
        count['cannon_request_ticks'] += row['cannon']
        if row['aim']:
            count['laser_outside_direct_window_ticks'] += (
                row['laser'] and not row['cannon'] and not row['aim']['direct_window'])
        if row['supply']:
            count['supply_ticks'] += 1
            energy_sum[seat] += row['supply']['energy_percent']
            count['laser_recharging_ticks'] += row['supply']['laser_recharging']
        if row['damage']['last_contact_tick'] == row['tick']:
            count['recorded_debris_contact_ticks'] += 1
        if row['damage']['last_damage_tick'] == row['tick']:
            damage[seat][row['damage']['last_source']] += row['damage']['last_damage_percent']
        if row.get('pilot_damage'):
            event = row['pilot_damage']
            damage[seat]['pilot_' + event['cause']] += event['amount']
        legs = segments[seat]
        if not legs or legs[-1]['mode'] != state:
            legs.append({'mode': state, 'start': row['tick'], 'end': row['tick']})
        else:
            legs[-1]['end'] = row['tick']
    seats = []
    for seat in range(2):
        assert counters[seat]['ticks'] == summary['end_tick'], 'trace ends before native match'
        # Rows are pre-step. Include damage from the native terminal step once;
        # otherwise every pilot-death match would omit its killing hit.
        terminal = summary['damage'][seat]
        if terminal['last_damage_tick'] == summary['end_tick']:
            damage[seat][terminal['last_source']] += terminal['last_damage_percent']
        if terminal['last_contact_tick'] == summary['end_tick']:
            counters[seat]['recorded_debris_contact_ticks'] += 1
        terminal = summary['match']['pilots'][seat]['last_damage']
        if terminal and terminal['tick'] == summary['end_tick']:
            damage[seat]['pilot_' + terminal['cause']] += terminal['amount']
        legs = segments[seat]
        short = [legs[i-1:i+2] for i, leg in enumerate(legs)
                 if 0 < i < len(legs)-1 and leg['mode'] == 'engage'
                 and leg['end']-leg['start']+1 < 120
                 and legs[i-1]['mode'] == legs[i+1]['mode'] == 'climb']
        brain = summary['brains'][seat]
        recovery = summary['native_recovery'][seat]
        outcome = summary['match']['outcome']
        winner = outcome.get('winner') if isinstance(outcome, dict) else None
        seats.append({
            'seat': seat, 'counts': dict(counters[seat]), 'mode_ticks': dict(modes[seat]),
            'declined_ready_windows_by_mode': dict(declined[seat]),
            'recorded_damage_taken': dict(damage[seat]),
            'mean_energy_percent': energy_sum[seat]/max(1, counters[seat]['supply_ticks']),
            'short_climb_engage_climb': short,
            'longest_climb_ticks': max((s['end']-s['start']+1 for s in legs
                                        if s['mode'] == 'climb'), default=0),
            'weapons': summary['combat'][seat]['weapons'],
            'completed_sorties': brain['completed_sorties'],
            'completed_recoveries': brain['completed_recoveries'],
            'ships_lost': recovery['ships_lost'], 'rebuilds': recovery['rebuilds'],
            'pods_lost': recovery['pods_lost'],
            'pilot_death_tick': summary['match']['pilots'][seat]['death_tick'],
            'points': 1 if winner == f'player_{seat+1}' else 0.5 if winner is None else 0,
        })
    probes = []
    for row in rows(out / 'laser-probes.jsonl'):
        probes.append({'tick': row['tick'], 'seat': row['seat'],
                       'active_combat': row['active_combat'],
                       'extra_laser_hit_ticks': row['treatment_weapons']['laser_hit_ticks']
                       - row['control_weapons']['laser_hit_ticks']})
    return {'directory': str(case_dir), 'case': summary['case'],
            'end_tick': summary['end_tick'], 'outcome': summary['match']['outcome'],
            'end_reason': summary['match']['reason'], 'seats': seats, 'probes': probes,
            'hashes': {name: digest(out / name) for name in
                       ['combat.jsonl', 'actions.jsonl', 'summary.json', 'laser-probes.jsonl']}}


def analyze_root(root):
    results = read(root / 'results.json')
    assert all(r['exit_code'] == 0 for r in results), 'incomplete/failed matrix'
    cases = [analyze_case(path.parent) for path in sorted(root.glob('*/result.json'))]
    assert len(cases) == len(results)
    return {'directory': str(root), 'runtime': read(root / 'runtime.json'), 'cases': cases}


def first_change(control, candidate):
    a = rows(Path(control['directory']) / 'output/actions.jsonl')
    b = rows(Path(candidate['directory']) / 'output/actions.jsonl')
    seat = candidate['case']['candidate_seat']
    compared = 0
    for old, new in itertools.zip_longest(a, b):
        assert old is not None and new is not None, 'endings differ before any input change'
        assert old['tick'] == new['tick']
        assert old['tick'] == compared
        compared += 1
        assert old['scenario_before_step_fnv64'] == new['scenario_before_step_fnv64'], (
            'physics differs before first changed input')
        if old['actions'] != new['actions']:
            assert len(old['actions']) == len(new['actions']) == 6
            changed = [i for i, (a, b) in enumerate(zip(old['actions'], new['actions'])) if a != b]
            assert changed == [seat*3+2], changed
            old_weapon, new_weapon = old['actions'][changed[0]], new['actions'][changed[0]]
            # Scenario actions are externally tagged; only the laser payload byte changes.
            before, after = old_weapon['Scenario'], new_weapon['Scenario']
            assert before['kind'] == after['kind']
            assert before['payload'][0] == after['payload'][0] == seat
            assert before['payload'][2] == after['payload'][2]
            assert before['payload'][1] != after['payload'][1]
            return {'tick': old['tick'], 'before': old_weapon, 'after': new_weapon}
    assert compared == control['end_tick'] == candidate['end_tick'], 'incomplete action trace'
    assert control['outcome'] == candidate['outcome']
    return None


def compare(control, candidate):
    assert control['runtime']['binary_sha256'] == candidate['runtime']['binary_sha256']
    assert len(control['cases']) == len(candidate['cases'])
    totals = [Counter(), Counter()]
    pairs = []
    for old, new in zip(control['cases'], candidate['cases']):
        assert not old['case']['direct_laser_window'] and new['case']['direct_laser_window']
        for key in ['seed', 'shape', 'slumping', 'candidate_seat', 'asteroid_interval_seconds']:
            assert old['case'][key] == new['case'][key]
        first = first_change(old, new)
        seat = old['case']['candidate_seat']
        a, b = old['seats'][seat], new['seats'][seat]
        for total, result in zip(totals, [a, b]):
            for key in ['points', 'ships_lost', 'completed_sorties', 'completed_recoveries']:
                total[key] += result[key]
            total['pilot_deaths'] += result['pilot_death_tick'] is not None
            total['laser_hit_ticks'] += result['weapons']['laser_hit_ticks']
            total['cannon_hits'] += result['weapons']['cannon_hits']
            total['short_climb_engage_climb'] += len(result['short_climb_engage_climb'])
            total.update(result['counts'])
        pairs.append({'case': old['case'], 'first_input_change': first,
                      'control': a, 'candidate': b,
                      'end_ticks': [old['end_tick'], new['end_tick']],
                      'lost_win': a['points'] == 1 and b['points'] < 1})
    a, b = totals
    screen = {'points_not_lower': b['points'] >= a['points'],
              'pilot_deaths_not_higher': b['pilot_deaths'] <= a['pilot_deaths'],
              'ship_losses_not_higher': b['ships_lost'] <= a['ships_lost'],
              'demonstrated_defect_exercised': a['declined_direct_window_ticks'] > 0
              and b['eligible_direct_window_ticks'] > 0,
              'eligible_declined_shots_eliminated': b['declined_direct_window_ticks'] == 0}
    return {'control_totals': dict(a), 'candidate_totals': dict(b), 'screen': screen,
            'decision': 'advance_to_review' if all(screen.values()) else 'retain_default',
            'pairs': pairs}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('control', type=Path)
    parser.add_argument('--candidate', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    report = {'analyzer_sha256': digest(Path(__file__)), 'control': analyze_root(args.control)}
    if args.candidate:
        report['candidate'] = analyze_root(args.candidate)
        report['comparison'] = compare(report['control'], report['candidate'])
    args.out.write_text(json.dumps(report, indent=2) + '\n')
    if args.candidate:
        result = report['comparison']
        print(json.dumps({k: result[k] for k in
                          ['control_totals', 'candidate_totals', 'screen', 'decision']}, indent=2))


if __name__ == '__main__':
    main()
