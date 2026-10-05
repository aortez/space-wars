#!/usr/bin/env python3
"""Freeze and compare forecast-gated acquisition defense without changing defaults."""
import argparse
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
import copy
import csv
import importlib.util
from itertools import product, zip_longest
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('defense', Path(__file__).with_name('validate-acquisition-defense.py'))
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)
B, L, I, P, D, ROOT = A.B, A.L, A.I, A.P, A.D, A.ROOT
FIELD, FLAG = 'acquisition_clearance', '--acquisition-clearance-seats'
PROFILE = 'airborne_acquisition_clearance_v1'
NAMESPACE = PROFILE + ':held-out:2026-10'
PRIOR = ROOT / 'target/acquisition-defense/v1/summary.json'
KNOWN = tuple(group for group, _, _ in A.KNOWN) + ('known-boundary',)
BOUNDARY = 'fresh-world1-v10-asteroids3-p2'
RUNTIME = (
    'crates/spacewars-ai/src/mission_acquisition_defense.rs',
    'crates/spacewars-ai/src/mission_acquisition_defense_tests.rs',
    'crates/spacewars-ai/src/mission_pilot.rs',
    'crates/spacewars-ai/src/mission_policy.rs',
    'crates/spacewars-ai/src/mission_disengagement.rs',
    'crates/spacewars-ai/src/mission_boundary.rs',
    'crates/spacewars-ai/examples/surface_mission_soak.rs',
    'crates/spacewars-ai/examples/support/capture_evidence.rs',
)


def sources(prior):
    assert prior['complete'] and prior['profile'] == A.PROFILE
    assert prior['inputs'] == A.inputs()
    result = {}
    for group in KNOWN:
        old_group = BOUNDARY if group == 'known-boundary' else group
        result[group] = {mode: prior['runs'][old_group + suffix]
                         for mode, suffix in (('off', '-off'), ('legacy', '-on'))}
    return result


def cases(priors):
    result = []
    for group in KNOWN:
        for mode in ('off', 'legacy', 'on'):
            old = priors[group]['off']['item']
            result.append(dict(old, name=group + '-' + mode, group=group, stage='known_qualification',
                               world_cluster=group, mode=mode, defense=mode != 'off', clearance=mode == 'on'))
    for world, interval, seat in product(range(2), (0, 3), (0, 1)):
        group = f'fresh-world{world}-v10-asteroids{interval}-p{seat+1}'
        namespace = f'{NAMESPACE}:{world}'
        for mode in (('off', 'on') if (world + interval + seat) % 2 == 0 else ('on', 'off')):
            result.append(dict(name=group + '-' + mode, group=group, stage='held_out', mode=mode,
                arm='candidate', world='generated', seed=P.seed(namespace), seed_namespace=namespace,
                world_cluster=world, interval=interval, seat=seat, opponent=10, seconds=600,
                laser=True, defense=mode == 'on', clearance=mode == 'on'))
    return result


def command(old, binary, root, item):
    cmd = list(old)
    assert len(cmd) % 2 == 1 and len(cmd[1::2]) == len(set(cmd[1::2]))
    assert FLAG not in cmd
    cmd[0] = str(binary)
    cmd[cmd.index('--out') + 1] = str(root)
    value = str(item['seat']) if item['defense'] else 'none'
    if A.FLAG in cmd:
        cmd[cmd.index(A.FLAG) + 1] = value
    else:
        cmd += [A.FLAG, value]
    return cmd + [FLAG, str(item['seat']) if item['clearance'] else 'none']


def jobs(binary, out, priors):
    result = []
    for item in cases(priors):
        if item['stage'] == 'known_qualification':
            source = 'legacy' if item['defense'] else 'off'
            old = priors[item['group']][source]['command']
        else:
            old = [str(binary), *(v for pair in B.flags(item).items() for v in pair), '--out', 'unused']
        result.append(dict(item=item, command=command(old, binary, out / 'raw' / item['name'], item)))
    return result


def inputs():
    result = A.inputs()
    for path in ('tools/validate-acquisition-clearance.py', 'tools/tests/test_acquisition_clearance.py',
                 'docs/acquisition-clearance-plan.md', *RUNTIME):
        result[path] = P.digest(ROOT / path)
    return result


