#!/usr/bin/env python3
"""Measure the fixed20 native choices through one physical capture attempt."""
import argparse
from collections import Counter, defaultdict
import csv
import gzip
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location('landing_choices', Path(__file__).with_name('compare-landing-choices.py'))
Q = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(Q)
P, L, C, T, F = Q.P, Q.L, Q.C, Q.T, Q.F
HORIZON_SECONDS = 120
NAMES = ['choice', 'landed', 'exited', 'claim_started', 'claimed', 'boarded', 'departed']
PHASES = ['landing', 'exit', 'outbound', 'claim', 'return_board', 'departure']
RETRIES = ['replans', 'invalidations', 'cover_replans', 'solar_replans', 'circling_replans', 'objective_replans']
CENSORS = {'observation_horizon', 'match_finished', 'runner_ended'}


def live_work(root):
    with (root/'live-planning.csv').open() as stream:
        return [{k: v for k, v in r.items() if k != 'dispatch_ms'} for r in csv.DictReader(stream)]


def prefix_parity(before, after):
    """Prior executable ends at first choice; no claim of parity for the suffix."""
    old, new = [json.loads((root/'report.json').read_text())['transfer_probe'] for root in [before, after]]
    assert Q.without_wall_times(old) == Q.without_wall_times(
        {k: v for k, v in new.items() if k != 'capture_followthrough'} | {'scope':old['scope']})
    digest, rows, size = hashlib.sha256(), 0, 0
    with gzip.open(before/'trace.jsonl.gz', 'rb') as a, (after/'trace.jsonl').open('rb') as b:
        for line in a:
            assert line == next(b), 'first-choice controller/observation prefix changed'
            digest.update(line)
            rows += 1
            size += len(line)
    assert rows == 2*(old['acquisition']['outcome']['tick']+1)
    sensors, sensor_digest = 0, hashlib.sha256()
    with (before/'sensors.jsonl').open() as a, (after/'sensors.jsonl').open() as b:
        for line in a:
            old_row, new_row = [Q.without_wall_times(json.loads(v)) for v in [line, next(b)]]
            assert old_row == new_row, 'sensor counts or call order changed'
            sensor_digest.update((json.dumps(old_row, sort_keys=True, separators=(',', ':'))+'\n').encode())
            sensors += 1
    assert sensors == rows
    for name in P.UPSTREAM:
        data = (before/name).read_bytes()
        with (after/name).open('rb') as stream: assert stream.read(len(data)) == data, name
    assert F.digest(before/'transfer-probe.jsonl') == F.digest(after/'transfer-probe.jsonl')
    a, b = [live_work(root) for root in [before, after]]
    assert b[:len(a)] == a
    charges = [P.evaluation_charges(root, work) for root, work in zip([before, after], [a, b])]
    assert charges[1][:len(charges[0])] == charges[0]
    return dict(controller_rows=rows, decompressed_bytes=size, trace_sha256=digest.hexdigest(),
        sensors=dict(rows=sensors, work_sha256=sensor_digest.hexdigest()),
        exact_transfer_and_acquisition=True, exact_upstream_prefix=True,
        whole_run_budget_ticks=len(charges[1]), whole_run_evaluator_charged=sum(charges[1]))



