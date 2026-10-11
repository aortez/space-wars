#!/usr/bin/env python3
"""Preserve observed projectile tracks and frozen two-second screening episodes."""
import argparse
import gzip
import importlib.util
import json
import math
from pathlib import Path

spec = importlib.util.spec_from_file_location('projectiles', Path(__file__).with_name('validate-projectile-diagnostics.py'))
P = importlib.util.module_from_spec(spec)
spec.loader.exec_module(P)


def episodes(rows, predicate):
    result = []
    for row in rows:
        if not predicate(row):
            continue
        tick = row['tick']
        if not result or tick != result[-1]['last_tick'] + 1:
            result.append(dict(first_tick=tick, last_tick=tick, ticks=1))
        else:
            result[-1]['last_tick'] = tick
            result[-1]['ticks'] += 1
    return result


def analyze(root, item, contact):
    seat = item['seat']
    report = json.loads((root / 'report.json').read_text())
    attempt = report['missions'][seat]['escape_travel']['last']
    start, end = attempt['started_tick'], attempt['finished_tick']
    vehicle = attempt['vehicle']
    observations, retained, samples, ids = [], [], {}, set()
    source = contact['spawn_tick']
    shooters = contact['cannon_request_seats_at_spawn']
    assert len(shooters) == 1, 'ambiguous launch owner'
    owner = f'player_{shooters[0] + 1}'
    unavailable = truncated = 0
    for row in P.rows(root / 'projectiles.jsonl'):
        if row['seat'] != seat:
            continue
        tick, d = row['tick'], row['diagnostic']
        if start <= tick < end:
            unavailable += d is None
            truncated += d is not None and d['shells_in_range'] > d['capacity']
        if d is None:
            continue
        for p in d['projectiles']:
            if p['spawn_tick'] != source or p['owner'] != owner:
                continue
            ids.add(p['id'])
            screen = P.linear_approach(p['relative_position'], p['relative_velocity'],
                                       d['observer_radius'] + p['collision_radius'])
            record = dict(tick=tick, projectile=p, observer=d['observer'],
                          observer_radius=d['observer_radius'], ship_form=d['ship_form'],
                          vehicle=d['vehicle'], screen=screen)
            observations.append(record)
            if start <= tick < end and d['vehicle'] == vehicle and d['ship_form'] == 'ship':
                retained.append(record)
    assert len(ids) <= 1, 'launch metadata matched multiple stable identities'
    tracked_ticks = {r['tick'] for r in retained}
    for row in P.rows(root / 'capture-evidence.jsonl'):
        if row['seat'] != seat or row['pilot']['tick'] not in tracked_ticks:
            continue
        witness = row.get('escape_travel')
        if witness:
            c = witness['observation']['local']['combat']
            flight = c['recovery']['flight']['flight']
            limits = flight['limits']
            samples[row['pilot']['tick']] = dict(
                hull=row['pilot']['ship_health'], mission=row['mission'],
                flight=flight, gravity=row['pilot']['gravity'],
                half_turn_seconds=math.pi / max(limits['turn_speed'], .1),
                nominal_brake_seconds=flight['relative_speed'] / limits['brake_acceleration'],
                actions=row['actions'])
    for row in retained:
        row['native'] = samples.get(row['tick'])
    selected_ticks = {r['first_tick'] for r in episodes(retained, lambda r: r['screen']['entry_seconds'] is not None)}
    selected_ticks |= {r['first_tick'] for r in episodes(retained, lambda r: True)}
    if retained:
        selected_ticks.add(retained[-1]['tick'])
    summary = dict(stable_ids=sorted(ids), launch_tick=source, owner=owner,
                   transfer_start=start, transfer_end=end, transfer_end_reason=attempt['reason'],
                   tracked_samples=len(observations), transfer_samples=len(retained),
                   unavailable_observer_ticks=unavailable, truncated_transfer_ticks=truncated,
                   all_observed_intervals=episodes(observations, lambda r: True),
                   transfer_observed_intervals=episodes(retained, lambda r: True),
                   transfer_screen_intervals=episodes(retained, lambda r: r['screen']['entry_seconds'] is not None),
                   checkpoints=[r for r in retained if r['tick'] in selected_ticks],
                   final_damage=report['samples'][-1]['damage'][seat])
    return summary, dict(track=observations, transfer=retained,
                         report_loss_samples=[s for s in report['samples']
                                              if end - 60 <= s['pilots'][seat]['tick'] <= end + 60])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary', type=Path, required=True)
    parser.add_argument('--contacts', type=Path, default=Path('docs/data/transfer-threat-v1.json'))
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    s = json.loads(args.summary.read_text())
    contacts = json.loads(args.contacts.read_text())
    assert s['complete'] and contacts['complete']
    archive = args.out.with_suffix(args.out.suffix + '.gz')
    assert not args.out.exists() and not archive.exists()
    records, evidence, raw = {}, {}, {}
    for entry in s['plan']:
        run = s['runs'][entry['key']]
        root = P.root_of(run)
        for name, expected in run['hashes'].items():
            path = root / name
            assert P.digest(path) == expected
            raw[str(path)] = expected
        if not entry['enabled']:
            continue
        contact = contacts['runs'][entry['name']]['after']['attempts'][0]['fatal_contact']
        records[entry['key']], evidence[entry['key']] = analyze(root, run['item'], contact)
    payload = dict(schema=1, runs=evidence)
    with archive.open('xb') as stream:
        stream.write(gzip.compress((json.dumps(payload, sort_keys=True, allow_nan=False) + '\n').encode(), mtime=0))
    manifest = dict(schema=1, complete=True, source_commit=s['source_commit'], binary=s['binary'],
                    sources={str(p): P.digest(p) for p in [args.summary, args.contacts]},
                    analyzer_sha256=P.digest(__file__), runner_sha256=P.digest(P.__file__),
                    archive=dict(path=str(archive), sha256=P.digest(archive)), raw_files=raw,
                    parity={key: r['parity'] for key, r in s['runs'].items()},
                    diagnostics={key: r.get('projectiles') for key, r in s['runs'].items()}, runs=records)
    with args.out.open('x') as stream:
        stream.write(json.dumps(manifest, indent=2, allow_nan=False) + '\n')
    print(f'Archived {len(records)} tracks and {len(raw)} raw file hashes: {args.out}')


if __name__ == '__main__':
    main()
