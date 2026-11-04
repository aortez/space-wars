"""Residual initialization, actual gradient flow, native inference and strict resume."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import load, sha
from tools.flight_ppo.execution import load_export
from tools.flight_ppo.initialization import initialize_policy
from tools.flight_ppo.model import FlightModel, ARCHITECTURE, GOAL_RESIDUAL_ARCHITECTURE, VALUE_HEAD_ONLY, flatten
from tools.flight_ppo.tests import BRIDGE, equal_tree
from tools.flight_ppo import tests as policy_tests
from tools.flight_audit.goal_residual import assert_control_replay, CHANGED_SOURCES, benefit
from tools.flight_audit import goal_scaling_tests


class GoalResidualTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
        cls.bridge = Bridge(BRIDGE); cls.schema = cls.bridge.contract['schema']

    @classmethod
    def tearDownClass(cls): cls.bridge.close()

    def pair(self):
        torch.manual_seed(31); base = FlightModel(self.schema); rng = torch.get_rng_state().clone()
        torch.manual_seed(31); branch = FlightModel(self.schema, architecture_spec=GOAL_RESIDUAL_ARCHITECTURE)
        self.assertTrue(torch.equal(rng, torch.get_rng_state()))
        self.assertTrue(equal_tree(base.state_dict(), {k:v for k,v in branch.state_dict().items() if not k.startswith('goal_')}))
        x = torch.randn(8,4009)*.1; x[:,3497:] = 0
        return base, branch, x

    def test_initial_function_and_rng_preserved(self):
        base, branch, x = self.pair()
        self.assertTrue(equal_tree(base(x), branch(x)))
        self.assertEqual(sum(p.numel() for p in branch.parameters()),39546)
        self.assertEqual(base.architecture, ARCHITECTURE)

    def test_warm_start_hash_conversion_and_reversed_architecture_rejected(self):
        base, branch, x = self.pair()
        with torch.no_grad(): base.actor.bias.add_(.3)
        with tempfile.TemporaryDirectory() as d:
            p = Path(d)/'policy.json'; base.export(p); config = {'warm_start_sha256':sha(p)}
            rng = torch.get_rng_state().clone(); initialize_policy(branch,config,p)
            self.assertTrue(torch.equal(rng,torch.get_rng_state()))
            self.assertTrue(equal_tree(base(x),branch(x)))
            with self.assertRaisesRegex(ValueError,'hash mismatch'): initialize_policy(branch,{'warm_start_sha256':'bad'},p)
            with torch.no_grad(): branch.goal_output.bias[0] = 1
            with self.assertRaisesRegex(ValueError,'zero-output'): initialize_policy(branch,config,p)
            branch.export(p)
            with self.assertRaisesRegex(ValueError,'architecture conversion'): initialize_policy(base,{'warm_start_sha256':sha(p)},p)

    def test_zero_output_branch_receives_gradient_then_trains_hidden_layer(self):
        _, model, x = self.pair(); optimizer = torch.optim.Adam(model.parameters(),lr=.001,eps=1e-5)
        original = copy.deepcopy(model.state_dict())
        for step in range(2):
            optimizer.zero_grad(); actor,_ = model(x,value_gradient=VALUE_HEAD_ONLY)
            (actor-torch.tensor([.3,-.2,.4])).square().mean().backward()
            self.assertGreater(float(model.goal_output.weight.grad.norm()),0)
            if step == 0: self.assertEqual(float(model.goal_hidden.weight.grad.norm()),0)
            else: self.assertGreater(float(model.goal_hidden.weight.grad.norm()),0)
            optimizer.step()
        for k in ['goal_hidden.weight','goal_output.weight']:
            self.assertFalse(torch.equal(original[k],model.state_dict()[k]))

    def test_branch_uses_only_goal_and_bypasses_fusion_and_critic(self):
        base, model, x = self.pair()
        with torch.no_grad(): model.goal_output.weight.fill_(.3); model.goal_output.bias.fill_(.2)
        actor,value = model(x,value_gradient=VALUE_HEAD_ONLY); base_actor,base_value = base(x)
        correction = model.goal_output(torch.tanh(model.goal_hidden(x[:,:2]*25)))
        self.assertTrue(torch.equal(actor,base_actor+correction)); self.assertTrue(torch.equal(value,base_value))
        gradients = torch.autograd.grad(value.sum(),list(model.parameters()),allow_unused=True)
        for (name,_),g in zip(model.named_parameters(),gradients):
            if name.startswith('goal_'):self.assertIsNone(g)
        with torch.no_grad():
            model.fusion1.weight.zero_(); model.fusion1.bias.fill_(100)
            model.fusion2.weight.zero_(); model.fusion2.bias.fill_(100)
        x.requires_grad_(True); output = model(x)[0]
        derivative = torch.autograd.grad(output.sum(),x)[0]
        self.assertGreater(float(derivative[:,:2].abs().max()),0)
        self.assertEqual(float(derivative[:,2:].abs().max()),0)

    def test_nonzero_branch_export_native_parity_and_malformed_contracts(self):
        _,model,_ = self.pair()
        import gzip
        fixtures = json.loads(gzip.decompress(Path('data/training-experiments/learned-flight-recovery-response-v1-20261024/anchors/seed-17/start-1/inputs.json.gz').read_bytes()))
        with torch.no_grad(): model.goal_output.weight.normal_(0,.2);model.goal_output.bias.normal_(0,.1)
        with tempfile.TemporaryDirectory() as d:
            p = Path(d)/'policy.json';model.export(p); record=json.loads(p.read_text())
            self.assertEqual(record['format_version'],3)
            folded=load_export(p,self.schema)
            self.assertTrue(equal_tree(model.state_dict(),folded.state_dict()))
            self.bridge.call('load_policy',path=str(p))
            native=self.bridge.call('infer',inputs=fixtures)['outputs']
            expected=model(torch.tensor([flatten(f) for f in fixtures]))
            actual=torch.tensor([[n[k] for k in ['steering_latent','thrust_logit','brake_logit','value']] for n in native])
            torch.testing.assert_close(actual,torch.cat((expected[0],expected[1][:,None]),1),atol=1e-5,rtol=1e-5)
            invalid=[]
            for key,value in [('format_version',2),('architecture',ARCHITECTURE)]:
                v=copy.deepcopy(record);v[key]=value;invalid.append(v)
            v=copy.deepcopy(record);v['weights']['goal_output.weight']['values'].pop();invalid.append(v)
            v=copy.deepcopy(record);del v['weights']['goal_hidden.bias'];invalid.append(v)
            for v in invalid:
                p.write_text(json.dumps(v))
                with self.assertRaises(ValueError):load_export(p,self.schema)
                with self.assertRaises(RuntimeError):self.bridge.call('load_policy',path=str(p))

    def test_architecture_rejected_before_output_and_resume_cannot_cross(self):
        from tools.flight_ppo.run import main
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);(p/'c.json').write_text(json.dumps({'action_contract':'continuous-flight-v2','architecture':'bad'}))
            with mock.patch('sys.argv',['run','--bridge','unused','--config',str(p/'c.json'),'--out',str(p/'out')]):
                with self.assertRaisesRegex(ValueError,'architecture'):main()
            self.assertFalse((p/'out').exists())
            torch.save({'contract':{'architecture':ARCHITECTURE}},p/'state.pt')
            with self.assertRaisesRegex(ValueError,'mismatch'):load(p/'state.pt',{'architecture':GOAL_RESIDUAL_ARCHITECTURE},None)

    def test_residual_fresh_process_resume_is_exact(self):
        config=json.loads(Path('tools/flight_ppo/clear-control-config.json').read_text())
        config.update(architecture=GOAL_RESIDUAL_ARCHITECTURE,updates=2,rollout_ticks=8,epochs=2,minibatch=16,train_tasks_per_stratum=1)
        policy_tests.PolicyTests.check_continuation(self,config)

    def test_control_audit_cannot_hide_undeclared_changes(self):
        old={'contract':{'python_sources':{k:'old' for k in CHANGED_SOURCES},'bridge_sha256':'old-runtime'},'model':{'x':torch.tensor([1.])}}
        new=copy.deepcopy(old);new['contract']['python_sources']={k:'new' for k in CHANGED_SOURCES};new['contract']['bridge_sha256']='new-runtime'
        assert_control_replay(new,old,'new-runtime','old-runtime')
        with self.assertRaises(AssertionError):assert_control_replay(new,old,'unknown-runtime','old-runtime')
        new['model']['x']+=1
        with self.assertRaises(AssertionError):assert_control_replay(new,old,'new-runtime','old-runtime')

    def test_benefit_remains_joint_physical_gain_in_two_seeds(self):
        rows=goal_scaling_tests.GoalScalingTests.records(self)
        for r in rows:
            if r['arm']=='goal':r['arm']='residual'
        self.assertTrue(benefit(rows)['passed'])
        for r in rows:
            if r['arm']=='residual':r['position_directions']['final']['left']=0
        self.assertFalse(benefit(rows)['passed'])


if __name__=='__main__':unittest.main(verbosity=2)
