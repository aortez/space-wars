#!/usr/bin/env python3
"""Frozen option-off qualification and paired airborne acquisition defense screen."""
import argparse
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
import copy
import csv
import importlib.util
from itertools import islice, product, zip_longest
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('climb_comparison', Path(__file__).with_name('compare-pursuit-climb-laser.py'))
B = importlib.util.module_from_spec(spec)
spec.loader.exec_module(B)
L, I, P, D, ROOT = B.L, B.I, B.P, B.D, B.ROOT
FIELD = 'acquisition_defense'
FLAG = '--acquisition-defense-seats'
PROFILE = 'airborne_acquisition_defense_v1'
NAMESPACE = PROFILE + ':held-out:2026-10'
FRESH = ROOT / 'target/pursuit-climb-laser/fresh-v1/summary.json'
QUALIFIED = ROOT / 'target/pursuit-climb-laser/qualification-v2/summary.json'
KNOWN = (
    ('known-lost-win', FRESH, 'fresh-world2-v10-asteroids0-p1-candidate-on'),
    ('known-laser-off-win', FRESH, 'fresh-world2-v10-asteroids0-p1-candidate-off'),
    ('known-rescued-draw', QUALIFIED, 'candidate-laser'),
    ('known-control-win', QUALIFIED, 'control-laser'),
)
IMMUTABLE = ('started_tick', 'deadline_tick', 'planet', 'vehicle', 'opponent', 'hit_source',
             'capture', 'native_actions', 'direction', 'estimated_min_range', 'estimated_clearance')


def sources():
    return {group: dict(summary=str(path), run=name, record=json.loads(path.read_text())['runs'][name])
            for group, path, name in KNOWN}


def cases(priors):
    result = []
    for group, _, _ in KNOWN:
        old = priors[group]['record']
        laser = old['command'][old['command'].index(L.FLAG) + 1] != 'none'
        for enabled in (False, True):
            item = dict(old['item'], group=group, stage='known_qualification', laser=laser,
                        defense=enabled, world_cluster=group)
            item['name'] = group + ('-on' if enabled else '-off')
            result.append(item)
    for world, interval, seat in product(range(2), (0, 3), (0, 1)):
        group = f'fresh-world{world}-v10-asteroids{interval}-p{seat+1}'
        namespace = f'{NAMESPACE}:{world}'
        for enabled in ((False, True) if (world + interval + seat) % 2 == 0 else (True, False)):
            result.append(dict(name=group + ('-on' if enabled else '-off'), group=group,
                stage='held_out', arm='candidate', world='generated', seed=P.seed(namespace),
                seed_namespace=namespace, world_cluster=world, interval=interval, seat=seat,
                opponent=10, seconds=600, laser=True, defense=enabled))
    return result


def command(old, binary, root, seat, enabled):
    cmd = list(old)
    assert len(cmd) % 2 == 1 and len(cmd[1::2]) == len(set(cmd[1::2]))
    assert FLAG not in cmd
    cmd[0] = str(binary)
    cmd[cmd.index('--out') + 1] = str(root)
    return cmd + [FLAG, str(seat) if enabled else 'none']


def jobs(binary, out, priors):
    result = []
    for item in cases(priors):
        if item['stage'] == 'known_qualification':
            old = priors[item['group']]['record']['command']
        else:
            old = [str(binary), *(v for kv in B.flags(item).items() for v in kv), '--out', 'unused']
        result.append(dict(item=item, command=command(old, binary, out / 'raw' / item['name'],
                                                     item['seat'], item['defense'])))
    return result


def inputs():
    result = B.tool_inputs()
    for path in ('tools/validate-acquisition-defense.py', 'tools/tests/test_acquisition_defense.py',
                 'docs/acquisition-defense-plan.md'):
        result[path] = P.digest(ROOT / path)
    return result


def strip(value):
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items() if k not in (FIELD, FIELD + '_model')}
    if isinstance(value, list):
        return [strip(v) for v in value]
    return value


