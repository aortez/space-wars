"""Family identities, shared scaling, policy gradients and exact continuation."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import torch
from tools.flight_ppo import tests as policy_tests
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import load
from tools.flight_ppo.collector import Collector
from tools.flight_ppo.curriculum import sample
from tools.flight_ppo.model import FlightModel
from tools.flight_ppo.ppo import GLOBAL_CENTERING, FAMILY_CENTERING, advantage_centering, normalize_advantages, advantage_diagnostics, update
from tools.flight_ppo.tests import BRIDGE, equal_tree
from tools.flight_audit import value_isolation_tests, goal_scaling_tests
from tools.flight_audit.family_centering import assert_control_replay, CHANGED_SOURCES, benefit


class FamilyCenteringTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
        cls.bridge=Bridge(BRIDGE);cls.schema=cls.bridge.contract['schema']

    @classmethod
    def tearDownClass(cls):cls.bridge.close()

    def config(self):
        return json.loads(Path('data/training-experiments/learned-flight-goal-residual-v1-20261024/configs/residual/17.json').read_text())

    def test_family_means_removed_with_common_raw_scale(self):
        raw=torch.tensor([10.,100.,-10.,1.,14.,104.,-6.,5.]);labels=torch.arange(4).repeat(2)
        rng=torch.get_rng_state().clone();saved=raw.clone()
        actual=normalize_advantages(raw,labels,FAMILY_CENTERING)
        expected=torch.tensor([-2.]*4+[2.]*4)/raw.std(unbiased=False)
        self.assertTrue(torch.equal(actual,expected));self.assertTrue(torch.equal(raw,saved))
        self.assertTrue(torch.equal(rng,torch.get_rng_state()))
        self.assertTrue(torch.equal(actual[:4],actual[4:]*-1))
        global_adv=normalize_advantages(raw,labels)
        for i in range(4):
            self.assertAlmostEqual(float(actual[i+4]-actual[i]),float(global_adv[i+4]-global_adv[i]),places=6)
        self.assertLess(float(actual.std(unbiased=False)),.1)  # Not per-family variance scaling.

    def test_labels_follow_rows_and_unequal_group_sizes(self):
        raw=torch.tensor([3.,9.,1.,6.,2.,8.,4.,10.,5.]);labels=torch.tensor([0,1,2,3,0,1,2,3,3])
        actual=normalize_advantages(raw,labels,FAMILY_CENTERING)
        perm=torch.tensor([8,4,1,5,0,6,3,7,2])
        torch.testing.assert_close(normalize_advantages(raw[perm],labels[perm],FAMILY_CENTERING),actual[perm],atol=1e-7,rtol=1e-6)
        for i in range(4):self.assertAlmostEqual(float(actual[labels==i].mean()),0,places=6)

    def test_invalid_labels_and_modes_rejected_and_constant_advantages_finite(self):
        self.assertEqual(advantage_centering(),GLOBAL_CENTERING)
        raw=torch.ones(8);good=torch.arange(4).repeat(2)
        for labels in [None,good.float(),good[:4],torch.zeros(8,dtype=torch.int64),good+1]:
            with self.assertRaisesRegex(ValueError,'labels'):normalize_advantages(raw,labels,FAMILY_CENTERING)
        for spec in ['bad',False,{},[]]:
            with self.assertRaisesRegex(ValueError,'centering'):advantage_centering(spec)
        for mode in [GLOBAL_CENTERING,FAMILY_CENTERING]:self.assertTrue(torch.equal(normalize_advantages(raw,good,mode),torch.zeros(8)))

    def test_policy_gradient_matches_between_family_offset_removal(self):
        raw=torch.tensor([10.,100.,-10.,1.,14.,104.,-6.,5.]);labels=torch.arange(4).repeat(2)
        score=torch.tensor([.2,-.3,.4,-.1,.1,.2,-.1,.3])
        theta=torch.tensor(.0,requires_grad=True)
        global_adv=normalize_advantages(raw,labels)
        family_adv=normalize_advantages(raw,labels,FAMILY_CENTERING)
        # At the unchanged policy ratio=1, PPO clipping is inactive.
        first=-((theta*score).exp()*global_adv).mean()
        second=-((theta*score).exp()*family_adv).mean()
        g=torch.autograd.grad(first,theta)[0];f=torch.autograd.grad(second,theta)[0]
        means=torch.tensor([12.,102.,-8.,3.])
        expected=((means[labels]-raw.mean())/raw.std(unbiased=False)*score).mean()
        torch.testing.assert_close(f-g,expected)
        self.assertTrue(torch.equal((theta.detach()*score).exp(),torch.ones(8)))

    def test_explicit_global_mode_and_diagnostics_preserve_updates_and_rng(self):
        helper=value_isolation_tests.ValueIsolationTests()
        helper.schema=self.schema
        model,x=helper.model_inputs();batch,config=helper.rollout(model,x)
        batch['advantages']=torch.tensor([[10.,100.],[-10.,1.],[14.,104.],[-6.,5.]])
        batch['family_ids']=torch.arange(4).repeat(2).reshape(4,2)
        before=copy.deepcopy(model.state_dict());rng=torch.get_rng_state().clone();states=[]
        for mode in [None,GLOBAL_CENTERING]:
            model.load_state_dict(before);torch.set_rng_state(rng)
            optimizer=torch.optim.Adam(model.parameters(),lr=.000075,eps=1e-5)
            spec=dict(config,advantage_centering=mode)
            if mode:
                saved=copy.deepcopy(batch);signal=advantage_diagnostics(batch,spec)
                self.assertEqual(signal['family_ids'],list(range(4))*2)
                self.assertTrue(equal_tree(saved,batch));self.assertTrue(torch.equal(rng,torch.get_rng_state()))
            metrics=update(model,optimizer,batch,spec)
            states.append(copy.deepcopy([model.state_dict(),optimizer.state_dict(),metrics,torch.get_rng_state()]))
        self.assertTrue(equal_tree(*states))

    def test_collector_labels_survive_episode_reset_and_mapping_errors_are_early(self):
        config=self.config();config.update(rollout_ticks=40,train_tasks_per_stratum=1,advantage_centering=FAMILY_CENTERING)
        _,pools=sample(self.bridge,config,'train');collector=Collector(self.bridge,pools)
        model=FlightModel(self.schema,architecture_spec=config['architecture'])
        with torch.no_grad():
            model.actor.weight.zero_();model.actor.bias.copy_(torch.tensor([0.,-20.,20.]));model.log_std.fill_(-5)
        rollout=collector.collect(model,config)
        self.assertEqual(rollout['family_ids'].tolist(),[[0,1,2,3]]*40)
        self.assertGreater(len(collector.completed),0)
        state=self.bridge.call('state');collector.pools[1][0]['recipe']='settle-recover-v1/stratum-0'
        with self.assertRaisesRegex(ValueError,'mapping'):collector.collect(model,config)
        self.assertEqual(state,self.bridge.call('state'))

    def test_bad_configuration_rejected_before_output_and_resume_stays_strict(self):
        from tools.flight_ppo.run import main
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)
            for extra in [{'advantage_centering':'bad'},{'advantage_centering':FAMILY_CENTERING,'task_recipe':'clear-control-v1'}]:
                (p/'c.json').write_text(json.dumps({'action_contract':'continuous-flight-v2',**extra}))
                with mock.patch('sys.argv',['run','--bridge','unused','--config',str(p/'c.json'),'--out',str(p/'out')]):
                    with self.assertRaises(ValueError):main()
                self.assertFalse((p/'out').exists())
            torch.save({'contract':{'config':{'advantage_centering':GLOBAL_CENTERING}}},p/'state.pt')
            with self.assertRaisesRegex(ValueError,'mismatch'):load(p/'state.pt',{'config':{'advantage_centering':FAMILY_CENTERING}},None)

    def test_fresh_process_family_resume_is_exact(self):
        config=self.config();del config['warm_start_sha256']
        config.update(advantage_centering=FAMILY_CENTERING,updates=2,rollout_ticks=8,epochs=2,minibatch=16,train_tasks_per_stratum=1)
        policy_tests.PolicyTests.check_continuation(self,config)

    def test_control_comparison_and_joint_physical_gate_remain_strict(self):
        old={'contract':{'python_sources':{k:'old' for k in CHANGED_SOURCES},'bridge_sha256':'runtime'},'model':{'x':torch.tensor([1.])}}
        new=copy.deepcopy(old);new['contract']['python_sources']={k:'new' for k in CHANGED_SOURCES}
        assert_control_replay(new,old)
        new['contract']['bridge_sha256']='different'
        with self.assertRaises(AssertionError):assert_control_replay(new,old)
        rows=goal_scaling_tests.GoalScalingTests.records(self)
        for row in rows:row['arm']={'schema':'global','goal':'family'}[row['arm']]
        self.assertTrue(benefit(rows)['passed'])
        for row in rows:
            if row['arm']=='family':row['position_directions']['final']['left']=0
        self.assertFalse(benefit(rows)['passed'])


if __name__=='__main__':unittest.main(verbosity=2)
