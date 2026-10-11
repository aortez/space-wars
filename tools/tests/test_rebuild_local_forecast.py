import copy
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('local_forecast',Path(__file__).parents[1]/'probe-rebuild-local-forecast.py')
P=importlib.util.module_from_spec(spec);spec.loader.exec_module(P)


def fixture():
    rows=[]
    motion=dict(position=dict(x=1,y=2),velocity=dict(x=0,y=0),angle=0,spin=0)
    for step in range(121):
        landing=dict(phase='landed' if step>=90 else 'flying',supported_feet=2 if step>=60 else 0,
            settled_seconds=.25 if step>=90 else 0)
        foot=dict(count=1 if step>=60 else 0,contacts=[dict(point=1)] if step>=60 else [])
        rows.append(dict(tick=100+step,pilot=dict(ship=copy.deepcopy(motion),planet=dict(motion=copy.deepcopy(motion),revision=1),
            ship_form='ship',ship_available=True,ship_health=100,landing=landing),
            landing_diagnostics=dict(hull=dict(count=0,contacts=[]),feet=[foot,copy.deepcopy(foot)])))
    full=dict(tick=100,steps=120,report=dict(tick=100,selected_offset=-8),samples=rows,settles_within_horizon=True)
    probe=dict(tick=100,seat=1,report=copy.deepcopy(full['report']),read_only=True,production_qualified=False,
        future_actions_read=False,scope='selected_planet_geometry_and_gravity_ephemeris',action_policy='unoccupied_replacement_neutral',
        terrain_updates=False,other_actors=False,damage_and_hazards=False,horizon_ticks=120,step_nanoseconds=16666667,
        max_steps_per_call=4,max_planets=32,max_planet_colliders=128,max_shape_parts=16384,max_local_travel=16,
        input=dict(captured_before_construction=True,live_vehicle_available=False,bodies=2,colliders=20,planet_count=5),
        samples=copy.deepcopy([P.reference(r) for r in rows]),steps=120,stop='horizon',first_settled_tick=190,settles_within_horizon=True,
        chunks=[dict(first_step=i+1,steps=4,elapsed_ms=.02) for i in range(0,120,4)],completed_at_tick=129,
        timing=dict(setup_ms=.1,physics_steps_ms=.4,diagnostics_ms=.2))
    return probe,full,copy.deepcopy(rows)


class LocalForecastTest(unittest.TestCase):
    def test_commands_preserve_controls_and_restore_old_timed_reference_separately(self):
        prior={'commands':{n:['old','--out','old','--rebuild-support-alignment','false',
            '--rebuild-round-foot-probe','true','--rebuild-native-forecast','true','--rebuild-replay-end','29421'] for n in P.MODES}}
        for name in P.MODES:
            cmd=P.command(prior,Path('/binary'),Path('/output'),name)
            self.assertEqual(cmd[:3],['/binary','--out','/output'])
            self.assertNotIn('--rebuild-native-forecast',cmd)
            self.assertEqual(cmd[3:-2],prior['commands'][name][3:7]+prior['commands'][name][9:])
            self.assertEqual(cmd[-2:],['--rebuild-local-forecast','true'])

    def test_scope_failure_or_partial_work_is_inconclusive_even_after_settling(self):
        self.assertFalse(P.prediction(None,120,'horizon'))
        self.assertTrue(P.prediction(110,120,'horizon'))
        for first in (None,110):
            for steps in (0,119,120):
                self.assertIsNone(P.prediction(first,steps,'outside_model_scope'))
            self.assertIsNone(P.prediction(first,119,'horizon'))

    def test_audit_records_motion_and_classification_disagreements(self):
        probe,full,actual=fixture()
        result=P.analyze(probe,full,actual)
        self.assertTrue(result['matches_observed_settling'])
        self.assertEqual(result['actual']['max_position_error'],0)
        self.assertEqual(result['actual']['first_differences'],{})
        probe['samples'][10]['ship']=copy.deepcopy(probe['samples'][10]['ship'])
        probe['samples'][10]['ship']['position']['x']+=.5
        for row in probe['samples']:
            row['landing']=dict(row['landing'],phase='flying',settled_seconds=0)
            row['settled']=False
        probe.update(first_settled_tick=None,settles_within_horizon=False)
        result=P.analyze(probe,full,actual)
        self.assertFalse(result['matches_full_world_settling'])
        self.assertFalse(result['matches_observed_settling'])
        self.assertEqual(result['actual']['max_position_error'],.5)
        self.assertEqual(result['actual']['first_differences']['ship'],110)

    def test_audit_rejects_postbuild_inputs_excess_work_and_tampered_initial_pose(self):
        for mutate in (lambda p:p['input'].update(live_vehicle_available=True),
                       lambda p:p['chunks'][0].update(steps=5),
                       lambda p:p['samples'][0]['ship']['position'].update(x=99),
                       lambda p:p.update(completed_at_tick=128),
                       lambda p:p.update(other_actors=True)):
            probe,full,actual=fixture();mutate(probe)
            with self.assertRaises(AssertionError):P.analyze(probe,full,actual)

    def test_reaudit_cannot_change_model_or_plan(self):
        old={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(old,dict(old,**{p:'repair'}),True)
        for p in set(old)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(old,dict(old,**{p:'repair'}),True)


if __name__=='__main__':unittest.main()
