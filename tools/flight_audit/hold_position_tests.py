"""Hold sampling/admission, interval semantics and scope-specific gate checks."""
import copy
import math
import unittest
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.curriculum import HOLD_RECIPE, hold_candidate, generate
from tools.flight_ppo.tests import BRIDGE
from tools.flight_audit.hold_position import aggregate, comparison, hold_metrics


class HoldTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.bridge=Bridge(BRIDGE)
    @classmethod
    def tearDownClass(cls):cls.bridge.close()

    def test_fixed_sampling_heading_coverage_and_partition_separation(self):
        for i in range(128):
            c=hold_candidate(4310001,i);t=c['task'];slot=i%4;lo=-math.pi+slot*math.pi/2
            self.assertEqual(c,hold_candidate(4310001,i));self.assertNotEqual(t,hold_candidate(4310002,i)['task'])
            self.assertEqual(t['goal_offset'],{'x':0.,'y':0.});self.assertEqual(t['start']['relative_velocity'],{'x':0.,'y':0.})
            self.assertEqual(t['start']['spin'],0);self.assertFalse(t['start']['gravity'])
            self.assertTrue(lo<=t['start']['heading_offset']<=lo+math.pi/2)
            self.assertEqual(t['arrival'],{'position':3.,'velocity':1.,'spin':.2,'dwell_ticks':30})
            self.assertEqual(t['deadline_ticks'],1800)

    def test_admission_quota_rejections_and_state_isolation(self):
        before=self.bridge.call('state');batch,pools=generate(self.bridge,4310001,2,recipe=HOLD_RECIPE)
        self.assertTrue(batch['complete']);self.assertEqual([len(p) for p in pools],[2]*4)
        self.assertEqual(before,self.bridge.call('state'))
        c=copy.deepcopy(pools[0][0]);c['task']['goal_offset']['x']=.01
        self.assertFalse(self.bridge.call('inspect',candidate=c)['admitted'])
        c=copy.deepcopy(pools[0][0]);c['task']['arrival']['dwell_ticks']=29
        self.assertFalse(self.bridge.call('inspect',candidate=c)['admitted'])
        limited,_=generate(self.bridge,4310001,2,max_rounds=1,recipe=HOLD_RECIPE)
        self.assertFalse(limited['complete'])

    def test_zero_error_still_requires_30_complete_intervals(self):
        tasks=[hold_candidate(4310099,i)['task'] for i in range(4)]
        items=self.bridge.call('reset',tasks=tasks)['items']
        for item in items:
            self.assertEqual(item['frame']['errors'],{'position':0.,'velocity':0.,'spin':0.})
            self.assertEqual(item['progress'],{'elapsed_ticks':0,'dwell_ticks':0,'outcome':None})
        for tick in range(1,31):
            items=self.bridge.call('step',actions=[{'steering':0.,'thrust':0.,'brake':0.}]*4)['items']
            for item in items:
                self.assertEqual(item['progress']['dwell_ticks'],tick)
                self.assertEqual(item['progress']['outcome'],'success' if tick==30 else None)
        for t in self.bridge.call('state')['traces']:
            m=hold_metrics(t);self.assertTrue(m['immediate_hold']);self.assertIsNone(m['first_tolerance_exit'])

    def test_thrust_leaves_tolerance_and_hold_evidence_detects_it(self):
        self.bridge.call('reset',tasks=[hold_candidate(4310099,i)['task'] for i in range(4)])
        for _ in range(30):self.bridge.call('step',actions=[{'steering':0.,'thrust':.5,'brake':.5}]*4)
        for t in self.bridge.call('state')['traces']:
            m=hold_metrics(t,True);self.assertFalse(m['immediate_hold'])
            self.assertIn('velocity',m['first_tolerance_exit']['violations']);self.assertLess(m['first_tolerance_exit']['tick'],30)
            bad=copy.deepcopy(t);bad['steps'][0]['progress']['dwell_ticks']=30
            with self.assertRaises(ValueError):hold_metrics(bad,True)

    def rows(self):
        return [{'index':i,'recipe':f'{HOLD_RECIPE}/stratum-{i%4}','outcome':'deadline','ticks':1800,'collision':False,'damage':False,'initial_errors':{'position':0.,'velocity':0.,'spin':0.}} for i in range(16)]

    def test_returning_later_is_not_immediate_holding_and_gates_need_two_seeds(self):
        before=self.rows();later=copy.deepcopy(before)
        for row in later:row.update(outcome='success',ticks=60)
        g=comparison(before,later);self.assertTrue(g['learning_evidence']);self.assertFalse(g['scoped_hold'])
        for row in later:row['ticks']=30
        g=comparison(before,later);self.assertTrue(g['scoped_hold'])
        fail=comparison(before,before)
        self.assertFalse(aggregate([{'seed':s,'mean':g if s==17 else fail} for s in [17,29,43]])['scoped_hold'])
        self.assertTrue(aggregate([{'seed':s,'mean':g if s!=43 else fail} for s in [17,29,43]])['scoped_hold'])
        later[0]['recipe']='aligned-approach-v1/stratum-0'
        with self.assertRaises(ValueError):comparison(before,later)

    def test_hold_gate_requires_heading_coverage_gain_and_no_failures(self):
        before=self.rows();after=copy.deepcopy(before)
        for row in after:
            if row['index']%4!=0:row.update(outcome='success',ticks=30)
        self.assertFalse(comparison(before,after)['scoped_hold'])
        for row in after:row.update(outcome='success',ticks=30)
        self.assertFalse(comparison(after,after)['scoped_hold'])
        after[0].update(outcome='collision',collision=True)
        self.assertFalse(comparison(before,after)['scoped_hold'])


if __name__=='__main__':unittest.main(verbosity=2)
