import copy
import importlib.util
from pathlib import Path
import unittest

def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
P=module('selection',Path(__file__).parents[1]/'validate-rebuild-forecast-selection.py')
L=module('local_fixture',Path(__file__).with_name('test_rebuild_local_forecast.py'))


def fixture():
    f,_,_=L.fixture();f.update(tick=60,start_delay=40,warmup_steps=40,launch_tick=100)
    f['chunks']=[dict(first_step=i+1,steps=4,elapsed_ms=.02) for i in range(0,160,4)]
    report=dict(tick=100,revision=1,selected_offset=-8,anchor=dict(x=1,y=2),
        attempts=[dict(offset=-8,rejection=None,route=dict(failure=None,length=2))])
    f['report']['revision']=1
    work=[dict(tick=61+i,steps=4) for i in range(40)]
    event=dict(tick=100,offset=-8,accepted=True,prediction=True,forecast=f,
        revalidation=dict(tick=100,report=report,launch_matches=True))
    return event,work


class ForecastSelectionTest(unittest.TestCase):
    def test_only_selector_differs_between_paired_controls(self):
        prior={'commands':{n:['old','--out','old','--rebuild-support-alignment','false',
            '--rebuild-round-foot-probe','true','--rebuild-local-forecast','true','--rebuild-replay-end','29421']
            for n in ('control_handoff','control_coarse')}}
        for base in ('handoff','coarse'):
            control=P.command(prior,Path('/binary'),Path('/out'),'control_'+base)
            candidate=P.command(prior,Path('/binary'),Path('/out'),'selection_'+base)
            self.assertEqual(control[:-1],candidate[:-1]);self.assertEqual(control[-1],'false');self.assertEqual(candidate[-1],'true')
            self.assertNotIn('--rebuild-local-forecast',candidate)
            self.assertIn('--rebuild-round-foot-probe',candidate)

    def test_acceptance_requires_full_forecast_at_scheduled_tick_and_fresh_validation(self):
        e,w=fixture();m=P.check_forecast(e,w)
        self.assertEqual((m['capture_tick'],m['launch_tick'],m['decision_tick']),(60,100,100))
        self.assertEqual(m['warmup_steps']+m['steps'],160)
        for mutate in (lambda e:e.update(tick=101),lambda e:e.update(prediction=False),
                       lambda e:e['revalidation'].update(launch_matches=False),
                       lambda e:e['revalidation']['report'].update(revision=2),
                       lambda e:e['revalidation']['report']['attempts'][0]['route'].update(failure='disconnected')):
            e,w=fixture();mutate(e)
            with self.assertRaises(AssertionError):P.check_forecast(e,w)

    def test_work_quota_and_contiguous_native_schedule_are_verified(self):
        for mutate in (lambda w:w[0].update(steps=5),lambda w:w[0].update(tick=62)):
            e,w=fixture();mutate(w)
            with self.assertRaises(AssertionError):P.check_forecast(e,w)

    def test_negative_forecast_is_retained_without_authorizing_construction(self):
        e,w=fixture();e['forecast']['first_settled_tick']=None;e['forecast']['settles_within_horizon']=False
        for row in e['forecast']['samples']:
            row['landing']=dict(row['landing'],phase='flying',settled_seconds=0);row['settled']=False
        e.update(accepted=False,prediction=False,revalidation=None)
        self.assertFalse(P.check_forecast(e,w)['prediction'])
        e['accepted']=True
        with self.assertRaises(AssertionError):P.check_forecast(e,w)

    def test_reaudit_cannot_change_runtime_or_frozen_plan(self):
        old={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(old,dict(old,**{p:'repair'}),True)
        for p in set(old)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(old,dict(old,**{p:'repair'}),True)


if __name__=='__main__':unittest.main()
