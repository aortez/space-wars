#!/usr/bin/env python3
"""Observe the retained acquisition-defense counterexample without gameplay changes."""
import argparse
from concurrent.futures import ProcessPoolExecutor
import csv
import importlib.util
import json
from pathlib import Path
import subprocess
import traceback


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


A = module('defense', 'validate-acquisition-defense.py')
N = module('native_loss', 'diagnose-climb-laser-loss.py')
B, D, I, P, ROOT = A.B, A.D, A.I, A.P, A.ROOT
GROUP = 'fresh-world1-v10-asteroids3-p2'
BINARY_SHA256 = 'fb3e2feb2c7bed2ffb68841942f98291af3789ff41de6e4fd96f06f31e11375c'
SOURCE_FILES = (
    'crates/spacewars-ai/examples/support/impact_probe.rs',
    'crates/spacewars-ai/src/mission_acquisition_defense.rs',
    'crates/spacewars-ai/src/mission_boundary.rs',
    'crates/spacewars-ai/src/mission_disengagement.rs',
    'crates/spacewars-ai/src/mission_pilot.rs',
    'scenarios/spacewars/src/surface_sortie/impact.rs',
    'scenarios/spacewars/src/surface_sortie/match_rules.rs',
    'scenarios/spacewars/src/surface_sortie/combat.rs',
    'scenarios/spacewars/src/lib.rs',
    'scenarios/spacewars/src/weapons.rs',
)


def inputs():
    result = A.inputs()
    for path in ('tools/diagnose-climb-laser-loss.py', 'tools/tests/test_climb_laser_loss.py',
                 'docs/pursuit-climb-laser-loss-plan.md', 'tools/diagnose-acquisition-defense-loss.py',
                 'tools/tests/test_acquisition_defense_loss.py', 'docs/acquisition-defense-loss-plan.md',
                 *SOURCE_FILES):
        result[path] = P.digest(ROOT / path)
    return result


def select(prior):
    assert prior['complete'] and prior['profile'] == A.PROFILE
    assert prior['binary']['sha256'] == BINARY_SHA256
    result = [prior['runs'][GROUP + '-' + arm] for arm in ('off', 'on')]
    for enabled, run in zip((False, True), result):
        item = run['item']
        assert item['group'] == GROUP and item['seed'] == 14699744800433948105
        assert item['seat'] == 1 and item['opponent'] == 10 and item['interval'] == 3
        assert item['arm'] == 'candidate' and item['laser'] and item['defense'] == enabled
        flags = dict(zip(run['command'][1::2], run['command'][2::2]))
        assert flags[A.FLAG] == ('1' if enabled else 'none') and flags[A.L.FLAG] == '1'
    return result


def contact_receipt(row, previous_count, points):
    """A last-contact stamp is current only when the native counter advances."""
    d = row['damage']; tick = row['tick']; count = d['debris_contacts']
    assert count >= previous_count
    if count == previous_count:
        return None
    assert d['last_contact_tick'] == tick
    spawn = d['last_contact_spawn_tick']
    assert spawn is not None and 0 <= spawn < tick
    result = dict(tick=tick, contacts_this_step=count-previous_count,
        source=d['last_contact_source'], spawn_tick=spawn, age_ticks=tick-spawn)
    if result['source'] == 'cannon':
        result['firing'] = []
        for seat in (0, 1):
            before, after = points[seat][spawn], points[seat][spawn+1]
            assert before['tick'] == spawn and after['tick'] == spawn+1
            delta = after['weapons']['shells_fired'] - before['weapons']['shells_fired']
            assert delta in (0, 1)
            if delta:
                assert before['action']['cannon']
                result['firing'].append(dict(seat=seat, requested=True, fired_delta=delta))
        # Provenance has no projectile id/owner. Retain ambiguous multi-shooter
        # clocks; do not manufacture an identity from the first matching seat.
    return result


