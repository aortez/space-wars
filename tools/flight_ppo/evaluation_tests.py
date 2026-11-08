"""Continuous evaluation parity, RNG isolation, version guards and resume.

The v1 archive-replay suite remains in its sealed source snapshot. Those exports
and action traces deliberately cannot run against the v2 physics/action contract.
"""
import copy
import gzip
import json
import os
from pathlib import Path
import random
import tempfile
import time
import unittest
from unittest.mock import patch
import torch
from .bridge import Bridge
from .checkpoint import identity
from .curriculum import generate
from .evaluate import evaluate
from .execution import DETERMINISTIC, SAMPLED, DESCRIPTOR, evaluation_spec, execution_identity, episode_seed, load_export
from .model import FlightModel, actions, tensor
from .tests import BRIDGE, equal_tree

SPEC={'mode':SAMPLED,'repeats':2,'seed_base':5310000}


class EvaluationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
        location=os.environ.get('FLIGHT_CONTINUOUS_TEST_ARTIFACTS')
        cls.temporary=None
        if location:
            cls.root=Path(tempfile.mkdtemp(prefix='evaluation-',dir=location))
        else:
            cls.temporary=tempfile.TemporaryDirectory();cls.root=Path(cls.temporary.name)
        with Bridge(BRIDGE) as bridge:
            cls.schema=bridge.contract['schema']
            _,pools=generate(bridge,4110002,1)
            cls.candidates=[copy.deepcopy(p[0]) for p in pools[:2]]
            for c in cls.candidates:c['task']['deadline_ticks']=30
            cls.model=FlightModel(cls.schema)
            cls.policy=cls.root/'policy.json';cls.model.export(cls.policy)

    @classmethod
    def tearDownClass(cls):
        if cls.temporary:cls.temporary.cleanup()

    def test_specification_seed_descriptor_and_legacy_guards(self):
        self.assertEqual(evaluation_spec(),{'mode':DETERMINISTIC,'repeats':1})
        for spec in [{},{'mode':'deterministic-v1','repeats':1},{'mode':'mean-steering-sampled-buttons-v1','repeats':1,'seed_base':1},dict(SPEC,repeats=True),dict(SPEC,repeats=0),dict(SPEC,repeats=11),dict(SPEC,seed_base=-1),dict(SPEC,seed_base=True),{'mode':DETERMINISTIC,'repeats':True},{'mode':DETERMINISTIC,'repeats':2}]:
            with self.assertRaises(ValueError):evaluation_spec(spec)
        self.assertEqual(episode_seed(SPEC,17,1,3),5481003)
        for learner,index in [(None,0),(True,0),(-1,0),(17,1000),(17,-1),(17,True)]:
            with self.assertRaises(ValueError):episode_seed(SPEC,learner,0,index)
        changed=json.loads(DESCRIPTOR.read_text());changed['native_fallback']=True
        path=self.root/'bad-descriptor.json';path.write_text(json.dumps(changed))
        with patch('tools.flight_ppo.execution.DESCRIPTOR',path):
            with self.assertRaises(ValueError):execution_identity(SPEC)
        with self.assertRaises(ValueError):
            evaluate(BRIDGE,self.policy,[self.candidates[0]]*2,self.root/'invalid',execution=SPEC,learner_seed=17)
        self.assertFalse((self.root/'invalid').exists())

    def test_export_matches_saved_tensors_and_rejects_old_or_malformed_data(self):
        with torch.random.fork_rng(devices=[]):
            model=load_export(self.policy,self.schema)
            self.assertTrue(equal_tree(model.state_dict(),self.model.state_dict()))
            self.assertFalse(model.training)
            self.assertTrue(all(not p.requires_grad for p in model.parameters()))
            for change in ['version','schema','shape','finite']:
                record=json.loads(self.policy.read_text())
                if change=='version':record['format_version']=1
                elif change=='schema':record['observation_schema']['version']=1
                elif change=='shape':record['weights']['log_std']['shape']=[1]
                else:record['weights']['actor.weight']['values'][0]=float('nan')
                path=self.root/f'bad-{change}.json';path.write_text(json.dumps(record))
                with self.assertRaises(ValueError):load_export(path,self.schema)

    def test_native_mean_evaluation_matches_python_actions_and_is_rng_neutral(self):
        trng=torch.get_rng_state();prng=random.getstate();out=self.root/'mean'
        evaluate(BRIDGE,self.policy,self.candidates[:1],out,time.monotonic()+60)
        self.assertTrue(torch.equal(trng,torch.get_rng_state()));self.assertEqual(prng,random.getstate())
        c=self.candidates[0];trace=json.loads((out/f"{c['family']}-{c['index']}.json").read_text())
        with Bridge(BRIDGE) as bridge:
            item=bridge.call('reset',tasks=[c['task']])['items'][0]
            with torch.no_grad():actor,_=self.model(tensor([item]))
            expected=actions(actor[:,0],actor[:,1:])[0]
        for k,v in expected.items():self.assertAlmostEqual(trace['steps'][0]['action'][k],v,places=5)

    def test_sampled_evaluation_repeats_are_order_independent_rng_neutral_and_replayable(self):
        trng=torch.get_rng_state();prng=random.getstate()
        for label,candidates in [('forward',self.candidates),('reverse',list(reversed(self.candidates)))]:
            out=self.root/label
            rows=evaluate(BRIDGE,self.policy,candidates,out,time.monotonic()+60,execution=SPEC,learner_seed=17)
            self.assertEqual(len(rows),4)
            self.assertTrue(torch.equal(trng,torch.get_rng_state()));self.assertEqual(prng,random.getstate())
            for row in rows:
                name=f"{row['family']}-{row['index']}";directory=out/f"repeat-{row['repeat']}"
                trace=json.loads((directory/f'{name}.json').read_text())
                with gzip.open(directory/'policy'/f'{name}.json.gz','rt') as f:policy=json.load(f)
                self.assertEqual([p['action'] for p in policy],[s['action'] for s in trace['steps']])
                with Bridge(BRIDGE) as bridge:
                    restored=bridge.call('restore',traces=[trace])['items'][0]
                    self.assertEqual(restored['progress'],trace['steps'][-1]['progress'])
                if label=='reverse':
                    other=self.root/'forward'/f"repeat-{row['repeat']}"
                    self.assertEqual(trace,json.loads((other/f'{name}.json').read_text()))
                    with gzip.open(other/'policy'/f'{name}.json.gz','rt') as f:self.assertEqual(policy,json.load(f))

    def test_timeout_restores_rng_and_marks_incomplete_evaluation(self):
        trng=torch.get_rng_state();prng=random.getstate();out=self.root/'timeout'
        with self.assertRaises(TimeoutError):evaluate(BRIDGE,self.policy,self.candidates[:1],out,0,execution=SPEC,learner_seed=17)
        self.assertTrue(torch.equal(trng,torch.get_rng_state()));self.assertEqual(prng,random.getstate())
        self.assertFalse((out/'results.json').exists())
        self.assertEqual(json.loads((out/'metadata/interruption.json').read_text())['completed_episodes'],[])

    def test_evaluation_between_updates_preserves_complete_training_state(self):
        from .collector import Collector
        from .ppo import update
        from .checkpoint import save
        config=json.loads(Path('tools/flight_ppo/smoke-config.json').read_text())
        config.update(updates=2,rollout_ticks=8,epochs=2,minibatch=16,
                      evaluation=dict(SPEC,repeats=1))
        states=[]
        for name,enabled in [('evaluated',True),('skipped',False)]:
            torch.manual_seed(321);random.seed(654)
            out=self.root/name;out.mkdir()
            with Bridge(BRIDGE) as bridge:
                model=FlightModel(bridge.contract['schema'])
                optimizer=torch.optim.Adam(model.parameters(),lr=config['learning_rate'],eps=1e-5)
                collector=Collector(bridge,[[copy.deepcopy(self.candidates[i%2])] for i in range(4)],out/'traces')
                contract=identity(config,bridge);history=[]
                save(out/'checkpoint-0000.pt',contract,model,optimizer,collector,0,history)
                for n in [1,2]:
                    history.append(update(model,optimizer,collector.collect(model,config),config))
                    model.export(out/f'policy-{n:04}.json')
                    if n==1 and enabled:
                        evaluate(BRIDGE,out/f'policy-{n:04}.json',self.candidates,out/'evaluation',
                                 time.monotonic()+60,execution=config['evaluation'],learner_seed=321)
                    save(out/f'checkpoint-{n:04}.pt',contract,model,optimizer,collector,n,history)
                states.append(torch.load(out/'checkpoint-0002.pt',weights_only=True))
        self.assertTrue(equal_tree(*states))

    def test_checkpoint_identity_requires_explicit_continuous_contract(self):
        config=json.loads(Path('tools/flight_ppo/smoke-config.json').read_text())
        with Bridge(BRIDGE) as bridge:
            contract=identity(config,bridge)
            self.assertEqual(contract['format_version'],2)
            self.assertEqual(contract['action_contract'],'continuous-flight-v2')
            del config['action_contract']
            with self.assertRaises(ValueError):identity(config,bridge)


if __name__=='__main__':unittest.main(verbosity=2)
