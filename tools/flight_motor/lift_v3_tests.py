"""The lifted v2 stack loads under the v3 contracts and issues the v2 commands with a free sense."""
import json
from pathlib import Path
import tempfile
import unittest

from tools.flight_ppo.bridge import Bridge
from tools.flight_navigation import reference
from . import lift_v3, teacher_v2

FIXTURES = Path(__file__).resolve().parents[2] / 'crates/spacewars-ai/fixtures/learned-flight-stack-v2'


class LiftTests(unittest.TestCase):
    def test_the_lifted_fixtures_load_as_a_v3_stack_with_a_free_sense(self):
        motor = json.loads((FIXTURES / 'motor.json').read_text()); navigation = json.loads((FIXTURES / 'navigation.json').read_text())
        with Bridge(teacher_v2.DEFAULT_BRIDGE) as bridge:
            lifted_motor = lift_v3.lift_motor(motor, bridge.contract['schema_v3'])
        self.assertEqual(lifted_motor['format_version'], 5); self.assertEqual(lifted_motor['weights']['actor1.weight']['shape'], [64, 22])
        self.assertEqual(lifted_motor['weights']['actor1.weight']['values'][18:22], [0.] * 4)
        self.assertEqual(lifted_motor['weights']['actor1.weight']['values'][:18], motor['weights']['actor1.weight']['values'][:18])
        lifted_navigation = lift_v3.lift_navigation(navigation)
        self.assertEqual(lifted_navigation['format_version'], 6); self.assertEqual(lifted_navigation['weights']['actor.weight']['shape'], [4, 64])
        self.assertEqual(lifted_navigation['weights']['log_std']['shape'], [4])
        with tempfile.TemporaryDirectory() as directory, Bridge(reference.DEFAULT_BRIDGE) as bridge:
            motor_path = Path(directory) / 'motor.json'; navigation_path = Path(directory) / 'navigation.json'
            motor_path.write_text(json.dumps(lifted_motor)); navigation_path.write_text(json.dumps(lifted_navigation))
            bridge.call('load_motor_policy', path=str(motor_path))
            self.assertEqual(bridge.call('load_navigation_policy', path=str(navigation_path))['contract'], 3)
            inputs = {'self_goal': [0., 1. / 3.] + [0.] * 33, 'cells': [[0., 0.]] * 1472, 'rays': [[0., 0., 0., 0.]] * 128,
                      'nearest': [0.] * 6, 'objects': [[0.] * 16] * 32}
            output = bridge.call('infer_navigation', inputs=[inputs])['outputs'][0]
            self.assertEqual(output['sense_latent'], 0.); self.assertNotIn('advisory', output['command'])
            with self.assertRaises(RuntimeError):
                bridge.call('load_motor_policy', path=str(FIXTURES / 'motor.json'))
        with self.assertRaises(ValueError):
            lift_v3.lift_navigation(lifted_navigation)


if __name__ == '__main__':
    unittest.main()
