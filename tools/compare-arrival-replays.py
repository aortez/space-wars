#!/usr/bin/env python3
"""Join four frozen distant arrival screens to their existing physical replays."""
import argparse
from collections import Counter
import copy
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import subprocess


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


H = module('followthrough', 'probe-capture-followthrough.py')
R = module('retained_arrivals', 'retain-remote-surveys.py')
Q, P, C, T, F, A = H.Q, H.P, H.C, H.T, H.F, R.A
MAX_AGE = 1800
GEOMETRY_TOLERANCE = .002
SIDES = [-1, 1]
CLEARANCES = ['approach_clearance', 'parked_clearance', 'departure_clearance']


def plan(retained, physical):
    """Selection is the prior missing-geometry gap, never a physical outcome."""
    result = []
    for name, pair in retained['pairs'].items():
        for source in pair['sources']:
            active = source['active_source']
            for screen in source['screens']:
                if not screen['sites'] or (active and any(c['id']['planet'] == screen['destination'] for c in active['candidates'])):
                    continue
                matching = [c for c in physical['plan'] if c['reference_run'] == name
                    and c['seat'] == source['seat'] and c['source_tick'] == source['source_tick']
                    and c['destination'] == screen['destination']]
                assert len(matching) == 1
                case = matching[0]
                result.append(dict(case=case, retained_run=name, screen=screen,
                    command=physical['runs'][case['name']]['command']))
    assert [(v['case']['seat'],v['case']['source_tick'],v['case']['destination']) for v in result] == [
        (0,t,2) for t in [3816,3876,3934,3997]]
    assert all(v['case']['condition']['seed'] == 3491156488288037499 and v['case']['condition']['interval'] == 0 for v in result)
    return result


def json_vec(v): return dict(zip(['x','y'], v))
def distance(a,b): return math.dist(Q.vec(a),Q.vec(b))
def angle_difference(a,b): return math.atan2(math.sin(b-a),math.cos(b-a))


def project(measurement, motion):
    """Retrospective rigid transport only; never edits the frozen prediction."""
    site = copy.deepcopy(measurement['site'])
    origin = measurement['planet']
    angle = Q.f32(Q.f32(motion['angle'])-Q.f32(origin['angle']))
    def point(v):
        return json_vec(Q.add(Q.vec(motion['position']),Q.rotate(Q.sub(Q.vec(v),Q.vec(origin['position'])),angle)))
    for key in ['position','vehicle_position','hatch_position']: site[key] = point(site[key])
    site['normal'] = json_vec(Q.rotate(Q.vec(site['normal']),angle))
    site['boarding_hatches'] = [point(v) if v is not None else None for v in site['boarding_hatches']]
    dx,dy = Q.sub(Q.vec(site['vehicle_position']),Q.vec(motion['position']))
    site['velocity'] = json_vec(Q.add(Q.vec(motion['velocity']),Q.mul((-dy,dx),Q.f32(motion['spin']))))
    return site


def geometry_residual(projected, native):
    if native is None: return None
    assert projected['id'] == native['id']
    fields = {key:distance(projected[key],native[key]) for key in
              ['position','vehicle_position','hatch_position','local_position','normal','velocity']}
    patterns = [v is not None for v in projected['boarding_hatches']] == [v is not None for v in native['boarding_hatches']]
    hatches = [distance(a,b) if a is not None and b is not None else None
               for a,b in zip(projected['boarding_hatches'],native['boarding_hatches'])]
    values = list(fields.values())+[v for v in hatches if v is not None]
    margin = projected['hatch_has_settling_margin'] == native['hatch_has_settling_margin']
    return dict(fields=fields,boarding_hatches=hatches,hatch_availability_matches=patterns,
                settling_margin_matches=margin,revision_matches=projected['revision'] == native['revision'],
                maximum=max(values),within_reconstruction_tolerance=bool(patterns and margin
                    and projected['revision'] == native['revision'] and max(values) <= GEOMETRY_TOLERANCE))


