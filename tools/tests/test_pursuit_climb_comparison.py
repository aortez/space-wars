import copy
from collections import Counter
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch


TOOLS = Path(__file__).parents[1]
def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


B = module('climb_comparison', TOOLS / 'compare-pursuit-climb-laser.py')
Q = module('qualified_laser_tests', TOOLS / 'tests/test_pursuit_climb_laser.py')


def player():
    return dict(outcome='draw', completed_departures=1, claims=1, completed_recoveries=0,
                ships_lost=1, pilot_deaths=0, abandoned_visits=0, unfinished_visits=0,
                owned_planet_ticks=600, visits=[], progress=dict(eligible_ticks=300,
                    distance_observed_ticks=280, ticks_after_20s_without_progress=10,
                    longest_no_progress_ticks=20))


def complete_results():
    cases = B.cases()
    runs = {c['name']: dict(item=c, players=[player(), player()]) for c in cases}
    pairs = []
    for a, b in zip(cases[::2], cases[1::2]):
        option = {c['laser']: c['name'] for c in (a, b)}
        pairs.append(dict(group=a['group'], configuration=a['arm'], seat=a['seat'],
            opponent=a['opponent'], interval=a['interval'], seed=a['seed'], world_cluster=a['world_cluster'],
            off=option[False], on=option[True], benefits=['earlier_or_additional_departure'],
            first_difference=dict(tick=100), outcome_transition='draw->draw'))
    return runs, pairs


