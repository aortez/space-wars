"""Synthetic gate, complete-interval, continuous-action and pairing checks."""
import copy
import unittest
from .continuous_approach import CRITERIA, RECIPES, SAMPLE_SPEC, SAMPLED, aggregate, comparison, episode_seed, physical, trace_metrics


def rows(successes=0, sampled=False):
    return [{'index':i, 'recipe':RECIPES[i%4], 'outcome':'success' if i<successes else 'deadline',
        'initial_errors':{'position':5.,'velocity':0.,'spin':0.}, 'collision':False,'damage':False,
        **({'repeat':r,'execution_mode':SAMPLED,'rng_seed':episode_seed(SAMPLE_SPEC,17,r,i)} if sampled else {})}
        for r in range(4 if sampled else 1) for i in range(16)]


def trace():
    frame={'errors':{'position':2.,'velocity':.5,'spin':.1}}
    return {'version':2, 'task':{'arrival':CRITERIA,'deadline_ticks':1800}, 'initial':copy.deepcopy(frame),
        'steps':[{'action':{'steering':.1,'thrust':.25,'brake':.75},'frame':copy.deepcopy(frame),
            'evidence':{k:False for k in ['collision','damage','ship_lost','boundary_escape']},
            'progress':{'elapsed_ticks':i,'dwell_ticks':i,'outcome':'success' if i==30 else None}}
            for i in range(1,31)]}


class ContinuousApproachTests(unittest.TestCase):
    def test_exact_native_float32_tolerance_is_not_a_changed_physical_criterion(self):
        import struct
        self.assertEqual(CRITERIA['spin'],struct.unpack('f',struct.pack('f',.2))[0])
        t=trace();t['task']['arrival']=dict(CRITERIA,spin=.201)
        with self.assertRaises(ValueError):trace_metrics(t)

    def test_mean_gates_require_learning_coverage_and_physical_safety(self):
        initial=rows();final=rows(12)
        result=comparison(initial,final,17)
        self.assertTrue(result['learning_evidence']);self.assertTrue(result['scoped_progression'])
        self.assertFalse(comparison(final,final,17)['learning_evidence'])
        final[0]['collision']=True
        self.assertFalse(comparison(initial,final,17)['scoped_progression'])
        final=rows(14)
        for r in final:
            if r['recipe']==RECIPES[3]:r['outcome']='deadline'
        self.assertFalse(comparison(initial,final,17)['scoped_progression'])

    def test_aggregate_requires_two_models_and_all_completed(self):
        good=comparison(rows(),rows(12),17);bad=comparison(rows(),rows(),17)
        records=[{'seed':seed,'status':'completed','mean':good if seed<40 else bad} for seed in [17,29,43]]
        self.assertTrue(aggregate(records)['scoped_progression'])
        records[1]['mean']=bad;self.assertFalse(aggregate(records)['learning_evidence'])
        records[1]['mean']=good;records[2]={'seed':43,'status':'incomplete_or_failed'}
        self.assertFalse(aggregate(records)['scoped_progression'])

    def test_sampled_checks_seeds_tasks_and_never_provides_primary_gate(self):
        a,b=rows(0,True),rows(12,True)
        r=comparison(a,b,17,True);self.assertEqual(r['gain'],48);self.assertNotIn('scoped_progression',r)
        b[0]['rng_seed']+=1
        with self.assertRaises(ValueError):physical(b,17,True)
        b=rows(12,True);b[1]['index']=b[0]['index']
        with self.assertRaises(ValueError):physical(b,17,True)
        b=rows(12);b[0]['initial_errors']['position']=6.
        with self.assertRaises(ValueError):comparison(rows(),b,17)

    def test_success_requires_preceding_frame_and_all_complete_intervals(self):
        t=trace();self.assertEqual(trace_metrics(t)['maximum_dwell_ticks'],30)
        t['initial']['errors']['position']=4.
        with self.assertRaises(ValueError):trace_metrics(t)
        t=trace();t['steps'][-10]['frame']['errors']['spin']=.3
        with self.assertRaises(ValueError):trace_metrics(t)
        t=trace();t['steps'][-1]['evidence']['damage']=True
        with self.assertRaises(ValueError):trace_metrics(t)

    def test_fractional_metrics_reject_binary_invalid_and_incomplete_records(self):
        t=trace();r=trace_metrics(t)
        self.assertEqual(r['controls']['thrust']['sum'],7.5)
        for value in [True,False,-.01,1.01,float('nan')]:
            bad=copy.deepcopy(t);bad['steps'][0]['action']['thrust']=value
            with self.assertRaises(ValueError):trace_metrics(bad)
        t['steps']=t['steps'][:10]
        with self.assertRaises(ValueError):trace_metrics(t)
        self.assertIsNone(trace_metrics(t,allow_active=True)['outcome'])


if __name__=='__main__':unittest.main(verbosity=2)
