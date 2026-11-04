"""Probe setup, native admission and independent complete-interval audit semantics."""
import copy
import math
import unittest
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.tests import BRIDGE
from tools.flight_audit.hold_robustness import VARIANTS, LIMITS, candidate, assert_initial, inspect_trace, groups


def synthetic(horizon, deadline, errors, initial=None):
    initial=initial or {'position':0.,'velocity':0.,'spin':0.};previous=initial;dwell=0;steps=[]
    for tick,e in enumerate(errors,1):
        valid=lambda x:all(x[k]<=v for k,v in LIMITS.items())
        dwell=dwell+1 if valid(previous) and valid(e) else 0
        outcome='success' if dwell>=horizon else 'deadline' if tick==deadline else None
        steps.append({'frame':{'errors':dict(e)},'action':{'steering':0.,'thrust':0.,'brake':0.},
                      'evidence':{k:False for k in ['collision','damage','ship_lost','boundary_escape']},
                      'progress':{'elapsed_ticks':tick,'dwell_ticks':dwell,'outcome':outcome}});previous=e
    return {'version':2,'task':{'arrival':dict(LIMITS,dwell_ticks=horizon),'deadline_ticks':deadline},'initial':{'errors':initial},'steps':steps}


class RobustnessTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.bridge=Bridge(BRIDGE)
    @classmethod
    def tearDownClass(cls):cls.bridge.close()

    def test_declared_coverage_and_repeatability(self):
        cases=[candidate(i,v) for v in VARIANTS for i in range(8)]
        self.assertEqual(len(VARIANTS),23);self.assertEqual(len({c['index'] for c in cases}),184)
        self.assertTrue(all(c['seed']==4410001 for c in cases))
        self.assertEqual(cases,[candidate(i,v) for v in VARIANTS for i in range(8)])
        self.assertEqual([sum(c['recipe'].endswith(f'stratum-{i}') for c in cases) for i in range(4)],[46]*4)

    def test_signed_body_axes_and_scopes(self):
        for scope,magnitude in [('inside',1.5),('recovery',4.)]:
            a=candidate(2,f'{scope}-position-forward-positive')['task'];b=candidate(2,f'{scope}-position-right-positive')['task'];n=candidate(2,f'{scope}-position-forward-negative')['task']
            x,y=a['goal_offset'],b['goal_offset'];self.assertAlmostEqual(math.hypot(*x.values()),magnitude)
            self.assertAlmostEqual(x['x']*y['x']+x['y']*y['y'],0.)
            self.assertEqual(n['goal_offset'],{k:-v for k,v in x.items()})
            self.assertEqual(a['arrival']['dwell_ticks'],300 if scope=='inside' else 30)
            self.assertEqual(a['deadline_ticks'],300)

    def test_native_geometric_admission_and_envelope_rejection(self):
        for v in VARIANTS:
            r=self.bridge.call('inspect',candidate=candidate(0,v));self.assertTrue(r['admitted'],r['rejection'])
            self.assertTrue(r['evidence']['full_hull_start_clear'] and r['evidence']['direct_clear'])
        c=candidate(0,'nominal-300');c['task']['arrival']['velocity']=2.
        self.assertFalse(self.bridge.call('inspect',candidate=c)['admitted'])

    def test_neutral_setup_matches_declared_post_prime_perturbations(self):
        cs=[candidate(0,v) for v in VARIANTS]
        for start in range(0,len(cs),12):
            chunk=cs[start:start+12];items=self.bridge.call('reset',tasks=[c['task'] for c in chunk])['items']
            for item,c in zip(items,chunk):assert_initial(item,c)
        wrong=copy.deepcopy(items[0]);wrong['frame']['errors']['spin']=.9
        with self.assertRaises(ValueError):assert_initial(wrong,chunk[0])

    def test_native_long_hold_requires_all_three_hundred_intervals(self):
        c=candidate(1,'nominal-300');self.bridge.call('reset',tasks=[c['task']])
        for tick in range(1,301):
            item=self.bridge.call('step',actions=[{'steering':0.,'thrust':0.,'brake':0.}])['items'][0]
            self.assertEqual(item['progress']['outcome'],'success' if tick==300 else None)
        m=inspect_trace(self.bridge.call('state')['traces'][0]);self.assertEqual(m['maximum_dwell'],300)

    def test_exit_breaks_sustained_hold_even_after_reentry(self):
        zero={'position':0.,'velocity':0.,'spin':0.};errors=[dict(zero) for _ in range(120)];errors[49]['velocity']=1.1
        m=inspect_trace(synthetic(120,120,errors));self.assertEqual(m['outcome'],'deadline');self.assertEqual(m['first_exit_tick'],50)
        self.assertEqual(m['maximum_dwell'],69)
        bad=synthetic(120,120,errors);bad['steps'][-1]['progress']['outcome']='success'
        with self.assertRaises(ValueError):inspect_trace(bad)

    def test_recovery_starts_dwell_only_after_first_eligible_frame(self):
        zero={'position':0.,'velocity':0.,'spin':0.};trace=synthetic(30,300,[zero]*31,dict(zero,velocity=1.5))
        m=inspect_trace(trace);self.assertEqual(m['first_entry_tick'],1);self.assertEqual(m['ticks'],31);self.assertEqual(m['outcome'],'success')
        bad=copy.deepcopy(trace);bad['task']['arrival']['spin']=.3
        with self.assertRaises(ValueError):inspect_trace(bad)
        bad=copy.deepcopy(trace);bad['steps'][-1]['evidence']['collision']=True
        with self.assertRaises(ValueError):inspect_trace(bad)
        bad['steps'][-1]['progress']['outcome']='collision';self.assertTrue(inspect_trace(bad)['physical_failure'])

    def test_scope_gates_need_each_variant_and_heading_coverage(self):
        rows=[{'index':j*8+i,'anchor':i,'variant':v,'outcome':'success','physical_failure':False} for j,v in enumerate(VARIANTS) for i in range(8)]
        _,g=groups(rows);self.assertTrue(all(g.values()))
        bad=copy.deepcopy(rows)
        for e in bad:
            if e['variant']=='inside-spin-angular-positive' and e['anchor']%4==0:e['outcome']='deadline'
        report,g=groups(bad);self.assertEqual(report['inside-spin-angular-positive']['successes'],6)
        self.assertFalse(g['inside_perturbations']);self.assertTrue(g['recovery'])
        with self.assertRaises(ValueError):groups(rows[:-1])


if __name__=='__main__':unittest.main()
