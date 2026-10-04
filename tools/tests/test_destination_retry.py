import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('retry', Path(__file__).parents[1]/'validate-destination-retry.py')
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)


def memory():
    failure = dict(context=dict(planet=1, revision=0), selected_tick=1, started_tick=2,
        failed_tick=10, kind='incomplete_search', reason='unmeasured candidates', site=None, cover_search=None)
    return dict(failures=[failure], **dict.fromkeys(A.COUNTERS, 0)) | dict(
        recorded_failures=1, initial_changes=1, first_effect_tick=20,
        last_decision=dict(tick=20, path='initial', rejected=1, retained=2, failure=failure), last_retry=None)


class DestinationRetryTests(unittest.TestCase):
    def test_final_persistent_counters_do_not_double_count_snapshots(self):
        m = memory()
        report = dict(missions=[dict(destination_retry=m)], events=[dict(seat=0, telemetry=dict(destination_retry=copy.deepcopy(m)))] * 3)
        result = A.retry_stats(report, [0])
        self.assertEqual(result['0']['initial_changes'], 1)
        self.assertEqual(result['0']['first_effect_tick'], 20)

    def test_invalid_history_or_false_effect_is_rejected(self):
        for mutation in ['visit', 'future', 'effect', 'duplicate', 'planet', 'empty_origin', 'reset']:
            m = memory()
            report = dict(missions=[dict(destination_retry=m)])
            if mutation == 'visit': m['failures'][0]['started_tick'] = 0
            elif mutation == 'future': m['last_decision']['tick'] = 5
            elif mutation == 'effect': m['first_effect_tick'] = None
            elif mutation == 'duplicate': m['failures'] *= 2
            elif mutation == 'planet': m['last_decision']['rejected'] = 2
            elif mutation == 'empty_origin': m['recorded_failures'] = 0
            else:
                old = copy.deepcopy(m);old['recorded_failures'] += 1
                report['events'] = [dict(seat=0, telemetry=dict(destination_retry=old))]
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                A.retry_stats(report, [0])

    def test_unchanged_retry_is_not_an_effect_and_disabled_memory_is_absent(self):
        m = memory();f = m['failures'][0]
        m.update(initial_changes=0, first_effect_tick=None, last_decision=None,
            retry_selections=1, last_retry=dict(tick=1811, path='initial', failure=f))
        result = A.retry_stats(dict(missions=[dict(destination_retry=m)]), [0])
        self.assertIsNone(result['0']['first_effect_tick'])
        self.assertEqual(A.retry_stats(dict(missions=[{}]), []), {})
        with self.assertRaises(AssertionError):
            A.retry_stats(dict(missions=[dict(destination_retry=m)]), [])

    def test_new_worlds_and_paired_options_are_fixed(self):
        old = {p['seed'] for source in [A.Q, A.N, A.F, A.Q.R] for p in source.plan() if p['kind'] == 'held_out'}
        plan = A.plan()
        self.assertEqual(len(plan), 32)
        self.assertEqual(len({p['seed'] for p in plan}), 4)
        for item in plan:
            a, pa = A.arguments(dict(item, candidate=False))
            b, pb = A.arguments(dict(item, candidate=True))
            self.assertEqual(pa, pb)
            self.assertEqual(a[:-1], b[:-1])
            self.assertEqual(a[-1], 'none')
            self.assertEqual(b[-1], str(item['seat']))
            self.assertNotIn(item['seed'], old)
            self.assertNotIn('--cover-response-seats', b)

    def test_recorded_cases_include_both_cover_settings_and_successful_retry(self):
        study = dict(regressions={case[0]:{arm:{} for arm in ['predecessor','candidate']} for case in A.Q.A.CASES},
            runs={name+'-'+arm:{} for name in A.EXTRA_CASES for arm in ['predecessor','candidate']})
        plan = A.recorded_plan(study)
        self.assertEqual(len(plan), 16)
        self.assertEqual(len({p['name'] for p in plan}), 16)
        self.assertTrue(any('world2-asteroids0-p1-cover-candidate' in p['name'] for p in plan))

    def test_prefix_uses_world_tick_and_only_strips_the_new_memory(self):
        before = [dict(tick=t, controls=[1], mission={}, observation={
            'local': {'combat': {'recovery': {'flight': {'pilot': {'tick': t+1}}}}}}) for t in range(4)]
        after = copy.deepcopy(before)
        for row in after: row['mission']['destination_retry'] = memory()
        after[3]['controls'] = [2]
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory)/name for name in ['before.jsonl','after.jsonl']]
            def write():
                for path, rows in zip(paths, [before,after]):
                    path.write_text(''.join(json.dumps(row)+'\n' for row in rows))
            write()
            self.assertEqual(A.prefix_parity(*paths, 4)['rows'], 3)
            with self.assertRaises(AssertionError): A.prefix_parity(*paths, None)
            after[1]['mission']['cover_response'] = {}
            write()
            with self.assertRaises(AssertionError): A.prefix_parity(*paths, 4)


if __name__ == '__main__': unittest.main()
