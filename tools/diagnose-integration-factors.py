#!/usr/bin/env python3
"""Freeze a three-factor diagnosis of recorded integration failures and gains."""
import argparse
from concurrent.futures import ProcessPoolExecutor
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('strategies', Path(__file__).with_name('compare-recovery-strategies.py'))
S = importlib.util.module_from_spec(spec); spec.loader.exec_module(S)
C, B, I, P, R, TRACE, ROOT = S.C, S.B, S.I, S.P, S.R, S.D, S.ROOT
F, T, D = I.F, I.T, I.D
PROFILE = 'integration_factors_v1'
SOURCE = ROOT / 'target/recovery-strategies/v1/summary.json'
GROUPS = ('known-rescue', 'known-boundary', 'fresh-world0-p2-asteroids3', 'fresh-world1-p1-asteroids3')
FACTORS = {1: 'valuation', 2: 'retry', 4: 'powered'}
ORDER = (0, 7, 1, 2, 3, 4, 5, 6)
OWN = ('tools/diagnose-integration-factors.py', 'tools/tests/test_integration_factors.py',
       'docs/integration-factors-plan.md')


def inputs():
    return dict(S.inputs(), **{p: P.digest(ROOT / p) for p in OWN})


def label(mask):
    assert type(mask) is int and 0 <= mask < 8
    return '-'.join(f'{letter}{int(bool(mask & bit))}' for letter, bit in zip('vrp', FACTORS))


def cases(source):
    result = []
    for group in GROUPS:
        original = source['runs'][group + '-control']['item']
        assert original['interval'] == 3 and original['opponent'] == 10
        for mask in ORDER:
            result.append(dict(original, name=group + '-' + label(mask), mask=mask,
                configuration=label(mask), stage='selected_diagnosis',
                arm='candidate' if mask == 7 else 'control', laser=False, defense=False, clearance=False))
    return result


def flags(item):
    result = S.flags(dict(item, arm='control'))
    seat, mask = str(item['seat']), item['mask']
    for name in ('--admit-flag-costs', '--survey-current-neutral'):
        result[name] = seat if mask & 1 else 'none'
    result['--destination-retry-seats'] = seat if mask & 2 else 'none'
    result['--powered-capture-seats'] = seat if mask & 4 else 'none'
    result['--active-flight-checks'] = str(bool(mask & 4)).lower()
    return result


def jobs(source, binary, out):
    return [dict(item=item, command=[str(binary), *(v for pair in flags(item).items() for v in pair),
                                    '--out', str(out / 'raw' / item['name'])]) for item in cases(source)]


def contrasts():
    edges = {(mask, mask | bit) for bit in FACTORS for mask in range(8) if not mask & bit}
    return sorted(edges | {(0, mask) for mask in range(1, 8)})


def check_configuration(report, item):
    """Check independent factor settings before using shared physical auditors."""
    assert report['physics_ok'] and not report['audit_failures']
    seat, mask, expected = item['seat'], item['mask'], flags(item)
    enabled = {bit: [bool(mask & bit) and s == seat for s in (0, 1)] for bit in FACTORS}
    assert len(report['policy_configuration']) == 2
    for s, descriptor in enumerate(report['policy_configuration']):
        assert descriptor['policy'] == expected[f'--p{s+1}-policy']
        assert ('jetpack' in descriptor['sensor_profile']) == enabled[4][s]
        for key, model in P.MODELS.items():
            bit = 2 if key == 'destination_retry_model' else 1
            assert descriptor.get(key) == (model if enabled[bit][s] else None), key
        assert 'cover_response_model' not in descriptor and 'cover_retry_model' not in descriptor
    for key, profile in P.PROFILES.items():
        bit = 2 if key == 'destination_retry' else 4
        assert report.get(key, {}).get('enabled_seats', [False, False]) == enabled[bit], key
        assert report.get(key, {}).get('profile') == (profile if mask & bit else None), key
    for key in ('flag_cost_admission', 'current_neutral_survey'):
        assert report['mission_evaluation'].get(key, {}).get('enabled_seats', [False, False]) == enabled[1], key
    for key in ('cover_response', 'cover_retry_cooldown', 'bounded_acquisition', 'initial_cover',
                'actual_route_recovery', 'capture_escape', 'escape_travel', 'transfer_approach',
                'transfer_speed', 'pursuit_health', 'projectile_response', 'pursuit_climb_laser',
                'acquisition_defense', 'acquisition_clearance'):
        assert key not in report, key
    assert report['pursuit_disengagement'] == dict(enabled_seats=[False, False], probe_handoff=False,
                                                  boundary_guidance=False, destination_cover_probe=False)
    live = report['live_objective_planning']
    assert live['enabled_seats'] == [0, 1] and live['allowance'] == dict(graph=4, physics_queries=384)
    assert live['objective_dependencies'] == 'routes'
    assert all(live.get(name.replace('-', '_')) is True for name in P.ROUTE_OPTIONS)
    assert 'covered_request_handoff' not in live and 'actual_failure_feedback' not in live
    assert report['seed'] == item['seed'] and report['mode'] == expected['--mode']
    assert report['seat'] == int(expected['--seat'])
    assert report['combat_breaks'] == dict(interval_seconds=15, duration_seconds=4)
    assert report['landing_survey_hz'] == 4 and report['termination'] == 'round_finished'


