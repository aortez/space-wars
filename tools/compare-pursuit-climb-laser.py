#!/usr/bin/env python3
"""Freeze and execute the fresh-world, paired pursuit-climb laser comparison."""
import argparse
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
import copy
import gzip
import hashlib
import importlib.util
import itertools
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import traceback


spec = importlib.util.spec_from_file_location(
    'climb_qualification', Path(__file__).with_name('validate-pursuit-climb-laser.py'))
L = importlib.util.module_from_spec(spec)
spec.loader.exec_module(L)
I, P, D, ROOT = L.I, L.I.P, L.D, L.ROOT
NAMESPACE = 'pursuit_climb_laser_v1:held-out:2026-10'
QUALIFICATION = ROOT / 'docs/data/pursuit-climb-laser-v1.json'
POINTS = dict(win=1., draw=.5, loss=0.)


def cases():
    result = []
    for world, opponent, interval, seat, arm in itertools.product(
            range(4), (9, 10, 16), (0, 3), (0, 1), ('control', 'candidate')):
        group = f'fresh-world{world}-v{opponent}-asteroids{interval}-p{seat+1}-{arm}'
        namespace = f'{NAMESPACE}:{world}'
        order = (False, True) if (world + opponent + interval + seat + (arm == 'candidate')) % 2 == 0 else (True, False)
        for enabled in order:
            result.append(dict(name=group + ('-on' if enabled else '-off'), group=group,
                arm=arm, laser=enabled, stage='held_out', world='generated', seat=seat,
                opponent=opponent, interval=interval, seed=P.seed(namespace), seed_namespace=namespace,
                world_cluster=world, seconds=600))
    return result


def flags(item):
    result = P.flags(item)
    assert L.FLAG not in result
    result.update({L.FLAG: str(item['seat']) if item['laser'] else 'none',
                   '--trace-start-tick': '0', '--trace-end-tick': str(D.END)})
    return result


def jobs(binary, out):
    result = []
    for item in cases():
        argv = [arg for pair in flags(item).items() for arg in pair]
        result.append(dict(item=item, command=[str(binary), *argv, '--out', str(out / 'raw' / item['name'])]))
    return result


def tool_inputs():
    qualification = json.loads(QUALIFICATION.read_text())
    inputs = json.loads(Path(qualification['qualification_summary']['path']).read_text())['inputs']
    inputs = dict(inputs)
    # Freeze every imported evaluator, including this fresh comparison driver.
    for name, digest in inputs.items():
        assert P.digest(ROOT / name) == digest, f'qualified tool changed: {name}'
    inputs[str(Path(__file__).resolve().relative_to(ROOT))] = P.digest(__file__)
    for path in ('tools/tests/test_pursuit_climb_comparison.py', 'docs/pursuit-climb-laser-comparison.md'):
        inputs[path] = P.digest(ROOT / path)
    return inputs


def verify_plan(plan, out, audit_revision=False):
    assert plan['profile'] == L.PROFILE and plan['namespace'] == NAMESPACE
    qualification = json.loads(QUALIFICATION.read_text())
    assert plan['binary']['sha256'] == qualification['binary']['sha256']
    assert plan['runtime_source_commit'] == qualification['runtime_source_commit']
    assert plan['jobs'] == jobs(Path(plan['binary']['path']), out)
    assert len(plan['jobs']) == 192
    current = tool_inputs()
    if audit_revision:
        allowed = {str(Path(__file__).resolve().relative_to(ROOT)),
                   'tools/tests/test_pursuit_climb_comparison.py'}
        assert set(plan['inputs']) == set(current)
        assert all(current[k] == v for k, v in plan['inputs'].items() if k not in allowed)
    else:
        assert plan['inputs'] == current
    assert P.digest(plan['binary']['path']) == plan['binary']['sha256']
    assert P.digest(QUALIFICATION) == plan['qualification']['sha256']