class PursuitClimbComparisonTests(unittest.TestCase):
    def test_frozen_plan_rejects_shadowed_flags_changed_matrix_and_unqualified_binary(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'binary';binary.write_bytes(b'frozen')
            qualification = root / 'qualification.json'
            qualification.write_text(json.dumps(dict(binary=dict(sha256=B.P.digest(binary)), runtime_source_commit='runtime')))
            plan = dict(profile=B.L.PROFILE, namespace=B.NAMESPACE, runtime_source_commit='runtime',
                        binary=dict(path=str(binary), sha256=B.P.digest(binary)), inputs={},
                        qualification=dict(sha256=B.P.digest(qualification)), jobs=B.jobs(binary, root))
            with patch.object(B, 'QUALIFICATION', qualification), patch.object(B, 'tool_inputs', return_value={}):
                B.verify_plan(plan, root)
                for mutate in (lambda p: p['jobs'][0]['command'].extend(['--seconds', '1']),
                               lambda p: p['jobs'].pop(),
                               lambda p: p['jobs'][0]['item'].update(seed=1),
                               lambda p: p['binary'].update(sha256='unqualified'),
                               lambda p: p.update(runtime_source_commit='changed')):
                    changed = copy.deepcopy(plan);mutate(changed)
                    with self.assertRaises(AssertionError): B.verify_plan(changed, root)

    def test_balanced_fresh_matrix_and_isolated_on_off_flags(self):
        cases = B.cases()
        self.assertEqual(len(cases), 192)
        self.assertEqual(len({c['name'] for c in cases}), 192)
        self.assertEqual(len({c['seed'] for c in cases}), 4)
        self.assertFalse({c['seed'] for c in cases} & {c['seed'] for c in B.P.plan()})
        counts = Counter((c['arm'], c['laser'], c['seat'], c['opponent'], c['interval']) for c in cases)
        self.assertEqual(set(counts.values()), {4})
        for a, b in zip(cases[::2], cases[1::2]):
            self.assertEqual({a['laser'], b['laser']}, {False, True})
            self.assertEqual({k: v for k, v in a.items() if k not in ('name', 'laser')},
                             {k: v for k, v in b.items() if k not in ('name', 'laser')})
            fa, fb = B.flags(a), B.flags(b)
            self.assertEqual({k for k in fa if fa[k] != fb[k]}, {B.L.FLAG})
            for c, f in ((a, fa), (b, fb)):
                self.assertEqual(f[B.L.FLAG], str(c['seat']) if c['laser'] else 'none')
                self.assertEqual(f['--mode'], 'duel')
                self.assertEqual((f['--seconds'], f['--require-finish']), ('600', 'true'))
                self.assertEqual(f['--trace-end-tick'], str(B.D.END))
                self.assertNotIn('--trace-impact', f)

    def test_duplicate_or_missing_cases_cannot_be_scored(self):
        runs, pairs = complete_results()
        with self.assertRaises(AssertionError):
            B.screen(runs, pairs[:-1])
        pairs[-1] = copy.deepcopy(pairs[0])
        with self.assertRaises(AssertionError):
            B.screen(runs, pairs)

    def test_configuration_regression_is_not_hidden_by_other_configuration(self):
        runs, pairs = complete_results()
        self.assertEqual(B.screen(runs, pairs)['decision'], 'advance_to_further_evaluation')
        worse = next(p for p in pairs if p['configuration'] == 'candidate')
        better = next(p for p in pairs if p['configuration'] == 'control')
        runs[worse['on']]['players'][worse['seat']]['outcome'] = 'loss'
        runs[better['on']]['players'][better['seat']]['outcome'] = 'win'
        screen = B.screen(runs, pairs)
        self.assertEqual(screen['configurations']['candidate']['decision'], 'retain')
        self.assertEqual(screen['configurations']['control']['decision'], 'advance_to_further_evaluation')
        self.assertEqual(screen['decision'], 'retain')
        self.assertFalse(screen['default_promotion'])
        runs, pairs = complete_results()
        worse = next(p for p in pairs if p['configuration'] == 'candidate' and p['opponent'] == 9)
        better = next(p for p in pairs if p['configuration'] == 'candidate' and p['opponent'] == 10)
        runs[worse['on']]['players'][worse['seat']]['outcome'] = 'loss'
        runs[better['on']]['players'][better['seat']]['outcome'] = 'win'
        screen = B.screen(runs, pairs)['configurations']['candidate']
        self.assertEqual(screen['totals']['off']['points'], screen['totals']['on']['points'])
        self.assertIn('match_points_regressed:opponent:9', screen['reasons'])

    def test_each_safety_gate_can_refuse_advancement(self):
        for field, delta, reason in [('completed_departures', -1, 'fewer_completed_departures'),
            ('ships_lost', 1, 'ships_lost_increased'), ('pilot_deaths', 1, 'pilot_deaths_increased'),
            ('longest_no_progress_ticks', 1, 'worst_no_progress_ticks_increased'),
            ('ticks_after_20s_without_progress', 1, 'no_progress_fraction_increased')]:
            runs, pairs = complete_results()
            pair = pairs[0]
            subject = runs[pair['on']]['players'][pair['seat']]
            target = subject['progress'] if field in subject['progress'] else subject
            target[field] += delta
            self.assertIn(reason, B.screen(runs, pairs)['configurations'][pair['configuration']]['reasons'])

    def test_unchanged_and_unknown_progress_do_not_pass(self):
        runs, pairs = complete_results()
        for p in pairs:
            p['benefits'] = []
            p['first_difference']['tick'] = None
            for arm in ('off', 'on'):
                runs[p[arm]]['players'][p['seat']]['progress']['eligible_ticks'] = 0
        for config in B.screen(runs, pairs)['configurations'].values():
            self.assertIn('no_useful_fresh_change', config['reasons'])
            self.assertIn('unknown_no_progress_fraction', config['reasons'])

    def test_watch_climb_retains_missing_target_refusal_and_exact_actions(self):
        row, trace, previous = Q.fixture()
        trace['mission']['goal'] = 'watch'
        trace['observation']['local']['combat']['target'] = None
        gate = trace['mission'][B.L.FIELD]
        gate['requested_ticks'] = 0
        gate['last'].update(decision='unavailable', distance=None, heading_error=None)
        row['actions'] = trace['actions'] = Q.actions()
        B.audit_watch_check(row, trace, previous)
        row['actions'] = trace['actions'] = Q.actions(1)
        with self.assertRaises(AssertionError):
            B.audit_watch_check(row, trace, previous)

    def test_report_normalization_preserves_counts_clocks_and_physical_state(self):
        before = dict(tick=100, health=80., sensor=dict(count=10, mean_ms=1.))
        after = dict(before, pursuit_climb_laser={'checks': 1}, pursuit_climb_laser_model=B.L.PROFILE)
        self.assertEqual(B.normalized_report(before), B.normalized_report(after))
        for changed in (dict(after, tick=101), dict(after, health=79.), dict(after, sensor=dict(count=11, mean_ms=1.))):
            self.assertNotEqual(B.normalized_report(before), B.normalized_report(changed))

    def test_generated_raw_files_round_trip_before_removal_and_tampering_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            root = base / 'run'; root.mkdir()
            files = {'trace.jsonl': b'{"tick":0}\n' * 10, 'empty.jsonl': b'', 'report.json': b'{}\n'}
            for name, content in files.items(): (root / name).write_bytes(content)
            record = B.pack(root, base / 'run.tar.gz')
            self.assertFalse(root.exists())
            B.verify_archive(record)
            B.unpack(record, root)
            self.assertEqual({p.name: p.read_bytes() for p in root.iterdir()}, files)
            self.assertEqual(B.pack(root, base / 'run.tar.gz'), record)
            changed = copy.deepcopy(record);changed['files']['report.json']['sha256'] = 'bad'
            with self.assertRaises(AssertionError): B.verify_archive(changed)
            (base / 'run.tar.gz').write_bytes(b'changed')
            with self.assertRaises(AssertionError): B.verify_archive(record)

    def test_archive_cannot_extract_links_parent_paths_or_duplicate_names(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'bad.tar.gz'
            for kind in ('link', 'parent', 'duplicate'):
                with tarfile.open(path, 'w:gz') as archive:
                    for _ in range(2 if kind == 'duplicate' else 1):
                        info = tarfile.TarInfo('../outside' if kind == 'parent' else 'trace.jsonl')
                        if kind == 'link':info.type = tarfile.SYMTYPE;info.linkname = '/outside'
                        archive.addfile(info, io.BytesIO(b''))
                with self.assertRaises(AssertionError):B.archive_files(path)


if __name__ == '__main__':
    unittest.main()
