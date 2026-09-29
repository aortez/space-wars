#!/usr/bin/env python3
"""Paired transfer forecasts with point-mass and body-origin motion."""
import argparse
import json
import math
from pathlib import Path
import statistics
import subprocess
import importlib.util


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


A = module('arrival_replay', 'compare-arrival-replays.py')
E = module('transfer_forecast', 'forecast-transfer-references.py')
F, C, T, Q = A.F, A.C, A.T, A.Q
MODELS = dict(point='guided_transfer_forecast_v1', body='guided_transfer_forecast_body_v2')


def radial_bearing(ship, planet):
    """Diagnostic f64 bearing of POSITION, never the ship's nose heading."""
    dx = ship['position']['x']-planet['position']['x']
    dy = ship['position']['y']-planet['position']['y']
    if math.hypot(dx,dy) == 0:
        return None
    angle = (-math.atan2(dx,dy)-planet['angle']) % math.tau
    return dict(radians=angle, nearest_bin=math.floor(angle*64/math.tau+.5)%64)


def compare_models(case, reports, trace):
    assert set(reports) == set(MODELS)
    assert trace and [r['tick'] for r in trace] == list(range(case['source_tick'],trace[-1]['tick']+1))
    assert all(r['target'] == case['destination'] for r in trace)
    terminal = trace[-1]['terminal']
    assert terminal and terminal['tick'] == trace[-1]['tick']
    assert all(r['terminal'] is None for r in trace[:-1])
    limit = len(trace)-1 if terminal['reason'] == 'arrived' else len(trace)-2
    models = {}
    by_tick = {}
    for label,report in reports.items():
        samples = {}
        invalid = []
        if report is not None:
            assert report['model'] == MODELS[label]
            assert report['source_tick'] == case['source_tick'] and report['destination'] == case['destination']
            for sample in report['samples']:
                t = sample['after_ticks']
                assert t not in samples and t not in invalid
                ship = sample['ship']
                values = [ship[key][axis] for key in ['position','velocity'] for axis in ['x','y']]
                values += [ship['angle'],ship['spin']]
                if not all(isinstance(v,(int,float)) and math.isfinite(v) for v in values):
                    assert report['end'] == 'non_finite' and t == report['ticks']
                    invalid.append(t)
                    continue
                samples[t] = sample
        by_tick[label] = samples
        endpoint = None
        if report and report['end'] == 'kinematic_handoff' and terminal['reason'] == 'arrived':
            predicted,actual = report['samples'][-1],trace[-1]
            planet = next(p['motion'] for p in actual['planets'] if p['index'] == case['destination'])
            endpoint = dict(predicted_tick=case['source_tick']+report['ticks'],actual_tick=actual['tick'],
                predicted_minus_actual_ticks=report['ticks']-(actual['tick']-case['source_tick']),
                different_time_motion_difference=A.motion_difference(predicted['ship'],actual['ship']),
                predicted_radial_bearing=radial_bearing(predicted['ship'],predicted['target']),
                actual_radial_bearing=radial_bearing(actual['ship'],planet))
        models[label] = dict(end=report['end'] if report else 'source_unknown',endpoint=endpoint,
                            invalid_prediction_samples=invalid)
    common = sorted(set(by_tick['point']) & set(by_tick['body']))
    common = [t for t in common if 0 <= t <= limit]
    paired = []
    for t in common:
        assert by_tick['point'][t]['target'] == by_tick['body'][t]['target'], 'motion model changed target ephemeris'
        row = dict(after_ticks=t,tick=case['source_tick']+t)
        for label in MODELS:
            sample = by_tick[label][t]
            difference = A.motion_difference(sample['ship'],trace[t]['ship'])
            assert all(math.isfinite(v) for v in difference.values())
            row[label] = dict(difference,same_frame=sample['frame'] == trace[t]['frame'])
        paired.append(row)
    for label in MODELS:
        values = [r[label] for r in paired if r['after_ticks'] > 0]
        models[label]['common_sample_metrics'] = dict(
            count=len(values),
            rms_position=math.sqrt(statistics.mean(v['position']**2 for v in values)) if values else None,
            rms_velocity=math.sqrt(statistics.mean(v['velocity']**2 for v in values)) if values else None,
            mean_absolute_heading_degrees=statistics.mean(abs(math.degrees(v['heading_actual_minus_predicted'])) for v in values) if values else None,
            maximum_absolute_heading_degrees=max((abs(math.degrees(v['heading_actual_minus_predicted'])) for v in values),default=None))
    return dict(physical_terminal=terminal,models=models,common_samples=paired,
                last_common_sample=paired[-1] if paired else None)