def strip_gate(value):
    if isinstance(value, dict):
        return {k: strip_gate(v) for k, v in value.items() if k not in (FIELD, FIELD + '_model')}
    if isinstance(value, list):
        return [strip_gate(v) for v in value]
    return value


def strip_options(value):
    return A.strip(strip_gate(value))


def audit_check(trace, witness, previous, committed, previous_defense):
    tick, m, o = trace['tick'], trace['mission'], trace['observation']
    state = m[FIELD]; check = state['last']; p = o['local']['combat']['recovery']['flight']['pilot']
    assert check['tick'] == tick and witness['observation'] == o and witness['telemetry'] == state
    assert state['checks'] == previous['checks'] + 1
    # Bind the complete native eligibility and first-site history, even when no
    # actual defense attempt is created. This synthetic shape is audit-only.
    candidate = dict(check, started_tick=tick, deadline_tick=tick + 720,
                     finished_tick=None, clear_since=None)
    A.audit_start(o, candidate, committed)
    clearance = check['estimated_clearance']
    assert math.isfinite(clearance), 'nonfinite native game forecast is invalid evidence'
    admitted = clearance >= 0.0
    assert check['decision'] == ('admitted' if admitted else 'negative_clearance')
    assert state['rejected'] == previous['rejected'] + int(not admitted)
    if admitted:
        attempt = m[A.FIELD]['last']
        assert attempt['started_tick'] == tick
        for key in ('planet', 'vehicle', 'opponent', 'hit_source', 'capture', 'native_actions',
                    'direction', 'estimated_min_range', 'estimated_clearance'):
            assert attempt[key] == check[key], key
    else:
        assert trace['actions'] == check['native_actions'], 'rejection changed native actions'
        assert m['capture'] == check['capture'] and m['target'] == p['planet']['index']
        assert m['goal'] == 'capture' and m['recovery'] is None
        assert m[A.FIELD] == previous_defense, 'rejection started or changed an escape'
        assert not any(e['tick'] == tick and e['kind'] in ('replan', 'acquisition_defense_started')
                       for e in m['events']), 'rejection changed coordinator task'
    return state


def audit(root, item, report):
    enabled, seat = item['clearance'], item['seat']
    assert not enabled or item['defense']
    expected = dict(profile=PROFILE, enabled_seats=[s == seat for s in (0, 1)]) if enabled else None
    assert report.get(FIELD) == expected
    for s, descriptor in enumerate(report['policy_configuration']):
        assert descriptor.get(FIELD + '_model') == (PROFILE if enabled and s == seat else None)
    state = dict(checks=0, rejected=0, last=None)
    defense = dict(attempts=0, separated=0, timed_out=0, last=None) if item['defense'] else None
    witnesses, committed, decisions = [], set(), Counter()
    rows = 0
    for row, trace in zip_longest(D.rows(root / 'capture-evidence.jsonl'), D.rows(root / 'trace.jsonl')):
        assert row is not None and trace is not None
        tick, actor = rows // 2, rows % 2
        assert (trace['tick'], trace['seat']) == (row['pilot']['tick'], row['seat']) == (tick, actor)
        m, o = trace['mission'], trace['observation']; p = o['local']['combat']['recovery']['flight']['pilot']
        if actor == seat:
            capture = m['capture']
            if capture and (capture['site'] is not None or p['location'] == 'on_foot' or p['landing']['supported_feet'] > 0):
                committed.add(capture['started_tick'])
        if not enabled or actor != seat:
            assert FIELD not in m and FIELD not in row
        else:
            gate = m[FIELD]
            current = gate['last'] is not None and gate['last']['tick'] == tick
            assert (FIELD in row) == current
            if current:
                state = audit_check(trace, row[FIELD], state, committed, defense)
                witnesses.append(row)
                decisions[state['last']['decision']] += 1
            assert gate == state, 'unwitnessed gate update'
            attempt = m[A.FIELD]['last']
            if attempt is not None and attempt['started_tick'] == tick:
                assert current and state['last']['decision'] == 'admitted'
        if actor == seat:
            defense = m.get(A.FIELD)
        rows += 1
    assert rows == report['elapsed_ticks'] * 2
    for actor, mission in enumerate(report['missions']):
        assert mission.get(FIELD) == (state if enabled and actor == seat else None)
    result = dict(enabled=enabled, counters=state if enabled else None, decisions=dict(decisions),
        dense_rows=rows, checks=[w[FIELD]['telemetry']['last'] for w in witnesses])
    I.write(root / 'clearance-audit.json', dict(summary=result, witnesses=witnesses))
    return result