def analyze(root, job, out):
    item = job['item']
    report = json.loads((root / 'report.json').read_text())
    check_configuration(report, item)
    assert report['seconds'] == item['seconds'] and report['world'] == item['world']
    assert report['round']['time_limit_seconds'] == 600
    visit_audit = F.M.audit(report)
    assert not any(visit_audit['counts'].get(k, 0) for k in
                   ('unverified', 'corrected_visits', 'milestones_outside_visit'))
    for progress in report['mission_progress']['players']:
        assert progress['eligible_ticks'] == progress['progress_ticks'] + progress['ticks_without_progress']
        assert 0 <= progress['ticks_after_20s_without_progress'] <= progress['ticks_without_progress']
        assert progress['distance_observed_ticks'] <= progress['eligible_ticks']
    # The shared dense auditor's arm controls powered execution only.
    dense_item = dict(item, arm='candidate' if item['mask'] & 4 else 'control')
    dense = I.audit_dense(F.rows(root / 'capture-evidence.jsonl'), report, dense_item)
    audit = dict(physical=dense, visit_endings=visit_audit,
                 allocation=F.allocation_audit(root),
                 predictions=F.first_predictions(report, root / 'mission-evaluations.jsonl'),
                 evaluation_coverage=D.evaluation_coverage(root / 'mission-evaluations.jsonl'),
                 switches=F.P.switch_predictions(report, F.rows(root / 'mission-evaluations.jsonl')),
                 retry=T.retry_stats(report, [item['seat']] if item['mask'] & 2 else []))
    path = out / (item['name'] + '-audit.json')
    I.write(path, audit)
    players = []
    for seat in (0, 1):
        player = D.finished_player(report, seat)
        visits = player['visits']
        players.append(dict(outcome=player['outcome'], owned_planets=player['owned_planets'],
            owned_planet_ticks=dense['owned_planet_ticks'].get(seat, 0),
            completed_departures=player['completed_sorties'], completed_recoveries=player['completed_recoveries'],
            claims=sum(v['claimed_tick'] is not None for v in visits),
            abandoned_visits=sum(v['abandoned_tick'] is not None for v in visits),
            unfinished_visits=sum(v['abandoned_tick'] is None and v['departed_tick'] is None for v in visits),
            ships_lost=player['recovery']['ships_lost'], pilot_deaths=int(player['death_tick'] is not None),
            death_tick=player['death_tick'], pilot_health=player['pilot_health'],
            first_claim_tick=player['first_claim_tick'], first_departure_tick=player['first_departed_tick'],
            visits=visits, switches=player['switches'], progress=report['mission_progress']['players'][seat],
            combat=report['final_combat'][seat], longest_phase_ticks=player['longest_phase_ticks']))
        assert players[-1]['completed_departures'] == sum(v['departed_tick'] is not None for v in visits)
    return dict(audit=dict(path=str(path), sha256=P.digest(path)), players=players,
                elapsed_ticks=report['elapsed_ticks'], termination=report['termination'],
                round=report['round'], allocation=audit['allocation'],
                continuation={k: v for k, v in dense['continuation'].items()
                              if k not in ('witnesses', 'launch_presses')},
                route_summary=dict(publications=dense['publications'], max_age=dense['max_publication_age'],
                    powered_publications=len(dense['powered_publications']), launches=len(dense['launches']),
                    completed_crossings=dense['completed_crossings']),
                prediction_outcomes=audit['predictions']['outcomes'], retry=audit['retry'],
                timings={key: report.get(key) for key in ('sensors', 'policy', 'steps', 'measured_tick')},
                planner_timings={key: report['live_objective_planning'][key]
                                 for key in ('dispatch', 'active_dispatch', 'timing_scope')})


