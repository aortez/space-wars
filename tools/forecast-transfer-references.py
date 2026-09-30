#!/usr/bin/env python3
"""Fixed conditional-flight forecasts; preserve unknowns and physical interruptions."""
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import statistics
import struct
import subprocess

SPEC = importlib.util.spec_from_file_location('calibration', Path(__file__).with_name('calibrate-transfer-references.py'))
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)
T, F = C.T, C.F


def discovery_plan():
    return [dict(name=f'holdout{world}-asteroids{interval}',
                 seed=int.from_bytes(hashlib.sha256(f'transfer-forecast-v1:{world}'.encode()).digest()[:8], 'little'),
                 interval=interval, policies=[13, 13])
            for world in range(2) for interval in [0, 3]]


def distance(a, b):
    return math.hypot(a['x'] - b['x'], a['y'] - b['y'])


def target_ephemeris(source, sample_ticks):
    """Independent numeric reconstruction; source identity remains bit-exact.

    Match f32 phase/translation recurrence and normalized orbital directions.
    libm rounding can differ, so derived positions/velocities use explicit small
    tolerances; reported terminal geometry is recomputed from its recorded poses.
    """
    f32 = lambda x: struct.unpack('!f', struct.pack('!f', x))[0]
    dt = f32(1 / 60)
    orbit = source['orbit']
    phase = orbit['phase'] if orbit else None
    position = source['motion']['position']
    result = {0: {k: source['motion'][k] for k in ['position', 'velocity']}}
    for tick in range(1, max(sample_ticks) + 1):
        if orbit:
            phase = f32(phase + f32(orbit['rate'] * dt))
            x, y = f32(math.cos(phase)), f32(math.sin(phase))
            length = f32(math.sqrt(f32(f32(x*x) + f32(y*y))))
            direction = dict(x=f32(x / length), y=f32(y / length))
            next_position = {a: f32(orbit['center'][a] + f32(direction[a] * orbit['radius'])) for a in ['x', 'y']}
        else:
            next_position = {a: f32(position[a] + f32(source['translation_velocity'][a] * dt)) for a in ['x', 'y']}
        velocity = {a: f32(f32(next_position[a] - position[a]) / dt) for a in ['x', 'y']}
        position = next_position
        if tick in sample_ticks:
            result[tick] = dict(position=position, velocity=velocity)
    return result


def score(report, trace):
    """Never turn a censored flight or model-domain exit into a timing error."""
    outcome = trace[-1]['terminal']
    completed = report is not None and report['end'] == 'kinematic_handoff'
    comparable = completed and outcome['reason'] == 'arrived'
    samples = []
    invalid_samples = []
    if report is not None:
        for sample in report['samples']:
            after = sample['after_ticks']
            if after >= len(trace):
                continue
            actual = trace[after]
            assert actual['tick'] == trace[0]['tick'] + after
            if actual['terminal'] and actual['terminal']['reason'] != 'arrived':
                continue  # Do not score the observation containing an interruption.
            finite = all(isinstance(sample['ship'][key][axis], (int, float))
                         and math.isfinite(sample['ship'][key][axis])
                         for key in ['position', 'velocity'] for axis in ['x', 'y'])
            if not finite:
                assert report['end'] == 'non_finite' and after == report['ticks']
                invalid_samples.append(after)
                continue
            samples.append(dict(after_ticks=after,
                position_error=distance(sample['ship']['position'], actual['ship']['position']),
                velocity_error=distance(sample['ship']['velocity'], actual['ship']['velocity']),
                same_frame=sample['frame'] == actual['frame']))
    return dict(physical_outcome=outcome, model_end=report['end'] if report else 'source_unknown',
                comparable_handoff=comparable,
                time_error_seconds=(report['handoff_seconds'] - outcome['elapsed_ticks'] / 60) if comparable else None,
                sampled_errors=samples, invalid_prediction_samples=invalid_samples)


