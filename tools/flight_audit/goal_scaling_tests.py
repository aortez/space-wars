"""Function preservation, scaled gradients, strict resume and physical benefit gates."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import torch
from tools.flight_ppo.model import FlightModel, SCHEMA_SCALING, TASK_SCALING, GOAL_SCALING, VALUE_HEAD_ONLY
from tools.flight_ppo.initialization import initialize_policy
from tools.flight_ppo.checkpoint import load
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.goal_scaling import benefit, assert_control_replay, CHANGED_SOURCES


class GoalScalingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
        cls.schema=json.loads(Path('data/training-experiments/learned-flight-value-isolation-v1-20261024/pilot/head_only/seed-17/policy-0000.json').read_text())['observation_schema']

    def pair(self):
        torch.manual_seed(93); base=FlightModel(self.schema); goal=FlightModel(self.schema,GOAL_SCALING)
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'policy.json'; base.export(p); rng=torch.get_rng_state().clone()
            initialize_policy(goal,{'warm_start_sha256':hashlib.sha256(p.read_bytes()).hexdigest()},p)
            self.assertTrue(torch.equal(rng,torch.get_rng_state()))
            out=Path(d)/'goal.json';goal.export(out);self.assertEqual(p.read_bytes(),out.read_bytes())
        torch.manual_seed(12);x=torch.randn(8,4009)*.2
        x[:,3497:].reshape(-1,32,16)[:,:,0]=1
        x[::2,3497:].reshape(-1,32,16)[:,:,0]=0
        return base,goal,x

    def test_warm_start_preserves_function_and_changes_only_goal_columns(self):
        base,goal,x=self.pair();saved=copy.deepcopy(goal.state_dict())
        self.assertTrue(equal_tree(base(x),goal(x)))
        self.assertTrue(equal_tree(base.state_dict(),goal.inference_state_dict()))
        self.assertTrue(equal_tree(saved,goal.state_dict()))
        self.assertEqual(sum(p.numel() for p in goal.parameters()),39447)
        for name,p in base.state_dict().items():
            if name in ['query.weight','fusion1.weight']:
                self.assertTrue(torch.equal(p[:,:2]/32,saved[name][:,:2]));self.assertTrue(torch.equal(p[:,2:],saved[name][:,2:]))
            else:self.assertTrue(torch.equal(p,saved[name]))

    def test_goal_gradient_scale_changes_both_consumers_without_velocity_scaling(self):
        base,goal,x=self.pair()
        for model in [base,goal]:model(x,value_gradient=VALUE_HEAD_ONLY)[0].square().sum().backward()
        for (name,p),(other,q) in zip(base.named_parameters(),goal.named_parameters()):
            self.assertEqual(name,other)
            if p.grad is None:self.assertIsNone(q.grad);continue
            expected=p.grad.clone()
            if name in ['query.weight','fusion1.weight']:expected[:,:2]*=32
            torch.testing.assert_close(q.grad,expected,atol=1e-7,rtol=1e-5)

    def test_warm_start_remains_hash_checked_and_rejects_legacy_task_mode(self):
        base,goal,_=self.pair()
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'policy.json';base.export(p)
            with self.assertRaisesRegex(ValueError,'hash mismatch'):initialize_policy(goal,{'warm_start_sha256':'bad'},p)
            legacy=FlightModel(self.schema,TASK_SCALING)
            with self.assertRaisesRegex(ValueError,'parameterization'):initialize_policy(legacy,{'warm_start_sha256':hashlib.sha256(p.read_bytes()).hexdigest()},p)

    def test_invalid_scaling_rejected_before_output_creation(self):
        from tools.flight_ppo.run import main
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);(p/'config.json').write_text(json.dumps({'action_contract':'continuous-flight-v2','input_scaling':'goal-unknown'}))
            with mock.patch('sys.argv',['run','--bridge','unused','--config',str(p/'config.json'),'--out',str(p/'out')]):
                with self.assertRaisesRegex(ValueError,'scaling'):main()
            self.assertFalse((p/'out').exists())

    def test_resume_refuses_scaling_mismatch(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'state.pt';torch.save({'contract':{'config':{'input_scaling':SCHEMA_SCALING}}},p)
            with self.assertRaisesRegex(ValueError,'mismatch'):load(p,{'config':{'input_scaling':GOAL_SCALING}},None)

    def test_control_audit_does_not_hide_state_or_extra_source_changes(self):
        old={'contract':{'python_sources':{k:'old' for k in CHANGED_SOURCES}},'history':[1],'model':{'x':torch.tensor([1.])}}
        new=copy.deepcopy(old);new['contract']['python_sources']={k:'new' for k in CHANGED_SOURCES};assert_control_replay(new,old)
        new['model']['x']+=1
        with self.assertRaises(AssertionError):assert_control_replay(new,old)
        new=copy.deepcopy(old);new['contract']['python_sources']['other.py']='new'
        with self.assertRaises(AssertionError):assert_control_replay(new,old)

    def records(self):
        rows=[]
        for seed in [17,29,43]:
            for arm in ['schema','goal']:
                initial={'successes':8,'by_family':{'position':0},'physical_failures':0}
                final=copy.deepcopy(initial)
                if arm=='goal':final['successes']=9;final['by_family']['position']=1
                rows.append({'seed':seed,'arm':arm,'primary':{'initial':initial,'final':final},
                    'position_directions':{'final':{'forward':0,'left':int(arm=='goal'),'backward':0,'right':0}},
                    'legacy_hold':{'final':{'immediate_holds':16,'immediate_holds_by_stratum':[4]*4,'physical_failures':0}}})
        return rows

    def test_benefit_requires_joint_position_gain_and_holding_in_two_seeds(self):
        rows=self.records();self.assertTrue(benefit(rows)['passed'])
        for row in rows:
            if row['arm']=='goal' and row['seed']!=43:row['legacy_hold']['final']['immediate_holds']=11
        self.assertFalse(benefit(rows)['passed'])

    def test_response_or_forward_only_gain_cannot_pass_physical_gate(self):
        rows=self.records()
        for row in rows:
            if row['arm']=='goal':row['position_directions']['final']['left']=0;row['position_directions']['final']['forward']=1
        self.assertFalse(benefit(rows)['passed'])
        rows=self.records()
        for row in rows:
            if row['arm']=='goal':row['primary']['final']['physical_failures']=1
        self.assertFalse(benefit(rows)['passed'])
        rows=self.records()
        for row in rows:
            if row['arm']=='schema':row['primary']['final']['successes']=9;row['primary']['final']['by_family']['position']=1
        self.assertFalse(benefit(rows)['passed'])


if __name__=='__main__':unittest.main(verbosity=2)