def verify_plan(plan, out, reaudit=False):
    assert plan['profile'] == PROFILE
    current = inputs()
    assert set(current) == set(plan['inputs'])
    changed = {k for k in current if current[k] != plan['inputs'][k]}
    assert not changed or reaudit and changed <= set(OWN[:2])
    assert P.digest(plan['source']['path']) == plan['source']['sha256']
    source = json.loads(Path(plan['source']['path']).read_text())
    assert source['complete'] and source['profile'] == S.PROFILE
    assert plan['jobs'] == jobs(source, Path(plan['binary']['path']), out)
    assert P.digest(plan['binary']['path']) == plan['binary']['sha256'] == source['binary']['sha256']
    assert plan['runtime_source_commit'] == source['runtime_source_commit']
    expected = {g + '-' + label(m): source['runs'][g + ('-candidate' if m else '-control')]
                for g in GROUPS for m in (0, 7)}
    assert plan['replay_sources'] == expected
    for job in plan['jobs']:
        if job['item']['mask'] in (0, 7):
            prior = expected[job['item']['name']]
            assert job['command'][1:-1] == prior['command'][1:-1]
    return source


def freeze(out):
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()
    source = json.loads(SOURCE.read_text())
    assert source['complete'] and source['inputs'] == S.inputs()
    assert P.digest(source['binary']['path']) == source['binary']['sha256']
    out.mkdir(parents=True, exist_ok=False)
    for name in ('raw', 'logs', 'archives', 'inputs'):
        (out / name).mkdir()
    binary = out / 'surface_mission_soak'
    shutil.copy2(source['binary']['path'], binary); binary.chmod(0o555)
    plan = dict(schema=1, profile=PROFILE, inputs=inputs(),
        source=dict(path=str(SOURCE), sha256=P.digest(SOURCE)),
        runtime_source_commit=source['runtime_source_commit'],
        runner_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        binary=dict(path=str(binary), sha256=P.digest(binary)), jobs=jobs(source, binary, out),
        replay_sources={g + '-' + label(m): source['runs'][g + ('-candidate' if m else '-control')]
                        for g in GROUPS for m in (0, 7)}, default_changes=False, fresh_games=0)
    verify_plan(plan, out)
    I.write(out / 'plan.json', plan)
    print('Frozen 32 diagnostic games: four settings, eight factor combinations.', flush=True)


def run_case(job, plan, out, reaudit):
    result = copy.deepcopy(job); name = job['item']['name']; root = S.L.root_of(job)
    log, metadata = out / 'logs' / (name + '.log'), out / (name + '-raw.json')
    try:
        if metadata.exists():
            assert reaudit, 'existing game requires explicit evidence reuse'
            old = json.loads(metadata.read_text())
            assert old['item'] == job['item'] and old['command'] == job['command']
            assert P.digest(log) == old['log_sha256']
            if not root.exists():
                record = json.loads((out / (name + '-result.json')).read_text())
                B.unpack(record['archive'], root)
            result.update(hashes=old['hashes'], log_sha256=old['log_sha256'], reused_raw=True)
        else:
            assert not root.exists()
            with log.open('x') as stream:
                subprocess.run(job['command'], check=True, stdout=stream, stderr=stream, timeout=1800)
            result.update(hashes=I.raw_hashes(root), log_sha256=P.digest(log))
            I.write(metadata, result)
        assert all(P.digest(root / k) == v for k, v in result['hashes'].items())
        result.update(analyze(root, job, root))
        report = json.loads((root / 'report.json').read_text())
        result['defense'] = C.A.audit(root, job['item'], report)
        result['clearance'] = C.audit(root, job['item'], report)
        result['native'] = S.native_audit(root, report, job['item'])
        if (root / 'impact.jsonl').exists():
            result['impact'] = C.A.audit_impact(root, report)
        if name in plan['replay_sources']:
            prior = plan['replay_sources'][name]; oldroot = out / 'inputs' / name
            if not oldroot.exists(): B.unpack(prior['archive'], oldroot)
            result['retention'] = C.parity(prior, result, oldroot)
            shutil.rmtree(oldroot)
        assert all(P.digest(root / k) == v for k, v in result['hashes'].items())
        result['audited'] = True
    except Exception:
        result['error'] = traceback.format_exc()
    I.write(out / (name + '-result.json'), result)
    print(name, 'audited' if result.get('audited') else result['error'], flush=True)
    return result


