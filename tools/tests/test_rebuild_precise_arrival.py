import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('arrival',Path(__file__).parents[1]/'validate-rebuild-precise-arrival.py')
P=importlib.util.module_from_spec(spec);spec.loader.exec_module(P)


class PreciseArrivalTest(unittest.TestCase):
    def test_pairs_change_only_arrival_and_remove_the_old_probe(self):
        prior={'commands':{n:['old','--out','old','--rebuild-radial-placement',str(n=='radial_handoff').lower(),
            '--rebuild-placement-probe','request'] for n in ('control_handoff','radial_handoff')}}
        for case in ('handoff','coarse'):
            a=P.command(prior,Path('/binary'),Path('/raw'),'control_'+case)
            b=P.command(prior,Path('/binary'),Path('/raw'),'precise_'+case)
            self.assertEqual(a[:-1],b[:-1]);self.assertEqual((a[-1],b[-1]),('false','true'))
            self.assertNotIn('--rebuild-placement-probe',a)

    def test_audit_repairs_cannot_alter_controller_or_request_bounds(self):
        old={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(old,dict(old,**{p:'repair'}),True)
        for p in set(old)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(old,dict(old,**{p:'repair'}),True)

    def test_arrival_audit_uses_the_matching_new_ground_task_and_actual_foot(self):
        site=dict(planet=0,revision=1,position=dict(x=4,y=60))
        pilot=dict(actor=dict(position=dict(x=4.05,y=60.45)),actor_up=dict(x=0,y=1),
            planet=dict(motion=dict(position=dict(x=0,y=0),angle=0)),recovery=dict(placement=None))
        ground=dict(started_tick=10,goal='arrived',precise_rebuild=True,destination=dict(rebuild=site))
        first=dict(tick=10,pilots=[{},pilot],task=dict(relocations=1,relocation_site=site,ground=ground))
        arrived=dict(tick=11,pilots=[{},pilot],task=dict(relocations=1,relocation_site=None,ground=ground))
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);path=root/'rebuild-live.jsonl'
            def write():path.write_text('\n'.join(map(json.dumps,[first,arrived]))+'\n')
            write();result=P.audit_arrival(root,'live',True)
            self.assertEqual([a['tick'] for a in result['arrivals']],[11])
            pilot['actor']['position']['x']=4.2;write()
            with self.assertRaises(AssertionError):P.audit_arrival(root,'live',True)


if __name__=='__main__':unittest.main()