def audit_budget(root, report):
    """Reconcile every consumer, including the new physical suffix."""
    elapsed = report['elapsed_ticks']
    live, charges, totals = live_work(root), defaultdict(lambda:[0,0]), {}
    for row in live:
        tick = int(row['tick'])
        g, q, tg, tq = [int(row[k]) for k in ['graph','queries','total_graph','total_queries']]
        assert 0 <= tick < elapsed and int(row['graph_budget']) == 4 and int(row['query_budget']) == 384
        assert 0 <= g <= tg <= 4 and 0 <= q <= tq <= 384
        assert totals.setdefault(tick, (tg,tq)) == (tg,tq)
        charges[tick][0] += g
        charges[tick][1] += q
    assert all(tuple(charges[t]) == v for t,v in totals.items())
    flag, shadow = [list(T.rows(root/name)) for name in ['flag-survey-work.jsonl','flag-value-shadow-work.jsonl']]
    assert len(flag) == len(shadow) == elapsed
    evaluator, fg, fq, sg, maxg, maxq = [], 0, 0, 0, 0, 0
    for tick,(f,s) in enumerate(zip(flag,shadow)):
        assert f['tick'] == s['tick'] == tick
        lg,lq = charges[tick]
        rem,a = f['remaining_after_evaluation'],f['allocation']
        used = a['charged']
        assert rem['physics_queries'] == 384-lq and 0 <= rem['graph'] <= 4-lg
        e = 4-lg-rem['graph']
        evaluator.append(e)
        assert a['tick'] == tick+1 and a['allowance'] == dict(graph=min(rem['graph'],2),physics_queries=rem['physics_queries'])
        assert 0 <= used['graph'] <= min(rem['graph'],2) and 0 <= used['physics_queries'] <= rem['physics_queries']
        for key in ['graph','physics_queries']:
            assert sum(j['charged'][key] for j in a['jobs']) == used[key]
            assert all(0 <= j['charged'][key] <= j['limits']['per_tick'][key] for j in a['jobs'])
        assert s['remaining_after_flag_survey'] == dict(graph=rem['graph']-used['graph'],physics_queries=0)
        assert 0 <= s['charged']['graph'] <= rem['graph']-used['graph'] and s['charged']['physics_queries'] == 0
        fg += used['graph']
        fq += used['physics_queries']
        sg += s['charged']['graph']
        maxg,maxq = max(maxg,lg+e+used['graph']+s['charged']['graph']),max(maxq,lq+used['physics_queries'])
    lgr,lqu = [sum(x[i] for x in charges.values()) for i in [0,1]]
    assert lgr == report['live_objective_planning']['telemetry']['graph']
    assert lqu == report['live_objective_planning']['telemetry']['physics_queries']
    assert sum(evaluator) == report['mission_evaluation']['charged']
    assert fg == report['flag_survey']['telemetry']['graph'] and fq == report['flag_survey']['telemetry']['physics_queries']
    assert sg == report['flag_survey']['shadow']['charged'] and maxg <= 4 and maxq <= 384
    return dict(executed_ticks=elapsed,live_graph=lgr,live_queries=lqu,evaluator_graph=sum(evaluator),flag_graph=fg,
        flag_queries=fq,shadow_graph=sg,max_combined_graph=maxg,max_combined_queries=maxq,
        evaluator_charge_sha256=hashlib.sha256(bytes(evaluator)).hexdigest())


def cost_reference(source, row):
    p = P.pilot(row)
    c = row['mission']['capture']
    assert source['pilot'] == p and source['mission'] == row['mission']
    assert source['physics_queries'] == 0
    r = source['reference']
    if r is None:
        assert isinstance(source['reference_unknown'], str) and source['reference_unknown']
        return None
    assert source['reference_unknown'] is None
    assert r['source_tick'] == r['observed_choice_tick'] == row['tick']
    assert r['selected_site'] == c['site'] and r['destination'] == p['planet']['index']
    assert r['visit_tick'] == max(e['tick'] for e in row['mission']['events']
                                if e['kind'] == 'selected' and e['planet'] == r['destination'])
    assert r['elapsed_landing_ticks'] == 0 and r['full'] == r['remaining']
    L.audit_local(r)
    if r['full'] is None:
        assert r['unknown']
        return None
    assert r['unknown'] is None and r['model'] == 'source_local_reference_v1'
    e, planet = r['evidence'], p['planet']
    site = next(s for s in p['sites'] if s['id'] == c['site'])
    assert e['site'] == site['id'] and e['revision'] == planet['revision'] == site['revision']
    assert e['observed_owner'] is None and planet['claim']['owner'] is None and planet['claim']['flag'] is None
    assert not e['remote'] and e['tick'] == row['tick'] and e['age_ticks'] == 0
    assert e['radius'] == planet['radius'] and e['stage_seconds'] == planet['claim']['stage_required_seconds']
    assert e['gravity'] == row['observation']['local']['objective_gravity']
    assert e['choice'] == [r['visit_tick'], row['tick']]
    Q.close(e['flag_range'], Q.f32(Q.f32(planet['claim']['flag_interaction_range'])-Q.f32(.2)), .000001)
    assert any(site['boarding_hatches'])
    assert any(v['site'] == site['id'] and all(v[k] for k in ['grounded', 'approach', 'departure'])
               for v in row['observation']['local']['cover'])
    expected = [17.866667, 1/60, 4/60, 3+1/60, 2/60, 3.766667]
    for key, value in zip(PHASES, expected): Q.close(r['full'][key], value, .000003)
    return r['full']