def parity(prior, run, oldroot, normalize=False):
    root = L.root_of(run)
    assert set(prior['hashes']) == set(run['hashes'])
    exact, projected = [], []
    for name in run['hashes']:
        if name in ('report.json', 'sensors.jsonl', 'live-planning.csv'):
            continue
        if normalize and name in ('trace.jsonl', 'capture-evidence.jsonl'):
            for a, b in zip_longest(D.rows(oldroot / name), D.rows(root / name)):
                assert a is not None and b is not None and a == strip_gate(b), name
            projected.append(name)
        else:
            assert prior['hashes'][name] == run['hashes'][name], name
            exact.append(name)
    a, b = [json.loads((r / 'report.json').read_text()) for r in (oldroot, root)]
    assert D.timing_free(a) == D.timing_free(strip_gate(b) if normalize else b)
    sensors = D.compare_jsonl(oldroot / 'sensors.jsonl', root / 'sensors.jsonl')
    with (oldroot / 'live-planning.csv').open() as a, (root / 'live-planning.csv').open() as b:
        left, right = [[D.timing_free(row) for row in csv.DictReader(f)] for f in (a, b)]
    assert left == right
    A.retained_results(prior, run)
    return dict(exact_streams=sorted(exact), projected_streams=projected,
                sensor_rows=sensors, planning_rows=len(left), non_timing_report=True,
                physical_and_planning_results=True)


def prefix(before, after, first):
    count = 0
    for a, b in zip_longest(before, after):
        assert a is not None and b is not None, 'ending changed before handoff'
        assert (a['tick'], a['seat']) == (b['tick'], b['seat'])
        if first == (b['tick'], b['seat']):
            attempt = b['mission'][A.FIELD]['last']
            assert attempt['started_tick'] == b['tick']
            assert a['observation'] == b['observation']
            assert a['actions'] == attempt['native_actions']
            assert a['mission']['capture'] == attempt['capture']
            return dict(tick=b['tick'], seat=b['seat'], exact_prefix_rows=count,
                        source_observation_capture_and_native_actions_equal=True)
        assert a == strip_options(b), 'state changed before accepted handoff'
        count += 1
    assert first is None
    return dict(tick=None, exact_prefix_rows=count)


def changes(x, y):
    benefits, regressions = [], []
    if B.POINTS[y['outcome']] > B.POINTS[x['outcome']]: benefits.append('more_match_points')
    if B.POINTS[y['outcome']] < B.POINTS[x['outcome']]: regressions.append('fewer_match_points')
    for key in ('ships_lost', 'pilot_deaths'):
        if y[key] < x[key]: benefits.append('fewer_' + key)
        if y[key] > x[key]: regressions.append('more_' + key)
    if x['death_tick'] is not None and y['death_tick'] is not None:
        if y['death_tick'] > x['death_tick']: benefits.append('later_pilot_death')
        if y['death_tick'] < x['death_tick']: regressions.append('earlier_pilot_death')
    return benefits, regressions