def replay_parity(prior, run, oldroot):
    root = L.root_of(run)
    assert set(prior['hashes']) == set(run['hashes'])
    exact = sorted(set(prior['hashes']) - {'report.json', 'sensors.jsonl', 'live-planning.csv'})
    for name in exact:
        assert prior['hashes'][name] == run['hashes'][name], name
    before, after = [json.loads((r / 'report.json').read_text()) for r in (oldroot, root)]
    assert FIELD not in after
    assert D.timing_free(before) == D.timing_free(after), 'disabled report changed'
    sensors = D.compare_jsonl(oldroot / 'sensors.jsonl', root / 'sensors.jsonl')
    with (oldroot / 'live-planning.csv').open() as a, (root / 'live-planning.csv').open() as b:
        left, right = [[D.timing_free(r) for r in csv.DictReader(f)] for f in (a, b)]
    assert left == right
    retained_results(prior, run)
    return dict(exact_streams=exact, sensor_rows=sensors, planning_rows=len(left),
                non_timing_report=True, physical_and_planning_results=True)


def retained_results(prior, run):
    # Live audit counters use integer seat keys; persisted JSON uses strings.
    # Compare their wire representation without dropping any fields or counts.
    for key in ('players', 'allocation', 'continuation', 'route_summary', 'prediction_outcomes', 'retry'):
        assert prior[key] == json.loads(json.dumps(run[key], allow_nan=False)), key


def prefix(before, after, first):
    count = 0
    for a, b in zip_longest(before, after):
        assert a is not None and b is not None, 'ending changed before handoff'
        assert (a['tick'], a['seat']) == (b['tick'], b['seat'])
        if first == (b['tick'], b['seat']):
            attempt = b['mission'][FIELD]['last']
            assert attempt['started_tick'] == b['tick']
            assert a['observation'] == b['observation']
            assert a['actions'] == attempt['native_actions']
            assert a['mission']['capture'] == attempt['capture']
            return dict(tick=b['tick'], seat=b['seat'], exact_prefix_rows=count,
                        native_actions=a['actions'], issued_actions=b['actions'],
                        source_observation_and_capture_equal=True)
        assert a == strip(b), 'state changed before handoff'
        count += 1
    assert first is None
    return dict(tick=None, exact_prefix_rows=count)


def xy(v):
    return v['x'], v['y']


def sub(a, b):
    return tuple(x - y for x, y in zip(xy(a), xy(b)))


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def unit(v):
    n = math.hypot(*v)
    return tuple(x / n for x in v) if n > 1.e-8 else (0., 0.)


def stopping(o, p):
    outward = unit(sub(p['ship']['position'], o['boundary']['center']))
    closing = max(0., dot(xy(p['ship']['velocity']), outward))
    limits = o['local']['combat']['recovery']['flight']['flight']['limits']
    braking = max(5., limits['brake_acceleration'] * .5 - max(0., dot(xy(p['gravity']), outward)))
    return (o['boundary']['radius'] - math.dist(xy(p['ship']['position']), xy(o['boundary']['center']))
            - 20. - closing * .6 - closing * closing / (2. * braking))


def audit_start(o, a, committed):
    c = o['local']['combat']; f = c['recovery']['flight']; p = f['pilot']; t = c['target']
    tick = p['tick']; capture = a['capture']; receipt = capture['acquisition']
    assert a['started_tick'] == tick and a['deadline_tick'] == tick + 720
    assert a['finished_tick'] is None and a['clear_since'] is None
    assert o['match_rules'] and p['controls_armed'] and p['queries_ready'] and f['flight']['enabled']
    assert p['ship_available'] and p['ship_form'] == 'ship' and isinstance(p['location'], dict)
    assert p['location']['aboard'] == a['vehicle'] == p['vehicle']
    assert p['landing']['phase'] == 'flying' and p['landing']['supported_feet'] == 0
    assert c['weapons']['last_hit_taken_tick'] == tick
    assert c['weapons']['last_hit_source'] == a['hit_source'] and a['hit_source'] in ('laser', 'cannon')
    assert t and t['visible'] and not t['ground_occluded'] and t['owner'] != p['owner']
    assert t['owner'] == a['opponent'] and t['ship_form'] == 'ship' and t['health'] > 0
    assert math.dist(xy(t['motion']['position']), xy(p['ship']['position'])) < 300.001
    assert capture['goal'] == 'survey' and capture['site'] is None
    assert capture['started_tick'] is not None and capture['started_tick'] <= tick
    assert capture['started_tick'] not in committed
    assert capture['failed_tick'] is None and capture['completed_tick'] is None
    assert capture['objective_route'] is None
    assert all(capture['landing'][k] is None for k in ('landed_tick', 'claimed_tick', 'boarded_tick'))
    assert (receipt['tick'], receipt['planet'], receipt['revision']) == (tick, p['planet']['index'], p['planet']['revision'])
    assert a['planet'] == p['planet']['index'] and receipt['selected_site'] is None
    assert receipt['survey_rejected_by'] is None and receipt['reason'] in ('scan_deferred', 'candidates_rejected')
    assert abs(math.hypot(*xy(a['direction'])) - 1.) < 1.e-5
    assert all(math.isfinite(a[k]) for k in ('estimated_min_range', 'estimated_clearance'))


