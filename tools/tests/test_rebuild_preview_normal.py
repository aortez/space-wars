import copy
import importlib.util
import json
import tempfile
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('normal',Path(__file__).parents[1]/'validate-rebuild-preview-normal.py')
P=importlib.util.module_from_spec(spec);spec.loader.exec_module(P)


def record():
    c=dict(seat=1,captured_tick=24000,arrival_tick=None,planet=1,revision=22,ships_lost=1,
        bearing=343,position=dict(x=30.,y=0.),normal=dict(x=1.,y=0.),offset=-11.)
    validation=dict(tick=24000,planet=1,revision=22,standing=c['position'],selected_offset=-11.,attempts=[dict(offset=-11.,rejection=None)])
    capture=dict(tick=24000,seat=1,kind='captured',context=c,validation=validation,offset_checks=1)
    arrival=dict(tick=24300,seat=1,kind='arrived',context=dict(c,arrival_tick=24300),foot=c['position'],distance=0.)
    return dict(schema=1,seat=1,requested=True,eligible=True,events=[capture,arrival])


def report(r):
    return dict(tick=24700,planet=1,revision=22,standing=dict(x=30.5,y=0.),preview_normal=r['events'][1]['context'])


class PreviewNormalTest(unittest.TestCase):
    def test_pairs_change_only_live_preview_flag(self):
        prior=dict(commands={n:['old','--out','old','--rebuild-forecast-selection','true','--rebuild-contact-frame','true',
            '--rebuild-precise-arrival','true','--rebuild-contact-after-veto',str(n=='contact_coarse').lower()] for n in ('control_handoff','contact_coarse')})
        for suffix in ('handoff','coarse'):
            control=P.command(prior,Path('/b'),Path('/o'),'control_'+suffix)
            candidate=P.command(prior,Path('/b'),Path('/o'),'normal_'+suffix)
            self.assertEqual(control[:-1],candidate[:-1]);self.assertEqual(control[-1],'false');self.assertEqual(candidate[-1],'true')
            self.assertEqual(control[control.index('--rebuild-contact-after-veto')+1],str(suffix=='coarse').lower())

    def test_controls_and_recorded_forks_cannot_capture(self):
        for requested,live in ((False,True),(True,False),(False,False)):
            r=dict(schema=1,seat=1,requested=requested,eligible=False,events=[]);P.check_record(r,requested,live)
            r['events']=record()['events']
            with self.assertRaises(AssertionError):P.check_record(r,requested,live)

    def test_capture_and_arrival_have_exact_source_and_native_distance(self):
        P.check_record(record(),True,True)
        mutations=(lambda r:r['events'][0].update(offset_checks=2),
            lambda r:r['events'][0]['validation'].update(selected_offset=-12),
            lambda r:r['events'][1].update(distance=.12),
            lambda r:r['events'][1]['context'].update(normal=dict(x=0.,y=1.)),
            lambda r:r['events'][1]['context'].update(revision=23),
            lambda r:r['events'][0]['context'].update(ships_lost=2))
        for mutate in mutations:
            r=copy.deepcopy(record());mutate(r)
            with self.assertRaises(AssertionError):P.check_record(r,True,True)

    def test_context_must_clear_before_replacement_and_cannot_activate_twice(self):
        for extra in (0,1):
            r=record();r['events'].append(copy.deepcopy(r['events'][extra]));r['events'][-1]['tick']=24400
            with self.assertRaises(AssertionError):P.check_record(r,True,True)
        r=record();r['events'].append(dict(tick=24400,seat=1,kind='cleared',reason='context_changed',context=r['events'][1]['context']))
        P.check_record(r,True,True)
        r['events'][-1]['context']=r['events'][0]['context']
        with self.assertRaises(AssertionError):P.check_record(r,True,True)

    def test_native_report_requires_reached_same_revision_site_with_actual_point(self):
        r=record();captured,arrived=P.check_record(r,True,True);p=report(r)
        self.assertTrue(P.check_report(p,captured,arrived))
        for fields in (dict(tick=24300),dict(planet=2),dict(revision=23),dict(standing=dict(x=32.,y=0.)),dict(radial_up=dict(x=1.,y=0.))):
            with self.assertRaises(AssertionError):P.check_report(dict(p,**fields),captured,arrived)
        with self.assertRaises(AssertionError):P.check_report(p,captured,{})
        self.assertFalse(P.check_report(dict(tick=24300),{},{}))

    def test_anchored_forecast_retains_measured_normal_and_actual_hatch_origin(self):
        r=record();captured,arrived=P.check_record(r,True,True)
        p=dict(report(r),anchor=dict(x=30.4,y=0.),anchor_up=dict(x=1.,y=0.))
        self.assertTrue(P.check_report(p,captured,arrived))
        for fields in (dict(anchor_up=dict(x=0.,y=1.)),dict(anchor=dict(x=32.,y=0.))):
            with self.assertRaises(AssertionError):P.check_report(dict(p,**fields),captured,arrived)

    def test_selected_site_uses_exact_native_f32_identity_across_json_round_trips(self):
        r=record();r['events']=r['events'][:1];c=r['events'][0]['context']
        c['position']['x']=29.005271911621094
        site=dict(position=copy.deepcopy(c['position']),planet=1,revision=22)
        copied=copy.deepcopy(site);copied['position']['x']=29.005271911621097
        row=dict(tick=24000,task=dict(relocations=1,relocation_site=copied),pilots=[{},dict(recovery=dict(placement=None))],
            observation=dict(rebuild=dict(tick=24000,site=site,attempts=[dict(bearing=343,placement=r['events'][0]['validation'])])))
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'rebuild-preview-normal-live.json').write_text(json.dumps(r))
            (root/'rebuild-selection-live.jsonl').write_text('')
            path=root/'rebuild-live.jsonl';path.write_text(json.dumps(row)+'\n')
            self.assertEqual(len(P.audit_normal(root,'live',True)['captures']),1)
            copied['position']['x']+=.0001;path.write_text(json.dumps(row)+'\n')
            with self.assertRaises(AssertionError):P.audit_normal(root,'live',True)

    def test_reaudit_can_only_repair_auditor_and_its_tests(self):
        old={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(old,dict(old,**{p:'repair'}),True)
        for p in set(old)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(old,dict(old,**{p:'repair'}),True)


if __name__=='__main__':unittest.main()
