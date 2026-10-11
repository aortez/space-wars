#!/usr/bin/env python3
"""Audit retained transfer exposure and damage provenance without running a policy.

Reads the completed speed comparison and its immediate baseline. Relative motion
and firing eligibility describe recorded observations; neither predicts a hit.
The actual contact's spawn tick, not enemy proximity, identifies an old missile.
"""
import argparse
from collections import Counter
import gzip
import hashlib
import json
import math
from pathlib import Path
import subprocess


WEAPON_ACTION = 1398079493


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def checked_path(root, name, hashes):
    path = root / name
    assert digest(path) == hashes[name], f'changed retained input: {path}'
    return path


def vector(v):
    return v['x'], v['y']


def difference(a, b):
    return tuple(x - y for x, y in zip(vector(a), vector(b)))


def relative_geometry(ship, target):
    """World-relative range/opening and the native bounded-lead aim error.

    Python arithmetic approximates the native f32 calculation. Consumers must
    not treat a value near a firing threshold as an exact policy reconstruction.
    """
    position = difference(target['position'], ship['position'])
    velocity = difference(target['velocity'], ship['velocity'])
    distance = math.hypot(*position)
    lead = min(distance / 300., 1.)
    aim = tuple(p + v * lead for p, v in zip(position, velocity))
    angle = ship['angle']
    local = (aim[0] * math.cos(angle) + aim[1] * math.sin(angle),
             -aim[0] * math.sin(angle) + aim[1] * math.cos(angle))
    error = 0. if sum(v * v for v in local) <= 1.e-5 else math.atan2(*local)
    return dict(range=distance,
                opening_speed=sum(p * v for p, v in zip(position, velocity)) / distance
                if distance > 0. else None,
                relative_speed=math.hypot(*velocity),
                aim_error_radians=error)


def weapon_action(row):
    actions = [a['Scenario']['payload'] for a in row['actions']
               if a.get('Scenario', {}).get('kind') == WEAPON_ACTION]
    assert len(actions) == 1, 'missing or duplicate weapon action'
    payload = actions[0]
    assert len(payload) == 3 and payload[0] == row['seat']
    assert all(v in (0, 1) for v in payload[1:])
    return dict(laser=bool(payload[1]), cannon=bool(payload[2]))


def exposure(row):
    witness = row['escape_travel']
    c = witness['observation']['local']['combat']
    p = c['recovery']['flight']['pilot']
    assert p['tick'] == row['pilot']['tick']
    t = c['target']
    recent = c['weapons']['last_hit_taken_tick']
    assert recent is None or recent <= p['tick']
    result = dict(tick=p['tick'], hull=p['ship_health'], vehicle=p['vehicle'],
                  ship_form=p['ship_form'], mission_goal=row['mission']['goal'],
                  combat_goal=(witness['combat'] or {}).get('goal'),
                  last_hit_tick=recent,
                  recent_hit=recent is not None and p['tick'] - recent < 180,
                  laser_ready=c['laser_available'], cannon_ready=c['cannon_ready'],
                  action=weapon_action(row), target=None)
    if t is not None:
        result['target'] = dict(**relative_geometry(p['ship'], t['motion']),
                                visible=t['visible'], ground_occluded=t['ground_occluded'],
                                hull=t['health'], form=t['ship_form'])
    return result


def weapon_gate_failures(row, weapon):
    """Explain zero requests with independent, possibly overlapping blockers."""
    t = row['target']
    if t is None:
        return ['target_unavailable']
    failures = []
    if row['combat_goal'] != 'engage ship':
        failures.append('combat_controller_not_engaging')
    if not t['visible'] or t['ground_occluded']:
        failures.append('visibility')
    if not row[weapon + '_ready']:
        failures.append('readiness')
    if abs(t['aim_error_radians']) >= .08:
        failures.append('aim')
    if (weapon == 'laser' and t['range'] > 250.) or (
            weapon == 'cannon' and not 20. <= t['range'] <= 220.):
        failures.append('range')
    return failures


def contact_at_loss(sample, seat, loss_tick):
    """A later sample can retain an exact contact; stale contacts cannot be joined."""
    if sample['pilots'][seat]['tick'] < loss_tick:
        return None
    d = sample['damage'][seat]
    if not (d['last_damage_tick'] == d['last_contact_tick'] == loss_tick
            and d['last_source'] == d['last_contact_source'] == 'cannon'
            and d['last_ship_lost']):
        return None
    spawn = d['last_contact_spawn_tick']
    assert spawn is not None and 0 <= spawn < loss_tick
    return dict(spawn_tick=spawn, contact_tick=loss_tick,
                age_ticks=loss_tick - spawn, age_seconds=(loss_tick - spawn) / 60.,
                damage=d['last_damage_percent'], sampled_tick=sample['pilots'][seat]['tick'])