def retry_counts(c):
    return [c[k] for k in RETRIES]+[c['landing'][k] for k in ['invalidations', 'landing_retries']]


def interruption(source, row, milestones, tactical):
    """Check identity/context before accepting any new physical progress."""
    p, m, origin = P.pilot(row), row['mission'], source['pilot']
    destination = origin['planet']['index']
    if any(p[k] != origin[k] for k in ['owner', 'vehicle', 'spaceling']): return 'actor_or_vehicle_changed'
    if (not p['ship_available'] or p['ship_health'] <= 0 or p['ship_form'] != 'ship'
        or p['location'] == 'on_foot' and p['actor'] is None): return 'ship_or_pilot_lost'
    if m['goal'] == 'recover' or m['recovery'] is not None: return 'recovery'
    planet = next((v for v in row['observation']['planets'] if v['index'] == destination), None)
    if planet is None: return 'destination_absent'
    claim = planet['claim']
    if claim is None: return 'claim_unavailable'
    original = origin['planet']['claim']
    if (any(planet[k] != origin['planet'][k] for k in ['revision', 'radius'])
        or any(claim[k] != original[k] for k in ['stage_required_seconds', 'flag_interaction_range'])):
        return 'material_or_rules_changed'
    if (claim['owner'] not in [None, p['owner']] or claim['claimant'] not in [None, p['owner']]
        or claim['flag'] is not None and claim['flag']['player'] != p['owner']): return 'foreign_claim_context'
    if milestones['claimed'] is not None and (claim['owner'] != p['owner'] or claim['flag'] is None): return 'ownership_lost'
    if (m['replans'] != source['mission']['replans'] or row['tick'] > origin['tick']
        and any(e['tick'] == row['tick'] and e['kind'] in ['selected', 'replan', 'arrived'] for e in m['events'])):
        return 'attempt_replaced'
    if any(e['tick'] == row['tick'] and e['kind'] == 'departed' and e['planet'] == destination for e in m['events']):
        distance = math.dist(Q.vec(p['ship']['position']), Q.vec(planet['motion']['position']))
        if (milestones['boarded'] is None or p['location'] != origin['location']
            or m['completed_sorties'] != source['mission']['completed_sorties']+1
            or not (tactical is not None or distance > planet['radius']+70)):
            return 'unwitnessed_departure'
        return 'departed'
    if m['target'] != destination or m['goal'] not in ['capture', 'avoid_sun']: return 'retargeted'
    c = m['capture']
    if c is None: return 'capture_ended'
    if c['failed_tick'] is not None or c['failure'] is not None: return 'capture_failed'
    original_capture = source['mission']['capture']
    if c['started_tick'] != original_capture['started_tick'] or retry_counts(c) != retry_counts(original_capture):
        return 'capture_replanned'
    if c['site'] != original_capture['site'] or c['landing']['site'] not in [None, original_capture['site']]: return 'site_changed'
    if p['planet']['index'] != destination and milestones['boarded'] is None: return 'approach_frame_changed'
    return None


