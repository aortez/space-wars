"""Synthetic tests for matched velocity perturbations and force interpretation."""
import math
import unittest
import torch
from tools.flight_motor import sustained
from . import motor_feedback as audit


def inputs():
    x=torch.zeros(3,18);angles=torch.tensor([0.,.7,-1.1]);x[:,0]=angles.sin();x[:,1]=angles.cos();x[:,2]=.6
    x[:,3:5]=torch.tensor([[.1,.8],[-.1,.4],[.2,.1]]);x[:,5:7]=x[:,3:5]+torch.tensor([.01,-.02]);x[:,10:15]=1
    return x


class FeedbackTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1)

    def test_velocity_sweep_preserves_lateral_origin_offset_and_other_inputs(self):
        x=inputs();q=audit.direction(x);y=audit.alter(x,[6,12,18],[9,12,15])
        torch.testing.assert_close((y[:,3:5]*20*q).sum(1),torch.tensor([9.,12.,15.]))
        torch.testing.assert_close(y[:,5:7]-y[:,3:5],x[:,5:7]-x[:,3:5])
        lateral=torch.stack((q[:,1],-q[:,0]),dim=1)
        torch.testing.assert_close((y[:,3:5]*lateral).sum(1),(x[:,3:5]*lateral).sum(1))
        for i in [0,1,*range(7,18)]:self.assertTrue(torch.equal(x[:,i],y[:,i]))

    def test_derived_error_has_expected_sign_under_rotated_sweep(self):
        x=audit.alter(inputs(),[12]*3,[15]*3);q=audit.direction(x)
        e=sustained.residual_features(x)[:,18:20]*20
        torch.testing.assert_close((e*q).sum(1),torch.full((3,),-3.),atol=2e-6,rtol=1e-6)

    def test_brake_cap_and_governor_match_open_wing_control_law(self):
        x=inputs();x[:,0]=0;x[:,1]=1;x[:,3:7]=0
        x=audit.alter(x,[18]*3,[5,10,20]);act=torch.tensor([[.2,.2]]*3)
        torch.testing.assert_close(audit.acceleration(x,act),torch.tensor([5.,1.,1.]))
        x=audit.alter(x,[18]*3,[60,65,70]);act[:,1]=0
        torch.testing.assert_close(audit.acceleration(x,act),torch.tensor([9.,4.5,0.]))

    def test_decomposition_and_analytic_gradient_match_finite_difference(self):
        torch.manual_seed(5);m=sustained.ResidualModel({'version':1,'features':list(range(18))});m.requires_grad_(False)
        with torch.no_grad():m.residual.weight.uniform_(-.2,.2)
        x=inputs();c=audit.components(m,x)
        torch.testing.assert_close(c['logits'],m(x)[0][:,1:]);audit.derivatives(m,x)

    def test_zero_speed_residual_and_velocity_derivative_are_exactly_zero(self):
        m=sustained.ResidualModel({'version':1,'features':list(range(18))});m.requires_grad_(False);x=inputs();x[:,2]=0
        with torch.no_grad():m.residual.weight.fill_(.2)
        d=audit.derivatives(m,x)
        self.assertEqual(d['residual_raw_velocity'].count_nonzero(),0);self.assertEqual(d['residual_error'].count_nonzero(),0)
        torch.testing.assert_close(d['total'],d['base'])

    def test_underspeed_overspeed_and_heading_readiness(self):
        x=inputs();x[:,0]=0;x[:,1]=1;x[:,3:7]=0;x=audit.alter(x,[12]*3,[8,12,16])
        _,g=audit.training_groups(x);self.assertEqual(g['ready_under'].tolist(),[True,False,False]);self.assertEqual(g['ready_over'].tolist(),[False,False,True])
        x[2,7]=.2;_,g=audit.training_groups(x);self.assertFalse(g['ready_over'][2])

    def test_stopping_comparison_uses_identical_sigmoid_layout(self):
        m=sustained.ResidualModel({'version':1,'features':list(range(18))});m.requires_grad_(False)
        x=inputs().repeat(1296,1);x[:,2]=0
        with torch.no_grad():g=audit.components(m,x)
        self.assertTrue(torch.equal(g['logits'],g['base_logits']))
        self.assertTrue(torch.equal(g['actions'],g['base_logits'].contiguous().sigmoid()))
        self.assertEqual(g['residual_logits'].count_nonzero(),0)


if __name__=='__main__':unittest.main(verbosity=2)
