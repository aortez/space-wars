import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('feedback', Path(__file__).parents[1]/'validate-walk-feedback.py')
M = importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)


class WalkFeedbackTests(unittest.TestCase):
    def test_plan_retains_all_cases_and_changes_only_feedback(self):
        items = M.P.plan()
        self.assertEqual(len(items), 14)
        self.assertEqual(sum(i['kind'] == 'directed' for i in items), 6)
        self.assertEqual(sum(i['kind'] == 'armed' for i in items), 8)
        for item in items:
            self.assertEqual(M.arguments(item, False), M.E.arguments(item, True))
            self.assertEqual(M.arguments(item, True), M.arguments(item, False) + ['--cover-walk-feedback', 'true'])

    def test_advances_require_a_matching_current_receipt_and_keep_the_limits(self):
        a, b = dict(planet=0, bearing=1), dict(planet=0, bearing=2)
        objective = dict(planet=0, revision=0, owner='player_2', position=dict(x=0, y=60), range=2.8)
        before = dict(seat=0, pilot=dict(tick=199, owner='player_1', site_query=dict(selected=a),
            planet=dict(index=0, revision=0)), objective_evidence=None,
            capture=dict(cover_response=dict(search=dict(planet=0, revision=0, objective=objective,
                started_tick=150, deadline_tick=750, probes=1, pending=[a,b]))))
        after = copy.deepcopy(before)
        after['pilot']['tick'] = 200
        after['objective_work'] = 'pending'
        after['objective_evidence'] = dict(tick=200, generation=7, request_tick=180,
            measurement_tick=180, objective=objective, source_objective=objective,
            exhausted_walk=dict(actor='player_1', site=a), invalidated_by=None, submission_deferred_by=None)
        after['capture']['cover_response']['search'].update(pending=[b], walk_deferred=[a], probes=2)
        audit = M.audit_feedback([before, after, after])
        self.assertEqual(len(audit['first_receipts']), 1)
        self.assertEqual(len(audit['advances']), 1)
        mutations = [
            lambda r: r['objective_evidence'].update(exhausted_walk=None),
            lambda r: r['objective_evidence'].update(measurement_tick=79),
            lambda r: r['objective_evidence'].update(request_tick=201),
            lambda r: r['objective_evidence'].update(invalidated_by='expired'),
            lambda r: r['objective_evidence']['exhausted_walk'].update(actor='player_2'),
            lambda r: r['objective_evidence']['exhausted_walk'].update(site=b),
            lambda r: r['objective_evidence']['source_objective'].update(revision=1),
            lambda r: r['capture']['cover_response']['search'].update(deadline_tick=800),
            lambda r: r['capture']['cover_response']['search'].update(probes=9),
            lambda r: r.update(objective_work='stale'),
        ]
        for mutate in mutations:
            row = copy.deepcopy(after)
            mutate(row)
            with self.assertRaises(AssertionError):
                M.audit_feedback([before, row])


if __name__ == '__main__':
    unittest.main()