def reconstruct(source, control):
    """Milestones require dense native clocks AND independent physical witnesses."""
    start, origin = control[0]['tick'], source['pilot']
    milestones = dict.fromkeys(NAMES)
    milestones['choice'] = start
    tactical, touchdown, reason = None, None, None
    for offset, row in enumerate(control):
        assert row['tick'] == P.pilot(row)['tick'] == start+offset
        assert reason is None, 'trace continues beyond first attempt endpoint'
        reason = interruption(source, row, milestones, tactical)
        if reason == 'departed': milestones['departed'] = row['tick']
        if reason is not None: continue
        p, c = P.pilot(row), row['mission']['capture']
        planet = next(v for v in row['observation']['planets'] if v['index'] == origin['planet']['index'])
        claim, landed = planet['claim'], c['landing']
        owned = (claim['owner'] == origin['owner'] and claim['flag'] is not None
                 and claim['flag']['player'] == origin['owner'] and claim['captures'] > origin['planet']['claim']['captures'])
        events = [
            ('landed', landed['landed_tick'], p['landing']['phase'] == 'landed' and p['location'] == origin['location']),
            ('exited', row['tick'] if p['location'] == 'on_foot' else None,
                p['last_transfer'] == 'exited' and p['transfers'] > origin['transfers']),
            ('claim_started', row['tick'] if p['location'] == 'on_foot' and claim['claimant'] == p['owner']
                and claim['phase'] in ['raising', 'lowering'] and claim['progress'] > 0 else None, True),
            ('claimed', landed['claimed_tick'], owned and p['location'] == 'on_foot'),
            ('boarded', landed['boarded_tick'], p['location'] == origin['location']
                and p['last_transfer'] == 'boarded' and p['transfers'] >= origin['transfers']+2)]
        for key, tick, witness in events:
            if tick is None or milestones[key] is not None: continue
            if tick != row['tick'] or not witness or milestones[NAMES[NAMES.index(key)-1]] is None:
                reason = 'unwitnessed_milestone'
                break
            milestones[key] = tick
            if key == 'landed': touchdown = row
        if reason is not None: continue
        completed = c['completed_tick']
        if completed is not None:
            if milestones['boarded'] is None or not milestones['boarded'] <= completed <= row['tick']:
                reason = 'unwitnessed_completion'
                continue
            if tactical is None: tactical = completed
        if row['tick']-start >= HORIZON_SECONDS*60: reason = 'observation_horizon'
    return milestones, tactical, touchdown, reason


def phases(milestones, prediction, outcome):
    result = {}
    for index, name in enumerate(PHASES):
        begin, end = [milestones[k] for k in NAMES[index:index+2]]
        expected = prediction[name] if prediction else None
        seconds = (end-begin)/60 if begin is not None and end is not None else None
        status = ('completed' if seconds is not None else 'not_started' if begin is None else
                  'censored' if outcome['reason'] in CENSORS else 'interrupted')
        result[name] = dict(status=status, predicted_seconds=expected, observed_seconds=seconds,
            error_seconds=seconds-expected if seconds is not None and expected is not None else None,
            incomplete_elapsed_seconds=(outcome['tick']-begin)/60 if begin is not None and end is None else None)
    return result


