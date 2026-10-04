#!/usr/bin/env python3
"""Audit recorded destination returns without changing or replaying gameplay."""
import argparse
from collections import Counter
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess


def module(name, filename):
    spec=importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result=importlib.util.module_from_spec(spec);spec.loader.exec_module(result)
    return result


Q=module('cover_response','validate-cover-response.py')
F=Q.F
DEFER_TICKS=1800
NO_DEFERRAL={
    'ship or surface recovery required', 'destination already secured',
    'left destination approach frame', 'pausing travel for nearby opponent',
    'better supported capture value', 'shorter supported capture trip',
    'experimental destination nomination',
}
DIRECT_DEFERRAL={'destination disappeared','departure did not clear its planet',
                 'transfer exhausted its progress budget'}
SCOPE=('Read-only audit of the 74 frozen cover-response runs, restricted to each tested v13 seat. '
       'First terminal events and accepted-switch source forecasts are joined to exact visits. '
       'An unwitnessed capture failure has unknown deferral semantics. Same-planet returns '
       'do not establish unchanged cover, ownership, route feasibility or causation. '
       'No gameplay replay, policy change, counterfactual success or fresh strength evidence.')


def snapshots(report, seat):
    yield report['missions'][seat]
    for row in report.get('events', []):
        if row['seat']==seat: yield row['telemetry']
    for row in report.get('samples', []):
        yield row['missions'][seat]


def failure_witness(report, seat, visit):
    found=[]
    for mission in snapshots(report,seat):
        c=mission.get('capture')
        if not c or mission.get('target')!=visit['planet'] or c.get('failure')!=visit['reason']:
            continue
        selected=next((e for e in reversed(mission['events']) if e['kind']=='selected'),None)
        if selected is None or (selected['planet'],selected['tick'])!=(visit['planet'],visit['selected_tick']):
            continue
        failed=c.get('failed_tick')
        if failed is None or not visit['selected_tick']<=failed<=visit['abandoned_tick']:
            continue
        found.append(dict(failed_tick=failed,started_tick=c.get('started_tick'),failure=c['failure'],
            goal=c['goal'],site=c.get('site'),acquisition=c.get('acquisition'),
            cover_response=c.get('cover_response'),cover_replans=c.get('cover_replans'),
            objective_route=c.get('objective_route')))
    return min(found,key=lambda x:x['failed_tick']) if found else None


def deferral(reason, witness):
    if reason in NO_DEFERRAL:
        return False, 'explicit non-deferring coordinator path'
    if reason in DIRECT_DEFERRAL:
        return True, 'explicit deferring coordinator path'
    if witness is not None:
        return True, 'witnessed terminal capture failure consumed by coordinator'
    return None, 'no verified call path for this terminal reason'


def pursuits(events, start, stop=None):
    """Only a paired start/end establishes duration; an open episode stays censored."""
    result=[]
    for i,event in enumerate(events):
        if event['kind']!='pursuit_started' or event['tick']<start or (stop is not None and event['tick']>=stop):
            continue
        following=events[i+1:]
        end=next((e for e in following if e['kind'] in ['pursuit_ended','pursuit_started']),None)
        if end is not None and end['kind']=='pursuit_started':
            raise ValueError('pursuit restarted without an ending')
        if end is not None and stop is not None and end['tick']>stop:
            end=None
        result.append(dict(start=event,end=end,duration_ticks=None if end is None else end['tick']-event['tick']))
    return result


def selection_source(verified, decisions, seat):
    event=verified['selection'];reason=event['reason']
    if reason is None:
        return dict(kind='initial_picker',scope='The native selected event has no switch reason; no cost-based initial selection is authorized by the current evaluator.')
    if reason not in ['better supported capture value','shorter supported capture trip']:
        return dict(kind='other',reason=reason)
    matches=[d for d in decisions['switches'] if d['seat']==seat and
             d['switch']['tick']==event['tick'] and d['switch']['to']==event['planet']]
    if len(matches)!=1:
        raise ValueError('accepted return switch needs exactly one source forecast')
    d=matches[0];switch=d['switch'];forecast=d['source_forecast']
    if (reason=='better supported capture value')!=(switch.get('value') is not None):
        raise ValueError('accepted switch reason disagrees with its value decision')
    if d['actual_visit']!=verified['recorded']:
        raise ValueError('switch forecast belongs to another visit')
    current=next(c for c in forecast['candidates'] if c['planet']==switch['from'])
    destination=next(c for c in forecast['candidates'] if c['planet']==switch['to'])
    return dict(kind='value_switch' if switch.get('value') else 'time_switch',switch=switch,
        source={k:forecast[k] for k in ['source_tick','completed_tick','current_target','selected_tick',
            'combat_risk','comparison_reason','opponent_distance']},
        current_candidate=current,destination_candidate=destination,
        nominal_destination_slower=destination['total_seconds']>current['total_seconds'])


