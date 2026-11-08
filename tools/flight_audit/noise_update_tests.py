"""Synthetic checks for scaled-noise attribution and neutral optimizer instrumentation."""
import copy
import unittest
import torch
from tools.flight_audit.hold_update_tests import TinyModel
from tools.flight_audit.noise_update import (inspect_gradients, parameter_masks, parameter_changes,
                                            projections, ratio_metrics, probe, COMPONENTS)
from tools.flight_audit.signal import inspect_gradients as previous_gradients
from tools.flight_ppo import exploration
from tools.flight_ppo.ppo import update
from tools.flight_ppo.tests import equal_tree


class NoiseUpdateTests(unittest.TestCase):
    def setUp(self):torch.set_num_threads(1);torch.manual_seed(121)

    def setup_rollout(self, amplitude):
        model=TinyModel();x=torch.randn(8,2,4)
        with torch.no_grad():
            actor,value=model(x.flatten(0,1))
            dist=exploration.distribution(model,actor,actor,torch.zeros_like(actor),torch.zeros(16),0.,amplitude)
            latent=dist.sample();lp=exploration.log_prob(model,dist,latent)
        adv=torch.linspace(-1,1,16).reshape(8,2)
        rollout={'observations':x,'ar_previous_observations':torch.zeros_like(x),
                 'ar_previous_latents':torch.zeros(8,2,3),'ar_previous_valid':torch.zeros(8,2),
                 'ar_latents':latent.reshape(8,2,3),'logp':lp.reshape(8,2),
                 'advantages':adv,'returns':value.reshape(8,2)+adv}
        config={'exploration':{'kind':exploration.SCALED_KIND,'rho':0.,'std_multiplier':amplitude},
                'epochs':2,'minibatch':8,'clip':.2,'value_weight':.5,'entropy_weight':.01,'grad_clip':.5,'target_kl':.03}
        return model,rollout,config

    def test_scaled_score_pressure_matches_analytic_gaussian(self):
        for amplitude in [1.,.25]:
            model,rollout,config=self.setup_rollout(amplitude)
            result,vectors,values,adv,pressure=inspect_gradients(model,rollout,config,torch.get_rng_state())
            with torch.no_grad():
                actor,_=model(rollout['observations'].flatten(0,1))
                expected=(rollout['ar_latents'].flatten(0,1)-actor)/(model.log_std.exp()*amplitude).square()*adv[:,None]
            torch.testing.assert_close(pressure,expected)
            self.assertLess(result['maximum_abs_logp_error'],1e-5)

    def test_full_amplitude_matches_previous_decomposition(self):
        model,rollout,config=self.setup_rollout(1.);rng=torch.get_rng_state().clone()
        self.assertTrue(equal_tree(inspect_gradients(model,rollout,config,rng),previous_gradients(model,rollout,config,rng)))

    def test_partitions_cover_parameters_and_projections_sum(self):
        model,rollout,config=self.setup_rollout(.25)
        result,vectors,*_=inspect_gradients(model,rollout,config,torch.get_rng_state())
        masks=parameter_masks(model);self.assertTrue(torch.all(sum(m.int() for m in masks.values())==1))
        projection,gradients=projections(model,rollout['observations'].flatten(0,1),vectors)
        for metric,groups in projection.items():
            for component in COMPONENTS:
                self.assertAlmostEqual(groups['all'][component],sum(groups[g][component] for g in masks),places=6)
            self.assertEqual(groups['actor_head']['weighted_critic'],0.)
            self.assertEqual(groups['critic_head']['policy'],0.)
            self.assertEqual(groups['exploration']['weighted_entropy'],0.)

    def test_self_likelihood_ratios_and_wrong_amplitude_detection(self):
        model,rollout,config=self.setup_rollout(.25)
        result=ratio_metrics(model,rollout,config)
        self.assertLess(result['approx_kl'],1e-10);self.assertEqual(result['clip_fraction'],0.)
        wrong=copy.deepcopy(config);wrong['exploration']['std_multiplier']=1.
        self.assertGreater(ratio_metrics(model,rollout,wrong)['approx_kl'],.1)

    def test_step_probe_hook_does_not_change_exact_optimizer_result(self):
        model,rollout,config=self.setup_rollout(.25);optimizer=torch.optim.Adam(model.parameters(),lr=.0003,eps=1e-5)
        before=copy.deepcopy(model.state_dict());opt=copy.deepcopy(optimizer.state_dict());rng=torch.get_rng_state().clone()
        results=[];steps=[]
        for instrument in [False,True]:
            model.load_state_dict(before);optimizer.load_state_dict(copy.deepcopy(opt));torch.set_rng_state(rng)
            hook=optimizer.register_step_post_hook(lambda *_:steps.append(probe(model,rollout['observations'].flatten(0,1)))) if instrument else None
            try:metrics=update(model,optimizer,rollout,config)
            finally:
                if hook:hook.remove()
            results.append(copy.deepcopy({'model':model.state_dict(),'optimizer':optimizer.state_dict(),'rng':torch.get_rng_state(),'metrics':metrics}))
        self.assertTrue(equal_tree(*results));self.assertEqual(len(steps),metrics['minibatches'])
        self.assertGreater(parameter_changes(model,before)['actor_head'],0.)

    def test_gradient_inspection_preserves_parameters_and_optimizer(self):
        model,rollout,config=self.setup_rollout(.25);before=copy.deepcopy(model.state_dict())
        inspect_gradients(model,rollout,config,torch.get_rng_state())
        self.assertTrue(equal_tree(before,model.state_dict()))
        self.assertTrue(all(v==0. for v in parameter_changes(model,before).values()))


if __name__=='__main__':unittest.main(verbosity=2)