def identity_changes(control, destination):
    """Keep the first actual change even when the final state returns to source."""
    source = control[0]
    p = P.pilot(source)
    original = R.neutral_identity(next(v for v in source['observation']['planets'] if v['index'] == destination))
    assert original is not None
    first_planet, first_actor = None,None
    for row in control:
        current = P.pilot(row)
        planet = next((v for v in row['observation']['planets'] if v['index'] == destination),None)
        if first_planet is None and (planet is None or T.f32_identity(R.neutral_identity(planet)) != T.f32_identity(original)):
            first_planet = row['tick']
        match_context = row['observation'].get('match_context')
        actor_changed = any(current[k] != p[k] for k in ['owner','vehicle','spaceling'])
        actor_changed |= not current['ship_available'] or current['ship_form'] != 'ship' or current['ship_health'] <= 0
        actor_changed |= bool(match_context and (match_context['finished'] or not match_context['pilots_alive'][source['seat']]))
        if first_actor is None and actor_changed: first_actor = row['tick']
    return dict(first_planet_change=first_planet,first_actor_change=first_actor)


def age_and_identity_valid(measurement_tick, tick, changes):
    return (measurement_tick <= tick <= measurement_tick+MAX_AGE
        and all(t is None or t > tick for t in changes.values()))


def motion_difference(predicted, actual):
    return dict(position=distance(predicted['position'],actual['position']),
        velocity=distance(predicted['velocity'],actual['velocity']),
        heading_actual_minus_predicted=angle_difference(predicted['angle'],actual['angle']),
        spin_actual_minus_predicted=actual['spin']-predicted['spin'])


def body_relative(ship, planet):
    offset = Q.sub(Q.vec(ship['position']),Q.vec(planet['position']))
    body_velocity = Q.add(Q.vec(planet['velocity']),Q.mul((-offset[1],offset[0]),Q.f32(planet['spin'])))
    return dict(position=json_vec(Q.rotate(offset,-planet['angle'])),
                velocity=json_vec(Q.rotate(Q.sub(Q.vec(ship['velocity']),body_velocity),-planet['angle'])),
                angle=angle_difference(planet['angle'],ship['angle']),spin=ship['spin']-planet['spin'])


def solar_sign(plan):
    if plan is None: return 'no_sun'
    values = [plan[k] for k in CLEARANCES]
    assert all(math.isfinite(v) for v in values)
    if any(abs(v) <= Q.CLEARANCE_TOLERANCE for v in values): return 'unresolved'
    return 'clear' if all(v > 0 for v in values) else 'unsafe'


def solar_difference(frozen, actual):
    if frozen is None or actual is None: return None
    return {k:actual[k]-frozen[k] for k in CLEARANCES+['arrival_seconds']}


def observed_frame(row, destination):
    p = P.pilot(row)
    assert p['planet']['index'] == destination
    return dict(tick=row['tick'],ship=p['ship'],planet=p['planet'],sun=row['observation']['local']['sun'],
                planet_orbit_omega=row['observation']['local']['planet_orbit_omega'])


