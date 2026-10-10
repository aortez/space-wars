import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('contact',Path(__file__).parents[1]/'validate-rebuild-contact-recovery.py')
P=importlib.util.module_from_spec(spec);spec.loader.exec_module(P)


def switch():
    return dict(schema=1,seat=1,requested=True,eligible=True,applied=True,trigger=copy.deepcopy(P.TRIGGER),
        transition=dict(tick=25439,seat=1,**{'from':'radial','to':'contact_normal'},
            pilot_unchanged=True,contact_unchanged=True,task_unchanged=True,search_unchanged=True,
            task=dict(started_tick=16820,ground_budget_ticks=5400,relocations=1),
            pilot=dict(recovery=dict(rebuilds=0,ships_lost=1))))


class ContactRecoveryTest(unittest.TestCase):
    def test_only_one_shot_flag_differs_in_coarse_pair(self):
        prior=dict(commands={n:['old','--out','old','--rebuild-radial-placement',str(n=='coarse').lower(),
            '--rebuild-forecast-selection','true','--rebuild-direction-forecast-ticks','1,2','--rebuild-replay-end','29421'] for n in ('handoff','coarse')})
        old=P.command(prior,Path('/binary'),Path('/raw'),'control_coarse');new=P.command(prior,Path('/binary'),Path('/raw'),'contact_coarse')
        self.assertEqual(old[:-1],new[:-1]);self.assertEqual(old[-1],'false');self.assertEqual(new[-1],'true')
        self.assertNotIn('--rebuild-direction-forecast-ticks',new)
        self.assertEqual(new[3:7],prior['commands']['coarse'][3:7])
        self.assertEqual(P.command(prior,Path('/b'),Path('/r'),'control_handoff')[-1],'false')

    def test_switch_requires_expected_boundary_without_resetting_task_or_search(self):
        P.check_switch(switch(),True,True)
        for mutate in (lambda r:r['trigger'].update(activation_tick=25438),
                       lambda r:r['transition']['task'].update(started_tick=25439),
                       lambda r:r['transition'].update(search_unchanged=False),
                       lambda r:r['transition']['pilot']['recovery'].update(rebuilds=1),
                       lambda r:r.update(applied=False)):
            r=switch();mutate(r)
            with self.assertRaises(AssertionError):P.check_switch(r,True,True)

    def test_recorded_and_disabled_forks_cannot_activate(self):
        for requested in (False,True):
            r=dict(schema=1,seat=1,requested=requested,eligible=False,applied=False,trigger=None,transition=None)
            P.check_switch(r,requested,False)
            r['trigger']=P.TRIGGER
            with self.assertRaises(AssertionError):P.check_switch(r,requested,False)

    def test_prefix_comparison_requires_raw_equality_and_records_later_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);a=root/'a';b=root/'b'
            rows=[dict(tick=t,actions=[0],pilots=[dict(recovery=dict(placement=None),position=0)],task=dict(started_tick=16820)) for t in range(23767,23771)]
            write=lambda path,data:path.write_text(''.join(json.dumps(r)+'\n' for r in data))
            write(b,rows);altered=copy.deepcopy(rows);altered[2]['actions']=[1];write(a,altered)
            m=P.compare_rows(a,b,23769);self.assertEqual(m['equal_rows'],2)
            self.assertEqual(m['first_differences']['actions']['tick'],23769)
            altered[1]['task']['started_tick']=23768;write(a,altered)
            with self.assertRaises(AssertionError):P.compare_rows(a,b,23769)

    def test_prefix_ignores_only_latched_placement_in_physical_projection(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);a=root/'a';b=root/'b'
            rows=[dict(tick=t,actions=[],pilots=[dict(recovery=dict(placement=None),position=0)],task={}) for t in (23767,23768)]
            b.write_text(''.join(json.dumps(r)+'\n' for r in rows));rows[1]['pilots'][0]['recovery']['placement']={'tick':23768}
            a.write_text(''.join(json.dumps(r)+'\n' for r in rows));m=P.compare_rows(a,b,23768)
            self.assertIn('row',m['first_differences']);self.assertNotIn('physical_pilots',m['first_differences'])
            rows[1]['pilots'][0]['position']=1;a.write_text(''.join(json.dumps(r)+'\n' for r in rows))
            self.assertEqual(P.compare_rows(a,b,23768)['first_differences']['physical_pilots']['tick'],23768)

    def test_veto_and_exhaustion_must_reproduce_before_switch(self):
        events=[dict(kind='evaluated',tick=25413,search_tick=25373,prediction=False,accepted=False,forecast=None),
            dict(kind='exhausted',tick=25438,search_tick=25373),dict(kind='update',tick=25438,elapsed_ms=1,pending=False)]
        new=copy.deepcopy(events);new[-1]['elapsed_ms']=2
        self.assertEqual(P.check_event_prefix(new,events)['events'],3)
        for altered in (events[:-2],copy.deepcopy(events)):
            if len(altered)>1:altered[0]['prediction']=True
            with self.assertRaises(AssertionError):P.check_event_prefix(altered,events)

    def test_direction_is_checked_at_query_tick_including_latched_old_reports(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);old=dict(tick=25438,standing=dict(x=1,y=0),radial_up=dict(x=1,y=0),attempts=[],selected_offset=None)
            new=dict(tick=25440,standing=dict(x=1,y=0),attempts=[],selected_offset=None)
            rows=[dict(tick=t,pilots=[{},dict(recovery=dict(placement=p))],observation=dict(rebuild=None)) for t,p in ((25438,old),(25439,old),(25440,new))]
            path=root/'rebuild-live.jsonl';write=lambda:path.write_text(''.join(json.dumps(r)+'\n' for r in rows))
            write();self.assertEqual(P.audit_orientation(root,'live',True,25439)['native_reports'],2)
            new['radial_up']=dict(x=1,y=0);write()
            with self.assertRaises(AssertionError):P.audit_orientation(root,'live',True,25439)

    def test_reaudit_cannot_change_runtime_trigger_or_frozen_plan(self):
        old={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(old,dict(old,**{p:'repair'}),True)
        for p in set(old)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(old,dict(old,**{p:'repair'}),True)


if __name__=='__main__':unittest.main()
