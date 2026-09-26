import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('forecast', Path(__file__).resolve().parents[1] / 'forecast-transfer-references.py')
F = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(F)


def fixture():
    vec = lambda x: dict(x=float(x), y=0.0)
    motion = lambda x, speed=0: dict(position=vec(x), velocity=vec(speed), angle=0.0, spin=0.0)
    ship = motion(0, 1)
    planets = [dict(index=0, motion=motion(-100), radius=10.0), dict(index=1, motion=motion(170), radius=10.0)]
    pilot = dict(tick=10, ship=ship, gravity=vec(0), planet=planets[0])
    control = dict(tick=10, seat=0, actions=['thrust'], observation=dict(planets=planets, sun=None,
        local=dict(combat=dict(recovery=dict(flight=dict(pilot=pilot))))))
    environment = dict(tick=10, sun=None, planets=[dict(motion=p['motion'], radius=p['radius'], gravity_scale=1.0, gravity_radius=10.0,
                        orbit=None, translation_velocity=vec(0)) for p in planets])
    samples = [dict(after_ticks=t, ship=motion(t + (3 if t else 0), 1), frame=int(t == 120), gravity=vec(0),
                    target=motion(170), target_range=170.0-t-(3 if t else 0), target_relative_speed=1.0) for t in [0, 60, 120]]
    report = dict(model='guided_transfer_forecast_v1', source_tick=10, destination=1, horizon_ticks=3600,
        ticks=120, charged_graph=120, end='kinematic_handoff', handoff_seconds=2.0,
        launch_ticks=60, transfer_ticks=60, solar_escape_ticks=0, avoidance_ticks=0, boundary_ticks=0, frame_changes=1,
        minimum_planet_clearance=10, minimum_sun_clearance=None, minimum_boundary_clearance=50,
        phases_truncated=False, phases=[dict(start_tick=0, end_tick=60, goal='launch', frame=0, obstacle=None),
                                       dict(start_tick=60, end_tick=120, goal='transfer', frame=0, obstacle=None)], samples=samples)
    forecast = dict(environment=environment, source_actions=['thrust'], report=report, unknown=None,
                    construction_ms=.01, prediction_ms=1., charged_graph=120, physics_queries=0)
    probe = dict(forecast=forecast, source=dict(nomination=dict(accepted=True)))
    case = dict(source_tick=10, destination=1, seat=0)
    trace = [dict(tick=10+t, ship=motion(t, 1), frame=int(t >= 120),
                  terminal=dict(tick=190, elapsed_ticks=180, reason='arrived') if t == 180 else None) for t in range(181)]
    return case, probe, control, trace


