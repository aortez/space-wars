#!/usr/bin/env python3
"""Audit the frozen standard-laser clearance-handoff qualification."""
import argparse
from collections import Counter
import copy
import importlib.util
import itertools
import json
from pathlib import Path
import re


spec = importlib.util.spec_from_file_location(
    'combat', Path(__file__).with_name('analyze-combat-opportunities.py'))
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)
PROFILE = 'material_hunt_clearance_handoff_v1'


def release_reason(row):
    before = row['before']
    if row['goal'] not in ['hunt', 'watch']:
        return 'different_mission'
    if row['opponent_range'] is None:
        return 'opponent_unavailable'
    if row['opponent_range'] > 300:
        return 'opponent_left_climb_range'
    anchor = next((g for g in row['geometry'] if g['planet'] == before['climb']), None)
    if anchor is None:
        return 'anchor_unavailable'
    if (anchor['clearance'] > 140 and anchor['radial_speed'] > -4
            and all(g['clearance'] > 140 for g in row['geometry'])):
        return 'normal_clearance'
    return 'unfinished'


def clearance_event(row):
    before = row['before']
    if before['climb'] is None:
        return None
    changed_anchor = before['climb'] != row['pursuit_climb']
    boundary = ((before['pursuit'] or {}).get('started_tick')
                != (row['pursuit'] or {}).get('started_tick'))
    bookkeeping = boundary or any(e['kind'] == 'replan' for e in row['mission_events'])
    if not changed_anchor and not bookkeeping:
        return None
    reason = release_reason(row)
    continued = before['goal'] in ['hunt', 'watch'] and reason == 'unfinished'
    mission_climb = (row['reason'] == 'climbing for a firing pass'
                     and not row['combat_active'])
    local_climb = (row['reason'] is None and row['combat_active']
                   and row['combat_goal'] == 'climb clear of ground')
    return {k: copy.deepcopy(row[k]) for k in [
        'tick', 'before', 'goal', 'reason', 'pursuit', 'pursuit_climb',
        'geometry', 'opponent_range', 'ship_health', 'weapons', 'mission_events',
        'combat_active', 'combat_goal']} | {
        'release_reason': reason, 'pursuit_boundary': boundary,
        'premature_release': changed_anchor and continued,
        'retained_handoff': bookkeeping and not changed_anchor and continued and mission_climb,
        # end_pursuit/reconsider can clear the anchor and hunt can re-enter the
        # local combat climb on that same decision. Equal anchors do not imply
        # that the mission clearance controller was preserved through it.
        'reentered_combat_climb': bookkeeping and not changed_anchor and continued and local_climb,
    }


def audit_case(case):
    out = Path(case['directory']) / 'output'
    summary = A.read(out / 'summary.json')
    assert not any(summary[k] for k in [
        'native_input_override', 'native_controller_reset', 'native_physics_edited'])
    assert summary['probe_counts'] == [0, 0]
    assert summary['host_work'] == {'graph': 4, 'physics_queries': 384}
    enabled_seat = case['case']['candidate_seat'] if case['case']['hunt_clearance_handoff'] else None
    events = [[], []]
    for row in A.rows(out / 'combat.jsonl'):
        expected = PROFILE if row['seat'] == enabled_seat else None
        assert row['hunt_clearance_handoff'] == expected, 'wrong active handoff profile'
        assert row['laser_window_profile'] is None, 'standard laser required'
        event = clearance_event(row)
        if event:
            events[row['seat']].append(event)
    maximum = Counter(graph=0, physics_queries=0)
    ticks = 0
    for row in A.rows(out / 'costs.jsonl'):
        assert row['tick'] == ticks, 'missing, repeated or unordered cost row'
        ticks += 1
        for kind in maximum:
            charged = sum(stage[kind] for stage in row['work'].values())
            assert 0 <= charged <= summary['host_work'][kind], 'shared work limit exceeded'
            maximum[kind] = max(maximum[kind], charged)
    assert ticks == case['end_tick'], 'incomplete cost trace'
    for seat in range(2):
        expected = PROFILE if seat == enabled_seat else None
        assert summary['brains'][seat].get('hunt_clearance_handoff') == expected
        pilot = summary['combat'][seat]['recovery']['flight']['pilot']
        case['seats'][seat].update(
            clearance_events=events[seat],
            premature_releases=sum(e['premature_release'] for e in events[seat]),
            retained_handoffs=sum(e['retained_handoff'] for e in events[seat]),
            reentered_combat_climbs=sum(e['reentered_combat_climb'] for e in events[seat]),
            final_pilot_health=summary['match']['pilots'][seat]['health'],
            final_ship_health=pilot['ship_health'],
            final_owned_planets=summary['match']['owned_planets'][seat],
            terminal_pilot_damage=summary['match']['pilots'][seat]['last_damage'])
    case['maximum_charged_work'] = dict(maximum)
    case['hashes'].update({name: A.digest(out / name) for name in [
        'costs.jsonl', 'initial.json', 'initial-controllers.txt',
        'initial-frame-p1.json', 'initial-frame-p2.json', 'final.json',
        'final-controllers.txt', 'final-frame-p1.json', 'final-frame-p2.json']})