def audit_step(trace, a, previous, committed):
    o, m = trace['observation'], trace['mission']
    c = o['local']['combat']; f = c['recovery']['flight']; p = f['pilot']; tick = trace['tick']
    assert a['observed_tick'] == tick
    if previous is None:
        audit_start(o, a, committed)
        previous_counts = dict.fromkeys(('controlled_ticks', 'laser_ticks', 'cannon_ticks'), 0)
    else:
        assert previous['finished_tick'] is None and previous['observed_tick'] + 1 == tick
        assert all(a[k] == previous[k] for k in IMMUTABLE)
        previous_counts = previous
        if a['reason'] not in ('recovery required', 'surface task takes priority', 'defense deadline', 'source planet unavailable'):
            planet = next(v for v in o['planets'] if v['index'] == a['planet'])
            clearance = math.dist(xy(p['ship']['position']), xy(planet['motion']['position'])) - planet['radius']
            assert math.isclose(clearance, a['source_clearance'], abs_tol=.003)
            t = c['target']
            if not (t and t['owner'] == a['opponent'] and t['ship_form'] == 'ship' and t['health'] > 0):
                t = None
            distance = opening = None
            if t:
                delta = sub(p['ship']['position'], t['motion']['position'])
                distance = math.hypot(*delta)
                opening = dot(sub(p['ship']['velocity'], t['motion']['velocity']), unit(delta))
            for key, value in (('range', distance), ('opening_speed', opening)):
                assert (value is None) == (a[key] is None)
                if value is not None:
                    assert math.isclose(value, a[key], abs_tol=.003)
            clear = (p['controls_armed'] and p['queries_ready'] and f['flight']['enabled']
                     and clearance > 70. and stopping(o, p) > 20. and t
                     and (t['ground_occluded'] or distance >= 350. and opening >= 0.))
            since = (previous['clear_since'] if previous['clear_since'] is not None else tick) if clear else None
            assert a['clear_since'] == since
            assert (a['reason'] == 'separation established') == (since is not None and tick - since >= 60)
    controlled = a['guidance'] in ('climb', 'escape', 'boundary')
    laser = cannon = 0
    if controlled:
        assert a['finished_tick'] is None and tick < a['deadline_tick']
        assert m['goal'] == 'disengage' and m['capture'] is None and m['recovery'] is None
        assert p['controls_armed'] and p['queries_ready'] and f['flight']['enabled']
        assert p['vehicle'] == a['vehicle'] and p['ship_available'] and p['ship_form'] == 'ship'
        assert p['landing']['supported_feet'] == 0
        payload = trace['actions'][0]['Scenario']['payload']
        assert payload[5] == 0 and payload[7] == trace['seat']
        if a['guidance'] == 'boundary':
            assert a['boundary']['active'] and payload[6] == 1
            assert trace['actions'][1]['Scenario']['payload'][1] == 0
        weapons = L.T.weapon_action(trace); laser, cannon = weapons['laser'], weapons['cannon']
        combat = m['combat']; assert combat is not None
        if combat['breaks']['active_until_tick'] is not None and tick < combat['breaks']['active_until_tick']:
            assert not laser and not cannon
        if laser or cannon:
            t = c['target']; assert t and t['visible'] and not t['ground_occluded']
            assert combat['goal'] == 'engage ship'
            geometry = L.T.relative_geometry(p['ship'], t['motion'])
            assert abs(geometry['aim_error_radians']) < .08001
            assert not laser or c['laser_available'] and geometry['range'] <= 250.001
            assert not cannon or c['cannon_ready'] and 19.999 <= geometry['range'] <= 220.001
    for key, increment in (('controlled_ticks', controlled), ('laser_ticks', laser), ('cannon_ticks', cannon)):
        assert a[key] == previous_counts[key] + int(increment), key
    if a['finished_tick'] is None:
        assert a['reason'] is None and tick < a['deadline_tick'] and m['pursuit'] is None
    else:
        assert a['finished_tick'] == tick and a['guidance'] == 'finished'
        reason = a['reason']
        if reason == 'defense deadline':
            assert tick == a['deadline_tick']
        elif reason == 'recovery required':
            assert (not p['ship_available'] or p['ship_form'] != 'ship' or p['vehicle'] != a['vehicle']
                    or p['location'] == 'on_foot' or m['recovery'] is not None)
        elif reason == 'surface task takes priority':
            assert not o['match_rules'] or m['capture'] is not None or p['landing']['supported_feet'] > 0 or p['landing']['phase'] == 'landed'
        elif reason == 'source planet unavailable':
            assert not any(v['index'] == a['planet'] for v in o['planets'])
        else:
            assert reason == 'separation established'
    return controlled