class ForecastAudit(unittest.TestCase):
    def test_scores_only_two_completed_endpoints(self):
        case, probe, control, trace = fixture()
        result = F.audit_forecast(case, probe, control, trace)
        self.assertEqual(result['time_error_seconds'], -1.0)
        self.assertEqual(result['sampled_errors'][-1]['position_error'], 3.0)
        for reason in ['solver_or_debris_contact', 'retargeted', 'ship_or_pilot_lost', 'timeout', 'match_finished']:
            changed = copy.deepcopy(trace)
            changed[-1]['terminal']['reason'] = reason
            result = F.audit_forecast(case, probe, control, changed)
            self.assertFalse(result['comparable_handoff'])
            self.assertIsNone(result['time_error_seconds'])
        probe['forecast']['report'].update(end='planet_envelope', handoff_seconds=None)
        self.assertIsNone(F.audit_forecast(case, probe, control, trace)['time_error_seconds'])

    def test_samples_stop_before_interruption_and_ignore_future_prediction(self):
        _, probe, _, trace = fixture()
        trace = trace[:61]
        trace[-1]['terminal'] = dict(tick=70, elapsed_ticks=60, reason='solver_or_debris_contact')
        result = F.score(probe['forecast']['report'], trace)
        self.assertEqual([s['after_ticks'] for s in result['sampled_errors']], [0])
        self.assertIsNone(result['time_error_seconds'])

    def test_checks_source_identity_first_command_and_work(self):
        for change in ['tick', 'actions', 'motion', 'gravity', 'work', 'phases', 'truncation', 'samples', 'fake_cost']:
            case, probe, control, trace = fixture()
            f = probe['forecast']
            r = f['report']
            if change == 'tick': f['environment']['tick'] += 1
            elif change == 'actions': f['source_actions'] = ['brake']
            elif change == 'motion': f['environment']['planets'][1]['motion'] = dict(control['observation']['planets'][1]['motion'], spin=.001)
            elif change == 'gravity': r['samples'][0]['gravity'] = dict(x=1.0, y=0.0)
            elif change == 'work': f['charged_graph'] += 1
            elif change == 'phases': r['phases'][1]['start_tick'] -= 1
            elif change == 'truncation': r['phases_truncated'] = True
            elif change == 'samples': r['samples'].pop(1)
            elif change == 'fake_cost': r['end'] = 'horizon'
            with self.subTest(change=change), self.assertRaises(AssertionError):
                F.audit_forecast(case, probe, control, trace)

    def test_refusal_and_unsupported_source_remain_unknown(self):
        for accepted in [False, True]:
            case, probe, control, trace = fixture()
            probe['source']['nomination']['accepted'] = accepted
            probe['forecast'].update(report=None, charged_graph=0,
                unknown='source unsupported' if accepted else 'source nomination refused',
                environment=probe['forecast']['environment'] if accepted else None)
            result = F.audit_forecast(case, probe, control, trace)
            self.assertEqual(result['model_end'], 'source_unknown')
            self.assertIsNone(result['time_error_seconds'])
            self.assertEqual(result['charged_graph'], 0)

    def test_terminal_claim_is_reconciled_to_motion_and_captured_ephemeris(self):
        for change in ['range', 'speed', 'negative', 'target']:
            case, probe, control, trace = fixture()
            sample = probe['forecast']['report']['samples'][-1]
            if change == 'range': sample['ship']['position']['x'] = 100000
            elif change == 'speed': sample['ship']['velocity']['x'] = 1000
            elif change == 'negative': sample['target_range'] = -1
            else: sample['target']['position']['x'] += 1
            with self.subTest(change=change), self.assertRaises(AssertionError):
                F.audit_forecast(case, probe, control, trace)

    def test_ephemeris_covers_orbit_and_translation(self):
        vec = lambda x, y: dict(x=x, y=y)
        source = dict(motion=dict(position=vec(3., 0.), velocity=vec(0., 0.)),
                      orbit=dict(phase=0., rate=.1, radius=3., center=vec(0., 0.)))
        orbit = F.target_ephemeris(source, {0, 60})[60]
        self.assertAlmostEqual(F.distance(orbit['position'], vec(0, 0)), 3, places=5)
        self.assertGreater(orbit['position']['y'], .29)
        source.update(orbit=None, translation_velocity=vec(3., 0.))
        translation = F.target_ephemeris(source, {0, 60})[60]
        self.assertAlmostEqual(translation['position']['x'], 6, places=4)
        self.assertAlmostEqual(translation['velocity']['x'], 3, places=4)

    def test_no_completions_keep_aggregate_denominator_zero(self):
        case, probe, control, trace = fixture()
        trace[-1]['terminal']['reason'] = 'solver_or_debris_contact'
        result = F.audit_forecast(case, probe, control, trace)
        summary = F.aggregate([dict(name='interrupted', group='fresh')], dict(interrupted=dict(forecast=result)))
        self.assertEqual(summary['fresh']['cases'], 1)
        self.assertEqual(summary['fresh']['comparable_handoffs'], 0)
        self.assertIsNone(summary['fresh']['mean_absolute_error_seconds'])
        self.assertEqual(summary['holdout']['cases'], 0)

    def test_nonfinite_model_exit_is_retained_without_numeric_error(self):
        case, probe, control, trace = fixture()
        report = probe['forecast']['report']
        report.update(end='non_finite', handoff_seconds=None)
        report['samples'][-1]['ship']['position']['x'] = None
        result = F.audit_forecast(case, probe, control, trace)
        self.assertEqual(result['invalid_prediction_samples'], [120])
        self.assertIsNone(result['time_error_seconds'])


if __name__ == '__main__':
    unittest.main()
