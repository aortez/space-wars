"""Short-stage task isolation and gates must fail closed on missing experience."""
import copy
import unittest
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.tests import BRIDGE
from tools.flight_ppo.curriculum import generate,sample,SETTLE_RECIPE,HOLD_RECIPE
from tools.flight_audit.short_recovery import pilot_gate,progression,SEEDS,FAMILIES


def records():
    coverage={f:{'directions':list(range(2 if f=='spin' else 4)),'correction_rows':300,'eligible_before':30} for f in FAMILIES}
    return [{'seed':seed,'training':{'coverage':copy.deepcopy(coverage),'outcomes':{'success':1}},
             'primary':{'initial':{'successes':10},'final':{'successes':12,'by_family':dict.fromkeys(FAMILIES,3),'physical_failures':0}},
             'legacy_hold':{'final':{'immediate_holds':12,'immediate_holds_by_stratum':[3]*4,'physical_failures':0}}} for seed in SEEDS]

class ShortRecoveryTests(unittest.TestCase):
    def test_only_requested_dwell_changes(self):
        class Pass:
            def call(self,op,candidate):return {'candidate':candidate,'admitted':True}
        for seed,n in [(4510001,16),(4510002,4)]:
            old,_=generate(Pass(),seed,n,recipe=SETTLE_RECIPE)
            explicit,_=generate(Pass(),seed,n,recipe=SETTLE_RECIPE,recovery_dwell_ticks=300)
            self.assertEqual(old,explicit)
            short,_=generate(Pass(),seed,n,recipe=SETTLE_RECIPE,recovery_dwell_ticks=30)
            for x in short['attempts']:
                self.assertEqual(x['candidate']['task']['arrival']['dwell_ticks'],30)
                x['candidate']['task']['arrival']['dwell_ticks']=300
            self.assertEqual(old,short)

    def test_invalid_overrides_fail_before_physics(self):
        for value in [0,120,True,30.,'30']:
            with self.assertRaises(ValueError):generate(None,1,1,recipe=SETTLE_RECIPE,recovery_dwell_ticks=value)
        with self.assertRaises(ValueError):generate(None,1,1,recipe=HOLD_RECIPE,recovery_dwell_ticks=30)
        with self.assertRaises(ValueError):sample(None,{'task_recipe':HOLD_RECIPE,'recovery_dwell_ticks':30},'train')

    def test_native_short_matrix_and_default_match(self):
        with Bridge(BRIDGE) as bridge:
            old,_=generate(bridge,4510002,4,recipe=SETTLE_RECIPE)
            short,_=generate(bridge,4510002,4,recipe=SETTLE_RECIPE,recovery_dwell_ticks=30)
            self.assertTrue(old['complete'] and short['complete'])
            for x,y in zip(old['attempts'],short['attempts']):
                self.assertAlmostEqual(x['evidence']['reference_time_seconds']-y['evidence']['reference_time_seconds'],4.5,places=5)
                y['evidence']['reference_time_seconds']=x['evidence']['reference_time_seconds']
                y['candidate']['task']['arrival']['dwell_ticks']=300
                self.assertEqual(x,y)

    def test_exposure_alone_cannot_pass_pilot(self):
        rows=records();self.assertTrue(pilot_gate(rows)['passed'])
        rows[0]['training']['outcomes']={};self.assertFalse(pilot_gate(rows)['passed'])
        rows=records()
        for r in rows[:2]:r['training']['coverage']['position']['eligible_before']=29
        self.assertFalse(pilot_gate(rows)['passed'])
        rows=records()
        for r in rows[:2]:r['legacy_hold']['final']['immediate_holds']=11
        self.assertFalse(pilot_gate(rows)['passed'])

    def test_progression_needs_every_family_and_joint_retention(self):
        rows=records();self.assertTrue(progression(rows)['passed'])
        rows[0]['primary']['final']['by_family']['position']=2
        rows[1]['legacy_hold']['final']['immediate_holds']=11
        self.assertFalse(progression(rows)['passed'])
        rows=records()
        for r in rows:r['primary']['initial']['successes']=12
        self.assertFalse(progression(rows)['passed'])

if __name__=='__main__':unittest.main()
