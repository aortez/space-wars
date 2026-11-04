"""Fixed recovery coverage, reward incentives and explicit policy initialization."""
import copy
import hashlib
import math
from pathlib import Path
import tempfile
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.tests import BRIDGE, equal_tree
from tools.flight_ppo.model import FlightModel
from tools.flight_ppo.initialization import initialize_policy
from tools.flight_ppo.curriculum import SETTLE_RECIPE, settle_candidate, generate
from tools.flight_ppo.reward import regulation_cost, reward
from tools.flight_audit.hold_robustness_tests import synthetic
from tools.flight_audit.settle_recover import inspect_trace, validate_active, incentive_checks

class SettleRecoverTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);cls.bridge=Bridge(BRIDGE)
    @classmethod
    def tearDownClass(cls):cls.bridge.close()

    def test_fixed_case_matrix_and_partition_separation(self):
        for seed,n in [(4510001,64),(4510002,16)]:
            cs=[settle_candidate(seed,i) for i in range(n)]
            self.assertEqual(cs,[settle_candidate(seed,i) for i in range(n)])
            for i,c in enumerate(cs):
                t=c['task'];self.assertEqual(t['arrival']['dwell_ticks'],300);self.assertEqual(t['deadline_ticks'],900)
                s=t['start'];self.assertFalse(s['gravity']);self.assertNotIn('hold_failure',t)
                if i%4 in [1,2]:
                    v=t['goal_offset'] if i%4==1 else s['relative_velocity'];angle=s['bearing']+s['heading_offset']+(i//4%4)*math.pi/2
                    delta=math.atan2(-v['x'],v['y'])-angle
                    self.assertLessEqual(abs(math.atan2(math.sin(delta),math.cos(delta))),math.pi/8+1e-12)
                if i%4==3:self.assertEqual(s['spin']>0,i//4%2==0)
        self.assertNotEqual(settle_candidate(4510001,0)['task'],settle_candidate(4510002,0)['task'])

    def test_native_admission_fixed_full_matrix_and_short_control(self):
        for seed,n in [(4510001,16),(4510002,4)]:
            batch,pools=generate(self.bridge,seed,n,recipe=SETTLE_RECIPE)
            self.assertTrue(batch['complete']);self.assertEqual(len(batch['attempts']),4*n)
            for pool in pools:
                for c in pool:
                    short=copy.deepcopy(c);short['task']['arrival']['dwell_ticks']=30
                    self.assertTrue(self.bridge.call('inspect',candidate=short)['admitted'])
        bad=settle_candidate(4510001,1);bad['task']['start']['gravity']=True
        self.assertFalse(self.bridge.call('inspect',candidate=bad)['admitted'])

    def test_rejected_cases_are_not_replaced(self):
        class Reject:
            def call(self,op,candidate):return {'candidate':candidate,'admitted':candidate['index']!=1}
        batch,pools=generate(Reject(),4510001,4,recipe=SETTLE_RECIPE)
        self.assertFalse(batch['complete']);self.assertEqual(len(batch['attempts']),16);self.assertEqual(len(pools[1]),3)

    def test_regulation_is_granular_bounded_and_zero_at_zero(self):
        zero={'errors':{'position':0.,'velocity':0.,'spin':0.}}
        self.assertEqual(regulation_cost(zero,.003),0.)
        for k,limit in [('position',3.),('velocity',1.),('spin',.2)]:
            values=[]
            for x in [0.,limit/4,limit/2,limit,limit*2,1e300]:
                f=copy.deepcopy(zero);f['errors'][k]=x;v=regulation_cost(f,.003);values.append(v)
                self.assertTrue(-.003<=v<=0)
            self.assertTrue(all(a>b for a,b in zip(values,values[1:])))
        for w in [-1.,.004,float('nan')]:
            with self.assertRaises(ValueError):regulation_cost(zero,w)
        zero['errors']['spin']=float('inf')
        with self.assertRaises(ValueError):regulation_cost(zero,.003)

    def test_zero_weight_preserves_reward_terms_exactly(self):
        f={'errors':{'position':2.,'velocity':.5,'spin':.1}}
        before={'frame':f};after={'frame':f,'progress':{'outcome':None}}
        old=reward(before,after,.9995,.25,4.)
        self.assertEqual(old,reward(before,after,.9995,.25,4.,0.));self.assertNotIn('regulation',old)
        new=reward(before,after,.9995,.25,4.,.003)
        for k in ['terminal','time','potential']:self.assertEqual(old[k],new[k])
        self.assertEqual(new['total'],old['total']+new['regulation'])

    def test_discounted_return_incentives(self):incentive_checks()

    def test_warm_start_exact_weights_rng_and_trainability(self):
        source=FlightModel(self.bridge.contract['schema']);target=FlightModel(self.bridge.contract['schema'])
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'policy.json';source.export(path);config={'warm_start_sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
            rng=torch.get_rng_state().clone();initialize_policy(target,config,path)
            self.assertTrue(torch.equal(rng,torch.get_rng_state()));self.assertTrue(equal_tree(source.state_dict(),target.state_dict()))
            self.assertTrue(all(p.requires_grad for p in target.parameters()));self.assertTrue(target.training)
            self.assertFalse(torch.optim.Adam(target.parameters()).state)
            with self.assertRaises(ValueError):initialize_policy(target,config,None)
            with self.assertRaises(ValueError):initialize_policy(target,{},path)
            with self.assertRaises(ValueError):initialize_policy(target,{'warm_start_sha256':'bad'},path)

    def test_sustained_dwell_and_nonterminal_rollout_cut(self):
        zero={'position':0.,'velocity':0.,'spin':0.}
        trace=synthetic(300,900,[zero]*300);self.assertEqual(inspect_trace(trace)['outcome'],'success')
        cut=synthetic(300,900,[zero]*256);validate_active(cut)
        with self.assertRaises(ValueError):inspect_trace(cut)
        interrupted=[dict(zero) for _ in range(900)]
        for i in range(249,900,250):interrupted[i]['velocity']=1.1
        self.assertEqual(inspect_trace(synthetic(300,900,interrupted))['outcome'],'deadline')

if __name__=='__main__':unittest.main()
