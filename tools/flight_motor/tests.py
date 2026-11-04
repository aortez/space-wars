"""Motor observation, native parity, optimizer isolation and exact continuation."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.model import actions
from tools.flight_ppo.tests import equal_tree
from .model import MotorModel,tensor
from .task import candidates,config,reward
from .run import identity,restore
from .audit import gate,inspect

BRIDGE='target/ci/examples/flight_motor_bridge'


class MotorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1);torch.use_deterministic_algorithms(True)

    def test_frozen_matrices_cover_both_signs_spins_and_disjoint_turn_angles(self):
        a=candidates(5310001);b=candidates(5310002,True)
        for rows in [a,b]:
            self.assertEqual(len(rows),32)
            self.assertEqual(sum(r['family']=='turn' for r in rows),24)
            self.assertEqual(sum(r['direction']=='left' and r['family']=='turn' for r in rows),12)
            self.assertEqual(len({r['task']['start']['spin'] for r in rows[:24]}),3)
            for i in range(4):self.assertEqual(len(rows[i::4]),8)
        self.assertFalse({r['task']['heading_delta'] for r in a[:24]} & {r['task']['heading_delta'] for r in b[:24]})
        self.assertEqual(a,candidates(5310001))

    def test_all_candidates_reset_and_command_direction_matches_native_inputs(self):
        with Bridge(BRIDGE) as b:
            for rows in [candidates(5310001),candidates(5310002,True)]:
                for row in rows:
                    item=b.call('reset',tasks=[row['task']])['items'][0]
                    self.assertAlmostEqual(item['errors']['heading'],abs(row['task']['heading_delta']),places=5)
                    self.assertEqual(item['progress']['elapsed_ticks'],0)
                    self.assertEqual(len(item['inputs']),18)

    def test_model_size_and_actor_critic_gradient_paths_are_independent(self):
        with Bridge(BRIDGE) as b:model=MotorModel(b.contract['schema'])
        self.assertEqual(sum(p.numel() for p in model.parameters()),3463)
        x=torch.randn(8,18);actor,value=model(x);value.sum().backward()
        self.assertTrue(all(p.grad is None for name,p in model.named_parameters() if name.startswith('actor')))
        model.zero_grad(set_to_none=True);model(x)[0].sum().backward()
        self.assertTrue(all(p.grad is None for name,p in model.named_parameters() if name.startswith('critic')))
        self.assertEqual(float(model.actor.bias.detach()[1]),-4.)

    def test_native_export_matches_and_rejects_malformed_weights(self):
        with tempfile.TemporaryDirectory() as d,Bridge(BRIDGE) as b:
            p=Path(d)/'policy.json';torch.manual_seed(17);model=MotorModel(b.contract['schema']);model.export(p)
            items=b.call('reset',tasks=[r['task'] for r in candidates(5310002,True)[:16]])['items']
            b.call('load_policy',path=str(p));native=b.call('infer',inputs=[i['inputs'] for i in items])['outputs']
            with torch.no_grad():actor,value=model(tensor(items))
            torch.testing.assert_close(torch.tensor([[n[k] for k in ['steering_latent','thrust_logit','brake_logit','value']] for n in native]),torch.cat((actor,value[:,None]),1),atol=1e-5,rtol=1e-5)
            for command,n in zip(actions(actor[:,0],actor[:,1:]),native):
                self.assertLess(max(abs(command[k]-n['action'][k]) for k in command),1e-5)
            good=json.loads(p.read_text())
            for change in ['shape','missing','extra','schema']:
                bad=copy.deepcopy(good)
                if change=='shape':bad['weights']['actor.weight']['shape']=[1,96]
                elif change=='missing':del bad['weights']['actor.bias']
                elif change=='extra':bad['weights']['extra']={'shape':[1],'values':[0]}
                else:bad['observation_schema']['version']=9
                atomic_json(p,bad)
                with self.assertRaises(RuntimeError):b.call('load_policy',path=str(p))
            with self.assertRaises(RuntimeError):b.call('infer',inputs=[[0]*17])

    def test_dense_cost_and_gates_do_not_count_reward_as_success(self):
        item={'errors':{'heading':0.,'velocity':0.,'spin':0.},'progress':{'outcome':None}}
        self.assertEqual(reward(item)['total'],0.)
        near=copy.deepcopy(item);near['errors']['heading']=.1
        far=copy.deepcopy(item);far['errors']['heading']=.2
        self.assertLess(reward(far)['total'],reward(near)['total'])
        item['progress']['outcome']='collision';self.assertEqual(reward(item)['total'],-5.)
        initial={'turn_successes':0};final={'turn_successes':20,'left':10,'right':10,'immediate_holds':8,'physical_failures':0}
        self.assertTrue(all(gate(initial,final).values()))
        final['left']=8;self.assertFalse(gate(initial,final)['turn_competence'])
        final['physical_failures']=1;self.assertFalse(gate(initial,final)['physical_safety'])

    def test_native_trace_dwell_replay_and_batch_rejection(self):
        with Bridge(BRIDGE) as b:
            row=candidates(5310001)[24];b.call('reset',tasks=[row['task']])
            before=b.call('state')
            with self.assertRaises(RuntimeError):b.call('step',actions=[{'steering':2.,'thrust':0.,'brake':0.}])
            self.assertEqual(before,b.call('state'))
            for _ in range(30):b.call('step',actions=[{'steering':0.,'thrust':0.,'brake':0.}])
            trace=b.call('state')['traces'][0];self.assertEqual(inspect(trace)['outcome'],'success')
            expected=b.call('restore',traces=[trace]);self.assertEqual(expected['items'][0],trace['steps'][-1]['item'])
            bad=copy.deepcopy(trace);bad['steps'][10]['item']['progress']['dwell_ticks']=100
            with self.assertRaises(AssertionError):inspect(bad)
            with self.assertRaises(RuntimeError):b.call('restore',traces=[bad])

    def test_fresh_process_resume_preserves_complete_state_and_rollout(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);c=config(17);atomic_json(p/'config.json',c)
            command=[sys.executable,'-m','tools.flight_motor.run','--bridge',BRIDGE,'--config',str(p/'config.json'),'--stop-after','2','--skip-evaluation']
            subprocess.run(command+['--out',str(p/'first')],check=True,capture_output=True,timeout=90)
            subprocess.run(command+['--out',str(p/'again'),'--resume',str(p/'first/checkpoint-0001.pt')],check=True,capture_output=True,timeout=90)
            first=torch.load(p/'first/checkpoint-0002.pt',weights_only=True);again=torch.load(p/'again/checkpoint-0002.pt',weights_only=True)
            self.assertTrue(equal_tree(first,again))
            self.assertTrue(equal_tree(torch.load(p/'first/rollout-0002.pt',weights_only=True),torch.load(p/'again/rollout-0002.pt',weights_only=True)))
            with Bridge(BRIDGE) as b:
                bad=identity(c,b);bad['config']={**c,'learning_rate':.01}
                with self.assertRaisesRegex(ValueError,'mismatch'):restore(p/'first/checkpoint-0002.pt',bad,b,p/'unused')
            self.assertFalse((p/'unused').exists())


if __name__=='__main__':unittest.main(verbosity=2)