def compare(a, b):
    roots = [L.root_of(r) for r in (a, b)]
    reports = [json.loads((r / 'report.json').read_text()) for r in roots]
    assert reports[0]['initial_world'] == reports[1]['initial_world']
    tick, seat = b['defense']['first_tick'], b['item']['seat']
    difference = prefix(*(D.rows(r / 'trace.jsonl') for r in roots), (tick, seat) if tick is not None else None)
    control = I.first_control_difference(*(r / 'destination-behavior.jsonl' for r in roots))
    if control:
        assert control['reason'] == 'actions' and tick is not None and control['tick'] >= tick
    else:
        for key in ('round', 'final_pilots', 'final_planets', 'final_audit', 'final_combat', 'elapsed_ticks'):
            assert reports[0][key] == reports[1][key], 'physical change without changed actions'
    if tick is None:
        assert D.timing_free(reports[0]) == D.timing_free(strip_options(reports[1]))
        for name in a['hashes']:
            if name in ('trace.jsonl', 'capture-evidence.jsonl'):
                for x, y in zip_longest(*(D.rows(r / name) for r in roots)):
                    assert x is not None and y is not None and x == strip_options(y)
            elif name == 'sensors.jsonl':
                D.compare_jsonl(*(r / name for r in roots))
            elif name == 'live-planning.csv':
                with (roots[0] / name).open() as x, (roots[1] / name).open() as y:
                    assert [D.timing_free(r) for r in csv.DictReader(x)] == [D.timing_free(r) for r in csv.DictReader(y)]
            elif name != 'report.json':
                assert a['hashes'][name] == b['hashes'][name], name
        A.retained_results(json.loads(json.dumps(a)), b)
    x, y = [r['players'][seat] for r in (a, b)]
    useful, missing = I.completion_changes(x, y, control['tick'] if control else None)
    benefits, regressions = changes(x, y)
    if useful: benefits.append('earlier_or_additional_departure')
    if control is None: assert not benefits and not regressions
    horizon = min(a['elapsed_ticks'], b['elapsed_ticks'])
    return dict(group=a['item']['group'], stage=a['item']['stage'], seat=seat,
        configuration=a['item']['arm'], world_cluster=a['item']['world_cluster'], interval=a['item']['interval'],
        off=a['item']['name'], on=b['item']['name'], first_handoff=difference,
        first_control_difference=control, benefits=benefits, regressions=regressions,
        outcome_transition=x['outcome'] + '->' + y['outcome'], useful_completed_changes=useful,
        missing_off_completions=missing, common_horizon=horizon,
        common_horizon_completed_visits={arm:[v for v in r['players'][seat]['visits']
            if v['departed_tick'] is not None and v['departed_tick'] <= horizon] for arm,r in (('off',a),('on',b))},
        off_visits_after_enabled_early_victory=[v for v in x['visits']
            if y['outcome']=='win' and b['elapsed_ticks']<a['elapsed_ticks']
            and v['departed_tick'] is not None and v['departed_tick']>horizon])


def legacy_comparison(old, run):
    checks = run['clearance']['checks']
    rejections = [c for c in checks if c['decision'] != 'admitted']
    if not rejections:
        return dict(first_rejection=None, parity=parity(json.loads(json.dumps(old)), run, L.root_of(old), True))
    first = rejections[0]; count = 0
    for a, b in zip_longest(D.rows(L.root_of(old) / 'trace.jsonl'), D.rows(L.root_of(run) / 'trace.jsonl')):
        assert a is not None and b is not None
        assert (a['tick'], a['seat']) == (b['tick'], b['seat'])
        if (b['tick'], b['seat']) == (first['tick'], run['item']['seat']):
            attempt = a['mission'][A.FIELD]['last']
            assert attempt['started_tick'] == first['tick']
            assert a['observation'] == b['observation'] and first['capture'] == attempt['capture']
            assert first['native_actions'] == attempt['native_actions'] == b['actions']
            assert first['estimated_clearance'] == attempt['estimated_clearance'] < 0
            return dict(first_rejection=first, exact_prefix_rows=count,
                        source_observation_capture_and_proposal_equal=True)
        assert a == strip_gate(b)
        count += 1
    raise AssertionError('missing rejection row')


def qualification(runs, pairs):
    reasons = []
    for p in pairs:
        assert p['stage'] == 'known_qualification'
        if p['regressions']: reasons.append(p['group'] + ':' + ','.join(p['regressions']))
        legacy = runs[p['group'] + '-legacy']['players'][p['seat']]
        current = runs[p['on']]['players'][p['seat']]
        _, regressed = changes(legacy, current)
        if regressed: reasons.append(p['group'] + ':legacy:' + ','.join(regressed))
    rescued = runs['known-lost-win-on']
    if rescued['players'][rescued['item']['seat']]['outcome'] != 'win': reasons.append('known_rescue_not_retained')
    boundary = runs['known-boundary-on']
    checks = boundary['clearance']['checks']
    if not any(c['tick']==15455 and c['decision']=='negative_clearance' for c in checks):
        reasons.append('boundary_proposal_not_rejected')
    p = next(p for p in pairs if p['group']=='known-boundary')
    if p['common_horizon_completed_visits']['off'] != p['common_horizon_completed_visits']['on']:
        reasons.append('boundary_completed_visits_changed')
    return dict(passed=not reasons, reasons=reasons, default_promotion=False)