def audit_watch_check(row, trace, previous):
    """A temporarily missing combat target can leave pursuit in Watch while climbing.

    The native hook records an unavailable decision, never a laser request.
    The older qualification had only Hunt entries; retain its firing checks.
    """
    m, e = trace['mission'], row[L.FIELD]
    o, gate = e['observation'], e['telemetry']
    check = gate['last']
    assert m['goal'] == 'watch' and o['local']['combat']['target'] is None
    assert o == trace['observation'] and gate == m[L.FIELD]
    pilot = o['local']['combat']['recovery']['flight']['pilot']
    assert {k: v for k, v in pilot.items() if k != 'sites'} == row['pilot']
    assert row['actions'] == trace['actions']
    for key in ('pursuit', 'combat', 'reason'):
        assert e[key] == m[key]
    assert check['tick'] == trace['tick'] == pilot['tick']
    assert pilot['owner'] == f"player_{trace['seat']+1}"
    assert check['source'] == 'mission' and m['reason'] == 'climbing for a firing pass'
    assert m['combat'] is None and check['decision'] == 'unavailable'
    assert check['distance'] is None and check['heading_error'] is None
    L.audit_actions(check['native_actions'], row['actions'], trace['seat'], False)
    assert gate['checks'] == previous['checks'] + 1
    assert gate['requested_ticks'] == previous['requested_ticks']
    return gate


def audit_laser(root, item, report):
    enabled, seat = item['laser'], item['seat']
    if enabled:
        assert report[L.FIELD]['profile'] == L.PROFILE
        assert report[L.FIELD]['enabled_seats'] == [s == seat for s in (0, 1)]
    else:
        assert L.FIELD not in report
    for s, descriptor in enumerate(report['policy_configuration']):
        assert descriptor.get(L.FIELD + '_model') == (L.PROFILE if enabled and s == seat else None)
    counters = dict(checks=0, requested_ticks=0, last=None)
    decisions, sources, breaks, witnesses, requests = Counter(), Counter(), Counter(), {}, []
    total = 0
    for index, (row, trace) in enumerate(itertools.zip_longest(
            D.rows(root / 'capture-evidence.jsonl'), D.rows(root / 'trace.jsonl'))):
        assert row is not None and trace is not None
        tick, actor = index // 2, index % 2
        p = trace['observation']['local']['combat']['recovery']['flight']['pilot']
        assert (row['pilot']['tick'], row['seat']) == (trace['tick'], trace['seat']) == (tick, actor)
        assert p['tick'] == tick and p['owner'] == f'player_{actor+1}'
        assert row['actions'] == trace['actions']
        m = trace['mission']
        if not enabled or actor != seat:
            assert L.FIELD not in row and L.FIELD not in m
        else:
            gate = m[L.FIELD]
            current = gate['last'] is not None and gate['last']['tick'] == tick
            assert current == (L.FIELD in row)
            selected = (m['reason'] == 'climbing for a firing pass'
                        or (m['combat'] or {}).get('goal') == 'climb clear of ground')
            assert current == selected, 'missing or misplaced climb check'
            if current:
                audit = audit_watch_check if m['goal'] == 'watch' else L.audit_check
                counters = audit(row, trace, counters)
                check = counters['last']
                decisions[check['decision']] += 1
                sources[check['source']] += 1
                witnesses.setdefault(check['decision'], row)
                if check['decision'] == 'requested':
                    requests.append(tick)
            assert gate == counters
        mode = (m['combat'] or {}).get('goal', '')
        if actor == seat and mode.startswith('flyby /'):
            assert L.T.weapon_action(row) == dict(laser=False, cannon=False)
            breaks[mode] += 1
        total += 1
    assert total == report['elapsed_ticks'] * 2 and report['dense_trace_ticks'] == [0, D.END]
    for s, mission in enumerate(report['missions']):
        assert mission.get(L.FIELD) == (counters if enabled and s == seat else None)
    return dict(enabled=enabled, counters=counters if enabled else None, decisions=dict(decisions),
                sources=dict(sources), request_ticks=requests, dense_rows=total,
                weapon_free_break_ticks=dict(breaks)), witnesses


def normalized_report(value):
    if isinstance(value, dict):
        return {k: normalized_report(v) for k, v in value.items()
                if k not in (L.FIELD, L.FIELD + '_model') and not k.endswith('_ms')}
    if isinstance(value, list):
        return [normalized_report(v) for v in value]
    return value


