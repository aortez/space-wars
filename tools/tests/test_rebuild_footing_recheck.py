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

    def test_revalidation_survives_an_available_escape_pod_and_counts_the_new_site(self):
        site=dict(planet=0,revision=4)
        held=dict(started_tick=5,ended_tick=10,reason='terrain changed',site=dict(planet=0,revision=3),bearing=7)
        pilot=dict(planet=dict(index=0,revision=4),ship_available=True,ship_form='escape_pod',recovery=dict(status='rebuilding'))
        first=dict(tick=10,task=dict(rebuild_footing=held,relocations=1,relocation_site=None,status='running'),
            pilots=[{},pilot],observation=dict(rebuild=None))
        next_row=json.loads(json.dumps(first));next_row['tick']=15
        next_row['task'].update(relocations=2,relocation_site=site)
        next_row['observation']['rebuild']=dict(tick=15,site=site,search=dict(),refinement=dict(coarse_candidates=0),attempts=[dict(bearing=7)])
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);p=root/'rebuild-live.jsonl'
            p.write_text('\n'.join(map(json.dumps,[first,next_row]))+'\n')
            result=F.audit_recheck(root,'live',True,True)
            self.assertEqual(result['rechecks'],[dict(tick=10,bearing=7,survey_ticks=[15],selected_tick=15)])
            next_row['task']['relocations']=1
            p.write_text('\n'.join(map(json.dumps,[first,next_row]))+'\n')
            with self.assertRaises(AssertionError):F.audit_recheck(root,'live',True,True)

    def test_site_comparison_preserves_native_float_bits_and_exact_identity(self):
        a=dict(planet=0,revision=22,position=dict(x=29.005197525024414,y=-15.964317321777344))
        b=dict(a,position=dict(a['position'],x=29.005197525024418))
        self.assertTrue(F.same_native(a,b))
        b['position']['x']+=.00001
        self.assertFalse(F.same_native(a,b))
        self.assertFalse(F.same_native(a,dict(a,revision=23)))
        self.assertFalse(F.same_native(a,dict(a,precise=True)))


if __name__=='__main__':unittest.main()