def audit(root, item, report):
    enabled, seat = item['defense'], item['seat']
    expected = dict(profile=PROFILE, enabled_seats=[s == seat for s in (0, 1)]) if enabled else None
    assert report.get(FIELD) == expected
    for s, descriptor in enumerate(report['policy_configuration']):
        assert descriptor.get(FIELD + '_model') == (PROFILE if enabled and s == seat else None)
    laser_expected = dict(profile=L.PROFILE, enabled_seats=[s == seat for s in (0, 1)]) if item['laser'] else None
    if laser_expected:
        assert all(report[L.FIELD][k] == v for k, v in laser_expected.items())
    else:
        assert L.FIELD not in report
    for s, descriptor in enumerate(report['policy_configuration']):
        assert descriptor.get(L.FIELD + '_model') == (L.PROFILE if item['laser'] and s == seat else None)
    attempts, witnesses, committed = {}, [], set()
    guidance, endings, laser_decisions = Counter(), Counter(), Counter()
    state = dict(attempts=0, separated=0, timed_out=0, last=None)
    laser_state = dict(checks=0, requested_ticks=0, last=None)
    total = 0
    for index, (row, trace) in enumerate(zip_longest(D.rows(root / 'capture-evidence.jsonl'), D.rows(root / 'trace.jsonl'))):
        assert row is not None and trace is not None
        tick, actor = index // 2, index % 2
        o, m = trace['observation'], trace['mission']; p = o['local']['combat']['recovery']['flight']['pilot']
        assert (trace['tick'], trace['seat']) == (row['pilot']['tick'], row['seat']) == (tick, actor)
        assert p['tick'] == tick and p['owner'] == f'player_{actor+1}'
        assert row['pilot'] == {k: v for k, v in p.items() if k != 'sites'}
        assert row['actions'] == trace['actions']
        controlled = False
        if actor == seat:
            capture = m['capture']
            if capture and (capture['site'] is not None or p['location'] == 'on_foot' or p['landing']['supported_feet'] > 0):
                committed.add(capture['started_tick'])
        if not enabled or actor != seat:
            assert FIELD not in row and FIELD not in m
        else:
            a = m[FIELD]['last']
            current = a is not None and a['observed_tick'] == tick
            assert (FIELD in row) == current
            if current:
                w = row[FIELD]
                assert w['observation'] == o and w['telemetry'] == m[FIELD]
                assert w['pursuit'] == m['pursuit'] and w['combat'] == m['combat']
                prior = attempts.get(a['started_tick'])
                if prior is None:
                    assert state['last'] is None or state['last']['finished_tick'] is not None
                    state['attempts'] += 1
                controlled = audit_step(trace, a, prior, committed)
                attempts[a['started_tick']] = a
                guidance[a['guidance']] += 1
                if a['finished_tick'] is not None:
                    endings[a['reason']] += 1
                    state['separated'] += int(a['reason'] == 'separation established')
                    state['timed_out'] += int(a['reason'] == 'defense deadline')
                state['last'] = a
                witnesses.append(row)
            else:
                assert state['last'] is None or state['last']['finished_tick'] is not None, 'missing active row'
            assert m[FIELD] == state
            if a and a['started_tick'] <= tick < a['deadline_tick']:
                assert m['pursuit'] is None, 'new pursuit before original deadline'
        if not item['laser'] or actor != seat:
            assert L.FIELD not in row and L.FIELD not in m
        else:
            gate = m[L.FIELD]
            current = gate['last'] is not None and gate['last']['tick'] == tick
            assert (L.FIELD in row) == current
            # Defensive flight uses native combat weapons; the separate climb
            # hook remains limited to pursuit (Hunt/Watch).
            selected = (m['goal'] in ('hunt', 'watch') and
                        (m['reason'] == 'climbing for a firing pass' or (m['combat'] or {}).get('goal') == 'climb clear of ground'))
            assert current == selected and not (current and controlled)
            if current:
                check = B.audit_watch_check if m['goal'] == 'watch' else L.audit_check
                laser_state = check(row, trace, laser_state)
                laser_decisions[laser_state['last']['decision']] += 1
            assert gate == laser_state
        if actor == seat and (m['combat'] or {}).get('goal', '').startswith('flyby /'):
            assert L.T.weapon_action(row) == dict(laser=False, cannon=False)
        total += 1
    assert total == 2 * report['elapsed_ticks'] and report['dense_trace_ticks'] == [0, D.END]
    for actor, mission in enumerate(report['missions']):
        assert mission.get(FIELD) == (state if enabled and actor == seat else None)
        assert mission.get(L.FIELD) == (laser_state if item['laser'] and actor == seat else None)
    result = dict(enabled=enabled, counters=state if enabled else None, attempts=list(attempts.values()),
                  guidance=dict(guidance), endings=dict(endings), dense_rows=total,
                  first_tick=min(attempts, default=None), laser=laser_state if item['laser'] else None,
                  laser_decisions=dict(laser_decisions))
    I.write(root / 'defense-audit.json', dict(summary=result, witnesses=witnesses))
    return result


