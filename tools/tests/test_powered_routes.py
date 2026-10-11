import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('powered_routes', Path(__file__).parents[1]/'validate-powered-routes.py')
M = importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)


class PoweredRouteTests(unittest.TestCase):
    def test_plan_changes_only_the_powered_route_flag(self):
        self.assertEqual(len(M.P.plan()), 14)
        self.assertEqual(sum(i['powered'] for i in M.P.plan()), 7)
        for item in M.P.plan():
            self.assertEqual(M.arguments(item, False), M.B.arguments(item, True))
            self.assertEqual(M.arguments(item, True), M.arguments(item, False) + ['--powered-objective-routes', 'true'])

    def test_powered_publications_keep_one_source_epoch_and_window(self):
        row = dict(seat=0, pilot=dict(tick=180, owner='player_1', planet=dict(index=0)),
            objective_work='ready', objective_evidence=dict(generation=7, measurement_tick=100, publication=dict(decision='validated_routes')),
            landing_objective=dict(planning='jetpack_round_trip', actor='player_1', tick=100, validated_tick=180,
                objective=dict(planet=0, revision=0), actual=None, sites=[dict(site=dict(planet=0, bearing=2),
                    endpoint=dict(id=12), outbound=dict(failure=None, flights=1), returning=dict(failure=None, flights=1),
                    crossing=dict(measured_tick=100, launch_until_tick=220, plan=dict(planet=0, revision=0)))]))
        self.assertEqual(M.audit_powered([row, row]), [row])
        for mutate in [
            lambda r: r['landing_objective']['sites'][0]['crossing'].update(measured_tick=101),
            lambda r: r['landing_objective']['sites'][0]['crossing'].update(launch_until_tick=221),
            lambda r: r['objective_evidence'].update(measurement_tick=101),
            lambda r: r['pilot'].update(tick=221),
            lambda r: r['landing_objective']['sites'][0]['outbound'].update(failure='disconnected'),
            lambda r: r['landing_objective']['sites'][0].update(endpoint=None),
            lambda r: r['landing_objective']['sites'][0].update(returning=None),
            lambda r: r['landing_objective'].update(planning='joint_round_trip'),
        ]:
            invalid = copy.deepcopy(row)
            mutate(invalid)
            with self.assertRaises(AssertionError):
                M.audit_powered([invalid])


if __name__ == '__main__':
    unittest.main()
