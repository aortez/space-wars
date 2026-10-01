import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location('retry', Path(__file__).parents[1]/'validate-cover-retries.py')
R = importlib.util.module_from_spec(spec)
spec.loader.exec_module(R)


def capture(start, count, first):
    return dict(started_tick=start, cover_retry_cooldown=dict(rejected=[],
        blocked_selections=count, first_blocked_tick=first, last_blocked_tick=first))


class CoverRetryTests(unittest.TestCase):
    def test_snapshot_repetition_cannot_multiply_activations_or_merge_captures(self):
        a, b = capture(10, 3, 12), capture(30, 1, 32)
        report = dict(missions=[dict(capture=b), dict(capture=None)], samples=[],
            events=[dict(seat=0, telemetry=dict(capture=c)) for c in [a, a, b, a]])
        result = R.retry_stats(report, [0])
        self.assertEqual(result['affected_captures'], 2)
        self.assertEqual(result['blocked_selections'], 4)
        self.assertEqual(result['first_blocked_tick'], 12)
        self.assertEqual(len(result['captures']), 2)

    def test_option_identity_clock_and_memory_limit_are_verified(self):
        report = dict(missions=[dict(capture=capture(10, 1, 12))], events=[], samples=[])
        with self.assertRaises(AssertionError):
            R.retry_stats(report, [])
        for mutation in ['future', 'duplicate', 'unbounded', 'missing_clock', 'duration']:
            r = copy.deepcopy(report)
            m = r['missions'][0]['capture']['cover_retry_cooldown']
            entry = dict(site=dict(planet=1, bearing=0), revision=1, rejected_tick=10, until_tick=1810)
            if mutation == 'future': m['first_blocked_tick'] = 9
            elif mutation == 'duplicate': m['rejected'] = [entry, entry]
            elif mutation == 'unbounded': m['rejected'] = [dict(entry, site=dict(planet=1, bearing=i)) for i in range(9)]
            elif mutation == 'missing_clock': m['first_blocked_tick'] = m['last_blocked_tick'] = None
            else: m['rejected'] = [dict(entry, until_tick=1800)]
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                R.retry_stats(r, [0])

    def test_paired_plan_changes_only_cover_option_and_uses_fresh_worlds(self):
        previous_seeds = {p['seed'] for p in R.N.plan() if p['kind'] == 'held_out'}
        for item in R.plan():
            a, pa = R.arguments(dict(item, candidate=False))
            b, pb = R.arguments(dict(item, candidate=True))
            self.assertEqual(a[:-1], b[:-1])
            self.assertEqual(pa, pb)
            self.assertEqual(a[-1], 'none')
            self.assertEqual(b[-1], str(item['seat']))
            self.assertEqual(a[a.index('--survey-current-neutral')+1], str(item['seat']))
            if item['kind'] == 'held_out':
                self.assertNotIn(item['seed'], previous_seeds)

    def test_prefix_parity_ignores_only_option_telemetry_and_stops_at_activation(self):
        a = [dict(tick=t, actions=[1], mission={}) for t in range(4)]
        b = [dict(r, mission=dict(cover_retry_cooldown={})) for r in a]
        b[3]['actions'] = [2]
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory)/name for name in ['before.jsonl', 'after.jsonl']]
            def write():
                for path, rows in zip(paths, [a, b]):
                    path.write_text(''.join(json.dumps(row)+'\n' for row in rows))
            write()
            self.assertEqual(R.prefix_parity(*paths, 3)['rows'], 3)
            with self.assertRaises(AssertionError):
                R.prefix_parity(*paths, None)
            b[1]['actions'] = [2]
            write()
            with self.assertRaises(AssertionError):
                R.prefix_parity(*paths, 3)


if __name__ == '__main__':
    unittest.main()
