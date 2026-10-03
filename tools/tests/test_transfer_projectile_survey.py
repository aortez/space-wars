import importlib.util
from pathlib import Path
import unittest
from test_escape_travel import fixture

spec=importlib.util.spec_from_file_location('survey',Path(__file__).parents[1]/'survey-transfer-projectiles.py')
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)


class TransferProjectileSurveyTests(unittest.TestCase):
    def test_transfer_screen_does_not_require_an_escape_but_retains_physical_gates(self):
        e,_=fixture();e.pop('escape_travel');pr=dict(recovery_active=False)
        self.assertTrue(S.eligible(e,pr))
        e['pilot']['landing']['supported_feet']=1;self.assertFalse(S.eligible(e,pr))
        e['pilot']['landing']['supported_feet']=0;pr['recovery_active']=True;self.assertFalse(S.eligible(e,pr))

    def test_contact_window_includes_two_seconds_and_labels_ambiguous_launches(self):
        ep=dict(id=1,spawn_tick=10,first_tick=20,last_tick=30)
        contacts=[dict(tick=t,spawn_tick=10,source='cannon') for t in [19,150,151]]
        c=S.contact_window(ep,contacts,{10:{1}})
        self.assertEqual([v['tick'] for v in c['within_window']],[150]);self.assertEqual(c['first_warning_lead_ticks'],130)
        self.assertEqual([v['tick'] for v in c['later']],[151])
        self.assertTrue(S.contact_window(ep,contacts,{10:{1,2}})['ambiguous'])


if __name__=='__main__':unittest.main()