def screen(runs, pairs):
    fresh = [p for p in pairs if p['stage']=='held_out']
    assert len(fresh)==8
    def totals(selected):
        return {arm:I.totals([runs[p[arm]]['players'][p['seat']] for p in selected])for arm in ('off','on')}
    reasons = []
    if not any(p['benefits'] for p in fresh): reasons.append('no_useful_fresh_change')
    strata = {'overall':totals(fresh)}
    for key in ('seat','interval','world_cluster'):
        for value in sorted({p[key]for p in fresh}):
            strata[f'{key}:{value}']=totals([p for p in fresh if p[key]==value])
    for key,value in strata.items():
        if value['on']['points'] < value['off']['points']: reasons.append('fresh_points_regressed:'+key)
    for key in ('ships_lost','pilot_deaths'):
        if strata['overall']['on'][key] > strata['overall']['off'][key]: reasons.append('fresh_'+key+'_increased')
    for p in fresh:
        if 'earlier_pilot_death' in p['regressions']: reasons.append('earlier_pilot_death:'+p['group'])
    return dict(decision='retain' if reasons else 'advance_to_broader_comparison', reasons=reasons,
                default_promotion=False, fresh_world_clusters=2, fresh_strata=strata,
                regression_pairs={p['group']:p['regressions']for p in fresh if p['regressions']},
                fresh_changed_pairs=sum(p['first_control_difference'] is not None for p in fresh))


def run_case(job, plan):
    result = copy.deepcopy(job); root = L.root_of(job); out = root.parent.parent
    name = job['item']['name']; log = out / 'logs' / (name + '.log')
    try:
        assert not root.exists()
        with log.open('x') as stream:
            subprocess.run(job['command'], stdout=stream, stderr=stream, check=True, timeout=1800)
        result.update(hashes=I.raw_hashes(root), log_sha256=P.digest(log))
        I.write(out / (name + '-raw.json'), result)
        result.update(I.analyze(root, job, root))
        report = json.loads((root / 'report.json').read_text())
        result['defense'] = A.audit(root, job['item'], report)
        result['clearance'] = audit(root, job['item'], report)
        if (root / 'impact.jsonl').exists(): result['impact'] = A.audit_impact(root, report)
        if job['item']['stage']=='known_qualification' and job['item']['mode']!='on':
            old = plan['sources'][job['item']['group']][job['item']['mode']]
            extraction = out / 'inputs' / name
            B.unpack(old['archive'], extraction)
            result['retention'] = parity(old, result, extraction)
            assert json.loads(json.dumps(result['defense'])) == old['defense']
            assert {p.name:dict(sha256=P.digest(p),bytes=p.stat().st_size)for p in extraction.iterdir()}==old['archive']['files']
            shutil.rmtree(extraction)
        assert all(P.digest(root/k)==v for k,v in result['hashes'].items())
        result['audited'] = True
    except Exception:
        result['error'] = traceback.format_exc()
    return result


def freeze(out):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior = json.loads(PRIOR.read_text()); priors = sources(prior)
    seeds = {c['seed']for c in cases(priors) if c['stage']=='held_out'}
    old_seeds = {c['seed']for c in B.cases()} | {c['seed']for c in P.plan()}
    old_seeds |= {r['item']['seed']for r in prior['runs'].values()}
    assert len(seeds)==2 and not seeds & old_seeds
    out.mkdir(parents=True,exist_ok=False)
    for directory in ('raw','logs','archives','inputs'): (out/directory).mkdir()
    binary = out/'surface_mission_soak'
    shutil.copy2(ROOT/'target/release/examples/surface_mission_soak',binary); binary.chmod(0o555)
    plan = dict(schema=1,profile=PROFILE,namespace=NAMESPACE,default_changes=False,
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        binary=dict(path=str(binary),sha256=P.digest(binary)),inputs=inputs(),sources=priors,
        prior_summary=dict(path=str(PRIOR),sha256=P.digest(PRIOR)),jobs=jobs(binary,out,priors))
    assert len(plan['jobs'])==31
    I.write(out/'plan.json',plan)
    print('Frozen 15 known games and 16 conditional fresh games',flush=True)


