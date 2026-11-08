"""Paired exploration, stationary AR noise, exact continuation and physical gates."""
import copy
import math
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.model import actions
from tools.flight_motor.tests import BRIDGE
from tools.flight_motor.audit import HEADING
from tools.flight_motor.velocity_audit import task_matches
from .motor_exploration import Sampler, VARIANTS, candidates, draws, inspect, exposure, seed_gate


class MotorExplorationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1)

    def test_complete_matrix_and_noise_are_paired_without_global_rng(self):
        rows=candidates();rng=torch.get_rng_state().clone();noise=draws(rows)
        self.assertTrue(torch.equal(rng,torch.get_rng_state()));self.assertEqual(noise,draws(rows))
        self.assertEqual(len(rows),40);self.assertEqual(sum(r['family']=='accelerate' for r in rows),24)
        self.assertEqual(sum(r['family']=='brake' for r in rows),16)
        self.assertEqual(len({r['noise_seed'] for r in rows}),40)
        for case in range(10):
            group=rows[case*4:case*4+4];self.assertTrue(all(r['task']==group[0]['task'] for r in group))
        self.assertTrue(all(len(v)==360 and len(v[0])==2 for v in noise.values()))

    def test_baseline_matches_original_actuator_distribution_and_steering_mean(self):
        actor=torch.tensor([.4,-4.,.6]);std=torch.tensor([.48,.49]);epsilon=torch.tensor([1.2,-.6])
        sampler=Sampler('baseline',std);out=sampler.step(actor,epsilon)
        latent=actor.clone();latent[1:]+=std*epsilon
        self.assertEqual(out['action'],actions(actor[:1],latent[None,1:])[0])
        for variant in VARIANTS:
            s=Sampler(variant,std);r=s.step(actor,epsilon)
            self.assertEqual(r['latent'][0],float(actor[0]));self.assertEqual(r['action']['steering'],out['action']['steering'])
            self.assertTrue(all(0<=r['action'][k]<=1 for k in ['thrust','brake']))

    def test_persistent_residual_has_stationary_start_and_exact_restore(self):
        innovations=draws(candidates())['0'];std=[.5,.49];s=Sampler('wide8_persistent',std)
        self.assertEqual(s.step([0.,-4.,0.],innovations[0])['residual'],innovations[0])
        first=torch.tensor(innovations[0]);second=torch.tensor(innovations[1])
        self.assertEqual(s.step([.2,-3.,1.],innovations[1])['residual'],(.95*first+math.sqrt(1-.95**2)*second).tolist())
        for epsilon in innovations[2:180]:s.step([.2,-3.,1.],epsilon)
        state=s.state();resumed=Sampler.restore(state,'wide8_persistent',std,innovations)
        for epsilon in innovations[180:]:self.assertEqual(s.step([-.3,-2.,2.],epsilon),resumed.step([-.3,-2.,2.],epsilon))
        bad=copy.deepcopy(state);bad['residual'][0]+=.1
        with self.assertRaisesRegex(ValueError,'mismatch'):Sampler.restore(bad,'wide8_persistent',std,innovations)
        with self.assertRaisesRegex(ValueError,'mismatch'):Sampler.restore(state,'wide8',std,innovations)

    def test_native_resets_use_actual_velocity_and_all_fixed_horizons(self):
        with Bridge(BRIDGE) as b:
            for row in candidates()[::4]:
                item=b.call('reset',tasks=[row['task']])['items'][0];trace=b.call('state')['traces'][0]
                self.assertTrue(task_matches(trace['task'],row['task']));self.assertLess(item['errors']['heading'],1e-6)
                m=exposure(item,trace['command']);self.assertAlmostEqual(m['forward'],row['initial_speed'],places=4)
                self.assertLess(m['lateral'],1e-5)
            task=copy.deepcopy(candidates()[24]['task']);task['start']['relative_velocity']={'x':0.,'y':0.}
            b.call('reset',tasks=[task])
            for _ in range(360):b.call('step',actions=[{'steering':0.,'thrust':0.,'brake':0.}])
            trace=b.call('state')['traces'][0];result=inspect(trace)
            self.assertTrue(result['motor_retained']);self.assertEqual(result['motor_acquired_tick'],30)
            self.assertEqual(result['heading_intervals'],360);self.assertFalse(result['exposed'])
            bad=copy.deepcopy(trace);bad['steps'][5]['item']['retention']['first_exit_tick']=6
            with self.assertRaises(AssertionError):inspect(bad)
            # Crossing the half-command bound at the first endpoint does not
            # count the preceding interval as qualified.
            task=copy.deepcopy(candidates()[0]['task']);h=task['start']['bearing']+task['start']['heading_offset']
            task['start']['relative_velocity']={'x':-math.sin(h)*2.99,'y':math.cos(h)*2.99}
            b.call('reset',tasks=[task])
            for tick in range(31):b.call('step',actions=[{'steering':0.,'thrust':1. if tick==0 else 0.,'brake':0.}])
            result=inspect(b.call('state')['traces'][0])
            self.assertEqual(result['first_exposure_tick'],31);self.assertEqual(result['longest_exposure_intervals'],30)

    def test_exposure_requires_direction_heading_spin_and_bounded_speed(self):
        item={'frame':{'velocity':{'x':0.,'y':6.}},'errors':{'heading':0.,'spin':0.}}
        command={'heading':0.,'speed':6.};self.assertTrue(exposure(item,command)['useful'])
        for axis,value in [('x',2.),('y',2.),('y',10.),('y',-6.)]:
            bad=copy.deepcopy(item);bad['frame']['velocity'][axis]=value;self.assertFalse(exposure(bad,command)['useful'])
        for axis,value in [('heading',HEADING+.001),('spin',.21)]:
            bad=copy.deepcopy(item);bad['errors'][axis]=value;self.assertFalse(exposure(bad,command)['useful'])

    def test_gate_requires_each_speed_heading_and_gain_without_claiming_tracking(self):
        rows=[{**r,'exposed':r['family']=='accelerate','heading_intervals':360,'outcome':'deadline',
               'motor_acquired_tick':None,'motor_retained':False} for r in candidates()]
        baseline=copy.deepcopy(rows)
        for r in baseline:r['exposed']=False
        self.assertTrue(seed_gate(rows,baseline)['passed']);self.assertEqual(seed_gate(rows,baseline)['motor_tracking_retained'],0)
        self.assertFalse(seed_gate(rows,rows)['passed'])
        bad=copy.deepcopy(rows)
        for r in bad:
            if r['level']==18:r['exposed']=False
        self.assertFalse(seed_gate(bad,baseline)['checks']['exposure'])
        bad=copy.deepcopy(rows)
        for r in bad:r['heading_intervals']=320
        self.assertFalse(seed_gate(bad,baseline)['checks']['heading'])
        bad=copy.deepcopy(rows);bad[0]['outcome']='collision';self.assertFalse(seed_gate(bad,baseline)['checks']['physical_safety'])


if __name__=='__main__':unittest.main(verbosity=2)