def normalize_snapshot(value, seat):
    value = copy.deepcopy(value)
    assert value['brains'][seat].pop('hunt_clearance_handoff') == PROFILE
    return value


def normalize_controllers(value):
    # Only the selected option differs; do not discard other controller state.
    pattern = (r'(?m)^([ \t]*)hunt_clearance_handoff: Some\(\n[ \t]*"'
               + PROFILE + r'",\n[ \t]*\),$')
    value, count = re.subn(pattern, r'\1hunt_clearance_handoff: None,', value)
    assert count == 1, 'expected exactly one enabled controller profile'
    return value


def audit_snapshots(old, new, prefix):
    paths = [Path(c['directory']) / 'output' for c in [old, new]]
    a, b = [A.read(p / f'{prefix}.json') for p in paths]
    assert a == normalize_snapshot(b, new['case']['candidate_seat'])
    a, b = [(p / f'{prefix}-controllers.txt').read_text() for p in paths]
    assert a == normalize_controllers(b), 'controller state differs beyond selected option'
    for seat in [1, 2]:
        a, b = [(p / f'{prefix}-frame-p{seat}.json').read_bytes() for p in paths]
        assert a == b, 'physical snapshot differs'


def audit_prefix(old, new, first):
    """Tie the first controller change to the fix, allowing equal encoded inputs."""
    seat = new['case']['candidate_seat']
    paths = [Path(c['directory']) / 'output' for c in [old, new]]
    actions = [A.rows(p / 'actions.jsonl') for p in paths]
    combat = [A.rows(p / 'combat.jsonl') for p in paths]
    cause = None
    first_decision = None
    ticks = old['end_tick'] if first is None else first['tick'] + 1
    for tick in range(ticks):
        for current_seat in range(2):
            a, b = [next(stream) for stream in combat]
            assert a['tick'] == b['tick'] == tick
            assert a['seat'] == b['seat'] == current_seat
            a.pop('hunt_clearance_handoff')
            b.pop('hunt_clearance_handoff')
            if a != b and cause is None:
                assert current_seat == seat, 'opponent controller changed before handoff'
                before, after = clearance_event(a), clearance_event(b)
                assert before and after, 'first controller change is not a handoff'
                assert (before['premature_release'] or before['reentered_combat_climb'])
                assert after['retained_handoff']
                assert a['before'] == b['before'] and a['geometry'] == b['geometry']
                assert a['pursuit'] == b['pursuit']
                assert a['mission_events'] == b['mission_events']
                cause = dict(tick=tick, control=before, candidate=after)
        a, b = [next(stream) for stream in actions]
        assert a['tick'] == b['tick'] == tick
        if a['normalized_decision_fnv64'] != b['normalized_decision_fnv64']:
            assert cause is not None, 'decision differs before the handoff'
            if first_decision is None:
                first_decision = tick
    if first:
        assert cause is not None and cause['tick'] <= first['tick']
    audit_snapshots(old, new, 'initial')
    if cause is None:
        assert first is None
        audit_snapshots(old, new, 'final')
    return dict(ticks_checked=ticks, first_controller_change=cause,
                first_normalized_decision_change=first_decision,
                final_snapshots_identical_except_profile=cause is None)


