import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('bounds', Path(__file__).parents[1]/'validate-walk-bounds.py')
M = importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)


class WalkBoundsTests(unittest.TestCase):
    def test_plan_keeps_both_route_models_and_all_fixed_comparisons(self):
        self.assertEqual(len(M.P.plan()), 14)
        for item in M.P.plan():
            self.assertEqual(M.arguments(item, False), M.B.arguments(item, True))
            self.assertEqual(M.arguments(item, True), M.arguments(item, False) + ['--cover-walk-bounds', 'true'])

    def test_unsupported_notices_are_distinct_and_require_a_valid_source(self):
        a, b = dict(planet=0, bearing=1), dict(planet=0, bearing=2)
        objective = dict(planet=0, revision=0, owner='player_2', position=dict(x=0, y=60), range=2.8)
        before = dict(seat=0, pilot=dict(tick=199, owner='player_1', site_query=dict(selected=a),
            planet=dict(index=0, revision=0)), objective_evidence=None,
            capture=dict(cover_response=dict(search=dict(planet=0, revision=0, objective=objective,
                started_tick=150, deadline_tick=750, probes=1, pending=[a,b]))))
        after = copy.deepcopy(before)
        after['pilot']['tick'] = 200
        after['objective_work'] = 'pending'
        after['objective_evidence'] = dict(tick=200, generation=7, request_tick=199,
            measurement_tick=199, objective=objective, source_objective=objective,
            unsupported_walk=dict(actor='player_1', site=a, required_steps=226, max_steps=224),
            invalidated_by=None, submission_deferred_by=None)
        after['capture']['cover_response']['search'].update(pending=[b], walk_deferred=[a], probes=2)
        audit = M.audit_methods([before, after, after])
        self.assertEqual(len(audit['first_receipts']), 1)
        self.assertEqual(len(audit['advances']), 1)
        self.assertEqual(audit['first_receipts'][0]['kind'], 'unsupported')
        self.assertEqual(audit['advances'][0]['kind'], 'unsupported')
        for mutate in [
            lambda r: r['objective_evidence']['unsupported_walk'].update(required_steps=224),
            lambda r: r['objective_evidence']['unsupported_walk'].update(required_steps=261),
            lambda r: r['objective_evidence']['unsupported_walk'].update(max_steps=96),
            lambda r: r['objective_evidence']['unsupported_walk'].update(actor='player_2'),
            lambda r: r['objective_evidence']['unsupported_walk'].update(site=b),
            lambda r: r['objective_evidence'].update(exhausted_walk=dict(actor='player_1', site=a)),
            lambda r: r['objective_evidence'].update(unsupported_walk=None),
            lambda r: r['objective_evidence'].update(measurement_tick=79),
            lambda r: r['objective_evidence'].update(measurement_tick=200),
            lambda r: r['objective_evidence'].update(invalidated_by='expired'),
            lambda r: r['objective_evidence']['source_objective'].update(revision=1),
            lambda r: r['capture']['cover_response']['search'].update(deadline_tick=800),
            lambda r: r['capture']['cover_response']['search'].update(probes=9),
            lambda r: r.update(objective_work='stale'),
        ]:
            row = copy.deepcopy(after)
            mutate(row)
            with self.assertRaises(AssertionError):
                M.audit_methods([before, row])
        completed = copy.deepcopy(after)
        completed['objective_evidence'].pop('unsupported_walk')
        completed['objective_evidence']['exhausted_walk'] = dict(actor='player_1', site=a)
        self.assertEqual(M.audit_methods([before, completed])['advances'][0]['kind'], 'completed')


if __name__ == '__main__':
    unittest.main()
