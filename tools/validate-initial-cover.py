#!/usr/bin/env python3
"""Frozen initial cover admission against retained matches and health regressions."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import importlib.util
from itertools import zip_longest
import json
import math
from pathlib import Path
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('approach', Path(__file__).with_name('probe-threatened-approach.py'))
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)
H, C, F = A.H, A.H.C, A.F
PROFILE = 'initial_qualified_cover_v1'
HEALTH_CASES = ['shared-armed-world0-p1-walking', 'shared-armed-world0-p1-powered',
                'shared-armed-world1-p1-powered']


def plan():
    primary = [dict(name=p['name'], source='retained', item=p, enabled=p['cover']) for p in C.P.plan()]
    lookup = {p['name']:p for p in C.P.plan()}
    return primary + [dict(name='health-'+name, source='health', item=lookup[name], enabled=True)
                      for name in HEALTH_CASES]


def command(old, binary, root, enabled):
    result = list(old['command'])
    assert '--initial-cover-seats' not in result
    result[0] = str(binary)
    result[result.index('--out')+1] = str(root)
    if enabled:
        seat = str(old['item']['seat'])
        assert result[result.index('--cover-response-seats')+1] in [seat, 'both']
        result += ['--initial-cover-seats', seat]
    return result


def without_initial(value):
    if isinstance(value, dict):
        return {k:without_initial(v) for k,v in value.items() if k != 'initial_cover'}
    if isinstance(value, list):
        return [without_initial(v) for v in value]
    return value


def qualifies(local, site):
    geometry = A.geometry(local, site)
    cover = geometry['cover']
    return bool(cover and cover['grounded'] and (cover['approach'] or
        geometry['aligned'] and abs(geometry['lateral_speed']) < 18 and geometry['height'] < 40))


def solar_qualified(local, site, native=None):
    plans = [A.C.solar_plan(local, site, side) for side in [-1, 1]]
    keys = ['approach_clearance', 'parked_clearance', 'departure_clearance']
    assert any(p is None or min(p[k] for k in keys) >= -A.C.CLEARANCE_TOLERANCE for p in plans)
    if native is not None:
        assert any(p and all(abs(native[k]-p[k]) <= A.C.CLEARANCE_TOLERANCE
                            for k in keys+['arrival_seconds']) for p in plans)
    return plans


def witness_observation(row):
    o = row['initial_cover']['observation']
    local = o['local']
    p = local['combat']['recovery']['flight']['pilot']
    assert {k:v for k,v in p.items() if k != 'sites'} == row['pilot']
    assert len(p['sites']) <= 64 and len({A.identity(s['id']) for s in p['sites']}) == len(p['sites'])
    for key in ['cover', 'landing_objective', 'objective_work', 'objective_evidence']:
        assert local.get(key) == row.get(key)
    assert o['planets'] == row['planets'] and o['match_context'] == row['match_context']
    return local, p


def audit_event(row, previous_requests, seed):
    local, p = witness_observation(row)
    tick = p['tick']; cap = row['capture']; initial = cap['initial_cover']
    response = cap['cover_response']; search = response['search']
    sites = {A.identity(s['id']):s for s in p['sites']}
    target = local['combat']['target']
    exposed = bool(target and not target['ground_occluded'] and
        math.dist(A.J.vector(target['motion']['position']), A.J.vector(p['ship']['position'])) < 300)
    if initial['armed_tick'] == tick:
        assert exposed and p['controls_armed'] and p['queries_ready']
        assert p['ship_available'] and p['ship_form'] == 'ship'
        assert p['landing']['supported_feet'] == 0 and p['landing']['phase'] != 'landed'
        assert isinstance(p['location'], dict) and 'aboard' in p['location']
        assert response['failures'] == 0 and response['required_since'] == tick
    if initial['last_seed_tick'] == tick:
        assert exposed and search['seeded'] and initial['finished_tick'] is None
        assert p['site_query'] == 'survey' or p['site_query'] == dict(selected=cap['acquisition']['required_site'])
        assert search['planet'] == p['planet']['index'] and search['revision'] == p['planet']['revision']
        for identity in search['pending']:
            site = sites[A.identity(identity)]
            assert site['revision'] == p['planet']['revision'] and qualifies(local, site)
            solar_qualified(local, site)
        seed = dict(tick=tick, ids=list(search['pending']), planet=search['planet'], revision=search['revision'])
    request = initial['last_request']
    if request and request['tick'] == tick:
        assert initial['requests'] == previous_requests+1 <= 8
        assert seed and request['site'] in seed['ids']
        assert (search['planet'],search['revision']) == (seed['planet'],seed['revision'])
        assert search['pending'][0] == request['site'] and cap['site'] is None
        assert initial['finished_tick'] is None and response['failures'] == 0
    else:
        assert initial['requests'] == previous_requests
    if initial['finished_tick'] == tick:
        if initial['outcome'] == 'selected_site':
            assert cap['acquisition']['reason'] == 'selected_site'
            assert initial['selected_site'] == cap['site']
            assert initial['selected_exposed'] == exposed
            site = sites[A.identity(cap['site'])]
            assert site['revision'] == p['planet']['revision']
            if exposed:
                assert initial['armed_tick'] is not None and qualifies(local, site)
            solar_qualified(local, site, cap['solar'])
            if cap['objective_route'] is not None:
                survey = row['landing_objective']; route = cap['objective_route']
                assert survey and route in survey['sites'] and route['site'] == cap['site']
                assert route['outbound']['failure'] is None and route['returning']['failure'] is None
                assert not route['outbound']['partial'] and not route['returning']['partial']
        else:
            assert initial['outcome'] == 'physical_surface'
            assert (p['location'] == 'on_foot' or p['landing']['supported_feet'] > 0 or p['landing']['phase'] == 'landed')
    return seed


def audit_initial(root, item, enabled):
    report = json.loads((root/'report.json').read_text())
    if enabled:
        assert report['initial_cover'] == dict(profile=PROFILE, enabled_seats=[s == item['seat'] for s in range(2)])
    else:
        assert 'initial_cover' not in report
    captures, seeds, requests, witnesses = {}, {}, {}, []
    counts = dict(armed=0, requests=0, requests_without_survey=0, first_choices=0, exposed_first_choices=0)
    for row in F.rows(root/'capture-evidence.jsonl'):
        cap = row.get('capture')
        initial = cap.get('initial_cover') if cap else None
        if cap:
            assert (initial is not None) == (enabled and row['seat'] == item['seat'])
        if initial is None:
            assert 'initial_cover' not in row
            continue
        tick = row['pilot']['tick']
        birth = initial['armed_tick'] if initial['armed_tick'] is not None else initial['finished_tick']
        if birth is None:
            assert initial['requests'] == 0 and 'initial_cover' not in row
            continue
        key = row['seat'], birth
        response = cap['cover_response']; search = response['search']
        assert response['measured_sites'] <= response['requested_sites'] <= 8 * response['searches']
        if search:
            assert search['deadline_tick'] == search['started_tick']+600
            assert 0 <= search['probes'] <= 8 and len(search['pending'])+len(search.get('walk_deferred',[])) <= 8
            if initial['armed_tick'] is not None and initial['finished_tick'] is None:
                assert search['started_tick'] == initial['armed_tick']
                assert response['failures'] == 0
        event = (initial['armed_tick'] == tick or initial['finished_tick'] == tick or
                 initial['last_seed_tick'] == tick or (initial['last_request'] or {}).get('tick') == tick)
        assert ('initial_cover' in row) == event
        if event:
            seeds[key] = audit_event(row, requests.get(key,0), seeds.get(key))
            witnesses.append(row)
            counts['armed'] += int(initial['armed_tick'] == tick)
            requested = (initial['last_request'] or {}).get('tick') == tick
            counts['requests'] += int(requested)
            counts['requests_without_survey'] += int(requested and row['landing_objective'] is None)
            chosen = initial['finished_tick'] == tick and initial['outcome'] == 'selected_site'
            counts['first_choices'] += int(chosen)
            counts['exposed_first_choices'] += int(chosen and initial['selected_exposed'])
        else:
            assert initial['requests'] == requests.get(key,0)
        requests[key] = initial['requests']
        captures[key] = dict(seat=row['seat'], last_tick=tick, initial=initial, search=search,
                             failure=cap['failure'], failed_tick=cap['failed_tick'], completed_tick=cap['completed_tick'])
    path = root/'initial-cover-witnesses.json'
    F.D.write(path, dict(schema=1,profile=PROFILE,counts=counts,captures=list(captures.values()),rows=witnesses))
    return dict(**counts, first_armed_tick=min((c['initial']['armed_tick'] for c in captures.values()
        if c['initial']['armed_tick'] is not None), default=None), captures=list(captures.values()), witness_sha256=F.E.digest(path))


def unchanged_state(old, new):
    roots = [C.P.root_of(r) for r in [old,new]]
    reports = [without_initial(json.loads((r/'report.json').read_text())) for r in roots]
    C.P.M.T.report_parity(*reports,C.P.M.V.EXACT_REPORT_FIELDS+['metrics','policy_configuration','cover_response','destination_retry','pursuit_health'])
    assert old['allocation'] == new['allocation']
    streams = C.P.M.V.EXACT_STREAMS+['capture-evidence.jsonl']
    for name in streams:
        for a,b in zip_longest(*(F.rows(r/name) for r in roots)):
            assert a is not None and b is not None and without_initial(a) == without_initial(b), name
    return dict(exact_streams_except_initial_telemetry=streams, sensor_parity=C.P.M.C.audit_sensors(*roots), exact_allocation=True)


def run(entry, old, binary, out, candidate):
    item = entry['item']; enabled = candidate and entry['enabled']
    root = out/(entry['name']+('-initial' if candidate else '-retained'))
    cmd = command(old,binary,root,enabled)
    result = dict(item=item,command=cmd)
    try:
        with (out/(root.name+'.log')).open('x') as log:
            subprocess.run(cmd,check=True,stdout=log,stderr=log,timeout=1800)
        result.update(C.analyze(root,item))
        if entry['source'] == 'health':
            result['pursuit_health'] = H.audit_health(root,item)
        if candidate:
            result['initial_cover'] = audit_initial(root,item,enabled)
            result['comparison'] = C.M.compare(C.P.root_of(old),root)
            diff = result['comparison']['first_control_difference']
            if diff is not None:
                assert result['initial_cover']['first_armed_tick'] is not None
                assert diff['tick'] >= result['initial_cover']['first_armed_tick']
            elif result['initial_cover']['armed'] == 0:
                result['unchanged_state'] = unchanged_state(old,result)
            if not enabled:
                result['disabled_control_parity'] = C.P.replay_parity(old,result)
        else:
            result['replay_parity'] = C.P.replay_parity(old,result)
            if entry['source'] == 'health':
                assert result['pursuit_health'] == old['pursuit_health']
        result['hashes'] = {p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    except Exception:
        result['error'] = traceback.format_exc()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior',type=Path,required=True)
    parser.add_argument('--health',type=Path,required=True)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code, tests and plan first'
    inputs = dict(retained=args.prior,health=args.health)
    prior = {key:json.loads(path.read_text()) for key,path in inputs.items()}
    for data in prior.values():
        assert data['complete'] and data['plan'] == C.P.plan()
        H.verify_inputs(data)
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True,exist_ok=False)
    result = dict(schema=1,complete=False,profile=PROFILE,
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.E.digest(binary),inputs={k:dict(path=str(p),sha256=F.E.digest(p)) for k,p in inputs.items()},
        runner_sha256=F.E.digest(Path(__file__)),
        tools={name:F.E.digest(Path(__file__).with_name(name)) for name in
            set(prior['health']['tools'])|{'validate-pursuit-health.py','probe-threatened-approach.py','probe-capture-jetpack.py','compare-landing-choices.py'}},
        plan=plan(),retention={},runs={})
    save = lambda:F.D.write(args.out/'summary.json',result)
    save()
    try:
        for candidate in [False,True]:
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures = [pool.submit(run,e,prior[e['source']]['runs'][e['item']['name']],binary,args.out,candidate) for e in result['plan']]
                for entry,future in zip(result['plan'],futures):
                    new = future.result()
                    (result['runs'] if candidate else result['retention'])[entry['name']] = new
                    save()
                    assert 'error' not in new, (entry['name'],new.get('error'))
                    print(entry['name']+(': initial cover audited' if candidate else ': retained exactly'),flush=True)
        for key,data in prior.items():
            H.verify_inputs(data)
            assert F.E.digest(inputs[key]) == result['inputs'][key]['sha256']
        assert F.E.digest(binary) == result['binary_sha256']
        result['complete'] = True
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
