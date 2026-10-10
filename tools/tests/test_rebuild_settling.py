import copy
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('settling',Path(__file__).parents[1]/'validate-rebuild-settling.py')
S=importlib.util.module_from_spec(spec); spec.loader.exec_module(S)


class RebuildSettlingTest(unittest.TestCase):
    def test_retained_commands_keep_the_existing_opt_in_once(self):
        command=['old','--seed','17','--terrain-flight-forecast','true','--out','old']
        source={'runs':{n:dict(item=dict(name=n),command=command) for n in S.A.ORDER}}
        jobs=S.jobs(source,Path('/frozen/binary'),Path('/results'))
        self.assertEqual([j['key'] for j in jobs],list(S.A.ORDER))
        for job in jobs:
            self.assertEqual(job['command'],['/frozen/binary',*command[1:-1],'/results/raw/'+job['key']])
        self.assertNotIn('--jetpacks',S.lab_command(Path('/frozen/lab'),Path('/results')))

    def test_placement_gate_requires_angle_and_correct_rejection(self):
        report=dict(selected_offset=-8,attempts=[dict(offset=-8,rejection=None,settling_angle_degrees=19.9),
            dict(offset=8,rejection='landing_misaligned',settling_angle_degrees=20)])
        S.check_placement(report)
        for fault in ('missing','edge','nan','wrong_rejection','wrong_selection'):
            bad=copy.deepcopy(report)
            if fault=='missing': bad['attempts'][0].pop('settling_angle_degrees')
            if fault=='edge': bad['attempts'][0]['settling_angle_degrees']=20
            if fault=='nan': bad['attempts'][0]['settling_angle_degrees']=float('nan')
            if fault=='wrong_rejection': bad['attempts'][1]['settling_angle_degrees']=19
            if fault=='wrong_selection': bad['selected_offset']=8
            with self.assertRaises(AssertionError,msg=fault): S.check_placement(bad)

    def test_diagnostic_only_changes_do_not_hide_real_native_or_action_changes(self):
        pilot=dict(actor={'x':1},recovery=dict(placement=None,status='ship_available',rebuilds=0))
        row=dict(tick=0,seat=0,observation=dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=pilot))))),actions=[])
        changed=copy.deepcopy(row)
        S.L.X.R.pilot(changed)['recovery']['placement']={'new':'diagnostic'}
        delta=S.comparison([row],[changed])
        self.assertIsNotNone(delta['first_observation']); self.assertIsNone(delta['first_native'])
        self.assertTrue(delta['actions_retained'])
        for field,value in [('status','clearance_blocked'),('rebuilds',1)]:
            bad=copy.deepcopy(changed); S.L.X.R.pilot(bad)['recovery'][field]=value
            self.assertIsNotNone(S.comparison([row],[bad])['first_native'])
        changed['actions']=[{'fake':True}]
        self.assertFalse(S.comparison([row],[changed])['actions_retained'])
        self.assertFalse(S.comparison([row],[])['actions_retained'])

    def test_complete_native_chain_survival_controls_and_trials_are_required(self):
        before=dict(outcome='win',pilot_deaths=0,ships_lost=1,completed_recoveries=0)
        sources={k:dict(runs={S.A.TARGET:dict(players=[{},before])}) for k in ('terrain','original')}
        milestones=[dict(tick=300,seat=1,state=dict(planet=0,claim_owner='player_2',form='escape_pod',location='on_foot')),
                    dict(tick=900,seat=1,state=dict(planet=0,claim_owner='player_2',form='ship',location={'aboard':1}))]
        runs={n:dict(retained_physics=True,retained_actions=True) for n in S.A.ORDER}
        runs[S.A.TARGET]=dict(players=[{},dict(before,completed_recoveries=1)],
            placement=dict(builds=[dict(tick=800,seat=1)]),
            terrain=dict(launches={'flight':dict(seat=1,completed_tick=200)},milestones=milestones))
        lab=dict(accepted=True)
        self.assertEqual(S.decision(runs,sources,lab)['decision'],'advance_to_broader_validation')
        for fault in ('boarding','claim','rebuild','crossing','counter','death','loss','extra_ship','control','action','lab'):
            bad=copy.deepcopy(runs); trials=copy.deepcopy(lab); target=bad[S.A.TARGET]
            if fault=='boarding': target['terrain']['milestones'].pop()
            if fault=='claim':
                for m in target['terrain']['milestones']: m['state']['claim_owner']=None
            if fault=='rebuild': target['placement']['builds']=[]
            if fault=='crossing': target['terrain']['launches']['flight']['completed_tick']=None
            if fault=='counter': target['players'][1]['completed_recoveries']=0
            if fault=='death': target['players'][1]['pilot_deaths']=1
            if fault=='loss': target['players'][1]['outcome']='loss'
            if fault=='extra_ship': target['players'][1]['ships_lost']=2
            if fault=='control': bad[S.A.ORDER[0]]['retained_physics']=False
            if fault=='action': bad[S.A.ORDER[0]]['retained_actions']=False
            if fault=='lab': trials['accepted']=False
            self.assertEqual(S.decision(bad,sources,trials)['decision'],'not_qualified',fault)


if __name__=='__main__': unittest.main()