def audit_capture(case, report, control):
    probe = report['transfer_probe']
    r = probe['capture_followthrough']
    assert r['schema'] == 1 and r['horizon_ticks'] == HORIZON_SECONDS*60
    a = probe['acquisition']['outcome']
    if a is None or a['reason'] != 'site_selected':
        assert r['source'] is None and r['outcome'] is None and r['observed_rows'] == 0
        assert all(v is None for v in r['milestones'].values())
        return dict(outcome='no_site_choice', prediction=None, phases={})
    rows = [v for v in control if v['tick'] >= a['tick']]
    assert rows and rows[0]['tick'] == a['tick']
    source, outcome = r['source'], r['outcome']
    if outcome['reason'] == 'unsupported_source':
        p = P.pilot(rows[0])
        assert source['pilot'] == p and source['mission'] == rows[0]['mission']
        assert (p['planet']['claim'] is None or p['planet']['claim']['owner'] is not None
                or p['planet']['claim']['flag'] is not None or probe['landing_choice']['report'] is None)
        assert len(rows) == r['observed_rows'] == 1 and outcome['controller_observed']
        assert outcome['tick'] == r['last_observed_tick'] == report['elapsed_ticks'] == a['tick']
        assert outcome['elapsed_ticks'] == 0 and r['milestones'] == dict.fromkeys(NAMES) | {'choice':a['tick']}
        assert r['tactical_completed_tick'] is None and r['touchdown'] is None
        return dict(outcome='unsupported_source', prediction=None, phases={}, milestones=r['milestones'],
                    unknown=source['choice_unknown'] or 'non-neutral source', controller_rows=1)
    Q.audit_choice(case, rows[0], probe['landing_choice'], allow_delayed_start=True)
    assert source['selected'] == probe['landing_choice']['report']['selected']
    assert source['choice_unknown'] is None
    prediction = cost_reference(source, rows[0])
    assert source['pilot']['planet']['index'] == case['destination']
    assert source['pilot']['location'] != 'on_foot'
    assert r['last_observed_tick'] == rows[-1]['tick'] and r['observed_rows'] == len(rows)
    milestones, tactical, touchdown, reason = reconstruct(source, rows)
    if reason is None:
        expected = 'match_finished' if report['round']['outcome'] is not None else 'runner_ended'
        assert not outcome['controller_observed'] and outcome['reason'] == expected
        assert outcome['tick'] == rows[-1]['tick']+1
    else:
        assert outcome['controller_observed'] and outcome['reason'] == reason and outcome['tick'] == rows[-1]['tick']
    assert report['elapsed_ticks'] == outcome['tick']
    assert outcome['elapsed_ticks'] == outcome['tick']-a['tick']
    assert milestones == r['milestones'] and tactical == r['tactical_completed_tick']
    if touchdown is None:
        assert r['touchdown'] is None
    else:
        p = P.pilot(touchdown)
        origin = source['pilot']
        planet = next(v for v in touchdown['observation']['planets'] if v['index'] == case['destination'])
        site = next(s for s in origin['sites'] if s['id'] == source['selected']['site'])
        expected = Q.rotate(Q.sub(Q.vec(site['vehicle_position']), Q.vec(origin['planet']['motion']['position'])), -origin['planet']['motion']['angle'])
        actual = Q.rotate(Q.sub(Q.vec(p['ship']['position']), Q.vec(planet['motion']['position'])), -planet['motion']['angle'])
        recorded = r['touchdown']
        assert recorded['tick'] == touchdown['tick'] and recorded['ship'] == p['ship'] and recorded['landing'] == p['landing']
        for key, value in [('expected_local', expected), ('actual_local', actual)]:
            for v, w in zip(Q.vec(recorded[key]), value): Q.close(v, w, .002)
        Q.close(recorded['offset'], math.dist(actual, expected), .003)
    return dict(outcome=outcome['reason'], elapsed_seconds=outcome['elapsed_ticks']/60,
        prediction=prediction, unknown=source['reference_unknown'] or (source['reference'] or {}).get('unknown'),
        milestones=milestones, phases=phases(milestones, prediction, outcome),
        touchdown_offset=None if touchdown is None else r['touchdown']['offset'], controller_rows=len(rows))



