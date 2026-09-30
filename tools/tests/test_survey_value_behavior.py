import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import gzip
from test_flag_value_shadow import fixture

SPEC = importlib.util.spec_from_file_location('survey_value', Path(__file__).resolve().parents[1]/'compare-survey-value.py')
T = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(T)


def initial_world(tick=1):
    return dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=dict(tick=tick))))))


class SurveyValueBehaviorTests(unittest.TestCase):
    def test_predeclared_plan_keeps_both_seats_all_bearings_and_held_out_pairs(self):
        plan = T.plan()
        self.assertEqual(len(plan), 58)
        self.assertEqual(len({r['name'] for r in plan}), 58)
        held = [r for r in plan if r['kind'] == 'held_out']
        self.assertEqual(len(held), 24)
        self.assertEqual(len({r['seed'] for r in held}), 4)
        for seed in {r['seed'] for r in held}:
            for interval in [0, 3]:
                self.assertEqual({(r['version'],r['seat']) for r in held if r['seed']==seed and r['interval']==interval},
                    {(13,None),(14,0),(14,1)})
        for world in ['destination', 'value-destination']:
            for seat in range(2):
                cases = [r for r in plan if r.get('world')==world and r['seat']==seat]
                self.assertEqual({(r['bearing'],r['version']) for r in cases},
                    {(b,v) for b in [0.,.4,.8,1.2] for v in [13,14]})

    def test_duel_observer_stays_fixed_while_candidate_seat_changes(self):
        for item in T.plan():
            args, policies = T.arguments(item)
            args = dict(zip(args[::2],args[1::2]))
            self.assertEqual(args['--seat'], str(item['seat'] if item['kind']=='directed' else 0))
            for seat in range(2):
                other = 10 if item['kind']=='regression' else 13
                self.assertEqual(policies[seat], item['version'] if seat==item['seat'] else other)

    def test_consumed_references_bind_actual_sources_and_reject_renewed_or_changed_costs(self):
        shadow, _, samples = fixture()
        r = shadow[0]['augmented']
        r.update(policy='material_mission_v14', model='capture_mission_survey_value_v1')
        c = r['candidates'][0]
        c.update(evidence_kind=T.FLAG_KIND, evidence_age_ticks=40, revision=3,
            observed_owner='player_2', first_rebuild_foothold=False)
        s = samples[0]
        s.update(measurement=dict(tick=60, revision=3, finding='measured',ship_form='ship',climb_clear=True,
            site=dict(id=s['site'],revision=3,hatch_has_settling_margin=True,boarding_hatches=[{},None])),
            objective=dict(planet=1,revision=3, owner='player_2',position=dict(x=0,y=50),range=2.8))
        s['validation'].update(model='captured_query_unions_v1',source_radius=50,source_objective=copy.deepcopy(s['objective']),
            complete=True, predicates_valid=True, predicate_failure=None, geometry=dict(valid=True))
        s['route'].update(site=s['site'],endpoint=dict(position=dict(x=0,y=50)))
        for leg in ['outbound','returning']:
            s['route'][leg].update(failure=None,partial=False,jumps=0,flights=0)
        self.assertEqual(T.flag_references([r], samples)['distinct_surveys'], 1)
        for mutation in range(13):
            bad, source = copy.deepcopy(r), copy.deepcopy(samples)
            candidate = bad['candidates'][0]
            if mutation == 0: bad['policy']='material_mission_v13'
            elif mutation == 1: candidate['evidence_tick']+=1
            elif mutation == 2: candidate['evidence_age_ticks']=0
            elif mutation == 3: source[0]['validated_tick']+=1
            elif mutation == 4: candidate['local']['outbound']+=1
            elif mutation == 5: candidate['value']['priority_units']=1
            elif mutation == 6: source[0]['reason']='rejected geometry'
            elif mutation == 7: source[0]['validation']['geometry']['valid']=False
            elif mutation == 8: source[0]['validation']['source_objective']['planet']=2
            elif mutation == 9: source[0]['route']['site']=dict(planet=2,bearing=0)
            elif mutation == 10: source[0]['measurement']['finding']='unavailable'
            elif mutation == 11: source[0]['route']['outbound']['length']=100
            elif mutation == 12: source[0]['measurement']['site']['boarding_hatches']=[None,None]
            with self.subTest(mutation=mutation), self.assertRaises((AssertionError,KeyError)):
                T.flag_references([bad], source)

    def test_range_progress_counts_measured_non_improvement_and_resets_after_gain(self):
        def row(tick, distance=100, goal='transfer', selected=1):
            return dict(tick=tick,seat=0,goal=goal,target=1,selected_tick=selected,aboard=True,
                ship_available=True,ship_form='ship',distance_to_target=distance,owned_planets=[1,1])
        samples = [row(tick) for tick in range(602)]
        result = T.progress(samples)['players'][0]
        self.assertEqual(result['longest_no_range_gain_ticks'],601)
        self.assertEqual(result['ticks_without_range_gain_after_10s'],2)
        samples.extend(row(tick, 97) for tick in range(602,610))
        self.assertEqual(T.progress(samples)['players'][0]['ticks_without_range_gain_after_10s'],2)
        # Small gains accumulate, but a phase/visit reset starts a new measure.
        samples = [row(tick,100-tick*.01) for tick in range(800)]
        self.assertEqual(T.progress(samples)['players'][0]['ticks_without_range_gain_after_10s'],0)
        samples = [row(t, goal='capture' if t==400 else 'transfer') for t in range(800)]
        self.assertEqual(T.progress(samples)['players'][0]['ticks_without_range_gain_after_10s'],0)
        samples = [row(t,selected=1 if t<400 else 400) for t in range(800)]
        self.assertEqual(T.progress(samples)['players'][0]['ticks_without_range_gain_after_10s'],0)
        with self.assertRaises(AssertionError): T.progress([row(1),row(3)])

    def test_work_audit_reconciles_all_three_stages_and_refuses_reused_budget(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            (root/'live-planning.csv').write_text('tick,graph,queries,graph_budget,query_budget,total_graph,total_queries\n1,1,100,4,384,1,100\n')
            e=dict(tick=1,remaining_before_evaluation=dict(graph=3,physics_queries=284),
                allowance=dict(graph=3,physics_queries=0),charged=dict(graph=2,physics_queries=0))
            f=dict(tick=1,remaining_after_evaluation=dict(graph=1,physics_queries=284),
                allocation=dict(charged=dict(graph=1,physics_queries=50),
                    jobs=[dict(charged=dict(graph=1,physics_queries=50))]))
            report=dict(elapsed_ticks=1,initial_world=initial_world(),mission_evaluation=dict(charged=2),
                flag_survey=dict(telemetry=dict(graph=1,physics_queries=50)))
            for mutation in range(6):
                bad, evaluated=copy.deepcopy(f),copy.deepcopy(e)
                if mutation==1: bad['remaining_after_evaluation']['graph']=2
                if mutation==2: bad['allocation']['charged']['physics_queries']=400
                if mutation==3: bad['allocation']['jobs'][0]['charged']['graph']=0
                if mutation==4: bad['tick']=2
                if mutation==5: bad['tick']=evaluated['tick']=2
                (root/'mission-evaluation-work.jsonl').write_text(json.dumps(evaluated)+'\n')
                (root/'flag-survey-work.jsonl').write_text(json.dumps(bad)+'\n')
                if mutation==0:
                    self.assertEqual(T.work_audit(root,report)['maximum_combined_work'],dict(graph=4,physics_queries=150))
                else:
                    with self.subTest(mutation=mutation), self.assertRaises(AssertionError): T.work_audit(root,report)

    def test_trace_requires_every_tick_and_actor_including_its_prefix_and_suffix(self):
        report=dict(mode='duel',seat=0,initial_world=initial_world(),elapsed_ticks=3)
        valid=[dict(tick=t,seat=s) for t in range(1,4) for s in range(2)]
        self.assertEqual(list(T.validated_trace(valid,report)),valid)
        for bad in [valid[1:],valid[:-1],valid[::2],[],valid[:2]+valid[3:],valid+[valid[-1]]]:
            with self.assertRaises(AssertionError): list(T.validated_trace(bad,report))
        report.update(mode='quiet',seat=1)
        self.assertEqual(len(list(T.validated_trace([r for r in valid if r['seat']==1],report))),3)

    def test_later_switch_does_not_excuse_truncation_before_it(self):
        report=dict(mode='duel',seat=0,initial_world=initial_world(),elapsed_ticks=10)
        valid=[dict(tick=t,seat=s,policy='test') for t in range(1,11) for s in range(2)]
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            for name,records in [('a',valid),('b',valid[:16])]:
                (root/name).mkdir()
                with gzip.open(root/name/'destination-behavior.jsonl.gz','wt') as f:
                    for row in records: f.write(json.dumps(row)+'\n')
            for length in [10,8]:
                # Reject both a truncated trace and a complete shorter run
                # that already diverged before the other arm's later switch.
                other=dict(report,elapsed_ticks=length)
                with patch.object(T.D,'finished_player',return_value=dict(switches=[dict(tick=9)])), \
                        patch.object(T.D,'same_physical_outcomes',return_value=True),self.assertRaises(AssertionError):
                    T.compare_controls(root,'a','b',[report,other])


if __name__=='__main__': unittest.main()