def compare(pair, runs):
    a, b = [next(runs[j['item']['name']] for j in pair if j['item']['defense'] == enabled) for enabled in (False, True)]
    roots = [L.root_of(r) for r in (a, b)]
    reports = [json.loads((r / 'report.json').read_text()) for r in roots]
    assert reports[0]['initial_world'] == reports[1]['initial_world']
    tick, seat = b['defense']['first_tick'], b['item']['seat']
    difference = prefix(*(D.rows(r / 'trace.jsonl') for r in roots), (tick, seat) if tick is not None else None)
    control_difference = I.first_control_difference(*(r / 'destination-behavior.jsonl' for r in roots))
    if control_difference:
        assert control_difference['reason'] == 'actions'
        assert tick is not None and control_difference['tick'] >= tick
    else:
        for key in ('round', 'final_pilots', 'final_planets', 'final_audit', 'final_combat', 'elapsed_ticks'):
            assert reports[0][key] == reports[1][key], 'physical change without changed actions'
    if tick is None:
        assert D.timing_free(reports[0]) == D.timing_free(strip(reports[1]))
        for name in a['hashes']:
            if name in ('trace.jsonl', 'capture-evidence.jsonl'):
                for left, right in zip_longest(*(D.rows(r / name) for r in roots)):
                    assert left is not None and right is not None and left == strip(right)
            elif name == 'sensors.jsonl':
                D.compare_jsonl(*(r / name for r in roots))
            elif name == 'live-planning.csv':
                with (roots[0] / name).open() as left, (roots[1] / name).open() as right:
                    assert [D.timing_free(x) for x in csv.DictReader(left)] == [D.timing_free(x) for x in csv.DictReader(right)]
            elif name != 'report.json':
                assert a['hashes'][name] == b['hashes'][name], name
        assert a['players'] == b['players'] and a['allocation'] == b['allocation']
    x, y = [r['players'][seat] for r in (a, b)]
    useful, missing = I.completion_changes(x, y, control_difference['tick'] if control_difference else None)
    benefits = []
    if B.POINTS[y['outcome']] > B.POINTS[x['outcome']]: benefits.append('more_match_points')
    for key in ('ships_lost', 'pilot_deaths'):
        if y[key] < x[key]: benefits.append('fewer_' + key)
    if useful: benefits.append('earlier_or_additional_departure')
    if control_difference is None: assert not benefits
    horizon = min(a['elapsed_ticks'], b['elapsed_ticks'])
    common = {arm: [v for v in r['players'][seat]['visits'] if v['departed_tick'] is not None and v['departed_tick'] <= horizon]
              for arm, r in (('off', a), ('on', b))}
    censored = [v for v in x['visits'] if y['outcome'] == 'win' and b['elapsed_ticks'] < a['elapsed_ticks']
                and v['departed_tick'] is not None and v['departed_tick'] > horizon]
    return dict(group=a['item']['group'], stage=a['item']['stage'], seat=seat,
        configuration=a['item']['arm'], world_cluster=a['item']['world_cluster'], interval=a['item']['interval'],
        off=a['item']['name'], on=b['item']['name'], first_handoff=difference,
        first_control_difference=control_difference, benefits=benefits,
        outcome_transition=x['outcome'] + '->' + y['outcome'], useful_completed_changes=useful,
        missing_off_completions=missing, common_horizon=horizon, common_horizon_completed_visits=common,
        off_visits_after_enabled_early_victory=censored)


