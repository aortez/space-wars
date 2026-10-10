import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('forecast',Path(__file__).parents[1]/'probe-rebuild-native-forecast.py')
P=importlib.util.module_from_spec(spec);spec.loader.exec_module(P)


class NativeForecastTest(unittest.TestCase):
    def test_preserves_retained_controls_and_existing_probes(self):
        prior={'commands':{n:['old','--out','old','--rebuild-support-alignment','false',
            '--rebuild-round-foot-probe','true','--rebuild-replay-end','29421'] for n in P.MODES}}
        for name in P.MODES:
            cmd=P.command(prior,Path('/binary'),Path('/output'),name)
            self.assertEqual(cmd[:3],['/binary','--out','/output'])
            self.assertEqual(cmd[3:-2],prior['commands'][name][3:])
            self.assertEqual(cmd[-2:],['--rebuild-native-forecast','true'])

    def test_incomplete_forecast_does_not_imply_failed_settling(self):
        self.assertFalse(P.prediction(None,120,'horizon'))
        for steps in (1,119,120):
            for stop in ('vehicle_unavailable','round_finished'):
                self.assertIsNone(P.prediction(None,steps,stop))
                self.assertTrue(P.prediction(100,steps,stop))
        self.assertIsNone(P.prediction(None,119,'horizon'))

    def test_settled_pod_and_incomplete_contact_do_not_qualify(self):
        p=dict(ship_form='ship',ship_available=True,ship_health=100,
            landing=dict(phase='landed',supported_feet=2,settled_seconds=.25))
        self.assertTrue(P.settled(dict(pilot=p)))
        for field,value in [('ship_form','escape_pod'),('ship_available',False),('ship_health',0)]:
            self.assertFalse(P.settled(dict(pilot=dict(p,**{field:value}))))
        for field,value in [('phase','settling'),('supported_feet',1),('settled_seconds',.24)]:
            self.assertFalse(P.settled(dict(pilot=dict(p,landing=dict(p['landing'],**{field:value})))))

    def test_reaudit_cannot_change_native_horizon_or_plan(self):
        old={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(old,dict(old,**{p:'repair'}),True)
        for p in set(old)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(old,dict(old,**{p:'repair'}),True)


if __name__=='__main__':unittest.main()