def aggregate(pairs):
    result = dict(cases=len(pairs),models={})
    for label in MODELS:
        endpoints = [p['comparison']['models'][label]['endpoint'] for p in pairs.values()]
        errors = [abs(e['predicted_minus_actual_ticks'])/60 for e in endpoints if e is not None]
        samples = [r[label] for p in pairs.values() for r in p['comparison']['common_samples'] if r['after_ticks'] > 0]
        result['models'][label] = dict(comparable_handoffs=len(errors),
            mean_absolute_time_seconds=statistics.mean(errors) if errors else None,
            maximum_absolute_time_seconds=max(errors,default=None),common_nonzero_samples=len(samples),
            rms_position=math.sqrt(statistics.mean(v['position']**2 for v in samples)) if samples else None,
            rms_velocity=math.sqrt(statistics.mean(v['velocity']**2 for v in samples)) if samples else None,
            mean_absolute_heading_degrees=statistics.mean(abs(math.degrees(v['heading_actual_minus_predicted'])) for v in samples) if samples else None,
            charged_graph=sum(p['runs'][label]['forecast_audit']['charged_graph'] for p in pairs.values()),
            maximum_prediction_ms=max((p['runs'][label]['forecast_audit']['prediction_ms'] for p in pairs.values()),default=None))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--ordinary',type=Path,default=Path('target/capture-flag-survey/remote-retention-v1'))
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code and plan first'
    paths = [Path('docs/data/'+n) for n in ['capture-remote-retention-v1.json',
        'capture-followthrough-v1.json','capture-site-acquisition-v1.json','capture-arrival-replay-v1.json']]
    retained,physical,acquisition,previous = [json.loads(p.read_text()) for p in paths]
    assert F.digest(args.ordinary/'summary.json') == retained['raw_summary_sha256']
    plan = A.plan(retained,physical)
    assert plan == previous['plan']
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True,exist_ok=False)
    commands = {}
    for spec in plan:
        name = spec['case']['name']
        commands[name] = {}
        for label in MODELS:
            command = [str(binary)]+spec['command'][1:]
            command[command.index('--out')+1] = str(args.out/(name+'-'+label))
            commands[name][label] = command+['--forecast-transfer','true',
                '--forecast-transfer-body-motion',str(label=='body').lower()]
    result = dict(schema=1,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.digest(binary),input_hashes={str(p):F.digest(p) for p in paths},
        plan=plan,commands=commands,pairs={},scope='Four fixed correlated nominations, two observational models. No live controls, ranking, cost admission, survey selection, or fitted constants change. Common-time ship errors stop at physical transfer end; endpoint differences may compare different times. Offline graph charges are not shared playing-budget work or device performance.')
    def save(): (args.out/'summary.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    save()
    for spec in plan:
        case,name = spec['case'],spec['case']['name']
        ordinary = args.ordinary/(spec['retained_run']+'-on')
        old = json.loads((ordinary/'report.json').read_text())
        source = next(s for s in old['transfer_comparison']['sources'] if s['seat'] == case['seat'])
        old_forecast = next(c['forecast'] for c in source['last_snapshot']['candidates'] if c['destination'] == case['destination'])
        pair = dict(runs={})
        result['pairs'][name] = pair
        for label,model in MODELS.items():
            root = args.out/(name+'-'+label)
            command = commands[name][label]
            log_path = args.out/(root.name+'.log')
            try:
                with log_path.open('w') as log:
                    subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,check=True)
                audit = A.audit_replay(spec,root,ordinary,physical,acquisition,retained['pairs'][spec['retained_run']])
                report = json.loads((root/'report.json').read_text())
                probe = report['transfer_probe']
                control = next(r for r in C.read_trace(root,case['source_tick'],case['source_tick']) if r['seat']==case['seat'])
                trace = T.rows(root/'transfer-probe.jsonl')
                forecast_audit = E.audit_forecast(case,probe,control,trace,model=model)
                if label == 'point':
                    assert probe['forecast']['report'] == old_forecast, 'legacy forecast changed from frozen distant candidate'
                pair['runs'][label] = dict(command=command,physical_audit=audit,
                    forecast=probe['forecast'],forecast_audit=forecast_audit,
                    log_sha256=F.digest(log_path),
                    hashes={p.name:F.digest(p) for p in sorted(root.iterdir()) if p.is_file()})
                save()
                print(name,label,'audited',flush=True)
            except Exception as error:
                result['error'] = dict(case=name,model=label,error=repr(error),command=command,
                                      log_sha256=F.digest(log_path) if log_path.exists() else None)
                save()
                raise
        point,body = [pair['runs'][k] for k in MODELS]
        assert point['physical_audit'] == body['physical_audit']
        assert point['forecast']['environment'] == body['forecast']['environment']
        assert point['forecast']['source_actions'] == body['forecast']['source_actions']
        pair['comparison'] = compare_models(case,{k:v['forecast']['report'] for k,v in pair['runs'].items()},trace)
        save()
    assert F.digest(binary) == result['binary_sha256']
    assert all(F.digest(Path(p)) == h for p,h in result['input_hashes'].items())
    result['results'] = aggregate(result['pairs'])
    result['complete'] = True
    save()
    print(json.dumps(result['results'],indent=2))


if __name__ == '__main__': main()