def replay(prior, out):
    name = prior['item']['name']
    root = out / 'raw' / name
    old = out / 'inputs' / name
    command = N.command(prior, root)
    log = out / 'logs' / (name + '.log')
    with log.open('x') as stream:
        subprocess.run(command, check=True, stdout=stream, stderr=stream, timeout=1800)
    result = dict(item=prior['item'], command=command, hashes=I.raw_hashes(root), log_sha256=P.digest(log))
    I.write(out / (name + '-raw.json'), result)
    before, after = [json.loads((p / 'report.json').read_text()) for p in (old, root)]
    N.compare_reports(before, after)
    assert set(result['hashes']) == set(prior['hashes']) | {'impact.jsonl'}
    exact = sorted(set(prior['hashes']) - {'report.json', 'sensors.jsonl', 'live-planning.csv'})
    for filename in exact:
        assert result['hashes'][filename] == prior['hashes'][filename], filename
    sensor_rows = D.compare_jsonl(old / 'sensors.jsonl', root / 'sensors.jsonl')
    with (old / 'live-planning.csv').open() as a, (root / 'live-planning.csv').open() as b:
        left, right = [[D.timing_free(row) for row in csv.DictReader(stream)] for stream in (a, b)]
    assert left == right, 'charged planning ledger changed'
    result.update(I.analyze(root, prior, out))
    A.retained_results(prior, result)
    result['defense'] = A.audit(root, prior['item'], after)
    assert json.loads(json.dumps(result['defense'])) == prior['defense']
    result['diagnosis'] = N.analyze_impact(root, after)
    result['parity'] = dict(exact_streams=exact, non_timing_report=True, sensor_rows=sensor_rows,
                            planning_rows=len(left), exact_gameplay_audits=True)
    assert all(P.digest(root / filename) == digest for filename, digest in result['hashes'].items())
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert __debug__
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip(), 'commit observation plan first'
    prior_path, out = args.prior.resolve(), args.out.resolve()
    prior = json.loads(prior_path.read_text())
    selected = select(prior)
    assert len(prior['runs']) == 24 and len(prior['pairs']) == 12
    assert prior['inputs'] == A.inputs()
    assert P.digest(prior['binary']['path']) == BINARY_SHA256
    out.mkdir(parents=True, exist_ok=False)
    for directory in ('raw', 'inputs', 'logs', 'archives'):
        (out / directory).mkdir()
    frozen = inputs()
    result = dict(schema=1, complete=False, observational_only=True, default_changes=False,
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        runtime_source_commit=prior['source_commit'], binary=prior['binary'], inputs=frozen,
        prior_summary=dict(path=str(prior_path), sha256=P.digest(prior_path)),
        plan=dict(group=GROUP, commands=[N.command(r, out / 'raw' / r['item']['name']) for r in selected]), runs={})
    def save(): I.write(out / 'summary.json', result)
    save()
    try:
        for run in selected:
            B.unpack(run['archive'], out / 'inputs' / run['item']['name'])
        with ProcessPoolExecutor(max_workers=2) as pool:
            futures = {r['item']['name']: pool.submit(replay, r, out) for r in selected}
            for name, future in futures.items():
                try:
                    result['runs'][name] = future.result()
                    print(name, 'exact gameplay parity and native impact joins passed', flush=True)
                except Exception:
                    result['runs'][name] = dict(error=traceback.format_exc())
                    print(name, result['runs'][name]['error'], flush=True)
                save()
        assert all('error' not in run for run in result['runs'].values()), 'replay/audit failed'
        roots = [out / 'raw' / r['item']['name'] for r in selected]
        result['differences'] = N.differences(*(map(N.point, D.rows(root / 'trace.jsonl')) for root in roots))
        for path, digest in frozen.items(): assert P.digest(ROOT / path) == digest
        assert P.digest(prior_path) == result['prior_summary']['sha256']
        assert P.digest(prior['binary']['path']) == BINARY_SHA256
        for run in selected:
            original = out / 'inputs' / run['item']['name']
            assert {p.name: dict(sha256=P.digest(p), bytes=p.stat().st_size) for p in original.iterdir()} == run['archive']['files']
        result['complete'] = True
    finally:
        save()


if __name__ == '__main__':
    main()