def audit_forecast(case, probe, source_control, trace, *, model='guided_transfer_forecast_v1'):
    assert model in {'guided_transfer_forecast_v1', 'guided_transfer_forecast_body_v2'}
    forecast = probe['forecast']
    assert forecast is not None
    observation = source_control['observation']
    pilot = observation['local']['combat']['recovery']['flight']['pilot']
    assert source_control['tick'] == pilot['tick'] == case['source_tick']
    assert source_control['seat'] == case['seat']
    assert forecast['source_actions'] == source_control['actions']
    assert forecast['physics_queries'] == 0
    assert all(math.isfinite(forecast[k]) and forecast[k] >= 0 for k in ['construction_ms', 'prediction_ms'])
    environment = forecast['environment']
    if environment is not None:
        assert environment['tick'] == case['source_tick']
        assert 1 <= len(environment['planets']) == len(observation['planets']) <= 8
        for index, (body, planet) in enumerate(zip(environment['planets'], observation['planets'])):
            assert planet['index'] == index
            assert T.f32_identity(body['motion']) == T.f32_identity(planet['motion'])
            assert T.f32_identity(body['radius']) == T.f32_identity(planet['radius'])
            assert math.isfinite(body['gravity_scale']) and body['gravity_scale'] >= 0
            assert math.isfinite(body['gravity_radius']) and body['gravity_radius'] >= 0
        sun = environment['sun']
        assert T.f32_identity(sun[0] if sun else None) == T.f32_identity(observation['sun'])
    report = forecast['report']
    if report is None:
        assert forecast['unknown'] and forecast['charged_graph'] == 0
        if not probe['source']['nomination']['accepted']:
            assert forecast['unknown'] == 'source nomination refused' and environment is None
        return dict(score(report, trace), unknown=forecast['unknown'], charged_graph=0,
                    construction_ms=forecast['construction_ms'], prediction_ms=forecast['prediction_ms'])
    assert probe['source']['nomination']['accepted'] and environment is not None and forecast['unknown'] is None
    assert report['model'] == model
    assert report['source_tick'] == case['source_tick'] and report['destination'] == case['destination']
    assert report['horizon_ticks'] == 3600
    ticks = report['ticks']
    assert 1 <= ticks <= report['horizon_ticks'] and ticks == report['charged_graph'] == forecast['charged_graph']
    assert report['end'] in {'kinematic_handoff', 'planet_envelope', 'sun_envelope', 'boundary_envelope',
                             'controller_interrupted', 'match_time_limit', 'horizon', 'non_finite'}
    assert sum(report[k] for k in ['launch_ticks', 'transfer_ticks', 'solar_escape_ticks']) == ticks
    assert all(0 <= report[k] <= ticks for k in ['avoidance_ticks', 'boundary_ticks', 'frame_changes'])
    phases = report['phases']
    assert 1 <= len(phases) <= 128 and phases[0]['start_tick'] == 0
    assert all(p['start_tick'] < p['end_tick'] for p in phases)
    assert all(a['end_tick'] == b['start_tick'] for a, b in zip(phases, phases[1:]))
    assert all(p['goal'] in {'launch', 'transfer', 'avoid_sun'} for p in phases)
    assert all(0 <= p['frame'] < len(environment['planets']) for p in phases)
    if report['phases_truncated']:
        assert len(phases) == 128 and phases[-1]['end_tick'] < ticks
    else:
        assert phases[-1]['end_tick'] == ticks
        for goal, key in [('launch', 'launch_ticks'), ('transfer', 'transfer_ticks'), ('avoid_sun', 'solar_escape_ticks')]:
            assert sum(p['end_tick'] - p['start_tick'] for p in phases if p['goal'] == goal) == report[key]
        assert sum(p['end_tick'] - p['start_tick'] for p in phases if p['obstacle'] is not None) == report['avoidance_ticks']
    samples = report['samples']
    expected_ticks = list(range(0, ticks, 60)) + [ticks]
    assert [s['after_ticks'] for s in samples] == expected_ticks and len(samples) <= 62
    assert T.f32_identity(samples[0]['ship']) == T.f32_identity(pilot['ship'])
    assert T.f32_identity(samples[0]['gravity']) == T.f32_identity(pilot['gravity'])
    assert samples[0]['frame'] == pilot['planet']['index']
    target = next(p for p in observation['planets'] if p['index'] == case['destination'])
    assert T.f32_identity(samples[0]['target']) == T.f32_identity(target['motion'])
    ephemeris = target_ephemeris(environment['planets'][case['destination']], set(expected_ticks))
    for sample in samples:
        body = ephemeris[sample['after_ticks']]
        assert distance(sample['target']['position'], body['position']) < .002
        assert distance(sample['target']['velocity'], body['velocity']) < .02
        if report['end'] == 'non_finite' and sample['after_ticks'] == ticks:
            continue
        for key, field in [('position', 'target_range'), ('velocity', 'target_relative_speed')]:
            assert isinstance(sample[field], (int, float)) and math.isfinite(sample[field]) and sample[field] >= 0
            assert math.isclose(sample[field], distance(sample['ship'][key], sample['target'][key]), abs_tol=.002)
    assert math.isclose(samples[0]['target_range'], distance(pilot['ship']['position'], target['motion']['position']), abs_tol=.002)
    assert math.isclose(samples[0]['target_relative_speed'], distance(pilot['ship']['velocity'], target['motion']['velocity']), abs_tol=.002)
    if report['end'] == 'kinematic_handoff':
        assert math.isclose(report['handoff_seconds'], ticks / 60, abs_tol=.00001)
        assert samples[-1]['frame'] == case['destination']
        assert samples[-1]['target_range'] < target['radius'] + 105 and samples[-1]['target_relative_speed'] < 18
        assert distance(samples[-1]['ship']['position'], samples[-1]['target']['position']) < target['radius'] + 105
        assert distance(samples[-1]['ship']['velocity'], samples[-1]['target']['velocity']) < 18
        assert report['minimum_planet_clearance'] > 0 and report['minimum_boundary_clearance'] > 0
        assert report['minimum_sun_clearance'] is None or report['minimum_sun_clearance'] > 0
    else:
        assert report['handoff_seconds'] is None
    if report['end'] == 'horizon':
        assert ticks == 3600
    return dict(score(report, trace), unknown=None, charged_graph=ticks,
                construction_ms=forecast['construction_ms'], prediction_ms=forecast['prediction_ms'],
                phases_truncated=report['phases_truncated'], phases=phases,
                phase_counts={k: report[k] for k in ['launch_ticks', 'transfer_ticks', 'solar_escape_ticks',
                                                   'avoidance_ticks', 'boundary_ticks', 'frame_changes']})