def screen(runs, pairs):
    assert len(runs) == 24 and len(pairs) == 12
    known = [p for p in pairs if p['stage'] == 'known_qualification']
    fresh = [p for p in pairs if p['stage'] == 'held_out']
    assert len(known) == 4 and len(fresh) == 8
    def totals(selected):
        return {arm: I.totals([runs[p[arm]]['players'][p['seat']] for p in selected]) for arm in ('off', 'on')}
    reasons, regressions = [], []
    target = next(p for p in known if p['group'] == 'known-lost-win')
    if not any(b != 'earlier_or_additional_departure' for b in target['benefits']):
        reasons.append('diagnosed_loss_not_improved')
    for p in pairs:
        x, y = [runs[p[arm]]['players'][p['seat']] for arm in ('off', 'on')]
        regressed = B.POINTS[y['outcome']] < B.POINTS[x['outcome']] or any(y[k] > x[k] for k in ('ships_lost', 'pilot_deaths'))
        if regressed:
            regressions.append(p['group'])
            if p in known: reasons.append('known_regression:' + p['group'])
    if not any(p['benefits'] for p in fresh): reasons.append('no_useful_fresh_change')
    aggregate = totals(fresh)
    strata = {'overall': aggregate}
    for key in ('seat', 'interval', 'world_cluster'):
        for value in sorted({p[key] for p in fresh}):
            strata[f'{key}:{value}'] = totals([p for p in fresh if p[key] == value])
    for key, value in strata.items():
        if value['on']['points'] < value['off']['points']: reasons.append('fresh_points_regressed:' + key)
    for key in ('ships_lost', 'pilot_deaths'):
        if aggregate['on'][key] > aggregate['off'][key]: reasons.append('fresh_' + key + '_increased')
    return dict(decision='retain' if reasons else 'advance_to_broader_comparison', reasons=reasons,
                default_promotion=False, fresh_world_clusters=2, fresh_totals=aggregate,
                fresh_strata=strata, regression_pairs=regressions,
                changed_pairs=sum(p['first_handoff']['tick'] is not None for p in pairs),
                fresh_changed_pairs=sum(p['first_handoff']['tick'] is not None for p in fresh))


def audit_impact(root, report):
    """The inherited partial window cannot establish losses before it begins."""
    traces = islice(D.rows(root / 'trace.jsonl'), D.IMPACT_START * 2, None)
    count, final, initial_losses, previous_losses, losses = 0, None, {}, {}, []
    for row in D.rows(root / 'impact.jsonl'):
        if 'final_tick' in row:
            assert final is None
            final = row
            continue
        assert final is None
        trace = next(traces)
        tick, seat = D.IMPACT_START + count // 2, count % 2
        assert (row['tick'], row['seat']) == (trace['tick'], trace['seat']) == (tick, seat)
        assert not row['overridden'] and row['controls'] == row['bot_controls']
        assert row['actions'] == trace['actions'] and row['goal'] == trace['mission']['goal']
        assert {k: row['controls'][k] for k in trace['controls']} == trace['controls']
        p = trace['observation']['local']['combat']['recovery']['flight']['pilot']
        assert (row['form'], row['ship'], row['location']) == (p['ship_form'], p['ship'], p['location'])
        lost = (p['recovery'] or {}).get('ships_lost', 0)
        initial_losses.setdefault(seat, lost)
        previous = previous_losses.get(seat, lost)
        assert lost in (previous, previous + 1)
        if lost > previous: losses.append(dict(seat=seat, receipt=D.loss_receipt(row, tick)))
        previous_losses[seat] = lost
        count += 1
    ticks = report['elapsed_ticks']
    assert count == max(0, ticks - D.IMPACT_START) * 2 and next(traces, None) is None
    assert final['final_tick'] == ticks and final['round'] == report['round']
    assert final['config'] == dict(seat=0, control='bot', control_from_tick=0,
                                 trace_start_tick=D.IMPACT_START, trace_end_tick=D.END)
    for seat in (0, 1):
        lost = (report['final_pilots'][seat]['recovery'] or {}).get('ships_lost', 0)
        if seat not in previous_losses:
            initial_losses[seat] = lost
            continue
        previous = previous_losses[seat]
        assert lost in (previous, previous + 1)
        if lost > previous:
            losses.append(dict(seat=seat, receipt=D.loss_receipt(dict(tick=ticks, damage=final['damage'][seat]), ticks)))
        assert sum(v['seat'] == seat for v in losses) + initial_losses[seat] == lost
    return dict(rows=count, overrides=0, losses_before_window=initial_losses, losses=losses, final=final)


