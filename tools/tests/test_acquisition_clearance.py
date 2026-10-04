import copy
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


C = module('clearance', Path(__file__).parents[1] / 'validate-acquisition-clearance.py')
F = module('defense_fixtures', Path(__file__).with_name('test_acquisition_defense.py'))


def fixture(admitted=False):
    trace, attempt = F.fixture()
    check = {key: copy.deepcopy(attempt[key]) for key in ('planet', 'vehicle', 'opponent', 'hit_source',
        'capture', 'native_actions', 'direction', 'estimated_min_range', 'estimated_clearance')}
    check.update(tick=100, decision='admitted' if admitted else 'negative_clearance')
    if not admitted:
        check['estimated_clearance'] = -1.
    previous = dict(checks=0, rejected=0, last=None)
    defense = dict(attempts=0, separated=0, timed_out=0, last=None)
    m = trace['mission']
    m[C.FIELD] = dict(checks=1, rejected=int(not admitted), last=check)
    if not admitted:
        m.update(capture=check['capture'], goal='capture', target=0, events=[])
        m[C.A.FIELD] = copy.deepcopy(defense)
    witness = dict(observation=trace['observation'], telemetry=m[C.FIELD])
    return trace, witness, previous, set(), defense


class AcquisitionClearanceTests(unittest.TestCase):
    def test_matrix_separates_legacy_parity_from_new_known_and_fresh_games(self):
        priors = {g:dict(off=dict(item=dict(seat=0,laser=True))) for g in C.KNOWN}
        cases = C.cases(priors)
        self.assertEqual(len(cases),31)
        self.assertEqual(len({c['name'] for c in cases}),31)
        known = [c for c in cases if c['stage']=='known_qualification']
        self.assertEqual(len(known),15)
        for group in C.KNOWN:
            self.assertEqual({c['mode'] for c in known if c['group']==group},{'off','legacy','on'})
        fresh = [c for c in cases if c['stage']=='held_out']
        self.assertEqual(len(fresh),16)
        self.assertEqual(len({c['seed']for c in fresh}),2)
        self.assertEqual({(c['world_cluster'],c['seat'],c['interval'])for c in fresh},
                         set(C.product(range(2),(0,1),(0,3))))
        self.assertTrue(all(c['clearance']==(c['mode']=='on') and c['defense']==(c['mode']!='off')for c in cases))
        self.assertTrue(all(c['laser'] and c['opponent']==10 for c in fresh))
        self.assertFalse({c['seed']for c in fresh}&{c['seed']for c in C.B.cases()})

    def test_commands_preserve_original_settings_and_explicitly_disable_new_option(self):
        old=['old','--seed','42','--out','old-output',C.A.FLAG,'1']
        before=list(old)
        for mode in ('off','legacy','on'):
            item=dict(seat=1,defense=mode!='off',clearance=mode=='on')
            command=C.command(old,'new','new-output',item)
            flags=dict(zip(command[1::2],command[2::2]))
            self.assertEqual(flags[C.FLAG],'1' if mode=='on' else 'none')
            self.assertEqual(flags[C.A.FLAG],'none' if mode=='off' else '1')
            self.assertEqual(flags['--seed'],'42')
        self.assertEqual(old,before)
        for bad in (old+['--seed','3'],old+[C.FLAG,'none']):
            with self.assertRaises(AssertionError):C.command(bad,'new','out',item)

    def test_rejected_proposal_must_keep_native_actions_capture_and_escape_state(self):
        C.audit_check(*fixture())
        for change in range(5):
            args=fixture();trace=args[0];m=trace['mission']
            if change==0:trace['actions'][0]['Scenario']['payload'][0]=1
            elif change==1:m['capture']=None
            elif change==2:m[C.A.FIELD]['attempts']=1
            elif change==3:m['target']=1
            else:m['events']=[dict(tick=100,kind='replan')]
            with self.subTest(change=change),self.assertRaises(AssertionError):C.audit_check(*args)

    def test_decision_follows_finite_forecast_sign_including_zero(self):
        args=fixture(True)
        args[0]['mission'][C.FIELD]['last']['estimated_clearance']=0.
        args[0]['mission'][C.A.FIELD]['last']['estimated_clearance']=0.
        C.audit_check(*args)
        for value in (1.,0.,float('nan'),float('inf')):
            args=fixture();args[0]['mission'][C.FIELD]['last']['estimated_clearance']=value
            with self.subTest(value=value),self.assertRaises(AssertionError):C.audit_check(*args)

    def test_admitted_receipt_must_match_actual_attempt_not_a_new_forecast(self):
        C.audit_check(*fixture(True))
        for key,value in (('estimated_clearance',40.),('estimated_min_range',80.),('vehicle',1)):
            args=fixture(True)
            args[0]['mission'][C.A.FIELD]['last'][key]=value
            with self.subTest(key=key),self.assertRaises(AssertionError):C.audit_check(*args)

    def test_current_clock_and_uncommitted_history_are_required_even_for_rejections(self):
        args=fixture();args[0]['mission'][C.FIELD]['last']['tick']=99
        with self.assertRaises(AssertionError):C.audit_check(*args)
        args=fixture();args[3].add(90)
        with self.assertRaises(AssertionError):C.audit_check(*args)
        args=fixture();args[2]['checks']=1
        with self.assertRaises(AssertionError):C.audit_check(*args)

    def test_projection_removes_only_option_telemetry(self):
        value=dict(acquisition_clearance=dict(checks=1),acquisition_clearance_model='profile',
                   acquisition_defense=dict(attempts=1),actions=[1,2],damage=5)
        self.assertEqual(C.strip_gate(value),dict(acquisition_defense=dict(attempts=1),actions=[1,2],damage=5))
        self.assertEqual(C.strip_options(value),dict(actions=[1,2],damage=5))

    def test_rejections_cannot_hide_changes_before_the_first_accepted_handoff(self):
        before=dict(tick=100,seat=0,actions=[1],mission=dict(capture='native'))
        after=copy.deepcopy(before)
        after['mission'].update(acquisition_defense=dict(attempts=0),acquisition_clearance=dict(rejected=1))
        self.assertIsNone(C.prefix(iter([before]),iter([after]),None)['tick'])
        after['actions']=[2]
        with self.assertRaises(AssertionError):C.prefix(iter([before]),iter([after]),None)

    def test_survival_regression_is_reported_even_when_outcome_and_counts_match(self):
        x=dict(outcome='loss',ships_lost=1,pilot_deaths=1,death_tick=18397)
        y=dict(x,death_tick=15971)
        self.assertEqual(C.changes(x,y),([],['earlier_pilot_death']))
        self.assertEqual(C.changes(y,x),(['later_pilot_death'],[]))
        self.assertEqual(C.changes(dict(x,death_tick=None),dict(y,death_tick=None)),([],[]))

    def test_fresh_gate_rejects_earlier_death_despite_an_improvement_elsewhere(self):
        pairs=[];runs={}
        for world,interval,seat in C.product(range(2),(0,3),(0,1)):
            group=f'{world}-{interval}-{seat}'
            pair=dict(group=group,stage='held_out',world_cluster=world,interval=interval,seat=seat,
                off=group+'-off',on=group+'-on',benefits=[],regressions=[],first_control_difference=None)
            pairs.append(pair)
            for arm in ('off','on'):runs[pair[arm]]=dict(players=[{},{}])
        pairs[0]['benefits']=['more_match_points']
        pairs[1]['regressions']=['earlier_pilot_death']
        with patch.object(C.I,'totals',return_value=dict(points=4,ships_lost=2,pilot_deaths=2)):
            result=C.screen(runs,pairs)
        self.assertEqual(result['decision'],'retain')
        self.assertEqual(result['reasons'],['earlier_pilot_death:'+pairs[1]['group']])


if __name__=='__main__':unittest.main()