def compare(a, b, reports):
    ra, rb = reports[a['item']['name']], reports[b['item']['name']]
    assert ra['initial_world'] == rb['initial_world']
    difference = I.first_control_difference(*(S.L.root_of(r) / 'destination-behavior.jsonl' for r in (a, b)))
    same = all(ra[k] == rb[k] for k in ('round', 'final_pilots', 'final_planets', 'final_audit',
                                       'final_combat', 'asteroid_events', 'elapsed_ticks'))
    assert same if difference is None else difference['reason'] == 'actions'
    seat = a['item']['seat']; x, y = a['players'][seat], b['players'][seat]
    accounting = S.departure_accounting(x, y, a['elapsed_ticks'], b['elapsed_ticks'])
    useful, missing = I.completion_changes(x, y, difference['tick'] if difference else None)
    useful = [v for v in useful if v['candidate'] not in accounting['post_victory_exempt_visits']['after']]
    benefits, regressions = C.changes(x, y)
    if useful: benefits.append('earlier_or_additional_departure')
    bit = a['item']['mask'] ^ b['item']['mask']
    return dict(group=a['item']['group'], before=a['item']['name'], after=b['item']['name'], seat=seat,
        before_mask=a['item']['mask'], after_mask=b['item']['mask'],
        factor=FACTORS.get(bit), first_control_difference=difference, same_physical_outcomes=same,
        outcome_transition=x['outcome'] + '->' + y['outcome'], benefits=benefits, regressions=regressions,
        useful_completed_changes=useful, missing_before_completions=missing, **accounting)


def witnesses(run, wanted):
    """Retain consumed first-change rows and a compact chronology of both actors."""
    previous, signatures, rows, timeline = {}, {}, {}, []
    for row in TRACE.rows(S.L.root_of(run) / 'trace.jsonl'):
        tick, seat = row['tick'], row['seat']; m, p = row['mission'], R.pilot(row)
        recovery, capture = m['recovery'], m['capture']
        state = dict(goal=m['goal'], target=m['target'],
            capture_started=(capture or {}).get('started_tick'), capture_goal=(capture or {}).get('goal'),
            combat_goal=(m['combat'] or {}).get('goal'),
            recovery_goal=(recovery or {}).get('goal'),
            ships_lost=(p['recovery'] or {}).get('ships_lost'),
            rebuilds=(p['recovery'] or {}).get('rebuilds'), completed_recoveries=m['completed_recoveries'])
        if signatures.get(seat) != state:
            timeline.append(dict(tick=tick, seat=seat, state=state, reason=m['reason'],
                ship_health=p['ship_health'], pilot_health=row['observation']['match_context']['pilot_health'],
                pursuit=m['pursuit']))
            signatures[seat] = state
        key = (tick, seat)
        if key in wanted:
            rows[f'{tick}:{seat}'] = dict(previous=previous.get(seat), current=row)
        previous[seat] = row
    assert set(rows) == {f'{t}:{s}' for t, s in wanted}
    path = S.L.root_of(run) / 'factor-witness-audit.json'
    I.write(path, dict(first_changes=rows, timeline=timeline))
    run['factor_witnesses'] = dict(path=str(path), sha256=P.digest(path), points=len(rows),
                                   timeline_events=len(timeline))
    return rows


def pack(run, out, reaudit):
    root = S.L.root_of(run)
    assert all(P.digest(root / k) == v for k, v in run['hashes'].items())
    suffix = '-' + P.digest(__file__)[:12] if reaudit else ''
    run['archive'] = B.pack(root, out / 'archives' / (root.name + suffix + '.tar.gz'))
    I.write(out / (root.name + '-result.json'), run)
    return run