def run_case(job, plan, previous=None):
    result = copy.deepcopy(job); root = L.root_of(job); out = root.parent.parent
    log = out / 'logs' / (root.name + '.log')
    try:
        if previous is None:
            assert not root.exists()
            with log.open('x') as stream:
                subprocess.run(job['command'], stdout=stream, stderr=stream, check=True, timeout=1800)
            result['hashes'] = I.raw_hashes(root)
        else:
            assert previous['item'] == job['item'] and previous['command'] == job['command']
            assert P.digest(log) == previous['log_sha256']
            if not root.exists(): B.unpack(previous['archive'], root)
            assert all(P.digest(root / k) == v for k, v in previous['hashes'].items())
            result['hashes'] = previous['hashes']
            result['reused_raw'] = True
        result.update(I.analyze(root, job, root))
        report = json.loads((root / 'report.json').read_text())
        result['defense'] = audit(root, job['item'], report)
        if (root / 'impact.jsonl').exists():
            result['impact'] = audit_impact(root, report)
        if job['item']['stage'] == 'known_qualification' and not job['item']['defense']:
            prior = plan['sources'][job['item']['group']]['record']
            extracted = 'archive' in prior
            oldroot = out / 'inputs' / job['item']['group'] if extracted else L.root_of(prior)
            if extracted and not oldroot.exists(): B.unpack(prior['archive'], oldroot)
            assert all(P.digest(oldroot / name) == digest for name, digest in prior['hashes'].items())
            result['retention'] = replay_parity(prior, result, oldroot)
            if extracted: shutil.rmtree(oldroot)
        assert all(P.digest(root / name) == digest for name, digest in result['hashes'].items())
        result['audited'] = True
    except Exception:
        result['error'] = traceback.format_exc()
    if log.exists(): result['log_sha256'] = P.digest(log)
    return result


def freeze(out):
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip(), 'commit the plan/runtime first'
    priors = sources()
    fresh_seeds = {c['seed'] for c in cases(priors) if c['stage'] == 'held_out'}
    old_seeds = {c['seed'] for c in B.cases()} | {c['seed'] for c in P.plan()}
    assert not fresh_seeds & old_seeds
    out.mkdir(parents=True, exist_ok=False)
    for name in ('raw', 'logs', 'archives', 'inputs'): (out / name).mkdir()
    binary = out / 'surface_mission_soak'
    shutil.copy2(ROOT / 'target/release/examples/surface_mission_soak', binary)
    binary.chmod(0o555)
    plan = dict(schema=1, profile=PROFILE, namespace=NAMESPACE,
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        binary=dict(path=str(binary), sha256=P.digest(binary)), sources=priors,
        source_hashes={str(p): P.digest(p) for p in (FRESH, QUALIFIED)}, inputs=inputs(),
        jobs=jobs(binary, out, priors), default_changes=False)
    I.write(out / 'plan.json', plan)
    print('Frozen 24 games / 12 pairs', flush=True)


