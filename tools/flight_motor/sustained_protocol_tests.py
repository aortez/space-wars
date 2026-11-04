"""LF-061 admits recorded training failures without weakening evaluation."""
import copy
from pathlib import Path
import unittest
from . import aligned,sustained_audit,storage
from .aligned_audit import gate

OLD=Path('data/training-experiments/learned-flight-motor-sustained-v1-20261025')


class SustainedProtocolTests(unittest.TestCase):
    def record(self):
        r=next(v for v in storage.read_json(OLD/'results.json')['records'] if v['arm']=='sustain' and v['seed']==17)
        state=storage.load_torch(OLD/'preflight/sustain/seed-17/checkpoint-0004.pt.gz')
        r['training_physical_failures']=sustained_audit.training_failures(state['collector']['completed'])
        return r

    def test_recorded_collision_counts_forgone_time_without_hiding_failure(self):
        r=self.record();before=copy.deepcopy(r)
        self.assertEqual(r['training_physical_failures'][0]['ticks'],691)
        self.assertEqual(r['training_physical_failures'][0]['forgone_continuation_ticks'],209)
        self.assertEqual(r['training_stats']['movement_ticks_after_360'],871)
        self.assertFalse(all(sustained_audit.preflight_checks(r,'strict-v1').values()))
        self.assertTrue(all(sustained_audit.preflight_checks(r,'record-physical-failures-v1').values()))
        self.assertEqual(r,before)

    def test_unexplained_missing_exposure_and_unknown_outcomes_rejected(self):
        r=self.record();r['training_physical_failures'][0]['forgone_continuation_ticks']=0
        self.assertFalse(all(sustained_audit.preflight_checks(r,'record-physical-failures-v1').values()))
        r['training_outcomes']['track']['broken']=1
        with self.assertRaises(ValueError):sustained_audit.preflight_checks(r,'record-physical-failures-v1')

    def test_recorded_physical_failure_keeps_terminal_penalty(self):
        t=storage.read_json(OLD/'preflight/sustain/seed-17/training-traces/episode-000003.json.gz')
        result=sustained_audit.inspect_training(t)
        self.assertEqual(result['outcome'],'collision')
        self.assertEqual(sustained_audit.reward(t['steps'][-1]['item'])['terminal'],-5.)

    def test_final_gate_still_rejects_single_physical_failure(self):
        final=[{**v,'retained':True,'outcome':'success','first_success_tick':30} for v in aligned.candidates(5910002,True)]
        initial=[{**v,'retained':False} for v in final]
        parent=[v for v in final if v['suite'].startswith('legacy')]
        self.assertTrue(gate(initial,final,parent)['passed'])
        final[0]={**final[0],'retained':False,'outcome':'collision'}
        result=gate(initial,final,parent)
        self.assertEqual(result['physical_failures'],1)
        self.assertFalse(result['passed']);self.assertFalse(result['movement_learning_passed'])


if __name__=='__main__':unittest.main(verbosity=2)
