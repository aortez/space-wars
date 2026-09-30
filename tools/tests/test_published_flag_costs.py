import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('flag_costs', Path(__file__).parents[1]/'validate-flag-costs.py')
F = importlib.util.module_from_spec(spec)
spec.loader.exec_module(F)


class FlagCostAuditTests(unittest.TestCase):
    def test_shared_budget_includes_evaluator_and_unpublished_flag_work(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'live-planning.csv').write_text('tick,graph,queries,graph_budget,query_budget\n1,1,100,4,384\n')
            evaluation = dict(tick=1, remaining_before_evaluation=dict(graph=3, physics_queries=284),
                allowance=dict(graph=3, physics_queries=0), charged=dict(graph=2, physics_queries=0))
            flag = dict(tick=1, remaining_after_evaluation=dict(graph=1, physics_queries=284),
                allocation=dict(tick=1, allowance=dict(graph=1, physics_queries=284),
                    charged=dict(graph=1, physics_queries=250),
                    jobs=[dict(charged=dict(graph=1, physics_queries=250))]))
            def save():
                (root/'mission-evaluation-work.jsonl').write_text(json.dumps(evaluation)+'\n')
                (root/'flag-survey-work.jsonl').write_text(json.dumps(flag)+'\n')
            save()
            self.assertEqual(F.allocation_audit(root)['maximum'], dict(graph=4, physics_queries=350))
            # Each planner's own allowance could pass while their sum exceeds
            # the shared quota; the recorded residual must also agree.
            flag['allocation']['charged']['graph'] = 2
            flag['allocation']['allowance']['graph'] = 2
            flag['allocation']['jobs'][0]['charged']['graph'] = 2
            save()
            with self.assertRaises(AssertionError):
                F.allocation_audit(root)

    def test_first_forecast_stays_frozen_and_abandoned_trips_are_retained(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'flag-survey.jsonl').write_text('')
            visit = dict(planet=1, selected_tick=1, arrived_tick=2, landed_tick=None,
                claimed_tick=None, boarded_tick=None, departed_tick=None, abandoned_tick=20)
            report = dict(metrics=[dict(visits=[visit]), dict(visits=[])])
            candidate = dict(planet=1, current=True, total_seconds=10.0, travel_seconds=2.0,
                local=dict(landing=3, exit=0.1, outbound=0.2, claim=3, return_board=0.1, departure=1.6))
            first = dict(actor='player_1', source_tick=3, selected_tick=1, candidates=[candidate])
            later = dict(first, source_tick=6, candidates=[dict(candidate, total_seconds=1.0)])
            path = root/'mission-evaluations.jsonl'
            path.write_text('\n'.join(map(json.dumps, [first, later]))+'\n')
            result = F.first_predictions(report, path)
            self.assertEqual(result['outcomes'], dict(abandoned=1))
            attempt = result['attempts'][0]
            self.assertEqual(attempt['source_forecast']['source_tick'], 3)
            self.assertEqual(attempt['candidate']['total_seconds'], 10.0)
            self.assertEqual(attempt['already_observed'], dict(arrived_tick=2))
            self.assertTrue(all(r['actual_seconds'] is None for r in attempt['references']['records']))

    def test_unpublished_cost_cannot_pass_the_evidence_audit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'flag-survey.jsonl').write_text('')
            row = dict(actor='player_1', source_tick=10, candidates=[], flag_admissions=[dict(
                reason=None, used=True, source_tick=1, completed_tick=11, validated_tick=None)])
            path = root/'mission-evaluations.jsonl'
            path.write_text(json.dumps(row)+'\n')
            with self.assertRaises(AssertionError):
                F.first_predictions(dict(metrics=[dict(visits=[]), dict(visits=[])]), path)


if __name__ == '__main__':
    unittest.main()