def compare(pair, runs):
    by_option = {j['item']['laser']: runs[j['item']['name']] for j in pair}
    a, b = by_option[False], by_option[True]
    roots = [L.root_of(r) for r in (a, b)]
    reports = [json.loads((root / 'report.json').read_text()) for root in roots]
    assert reports[0]['initial_world'] == reports[1]['initial_world']
    requests = b['laser']['request_ticks']
    difference = L.compare_prefix(*(D.rows(root / 'trace.jsonl') for root in roots),
                                  requests[0] if requests else None)
    if difference['tick'] is None:
        assert normalized_report(reports[0]) == normalized_report(reports[1])
        assert a['players'] == b['players'] and a['allocation'] == b['allocation']
        for filename in a['hashes']:
            if filename not in ('report.json', 'sensors.jsonl', 'live-planning.csv',
                                 'trace.jsonl', 'capture-evidence.jsonl'):
                assert a['hashes'][filename] == b['hashes'][filename], filename
        D.compare_jsonl(roots[0] / 'sensors.jsonl', roots[1] / 'sensors.jsonl')
    item = a['item']
    x, y = [r['players'][item['seat']] for r in (a, b)]
    useful, missing = I.completion_changes(x, y, difference['tick'])
    benefits = []
    if POINTS[y['outcome']] > POINTS[x['outcome']]:
        benefits.append('more_match_points')
    for key in ('ships_lost', 'pilot_deaths'):
        if y[key] < x[key]:
            benefits.append('fewer_' + key)
    if useful:
        benefits.append('earlier_or_additional_departure')
    if difference['tick'] is None:
        assert not benefits
    return dict(group=item['group'], configuration=item['arm'], seat=item['seat'],
                opponent=item['opponent'], interval=item['interval'], seed=item['seed'],
                world_cluster=item['world_cluster'], off=a['item']['name'], on=b['item']['name'],
                first_difference=difference, benefits=benefits,
                useful_completed_changes=useful, missing_off_completions=missing,
                outcome_transition=x['outcome'] + '->' + y['outcome'])


def screen(runs, comparisons):
    assert len(comparisons) == 96 and len(runs) == 192
    assert len({p['group'] for p in comparisons}) == 96
    assert {r['item']['name'] for r in runs.values()} == {c['name'] for c in cases()}
    result = {}
    for configuration in ('control', 'candidate'):
        pairs = [p for p in comparisons if p['configuration'] == configuration]
        assert len(pairs) == 48
        def aggregate(selected):
            return {arm: I.totals([runs[p[arm]]['players'][p['seat']] for p in selected]) for arm in ('off', 'on')}
        totals, strata, reasons = aggregate(pairs), {}, []
        for key in ('opponent', 'seat', 'interval', 'world_cluster'):
            for value in sorted({p[key] for p in pairs}):
                name = f'{key}:{value}'
                strata[name] = aggregate([p for p in pairs if p[key] == value])
                if key != 'world_cluster' and strata[name]['on']['points'] < strata[name]['off']['points']:
                    reasons.append('match_points_regressed:' + name)
        if not any(p['benefits'] for p in pairs):
            reasons.append('no_useful_fresh_change')
        a, b = totals['off'], totals['on']
        if b['completed_departures'] < a['completed_departures']:
            reasons.append('fewer_completed_departures')
        for key in ('ships_lost', 'pilot_deaths', 'worst_no_progress_ticks'):
            if b[key] > a[key]:
                reasons.append(key + '_increased')
        if a['no_progress_fraction'] is None or b['no_progress_fraction'] is None:
            reasons.append('unknown_no_progress_fraction')
        elif b['ticks_after_20s_without_progress'] * a['eligible_ticks'] > a['ticks_after_20s_without_progress'] * b['eligible_ticks']:
            reasons.append('no_progress_fraction_increased')
        regressions = []
        for p in pairs:
            x, y = [runs[p[k]]['players'][p['seat']] for k in ('off', 'on')]
            if (POINTS[y['outcome']] < POINTS[x['outcome']] or y['ships_lost'] > x['ships_lost']
                    or y['pilot_deaths'] > x['pilot_deaths']):
                regressions.append(p['group'])
        result[configuration] = dict(decision='retain' if reasons else 'advance_to_further_evaluation',
            reasons=reasons, totals=totals, strata=strata,
            changed_pairs=sum(p['first_difference']['tick'] is not None for p in pairs),
            useful_pairs=sum(bool(p['benefits']) for p in pairs),
            outcomes=dict(Counter(p['outcome_transition'] for p in pairs)), regression_pairs=regressions)
    return dict(configurations=result, default_promotion=False, independent_world_clusters=4,
                decision='retain' if any(r['reasons'] for r in result.values()) else 'advance_to_further_evaluation')


def archive_files(path):
    """Verify every member without extraction; reject links and unsafe names."""
    result = {}
    with tarfile.open(path, 'r|gz') as archive:
        for member in archive:
            assert member.isfile() and Path(member.name).name == member.name and member.name not in ('.', '..')
            assert member.name not in result
            with archive.extractfile(member) as stream:
                digest = hashlib.file_digest(stream, 'sha256').hexdigest()
            result[member.name] = dict(sha256=digest, bytes=member.size)
    return result


