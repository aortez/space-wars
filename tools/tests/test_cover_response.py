import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('response', Path(__file__).parents[1]/'validate-cover-response.py')
Q = importlib.util.module_from_spec(spec)
spec.loader.exec_module(Q)


def capture(start=10, first=12):
    return dict(started_tick=start, cover_response=dict(required_since=start+1, first_effect_tick=first,
        search=None, **dict.fromkeys(Q.COUNTERS, 0),) | dict(failures=1, searches=1,
            filtered_directions=3, requested_sites=2, measured_sites=1))


class CoverResponseTests(unittest.TestCase):
    def test_repeated_snapshots_do_not_multiply_searches(self):
        a, b = capture(), capture(30, 32)
        report = dict(missions=[dict(capture=b)], events=[dict(seat=0, telemetry=dict(capture=c)) for c in [a, a, b]], samples=[])
        result = Q.response_stats(report, [0])
        self.assertEqual(result['affected_captures'], 2)
        self.assertEqual(result['totals']['requested_sites'], 4)
        self.assertEqual(result['first_effect_tick'], 12)
        with self.assertRaises(AssertionError):
            Q.response_stats(report, [])

    def test_limits_and_unknown_classification_are_audited(self):
        c = capture()
        state = c['cover_response']
        state['requested_sites'] = state['measured_sites'] = 8
        state['search'] = dict(planet=1, revision=2, objective=None, started_tick=11, deadline_tick=611,
            hold_altitude=80, seeded=True, pending=[], probes=8, omitted=2,
            finished_tick=300, outcome='evidence_budget_exhausted', guidance=None)
        Q.check_state(c, state)
        for mutation in ['deadline', 'probes', 'duplicate', 'false_exhaustion', 'premature_budget', 'future']:
            bad = copy.deepcopy(state)
            s = bad['search']
            if mutation == 'deadline': s['deadline_tick'] += 1
            elif mutation == 'probes': s['probes'] = 9
            elif mutation == 'duplicate': s['pending'] = [dict(planet=1, bearing=2)] * 2
            elif mutation == 'false_exhaustion': s['outcome'] = 'observed_candidates_exhausted'
            elif mutation == 'premature_budget': s['probes'] = 7
            else: bad['first_effect_tick'] = 9
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                Q.check_state(c, bad)

    def test_disabled_metadata_has_no_state(self):
        report = dict(missions=[dict(capture=dict(started_tick=10))], events=[], samples=[])
        self.assertEqual(Q.response_stats(report, [])['affected_captures'], 0)
        with self.assertRaises(AssertionError):
            Q.response_stats(report, [0])

    def test_new_worlds_and_paired_commands_change_only_the_new_option(self):
        previous = {p['seed'] for source in [Q.F, Q.N, Q.R] for p in source.plan() if p['kind'] == 'held_out'}
        for item in Q.plan():
            a, pa = Q.arguments(dict(item, candidate=False))
            b, pb = Q.arguments(dict(item, candidate=True))
            self.assertEqual(a[:-1], b[:-1])
            self.assertEqual(pa, pb)
            self.assertEqual(a[-1], 'none')
            self.assertEqual(b[-1], str(item['seat']))
            self.assertNotIn('--cover-retry-seats', b)
            if item['kind'] == 'held_out':
                self.assertNotIn(item['seed'], previous)
        self.assertEqual(len(Q.plan()), 64)

    def test_prefix_uses_world_clock_and_normalizes_only_this_option(self):
        a = [dict(tick=t, actions=[1], mission={}, observation={
            'local': {'combat': {'recovery': {'flight': {'pilot': {'tick': t+1}}}}}}) for t in range(4)]
        b = [dict(r, mission=dict(cover_response={})) for r in a]
        b[3]['actions'] = [2]
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory)/name for name in ['before.jsonl', 'after.jsonl']]
            def write():
                for path, rows in zip(paths, [a, b]):
                    path.write_text(''.join(json.dumps(row)+'\n' for row in rows))
            write()
            self.assertEqual(Q.prefix_parity(*paths, 4)['rows'], 3)
            with self.assertRaises(AssertionError):
                Q.prefix_parity(*paths, None)
            b[1]['mission']['cover_retry_cooldown'] = {}
            write()
            with self.assertRaises(AssertionError):
                Q.prefix_parity(*paths, 4)


if __name__ == '__main__':
    unittest.main()
