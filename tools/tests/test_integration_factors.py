import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('factors', Path(__file__).resolve().parents[1] / 'diagnose-integration-factors.py')
Q = importlib.util.module_from_spec(spec); spec.loader.exec_module(Q)


def source(binary=Path('/source/binary')):
    runs = {}
    for item in Q.S.cases():
        if item['group'] in Q.GROUPS and item['configuration'] in ('control', 'candidate'):
            runs[item['name']] = dict(item=item, command=[str(binary),
                *(v for pair in Q.S.flags(item).items() for v in pair), '--out', '/source/' + item['name']])
    return dict(complete=True, profile=Q.S.PROFILE, runtime_source_commit='qualified-runtime',
                binary=dict(path=str(binary)), runs=runs)


def report(item):
    mask, seat = item['mask'], item['seat']
    result = dict(physics_ok=True, audit_failures=[], seed=item['seed'], mode='duel', seat=0,
        landing_survey_hz=4, termination='round_finished',
        combat_breaks=dict(interval_seconds=15, duration_seconds=4), mission_evaluation={},
        pursuit_disengagement=dict(enabled_seats=[False, False], probe_handoff=False,
                                   boundary_guidance=False, destination_cover_probe=False),
        live_objective_planning=dict(enabled_seats=[0, 1], allowance=dict(graph=4, physics_queries=384),
                                    objective_dependencies='routes',
                                    **{k.replace('-', '_'): True for k in Q.P.ROUTE_OPTIONS}))
    descriptors=[]
    for s in (0, 1):
        d=dict(policy='material_mission_v13' if s==seat else 'material_mission_v10',
               sensor_profile='mission_jetpack' if mask & 4 and s==seat else 'mission_joint')
        if s==seat:
            if mask & 1:
                d.update(flag_cost_model=Q.P.MODELS['flag_cost_model'],
                         current_neutral_model=Q.P.MODELS['current_neutral_model'])
            if mask & 2: d['destination_retry_model']=Q.P.MODELS['destination_retry_model']
        descriptors.append(d)
    result['policy_configuration']=descriptors
    for key, bit in (('powered_capture',4),('active_flight_checks',4),('destination_retry',2)):
        if mask & bit:
            result[key]=dict(profile=Q.P.PROFILES[key],enabled_seats=[s==seat for s in (0,1)])
    if mask & 1:
        for key in ('flag_cost_admission','current_neutral_survey'):
            result['mission_evaluation'][key]=dict(enabled_seats=[s==seat for s in (0,1)])
    return result