def execute(path, reaudit=False):
    plan = json.loads(path.read_text()); out = path.parent
    assert plan['profile'] == PROFILE and plan['namespace'] == NAMESPACE
    current_inputs = inputs()
    if reaudit:
        allowed = {'tools/validate-acquisition-defense.py', 'tools/tests/test_acquisition_defense.py'}
        assert set(plan['inputs']) == set(current_inputs)
        assert all(v == current_inputs[k] for k, v in plan['inputs'].items() if k not in allowed)
    else:
        assert plan['inputs'] == current_inputs
    assert P.digest(plan['binary']['path']) == plan['binary']['sha256']
    assert all(P.digest(k) == v for k, v in plan['source_hashes'].items())
    assert plan['jobs'] == jobs(Path(plan['binary']['path']), out, plan['sources'])
    summary_path = out / 'summary.json'
    cached = {}
    if summary_path.exists():
        summary = json.loads(summary_path.read_text())
        assert summary['plan_sha256'] == P.digest(path)
        if reaudit:
            revision = P.digest(summary_path)
            backup = out / ('summary-before-reaudit-' + revision[:12] + '.json')
            assert not backup.exists()
            shutil.copy2(summary_path, backup)
            cached = summary['runs']
            # A second concurrent game may finish after another audit fails.
            # Bind its native outputs and log before re-auditing, never rerun it.
            for job in plan['jobs']:
                name = job['item']['name']; root = L.root_of(job)
                if name not in cached and root.exists():
                    assert (root / 'report.json').exists(), 'incomplete native game'
                    raw = {p.name: P.digest(p) for p in root.iterdir()
                           if p.is_file() and not p.name.endswith('-audit.json')}
                    cached[name] = dict(job, hashes=raw,
                        log_sha256=P.digest(out / 'logs' / (name + '.log')))
            receipt = out / ('reaudit-inputs-' + revision[:12] + '.json')
            I.write(receipt, dict(prior_summary=dict(path=str(backup), sha256=revision),
                                 current_inputs=current_inputs, cached=cached))
            summary = dict(summary, complete=False, inputs=current_inputs, runs={}, pairs=[],
                           reaudit_receipt=dict(path=str(receipt), sha256=P.digest(receipt)))
            summary.pop('error', None)
            summary.pop('screen', None)
        else:
            assert not summary.get('error')
            assert all(r.get('audited') and 'error' not in r for r in summary['runs'].values())
        for r in list(summary['runs'].values()) + list(cached.values()):
            if 'archive' in r: B.verify_archive(r['archive'])
            else: assert all(P.digest(L.root_of(r) / k) == v for k, v in r['hashes'].items())
    else:
        summary = dict(schema=1, profile=PROFILE, complete=False, plan_sha256=P.digest(path),
                       binary=plan['binary'], source_commit=plan['source_commit'], inputs=current_inputs, runs={}, pairs=[])
    def save(): I.write(summary_path, summary)
    def run_batch(pool, batch):
        pending = {j['item']['name']: pool.submit(run_case, j, plan, cached.get(j['item']['name']))
                   for j in batch if j['item']['name'] not in summary['runs']}
        for name, future in pending.items():
            summary['runs'][name] = future.result(); save()
            assert 'error' not in summary['runs'][name], summary['runs'][name].get('error')
            print(name, 'audited', flush=True)
    save()
    try:
        with ProcessPoolExecutor(max_workers=2) as pool:
            # All four exact disabled replays precede any enabled qualification.
            known_off = [j for j in plan['jobs'] if j['item']['stage'] == 'known_qualification' and not j['item']['defense']]
            for i in range(0, len(known_off), 2): run_batch(pool, known_off[i:i+2])
            for group in dict.fromkeys(j['item']['group'] for j in plan['jobs']):
                if any(p['group'] == group for p in summary['pairs']): continue
                pair = [j for j in plan['jobs'] if j['item']['group'] == group]
                run_batch(pool, pair)
                comparison = compare(pair, summary['runs'])
                for job in pair:
                    r = summary['runs'][job['item']['name']]
                    r['archive'] = B.pack(L.root_of(r), out / 'archives' / (r['item']['name'] + '.tar.gz'))
                summary['pairs'].append(comparison); save()
                print(group, comparison['outcome_transition'], comparison['benefits'], flush=True)
        summary['screen'] = screen(summary['runs'], summary['pairs'])
        assert current_inputs == inputs() and P.digest(plan['binary']['path']) == plan['binary']['sha256']
        assert all(P.digest(k) == v for k, v in plan['source_hashes'].items())
        summary['complete'] = True
    except Exception:
        summary['error'] = traceback.format_exc()
        raise
    finally:
        save()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    sub.add_parser('plan').add_argument('--out', type=Path, required=True)
    run = sub.add_parser('run')
    run.add_argument('--plan', type=Path, required=True)
    run.add_argument('--reaudit', action='store_true', help='preserve failed audit and recheck cached games without rerunning')
    args = parser.parse_args()
    if args.action == 'plan': freeze(args.out.resolve())
    else: execute(args.plan.resolve(), args.reaudit)


if __name__ == '__main__':
    main()
