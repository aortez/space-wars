"""Synthetic tests for explicit task scaling and native-compatible weight folding."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
import torch
from tools.flight_ppo.model import FlightModel, SCHEMA_SCALING, TASK_SCALING, input_scaling
from tools.flight_ppo.execution import load_export
from tools.flight_ppo.tests import equal_tree


class ScalingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)
        cls.schema=json.loads(Path('data/training-experiments/learned-flight-continuous-approach-v1-20261024/seed-17/policy-0128.json').read_text())['observation_schema']

    def models(self):
        torch.manual_seed(73);base=FlightModel(self.schema)
        torch.manual_seed(73);scaled=FlightModel(self.schema,TASK_SCALING)
        return base,scaled

    def fixture(self):
        torch.manual_seed(91);x=torch.randn(12,4009)*.1
        x[:,3497:].reshape(-1,32,16)[:,:,0]=1
        return x

    def test_strict_scaling_and_equal_initial_parameter_draws(self):
        self.assertEqual(input_scaling(),SCHEMA_SCALING)
        for bad in ['unknown',{},1,True]:
            with self.assertRaises(ValueError):input_scaling(bad)
        base,scaled=self.models()
        self.assertTrue(equal_tree(base.state_dict(),scaled.state_dict()))
        self.assertEqual(sum(p.numel() for p in scaled.parameters()),39447)
        self.assertNotIn('task_input_factors',scaled.state_dict())

    def test_only_four_task_features_change_in_both_consumers(self):
        base,scaled=self.models();x=self.fixture();captured={}
        def capture(name):
            def hook(module,args):captured[name]=args[0].detach().clone()
            return hook
        scaled.query.register_forward_pre_hook(capture('query'))
        scaled.fusion1.register_forward_pre_hook(capture('fusion'))
        scaled(x)
        expected=x[:,:35].clone();expected[:,:2]*=25;expected[:,2:4]*=17.5
        self.assertTrue(torch.equal(expected,captured['query']))
        self.assertTrue(torch.equal(expected,captured['fusion'][:,:35]))
        before=x.clone();scaled(x);self.assertTrue(torch.equal(before,x))

    def test_folded_export_matches_training_and_does_not_mutate(self):
        _,scaled=self.models();original=copy.deepcopy(scaled.state_dict());x=self.fixture()
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'policy.json';scaled.export(p);folded=load_export(p,self.schema)
            for a,b in zip(scaled(x),folded(x)):torch.testing.assert_close(a,b,atol=1e-5,rtol=1e-5)
            self.assertTrue(equal_tree(folded.state_dict(),scaled.inference_state_dict()))
        self.assertTrue(equal_tree(original,scaled.state_dict()))
        state=scaled.inference_state_dict()
        self.assertEqual({k for k in state if not torch.equal(state[k],original[k])},{'query.weight','fusion1.weight'})
        self.assertTrue(torch.equal(state['fusion1.weight'][:,4:],original['fusion1.weight'][:,4:]))

    def test_default_export_is_unmodified_state(self):
        base,_=self.models();self.assertTrue(equal_tree(base.state_dict(),base.inference_state_dict()))
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'policy.json';base.export(p);loaded=load_export(p,self.schema)
            for a,b in zip(base(self.fixture()),loaded(self.fixture())):self.assertTrue(torch.equal(a,b))

    def test_task_gradients_reach_both_input_paths(self):
        _,scaled=self.models();x=self.fixture();actor,value=scaled(x)
        (actor.square().sum()+value.square().sum()).backward()
        for key in ['query.weight','fusion1.weight']:
            grad=dict(scaled.named_parameters())[key].grad[:,:4]
            self.assertTrue(torch.isfinite(grad).all());self.assertGreater(float(grad.abs().sum()),0)


if __name__=='__main__':unittest.main(verbosity=2)
