import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('contact',Path(__file__).parents[1]/'validate-rebuild-contact-frame.py')
C=importlib.util.module_from_spec(spec);spec.loader.exec_module(C)


class ContactFrameTest(unittest.TestCase):
    def test_each_candidate_changes_only_native_contact_frame(self):
        common=['old','--out','old','--rebuild-refinement','true','--rebuild-staging','true',
            '--rebuild-staging-walk','true','--rebuild-footing-hold','false','--rebuild-staging-handoff']
        prior={'commands':{'control_'+n:common+[str(n=='handoff').lower()] for n in ('walk','handoff')}}
        for base in ('walk','handoff'):
            old=C.command(prior,Path('/binary'),Path('/raw'),'control_'+base)
            new=C.command(prior,Path('/binary'),Path('/raw'),'contact_'+base)
            self.assertEqual(old[:-1],new[:-1])
            self.assertEqual(old[-4:],['--rebuild-contact-probe','true','--rebuild-contact-frame','false'])
            self.assertEqual(new[-4:],['--rebuild-contact-probe','true','--rebuild-contact-frame','true'])

    def test_audit_resume_cannot_change_runtime_or_bounds(self):
        original={p:'old' for p in (*C.CHANGED,*C.H.OWN)}
        for p in C.OWN[:2]:C.check_inputs(original,dict(original,**{p:'repair'}),True)
        for p in set(original)-set(C.OWN[:2]):
            with self.assertRaises(AssertionError):C.check_inputs(original,dict(original,**{p:'changed'}),True)
        with self.assertRaises(AssertionError):C.check_inputs(original,{},True)

    def test_contact_audit_rejects_a_stale_anchor_and_missing_rows(self):
        def point(x,y):return dict(x=x,y=y)
        report=dict(tick=10,standing=point(2,0))
        row=dict(tick=10,pilots=[{},dict(recovery=dict(placement=report))])
        probe=dict(tick=10,seat=1,enabled=True,contact=dict(planet=0,frame_position=point(5,6),frame_angle=0,
            solver_position=point(6.6,6),current_position=point(7,6),offset=.4,
            solver_normal=point(0,1),current_normal=point(0,1),local_position=point(2,0)),
            candidate={'Ok':[0,point(7,6),point(0,1)]})
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);trace=root/'rebuild-live.jsonl';contacts=root/'rebuild-live-contacts.jsonl'
            trace.write_text(json.dumps(row)+'\n');contacts.write_text(json.dumps(probe)+'\n')
            result=C.audit_contacts(root,'live',True)
            self.assertEqual(result['attempt_ticks'],[10])
            row['pilots'][1]['recovery']['placement']['standing']=point(1.6,0)
            trace.write_text(json.dumps(row)+'\n')
            with self.assertRaises(AssertionError):C.audit_contacts(root,'live',True)
            contacts.write_text('')
            with self.assertRaises(AssertionError):C.audit_contacts(root,'live',True)


if __name__=='__main__':unittest.main()
