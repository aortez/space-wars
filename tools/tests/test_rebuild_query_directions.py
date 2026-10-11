import copy
import importlib.util
from pathlib import Path
import unittest


def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
P=module('query_directions',Path(__file__).parents[1]/'probe-rebuild-query-directions.py')
S=module('standing_fixture',Path(__file__).with_name('test_rebuild_standing_search.py'))


def fixture():
    contact=S.fixture();contact['attempts'][0]['placement']['standing']=dict(x=1,y=0)
    radial=copy.deepcopy(contact);radial['attempts'][0]['placement']['radial_up']=dict(x=1,y=0)
    return dict(schema=1,tick=100,seat=1,native_direction='contact_normal',directions=dict(contact_normal=contact,radial=radial),
        physics_unchanged=True,native_flag_unchanged=True,same_search_inputs=True,preview_only=True,controller_input=False),copy.deepcopy(contact)


class QueryDirectionsTest(unittest.TestCase):
    def test_commands_replace_only_diagnostic_flag_and_keep_native_controls(self):
        prior=dict(commands={n:['old','--out','old','--rebuild-forecast-selection','true','--rebuild-radial-placement',str(n=='coarse').lower(),
            '--rebuild-standing-forecast-ticks',','.join(map(str,ticks)),'--rebuild-replay-end','29421'] for n,ticks in P.MODES.items()})
        for mode in P.MODES:
            expected=list(prior['commands'][mode]);expected[0]='/binary';expected[2]='/raw';expected[7]='--rebuild-direction-forecast-ticks'
            self.assertEqual(P.command(prior,Path('/binary'),Path('/raw'),mode),expected)

    def test_all_offsets_are_paired_and_query_angle_is_measured(self):
        p,r=fixture();m=P.analyze(p,100,'contact_normal',r)
        self.assertEqual(m['paired_offsets'],26)
        self.assertEqual(m['transitions'],{'positive -> positive':1,'rejected:no_ground -> rejected:no_ground':25})
        self.assertEqual((m['forecasts'],m['physics_steps'],m['offset_checks']),(2,240,54))
        self.assertEqual(m['query_angles'],[dict(bearing=1,degrees=90)])

    def test_geometry_and_conditional_outcomes_are_distinct(self):
        p,r=fixture();case=p['directions']['radial']['attempts'][0]
        case['placement']['attempts'][0]['rejection']='hull_obstructed';case['forecasts']=[]
        p['directions']['radial'].update(forecasts=0,offset_checks=26)
        m=P.analyze(p,100,'contact_normal',r)
        self.assertEqual(m['transitions']['positive -> rejected:hull_obstructed'],1)
        self.assertFalse(m['directions']['radial']['classifications'])
        p,r=fixture();f=p['directions']['radial']['attempts'][0]['forecasts'][0]['forecast']
        for s in f['samples']:
            s['landing'].update(phase='flying',settled_seconds=0);s['settled']=False
        f.update(first_settled_tick=None,settles_within_horizon=False)
        m=P.analyze(p,100,'contact_normal',r)
        self.assertEqual(m['transitions']['positive -> negative'],1)

    def test_pair_rejects_changed_search_inputs_or_direction_metadata(self):
        for mutate in (lambda r:r['attempts'][0]['precise_route']['diagnostics'].update(length=2),
                       lambda r:r['attempts'][0]['normal'].update(x=.1),
                       lambda r:r['map']['nodes'][0]['position'].update(x=2),
                       lambda r:r['attempts'][0]['placement']['radial_up'].update(y=.5),
                       lambda r:r['attempts'][0]['placement']['standing'].update(x=1.1)):
            p,r=fixture();mutate(p['directions']['radial'])
            with self.assertRaises(AssertionError):P.analyze(p,100,'contact_normal',r)

    def test_either_native_baseline_must_match_every_non_timing_field(self):
        for native in P.DIRECTIONS:
            p,_=fixture();p['native_direction']=native;r=copy.deepcopy(p['directions'][native])
            f=p['directions'][native]['attempts'][0]['forecasts'][0]['forecast']
            f['timing']['setup_ms']+=1;f['chunks'][0]['elapsed_ms']+=1
            self.assertTrue(P.analyze(p,100,native,r)['baseline_matches_except_timing'])
            f['samples'][0]['ship']['position']['x']+=.1
            with self.assertRaises(AssertionError):P.analyze(p,100,native,r)

    def test_missing_direction_forecast_or_exceeded_budget_cannot_be_hidden(self):
        for mutate in (lambda p:p['directions'].pop('radial'),
                       lambda p:p['directions']['radial']['attempts'][0].update(forecasts=[]),
                       lambda p:p['directions']['radial'].update(forecasts=65),
                       lambda p:p.update(native_flag_unchanged=False),
                       lambda p:p.update(controller_input=True)):
            p,r=fixture();mutate(p)
            with self.assertRaises(AssertionError):P.analyze(p,100,'contact_normal',r)

    def test_scope_failure_stays_inconclusive_and_coarse_access_stays_separate(self):
        p,r=fixture()
        for d in P.DIRECTIONS:p['directions'][d]['attempts'][0]['precise_route']['diagnostics']['failure']='disconnected'
        r=copy.deepcopy(p['directions']['contact_normal'])
        f=p['directions']['radial']['attempts'][0]['forecasts'][0]['forecast'];f.update(stop='outside_model_scope',settles_within_horizon=None)
        m=P.analyze(p,100,'contact_normal',r)
        self.assertEqual(m['transitions']['positive -> inconclusive'],1)
        self.assertEqual(m['directions']['contact_normal']['precisely_reachable_positive'],0)
        self.assertEqual(m['directions']['contact_normal']['accepted'][0]['route_kind'],'coarse_only')

    def test_reaudit_cannot_change_either_runtime_or_the_frozen_plan(self):
        old={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(old,dict(old,**{p:'repair'}),True)
        for p in set(old)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(old,dict(old,**{p:'repair'}),True)


if __name__=='__main__':unittest.main()
