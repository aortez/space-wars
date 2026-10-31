"""Gradient routing, strict contracts and legacy arithmetic for critic isolation."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import identity,load,save
from tools.flight_ppo.collector import Collector
from tools.flight_ppo.curriculum import generate
from tools.flight_ppo.model import FlightModel,VALUE_SHARED,VALUE_HEAD_ONLY,value_gradient_mode
from tools.flight_ppo.ppo import update
from tools.flight_ppo import exploration
from tools.flight_ppo.tests import BRIDGE,equal_tree
from tools.flight_audit.value_isolation import assert_parent_replay,CHANGED_SOURCES


class ValueIsolationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
        cls.bridge=Bridge(BRIDGE);cls.schema=cls.bridge.contract['schema']
    @classmethod
    def tearDownClass(cls):cls.bridge.close()
    def setUp(self):torch.manual_seed(117)
    def model_inputs(self):
        model=FlightModel(self.schema);x=torch.zeros(8,4009);x[:,:35]=torch.randn(8,35)*.1;x[:,35:3497]=torch.rand(8,3462)*.2
        return model,x
    def grads(self,loss,model):
        return {n:None if g is None else g.detach().clone() for (n,p),g in zip(model.named_parameters(),torch.autograd.grad(loss,list(model.parameters()),allow_unused=True))}
    def rollout(self,model,x):
        with torch.no_grad():
            actor,value=model(x);dist=exploration.distribution(model,actor,actor,torch.zeros_like(actor),torch.zeros(8),0.,.25)
            z=dist.sample();lp=exploration.log_prob(model,dist,z)
        batch={'observations':x.reshape(4,2,4009),'ar_previous_observations':torch.zeros(4,2,4009),
               'ar_previous_latents':torch.zeros(4,2,3),'ar_previous_valid':torch.zeros(4,2),
               'ar_latents':z.reshape(4,2,3),'logp':lp.reshape(4,2),'advantages':torch.zeros(4,2),'returns':(value+1).reshape(4,2)}
        config={'exploration':{'kind':exploration.SCALED_KIND,'rho':0.,'std_multiplier':.25},
                'epochs':1,'minibatch':8,'clip':.2,'value_weight':.5,'entropy_weight':0.,'grad_clip':.5,'target_kl':.03}
        return batch,config

    def test_mode_validation_and_legacy_default(self):
        self.assertEqual(value_gradient_mode(),VALUE_SHARED)
        for v in [True,False,0,[],{},'shared','head-only-v2']:
            with self.assertRaises(ValueError):value_gradient_mode(v)
        for v in [VALUE_SHARED,VALUE_HEAD_ONLY]:self.assertEqual(value_gradient_mode(v),v)

    def test_forward_values_actor_and_rng_are_unchanged(self):
        model,x=self.model_inputs();rng=torch.get_rng_state().clone();before=copy.deepcopy(model.state_dict())
        original=model(x);shared=model(x,value_gradient=VALUE_SHARED);head=model(x,value_gradient=VALUE_HEAD_ONLY)
        self.assertTrue(equal_tree(original,shared) and equal_tree(original,head))
        self.assertTrue(torch.equal(rng,torch.get_rng_state()));self.assertTrue(equal_tree(before,model.state_dict()))

    def test_value_gradients_stop_only_at_shared_features(self):
        model,x=self.model_inputs();shared=self.grads(model(x)[1].square().mean(),model)
        head=self.grads(model(x,value_gradient=VALUE_HEAD_ONLY)[1].square().mean(),model)
        self.assertGreater(sum(float(g.norm()) for n,g in shared.items() if g is not None and not n.startswith('critic.')),0)
        for n,g in head.items():
            if n.startswith('critic.'):
                self.assertIsNotNone(g);self.assertTrue(torch.equal(g,shared[n]));self.assertGreater(float(g.norm()),0)
            else:self.assertIsNone(g,n)

    def test_actor_gradients_are_identical_in_both_modes(self):
        model,x=self.model_inputs();a=self.grads(model(x)[0].square().mean(),model)
        b=self.grads(model(x,value_gradient=VALUE_HEAD_ONLY)[0].square().mean(),model)
        self.assertTrue(equal_tree(a,b));self.assertGreater(float(b['fusion2.weight'].norm()),0)

    def test_ppo_still_trains_value_head_without_changing_encoder_for_value_only(self):
        model,x=self.model_inputs();batch,config=self.rollout(model,x);before=copy.deepcopy(model.state_dict())
        optimizer=torch.optim.Adam(model.parameters(),lr=.000075,eps=1e-5)
        update(model,optimizer,batch,dict(config,value_gradient=VALUE_HEAD_ONLY))
        for n,p in model.state_dict().items():
            if n.startswith('critic.'):self.assertFalse(torch.equal(p,before[n]))
            else:self.assertTrue(torch.equal(p,before[n]),n)
        model.load_state_dict(before);optimizer=torch.optim.Adam(model.parameters(),lr=.000075,eps=1e-5)
        update(model,optimizer,batch,config)
        self.assertFalse(torch.equal(model.state_dict()['fusion2.weight'],before['fusion2.weight']))

    def test_explicit_shared_ppo_is_exact_legacy_default(self):
        model,x=self.model_inputs();batch,config=self.rollout(model,x);batch['advantages']=torch.linspace(-1,1,8).reshape(4,2)
        before=copy.deepcopy(model.state_dict());rng=torch.get_rng_state().clone();states=[]
        for spec in [{},{'value_gradient':VALUE_SHARED}]:
            model.load_state_dict(before);torch.set_rng_state(rng);optimizer=torch.optim.Adam(model.parameters(),lr=.000075,eps=1e-5)
            metrics=update(model,optimizer,batch,dict(config,**spec))
            states.append(copy.deepcopy({'model':model.state_dict(),'optimizer':optimizer.state_dict(),'rng':torch.get_rng_state(),'metrics':metrics}))
        self.assertTrue(equal_tree(*states))

    def test_invalid_mode_rejected_before_output_directory(self):
        from tools.flight_ppo.run import main
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp);(p/'c.json').write_text(json.dumps({'action_contract':'continuous-flight-v2','value_gradient':'bad'}))
            with mock.patch('sys.argv',['run','--bridge','unused','--config',str(p/'c.json'),'--out',str(p/'out')]):
                with self.assertRaisesRegex(ValueError,'value-gradient'):main()
            self.assertFalse((p/'out').exists())

    def test_checkpoint_rejects_gradient_mode_mismatch(self):
        config=json.loads(Path('tools/flight_ppo/clear-control-config.json').read_text());config['value_gradient']=VALUE_SHARED
        _,pools=generate(self.bridge,4110001,1);collector=Collector(self.bridge,pools);model=FlightModel(self.schema)
        optimizer=torch.optim.Adam(model.parameters(),lr=config['learning_rate'],eps=1e-5)
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'checkpoint.pt';save(path,identity(config,self.bridge),model,optimizer,collector,0,[])
            with self.assertRaisesRegex(ValueError,'mismatch'):load(path,identity(dict(config,value_gradient=VALUE_HEAD_ONLY),self.bridge),self.bridge)

    def test_parent_audit_normalization_cannot_hide_state_changes(self):
        parent={'contract':{'config':{},'python_sources':{k:'old' for k in CHANGED_SOURCES}},'model':{'x':torch.tensor([1.])},'optimizer':{},'history':[1]}
        current=copy.deepcopy(parent);current['contract']['config']['value_gradient']=VALUE_SHARED
        current['contract']['python_sources']={k:'new' for k in CHANGED_SOURCES}
        assert_parent_replay(current,parent)
        current['model']['x']+=1
        with self.assertRaises(AssertionError):assert_parent_replay(current,parent)


if __name__=='__main__':unittest.main()
