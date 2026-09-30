import copy
import importlib.util
import math
from pathlib import Path
import unittest


def module(name,path):
    spec = importlib.util.spec_from_file_location(name,path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


B = module('body_motion',Path(__file__).parents[1]/'compare-transfer-body-motion.py')
F = module('forecast_fixture',Path(__file__).with_name('test_transfer_forecast.py'))


def fixture():
    case,probe,_,trace = F.fixture()
    reports = {label:copy.deepcopy(probe['forecast']['report']) for label in B.MODELS}
    for label,report in reports.items(): report['model'] = B.MODELS[label]
    for row in trace:
        row['target'] = case['destination']
        row['planets'] = [dict(index=case['destination'],motion=reports['point']['samples'][0]['target'])]
    return case,reports,trace


class BodyMotionComparison(unittest.TestCase):
    def test_common_time_samples_and_different_time_endpoints_are_separate(self):
        args = fixture()
        result = B.compare_models(*args)
        self.assertEqual([s['after_ticks'] for s in result['common_samples']],[0,60,120])
        for model in result['models'].values():
            self.assertEqual(model['endpoint']['predicted_minus_actual_ticks'],-60)
            self.assertEqual(model['common_sample_metrics']['rms_position'],3.)
        self.assertEqual(result['last_common_sample']['point']['position'],3.)

    def test_interruption_censors_terminal_sample_and_timing(self):
        case,reports,trace = fixture()
        trace = trace[:61]
        trace[-1]['terminal'] = dict(tick=70,elapsed_ticks=60,reason='solver_or_debris_contact')
        result = B.compare_models(case,reports,trace)
        self.assertEqual([s['after_ticks'] for s in result['common_samples']],[0])
        self.assertTrue(all(v['endpoint'] is None for v in result['models'].values()))

    def test_model_clock_and_target_corruption_are_rejected(self):
        for change in ['model','source','target','ephemeris','gap','duplicate','nonfinite']:
            case,reports,trace = fixture()
            if change == 'model': reports['body']['model'] = B.MODELS['point']
            elif change == 'source': reports['body']['source_tick'] += 1
            elif change == 'target': trace[1]['target'] += 1
            elif change == 'ephemeris': reports['body']['samples'][1]['target']['angle'] += .1
            elif change == 'gap': trace.pop(1)
            elif change == 'duplicate': reports['body']['samples'].append(reports['body']['samples'][0])
            else: reports['body']['samples'][1]['ship']['angle'] = float('nan')
            with self.subTest(change=change),self.assertRaises(AssertionError):
                B.compare_models(case,reports,trace)

    def test_unknown_forecast_does_not_manufacture_pair_or_time(self):
        case,reports,trace = fixture()
        reports['body'] = None
        result = B.compare_models(case,reports,trace)
        self.assertEqual(result['common_samples'],[])
        self.assertIsNone(result['models']['body']['endpoint'])

    def test_declared_nonfinite_terminal_stays_an_outcome_not_an_error_sample(self):
        case,reports,trace = fixture()
        report = reports['body']
        report.update(end='non_finite',handoff_seconds=None)
        report['samples'][-1]['ship']['position']['x'] = None
        result = B.compare_models(case,reports,trace)
        self.assertEqual(result['models']['body']['invalid_prediction_samples'],[120])
        self.assertIsNone(result['models']['body']['endpoint'])
        self.assertEqual([s['after_ticks'] for s in result['common_samples']],[0,60])
        report['samples'][0]['ship']['position']['x'] = None
        with self.assertRaises(AssertionError): B.compare_models(case,reports,trace)

    def test_late_predicted_samples_do_not_score_post_handoff_ship(self):
        case,reports,trace = fixture()
        trace = trace[:61]
        trace[-1]['terminal'] = dict(tick=70,elapsed_ticks=60,reason='arrived')
        result = B.compare_models(case,reports,trace)
        self.assertEqual([s['after_ticks'] for s in result['common_samples']],[0,60])
        self.assertEqual(result['models']['body']['endpoint']['predicted_minus_actual_ticks'],60)

    def test_radial_bearing_uses_position_in_planet_frame_not_nose(self):
        ship = dict(position=dict(x=10.,y=25.),angle=math.pi)
        planet = dict(position=dict(x=10.,y=5.),angle=math.pi/2)
        bearing = B.radial_bearing(ship,planet)
        self.assertEqual(bearing['nearest_bin'],48)
        ship['angle'] = 0.
        self.assertEqual(B.radial_bearing(ship,planet),bearing)
        ship['position'] = planet['position']
        self.assertIsNone(B.radial_bearing(ship,planet))


if __name__ == '__main__': unittest.main()