def compare_screen(case, screen, control, handoff_tick, choice_tick, native):
    """Forecast data and later observed data stay separate in the returned record."""
    destination,source_tick = case['destination'],case['source_tick']
    assert control and control[0]['tick'] == source_tick
    assert P.pilot(control[0])['owner'] == f"player_{case['seat']+1}"
    assert all(r['seat'] == case['seat'] and P.pilot(r)['tick'] == r['tick'] for r in control)
    assert [r['tick'] for r in control] == list(range(source_tick,control[-1]['tick']+1))
    assert screen['source_tick'] == source_tick and screen['destination'] == destination
    assert screen['complete'] and screen['unknown'] is None
    predicted = screen['arrival']
    assert predicted and predicted['tick'] >= source_tick and predicted['planet']['index'] == destination
    rows = {r['tick']:r for r in control}
    assert source_tick <= handoff_tick <= choice_tick and handoff_tick in rows and choice_tick in rows
    arrival, choice = rows[handoff_tick],rows[choice_tick]
    assert P.first_choice(choice,handoff_tick,destination) is not None
    assert native['observation_tick'] == choice_tick and native['report']['tick'] == choice_tick
    assert native['report']['selected']['site'] == choice['mission']['capture']['site']
    actual_frame, choice_frame = [observed_frame(r,destination) for r in [arrival,choice]]
    changes = identity_changes([r for r in control if r['tick'] <= choice_tick],destination)
    at_predicted = rows.get(predicted['tick'])
    same_tick = None
    if at_predicted:
        planet = next(p for p in at_predicted['observation']['planets'] if p['index'] == destination)
        same_tick = dict(tick=predicted['tick'],actual_planet=planet,
            motion_difference=motion_difference(predicted['planet']['motion'],planet['motion']),
            endpoint_identity_matches=T.f32_identity(R.neutral_identity(predicted['planet'])) == T.f32_identity(R.neutral_identity(planet)))
    selected = native['report']['selected']
    native_ids = [s['id'] for s in P.pilot(choice)['sites']]
    result = dict(source_tick=source_tick,destination=destination,original_generation=screen['generation'],
        predicted_handoff=predicted,actual_handoff=actual_frame,actual_choice=choice_frame,
        actual_minus_predicted_transfer_ticks=handoff_tick-predicted['tick'],
        actual_acquisition_ticks=choice_tick-handoff_tick,identity=changes,
        different_time_endpoint_differences=dict(predicted_tick=predicted['tick'],actual_tick=handoff_tick,
            ship_world=motion_difference(predicted['ship'],actual_frame['ship']),
            planet=motion_difference(predicted['planet']['motion'],actual_frame['planet']['motion']),
            ship_body_relative=motion_difference(body_relative(predicted['ship'],predicted['planet']['motion']),
                body_relative(actual_frame['ship'],actual_frame['planet']['motion']))),
        same_predicted_tick_planet=same_tick,
        native_selected=selected,native_choice_outside_retained_set=selected['site'] not in [s['source']['id'] for s in screen['sites']],
        sites=[])
    for historical in screen['sites']:
        assert historical['unknown'] is None and historical['projected'] is not None
        assert [d['side'] for d in historical['directions']] == SIDES
        candidate = historical['source']
        measurement = candidate['measurement']
        measured_tick = measurement['tick']
        assert candidate['id'] == measurement['site']['id'] and candidate['id']['planet'] == destination
        assert screen['generation'] <= measured_tick <= source_tick
        assert historical['arrival_age_ticks'] == predicted['tick']-measured_tick <= MAX_AGE
        assert measurement['finding'] == 'measured' and measurement['climb_clear'] is True
        assert geometry_residual(project(measurement,predicted['planet']['motion']),historical['projected'])['within_reconstruction_tolerance']
        entry = dict(id=candidate['id'],measurement_tick=measured_tick,source_age_ticks=source_tick-measured_tick,
                     predicted_age_ticks=historical['arrival_age_ticks'],source_record=candidate,
                     frozen_prediction=historical,retrospective={})
        for label,row,frame in [('handoff',arrival,actual_frame),('choice',choice,choice_frame)]:
            projected = project(measurement,frame['planet']['motion'])
            now = row['tick']
            entry['retrospective'][label] = dict(purpose='analysis using later actual frame; not a source prediction',
                tick=now,age_ticks=now-measured_tick,age_and_identity_valid=age_and_identity_valid(measured_tick,now,changes),
                projected=projected,solar_context_unchanged=predicted['sun'] == frame['sun']
                    and predicted['planet_orbit_omega'] == frame['planet_orbit_omega'],
                directions=[dict(side=d,solar=Q.solar_plan(row['observation']['local'],projected,d)) for d in SIDES])
        fresh = next((s for s in P.pilot(choice)['sites'] if s['id'] == candidate['id']),None)
        assessments = [a for a in native['report']['assessments'] if a['site'] == candidate['id']]
        assert (candidate['id'] in native_ids) == (fresh is not None)
        assert len(assessments) == (2 if fresh else 0)
        assert {a['side'] for a in assessments} == (set(SIDES) if fresh else set())
        retrospective = entry['retrospective']['choice']
        entry['fresh_native_site'] = fresh
        entry['fresh_native_assessments'] = assessments
        entry['same_frame_geometry_residual'] = geometry_residual(retrospective['projected'],fresh)
        entry['native_site_status'] = 'present' if fresh else 'absent_from_fresh_survey'
        entry['direction_comparisons'] = []
        for frozen,reprojected in zip(historical['directions'],retrospective['directions']):
            assessment = next((a for a in assessments if a['side'] == frozen['side']),None)
            native_solar = assessment['solar'] if assessment else None
            entry['direction_comparisons'].append(dict(side=frozen['side'],
                frozen_sign=solar_sign(frozen['solar']),retrospective_sign=solar_sign(reprojected['solar']),
                native_sign=solar_sign(native_solar) if assessment else 'not_observed',
                native_minus_frozen=solar_difference(frozen['solar'],native_solar),
                native_minus_retrospective=solar_difference(reprojected['solar'],native_solar)))
        result['sites'].append(entry)
    viable = [a for s in result['sites'] for a in s['fresh_native_assessments'] if a['rejection'] is None]
    best = min(viable,key=lambda a:a['total_score']) if viable else None
    result['native_best_retained'] = best
    result['retained_score_above_native_choice'] = best['total_score']-selected['total_score'] if best else None
    return result


