#!/usr/bin/env python3
"""Execute the frozen integrated-bot matrix and audit every complete trajectory."""
import argparse
from collections import Counter, defaultdict
from concurrent.futures import ProcessPoolExecutor
import importlib.util
import itertools
import json
import math
from pathlib import Path
import subprocess
import traceback
import types


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


P = module('integrated_plan', 'plan-integrated-bot.py')
C = module('continuations', 'validate-flight-continuation.py')
T = module('retry', 'validate-destination-retry.py')
M, F, R = C.M, C.F, C.R
D = F.D
ROOT = Path(__file__).resolve().parents[1]


def write(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')


def verify_plan(plan, manifest):
    P.require(plan['purpose'] == 'plan' and not plan['results'], 'expected unrun frozen plan')
    P.require(plan['source_commit'] == manifest['frozen_commit'], 'wrong plan source')
    P.require([job['item'] for job in plan['cases']] == P.plan(), 'case matrix changed')
    for field in ('candidate', 'control', 'host'):
        P.require(plan[field] == manifest[field], f'{field} identity changed')
    P.require(plan['binary'] == manifest['binary'], 'binary identity changed')
    base = Path(plan['binary']['path']).parent
    for job in plan['cases']:
        item = job['item']
        argv = [part for pair in P.flags(item).items() for part in pair]
        expected = [plan['binary']['path'], *argv, '--out', str(base / item['name'])]
        P.require(job['command'] == expected, f"{item['name']}: command changed")
        identities = [f"retained_v{item['opponent']}"] * 2
        identities[item['seat']] = plan[item['arm']]
        P.require(job['configuration_by_seat'] == identities and job['host'] == P.HOST,
                  'per-seat or host identity changed')


def audit_dense(rows, report, item):
    """Reuse physical/flight auditors in one pass through the large capture stream."""
    visits = {s: [dict(planet=v['planet'], selected_tick=v['selected_tick'], recorded=v,
                      physical=dict.fromkeys(('landed', 'exited', 'claimed', 'boarded', 'departed')))
                  for v in report['metrics'][s]['visits']] for s in (0, 1)}
    enabled = [item['arm'] == 'candidate' and s == item['seat'] for s in (0, 1)]
    counts, ownership, invalidations = Counter(), Counter(), Counter()
    first, last, previous_goal, forecasts, ground_surveys = {}, {}, {}, {}, {}
    indices, max_age = {0: 0, 1: 0}, 0
    launched, completed = set(), set()
    launches, resumptions, witnesses = [], [], []
    powered_publications, walking_receipts = {}, {}
    publications = Counter()
    lowest = 1.0

    def checked_rows():
        nonlocal max_age, lowest
        for row in rows:
            seat, pilot = row['seat'], row['pilot']
            tick = pilot['tick']
            assert seat in (0, 1) and row['schema'] == 1
            assert pilot['owner'] == f'player_{seat+1}'
            assert row['mission']['policy'] == report['policy_configuration'][seat]['policy']
            assert row['mission']['powered_capture'] == enabled[seat]
            if seat in last:
                assert tick == last[seat] + 1, 'capture trace is not dense'
            first.setdefault(seat, tick)
            last[seat] = tick
            counts[seat] += 1
            ownership[seat] += sum(p is not None and p['claim'] is not None
                                   and p['claim']['owner'] == pilot['owner'] for p in row['planets'])
            policy = report['policy_configuration'][seat]['policy']
            planning = ('jetpack_round_trip' if enabled[seat] else
                        'legacy' if policy == 'material_mission_v9' else 'joint_round_trip')
            publication = M.audit_publication(row, planning)
            if publication:
                max_age = max(max_age, publication['age'])
                publications[seat] += 1
            evidence = row.get('objective_evidence')
            if evidence:
                if evidence['invalidated_by']:
                    invalidations[evidence['invalidated_by']] += 1
                if evidence.get('exhausted_walk') or evidence.get('unsupported_walk'):
                    receipt = R.B.audit_methods(iter([row]))['first_receipts'][0]
                    key = (seat, receipt['generation'])
                    # Repeated receipts must retain the same generation and age source.
                    old = walking_receipts.setdefault(key, receipt)
                    assert all(old[k] == receipt[k] for k in
                               ('kind', 'measurement_tick', 'site', 'bounds') if k in receipt)
            if R.audit_powered(iter([row])):
                survey = row['landing_objective']
                for route in survey['sites'] + ([survey['actual']] if survey.get('actual') else []):
                    if route.get('crossing'):
                        key = (seat, evidence['generation'], json.dumps(route['site'], sort_keys=True))
                        powered_publications.setdefault(key, dict(seat=seat, tick=tick, site=route['site'],
                            generation=evidence['generation'], measurement_tick=evidence['measurement_tick']))
            capture = row.get('capture')
            assert not (capture or {}).get('cover_response'), 'disabled cover response activated'
            ground = (capture or {}).get('ground')
            pack = row.get('jetpack')
            if pack:
                assert math.isfinite(pack['charge']) and 0 <= pack['charge'] <= 1
                lowest = min(lowest, pack['charge'])
                if pack.get('vehicle_forecast'):
                    forecasts[seat] = pack['vehicle_forecast']
                if pack['surveyed']:
                    ground_surveys[seat] = dict(tick=tick, equipment=pack)
            goal = ground['goal'] if ground else None
            if goal == 'jetpack_lift' and previous_goal.get(seat) != goal:
                launch = dict(seat=seat, tick=tick, equipment=pack,
                              latest_forecast=forecasts.get(seat), ground=ground)
                takeoff = M.first_launch(seat, ground, launched)
                launch['kind'] = (M.audit_launch if takeoff else M.audit_corridor)(
                    launch, ground_surveys.get(seat))
                if launch['kind'] != 'forecast_vehicle':
                    launch['source_survey'] = ground_surveys[seat]
                (launches if takeoff else resumptions).append(launch)
                witnesses.append(row)
            previous_goal[seat] = goal
            if ground and ground.get('crossing') and ground['crossing']['completed_tick'] is not None:
                key = (seat, ground['crossing']['completed_tick'])
                if key not in completed:
                    completed.add(key)
                    witnesses.append(row)
            index = indices[seat]
            while index+1 < len(visits[seat]) and visits[seat][index+1]['selected_tick'] <= tick:
                index += 1
            indices[seat] = index
            if visits[seat]:
                visit = visits[seat][index]
                end = visit['recorded']['departed_tick'] or visit['recorded']['abandoned_tick']
                if visit['selected_tick'] <= tick and (end is None or tick <= end):
                    before = dict(visit['physical'])
                    M.observe_visit(visit, row)
                    if before != visit['physical']:
                        witnesses.append(row)
            if item['world'] == 'value-destination':
                assert row['actions'][2]['Scenario']['payload'][1:] == [0, 0]
            yield row

    continuation = C.audit_continuations(checked_rows())
    expected = {item['seat']} if item['world'] == 'value-destination' else {0, 1}
    assert set(counts) == expected
    for seat in expected:
        assert counts[seat] == report['elapsed_ticks']
        assert first[seat] == report['final_pilots'][seat]['tick'] - report['elapsed_ticks']
        assert last[seat]+1 == report['final_pilots'][seat]['tick']
        for visit in visits[seat]:
            if visit['recorded']['departed_tick'] is not None:
                assert visit['physical']['departed'] == visit['recorded']['departed_tick']
    if item['arm'] == 'control':
        assert continuation['flights'] == 0
    return dict(visits=visits, rows=dict(counts), first_ticks=first, last_ticks=last,
                owned_planet_ticks=dict(ownership), publications=dict(publications),
                invalidations=dict(invalidations), max_publication_age=max_age,
                launches=launches, resumed_flights=resumptions, completed_crossings=sorted(completed),
                lowest_pack_charge=lowest, powered_publications=list(powered_publications.values()),
                walking_receipts=list(walking_receipts.values()), continuation=continuation,
                physical_witnesses=witnesses)


def analyze(root, job, out):
    item = job['item']
    report = json.loads((root / 'report.json').read_text())
    P.check_configuration(report, item)
    assert report['seconds'] == item['seconds'] and report['world'] == item['world']
    assert report['round']['time_limit_seconds'] == 600
    visit_audit = F.M.audit(report)
    assert not any(visit_audit['counts'].get(k, 0) for k in
                   ('unverified', 'corrected_visits', 'milestones_outside_visit'))
    for progress in report['mission_progress']['players']:
        assert progress['eligible_ticks'] == progress['progress_ticks'] + progress['ticks_without_progress']
        assert 0 <= progress['ticks_after_20s_without_progress'] <= progress['ticks_without_progress']
        assert progress['distance_observed_ticks'] <= progress['eligible_ticks']
    dense = audit_dense(F.rows(root / 'capture-evidence.jsonl'), report, item)
    audit = dict(physical=dense, visit_endings=visit_audit,
                 allocation=F.allocation_audit(root),
                 predictions=F.first_predictions(report, root / 'mission-evaluations.jsonl'),
                 evaluation_coverage=D.evaluation_coverage(root / 'mission-evaluations.jsonl'),
                 switches=F.P.switch_predictions(report, F.rows(root / 'mission-evaluations.jsonl')),
                 retry=T.retry_stats(report, [item['seat']] if item['arm'] == 'candidate' else []))
    path = out / (item['name'] + '-audit.json')
    write(path, audit)
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


def raw_hashes(root):
    return {p.name: P.digest(p) for p in sorted(root.iterdir()) if p.is_file()}


def run_job(job, out, previous):
    name = job['item']['name']
    root = Path(job['command'][-1])
    # Keep decimal bearings in case names; do not replace the apparent suffix.
    log = root.parent / (root.name + '.log')
    result = dict(item=job['item'], command=job['command'])
    try:
        if previous is not None:
            assert previous['command'] == job['command']
            assert raw_hashes(root) == previous['hashes'], 'prior raw evidence changed'
            assert P.digest(log) == previous['log_sha256']
            result['reused_raw'] = True
        else:
            P.require(not root.exists(), 'refusing to overwrite a planned case')
            with log.open('x') as stream:
                subprocess.run(job['command'], check=True, stdout=stream, stderr=stream, timeout=1800)
        result['hashes'] = raw_hashes(root)
        result['log_sha256'] = P.digest(log)
        result.update(analyze(root, job, out))
        result['audited'] = True
    except Exception:
        result['error'] = traceback.format_exc()
        if previous is not None:
            result['hashes'] = previous['hashes']
            result['log_sha256'] = previous['log_sha256']
        elif root.exists():
            result['hashes'] = raw_hashes(root)
        if previous is None and log.exists():
            result['log_sha256'] = P.digest(log)
    print(name + (': audited' if result.get('audited') else ': FAILED'), flush=True)
    return result


def first_control_difference(a, b):
    matched = 0
    for x, y in itertools.zip_longest(F.rows(a), F.rows(b)):
        if x is None or y is None:
            return dict(tick=(x or y)['tick'], reason='trace_length', matched_rows=matched)
        assert (x['tick'], x['seat']) == (y['tick'], y['seat'])
        if x['actions'] != y['actions']:
            return dict(tick=x['tick'], seat=x['seat'], reason='actions', matched_rows=matched,
                        control=x['actions'], candidate=y['actions'])
        matched += 1
    return None


def completion_changes(a, b, first_tick):
    """Compare executed same-planet completion ordinals, never unchosen forecasts."""
    def by_planet(player):
        result = defaultdict(list)
        for v in player['visits']:
            if v['departed_tick'] is not None:
                result[v['planet']].append(v)
        return result
    control, candidate = by_planet(a), by_planet(b)
    useful, missing = [], []
    for planet in sorted(control.keys() | candidate.keys()):
        for index, (old, new) in enumerate(itertools.zip_longest(control[planet], candidate[planet])):
            if old is not None and new is None:
                missing.append(dict(planet=planet, completion_ordinal=index, control=old))
            if (new is not None and first_tick is not None and new['departed_tick'] >= first_tick
                    and (old is None or new['departed_tick'] < old['departed_tick'])):
                useful.append(dict(planet=planet, completion_ordinal=index, control=old, candidate=new,
                                   kind='additional' if old is None else 'earlier'))
    return useful, missing


def compare(pair, runs):
    jobs = {j['item']['arm']: j for j in pair}
    before, after = [jobs[arm] for arm in ('control', 'candidate')]
    a, b = [runs[j['item']['name']] for j in (before, after)]
    roots = [Path(j['command'][-1]) for j in (before, after)]
    reports = [json.loads((root / 'report.json').read_text()) for root in roots]
    assert reports[0]['initial_world'] == reports[1]['initial_world'], 'paired initial state changed'
    difference = first_control_difference(*(root / 'destination-behavior.jsonl' for root in roots))
    physical_fields = ('round', 'final_pilots', 'final_planets', 'final_audit', 'final_combat',
                       'asteroid_events', 'elapsed_ticks')
    same = all(reports[0][k] == reports[1][k] for k in physical_fields)
    if difference is None:
        assert same, 'physical outcomes changed without an action difference'
    elif difference['reason'] == 'trace_length':
        raise ValueError('match ending differs with no preceding changed action')
    seat = before['item']['seat']
    useful, missing = completion_changes(a['players'][seat], b['players'][seat],
                                        difference['tick'] if difference else None)
    return dict(group=before['item']['group'], stage=before['item']['stage'], seat=seat,
                opponent=before['item']['opponent'], interval=before['item']['interval'],
                seed=before['item']['seed'], world=before['item']['world'],
                control=before['item']['name'], candidate=after['item']['name'],
                same_physical_outcomes=same, first_control_difference=difference,
                useful_completed_changes=useful, missing_control_completions=missing)


def totals(players):
    counts = Counter()
    for player in players:
        counts[player['outcome']] += 1
        for key in ('completed_departures', 'claims', 'completed_recoveries', 'ships_lost',
                    'pilot_deaths', 'abandoned_visits', 'unfinished_visits', 'owned_planet_ticks'):
            counts[key] += player[key]
        for key in ('eligible_ticks', 'distance_observed_ticks', 'ticks_after_20s_without_progress'):
            counts[key] += player['progress'][key]
        counts['worst_no_progress_ticks'] = max(counts['worst_no_progress_ticks'],
                                                player['progress']['longest_no_progress_ticks'])
    result = dict(counts)
    result['points'] = counts['win'] + 0.5*counts['draw']
    result['no_progress_fraction'] = (counts['ticks_after_20s_without_progress']/counts['eligible_ticks']
                                      if counts['eligible_ticks'] else None)
    return result


def decision(runs, comparisons):
    fresh = [p for p in comparisons if p['stage'] == 'held_out']
    assert len(fresh) == 48 and len(comparisons) == 56
    def aggregate(pairs):
        return {arm: totals([runs[p[arm]]['players'][p['seat']] for p in pairs])
                for arm in ('control', 'candidate')}
    aggregate_all = aggregate(fresh)
    strata = {}
    for key in ('opponent', 'seat', 'interval', 'seed'):
        for value in sorted({p[key] for p in fresh}):
            strata[f'{key}:{value}'] = aggregate([p for p in fresh if p[key] == value])
    reasons = []
    if not any(p['useful_completed_changes'] for p in fresh):
        reasons.append('no_useful_completed_change_in_fresh_pairs')
    for name, arms in strata.items():
        if not name.startswith('seed:') and arms['candidate']['points'] < arms['control']['points']:
            reasons.append('match_points_regressed:' + name)
    a, b = [aggregate_all[arm] for arm in ('control', 'candidate')]
    if b['completed_departures'] < a['completed_departures']:
        reasons.append('fewer_completed_departures')
    for key in ('ships_lost', 'pilot_deaths', 'worst_no_progress_ticks'):
        if b[key] > a[key]:
            reasons.append(key + '_increased')
    if a['no_progress_fraction'] is None or b['no_progress_fraction'] is None:
        reasons.append('unknown_no_progress_fraction')
    # Compare integer products to avoid float rounding at the acceptance boundary.
    elif (b['ticks_after_20s_without_progress'] * a['eligible_ticks']
          > a['ticks_after_20s_without_progress'] * b['eligible_ticks']):
        reasons.append('no_progress_fraction_increased')
    for pair in comparisons:
        if pair['world'] == 'value-destination' and pair['missing_control_completions']:
            reasons.append('directed_completion_regressed:' + pair['group'])
    review = {}
    for pair in fresh:
        player = runs[pair['candidate']]['players'][pair['seat']]
        if pair['first_control_difference'] and player['outcome'] == 'loss':
            review.setdefault(f"first_changed_loss_v{pair['opponent']}", pair['candidate'])
    longest = max(fresh, key=lambda p: runs[p['candidate']]['players'][p['seat']]
                  ['progress']['longest_no_progress_ticks'])
    review['longest_candidate_no_progress'] = longest['candidate']
    return dict(decision='retain' if reasons else 'advance_to_further_evaluation', reasons=reasons,
                fresh=aggregate_all, strata=strata,
                qualification=aggregate([p for p in comparisons if p['stage'] == 'qualification']),
                changed_fresh_pairs=sum(p['first_control_difference'] is not None for p in fresh),
                useful_changed_fresh_pairs=sum(bool(p['useful_completed_changes']) for p in fresh),
                combat_review_cases=review, default_promotion=False)


def tool_inputs():
    found, pending = {}, [P, C, T]
    while pending:
        mod = pending.pop()
        path = Path(mod.__file__).resolve()
        if path in found or path.parent != ROOT / 'tools':
            continue
        found[path] = P.digest(path)
        pending += [v for v in vars(mod).values() if isinstance(v, types.ModuleType)
                    and getattr(v, '__file__', None)]
    found[Path(__file__).resolve()] = P.digest(Path(__file__))
    return {str(p.relative_to(ROOT)): digest for p, digest in sorted(found.items())}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--previous', type=Path, help='re-audit hash-bound completed cases before continuing')
    args = parser.parse_args()
    P.require(__debug__, 'existing auditors require Python assertions enabled')
    P.require(not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip(),
              'freeze runner, tests and audit interpretation first')
    manifest = json.loads((ROOT / 'docs/data/integrated-bot-candidate-v1.json').read_text())
    P.require(P.digest(args.plan) == manifest['plan']['sha256'], 'plan hash changed')
    plan = json.loads(args.plan.read_text())
    verify_plan(plan, manifest)
    binary = Path(plan['binary']['path'])
    P.require(P.digest(binary) == plan['binary']['sha256'], 'frozen binary changed')
    prior = json.loads(args.previous.read_text()) if args.previous else None
    if prior:
        P.require(prior['plan_sha256'] == P.digest(args.plan), 'previous run used another plan')
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, source_commit=plan['source_commit'],
        runner_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        plan_path=str(args.plan), plan_sha256=P.digest(args.plan), binary=plan['binary'],
        inputs=tool_inputs(), runs={}, comparisons=[])
    if prior:
        result['previous_summary'] = dict(path=str(args.previous), sha256=P.digest(args.previous))
    save = lambda: write(args.out / 'summary.json', result)
    save()
    try:
        with ProcessPoolExecutor(max_workers=2) as pool:
            for i in range(0, len(plan['cases']), 2):
                pair = plan['cases'][i:i+2]
                futures = [pool.submit(run_job, job, args.out,
                           (prior or {}).get('runs', {}).get(job['item']['name'])) for job in pair]
                # Drain this pair on failure; no further cases are submitted.
                for job, future in zip(pair, futures):
                    result['runs'][job['item']['name']] = future.result()
                    save()
                P.require(all(result['runs'][job['item']['name']].get('audited') for job in pair),
                          'case audit failed; the current pair is retained and execution stopped')
                result['comparisons'].append(compare(pair, result['runs']))
                save()
                print(f"Audited {i+2}/{len(plan['cases'])} cases", flush=True)
        result['screen'] = decision(result['runs'], result['comparisons'])
        P.require(P.digest(args.plan) == result['plan_sha256'], 'plan changed during execution')
        P.require(P.digest(binary) == result['binary']['sha256'], 'binary changed during execution')
        P.require(tool_inputs() == result['inputs'], 'auditors changed during execution')
        result['complete'] = True
    except BaseException:
        result['error'] = traceback.format_exc()
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
