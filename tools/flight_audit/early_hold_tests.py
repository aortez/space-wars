"""First-exit semantics, paired sampling and true-terminal credit checks."""
import copy
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.curriculum import HOLD_RECIPE, HOLD_EXIT_RECIPE, hold_candidate, hold_exit_candidate, sample
from tools.flight_ppo.tests import BRIDGE
from tools.flight_ppo.reward import reward, potential
from tools.flight_ppo.ppo import gae
from tools.flight_audit.early_hold import trace_metrics, matched_tasks


class EarlyHoldTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.bridge=Bridge(BRIDGE)
    @classmethod
    def tearDownClass(cls):cls.bridge.close()

    def test_identical_starts_and_development_unchanged(self):
        matched_tasks([hold_candidate(4310001,i) for i in range(128)], [hold_exit_candidate(4310001,i) for i in range(128)])
        c={'task_recipe':HOLD_RECIPE,'training_hold_failure':HOLD_EXIT_RECIPE,'train_task_seed':4310001,
           'development_task_seed':4310002,'train_tasks_per_stratum':1,'development_tasks_per_stratum':1}
        early,_=sample(self.bridge,c,'train');dev,_=sample(self.bridge,c,'development')
        c['training_hold_failure']='none';long,_=sample(self.bridge,c,'train');control_dev,_=sample(self.bridge,c,'development')
        self.assertEqual(dev,control_dev)
        matched_tasks([r['candidate'] for r in long['attempts']], [r['candidate'] for r in early['attempts']])
        c['training_hold_failure']=HOLD_EXIT_RECIPE;c['task_recipe']='aligned-approach-v1'
        with self.assertRaises(ValueError):sample(self.bridge,c,'train')

    def failure(self):
        before=self.bridge.call('reset',tasks=[hold_exit_candidate(4310099,0)['task']])['items'][0]
        for _ in range(30):
            after=self.bridge.call('step',actions=[{'steering':0.,'thrust':.5,'brake':.5}])['items'][0]
            if after['progress']['outcome']:break
            before=after
        return before,after,self.bridge.call('state')['traces'][0]

    def test_native_first_exit_true_terminal_and_replay(self):
        _,after,trace=self.failure()
        self.assertEqual(after['progress']['outcome'],'hold_broken');self.assertGreater(after['frame']['errors']['velocity'],1)
        m=trace_metrics(trace);self.assertLess(m['ticks'],30);self.assertEqual(m['qualified_before_ticks'],m['ticks'])
        self.assertEqual(trace['version'],3)
        self.bridge.call('restore',traces=[trace])
        with self.assertRaises(RuntimeError):self.bridge.call('step',actions=[{'steering':0.,'thrust':0.,'brake':0.}])

    def test_trace_rejects_relabeling_or_early_terminal(self):
        _,_,trace=self.failure()
        for change in ['version','outcome','tolerance']:
            bad=copy.deepcopy(trace)
            if change=='version':bad['version']=2
            elif change=='outcome':bad['steps'][-1]['progress']['outcome']='deadline'
            else:bad['steps'][-1]['frame']['errors']['velocity']=0.
            with self.assertRaises(ValueError):trace_metrics(bad)

    def test_success_still_needs_thirty_complete_intervals(self):
        self.bridge.call('reset',tasks=[hold_exit_candidate(4310099,1)['task']])
        for tick in range(1,31):
            item=self.bridge.call('step',actions=[{'steering':0.,'thrust':0.,'brake':0.}])['items'][0]
            self.assertEqual(item['progress']['outcome'],'success' if tick==30 else None)
        self.assertEqual(trace_metrics(self.bridge.call('state')['traces'][0])['ticks'],30)

    def test_terminal_reward_and_bootstrap(self):
        before,after,_=self.failure();terms=reward(before,after,.9995,.25,4.)
        self.assertEqual(terms['terminal'],-10.);self.assertAlmostEqual(terms['potential'],-potential(before['frame']))
        r=torch.tensor([[terms['total']],[2.]])
        _,returns=gae(r,torch.tensor([[1.],[0.]]),torch.tensor([[7.],[9.]]),torch.tensor([40.]),.9,1.)
        self.assertAlmostEqual(returns[0].item(),terms['total'],places=5)
        self.assertEqual(returns[1].item(),38.)

    def test_physical_failure_precedes_hold_failure(self):
        _,_,trace=self.failure();trace['steps'][-1]['evidence']['collision']=True
        trace['steps'][-1]['progress']['outcome']='collision'
        self.assertEqual(trace_metrics(trace)['outcome'],'collision')
        trace['steps'][-1]['progress']['outcome']='hold_broken'
        with self.assertRaises(ValueError):trace_metrics(trace)


if __name__=='__main__':unittest.main()