def verify_archive(record):
    assert P.digest(record['path']) == record['sha256']
    assert archive_files(record['path']) == record['files']


def unpack(record, root):
    assert not root.exists()
    verify_archive(record)
    root.mkdir()
    with tarfile.open(record['path'], 'r|gz') as archive:
        for member in archive:
            # archive_files validated exact regular, flat members above.
            assert member.isfile() and Path(member.name).name == member.name and member.name not in ('.', '..')
            with archive.extractfile(member) as source, (root / member.name).open('xb') as target:
                shutil.copyfileobj(source, target)
    assert {p.name: dict(sha256=P.digest(p), bytes=p.stat().st_size) for p in root.iterdir()} == record['files']


def pack(root, path):
    assert root.is_dir()
    files = {p.name: dict(sha256=P.digest(p), bytes=p.stat().st_size) for p in sorted(root.iterdir()) if p.is_file()}
    assert len(files) == len(list(root.iterdir())), 'unexpected generated subdirectory'
    if not path.exists():
        temporary = path.with_suffix(path.suffix + '.partial')
        with temporary.open('xb') as output, gzip.GzipFile(fileobj=output, mode='wb', compresslevel=1, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w|') as archive:
                for name, info in files.items():
                    member = tarfile.TarInfo(name)
                    member.size, member.mode, member.mtime = info['bytes'], 0o644, 0
                    with (root / name).open('rb') as source:
                        archive.addfile(member, source)
        assert archive_files(temporary) == files, 'lossless compression check failed'
        temporary.rename(path)
    else:
        assert archive_files(path) == files, 'existing archive differs'
    record = dict(path=str(path), sha256=P.digest(path), bytes=path.stat().st_size,
                  raw_bytes=sum(f['bytes'] for f in files.values()), files=files)
    assert {p.name: dict(sha256=P.digest(p), bytes=p.stat().st_size) for p in root.iterdir()} == files
    # Only this newly generated run directory is removed, after a full hash
    # round trip. Its exact bytes remain recoverable from the retained archive.
    shutil.rmtree(root)
    return record


def run_case(job, previous=None):
    result = copy.deepcopy(job)
    root = L.root_of(job)
    log = root.parent.parent / 'logs' / (root.name + '.log')
    try:
        if previous is None:
            assert not root.exists()
            with log.open('x') as stream:
                subprocess.run(job['command'], check=True, stdout=stream, stderr=stream, timeout=1800)
            result['hashes'] = I.raw_hashes(root)
        else:
            assert previous['item'] == job['item'] and previous['command'] == job['command']
            if not root.exists():
                unpack(previous['archive'], root)
            assert all(P.digest(root / name) == digest for name, digest in previous['hashes'].items())
            assert P.digest(log) == previous['log_sha256']
            result['hashes'] = previous['hashes']
            result['reused_raw'] = True
        result.update(I.analyze(root, job, root))
        report = json.loads((root / 'report.json').read_text())
        result['laser'], witnesses = audit_laser(root, job['item'], report)
        I.write(root / 'laser-audit.json', dict(summary=result['laser'], first_witness_by_decision=witnesses))
        assert all(P.digest(root / name) == digest for name, digest in result['hashes'].items())
        result['audited'] = True
    except Exception:
        result['error'] = traceback.format_exc()
        if 'hashes' not in result and root.exists():
            result['hashes'] = I.raw_hashes(root)
    finally:
        if log.exists():
            result['log_sha256'] = P.digest(log)
    return result


def freeze(out):
    qualification = json.loads(QUALIFICATION.read_text())
    assert qualification['complete'] and qualification['profile'] == L.PROFILE
    reference = qualification['qualification_summary']
    assert P.digest(reference['path']) == reference['sha256']
    completed = json.loads(Path(reference['path']).read_text())
    assert completed['complete'] and len(completed['runs']) == 4
    original = Path(qualification['binary']['path'])
    assert P.digest(original) == qualification['binary']['sha256']
    assert not {c['seed'] for c in cases()} & {c['seed'] for c in P.plan()}
    out.mkdir(parents=True, exist_ok=False)
    for directory in ('raw', 'logs', 'archives'):
        (out / directory).mkdir()
    binary = out / 'surface_mission_soak'
    shutil.copy2(original, binary)
    binary.chmod(0o555)
    assert P.digest(binary) == qualification['binary']['sha256']
    plan = dict(schema=1, profile=L.PROFILE, namespace=NAMESPACE,
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        runtime_source_commit=qualification['runtime_source_commit'],
        qualification=dict(path=str(QUALIFICATION), sha256=P.digest(QUALIFICATION)),
        binary=dict(path=str(binary), sha256=P.digest(binary)), inputs=tool_inputs(),
        jobs=jobs(binary, out), default_changes=False, independent_world_clusters=4)
    I.write(out / 'plan.json', plan)
    print(f"Frozen {len(plan['jobs'])} cases across four fresh world clusters", flush=True)


def execute(plan_path, resume, reaudit):
    out = plan_path.parent
    plan = json.loads(plan_path.read_text())
    verify_plan(plan, out, audit_revision=reaudit)
    summary_path = out / 'summary.json'
    prior = json.loads(summary_path.read_text()) if resume or reaudit else None
    if prior:
        assert prior['plan_sha256'] == P.digest(plan_path)
        if not reaudit:
            assert prior['inputs'] == tool_inputs()
    else:
        assert not summary_path.exists()
    if reaudit:
        saved = out / ('attempt-' + P.digest(summary_path)[:16] + '.json')
        assert not saved.exists()
        shutil.copy2(summary_path, saved)
    result = prior if resume and not reaudit else dict(schema=1, complete=False,
        plan_path=str(plan_path), plan_sha256=P.digest(plan_path), binary=plan['binary'],
        runtime_source_commit=plan['runtime_source_commit'],
        runner_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        inputs=tool_inputs(), runs={}, comparisons=[])
    if reaudit:
        result['previous_summary'] = dict(path=str(saved), sha256=P.digest(saved))
    result.pop('error', None)
    result['complete'] = False
    save = lambda: I.write(summary_path, result)
    save()
    try:
        with ProcessPoolExecutor(max_workers=2) as pool:
            for index in range(0, len(plan['jobs']), 2):
                pair = plan['jobs'][index:index+2]
                names = [j['item']['name'] for j in pair]
                if (resume and not reaudit and len(result['comparisons']) > index // 2
                        and all(result['runs'].get(n, {}).get('archive') for n in names)):
                    for name in names:
                        verify_archive(result['runs'][name]['archive'])
                    continue
                assert shutil.disk_usage(out).free > 6 * 1024**3, 'less than six GiB free; retain completed work'
                futures = [pool.submit(run_case, job, (prior or {}).get('runs', {}).get(job['item']['name'])) for job in pair]
                for name, future in zip(names, futures):
                    result['runs'][name] = future.result()
                    save()
                assert all(result['runs'][n].get('audited') for n in names), 'audit failed; pair retained and matrix stopped'
                comparison = compare(pair, result['runs'])
                if len(result['comparisons']) > index // 2:
                    result['comparisons'][index // 2] = comparison
                else:
                    result['comparisons'].append(comparison)
                save()
                revision = result['inputs'][str(Path(__file__).resolve().relative_to(ROOT))][:12]
                futures = [pool.submit(pack, L.root_of(j), out / 'archives' / (n + '.audit-' + revision + '.tar.gz'))
                           for j, n in zip(pair, names)]
                for name, future in zip(names, futures):
                    result['runs'][name]['archive'] = future.result()
                    save()
                print(f'Audited and archived {index+2}/192 games', flush=True)
        result['screen'] = screen(result['runs'], result['comparisons'])
        assert P.digest(plan_path) == result['plan_sha256']
        assert P.digest(plan['binary']['path']) == plan['binary']['sha256']
        assert tool_inputs() == result['inputs']
        result['complete'] = True
    except BaseException:
        result['error'] = traceback.format_exc()
        raise
    finally:
        save()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('plan', 'run'))
    parser.add_argument('--out', type=Path)
    parser.add_argument('--plan', type=Path)
    parser.add_argument('--resume', action='store_true')
    parser.add_argument('--reaudit', action='store_true')
    args = parser.parse_args()
    assert __debug__ and not (args.resume and args.reaudit)
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip(), 'freeze runner, tests and plan first'
    if args.mode == 'plan':
        assert args.out is not None and args.plan is None and not args.resume and not args.reaudit
        freeze(args.out.resolve())
    else:
        assert args.plan is not None and args.out is None
        execute(args.plan.resolve(), args.resume, args.reaudit)


if __name__ == '__main__':
    main()
