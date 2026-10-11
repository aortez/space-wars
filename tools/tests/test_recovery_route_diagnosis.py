import copy
import gzip
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    'route_diagnosis', Path(__file__).resolve().parents[1]/'diagnose-recovery-route.py')
R = importlib.util.module_from_spec(spec)
spec.loader.exec_module(R)


def fixture():
    return json.loads((R.ROOT/R.FIXTURE).read_text())


class RecoveryRouteDiagnosisTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with gzip.open(R.ROOT/R.BUNDLE, 'rt') as stream:
            cls.source = json.load(stream)

    def test_fixture_is_bound_to_the_retained_failed_game(self):
        f = fixture()
        R.verify_fixture(f, self.source)
        manifest = json.loads((R.ROOT/'docs/data/crossing-arrival-v1.json').read_text())
        self.assertEqual(f['source_sha256'], manifest['review_bundle']['sha256'])
        for field, value in [('seat', 0), ('trace', {}), ('archive_sha256', 'changed')]:
            with self.assertRaises(AssertionError):
                R.verify_fixture(dict(f, **{field: value}), self.source)

    def test_recorded_ledge_is_examined_but_all_heights_exceed_the_envelope(self):
        rows = [R.geometry(s) for s in fixture()['samples']]
        self.assertEqual([r['gap_rank'] for r in rows], [2, 1, 1, 1])
        for row in rows:
            self.assertTrue(row['examined_in_first_eight'])
            self.assertEqual(row['pre_clearance_rejections'], 24)
            candidates = row['endpoint_margins']
            self.assertEqual([(m['from_node'], m['to_node']) for m in candidates],
                             [(276-i, 282+i) for i in range(8)])
            for m in candidates:
                self.assertEqual([h['result'] for h in m['heights']], ['rise_over_10']*3)
            self.assertGreater(candidates[0]['heights'][0]['required_rise'], 14.07)
            self.assertLess(candidates[0]['heights'][0]['required_rise'], 14.09)

    def test_geometric_acceptance_does_not_claim_a_clear_or_executable_corridor(self):
        s = copy.deepcopy(fixture()['samples'][1])
        # Give the right-hand endpoints the same radius as the left. The
        # diagnostic can then request clearance, but cannot report a flight.
        nodes = {n['id']: n for n in s['ground']['nodes']}
        for a, b in [(276-i, 282+i) for i in range(8)]:
            left, right = nodes[a]['position'], nodes[b]['position']
            scale = R.math.hypot(*R.vector(left))/R.math.hypot(*R.vector(right))
            nodes[b]['position'] = {k: right[k]*scale for k in ('x', 'y')}
        result = R.geometry(s)
        self.assertEqual(result['pre_clearance_rejections'], 0)
        self.assertEqual({h['result'] for m in result['endpoint_margins'] for h in m['heights']},
                         {'needs_physical_clearance'})

    def test_projection_checks_full_trace_hash_and_rejects_missing_or_duplicate_samples(self):
        f = fixture()
        rows = []
        for sample in f['samples']:
            o = dict(flight=dict(pilot=sample['pilot']), ground=sample['ground'], jetpack=sample['jetpack'])
            task = dict(route=sample['observed_route'], started_tick=sample['ground_started_tick'], target=sample['target'])
            rows.append(dict(tick=sample['tick'], seat=1,
                observation=dict(local=dict(combat=dict(recovery=o))), mission=dict(recovery=dict(ground=task))))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'trace.jsonl'
            def bind(items):
                path.write_text(''.join(json.dumps(row)+'\n' for row in items))
                return dict(summary=dict(runs={R.CASE:dict(archive=dict(files={
                    'trace.jsonl':dict(sha256=R.digest(path), bytes=path.stat().st_size)}))}))
            source = bind(rows)
            self.assertEqual(R.trace_samples(path, source), f['samples'])
            path.write_text(path.read_text()+'\n')
            with self.assertRaises(AssertionError):
                R.trace_samples(path, source)
            for altered in [rows[:3], [*rows, rows[-1]]]:
                with self.assertRaises(AssertionError):
                    R.trace_samples(path, bind(altered))

    def test_native_evidence_requires_all_four_snapshots_and_a_passing_test(self):
        failed = dict(path=[], diagnostics=dict(failure='disconnected'))
        rows = [dict(tick=tick, ground_flag=failed, combined_flag=failed,
                     ground_hatches=failed, combined_hatches=failed,
                     synthetic_bridge_flag=dict(path=[276, 282], diagnostics=dict(failure=None)))
                for tick in R.TICKS]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'native.log'
            def write(items, passed=True):
                text = ''.join(R.NATIVE_PREFIX+json.dumps(row)+'\n' for row in items)
                path.write_text(text+('test result: ok. 1 passed; 0 failed; 0 ignored\n' if passed else 'test result: FAILED\n'))
            write(rows)
            self.assertEqual(R.native_results(path), rows)
            for items, passed in [(rows[:-1], True), (rows, False)]:
                write(items, passed)
                with self.assertRaises(AssertionError):
                    R.native_results(path)
            write(rows)
            path.write_text(path.read_text()+'test result: FAILED. 0 passed; 1 failed;\n')
            with self.assertRaises(AssertionError):
                R.native_results(path)
            changed = copy.deepcopy(rows)
            changed[0]['combined_flag']['path'] = [273, 282]
            write(changed)
            with self.assertRaises(AssertionError):
                R.native_results(path)


if __name__ == '__main__':
    unittest.main()
