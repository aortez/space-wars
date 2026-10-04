import copy
import importlib.util
from pathlib import Path
import unittest


spec = importlib.util.spec_from_file_location('visits', Path(__file__).parents[1]/'audit-mission-visits.py')
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)
spec = importlib.util.spec_from_file_location('validation', Path(__file__).parents[1]/'validate-visit-metrics.py')
V = importlib.util.module_from_spec(spec)
spec.loader.exec_module(V)


def event(tick, kind, planet=1, reason=None):
    return dict(tick=tick, kind=kind, planet=planet, reason=reason)


def fixture():
    visit = dict(planet=1, selected_tick=1, arrived_tick=2, landed_tick=None,
                 claimed_tick=None, boarded_tick=None, departed_tick=None,
                 abandoned_tick=10, reason='pausing travel for nearby opponent')
    events = [event(1, 'selected'), event(2, 'arrived'),
              event(10, 'replan', reason='capture approach exhausted its time or retry budget'),
              event(11, 'replan', planet=None, reason=visit['reason']),
              event(11, 'pursuit_started', planet=None, reason='nearby opponent after securing ground')]
    return dict(metrics=[dict(visits=[visit]), dict(visits=[])],
                missions=[dict(events=events), dict(events=[])])


class MissionVisitAuditTests(unittest.TestCase):
    def test_pursuit_after_failure_cannot_rewrite_its_cause(self):
        report = fixture()
        original = copy.deepcopy(report)
        result = A.audit(report)
        visit = result['visits'][0]
        self.assertEqual(visit['terminal']['tick'], 10)
        self.assertEqual(visit['following_pursuit']['tick'], 11)
        self.assertEqual(visit['expected']['reason'], 'capture approach exhausted its time or retry budget')
        self.assertEqual(set(visit['corrections']), {'reason'})
        self.assertEqual(report, original)

    def test_completed_visit_cannot_be_abandoned_by_a_later_replan(self):
        report = fixture()
        report['missions'][0]['events'][2] = event(10, 'departed')
        report['metrics'][0]['visits'][0].update(departed_tick=10, abandoned_tick=11)
        visit = A.audit(report)['visits'][0]
        self.assertEqual(visit['outcome'], 'completed')
        self.assertEqual(visit['expected'], dict(departed_tick=10, abandoned_tick=None, reason=None))

    def test_unknown_terminal_reason_stays_unknown(self):
        report = fixture()
        report['missions'][0]['events'][2]['reason'] = None
        self.assertIsNone(A.audit(report)['visits'][0]['expected']['reason'])

    def test_same_tick_reselection_and_other_seat_are_distinct_attempts(self):
        report = fixture()
        report['missions'][0]['events'] = report['missions'][0]['events'][:3]+[
            event(10, 'selected'), event(11, 'arrived'), event(12, 'departed')]
        visit = dict(report['metrics'][0]['visits'][0], selected_tick=10, arrived_tick=11,
                     abandoned_tick=None, departed_tick=12, reason=None)
        report['metrics'][0]['visits'].append(visit)
        report['metrics'][1]['visits'].append(copy.deepcopy(visit))
        report['missions'][1]['events'] = [event(10, 'selected'), event(11, 'arrived'), event(12, 'departed')]
        result = A.audit(report)
        self.assertEqual([v['outcome'] for v in result['visits']], ['abandoned', 'completed', 'completed'])
        self.assertEqual([v['terminal']['tick'] for v in result['visits']], [10, 12, 12])
        self.assertTrue(all(v['following_pursuit'] is None for v in result['visits']))

    def test_unrelated_events_do_not_end_a_visit(self):
        report = fixture()
        report['missions'][0]['events'].insert(2, event(5, 'replan', planet=0, reason='other'))
        report['missions'][0]['events'].insert(3, event(6, 'departed', planet=None))
        self.assertEqual(A.audit(report)['visits'][0]['terminal']['tick'], 10)

    def test_truncated_history_and_unwitnessed_reselection_stay_unverified(self):
        for events in [[event(10, 'replan')], [event(1, 'selected'), event(5, 'selected', planet=0)]]:
            report = fixture()
            report['missions'][0]['events'] = events
            result = A.audit(report)
            self.assertEqual(result['counts']['unverified'], 1)
            self.assertNotIn('expected', result['visits'][0])

    def test_milestone_after_terminal_event_is_flagged_not_invented(self):
        report = fixture()
        report['metrics'][0]['visits'][0]['claimed_tick'] = 12
        visit = A.audit(report)['visits'][0]
        self.assertEqual(visit['milestones_outside_visit'], dict(claimed_tick=12))

    def test_unfinished_attempt_has_no_synthetic_endpoint(self):
        report = fixture()
        report['missions'][0]['events'] = report['missions'][0]['events'][:2]
        report['metrics'][0]['visits'][0].update(abandoned_tick=None, reason=None)
        visit = A.audit(report)['visits'][0]
        self.assertEqual(visit['outcome'], 'unfinished')
        self.assertIsNone(visit['terminal'])
        self.assertEqual(visit['corrections'], {})

    def test_ambiguous_identity_and_backward_event_clock_are_rejected(self):
        for mutation in ['duplicate_visit', 'duplicate_selection', 'backward']:
            report = fixture()
            if mutation == 'duplicate_visit':
                report['metrics'][0]['visits'] *= 2
            elif mutation == 'duplicate_selection':
                report['missions'][0]['events'].insert(1, event(1, 'selected'))
            else:
                report['missions'][0]['events'][1]['tick'] = 0
            with self.assertRaises(ValueError):
                A.audit(report)

    def test_replay_allows_only_the_independently_verified_terminal_correction(self):
        before = fixture()
        after = copy.deepcopy(before)
        after['metrics'][0]['visits'][0]['reason'] = before['missions'][0]['events'][2]['reason']
        self.assertEqual(V.compare_metrics(before, after)['counts']['corrected_visits'], 1)
        after['metrics'][0]['visits'][0]['landed_tick'] = 5
        with self.assertRaisesRegex(AssertionError, 'beyond terminal correction'):
            V.compare_metrics(before, after)


if __name__ == '__main__':
    unittest.main()
