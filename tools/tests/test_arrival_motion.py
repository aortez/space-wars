import gzip
import importlib.util
import json
import math
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('arrival_motion', Path(__file__).parents[1] / 'analyze-arrival-motion.py')
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)


def row(tick, phase):
    site = dict(id=dict(planet=0, bearing=0), revision=0,
        vehicle_position=dict(x=0, y=65), normal=dict(x=0, y=1))
    pilot = dict(tick=tick, ship=dict(position=dict(x=0, y=100), velocity=dict(x=-12, y=-3)),
        planet=dict(revision=0, radius=60, motion=dict(position=dict(x=0, y=0),
            velocity=dict(x=0, y=0), spin=0.02)), sites=[site])
    capture = dict(goal=phase, site=site['id'], circling_replans=0, landing=dict(landing_retries=0))
    return dict(tick=tick-1, seat=0, mission=dict(target=0, capture=capture),
        observation=dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=pilot))))))


class ArrivalMotionTests(unittest.TestCase):
    def test_kinematics_remove_planet_rotation_at_ship_position(self):
        g = A.geometry(row(1, 'seek_cover'))
        self.assertEqual(g['tangent_speed'], 10)
        self.assertAlmostEqual(g['orbital_bearing_rate'], .1)
        self.assertEqual(g['radial_speed'], -3)
        self.assertEqual(g['signed_angle'], 0)
        self.assertEqual(g['altitude'], 40)
        self.assertEqual(g['height'], 35)

    def test_site_angle_and_lateral_error_keep_the_native_sign(self):
        r = row(1, 'seek_cover')
        p = A.pilot(r)
        p['sites'][0]['vehicle_position'] = dict(x=-65, y=0)
        self.assertAlmostEqual(A.geometry(r)['signed_angle'], math.pi/2)
        self.assertEqual(A.geometry(r)['side_error'], -65)
        p['sites'][0]['revision'] = 1
        with self.assertRaises(AssertionError):
            A.geometry(r)

    def analyze(self, rows):
        item = dict(name='case', seat=0, version=16, switch=dict(tick=1, to=0),
            visit=dict(landed_tick=3, departed_tick=4))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root/'case').mkdir()
            with gzip.open(root/'case/trace.jsonl.gz', 'wt') as stream:
                for r in rows:
                    stream.write(json.dumps(r)+'\n')
            return A.analyze(root, item)

    def test_phase_durations_use_controller_epochs_and_include_instant_approach(self):
        r = self.analyze([row(1, 'approach'), row(2, 'approach'), row(3, 'surface'), row(4, 'depart')])
        self.assertEqual(r['phase_ticks'], dict(circling=0, approach=2, landing=0, capture_and_departure=1))
        self.assertEqual(r['dense_ticks'], 4)

    def test_missing_duplicate_and_shifted_ticks_cannot_claim_dense_coverage(self):
        rows = [row(1, 'seek_cover'), row(2, 'approach'), row(3, 'surface'), row(4, 'depart')]
        for bad in [rows[1:], rows+[rows[0]], [{**rows[0], 'tick':1}, *rows[1:]]]:
            with self.assertRaises(AssertionError):
                self.analyze(bad)

    def test_source_geometry_binds_the_published_evidence_epoch_and_files(self):
        site = dict(planet=0, bearing=0)
        motion = dict(position=dict(x=0, y=0), angle=0)
        measurement = dict(tick=1, revision=0, planet=motion,
            site=dict(id=site, revision=0, vehicle_position=dict(x=0, y=65)))
        source = dict(actor='player_1', tick=2, ship_position=dict(x=0, y=100),
            evidence=dict(candidates=[dict(id=site, measurement=measurement)]),
            planets=[dict(index=0, revision=0, motion={**motion, 'angle':.1})])
        candidate = dict(planet=0, site=site, revision=0, evidence_tick=1, total_seconds=10)
        evaluation = dict(actor='player_1', source_tick=2, candidates=[candidate])
        item = dict(name='case', seat=0, switch=dict(to=0, source_tick=2))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root/'case').mkdir()
            inputs = root/'case/neutral-approach-inputs.jsonl.gz'
            with gzip.open(inputs, 'wt') as stream:
                stream.write(json.dumps(source)+'\n')
            evaluations = root/'case/mission-evaluations.jsonl'
            evaluations.write_text(json.dumps(evaluation)+'\n')
            result = A.source_geometry(root, item, dict(site=site, tick=3))
            self.assertAlmostEqual(result['signed_angle'], .1)
            self.assertEqual(result['neutral_inputs_sha256'], A.digest(inputs))
            self.assertEqual(result['evaluation_stream_sha256'], A.digest(evaluations))
            for mutation in [dict(evidence_tick=0), dict(revision=1),
                             dict(site=dict(planet=0, bearing=1)), dict(total_seconds=None)]:
                evaluations.write_text(json.dumps({**evaluation,
                    'candidates':[{**candidate, **mutation}]})+'\n')
                with self.assertRaises(AssertionError):
                    A.source_geometry(root, item, dict(site=site, tick=3))


if __name__ == '__main__':
    unittest.main()
