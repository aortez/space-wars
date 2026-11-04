"""Stage identity, complete velocity coverage, native coordinates and qualification."""
import copy
import math
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from .tests import BRIDGE
from .model import MotorModel
from .run import identity, restore
from .velocity import candidates, config, initialize, reward, MODES
from .velocity_audit import gate, task_matches, inspect

PARENT=Path('data/training-experiments/learned-flight-motor-turn-v1-20261025/pilot')


class VelocityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1)

    def test_complete_matrices_and_native_command_coordinates(self):
        with Bridge(BRIDGE) as b:
            for development,seed in [(False,5510001),(True,5510002)]:
                rows=candidates(seed,development);self.assertEqual(len(rows),104)
                self.assertEqual([len(rows[i::4]) for i in range(4)],[26]*4)
                self.assertEqual(len({r['task']['start']['seed'] for r in rows}),104)
                self.assertEqual(sum(r['family']=='brake' for r in rows),24)
                self.assertEqual(sum(r['family']=='track' for r in rows),48)
                for row in rows:
                    item=b.call('reset',tasks=[row['task']])['items'][0]
                    self.assertAlmostEqual(item['errors']['heading'],abs(row['task']['heading_delta']),places=5)
                    if row['family'] not in ['brake','track']:continue
                    t=row['task'];s=t['start'];heading=s['bearing']+s['heading_offset']+t['heading_delta']
                    trace=b.call('state')['traces'][0]
                    self.assertTrue(task_matches(trace['task'],t))
                    wrong=copy.deepcopy(t);wrong['speed']+=1
                    self.assertFalse(task_matches(trace['task'],wrong))
                    self.assertAlmostEqual(math.sin(trace['command']['heading']),math.sin(heading),places=5)
                    desired=(-math.sin(heading)*t['speed'],math.cos(heading)*t['speed'])
                    error=math.hypot(s['relative_velocity']['x']-desired[0],s['relative_velocity']['y']-desired[1])
                    self.assertAlmostEqual(item['errors']['velocity'],error,places=4)
                    self.assertAlmostEqual(item['inputs'][2],t['speed']/20,places=6)
            row=candidates(5510002,True)[24]
            b.call('reset',tasks=[row['task']])
            for _ in range(40):b.call('step',actions=[{'steering':0.,'thrust':.2,'brake':0.}])
            result=inspect(b.call('state')['traces'][0])
            self.assertEqual(result['ticks'],40);self.assertGreater(result['tail_velocity_rms'],0.)

    def test_initialization_only_copies_weights_and_rejects_wrong_source(self):
        with Bridge(BRIDGE) as b:
            for seed in [17,29,43]:
                p=PARENT/f'seed-{seed}/checkpoint-0064.pt';model=MotorModel(b.contract['schema'])
                rng=torch.get_rng_state().clone();initialize(model,p,config(seed))
                self.assertTrue(torch.equal(rng,torch.get_rng_state()))
                self.assertTrue(equal_tree(model.state_dict(),torch.load(p,weights_only=True)['model']))
                self.assertEqual(torch.optim.Adam(model.parameters()).state_dict()['state'],{})
                bad=copy.deepcopy(config(seed));bad['initialization']['parent_sha256']='wrong'
                with self.assertRaisesRegex(ValueError,'hash'):initialize(model,p,bad)
                with tempfile.TemporaryDirectory() as d:
                    with self.assertRaisesRegex(ValueError,'mismatch'):restore(p,identity(config(seed),b),b,Path(d)/'unused')
                    self.assertFalse((Path(d)/'unused').exists())

    def test_velocity_cost_and_joint_gate_reject_braking_only_and_heading_loss(self):
        item={'errors':{'heading':0.,'velocity':0.,'spin':0.},'progress':{'outcome':None}}
        self.assertEqual(reward(item)['total'],0.)
        item['errors']['velocity']=5.;self.assertAlmostEqual(reward(item)['total'],-1/60)
        rows=[{**r,'retained':True,'outcome':'success','first_success_tick':30 if r['family']=='hold' else 100} for r in candidates(5510002,True)]
        initial=copy.deepcopy(rows)
        for r in initial:
            if r['family']=='track':r.update(retained=False,outcome='deadline')
        self.assertTrue(gate(initial,rows)['passed'])
        bad=copy.deepcopy(rows);bad[72].update(retained=False,outcome='deadline')
        self.assertFalse(gate(initial,bad)['checks']['heading_retention'])
        bad=copy.deepcopy(rows)
        for r in bad:
            if r['family']=='track':r.update(retained=False,outcome='deadline')
        self.assertFalse(gate(initial,bad)['passed'])
        self.assertFalse(gate(rows,rows)['checks']['tracking_gain'])

    def test_new_stage_exact_resume_and_reward_collection(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);atomic_json(p/'config.json',config(17))
            cmd=[sys.executable,'-m','tools.flight_motor.run','--bridge',BRIDGE,'--config',str(p/'config.json'),'--stop-after','2','--skip-evaluation']
            subprocess.run(cmd+['--out',str(p/'first'),'--initialize-from',str(PARENT/'seed-17/checkpoint-0064.pt')],check=True,capture_output=True,timeout=90)
            subprocess.run(cmd+['--out',str(p/'again'),'--resume',str(p/'first/checkpoint-0001.pt')],check=True,capture_output=True,timeout=90)
            for name in ['checkpoint-0002.pt','rollout-0002.pt']:
                self.assertTrue(equal_tree(torch.load(p/'first'/name,weights_only=True),torch.load(p/'again'/name,weights_only=True)))
            zero=torch.load(p/'first/checkpoint-0000.pt',weights_only=True)
            self.assertEqual(zero['optimizer']['state'],{});self.assertEqual(zero['collector']['steps'],0)
            state=torch.load(p/'first/checkpoint-0002.pt',weights_only=True)['collector']
            for active,trace in zip(state['active'],state['traces']):
                self.assertAlmostEqual(active['reward'],sum(reward(s['item'])['total'] for s in trace['steps']),places=10)


if __name__=='__main__':unittest.main(verbosity=2)
