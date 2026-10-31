"""Graded timeout objective invariants and real collector/checkpoint integration."""
import copy
import json
import math
from pathlib import Path
import unittest
import torch
from . import tests
from .bridge import Bridge
from .collector import Collector
from .curriculum import generate
from .model import FlightModel
from .reward import deadline_reward, potential, reward


class ProgressTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
        cls.bridge=Bridge(tests.BRIDGE)
        cls.pools=generate(cls.bridge,4110001,1)[1]

    @classmethod
    def tearDownClass(cls):cls.bridge.close()

    def config(self,weight):
        config=json.loads((Path(__file__).parent/'clear-control-config.json').read_text())
        config.update(updates=2,rollout_ticks=8,epochs=2,minibatch=16,train_tasks_per_stratum=1,
            velocity_shaping_weight=.25,deadline_distance_weight=weight,
            exploration={'kind':'continuous-gaussian-ar1-v2','rho':0.})
        return config

    def test_grade_is_bounded_monotonic_and_invalid_values_fail(self):
        distances=[0,1,3,3.0001,4,10,33,80,1000,1e20]
        scores=[deadline_reward(d,4.) for d in distances]
        self.assertEqual(scores[:3],[-2.]*3)
        self.assertTrue(all(-6<=s<=-2 for s in scores))
        self.assertTrue(all(a>b for a,b in zip(scores[2:],scores[3:])))
        self.assertEqual(deadline_reward(33,4),-4.)
        self.assertEqual([deadline_reward(d,0.) for d in distances],[-2.]*len(distances))
        for w in [-1,4.1,float('nan'),float('inf')]:
            with self.assertRaises(ValueError):deadline_reward(10,w)
        for d in [-1,float('nan'),float('inf')]:
            with self.assertRaises(ValueError):deadline_reward(d,4.)

    def test_control_is_legacy_exact_and_only_deadline_terminal_changes(self):
        before={'frame':{'errors':{'position':80.,'velocity':6.,'spin':.8}}}
        after={'frame':{'errors':{'position':20.,'velocity':2.,'spin':.3}},'progress':{'outcome':None}}
        for outcome,base in [(None,0.),('deadline',-2.),('success',10.),('collision',-10.),('damage',-10.),('ship_lost',-10.),('boundary_escape',-10.),('controller_error',-10.)]:
            after['progress']['outcome']=outcome
            shaped=.9995*(potential(after['frame']) if outcome is None else 0.)-potential(before['frame'])
            legacy={'terminal':base,'time':-.001,'potential':shaped,'total':base-.001+shaped}
            self.assertEqual(reward(before,after,.9995,.25,0.),legacy)
            graded=reward(before,after,.9995,.25,4.)
            if outcome!='deadline':self.assertEqual(graded,legacy)
            else:
                self.assertEqual(graded['time'],legacy['time']);self.assertEqual(graded['potential'],legacy['potential'])
                self.assertAlmostEqual(graded['total']-legacy['total'],deadline_reward(20,4)+2)
        after['progress']['outcome']='environment_error'
        with self.assertRaises(RuntimeError):reward(before,after,.9995,.25,4.)

    def test_partial_progress_survives_telescoping_and_outcome_order_is_preserved(self):
        gamma=.9995
        start={'errors':{'position':80.,'velocity':6.,'spin':.8}}
        returns={}
        for weight in [0.,4.]:
            for distance in [3.,20.,80.]:
                frames=[start,{'errors':{'position':100.,'velocity':20.,'spin':1.}},
                        {'errors':{'position':distance,'velocity':12.,'spin':.7}}]
                total=0.
                for i in range(2):
                    term=reward({'frame':frames[i]},{'frame':frames[i+1],'progress':{'outcome':'deadline' if i==1 else None}},gamma,.25,weight)
                    total+=gamma**i*term['total']
                expected=-potential(start)-.001*(1+gamma)+gamma*deadline_reward(distance,weight)
                self.assertAlmostEqual(total,expected,places=13);returns[weight,distance]=total
        self.assertAlmostEqual(returns[0.,3.],returns[0.,80.],places=13)
        self.assertGreater(returns[4.,3.],returns[4.,20.]);self.assertGreater(returns[4.,20.],returns[4.,80.])
        def complete(n,terminal):return -.001*(1-gamma**n)/(1-gamma)+gamma**(n-1)*terminal
        self.assertGreater(min(complete(n,10.) for n in range(1,1801)),complete(1800,-2.))
        self.assertGreater(complete(1800,-6.),max(complete(n,-10.) for n in range(1,1801)))

    def test_collector_changes_real_deadline_rewards_without_changing_actions(self):
        pools=copy.deepcopy(self.pools)
        for pool in pools:pool[0]['task']['deadline_ticks']=32
        records=[]
        for weight in [0.,4.]:
            torch.manual_seed(17)
            collector=Collector(self.bridge,pools);model=FlightModel(self.bridge.contract['schema'])
            config=self.config(weight);config['rollout_ticks']=40
            rollout=collector.collect(model,config);records.append((rollout,collector.state()))
        a,b=records
        for key in a[0]:
            if key not in ['advantages','returns']:self.assertTrue(torch.equal(a[0][key],b[0][key]),key)
        self.assertFalse(torch.equal(a[0]['advantages'],b[0]['advantages']))
        self.assertEqual(a[1]['traces'],b[1]['traces'])
        deadlines=0
        for ea,eb in zip(a[1]['completed'],b[1]['completed']):
            self.assertEqual(ea['outcome'],eb['outcome']);self.assertEqual(ea['final_errors'],eb['final_errors'])
            if ea['outcome']=='deadline':
                deadlines+=1;self.assertEqual(ea['reward']['terminal'],-2.)
                self.assertEqual(eb['reward']['terminal'],deadline_reward(ea['final_errors']['position'],4.))
        self.assertEqual(deadlines,4)

    def test_both_weights_resume_exactly_in_fresh_processes(self):
        for weight in [0.,4.]:
            with self.subTest(weight=weight):tests.PolicyTests.check_continuation(self,self.config(weight))


if __name__=='__main__':unittest.main(verbosity=2)
