"""Scaled Gaussian sampling, PPO density/entropy and strict legacy isolation."""
import copy
import json
import math
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import torch
from torch.distributions import Normal
from tools.flight_ppo import exploration as ar
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.collector import Collector
from tools.flight_ppo.curriculum import generate
from tools.flight_ppo.model import FlightModel,ACTION_CONTRACT
from tools.flight_ppo.ppo import update
from tools.flight_ppo.tests import BRIDGE,equal_tree
from tools.flight_audit.noise_training import assert_parent_replay

class NoiseTrainingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True);cls.bridge=Bridge(BRIDGE);cls.schema=cls.bridge.contract['schema']
    @classmethod
    def tearDownClass(cls):cls.bridge.close()

    def test_versioned_multiplier_validation(self):
        old={'kind':ar.KIND,'rho':0.};self.assertEqual(ar.amplitude(old),1.)
        new={'kind':ar.SCALED_KIND,'rho':0.,'std_multiplier':.25};self.assertEqual(ar.amplitude(new),.25)
        for v in [0.,.001,1.1,True,float('nan'),float('inf'),'0.25']:
            with self.assertRaises(ValueError):ar.amplitude(dict(new,std_multiplier=v))
        with self.assertRaises(ValueError):ar.amplitude(dict(old,std_multiplier=.25))
        with self.assertRaises(ValueError):ar.amplitude({'kind':ar.SCALED_KIND,'rho':0.})

    def test_full_noise_samples_and_rng_are_exact_legacy(self):
        model=FlightModel(self.schema);actor=torch.tensor([[.2,-2.,.1]]);valid=torch.ones(1);old_actor=actor-.1;past=actor+.2
        expected=Normal(actor+.95*(past-old_actor),model.log_std.clamp(-5,2).exp()*math.sqrt(1-.95**2))
        actual=ar.distribution(model,actor,old_actor,past,valid,.95,1.)
        torch.testing.assert_close(actual.loc,expected.loc,rtol=0,atol=0)
        # Match the existing tensor-expression order exactly, including sqrt rounding.
        legacy=Normal(actor+(valid[:,None]*.95)*(past-old_actor),model.log_std.clamp(-5,2).exp()*(1-(valid[:,None]*.95).square()).sqrt())
        self.assertTrue(torch.equal(actual.scale,legacy.scale))
        rng=torch.get_rng_state().clone();x=actual.sample();after=torch.get_rng_state().clone();torch.set_rng_state(rng);y=legacy.sample()
        self.assertTrue(torch.equal(x,y) and torch.equal(after,torch.get_rng_state()))
        self.assertTrue(torch.equal(ar.log_prob(model,actual,x),ar.log_prob(model,legacy,y)))

    def test_scaled_conditional_density_entropy_and_gradients(self):
        model=FlightModel(self.schema);actor=torch.tensor([[.2,-2.,.1]],requires_grad=True);previous=torch.tensor([[.1,-1.,.3]],requires_grad=True);past=torch.tensor([[.5,-.8,.9]])
        dist=ar.distribution(model,actor,previous,past,torch.ones(1),.5,.25)
        expected=Normal(actor+.5*(past-previous),model.log_std.exp()*.25*math.sqrt(.75))
        latent=torch.tensor([[.1,-1.5,.2]])
        lp=expected.log_prob(latent).sum(-1)-model.log_jacobian(latent[:,0])-model.actuator_log_jacobian(latent[:,1:]).sum(-1)
        torch.testing.assert_close(ar.log_prob(model,dist,latent),lp,rtol=1e-6,atol=1e-6)
        gradient=torch.autograd.grad(ar.log_prob(model,dist,latent).sum(),actor,retain_graph=True)[0]
        torch.testing.assert_close(gradient,(latent-dist.loc)/dist.scale.square(),rtol=1e-5,atol=1e-5)
        rng=torch.get_rng_state().clone();ent=ar.entropy(model,dist);torch.set_rng_state(rng);z=expected.rsample()
        reference=expected.entropy().sum(-1)+model.log_jacobian(z[:,0])+model.actuator_log_jacobian(z[:,1:]).sum(-1)
        torch.testing.assert_close(ent,reference,atol=1e-6,rtol=1e-6)
        (ent.sum()+lp.sum()).backward();self.assertTrue(torch.isfinite(model.log_std.grad).all())

    def test_collector_and_optimizer_use_same_scaled_distribution(self):
        config=json.loads(Path('tools/flight_ppo/clear-control-config.json').read_text())
        config.update(rollout_ticks=8,epochs=1,minibatch=16,exploration={'kind':ar.SCALED_KIND,'rho':0.,'std_multiplier':.25})
        pools=generate(self.bridge,4110001,1)[1];collector=Collector(self.bridge,pools);model=FlightModel(self.schema)
        with mock.patch.object(ar,'distribution',wraps=ar.distribution) as spy:
            rollout=collector.collect(model,config)
            flat={k:v.flatten(0,1) for k,v in rollout.items()}
            with torch.no_grad():
                actor=model(flat['observations'])[0];previous=model(flat['ar_previous_observations'])[0]
                dist=ar.distribution(model,actor,previous,flat['ar_previous_latents'],flat['ar_previous_valid'],0.,.25)
                torch.testing.assert_close(ar.log_prob(model,dist,flat['ar_latents']),flat['logp'],atol=1e-5,rtol=1e-5)
            old=copy.deepcopy(model.state_dict());optimizer=torch.optim.Adam(model.parameters(),lr=config['learning_rate'],eps=1e-5)
            result=update(model,optimizer,rollout,config)
            self.assertGreater(result['minibatches'],0);self.assertFalse(equal_tree(old,model.state_dict()))
            self.assertTrue(all(call.args[-1]==.25 for call in spy.call_args_list))

    def test_unscaled_sampled_export_evaluation_is_rejected(self):
        from tools.flight_ppo.run import main
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp);config={'action_contract':ACTION_CONTRACT,'exploration':{'kind':ar.SCALED_KIND,'rho':0.,'std_multiplier':.25},
                              'evaluation':{'mode':'continuous-sampled-v2','repeats':1,'seed_base':1}}
            (p/'config.json').write_text(json.dumps(config))
            with mock.patch('sys.argv',['run','--bridge','unused','--config',str(p/'config.json'),'--out',str(p/'out')]):
                with self.assertRaisesRegex(ValueError,'export log_std'):main()
            self.assertFalse((p/'out').exists())

    def test_parent_replay_normalization_does_not_hide_model_changes(self):
        parent={'contract':{'config':{'exploration':{'kind':ar.KIND,'rho':0.}},'python_sources':{k:'old' for k in ['exploration.py','collector.py','ppo.py','run.py']}},
                'collector':{'exploration_state':{'spec':{'kind':ar.KIND,'rho':0.}}},'model':{'x':torch.tensor([1.])}}
        current=copy.deepcopy(parent);spec={'kind':ar.SCALED_KIND,'rho':0.,'std_multiplier':1.}
        current['contract']['config']['exploration']=spec;current['collector']['exploration_state']['spec']=spec
        current['contract']['python_sources']={k:'new' for k in current['contract']['python_sources']}
        assert_parent_replay(current,parent)
        current['model']['x']+=1
        with self.assertRaises(AssertionError):assert_parent_replay(current,parent)

if __name__=='__main__':unittest.main()