def bracketing_samples(samples, seat, start, end):
    before = [s for s in samples if s['pilots'][seat]['tick'] < start]
    inside = [s for s in samples if start <= s['pilots'][seat]['tick'] <= end]
    after = [s for s in samples if s['pilots'][seat]['tick'] > end]
    return before[-1:] + inside + after[:1]


def complete_actions(actions, actor, start, end):
    rows = [r for r in actions if r['seat'] == actor and start <= r['tick'] < end]
    assert [r['tick'] for r in rows] == list(range(start, end)), 'incomplete action coverage'
    return dict(rows=len(rows), laser_ticks=sum(r['laser'] for r in rows),
                cannon_ticks=sum(r['cannon'] for r in rows),
                goals=dict(Counter(r['goal'] for r in rows)))


def summarize_exposure(rows):
    assert rows
    targets = [r for r in rows if r['target'] is not None]
    result = dict(ticks=len(rows), first=rows[0], last=rows[-1],
                  missing_target_ticks=len(rows) - len(targets),
                  visible_ticks=sum(r['target']['visible'] for r in targets),
                  occluded_ticks=sum(r['target']['ground_occluded'] for r in targets),
                  recent_hit_ticks=sum(r['recent_hit'] for r in rows),
                  combat_goals=dict(Counter(r['combat_goal'] for r in rows)))
    result['first_closing'] = next((r for r in targets
                                   if r['target']['opening_speed'] is not None
                                   and r['target']['opening_speed'] < 0.), None)
    result['maximum_range'] = max(targets, key=lambda r: r['target']['range']) if targets else None
    result['first_inside_range'] = {str(bound): next((r for r in targets
                                                    if r['target']['range'] < bound), None)
                                    for bound in (350, 300, 260, 250, 220)}
    result['weapons'] = {}
    for weapon in ('laser', 'cannon'):
        failures = [weapon_gate_failures(r, weapon) for r in rows]
        result['weapons'][weapon] = dict(
            requested_ticks=sum(r['action'][weapon] for r in rows),
            ready_ticks=sum(r[weapon + '_ready'] for r in rows),
            approximate_eligible_ticks=sum(not f for f in failures),
            blockers=dict(Counter(reason for f in failures for reason in f)))
    return result