def frozen_matrix(plan, enabled):
    return [dict(seed=seed, shape=shape, slumping=True, candidate_seat=seat,
                 asteroid_interval_seconds=interval, probe_laser=False,
                 direct_laser_window=False, hunt_clearance_handoff=enabled)
            for seed, shape, interval, seat in itertools.product(
                plan['seeds'], plan['shapes'], plan['asteroid_intervals'], plan['seats'])]


def audit_root(root, plan, enabled):
    runtime = A.read(root / 'runtime.json')
    assert A.read(root / 'matrix.json') == frozen_matrix(plan, enabled), 'matrix differs from plan'
    assert runtime['matrix_sha256'] == A.digest(root / 'matrix.json')
    assert runtime['binary_sha256'] == A.digest(root / 'ai-tests')
    assert runtime['patch_sha256'] == A.digest(root / 'tracked-source.patch')
    assert (root / 'tracked-source.patch').read_bytes() == b'', 'experiment source was not frozen'
    assert not runtime['untracked_source_sha256'], 'uncommitted experiment source'
    assert runtime['time_limit_seconds'] == plan['match_seconds']
    assert runtime['breaks'] == dict(interval_seconds=plan['break_interval_seconds'],
                                     duration_seconds=plan['break_duration_seconds'])
    assert runtime['policy'] == 'material_mission_v13'
    assert runtime['recovery_profile'] == 'terrain_recovery_supported_hatch_ordinary_offsets_v1'
    result = A.analyze_root(root)
    for case in result['cases']:
        audit_case(case)
    return result


def compare(control, candidate):
    assert control['runtime']['source_head'] == candidate['runtime']['source_head']
    (a, b), pairs = A.paired_results(control, candidate, 'clearance-handoff')
    for old, new, pair in zip(control['cases'], candidate['cases'], pairs):
        pair['prefix_audit'] = audit_prefix(old, new, pair['first_input_change'])
        for total, result, case in zip([a, b], [pair['control'], pair['candidate']], [old, new]):
            for key in ['premature_releases', 'retained_handoffs', 'reentered_combat_climbs',
                        'final_owned_planets']:
                total[key] += result[key]
            total['elapsed_ticks'] += case['end_tick']
    exercised = [p for p in pairs if p['prefix_audit']['first_controller_change']]
    screen = dict(
        points_not_lower=b['points'] >= a['points'],
        wins_not_lower=b['wins'] >= a['wins'],
        pilot_deaths_not_higher=b['pilot_deaths'] <= a['pilot_deaths'],
        ship_losses_not_higher=b['ships_lost'] <= a['ships_lost'],
        completed_sorties_not_lower=b['completed_sorties'] >= a['completed_sorties'],
        actual_handoff_exercised=len(exercised) > 0,
        candidate_premature_releases_eliminated=b['premature_releases'] == 0)
    return dict(control_totals=dict(a), candidate_totals=dict(b), screen=screen,
                decision='advance_to_review' if all(screen.values()) else 'retain_default',
                exercised_pairs=len(exercised),
                exercised_seeds=sorted({p['case']['seed'] for p in exercised}), pairs=pairs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('control', type=Path)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    plan = A.read(args.plan)
    assert plan['schema'] == 'combat-clearance-qualification-v1'
    assert len(frozen_matrix(plan, False)) == plan['pairs']
    report = dict(audit_schema='combat-clearance-audit-v2',
                  plan=plan, plan_sha256=A.digest(args.plan),
                  analyzer_sha256=A.digest(Path(__file__)),
                  common_analyzer_sha256=A.digest(Path(A.__file__)),
                  control=audit_root(args.control, plan, False),
                  candidate=audit_root(args.candidate, plan, True))
    report['comparison'] = compare(report['control'], report['candidate'])
    args.out.write_text(json.dumps(report, indent=2) + '\n')
    result = report['comparison']
    print(json.dumps({k: result[k] for k in ['control_totals', 'candidate_totals',
        'screen', 'decision', 'exercised_pairs', 'exercised_seeds']}, indent=2))


if __name__ == '__main__':
    main()