class IntegrationFactorTests(unittest.TestCase):
    def test_full_factorial_covers_failures_and_gains_without_new_worlds(self):
        cases=Q.cases(source())
        self.assertEqual(len(cases),32)
        self.assertEqual(len({c['name'] for c in cases}),32)
        self.assertEqual({c['stage'] for c in cases},{'selected_diagnosis'})
        self.assertEqual({c['seat'] for c in cases},{0,1})
        for group in Q.GROUPS:
            selected=[c for c in cases if c['group']==group]
            self.assertEqual([c['mask'] for c in selected],list(Q.ORDER))
            self.assertEqual(len({c['seed'] for c in selected}),1)

    def test_contrasts_cover_each_factor_in_every_other_factor_context(self):
        pairs=Q.contrasts()
        self.assertEqual(len(pairs),16)
        for bit in (1,2,4):
            edges=[(a,b) for a,b in pairs if a^b==bit]
            self.assertEqual(len(edges),4)
            self.assertEqual({a for a,b in edges},{m for m in range(8) if not m & bit})
        self.assertTrue({(0,m) for m in range(1,8)} <= set(pairs))

    def test_each_factor_changes_only_its_dependent_options(self):
        allowed={1:{'--admit-flag-costs','--survey-current-neutral'},
                 2:{'--destination-retry-seats'},4:{'--powered-capture-seats','--active-flight-checks'}}
        cases=Q.cases(source())
        for group in Q.GROUPS:
            by_mask={c['mask']:Q.flags(c) for c in cases if c['group']==group}
            for a,b in Q.contrasts():
                bit=a^b
                if bit in allowed:
                    self.assertEqual({k for k in by_mask[a] if by_mask[a][k]!=by_mask[b][k]},allowed[bit])
            for flags in by_mask.values():
                for option in ('--pursuit-climb-laser-seats','--acquisition-defense-seats','--acquisition-clearance-seats'):
                    self.assertEqual(flags[option],'none')
                self.assertEqual((flags['--seconds'],flags['--require-finish']),('600','true'))

    def test_endpoint_commands_retain_original_observers_and_gameplay_options(self):
        prior=source(); cases=Q.cases(prior)
        for item in cases:
            if item['mask'] not in (0,7):continue
            old=prior['runs'][item['group']+('-candidate' if item['mask'] else '-control')]
            self.assertEqual([v for pair in Q.flags(item).items() for v in pair],old['command'][1:-2])
        for item in cases:
            flags=Q.flags(item)
            self.assertEqual('--trace-impact' in flags,item['group']=='known-rescue')
            if '--trace-impact' in flags:self.assertEqual(flags['--impact-pod-control'],'bot')

    def test_partial_configuration_is_audited_independently_in_both_seats(self):
        for item in Q.cases(source()):
            data=report(item); Q.check_configuration(data,item)
            for other in range(8):
                if other!=item['mask']:
                    with self.assertRaises(AssertionError):Q.check_configuration(data,dict(item,mask=other))

    def test_wrong_seat_profile_break_or_optional_combat_cannot_pass(self):
        item=next(c for c in Q.cases(source()) if c['mask']==7)
        for mutate in (
            lambda r:r['powered_capture'].update(enabled_seats=[True,True]),
            lambda r:r['policy_configuration'][item['seat']].update(destination_retry_model='wrong'),
            lambda r:r['mission_evaluation']['flag_cost_admission'].update(enabled_seats=[False,False]),
            lambda r:r['combat_breaks'].update(duration_seconds=3),
            lambda r:r.update(acquisition_defense={}),
            lambda r:r.update(pursuit_climb_laser={}),
            lambda r:r.update(termination='horizon'),
        ):
            data=report(item);mutate(data)
            with self.assertRaises(AssertionError):Q.check_configuration(data,item)

    def test_frozen_plan_rejects_case_binary_source_and_runtime_mutations(self):
        with tempfile.TemporaryDirectory() as directory:
            out=Path(directory); binary=out/'binary'; binary.write_bytes(b'qualified binary')
            old=source(binary);old['binary']['sha256']=Q.P.digest(binary)
            path=out/'source.json';path.write_text(json.dumps(old))
            frozen={'runtime.rs':'runtime',Q.OWN[0]:'runner',Q.OWN[2]:'plan'}
            plan=dict(profile=Q.PROFILE,inputs=frozen,source=dict(path=str(path),sha256=Q.P.digest(path)),
                jobs=Q.jobs(old,binary,out),binary=old['binary'],runtime_source_commit=old['runtime_source_commit'],
                replay_sources={g+'-'+Q.label(m):old['runs'][g+('-candidate' if m else '-control')]
                                for g in Q.GROUPS for m in (0,7)})
            with patch.object(Q,'inputs',return_value=frozen):
                Q.verify_plan(plan,out)
                for mutate in (lambda p:p['jobs'].pop(),lambda p:p['jobs'][0]['item'].update(mask=1),
                               lambda p:p.update(runtime_source_commit='different'),
                               lambda p:p['replay_sources'].clear(),lambda p:p['binary'].update(sha256='wrong')):
                    changed=copy.deepcopy(plan);mutate(changed)
                    with self.assertRaises(AssertionError):Q.verify_plan(changed,out,True)
            with patch.object(Q,'inputs',return_value={**frozen,Q.OWN[0]:'revised auditor'}):
                with self.assertRaises(AssertionError):Q.verify_plan(plan,out)
                Q.verify_plan(plan,out,True)
            for key in ('runtime.rs',Q.OWN[2]):
                with patch.object(Q,'inputs',return_value={**frozen,key:'changed'}):
                    with self.assertRaises(AssertionError):Q.verify_plan(plan,out,True)
            path.write_text('{}')
            with patch.object(Q,'inputs',return_value=frozen):
                with self.assertRaises(AssertionError):Q.verify_plan(plan,out,True)

    def test_diagnosis_requires_all_cases_and_pairs_and_never_promotes(self):
        prior=source(); cases=Q.cases(prior); runs={c['name']:dict(item=c) for c in cases}
        pairs=[dict(group=g,before_mask=a,after_mask=b,factor=Q.FACTORS.get(a^b),
                    first_control_difference=None,benefits=[],regressions=[])
               for g in Q.GROUPS for a,b in Q.contrasts()]
        result=Q.diagnosis(runs,pairs,prior)
        self.assertEqual(result['decision'],'diagnosis_only')
        self.assertFalse(result['default_promotion'])
        with self.assertRaises(AssertionError):Q.diagnosis(runs,pairs[:-1],prior)
        duplicate=copy.deepcopy(pairs);duplicate[-1]=duplicate[0]
        with self.assertRaises(AssertionError):Q.diagnosis(runs,duplicate,prior)
        changed=copy.deepcopy(runs);changed[cases[0]['name']]['item']['mask']=3
        with self.assertRaises(AssertionError):Q.diagnosis(changed,pairs,prior)


if __name__=='__main__':unittest.main()
