import copy
import importlib.util
import json
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('terrain_flight',Path(__file__).parents[1]/'validate-terrain-flight.py')
T=importlib.util.module_from_spec(spec); spec.loader.exec_module(T)


class TerrainFlightTest(unittest.TestCase):
    def test_five_replays_preserve_every_baseline_argument(self):
        source={'runs':{name:dict(item=dict(name=name),command=['old','--seed','17','--out','old']) for name in T.A.ORDER}}
        jobs=T.jobs(source,Path('/frozen/binary'),Path('/results'))
        self.assertEqual(len(jobs),5)
        self.assertTrue(jobs[0]['diagnostic'])
        for job in jobs:
            options=dict(zip(job['command'][1::2],job['command'][2::2]))
            self.assertEqual(options['--seed'],'17')
            self.assertEqual(job['command'][0],'/frozen/binary')
            self.assertEqual('--terrain-flight-forecast' in options,not job['diagnostic'])
            self.assertEqual('--probe-terrain-forecast' in options,job['diagnostic'])
        self.assertEqual([j['prior'] for j in jobs[1:]],list(T.A.ORDER))

    def test_native_progress_and_winning_survival_are_required(self):
        before=dict(outcome='win',pilot_deaths=0,ships_lost=1,completed_recoveries=0)
        after=dict(before,completed_recoveries=1)
        sources={k:dict(runs={T.A.TARGET:dict(players=[{},before])}) for k in ('original','arrival')}
        milestones=[dict(tick=300,seat=1,state=dict(planet=0,claim_owner='player_2',rebuilds=0,form='escape_pod',location='on_foot')),
                    dict(tick=800,seat=1,state=dict(planet=0,claim_owner='player_2',rebuilds=1,form='ship',location='on_foot')),
                    dict(tick=900,seat=1,state=dict(planet=0,claim_owner='player_2',rebuilds=1,form='ship',location={'aboard':1}))]
        runs={n:dict(retained_physics=True) for n in T.A.ORDER}
        runs['forecast-probe']=dict(probe=dict(both_native_plans_forecast=True))
        runs[T.A.TARGET]=dict(players=[{},after],terrain=dict(launches={'flight':dict(seat=1,completed_tick=200)},milestones=milestones))
        self.assertEqual(T.decision(runs,sources)['decision'],'advance_to_broader_validation')
        for fault in ('boarding','claim','rebuild','landing','counter','death','control','forecast','win'):
            bad=copy.deepcopy(runs); target=bad[T.A.TARGET]
            if fault=='boarding': target['terrain']['milestones'].pop()
            if fault=='claim':
                for m in target['terrain']['milestones']: m['state']['claim_owner']=None
            if fault=='rebuild':
                for m in target['terrain']['milestones']: m['state']['rebuilds']=0
            if fault=='landing': target['terrain']['launches']['flight']['completed_tick']=None
            if fault=='counter': target['players'][1]['completed_recoveries']=0
            if fault=='death': target['players'][1]['pilot_deaths']=1
            if fault=='win': target['players'][1]['outcome']='loss'
            if fault=='control': bad[T.A.ORDER[0]]['retained_physics']=False
            if fault=='forecast': bad['forecast-probe']['probe']['both_native_plans_forecast']=False
            self.assertEqual(T.decision(bad,sources)['decision'],'not_qualified',fault)

    def test_comparison_distinguishes_sensor_only_from_actions_and_native_state(self):
        pilot=dict(actor={'x':1})
        row=dict(tick=0,seat=0,observation=dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=pilot),jetpack={})))),actions=[],mission={})
        changed=copy.deepcopy(row); changed['observation']['local']['combat']['recovery']['jetpack']={'terrain_flight':{'forecast':None}}
        result=T.comparison([row],[changed])
        self.assertIsNotNone(result['first_observation'])
        self.assertIsNone(result['first_behavior'])
        self.assertIsNone(result['first_action']); self.assertIsNone(result['first_native'])
        changed['actions']=[{'fake':True}]
        self.assertIsNotNone(T.comparison([row],[changed])['first_action'])


if __name__=='__main__': unittest.main()
