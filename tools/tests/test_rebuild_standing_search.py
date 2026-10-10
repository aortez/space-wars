import copy
import importlib.util
from pathlib import Path
import unittest


def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
P=module('standing_search',Path(__file__).parents[1]/'probe-rebuild-standing-search.py')
L=module('local_fixture',Path(__file__).with_name('test_rebuild_local_forecast.py'))


def fixture():
    f,_,_=L.fixture();f.update(start_delay=0,warmup_steps=0,launch_tick=100)
    route=lambda length:dict(path=[1],diagnostics=dict(failure=None,length=length,jumps=0,flights=0))
    attempts=[dict(offset=o,rejection='no_ground') for o in P.OFFSETS]
    attempts[0]=dict(offset=-8.,rejection=None,settling_angle_degrees=10,route=route(.2)['diagnostics'],hatch=dict(x=2,y=0))
    placement=dict(tick=100,planet=0,revision=1,selected_offset=-8.,attempts=attempts)
    f['report']=dict(placement,attempts=[attempts[0]])
    case=dict(bearing=1,position=dict(x=1,y=0),normal=dict(x=0,y=1),distance=1,already_here=False,sparse_candidate=False,
        coarse_route=route(1),precise_route=route(1),staging=None,staging_routes=None,eligible=True,
        placement=placement,forecasts=[dict(offset=-8.,forecast=f)])
    return dict(tick=100,seat=1,planet=0,revision=1,foot=dict(x=0,y=0),offsets=P.OFFSETS,
        map=dict(nodes=[dict(id=1,position=case['position'])]),attempts=[case],max_forecasts=64,forecasts=1,offset_checks=27,
        physics_unchanged=True,actor_unchanged=True,recovery_unchanged=True,preview_only=True,
        controller_input=False,travel_or_build_time_predicted=False)


class StandingSearchTest(unittest.TestCase):
    def test_probe_command_only_adds_fixed_snapshot_ticks(self):
        prior={'commands':{base:['old','--out','old','--rebuild-forecast-selection','true','--rebuild-replay-end','29421'] for base,_ in P.MODES.values()}}
        for mode,(base,ticks) in P.MODES.items():
            cmd=P.command(prior,Path('/binary'),Path('/raw'),mode)
            self.assertEqual(cmd[3:-2],prior['commands'][base][3:])
            self.assertEqual(cmd[-2:],['--rebuild-standing-forecast-ticks',','.join(map(str,ticks))])

    def test_positive_preview_requires_precise_or_staged_access(self):
        r=fixture();m=P.analyze(r,100)
        self.assertEqual(m['precisely_reachable_positive'],1)
        r['attempts'][0]['precise_route']['diagnostics']['failure']='disconnected'
        m=P.analyze(r,100)
        self.assertEqual(m['precisely_reachable_positive'],0)
        self.assertEqual(m['accepted'][0]['route_kind'],'coarse_only')
        self.assertEqual(m['accepted'][0]['classification'],'positive')

    def test_every_in_range_node_and_accepted_offset_must_be_accounted_for(self):
        for mutate in (lambda r:r['map']['nodes'].append(dict(id=2,position=dict(x=2,y=0))),
                       lambda r:r['attempts'][0].update(forecasts=[]),
                       lambda r:r['attempts'][0].update(sparse_candidate=True),
                       lambda r:r.update(offset_checks=26),
                       lambda r:r.update(forecasts=65)):
            r=fixture();mutate(r)
            with self.assertRaises(AssertionError):P.analyze(r,100)

    def test_unexamined_budget_cases_cannot_be_reported_before_the_cap(self):
        r=fixture();r['attempts'][0]['forecasts'][0].update(forecast=None,unavailable='diagnostic forecast budget')
        with self.assertRaises(AssertionError):P.analyze(r,100)

    def test_forecast_work_and_epoch_are_checked_without_turning_scope_failure_positive(self):
        original=fixture()['attempts'][0]['forecasts'][0]['forecast']
        for mutate in (lambda f:f.update(start_delay=40),lambda f:f['chunks'][0].update(steps=5),
                       lambda f:f['input'].update(live_vehicle_available=True),lambda f:f.update(terrain_updates=True)):
            f=copy.deepcopy(original);mutate(f)
            with self.assertRaises(AssertionError):P.check_forecast(f,100,-8.)
        f=copy.deepcopy(original);f.update(stop='outside_model_scope',settles_within_horizon=None)
        self.assertIsNone(P.check_forecast(f,100,-8.)['prediction'])

    def test_staging_requires_a_measured_walk_only_first_leg(self):
        c=fixture()['attempts'][0]
        first=copy.deepcopy(c['precise_route']);first['diagnostics']['length']=3
        second=copy.deepcopy(first);second['diagnostics']['length']=23
        c['precise_route']['diagnostics']['length']=26
        c.update(staging=dict(walk_length=3,remaining_length=23),staging_routes=[first,second])
        self.assertEqual(P.route_kind(c),'staged')
        first['diagnostics']['jumps']=1
        with self.assertRaises(AssertionError):P.route_kind(c)

    def test_only_known_wall_time_fields_are_ignored_in_retained_selector_logs(self):
        f=fixture()['attempts'][0]['forecasts'][0]['forecast'];e=dict(kind='evaluated',forecast=f,accepted=True)
        changed=copy.deepcopy(e);changed['forecast']['timing']['setup_ms']+=10;changed['forecast']['chunks'][0]['elapsed_ms']+=10
        self.assertEqual(P.untimed_event(e),P.untimed_event(changed))
        changed['forecast']['samples'][0]['ship']['position']['x']+=1
        self.assertNotEqual(P.untimed_event(e),P.untimed_event(changed))
        self.assertIn('timing',e['forecast'])
        self.assertEqual(P.untimed_event(dict(kind='update',elapsed_ms=1,pending=True)),dict(kind='update',pending=True))

    def test_reaudit_cannot_change_runtime_or_plan(self):
        old={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(old,dict(old,**{p:'repair'}),True)
        for p in set(old)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(old,dict(old,**{p:'changed'}),True)


if __name__=='__main__':unittest.main()
