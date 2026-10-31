"""Retention continuation, breach memory, immutable legacy prefixes and response scope."""
import copy
import math
import unittest
from tools.flight_ppo.bridge import Bridge
from tools.flight_motor.tests import BRIDGE
from tools.flight_motor.task import candidates
from tools.flight_audit.motor_retention import inspect,prefix_matches,heading_input,seed_gate


class MotorRetentionTests(unittest.TestCase):
    def task(self,index=24):
        t=copy.deepcopy(candidates(5310001)[index]['task']);t['retention_horizon_ticks']=900;return t

    def test_legacy_prefix_is_exact_and_retention_has_no_early_terminal(self):
        with Bridge(BRIDGE) as b:
            old=self.task();del old['retention_horizon_ticks'];b.call('reset',tasks=[old])
            for _ in range(30):b.call('step',actions=[{'steering':0.,'thrust':0.,'brake':0.}])
            parent=b.call('state')['traces'][0]
            b.call('reset',tasks=[self.task()])
            for _ in range(900):item=b.call('step',actions=[{'steering':0.,'thrust':0.,'brake':0.}])['items'][0]
            trace=b.call('state')['traces'][0];prefix_matches(trace,parent);result=inspect(trace)
            self.assertTrue(result['retained']);self.assertEqual(result['first_success_tick'],30)
            self.assertEqual(result['post_acquisition_ticks'],870)
            prefix=copy.deepcopy(trace);prefix['steps']=prefix['steps'][:450]
            b.call('restore',traces=[prefix])
            for _ in range(450):b.call('step',actions=[{'steering':0.,'thrust':0.,'brake':0.}])
            self.assertEqual(b.call('state')['traces'][0],trace)
            corrupt=copy.deepcopy(trace);corrupt['steps'][0]['item']['inputs'][2]=.1
            with self.assertRaises(AssertionError):prefix_matches(corrupt,parent)

    def test_breach_is_not_erased_by_later_recovery(self):
        with Bridge(BRIDGE) as b:
            b.call('reset',tasks=[self.task()])
            for tick in range(900):
                action={'steering':0.,'thrust':1. if 30<=tick<35 else 0.,'brake':1. if tick>=35 else 0.}
                b.call('step',actions=[action])
            trace=b.call('state')['traces'][0];r=inspect(trace)
            self.assertFalse(r['retained']);self.assertIsNotNone(r['first_exit_tick'])
            self.assertLess(r['final_errors']['velocity'],1.)
            corrupt=copy.deepcopy(trace);corrupt['steps'][-1]['item']['retention']['first_exit_tick']=None
            with self.assertRaises(AssertionError):inspect(corrupt)

    def test_late_acquisition_does_not_pass_original_deadline(self):
        with Bridge(BRIDGE) as b:
            b.call('reset',tasks=[self.task()])
            for tick in range(900):
                b.call('step',actions=[{'steering':0.,'thrust':.2 if tick<400 else 0.,'brake':1.}])
            result=inspect(b.call('state')['traces'][0])
            self.assertGreater(result['first_success_tick'],360)
            self.assertIsNone(result['first_exit_tick'])
            self.assertFalse(result['acquired_on_time']);self.assertFalse(result['retained'])
            self.assertLess(result['final_errors']['velocity'],1.)

    def test_invalid_horizons_rejected_without_resetting_active_world(self):
        with Bridge(BRIDGE) as b:
            b.call('reset',tasks=[self.task()]);before=b.call('state')
            for horizon in [0,360,1801]:
                t=self.task();t['retention_horizon_ticks']=horizon
                with self.assertRaises(RuntimeError):b.call('reset',tasks=[t])
                self.assertEqual(b.call('state'),before)

    def test_heading_probe_changes_only_command_direction(self):
        x=list(range(18));saved=list(x)
        for d in range(-45,46):
            changed=heading_input(x,d)
            self.assertEqual(x,saved);self.assertEqual(changed[2:],saved[2:])
            self.assertAlmostEqual(changed[0]**2+changed[1]**2,1.,places=6)
            self.assertAlmostEqual(math.degrees(math.atan2(*changed[:2])),d,places=5)

    def records(self):
        return [{'family':'turn' if i<24 else 'hold','index':i,'direction':'left' if i%2==0 else 'right','retained':True,
                 'first_success_tick':60 if i<24 else 30,'outcome':'success','parent_success':True} for i in range(32)]

    def test_gate_cannot_hide_lost_parent_successes_or_vacuous_failed_seed(self):
        rows=self.records();self.assertTrue(seed_gate(rows)['passed'])
        rows[0].update(retained=False,outcome='deadline');self.assertFalse(seed_gate(rows)['passed'])
        self.assertEqual(seed_gate(rows)['lost_parent_successes'],[0])
        rows=self.records()
        for r in rows:r.update(parent_success=False,retained=False,outcome='deadline',first_success_tick=None)
        self.assertFalse(seed_gate(rows)['passed'])


if __name__=='__main__':unittest.main(verbosity=2)