def analyze_run(run):
    command = run['command']
    root = Path(command[command.index('--out') + 1])
    names = ('report.json', 'escape-travel-witnesses.json', 'capture-evidence.jsonl', 'trace.jsonl')
    paths = {name: checked_path(root, name, run['hashes']) for name in names}
    report = json.loads(paths['report.json'].read_text())
    witnesses = json.loads(paths['escape-travel-witnesses.json'].read_text())['rows']
    seat = run['item']['seat']
    attempts = []
    evidence = []
    for attempt in run['escape_travel']['last_attempts']:
        start, end = attempt['started_tick'], attempt['finished_tick']
        assert end is not None, 'diagnosis requires a finished attempt'
        selected = [r for r in witnesses if r['escape_travel']['telemetry']['last']['started_tick'] == start
                    and r['escape_travel']['telemetry']['last']['guidance'] in ('transfer', 'launch')]
        rows = [exposure(r) for r in selected]
        assert len(rows) == attempt['travel_ticks']
        assert all(start <= r['tick'] < end for r in rows)
        records = [s for s in report['samples'] if contact_at_loss(s, seat, end)]
        contact = contact_at_loss(records[0], seat, end) if records else None
        before = [s for s in report['samples'] if s['pilots'][seat]['tick'] < end]
        after = [s for s in report['samples'] if s['pilots'][seat]['tick'] >= end]
        bracket = [before[-1], after[0]] if before and after else []
        attempts.append(dict(started_tick=start, finished_tick=end, reason=attempt['reason'],
                             deadline_tick=attempt['deadline_tick'], destination=attempt['destination'],
                             exposure=summarize_exposure(rows), fatal_contact=contact))
        evidence.append(dict(rows=rows, loss_sample=records[0] if records else None,
                             loss_bracket_samples=bracket, actions=[], launch_rows=[], loss_trace=[]))
    # Full action coverage prevents a sparse trace from turning "not sampled"
    # into "did not fire". Weapon counters separately verify actual launches.
    with paths['capture-evidence.jsonl'].open() as stream:
        for line in stream:
            row = json.loads(line)
            tick = row['pilot']['tick']
            for a, e in zip(attempts, evidence):
                if a['started_tick'] <= tick <= a['finished_tick']:
                    e['actions'].append(dict(tick=tick, seat=row['seat'],
                                             goal=row['mission']['goal'],
                                             hull=row['pilot']['ship_health'],
                                             form=row['pilot']['ship_form'], **weapon_action(row)))
                if a['fatal_contact'] and tick == a['fatal_contact']['spawn_tick']:
                    e['launch_rows'].append(row)
    with paths['trace.jsonl'].open() as stream:
        for line in stream:
            row = json.loads(line)
            for a, e in zip(attempts, evidence):
                if row['seat'] == seat and row['tick'] == a['finished_tick']:
                    e['loss_trace'].append(row)
    for a, e in zip(attempts, evidence):
        start, end = a['started_tick'], a['finished_tick']
        a['actions_by_seat'] = {}
        for actor in range(2):
            a['actions_by_seat'][str(actor)] = complete_actions(e['actions'], actor, start, end)
        samples = bracketing_samples(report['samples'], seat, start, end)
        e['launch_counter_samples'] = samples
        a['launch_counter_samples'] = [dict(tick=s['pilots'][seat]['tick'],
                                           fired=[c['shells_fired'] for c in s['combat']],
                                           hits=[c['cannon_hits'] for c in s['combat']]) for s in samples]
        if a['fatal_contact']:
            assert len(e['loss_trace']) == 1
            weapons = e['loss_trace'][0]['observation']['local']['combat']['weapons']
            assert weapons['last_hit_source'] == 'cannon' and weapons['last_hit_taken_tick'] == end
            shooters = [r['seat'] for r in e['launch_rows'] if weapon_action(r)['cannon']]
            a['fatal_contact']['cannon_request_seats_at_spawn'] = shooters
            # A command and the retained spawn receipt support attribution;
            # no trajectory or repeated-contact identity is invented here.
    return dict(inputs={str(path): run['hashes'][name] for name, path in paths.items()},
                seat=seat, attempts=attempts), evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True, help='new JSON manifest; archive uses .json.gz')
    args = parser.parse_args()
    archive = args.out.with_suffix(args.out.suffix + '.gz')
    assert not args.out.exists() and not archive.exists(), 'preserve prior evidence'
    current = json.loads(args.summary.read_text())
    prior_path = Path(current['prior_summary']['path'])
    assert digest(prior_path) == current['prior_summary']['sha256']
    prior = json.loads(prior_path.read_text())
    assert current['complete'] and prior['complete']
    sources = {str(p): digest(p) for p in (args.summary, prior_path)}
    runs, evidence = {}, {}
    for name, run in current['runs'].items():
        if not run['escape_travel']['last_attempts']:
            continue
        runs[name], evidence[name] = {}, {}
        for label, trial in [('before', prior['runs'][name]), ('after', run)]:
            runs[name][label], evidence[name][label] = analyze_run(trial)
    payload = dict(schema=1, scope='Retained observations only; no counterfactual policy or projectile forecast.',
                   sources=sources, runs=evidence)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with archive.open('xb') as stream:
        stream.write(gzip.compress((json.dumps(payload, sort_keys=True, allow_nan=False) + '\n').encode(), mtime=0))
    native_sources = ['crates/spacewars-ai/src/lib.rs', 'crates/spacewars-ai/src/combat_pilot.rs',
                      'crates/spacewars-ai/src/mission_escape_travel.rs',
                      'scenarios/spacewars/src/surface_sortie/combat.rs',
                      'scenarios/spacewars/src/surface_sortie/impact.rs', 'scenarios/spacewars/src/lib.rs']
    result = dict(schema=1, complete=True, source_commit=subprocess.check_output(
        ['git', 'rev-parse', 'HEAD'], text=True).strip(), sources=sources,
        native_sources={name: digest(name) for name in native_sources},
        analyzer=dict(path=str(Path(__file__).relative_to(Path.cwd())) if Path(__file__).is_absolute() else __file__,
                      sha256=digest(__file__)),
        archive=dict(path=str(archive), sha256=digest(archive)), runs=runs)
    with args.out.open('x') as stream:
        stream.write(json.dumps(result, indent=2, allow_nan=False) + '\n')
    print(f'Analyzed {len(runs)} paired cases; saved {args.out} and {archive}')


if __name__ == '__main__':
    main()