def analyze(report, seat, decisions):
    audit=F.M.audit(report)
    if any(audit['counts'].get(k,0) for k in ['corrected_visits','milestones_outside_visit']):
        raise ValueError('visit endings disagree with terminal mission events')
    visits=[v for v in audit['visits'] if v['seat']==seat]
    events=report['missions'][seat]['events']
    records=[];counts=Counter(visits=len(visits));reasons=Counter()
    for index,record in enumerate(visits):
        if record['status']!='verified':
            counts['unverified_visits']+=1
            continue
        counts['verified_visits']+=1
        if record['outcome']!='abandoned': continue
        counts['abandoned_visits']+=1
        visit=record['recorded'];failure=record['terminal'];reason=failure['reason'];reasons[reason]+=1
        witness=failure_witness(report,seat,visit)
        deferred,proof=deferral(reason,witness)
        expiry=None if deferred is not True else failure['tick']+DEFER_TICKS
        following=visits[index+1:]
        next_visit=next((v for v in following if v['recorded']['planet']==visit['planet']),None)
        arrival_failure=visit['arrived_tick'] is not None and visit['claimed_tick'] is None
        row=dict(visit=visit,terminal=failure,arrival_without_claim=arrival_failure,
            failure_witness=witness,deferred=deferred,deferral_basis=proof,deferred_until=expiry,
            returned=next_visit is not None,return_unknown=None,return_visit=None,selection=None)
        records.append(row)
        counts['deferral_'+('unknown' if deferred is None else 'yes' if deferred else 'no')]+=1
        if witness is not None: counts['witnessed_capture_failures']+=1
        if arrival_failure: counts['arrived_unclaimed_abandonments']+=1
        stop=None if next_visit is None else next_visit['selected_tick']
        row['pursuits']=pursuits(events,failure['tick'],stop)
        if next_visit is None:
            counts['no_recorded_return']+=1
            row['return_unknown']='no later same-planet selection within this run; not permanent avoidance'
            continue
        if next_visit['status']!='verified':
            counts['unverified_returns']+=1
            row['return_unknown']='return selection lacks verified terminal history'
            continue
        returned=next_visit['recorded'];source=selection_source(next_visit,decisions,seat)
        elapsed=returned['selected_tick']-failure['tick']
        if elapsed<0 or (expiry is not None and returned['selected_tick']<expiry):
            raise ValueError('destination returned before its verified cooldown expired')
        row.update(return_visit=returned,selection=source,return_outcome=next_visit['outcome'],
            ticks_to_return=elapsed,ticks_after_expiry=None if expiry is None else returned['selected_tick']-expiry,
            intervening_visits=[v['recorded'] for v in following if v['selected_tick']<returned['selected_tick']],
            intervening_completed=sum(v.get('outcome')=='completed' for v in following if v['selected_tick']<returned['selected_tick']),
            intervening_unverified=sum(v['status']!='verified' for v in following if v['selected_tick']<returned['selected_tick']))
        if expiry is not None:
            row['confirmed_pursuit_overlap_ticks']=sum(max(0,min(expiry,e['end']['tick'])-max(failure['tick'],e['start']['tick']))
                for e in row['pursuits'] if e['end'] is not None)
            row['pursuit_ended_after_expiry']=any(e['end'] is not None and e['end']['tick']>=expiry for e in row['pursuits'])
        counts['verified_returns']+=1
        counts['return_'+source['kind']]+=1
        counts['return_ended_'+next_visit['outcome']]+=1
        if deferred is True:
            counts['deferred_returns']+=1
            counts['deferred_return_'+source['kind']]+=1
            counts['deferred_return_ended_'+next_visit['outcome']]+=1
            counts['deferred_return_after_pursuit_expiry']+=row['pursuit_ended_after_expiry']
        if witness is not None:
            counts['capture_failure_returns']+=1
            counts['capture_failure_return_'+source['kind']]+=1
        if source['kind'] in ['value_switch','time_switch']:
            destination=source['destination_candidate'];evidence=destination['evidence_tick']
            row['return_evidence_measured_after_failure']=None if evidence is None else evidence>failure['tick']
            validated=destination['route_validated_tick']
            row['return_route_validated_after_failure']=None if validated is None else validated>failure['tick']
            failed_revision=(witness or {}).get('acquisition')
            row['same_measured_material_revision']=None if failed_revision is None or failed_revision['planet']!=visit['planet'] else failed_revision['revision']==destination['revision']
            row['return_exposure_resolved']='unknown; published return cost does not model exposure'
            if deferred is True:
                counts['deferred_return_nominally_slower']+=source['nominal_destination_slower']
                counts['deferred_return_with_new_evidence']+=row['return_evidence_measured_after_failure'] is True
    return dict(counts=dict(counts),abandonment_reasons=dict(reasons),records=records,
        unverified=[v for v in visits if v['status']!='verified'],
        outcome=F.D.outcome(report,seat),completed_sorties=report['missions'][seat]['completed_sorties'])


