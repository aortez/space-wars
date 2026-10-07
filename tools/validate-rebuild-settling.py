#!/usr/bin/env python3
"""Freeze four retained games and twenty established trials for rebuild alignment."""
import argparse
import copy
import importlib.util
from itertools import zip_longest
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('terrain', Path(__file__).with_name('validate-terrain-flight.py'))
T = importlib.util.module_from_spec(spec); spec.loader.exec_module(T)
A, L, ROOT, P, I, B, D = T.A, T.L, T.ROOT, T.P, T.I, T.B, T.D
PROFILE = 'native_rebuild_settling_v1'
SOURCE = ROOT/'target/terrain-flight-forecast/v1/summary.json'
OWN = ('tools/validate-rebuild-settling.py', 'tools/tests/test_rebuild_settling.py',
       'docs/rebuild-settling-plan.md')
NEW = ('scenarios/spacewars/src/surface_sortie/tests/fixtures/rebuild-misalignment.json',
       'tools/run-ground-navigation-trials.py')
CHANGED = {'scenarios/spacewars/src/surface_sortie/landing.rs',
           'scenarios/spacewars/src/surface_sortie/rebuild_placement.rs'}


def inputs():
    return dict(T.inputs(), **{p:P.digest(ROOT/p) for p in (*OWN,*NEW)})


def jobs(source, binary, out):
    result = []
    for name in A.ORDER:
        prior = source['runs'][name]; cmd = list(prior['command'])
        assert len(cmd)%2 == 1 and len(set(cmd[1::2])) == len(cmd[1::2])
        assert cmd[cmd.index('--terrain-flight-forecast')+1] == 'true'
        cmd[0] = str(binary); cmd[cmd.index('--out')+1] = str(out/'raw'/name)
        result.append(dict(key=name, item=dict(prior['item'],stage='rebuild_settling'), command=cmd))
    return result


def lab_command(binary, out):
    return ['python3', str(ROOT/'tools/run-ground-navigation-trials.py'),
            '--binary', str(binary), '--out', str(out/'lab')]


def verify(plan, out, reaudit=False):
    assert plan['profile'] == PROFILE
    current = inputs(); assert current.keys() == plan['inputs'].keys()
    changed = {p for p in current if current[p] != plan['inputs'][p]}
    assert not changed or reaudit and changed <= set(OWN[:2])
    sources = {}
    for key, record in plan['sources'].items():
        assert P.digest(record['path']) == record['sha256']
        sources[key] = json.loads(Path(record['path']).read_text())
        assert sources[key]['complete']
        old = sources[key]['binary']; assert P.digest(old['path']) == old['sha256']
    assert {p for p,h in sources['terrain']['inputs'].items() if plan['inputs'][p] != h} == CHANGED
    for binary in (plan['binary'], plan['lab_binary']):
        assert P.digest(binary['path']) == binary['sha256']
    assert plan['jobs'] == jobs(sources['terrain'], Path(plan['binary']['path']), out)
    assert plan['lab_command'] == lab_command(Path(plan['lab_binary']['path']), out)
    return sources


