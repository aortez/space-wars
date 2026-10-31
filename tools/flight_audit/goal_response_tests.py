"""Synthetic consistency and interpretation checks; no physics or trained inference."""
import copy
import math
import unittest
from .goal_response import f32, grid, intervene, forward_acceleration, summarize


def inputs():
    own=[0.]*35
    own[:8]=[.02,.04,-.1,-.2,.1,.2,.12,.17]
    own[16]=1.;own[19]=40/45;own[20]=1.;own[21]=1.
    present=[0.]*16;present[0]=1.;present[7:9]=[.3,.4]
    return {'self_goal':own,'cells':[[.1,.2]],'rays':[[1.,.5,.3,.4],[0.,0.,0.,0.]],
            'nearest':[1.,.1,.2,.3,.3,.4],'objects':[present,[0.]*16]}


class GoalResponseTests(unittest.TestCase):
    def test_grid_is_complete_unique_and_bounded(self):
        cases=grid();self.assertEqual(len(cases),200)
        self.assertEqual(len({(tuple(c['goal']),c['forward_speed']) for c in cases}),200)
        self.assertEqual(sum(c['direction']=='origin' for c in cases),8)
        self.assertEqual(max(math.hypot(*c['goal']) for c in cases),60)

    def test_velocity_shift_updates_every_relative_representation_and_preserves_geometry(self):
        before=inputs();saved=copy.deepcopy(before);out=intervene(before,[3.,6.],[0.,140.])
        self.assertEqual(before,saved);self.assertEqual(out['cells'],before['cells'])
        own=out['self_goal'];self.assertEqual(own[:6],[f32(.01),f32(.02),-0.,-2.,0.,2.])
        self.assertAlmostEqual(own[6]-own[4],.02,places=6)
        self.assertAlmostEqual(own[7]-own[5],-.03,places=6)
        self.assertEqual(own[8:],before['self_goal'][8:])
        for actual in [out['rays'][0][2:],out['nearest'][4:],out['objects'][0][7:9]]:
            self.assertAlmostEqual(actual[0],.4,places=6);self.assertAlmostEqual(actual[1],-1.4,places=6)
        self.assertEqual(out['rays'][0][:2],before['rays'][0][:2])
        self.assertEqual(out['rays'][1],before['rays'][1]);self.assertEqual(out['objects'][1],before['objects'][1])
        self.assertEqual(out['nearest'][:4],before['nearest'][:4])
        self.assertEqual(out['objects'][0][:7]+out['objects'][0][9:],before['objects'][0][:7]+before['objects'][0][9:])

    def test_goal_only_shift_and_identity(self):
        original=inputs();out=intervene(original,[-12.,0.],[7.,14.])
        self.assertEqual(out['self_goal'][:2],[f32(-.04),0.])
        for a,b in zip(out['self_goal'][2:],original['self_goal'][2:]):self.assertAlmostEqual(a,b,places=6)
        identity=intervene(original,[6.,12.],[7.,14.])
        for a,b in zip(identity['self_goal'],original['self_goal']):self.assertAlmostEqual(a,b,places=6)
        self.assertEqual(identity['cells'],original['cells'])

    def test_absent_relative_payloads_and_stationary_target_guard(self):
        original=inputs();original['nearest']=[0.]*6
        out=intervene(original,[0.,0.],[0.,1.]);self.assertEqual(out['nearest'],[0.]*6)
        original['self_goal'][2]=.1
        with self.assertRaises(AssertionError):intervene(original,[0.,0.],[0.,1.])
        with self.assertRaises(ValueError):intervene(inputs(),[float('nan'),0.],[0.,1.])

    def test_force_diagnostic_uses_origin_velocity_cap_governor_and_frame(self):
        original=inputs();own=original['self_goal'];own[4:8]=[0.,1/70,0.,2/70]
        command={'steering':0.,'thrust':.1,'brake':.6}
        self.assertAlmostEqual(forward_acceleration(original,command),4.5-4*.6*2)
        own[14]=1/70
        self.assertAlmostEqual(forward_acceleration(original,command),4.5-4*.6)
        own[7]=20/70;own[14]=0
        self.assertAlmostEqual(forward_acceleration(original,command),4.5-40*.6)
        own[7]=70/70
        self.assertAlmostEqual(forward_acceleration(original,command),-40*.6)

    def test_constant_commands_do_not_become_learned_velocity_feedback(self):
        original=inputs();original['self_goal'][:8]=[0.]*8
        command={'steering':0.,'thrust':.1,'brake':.6}
        rows=[{'case':{'kind':'original'},'python_action':command}]
        for c in grid():
            x=intervene(original,c['goal'],[0.,c['forward_speed']])
            rows.append({'case':{'kind':'intervention',**c},'python_action':command,
                         'forward_actuator_acceleration':forward_acceleration(x,command)})
        summary=summarize(rows)
        self.assertFalse(summary['distance_near_minus_far']['material_throttle_or_brake'])
        self.assertFalse(summary['velocity_fast_minus_slow']['material_throttle_or_brake'])
        self.assertEqual(summary['velocity_fast_minus_slow']['delta']['brake'],0.)
        self.assertLess(summary['velocity_fast_minus_slow']['forward_acceleration_delta'],0.)
        self.assertAlmostEqual(summary['arrival']['goal_0_speed_1']['forward_acceleration'],2.1,places=6)


if __name__=='__main__':unittest.main(verbosity=2)
