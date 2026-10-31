"""Delayed credit, terminal isolation, collector invariants and exact restart."""
import json
from pathlib import Path
import unittest
import torch
from . import tests
from .bridge import Bridge
from .collector import Collector
from .curriculum import generate
from .model import FlightModel
from .ppo import gae


class CreditTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        cls.bridge = Bridge(tests.BRIDGE)
        cls.pools = generate(cls.bridge, 4110001, 1)[1]

    @classmethod
    def tearDownClass(cls):
        cls.bridge.close()

    def config(self, lam):
        config=json.loads((Path(__file__).parent/'clear-control-config.json').read_text())
        config.update(updates=2,rollout_ticks=8,epochs=2,minibatch=16,
                      train_tasks_per_stratum=1,velocity_shaping_weight=.25,
                      exploration={'kind':'continuous-gaussian-ar1-v2','rho':0.})
        config['lambda']=lam
        return config

    def test_delayed_signal_matches_declared_decay_at_60hz(self):
        gamma=.9995
        for delay in [60,180,240]:
            rewards=torch.zeros(256,1,dtype=torch.float64);rewards[delay]=1.
            for lam in [.98,1.]:
                advantages,_=gae(rewards,torch.zeros_like(rewards),torch.zeros_like(rewards),torch.zeros(1,dtype=torch.float64),gamma,lam)
                self.assertAlmostEqual(float(advantages[0]),(gamma*lam)**delay,places=13)
                self.assertEqual(float(advantages[delay]),1.)

    def test_lambda_one_discounted_returns_and_no_cross_episode_credit(self):
        rewards=torch.tensor([[1.,2.],[3.,4.],[5.,6.],[7.,8.]],dtype=torch.float64)
        values=torch.tensor([[9.,8.],[7.,6.],[5.,4.],[3.,2.]],dtype=torch.float64)
        dones=torch.tensor([[0.,0.],[1.,0.],[0.,0.],[0.,1.]],dtype=torch.float64)
        final=torch.tensor([11.,1000.],dtype=torch.float64);gamma=.9
        expected=torch.tensor([[1+.9*3,2+.9*4+.9**2*6+.9**3*8],
            [3,4+.9*6+.9**2*8],[5+.9*7+.9**2*11,6+.9*8],[7+.9*11,8]],dtype=torch.float64)
        advantages,returns=gae(rewards,dones,values,final,gamma,1.)
        torch.testing.assert_close(returns,expected,atol=1e-13,rtol=1e-13)
        torch.testing.assert_close(advantages,expected-values,atol=1e-13,rtol=1e-13)
        changed=rewards.clone();changed[2:,0]+=500
        other,_=gae(changed,dones,values,final,gamma,1.)
        self.assertTrue(torch.equal(advantages[:2,0],other[:2,0]))

    def test_collector_changes_advantages_but_not_first_rollout_rewards_or_actions(self):
        results=[]
        for lam in [.98,1.]:
            torch.manual_seed(17)
            collector=Collector(self.bridge,self.pools)
            model=FlightModel(self.bridge.contract['schema'])
            rollout=collector.collect(model,self.config(lam))
            results.append((rollout,collector.state()))
        a,b=results
        self.assertTrue(tests.equal_tree(a[1],b[1]))
        for key in a[0]:
            if key not in ('advantages','returns'):
                self.assertTrue(torch.equal(a[0][key],b[0][key]),key)
        self.assertFalse(torch.equal(a[0]['advantages'],b[0]['advantages']))

    def test_long_credit_resumes_exactly_in_fresh_process(self):
        tests.PolicyTests.check_continuation(self,self.config(1.))


if __name__=='__main__':
    unittest.main(verbosity=2)