def freeze(out, binary, lab_binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    out.mkdir(parents=True,exist_ok=False)
    for name in ('raw','logs','archives','inputs'): (out/name).mkdir()
    binaries = {}
    for key, source in (('binary',binary), ('lab_binary',lab_binary)):
        target = out/source.name; shutil.copy2(source,target); target.chmod(0o555)
        binaries[key] = dict(path=str(target),sha256=P.digest(target))
    source = json.loads(SOURCE.read_text())
    plan = dict(schema=1,profile=PROFILE,inputs=inputs(),fresh_games=0,default_promotion=False,
        runtime_source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        sources={k:dict(path=str(p),sha256=P.digest(p)) for k,p in [('terrain',SOURCE),('original',T.ORIGINAL)]},
        **binaries, jobs=jobs(source,Path(binaries['binary']['path']),out),
        lab_command=lab_command(Path(binaries['lab_binary']['path']),out),
        build_command=['cargo','+1.89.0','build','--release','--locked','-p','spacewars-ai',
            '--example','surface_mission_soak','--example','surface_flag_soak','--features','sensor-profile'])
    verify(plan,out); I.write(out/'plan.json',plan)
    print('Frozen four retained full games and twenty established recovery trials.',flush=True)


def physical_pilot(row):
    pilot = copy.deepcopy(L.X.R.pilot(row))
    # Placement previews/rejection reports are diagnostics, not body state.
    if pilot.get('recovery'): pilot['recovery'].pop('placement',None)
    return pilot


def comparison(before, after):
    result = dict(common_rows=0,before_rows=0,after_rows=0,first_observation=None,
                  first_action=None,first_native=None)
    for a,b in zip_longest(before,after):
        result['before_rows'] += a is not None; result['after_rows'] += b is not None
        if a is None or b is None: continue
        assert (a['tick'],a['seat']) == (b['tick'],b['seat']); result['common_rows'] += 1
        for key, changed in [('first_observation',a['observation']!=b['observation']),
                             ('first_action',a['actions']!=b['actions']),
                             ('first_native',physical_pilot(a)!=physical_pilot(b))]:
            if changed and result[key] is None:
                result[key] = dict(tick=b['tick'],seat=b['seat'],before=a,after=b)
    result['actions_retained'] = result['first_action'] is None and result['before_rows']==result['after_rows']
    return result


def check_placement(report):
    accepted = []
    for attempt in report['attempts']:
        angle = attempt.get('settling_angle_degrees')
        if attempt['rejection'] is None:
            assert angle is not None and math.isfinite(angle) and 0 <= angle < 20
            accepted.append(attempt['offset'])
        elif attempt['rejection'] == 'landing_misaligned':
            assert angle is not None and math.isfinite(angle) and angle >= 20
    assert (report['selected_offset'] in accepted if report['selected_offset'] is not None else not accepted)


def placement_audit(rows):
    reports = []; seen = set(); builds = []; witnesses = []; counts = {}; phases = {}; failures = {}
    for row in rows:
        tick,seat = row['tick'],row['seat']; o = row['observation']['local']['combat']['recovery']
        p = L.X.R.pilot(row); recovery = p.get('recovery') or {}
        previews = [('native',recovery.get('placement'))]
        previews += [('preview',a.get('placement')) for a in (o.get('rebuild') or {}).get('attempts',[])]
        for kind,report in previews:
            if not report: continue
            identity = (kind,seat,json.dumps(report,sort_keys=True,separators=(',',':')))
            if identity in seen: continue
            seen.add(identity); check_placement(report)
            reports.append(dict(kind=kind,seat=seat,observed_tick=tick,report=report))
        rebuilt = recovery.get('rebuilds',0)
        if rebuilt > counts.get(seat,0):
            report = recovery['placement']; check_placement(report)
            assert report['selected_offset'] is not None and p['ship_form']=='ship'
            builds.append(dict(tick=tick,seat=seat,count=rebuilt,placement=report,pilot=p))
            witnesses.append(row)
        counts[seat] = rebuilt
        if rebuilt:
            key = (seat,rebuilt,p['landing']['phase'],json.dumps(p['location'],sort_keys=True))
            if key not in phases:
                phases[key] = tick; witnesses.append(row)
        task = row['mission'].get('recovery') or {}
        failure = (task.get('status'),task.get('reason'))
        if failure[1] and failures.get(seat)!=failure:
            witnesses.append(row); failures[seat] = failure
    return dict(reports=reports,builds=builds,witnesses=witnesses,
        rejected_alignment=sum(a['rejection']=='landing_misaligned' for r in reports for a in r['report']['attempts']))


def run_case(job, plan, sources, out, reaudit):
    key = job['key']; prior = sources['terrain']['runs'][key]
    root,oldroot = out/'raw'/key,out/'inputs'/key
    log,marker = out/'logs'/(key+'.log'),out/(key+'-raw.json')
    result = copy.deepcopy(job)
    if marker.exists():
        assert reaudit
        raw = json.loads(marker.read_text()); assert raw['command']==job['command'] and P.digest(log)==raw['log_sha256']
        if not root.exists(): B.unpack(json.loads((out/(key+'-result.json')).read_text())['archive'],root)
        result.update(hashes=raw['hashes'],log_sha256=raw['log_sha256'],reused_raw=True)
    else:
        assert not root.exists() and not log.exists(), 'partial games are never retried'
        with log.open('x') as stream: subprocess.run(job['command'],stdout=stream,stderr=stream,check=True,timeout=1800)
        result.update(hashes=I.raw_hashes(root),log_sha256=P.digest(log)); I.write(marker,result)
    assert all(P.digest(root/p)==sha for p,sha in result['hashes'].items())
    report = json.loads((root/'report.json').read_text())
    result.update(L.Q.analyze(root,job,root))
    result['stopping_rule'] = L.X.audit_switch(D.rows(root/'trace.jsonl'),report,job['item'])
    result['defense'] = L.C.A.audit(root,job['item'],report)
    result['clearance'] = L.C.audit(root,job['item'],report)
    result['native'] = L.S.native_audit(root,report,job['item'])
    result['observer'] = L.observer_audit(D.rows(root/'impact.jsonl'),D.rows(root/'trace.jsonl'),report,report['seat'])
    ground = A.ground_audit(D.rows(root/'trace.jsonl'),report['elapsed_ticks']); I.write(root/'ground-arrival-audit.json',ground)
    result['ground'] = dict(sha256=P.digest(root/'ground-arrival-audit.json'),attempts=len(ground['attempts']),
        completed=sum(a['completed_tick'] is not None for a in ground['attempts'].values()))
    terrain = T.terrain_audit(D.rows(root/'trace.jsonl')); I.write(root/'terrain-flight-audit.json',terrain)
    result['terrain'] = dict(sha256=P.digest(root/'terrain-flight-audit.json'),surveys=len(terrain['surveys']),
        launches=terrain['launches'],milestones=terrain['milestones'])
    placement = placement_audit(D.rows(root/'trace.jsonl')); I.write(root/'rebuild-placement-audit.json',placement)
    result['placement'] = dict(sha256=P.digest(root/'rebuild-placement-audit.json'),reports=len(placement['reports']),
        rejected_alignment=placement['rejected_alignment'],builds=placement['builds'])
    L.extract(prior['archive'],oldroot,('trace.jsonl','report.json'))
    delta = comparison(D.rows(oldroot/'trace.jsonl'),D.rows(root/'trace.jsonl')); I.write(root/'baseline-comparison.json',delta)
    result['comparison'] = {k:(dict(tick=v['tick'],seat=v['seat']) if isinstance(v,dict) else v) for k,v in delta.items()}
    result['retained_physics'] = delta['first_native'] is None and result['elapsed_ticks']==prior['elapsed_ticks'] and result['players']==prior['players']
    result['retained_actions'] = delta['actions_retained']
    result['retained_non_timing_report'] = D.timing_free(json.loads((oldroot/'report.json').read_text()))==D.timing_free(report)
    result['exact_streams'] = [p for p,sha in prior['hashes'].items() if result['hashes'].get(p)==sha]
    result['sequence'] = L.sequence(root,report,job['item']['seat'])
    assert all(P.digest(root/p)==sha for p,sha in result['hashes'].items())
    result['audited'] = True
    suffix = '-'+P.digest(__file__)[:12] if reaudit else ''
    result['archive'] = B.pack(root,out/'archives'/(key+suffix+'.tar.gz'))
    I.write(out/(key+'-result.json'),result)
    print(key,'audited:',result['elapsed_ticks'],'ticks; rebuilds',len(placement['builds']),flush=True)
    return result


def run_lab(plan,out):
    marker,log = out/'lab-result.json',out/'logs/lab.log'
    if marker.exists():
        result = json.loads(marker.read_text())
        assert result['command']==plan['lab_command'] and P.digest(log)==result['log_sha256']
        for record in result['archives'].values(): B.verify_archive(record)
        return result
    assert not (out/'lab').exists() and not log.exists(), 'partial lab runs are never retried'
    with log.open('x') as stream:
        proc = subprocess.run(plan['lab_command'],stdout=stream,stderr=stream,timeout=1800)
    rows = json.loads((out/'lab/summary.json').read_text()); assert len(rows)==20
    archives = {r['name']:B.pack(out/'lab'/r['name'],out/'archives'/(r['name']+'.tar.gz')) for r in rows}
    result = dict(command=plan['lab_command'],exit_code=proc.returncode,rows=rows,archives=archives,
        accepted=all(r['accepted'] for r in rows) and proc.returncode==0,log_sha256=P.digest(log),
        summary_sha256=P.digest(out/'lab/summary.json'))
    I.write(marker,result); print('Established trials accepted:',sum(r['accepted'] for r in rows),'/ 20',flush=True)
    return result


def decision(runs,sources,lab):
    target = runs[A.TARGET]; actor = target['players'][1]
    before = sources['terrain']['runs'][A.TARGET]['players'][1]
    original = sources['original']['runs'][A.TARGET]['players'][1]
    completed = [f for f in target['terrain']['launches'].values() if f['seat']==1 and f['completed_tick'] is not None]
    milestones = [m for m in target['terrain']['milestones'] if m['seat']==1]
    after = min((f['completed_tick'] for f in completed),default=10**20)
    claim = next((m for m in milestones if m['tick']>=after and m['state']['planet']==0 and m['state']['claim_owner']=='player_2'),None)
    rebuild = next((b for b in target['placement']['builds'] if b['seat']==1 and claim and b['tick']>=claim['tick']),None)
    boarded = next((m for m in milestones if rebuild and m['tick']>=rebuild['tick'] and m['state']['form']=='ship' and isinstance(m['state']['location'],dict) and 'aboard' in m['state']['location']),None)
    recovery = bool(boarded) and actor['completed_recoveries']>before['completed_recoveries']
    controls = all(runs[n]['retained_physics'] and runs[n]['retained_actions'] for n in A.ORDER if n!=A.TARGET)
    survival = actor['outcome']==original['outcome'] and actor['pilot_deaths']<=original['pilot_deaths'] and actor['ships_lost']<=original['ships_lost']
    qualified = recovery and controls and survival and lab['accepted']
    return dict(decision='advance_to_broader_validation' if qualified else 'not_qualified',
        complete_recovery=recovery,controls_retained=controls,survival_retained=survival,established_trials_retained=lab['accepted'],
        native_receipt_ticks=dict(claim=claim and claim['tick'],rebuild=rebuild and rebuild['tick'],boarding=boarded and boarded['tick']),
        before=before,original_before=original,after=actor,default_promotion=False)


def execute(path,reaudit):
    out = path.parent; plan = json.loads(path.read_text()); sources = verify(plan,out,reaudit)
    target = out/'summary.json'
    if target.exists():
        assert reaudit
        backup = out/('summary-before-reaudit-'+P.digest(target)[:12]+'.json')
        assert not backup.exists(); shutil.copy2(target,backup)
    summary = dict(schema=1,profile=PROFILE,complete=False,inputs=inputs(),binary=plan['binary'],lab_binary=plan['lab_binary'],
        runtime_source_commit=plan['runtime_source_commit'],plan_sha256=P.digest(path),reaudit=reaudit,runs={})
    I.write(target,summary)
    try:
        summary['lab'] = run_lab(plan,out); I.write(target,summary)
        for job in plan['jobs']:
            summary['runs'][job['key']] = run_case(job,plan,sources,out,reaudit); I.write(target,summary)
        verify(plan,out,reaudit)
        summary.update(complete=True,screen=decision(summary['runs'],sources,summary['lab']))
    except BaseException:
        summary['error'] = traceback.format_exc(); raise
    finally: I.write(target,summary)
    print('Complete:',summary['screen']['decision'],flush=True)


if __name__=='__main__':
    assert __debug__
    parser = argparse.ArgumentParser(description=__doc__); sub = parser.add_subparsers(dest='action',required=True)
    p = sub.add_parser('plan'); p.add_argument('--out',type=Path,required=True)
    p.add_argument('--binary',type=Path,required=True); p.add_argument('--lab-binary',type=Path,required=True)
    p = sub.add_parser('run'); p.add_argument('--plan',type=Path,required=True); p.add_argument('--reaudit',action='store_true')
    args = parser.parse_args()
    if args.action=='plan': freeze(args.out.resolve(),args.binary.resolve(),args.lab_binary.resolve())
    else: execute(args.plan.resolve(),args.reaudit)