def execute(path):
    plan=json.loads(path.read_text()); out=path.parent
    assert plan['profile']==PROFILE and plan['namespace']==NAMESPACE
    assert inputs()==plan['inputs'] and P.digest(plan['binary']['path'])==plan['binary']['sha256']
    assert P.digest(plan['prior_summary']['path'])==plan['prior_summary']['sha256']
    assert plan['jobs']==jobs(Path(plan['binary']['path']),out,plan['sources'])
    summary_path=out/'summary.json'
    assert not summary_path.exists(), 'never rerun cached games to repair an audit'
    result=dict(schema=1,profile=PROFILE,complete=False,plan_sha256=P.digest(path),
        source_commit=plan['source_commit'],binary=plan['binary'],inputs=plan['inputs'],runs={},pairs=[],legacy={})
    def save(): I.write(summary_path,result)
    def batch(pool, selected):
        pending={j['item']['name']:pool.submit(run_case,j,plan) for j in selected}
        errors=[]
        for name,future in pending.items():
            try: run=future.result()
            except Exception: run=dict(error=traceback.format_exc())
            result['runs'][name]=run; save()
            if 'error'in run: errors.append((name,run['error']))
            else: print(name,'audited',flush=True)
        # Drain both jobs and retain native pre-audit metadata before stopping.
        assert not errors, errors
    def pack(run):
        run['archive']=B.pack(L.root_of(run),out/'archives'/(run['item']['name']+'.tar.gz'))
        save()
    def restore(run):
        if not L.root_of(run).exists(): B.unpack(run['archive'],L.root_of(run))
    save()
    try:
        with ProcessPoolExecutor(max_workers=2) as pool:
            # Verify both previous option states before running the new policy.
            disabled=[j for j in plan['jobs']if j['item']['stage']=='known_qualification' and not j['item']['clearance']]
            for i in range(0,len(disabled),2):
                selected=disabled[i:i+2]; batch(pool,selected)
                for job in selected: pack(result['runs'][job['item']['name']])
            for group in KNOWN:
                job=next(j for j in plan['jobs']if j['item']['name']==group+'-on')
                batch(pool,[job])
                off,legacy,on=[result['runs'][group+'-'+mode]for mode in ('off','legacy','on')]
                restore(off); restore(legacy)
                result['pairs'].append(compare(off,on))
                result['legacy'][group]=legacy_comparison(legacy,on)
                for run in (off,legacy,on): pack(run)
                print(group,result['pairs'][-1]['outcome_transition'],result['pairs'][-1]['regressions'],flush=True)
            result['qualification']=qualification(result['runs'],result['pairs']); save()
            if result['qualification']['passed']:
                for group in dict.fromkeys(j['item']['group']for j in plan['jobs']if j['item']['stage']=='held_out'):
                    pair=[j for j in plan['jobs']if j['item']['group']==group]
                    batch(pool,pair)
                    off,on=[result['runs'][group+'-'+mode]for mode in ('off','on')]
                    result['pairs'].append(compare(off,on))
                    for run in (off,on):pack(run)
                    p=result['pairs'][-1]
                    print(group,p['outcome_transition'],p['benefits'],p['regressions'],flush=True)
                result['screen']=screen(result['runs'],result['pairs'])
            else:
                result['screen']=dict(decision='retain',reasons=['qualification_failed'],
                    default_promotion=False,fresh_games_not_submitted=16)
        assert inputs()==plan['inputs'] and P.digest(plan['binary']['path'])==plan['binary']['sha256']
        assert P.digest(plan['prior_summary']['path'])==plan['prior_summary']['sha256']
        result['complete']=True
    except Exception:
        result['error']=traceback.format_exc(); raise
    finally: save()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='action',required=True)
    sub.add_parser('plan').add_argument('--out',type=Path,required=True)
    sub.add_parser('run').add_argument('--plan',type=Path,required=True)
    args=parser.parse_args()
    if args.action=='plan':freeze(args.out.resolve())
    else:execute(args.plan.resolve())


if __name__=='__main__': main()
