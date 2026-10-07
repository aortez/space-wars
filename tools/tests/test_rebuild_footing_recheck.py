import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('recheck',Path(__file__).parents[1]/'validate-rebuild-footing-recheck.py')
F=importlib.util.module_from_spec(spec);spec.loader.exec_module(F)


class FootingRecheckTest(unittest.TestCase):
    def test_variants_keep_the_native_frame_and_change_only_the_named_options(self):
        common=['old','--out','old','--rebuild-refinement','true','--rebuild-staging','true',
            '--rebuild-staging-walk','true','--rebuild-footing-hold','false',
            '--rebuild-contact-probe','true','--rebuild-contact-frame','true','--rebuild-staging-handoff']
        prior={'commands':{'contact_'+n:common+[str(n=='handoff').lower()] for n in ('walk','handoff')}}
        for base in ('walk','handoff'):
            variants={n:F.command(prior,Path('/binary'),Path('/raw'),n+'_'+base) for n in ('control','hold','recheck')}
            a,b,c=(variants[n] for n in ('control','hold','recheck'))
            changes=[i for i,(x,y) in enumerate(zip(a,b)) if x!=y]
            self.assertEqual(changes,[a.index('--rebuild-footing-hold')+1])
            self.assertEqual((a[changes[0]],b[changes[0]]),('false','true'))
            self.assertEqual(b[:-1],c[:-1]);self.assertEqual((b[-1],c[-1]),('false','true'))

    def test_audit_resume_cannot_change_runtime_or_bounds(self):
        original={p:'old' for p in (*F.CHANGED,*F.C.OWN)}
        for p in F.OWN[:2]:F.check_inputs(original,dict(original,**{p:'repair'}),True)
        for p in set(original)-set(F.OWN[:2]):
            with self.assertRaises(AssertionError):F.check_inputs(original,dict(original,**{p:'changed'}),True)
        with self.assertRaises(AssertionError):F.check_inputs(original,{},True)

    def test_failure_audit_rejects_prearrival_and_wrong_revision_failures(self):
        held=dict(started_tick=5,ended_tick=10,reason='native placement rejected',site=dict(planet=0,revision=3))
        report=dict(tick=10,planet=0,revision=3,selected_offset=None)
        pilot=dict(planet=dict(index=0,revision=3),recovery=dict(status='hatch_blocked',placement=report))
        row=dict(tick=10,task=dict(rebuild_footing=held),pilots=[{},pilot])
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);path=root/'rebuild-live.jsonl'
            def write():path.write_text(json.dumps(row)+'\n')
            write();self.assertEqual(F.audit_recheck(root,'live',True,True)['fresh_failure_ticks'],[10])
            for key,value in (('tick',4),('tick',11),('planet',1),('revision',2),('selected_offset',-10)):
                old=report[key];report[key]=value;write()
                with self.assertRaises(AssertionError):F.audit_recheck(root,'live',True,True)
                report[key]=old


if __name__=='__main__':unittest.main()