def planned_runs(study):
    result=[dict(name=i['name'],kind=i['kind'],seat=i['seat'],candidate=i['candidate'],source=study['runs'][i['name']]) for i in study['plan']]
    seats={case[0]:case[3] for case in Q.A.CASES}
    result += [dict(name='recorded-'+name+'-'+arm,kind='recorded',seat=seats[name],candidate=arm=='candidate',source=r)
               for name,arms in study['regressions'].items() for arm,r in arms.items()]
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--study',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze analysis and plan first'
    source=args.study/'summary.json';study=json.loads(source.read_text());assert study['complete']
    plan=planned_runs(study);assert len(plan)==74
    native_paths=['crates/spacewars-ai/src/mission_pilot.rs','crates/spacewars-ai/src/mission_destination.rs',
        'crates/spacewars-ai/src/mission_evaluation/selection.rs','crates/spacewars-ai/src/mission_evaluation/flag_costs.rs',
        'crates/spacewars-ai/src/mission_evaluation/value.rs']
    for path in native_paths:
        recorded=subprocess.check_output(['git','show',study['source_commit']+':'+path])
        assert hashlib.sha256(recorded).hexdigest()==F.E.digest(Path(path)), 'native source changed since the recorded runtime'
    args.out.mkdir(parents=True,exist_ok=False)
    result=dict(schema=1,scope=SCOPE,complete=False,source_summary_sha256=F.E.digest(source),
        source_runtime_commit=study['source_commit'],source_binary_sha256=study['binary_sha256'],
        analysis_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        analysis_sha256=F.E.digest(Path(__file__)),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [Q,F,F.P,F.D,F.M]},
        source_files={p:F.E.digest(Path(p)) for p in native_paths},
        plan=[{k:v for k,v in i.items() if k!='source'} for i in plan],runs={},totals={})
    save=lambda:F.D.write(args.out/'summary.json',result)
    save()
    try:
        for item in plan:
            r=item['source'];root=Path(r['command'][r['command'].index('--out')+1]);inputs={}
            for filename in ['report.json','mission-evaluations.jsonl']:
                sha=F.E.digest(root/filename);assert sha==r['hashes'][filename]
                inputs[filename]=dict(path=str(root/filename),sha256=sha)
            report=json.loads((root/'report.json').read_text());assert report['physics_ok']
            decisions=F.P.switch_predictions(report,F.rows(root/'mission-evaluations.jsonl'))
            assert decisions==r['decisions'], 'accepted forecast reconstruction differs from frozen study'
            record=analyze(report,item['seat'],decisions)
            record['inputs']=inputs
            result['runs'][item['name']]=record
            save()
        for kind in ['recorded','directed','held_out']:
            for candidate in [False,True]:
                counts=Counter();reasons=Counter();runs=0
                for item in plan:
                    if item['kind']==kind and item['candidate']==candidate:
                        run=result['runs'][item['name']];runs+=1;counts.update(run['counts']);reasons.update(run['abandonment_reasons'])
                result['totals'][kind+'-'+('candidate' if candidate else 'predecessor')]=dict(runs=runs,counts=dict(counts),abandonment_reasons=dict(reasons))
        result['complete']=True;save()
        print(json.dumps(result['totals'],indent=2))
    except Exception as error:
        result['error']=repr(error);save();raise


if __name__=='__main__':main()
