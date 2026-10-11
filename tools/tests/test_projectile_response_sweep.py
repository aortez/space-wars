import copy
import hashlib
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('sweep',Path(__file__).parents[1]/'validate-projectile-response-sweep.py')
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)


def fixture():
    plan=[];runs={}
    for world in (0,1):
        for seat in (0,1):
            for powered in (False,True):
                name=f"shared-armed-world{world}-p{seat+1}-"+('powered' if powered else 'walking')
                item=dict(name=name,seat=seat,powered=powered,kind='armed',seed=world+1)
                plan.append(dict(name=name,item=item,source='retained',enabled=True));runs[name]=dict(item=item)
    for name in ['shared-armed-world0-p1-walking','shared-armed-world0-p1-powered','shared-armed-world1-p1-powered']:
        key='health-'+name;item=copy.deepcopy(runs[name]['item'])
        plan.append(dict(name=key,item=item,source='health',enabled=True));runs[key]=dict(item=item)
    return dict(plan=plan,runs=runs)


class ResponseSweepTests(unittest.TestCase):
    def test_case_selection_keeps_all_regressions_and_both_seats_for_two_new_seeds(self):
        prior=fixture();original=copy.deepcopy(prior);cases=S.cases(prior)
        self.assertEqual(prior,original);self.assertEqual(len(cases),15)
        self.assertEqual([c['key'] for c in cases[:11]],[e['name'] for e in prior['plan']])
        fresh=[c for c in cases if c['group']=='fresh']
        self.assertEqual([c['item']['seat'] for c in fresh],[0,1,0,1])
        for c in fresh:
            item=c['item'];self.assertTrue(item['powered'])
            self.assertEqual(item['seed'],int.from_bytes(hashlib.sha256(item['seed_namespace'].encode()).digest()[:8],'little'))

    def test_explicit_probe_seat_does_not_change_reporting_seat_policies_or_budgets(self):
        case=S.cases(fixture())[-1]
        old=dict(command=['old','--out','before','--seed','12','--seat','0','--p2-policy','material_mission_v13','--objective-query-budget','384'])
        cmd=S.command(case,old,'binary','root','left')
        self.assertEqual(cmd[cmd.index('--seat')+1],'0')
        self.assertEqual(cmd[cmd.index('--probe-projectile-response-seat')+1],'1')
        self.assertEqual(cmd[cmd.index('--p2-policy')+1],'material_mission_v13')
        self.assertEqual(cmd[cmd.index('--objective-query-budget')+1],'384')
        self.assertNotIn('--probe-projectile-response',S.command(case,old,'binary','root','none'))


if __name__=='__main__':unittest.main()
