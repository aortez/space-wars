"""Procedural bearing geometry, immutable controls, stage scope and exact continuation."""
import copy
import json
import math
from pathlib import Path
import random
import tempfile
import unittest
from unittest import mock
import torch
from tools.flight_ppo import curriculum, tests as policy_tests
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import load
from tools.flight_ppo.tests import BRIDGE
from tools.flight_audit.bearing_curriculum import assert_control_replay, CHANGED_SOURCES, stage_candidates, stage_summary, benefit
from tools.flight_audit.recovery_response import local_goal


class Accept:
    def __init__(self,reject=None):self.reject=reject;self.calls=0
    def call(self,command,**args):
        assert command=='inspect';self.calls+=1;c=args['candidate']
        return {'candidate':c,'admitted':c['index']!=self.reject}


class BearingCurriculumTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1);torch.use_deterministic_algorithms(True)

    def config(self):
        c=json.loads(Path('data/training-experiments/learned-flight-family-centering-v1-20261024/configs/global/17.json').read_text())
        c['position_curriculum']={'kind':curriculum.BEARING_CURRICULUM,'stage':0}
        return c

    def test_only_position_goal_bearing_changes_and_no_global_rng_is_consumed(self):
        rng=random.getstate();torch_rng=torch.get_rng_state().clone()
        for seed in [4510001,4510002]:
            for index in range(64):
                old=curriculum.settle_candidate(seed,index)
                for stage in range(5):
                    new=curriculum.bearing_candidate(seed,index,stage)
                    if index%4!=1 or stage==4:self.assertEqual(old,new)
                    else:
                        normalized=copy.deepcopy(new);normalized['task']['goal_offset']=old['task']['goal_offset']
                        self.assertEqual(normalized,old)
                        self.assertAlmostEqual(math.hypot(*old['task']['goal_offset'].values()),math.hypot(*new['task']['goal_offset'].values()),places=12)
        self.assertEqual(rng,random.getstate());self.assertTrue(torch.equal(torch_rng,torch.get_rng_state()))

    def test_signed_corridor_margins_and_balanced_case_matrix(self):
        for stage,margin in enumerate(curriculum.BEARING_MARGINS_DEGREES):
            signs=[]
            for c in stage_candidates(4510002,stage):
                t=c['task'];d=math.hypot(*t['goal_offset'].values());s=t['start'];angle=s['bearing']+s['heading_offset']
                dx,dy=t['goal_offset']['x'],t['goal_offset']['y'];x=math.cos(angle)*dx+math.sin(angle)*dy;y=-math.sin(angle)*dx+math.cos(angle)*dy
                bearing=math.atan2(x,y);signs.append(bearing<0)
                self.assertAlmostEqual(math.degrees(abs(bearing)-math.asin(3/d)),margin,places=11)
                if stage==0:self.assertGreater(abs(x),3.)
                self.assertEqual(t['arrival']['dwell_ticks'],30);self.assertEqual(t['deadline_ticks'],900)
            self.assertEqual(sum(signs),8)
        for index in range(64):self.assertEqual(curriculum.bearing_candidate(4510001,index,4),curriculum.settle_candidate(4510001,index))

    def test_training_only_transform_keeps_legacy_and_development_batches_exact(self):
        config=self.config();base=copy.deepcopy(config);del base['position_curriculum']
        for partition in ['development','heldout']:
            self.assertEqual(curriculum.sample(Accept(),config,partition),curriculum.sample(Accept(),base,partition))
        a,p=curriculum.sample(Accept(),config,'train');b,q=curriculum.sample(Accept(),base,'train')
        self.assertEqual(a['position_curriculum'],config['position_curriculum']);self.assertNotIn('position_curriculum',b)
        for slot in [0,2,3]:self.assertEqual(p[slot],q[slot])
        self.assertNotEqual(p[1],q[1]);self.assertEqual(len(p[1]),16)

    def test_invalid_curriculum_rejected_before_bridge_or_output_and_resume_is_strict(self):
        from tools.flight_ppo.run import main
        bad=[None,True,{}, {'kind':curriculum.BEARING_CURRICULUM,'stage':True},
             {'kind':curriculum.BEARING_CURRICULUM,'stage':5},{'kind':'wrong','stage':0},
             {'kind':curriculum.BEARING_CURRICULUM,'stage':0,'extra':1}]
        with tempfile.TemporaryDirectory() as directory:
            p=Path(directory)
            for spec in bad:
                config=self.config();config['position_curriculum']=spec
                with self.assertRaises(ValueError):curriculum.sample(Accept(),config,'train')
                (p/'c.json').write_text(json.dumps(config))
                with mock.patch('sys.argv',['run','--bridge','unused','--config',str(p/'c.json'),'--out',str(p/'out')]):
                    with self.assertRaises(ValueError):main()
                self.assertFalse((p/'out').exists())
            for change in [{'task_recipe':'hold-position-v1'},{'recovery_dwell_ticks':300}]:
                config=self.config();config.update(change)
                with self.assertRaises(ValueError):curriculum.sample(Accept(),config,'train')
            torch.save({'contract':{'stage':0}},p/'s.pt')
            with self.assertRaisesRegex(ValueError,'mismatch'):load(p/'s.pt',{'stage':1},None)

    def test_rejected_candidate_is_retained_without_replacement(self):
        bridge=Accept(reject=5);batch,pools=curriculum.sample(bridge,self.config(),'train')
        self.assertFalse(batch['complete']);self.assertEqual(bridge.calls,64)
        self.assertEqual(len(batch['attempts']),64);self.assertEqual(len(pools[1]),15)
        self.assertEqual([v['candidate']['index'] for v in batch['attempts'] if not v['admitted']],[5])

    def test_native_reset_preserves_turn_required_geometry(self):
        cs=stage_candidates(4510002,0)
        with Bridge(BRIDGE) as bridge:
            items=bridge.call('reset',tasks=[c['task'] for c in cs])['items']
        for item in items:
            frame=item['frame'];x,y=local_goal(frame);d=math.hypot(x,y)
            self.assertGreater(abs(x),3.)
            self.assertAlmostEqual(math.degrees(abs(math.atan2(x,y))-math.asin(3/d)),5.,delta=.002)
            self.assertEqual(frame['errors']['velocity'],0.);self.assertEqual(frame['errors']['spin'],0.)

    def test_fresh_process_curriculum_resume_is_exact(self):
        config=self.config();del config['warm_start_sha256']
        config.update(updates=2,rollout_ticks=8,epochs=2,minibatch=16,train_tasks_per_stratum=1)
        with Bridge(BRIDGE) as self.bridge:
            policy_tests.PolicyTests.check_continuation(self,config)

    def records(self):
        records=[]
        for seed in [17,29,43]:
            for arm in ['broad','curriculum']:
                stage={'successes':14 if arm=='curriculum' else 9,'left':7,'right':7,'physical_failures':0}
                records.append({'seed':seed,'arm':arm,'stage':{'initial':{'successes':8},'final':stage},
                    'legacy_hold':{'final':{'immediate_holds':16,'immediate_holds_by_stratum':[4]*4,'physical_failures':0}},
                    'primary':{'initial':{'successes':10},'final':{'successes':11,'physical_failures':0}},
                    'all_position_training_cases_completed':True})
        return records

    def test_stage_gate_requires_learning_left_right_retention_in_same_seeds(self):
        rows=self.records();self.assertTrue(benefit(rows)['passed'])
        for r in rows:
            if r['arm']=='curriculum':r['stage']['final']['left']=5
        self.assertFalse(benefit(rows)['passed'])
        rows=self.records()
        for r in rows:
            if r['arm']=='curriculum' and r['seed']==17:r['stage']['final']['successes']=8
            if r['arm']=='curriculum' and r['seed']==43:r['legacy_hold']['final']['immediate_holds']=11
        self.assertFalse(benefit(rows)['passed'])
        rows=self.records()
        for r in rows:r['all_position_training_cases_completed']=False
        self.assertFalse(benefit(rows)['passed'])
        with self.assertRaises(AssertionError):stage_summary([{'index':1,'outcome':'success'}]*16)

    def test_control_replay_normalizes_only_declared_source_identity(self):
        old={'contract':{'python_sources':{k:'old' for k in CHANGED_SOURCES},'config':{},'bridge_sha256':'same'},'model':{'x':torch.tensor([1.])}}
        new=copy.deepcopy(old);new['contract']['python_sources']={k:'new' for k in CHANGED_SOURCES}
        assert_control_replay(new,old)
        new['contract']['config']['position_curriculum']={'kind':curriculum.BEARING_CURRICULUM,'stage':0}
        with self.assertRaises(AssertionError):assert_control_replay(new,old)


if __name__=='__main__':unittest.main(verbosity=2)