def read_control(case, root, report):
    outcome = report['transfer_probe']['capture_followthrough']['outcome']
    if outcome is None: outcome = report['transfer_probe']['acquisition']['outcome'] or report['transfer_probe']['outcome']
    observed = outcome.get('controller_observed', outcome['reason'] not in ['match_finished','runner_ended'])
    expected = 2*(report['elapsed_ticks']+int(observed))
    control, count, size, digest = [], 0, 0, hashlib.sha256()
    with (root/'trace.jsonl').open('rb') as stream:
        for line in stream:
            row = json.loads(line)
            assert (row['tick'],row['seat']) == (count//2,count%2)
            assert P.pilot(row)['tick'] == row['tick']
            if row['seat'] == case['seat'] and row['tick'] >= case['source_tick']: control.append(row)
            count += 1
            size += len(line)
            digest.update(line)
    assert count == expected
    sensors = 0
    for row in T.rows(root/'sensors.jsonl'):
        assert (row['tick'],row['seat']) == (sensors//2+1,sensors%2)
        sensors += 1
    assert sensors == count
    return control, dict(controller_rows=count,decompressed_bytes=size,trace_sha256=digest.hexdigest(),sensor_rows=sensors)


def aggregate(runs):
    audits = [v['audit'] for v in runs.values()]
    return dict(cases=len(audits), outcomes=dict(Counter(a['outcome'] for a in audits)),
        numeric_references=sum(a['prediction'] is not None for a in audits),
        unknown_references=dict(Counter(a.get('unknown') for a in audits if a['prediction'] is None)),
        milestones={k:sum(a.get('milestones', {}).get(k) is not None for a in audits) for k in NAMES},
        physical_ticks=sum(v['elapsed_ticks'] for v in runs.values()),
        prefix_rows=sum(v['parity']['controller_rows'] for v in runs.values()),
        phases={k:dict(completed=sum(a['phases'].get(k, {}).get('status') == 'completed' for a in audits),
            paired_errors=[a['phases'][k]['error_seconds'] for a in audits if a['phases'].get(k, {}).get('error_seconds') is not None]) for k in PHASES})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True, help='previous first-choice comparison corpus')
    parser.add_argument('--source-reference', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    previous = json.loads((opts.reference/'summary.json').read_text())
    cases, denominator = P.plan(opts.source_reference)
    assert cases == previous['plan'] and denominator == previous['denominator']
    opts.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=cases, denominator=denominator, horizon_seconds=HORIZON_SECONDS, runs={},
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()),
        binary_sha256=F.digest(opts.binary), reference_summary_sha256=F.digest(opts.reference/'summary.json'),
        source_summary_sha256=F.digest(opts.source_reference/'summary.json'),
        scope='All20 fixed neutral source-numeric alternatives, same nominations and transfer pursuit policy. Ordinary controls after handoff.120s from first native choice, first attempt only. Frozen existing empirical medians for actual chosen site when supported. All unknowns, interruptions and censors retained. Prior parity ends at first choice; suffix is new physics. No tuning, cost admission, live policy change, deployment, strength or Pi performance claim.')
    assert not result['source_dirty'], 'freeze runtime, runner and plan before physics'
    save = lambda: F.D.write(opts.out/'summary.json', result)
    save()
    for case in cases:
        name = case['name']+'-on'
        before, root = opts.reference/name, opts.out/name
        for file, digest in previous['runs'][case['name']]['files'].items(): assert F.digest(before/file) == digest
        bearing = case['candidate']['local_reference']['evidence']['site']['bearing']
        args = T.probe_args(case)+['--probe-transfer-pursuit', 'defer_new', '--bounded-acquisition-seats', 'none',
            '--probe-acquisition-seconds', str(P.HORIZON_SECONDS), '--probe-landing-reference-bearing', str(bearing),
            '--probe-capture-seconds', str(HORIZON_SECONDS)]
        run = F.D.run(opts.binary, opts.out, name, args, case['seat'], seconds=case['source_tick']//60+211)
        report = json.loads((root/'report.json').read_text())
        run['parity'] = prefix_parity(before, root)
        run['budget'] = audit_budget(root, report)
        run['source'] = P.audit_source(case, opts.source_reference, root, report['transfer_probe'])
        control, run['trace'] = read_control(case, root, report)
        run['audit'] = audit_capture(case, report, control)
        run['capture'] = report['transfer_probe']['capture_followthrough']
        run['trace_sha256'] = C.archive(root)
        assert run['trace_sha256'] == run['trace']['trace_sha256']
        run['files'] = {p.name:F.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
        result['runs'][case['name']] = run
        save()
        print(case['name'], run['audit']['outcome'], run['audit'].get('milestones'), flush=True)
    assert F.digest(opts.binary) == result['binary_sha256']
    result['results'] = aggregate(result['runs'])
    save()


if __name__ == '__main__': main()