def audit_unchanged(case, before, after, before_trace_hash, after_trace_hash):
    """Forecasting must be observational for the entire replay, not just its prefix."""
    assert before_trace_hash == after_trace_hash, 'forecast changed full controller/observation trace'
    for name in ['transfer-probe.jsonl', 'mission-evaluations.jsonl', 'flag-survey.jsonl', 'flag-value-shadow.jsonl']:
        assert F.digest(before / name) == F.digest(after / name), f'forecast changed {name}'
    a, b = [json.loads((root / 'report.json').read_text()) for root in [before, after]]
    assert F.D.same_physical_outcomes(a, b), 'forecast changed physical outcome'
    assert a['missions'] == b['missions']
    assert {k: v for k, v in a['transfer_probe'].items() if k not in {'forecast', 'forecast_schedule'}} == {
           k: v for k, v in b['transfer_probe'].items() if k not in {'forecast', 'forecast_schedule'}}
    return dict(exact_controller_trace=True, exact_evaluations_and_surveys=True,
                exact_probe=True, exact_physics=True, controller_trace_sha256=after_trace_hash)


def aggregate(cases, runs):
    output = {}
    for group in ['historical', 'fresh', 'holdout']:
        values = [runs[c['name']]['forecast'] for c in cases if c['group'] == group]
        errors = [r['time_error_seconds'] for r in values if r['time_error_seconds'] is not None]
        output[group] = dict(cases=len(values), model_ends=dict(Counter(r['model_end'] for r in values)),
            physical_outcomes=dict(Counter(r['physical_outcome']['reason'] for r in values)),
            comparable_handoffs=len(errors), mean_signed_error_seconds=statistics.mean(errors) if errors else None,
            mean_absolute_error_seconds=statistics.mean(map(abs, errors)) if errors else None,
            maximum_absolute_error_seconds=max(map(abs, errors), default=None),
            charged_graph=sum(r['charged_graph'] for r in values),
            maximum_prediction_ms=max((r['prediction_ms'] for r in values), default=None))
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--shadow-reference', type=Path, required=True)
    parser.add_argument('--calibration', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    opts.out.mkdir(parents=True, exist_ok=False)
    previous = json.loads((opts.calibration / 'summary.json').read_text())
    old_cases = previous['historical_plan'] + previous['fresh_plan']
    assert len(old_cases) == 17
    result = dict(schema=1, previous_plan=old_cases, discovery_plan=discovery_plan(), holdout_plan=None,
        discovery={}, normal_regression={}, runs={},
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()),
        binary_sha256=F.digest(opts.binary), calibration_summary_sha256=F.digest(opts.calibration / 'summary.json'),
        scope='Fixed observational forecast, no fitted constants or live admission. All17 prior controlled cases plus first eligible alternatives per seat in4 fixed holdout conditions. All31 ordinary regressions. Exact forecast on/off control, evaluation and physics parity. Timing errors only for paired completed kinematic/physical handoffs; interruptions and unknowns retained. Correlated engineering cases, not strength or Pi performance trials.')
    save = lambda: F.D.write(opts.out / 'summary.json', result)
    save()
    normal_root = opts.out / 'normal-regression'
    with (opts.out / 'normal-regression.log').open('w') as log:
        subprocess.run(['python3', str(Path(__file__).with_name('probe-transfer-references.py')), '--reference', str(opts.shadow_reference),
                        '--out', str(normal_root), '--binary', str(opts.binary)], check=True, stdout=log, stderr=log)
    normal = json.loads((normal_root / 'summary.json').read_text())
    old_normal = json.loads((opts.calibration / 'normal-regression' / 'summary.json').read_text())
    assert normal['plan'] == old_normal['plan']
    for case in normal['plan']:
        name = case['name']
        for key in ['trace_sha256', 'probe_trace_sha256', 'probe']:
            assert normal['runs'][name][key] == old_normal['runs'][name][key]
    result['normal_regression'] = dict(summary_sha256=F.digest(normal_root / 'summary.json'), cases=31,
                                       exact_controller_traces=True, exact_probe_traces=True, exact_outcomes_and_diagnostics=True)
    save()
    discovery_root = opts.out / 'discovery'
    discovery_root.mkdir()
    discoveries = []
    for condition in result['discovery_plan']:
        args = T.probe_args(dict(condition=condition, seat=0, source_tick=0, destination=0))[:-6]
        run = F.D.run(opts.binary, discovery_root, condition['name'], args + ['--sample-transfer-sources', 'true'], 0, seconds=180)
        root = discovery_root / condition['name']
        sources = T.rows(root / 'transfer-sources.jsonl')
        report = json.loads((root / 'report.json').read_text())
        assert [any(s['seat'] == seat for s in sources) for seat in [0, 1]] == report['transfer_sources']['found']
        run.update(sources=sources, found=report['transfer_sources']['found'], trace_sha256=C.archive(root),
                   sources_sha256=F.digest(root / 'transfer-sources.jsonl'))
        result['discovery'][condition['name']] = run
        discoveries.append((condition, sources))
        save()
    holdouts = [dict(c, group='holdout') for c in C.fresh_cases(discoveries)]
    result['holdout_plan'] = holdouts
    save()  # Complete discovered plan before running forecasts or counterpart flights.
    expected = T.reference_prefixes(opts.shadow_reference, previous['historical_plan'])
    expected.update(T.reference_prefixes(opts.calibration / 'discovery', previous['fresh_plan']))
    expected.update(T.reference_prefixes(discovery_root, holdouts))
    for case in old_cases + holdouts:
        name = case['name']
        if case['group'] == 'holdout':
            off_name = name + '-off'
            off = F.D.run(opts.binary, opts.out, off_name, T.probe_args(case) + ['--probe-transfer-pursuit', 'defer_new'],
                          case['seat'], seconds=case['source_tick']//60 + 61)
            before = opts.out / off_name
            off_probe = json.loads((before / 'report.json').read_text())['transfer_probe']
            off['probe'] = T.audit_probe(case, off_probe, T.rows(before / 'transfer-probe.jsonl'))
            off['parity'] = T.audit_prefix(before, case, expected[(case['condition']['name'], case['source_tick'])], off_probe)
            off['trace_sha256'] = C.archive(before)
        else:
            before = opts.calibration / (name + '-defer_new')
            off = previous['branches'][name + '-defer_new']
        run = F.D.run(opts.binary, opts.out, name, T.probe_args(case) + ['--probe-transfer-pursuit', 'defer_new', '--forecast-transfer', 'true'],
                      case['seat'], seconds=case['source_tick']//60 + 61)
        root = opts.out / name
        probe = json.loads((root / 'report.json').read_text())['transfer_probe']
        trace = T.rows(root / 'transfer-probe.jsonl')
        run.update(probe=T.audit_probe(case, probe, trace),
            parity=T.audit_prefix(root, case, expected[(case['condition']['name'], case['source_tick'])], probe),
            trace_sha256=C.archive(root), probe_trace_sha256=F.digest(root / 'transfer-probe.jsonl'))
        run['reconciled_probe_rows'] = C.audit_controls(case, root, trace)
        run['unchanged'] = audit_unchanged(case, before, root, off['trace_sha256'], run['trace_sha256'])
        source = next(r for r in C.read_trace(root, case['source_tick'], case['source_tick']) if r['seat'] == case['seat'])
        run['forecast'] = audit_forecast(case, probe, source, trace)
        if case['group'] == 'holdout':
            run['off'] = off
        result['runs'][name] = run
        save()
        print(name, run['forecast']['model_end'], run['forecast']['physical_outcome'], run['forecast']['time_error_seconds'], flush=True)
    assert F.digest(opts.binary) == result['binary_sha256']
    result['results'] = aggregate(old_cases + holdouts, result['runs'])
    save()


if __name__ == '__main__':
    main()