def audit_sensors(root, expected):
    digest,counters,stages,count = hashlib.sha256(),Counter(),Counter(),0
    for row in T.rows(root/'sensors.jsonl'):
        assert (row['tick'],row['seat']) == (count//2+1,count%2)
        stable = Q.without_wall_times(row)
        digest.update((json.dumps(stable,sort_keys=True,separators=(',',':'))+'\n').encode())
        counters.update(stable['profile']['counters'])
        stages.update({k:v['calls'] for k,v in stable['profile']['stages'].items()})
        count += 1
    actual = dict(sensor_rows=count,sensor_work_sha256=digest.hexdigest(),sensor_counters=dict(counters),sensor_stage_calls=dict(stages))
    assert actual == {k:expected[k] for k in actual}, 'whole-run sensor evidence changed'
    return actual


def capture_projection(raw):
    """Recreate the historical compact record without weakening dense-trace checks."""
    result = copy.deepcopy(raw)
    source = result['source']
    if source is None: return result
    pilot,mission = source['pilot'],source['mission']
    source['selected_site'] = next(s for s in pilot['sites'] if s['id'] == source['selected']['site'])
    source['pilot'] = {k:pilot[k] for k in ['tick','owner','vehicle','spaceling','location','ship',
        'ship_form','ship_health','transfers','planet']}
    source['mission'] = {k:mission[k] for k in ['policy','goal','target','replans','completed_sorties','capture']}
    source['mission']['visit_tick'] = max(e['tick'] for e in mission['events']
        if e['kind'] == 'selected' and e['planet'] == mission['target'])
    return result


def audit_replay(spec, root, ordinary, physical, acquisition, retained_pair):
    case,screen = spec['case'],spec['screen']
    name = case['name']
    archived = physical['runs'][name]
    for file,digest in retained_pair['runs'][1]['hashes'].items(): assert F.digest(ordinary/file) == digest
    ordinary_report = json.loads((ordinary/'report.json').read_text())
    source = next(s for s in ordinary_report['transfer_comparison']['sources'] if s['seat'] == case['seat'])
    candidate = next(c for c in source['last_snapshot']['candidates'] if c['destination'] == case['destination'])
    assert candidate['remote_arrival'] == screen
    assert candidate['source_actions'] == case['candidate']['source_actions']
    original_rows = {(r['seat'],r['tick']):r for r in C.read_trace(ordinary)}
    history = T.rows(ordinary/'destination-cover.jsonl')
    R.audit_memory(source,original_rows,history,case['condition']['seed'])
    group = next(g for g in source['retained_source']['groups'] if g['identity']['planet'] == case['destination'])
    frozen_audit = A.audit_screen(screen,dict(source,remote_source=group['survey']),candidate,
                                  original_rows[case['seat'],case['source_tick']],history)
    report = json.loads((root/'report.json').read_text())
    assert report['physics_ok'] and not report['audit_failures']
    assert report['elapsed_ticks'] == archived['elapsed_ticks']
    control,trace = H.read_control(case,root,report)
    assert trace == archived['trace'] and trace['trace_sha256'] == archived['trace_sha256'], 'physical trajectory changed'
    sensors = audit_sensors(root,physical['independent_audit']['report']['traces'][name])
    for file in P.UPSTREAM+['transfer-probe.jsonl']:
        assert F.digest(root/file) == archived['files'][file], 'upstream/probe bytes changed: '+file
    budget = H.audit_budget(root,report)
    assert budget == archived['budget']
    source_audit = P.audit_source(dict(case,reference_run=ordinary.name),ordinary.parent,root,report['transfer_probe'])
    assert source_audit == archived['source']
    capture_audit = H.audit_capture(case,report,control)
    assert T.f32_identity(capture_audit) == T.f32_identity(archived['audit'])
    assert T.f32_identity(Q.without_wall_times(capture_projection(report['transfer_probe']['capture_followthrough']))) == T.f32_identity(Q.without_wall_times(archived['capture']))
    old_probe = acquisition['runs'][name]['off']['probe']
    augmented = dict(case,expected_reason=old_probe['diagnostic']['reason'],expected_diagnostic=old_probe['diagnostic'])
    transfer_audit = T.audit_probe(augmented,report['transfer_probe'],T.rows(root/'transfer-probe.jsonl'))
    assert T.f32_identity(transfer_audit) == T.f32_identity(old_probe)
    probe = report['transfer_probe']
    outcome = probe['acquisition']['outcome']
    comparison = None
    if probe['outcome']['reason'] == 'arrived' and outcome and outcome['reason'] == 'site_selected':
        # The existing acquisition audit expects a run ending at that boundary.
        bounded = copy.deepcopy(report)
        bounded['elapsed_ticks'] = outcome['tick']
        bounded_control = [r for r in control if r['tick'] <= outcome['tick']]
        acquisition_audit = P.audit_acquisition(case,bounded,bounded_control)
        assert T.f32_identity(acquisition_audit) == T.f32_identity(acquisition['runs'][name]['acquisition'])
        choice_row = bounded_control[-1]
        native_audit = Q.audit_choice(case,choice_row,probe['landing_choice'],allow_delayed_start=True)
        comparison = compare_screen(case,screen,control,probe['outcome']['tick'],outcome['tick'],probe['landing_choice'])
    else:
        acquisition_audit = dict(outcome=outcome,transfer_outcome=probe['outcome'])
        native_audit = None
    trace_digest = C.archive(root)
    assert trace_digest == trace['trace_sha256']
    # The physical replay continues beyond transfer; audit only its transfer prefix.
    C.audit_controls(case,root,T.rows(root/'transfer-probe.jsonl'),end_tick=probe['outcome']['tick'])
    return dict(trace=trace,sensors=sensors,budget=budget,source=source_audit,frozen_screen_audit=frozen_audit,
        transfer=transfer_audit,acquisition=acquisition_audit,capture=capture_audit,native_choice_audit=native_audit,
        comparison=comparison,elapsed_ticks=report['elapsed_ticks'])


def aggregate(runs):
    comparisons = [r['comparison'] for r in runs.values() if r['comparison']]
    sites = [s for r in comparisons for s in r['sites']]
    directions = [d for s in sites for d in s['direction_comparisons']]
    residuals = [s['same_frame_geometry_residual'] for s in sites if s['same_frame_geometry_residual']]
    return dict(runs=len(runs),compared=len(comparisons),physical_ticks=sum(r['elapsed_ticks'] for r in runs.values()),
        trace_rows=sum(r['trace']['controller_rows'] for r in runs.values()),
        trace_bytes=sum(r['trace']['decompressed_bytes'] for r in runs.values()),
        actual_minus_predicted_transfer_ticks=[r['actual_minus_predicted_transfer_ticks'] for r in comparisons],
        actual_acquisition_ticks=[r['actual_acquisition_ticks'] for r in comparisons],
        selected_outside_retained_set=sum(r['native_choice_outside_retained_set'] for r in comparisons),
        sites=len(sites),native_site_statuses=dict(Counter(s['native_site_status'] for s in sites)),
        geometry_within_tolerance=sum(r['within_reconstruction_tolerance'] for r in residuals),
        maximum_geometry_residual=max((r['maximum'] for r in residuals),default=None),
        direction_signs=dict(Counter('/'.join(d[k] for k in ['frozen_sign','retrospective_sign','native_sign']) for d in directions)),
        directions=len(directions),choice_age_and_identity_valid=sum(s['retrospective']['choice']['age_and_identity_valid'] for s in sites),
        capture_outcomes=dict(Counter(r['capture']['outcome'] for r in runs.values())))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--retained',type=Path,default=Path('docs/data/capture-remote-retention-v1.json'))
    parser.add_argument('--physical',type=Path,default=Path('docs/data/capture-followthrough-v1.json'))
    parser.add_argument('--acquisition',type=Path,default=Path('docs/data/capture-site-acquisition-v1.json'))
    parser.add_argument('--ordinary',type=Path,default=Path('target/capture-flag-survey/remote-retention-v1'))
    parser.add_argument('--binary',type=Path,default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code and plan first'
    retained,physical,acquisition = [json.loads(p.read_text()) for p in [args.retained,args.physical,args.acquisition]]
    assert F.digest(args.ordinary/'summary.json') == retained['raw_summary_sha256']
    cases = plan(retained,physical)
    binary = args.binary.resolve(strict=True)
    assert F.digest(binary) == retained['binary_sha256'], 'reuse the frozen profiled runtime for this analysis-only study'
    args.out.mkdir(parents=True,exist_ok=False)
    result = dict(schema=1,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        runtime_commit=retained['source_commit'],binary_sha256=F.digest(binary),
        input_hashes={str(p):F.digest(p) for p in [args.retained,args.physical,args.acquisition]},
        plan=cases,runs={},scope='Four fixed correlated distant nominations; retrospective comparisons never backfill source predictions. No runtime/control/ranking/cost changes.')
    def save(): (args.out/'summary.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    save()
    for spec in cases:
        name = spec['case']['name']
        root = args.out/name
        command = [str(binary)]+spec['command'][1:]
        command[command.index('--out')+1] = str(root)
        try:
            with (args.out/(name+'.log')).open('w') as log:
                subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,check=True)
            audit = audit_replay(spec,root,args.ordinary/(spec['retained_run']+'-on'),physical,acquisition,retained['pairs'][spec['retained_run']])
            result['runs'][name] = dict(command=command,**audit,
                hashes={p.name:F.digest(p) for p in sorted(root.iterdir()) if p.is_file()})
            save()
            print(name,'compared' if audit['comparison'] else 'no comparable arrival/choice',flush=True)
        except Exception as error:
            result['error'] = dict(case=name,error=repr(error),command=command)
            save()
            raise
    assert F.digest(binary) == result['binary_sha256']
    assert all(F.digest(Path(p)) == digest for p,digest in result['input_hashes'].items())
    result['complete'] = True
    result['results'] = aggregate(result['runs'])
    save()
    print(json.dumps(result['results'],indent=2))


if __name__ == '__main__': main()
