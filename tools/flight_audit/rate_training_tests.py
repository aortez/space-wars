"""Gate revision and paired optimizer identity for the fixed learning-rate pilot."""
import copy
import unittest
import torch
from tools.flight_audit.rate_training import pilot_gate,assert_matched_initial,RATES
from tools.flight_audit.short_recovery import pilot_gate as legacy_gate,progression
from tools.flight_audit.short_recovery_tests import records


def fixtures():
    rows=records()
    for r in rows:
        for c in r['training']['coverage'].values():c['successes']=0
    return rows


class RateTrainingTests(unittest.TestCase):
    def test_successful_fast_recovery_can_satisfy_exposure(self):
        rows=fixtures();c=rows[1]['training']['coverage']['spin'];c.update(correction_rows=190,successes=6)
        self.assertFalse(legacy_gate(rows)['passed']);self.assertTrue(pilot_gate(rows)['passed'])
        c['successes']=0;self.assertFalse(pilot_gate(rows)['passed'])

    def test_success_does_not_replace_signed_coverage_or_retention(self):
        rows=fixtures();c=rows[0]['training']['coverage']['spin'];c.update(correction_rows=0,successes=10,directions=[0])
        self.assertFalse(pilot_gate(rows)['passed'])
        rows=fixtures()
        for r in rows[:2]:r['legacy_hold']['final']['immediate_holds']=11
        self.assertFalse(pilot_gate(rows)['passed'])

    def test_position_experience_and_each_seed_success_still_required(self):
        rows=fixtures();rows[0]['training']['outcomes']={};self.assertFalse(pilot_gate(rows)['passed'])
        rows=fixtures()
        for r in rows[:2]:r['training']['coverage']['position']['eligible_before']=29
        self.assertFalse(pilot_gate(rows)['passed'])

    def test_no_learning_gain_or_missing_family_cannot_qualify_progression(self):
        rows=fixtures();self.assertTrue(progression(rows)['passed'])
        for r in rows:r['primary']['initial']['successes']=12
        self.assertFalse(progression(rows)['passed'])
        rows=fixtures()
        for r in rows:r['primary']['final']['by_family']['position']=0
        self.assertFalse(progression(rows)['passed'])

    def test_initial_optimizer_identity_allows_only_declared_rate(self):
        a={'model':{'weight':torch.tensor([1.])},'collector':{},'torch_rng':torch.tensor([1]),'python_rng':(1,),
           'optimizer':{'state':{},'param_groups':[{'lr':RATES['control'],'eps':1e-5}]}}
        b=copy.deepcopy(a);b['optimizer']['param_groups'][0]['lr']=RATES['smaller'];assert_matched_initial(a,b)
        for key in ['model','collector','torch_rng','python_rng','optimizer']:
            changed=copy.deepcopy(b)
            if key=='model':changed[key]['weight']+=1
            elif key=='collector':changed[key]['ticks']=1
            elif key=='torch_rng':changed[key]+=1
            elif key=='python_rng':changed[key]=(2,)
            else:changed[key]['param_groups'][0]['eps']=1e-4
            with self.assertRaises(AssertionError):assert_matched_initial(a,changed)


if __name__=='__main__':unittest.main()