def diagnosis(runs, comparisons, source):
    expected = {c['name']: c for c in cases(source)}
    assert set(runs) == set(expected)
    assert all(run['item'] == expected[name] for name, run in runs.items())
    assert len(comparisons) == 64
    assert {(p['group'], p['before_mask'], p['after_mask']) for p in comparisons} == {
        (g, a, b) for g in GROUPS for a, b in contrasts()}
    effects = {}
    for bit, factor in FACTORS.items():
        pairs = [p for p in comparisons if p['factor'] == factor]
        assert len(pairs) == 16
        effects[factor] = dict(changed_pairs=sum(p['first_control_difference'] is not None for p in pairs),
            useful_pairs=sum(bool(p['benefits']) for p in pairs),
            regressions=[dict(group=p['group'], before_mask=p['before_mask'], after_mask=p['after_mask'],
                              changes=p['regressions']) for p in pairs if p['regressions']])
    return dict(factors=effects, decision='diagnosis_only', fresh_games=0, default_promotion=False)


def execute(path, reaudit):
    plan = json.loads(path.read_text()); out = path.parent
    source = verify_plan(plan, out, reaudit); target = out / 'summary.json'
    if target.exists():
        assert reaudit
        backup = out / ('summary-before-reaudit-' + P.digest(target)[:12] + '.json')
        assert not backup.exists(); shutil.copy2(target, backup)
    summary = dict(schema=1, profile=PROFILE, complete=False, plan_sha256=P.digest(path), inputs=inputs(),
        runtime_source_commit=plan['runtime_source_commit'], runner_commit=plan['runner_commit'],
        binary=plan['binary'], reaudit=reaudit, runs={}, comparisons=[])
    I.write(target, summary)
    with ProcessPoolExecutor(max_workers=2) as pool:
        for group in GROUPS:
            selected = [j for j in plan['jobs'] if j['item']['group'] == group]
            for batch in (selected[:2], selected[2:]):
                futures = [pool.submit(run_case, j, plan, out, reaudit) for j in batch]
                failed = []
                for future in futures:
                    run = future.result(); summary['runs'][run['item']['name']] = run
                    I.write(target, summary)
                    if not run.get('audited'): failed.append(run['item']['name'])
                assert not failed, f'invalid games retained: {failed}'
            runs = {j['item']['mask']: summary['runs'][j['item']['name']] for j in selected}
            reports = {r['item']['name']: json.loads((S.L.root_of(r) / 'report.json').read_text()) for r in runs.values()}
            try:
                pairs = [compare(runs[a], runs[b], reports) for a, b in contrasts()]
                wanted = {r['item']['name']: set() for r in runs.values()}
                for p in pairs:
                    if (first := p['first_control_difference']) is not None:
                        for side in ('before', 'after'): wanted[p[side]].add((first['tick'], first['seat']))
                evidence = {r['item']['name']: witnesses(r, wanted[r['item']['name']]) for r in runs.values()}
                for p in pairs:
                    if (first := p['first_control_difference']) is not None:
                        key = f"{first['tick']}:{first['seat']}"
                        a, b = [evidence[p[side]][key]['current'] for side in ('before', 'after')]
                        assert a['actions'] == first['control'] and b['actions'] == first['candidate']
                        assert R.physical(a) == R.physical(b), 'physics changed before the first changed action'
                        p['first_change_physical_state_equal'] = True
                I.write(out / (group + '-comparisons.json'), pairs)
                summary['comparisons'].extend(pairs)
            except Exception:
                summary['error'] = traceback.format_exc(); I.write(target, summary)
                raise
            del reports, evidence
            for future in [pool.submit(pack, r, out, reaudit) for r in runs.values()]:
                run = future.result(); summary['runs'][run['item']['name']] = run; I.write(target, summary)
            print(group, 'compared and archived;', len(summary['runs']), '/ 32 games', flush=True)
    verify_plan(plan, out, reaudit)
    summary['diagnosis'] = diagnosis(summary['runs'], summary['comparisons'], source)
    summary['complete'] = True; I.write(target, summary)
    print('Complete: diagnosis only; no promotion.', flush=True)


if __name__ == '__main__':
    assert __debug__, 'auditors require assertions enabled'
    parser = argparse.ArgumentParser(description=__doc__); sub = parser.add_subparsers(dest='action', required=True)
    p = sub.add_parser('plan'); p.add_argument('--out', type=Path, required=True)
    p = sub.add_parser('run'); p.add_argument('--plan', type=Path, required=True); p.add_argument('--reaudit', action='store_true')
    args = parser.parse_args()
    if args.action == 'plan': freeze(args.out.resolve())
    else: execute(args.plan.resolve(), args.reaudit)
