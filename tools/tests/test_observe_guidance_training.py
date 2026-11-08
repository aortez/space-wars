import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    'observe_guidance_training', Path(__file__).parents[1] / 'observe-guidance-training.py')
T = importlib.util.module_from_spec(spec)
spec.loader.exec_module(T)


def score(precision, additional=0, brake=100):
    result = dict.fromkeys(T.GRADED_ORDER, 0)
    result.update(precision_completed=precision, additional_completed=additional,
                  brake_progress_points=brake)
    return result


def generation(index, winner_id, winner, other):
    return dict(generation=index, best_id=winner_id, best_fitness=winner,
                population_ids=[winner_id, 'z'], fitness=[winner, other], best_genome={})


def fixture(history=None, pending=None, budget=3):
    history = history or []
    complete = len(history) == budget
    scores = history[-1]['fitness'] if complete else (pending or [None, None])
    return dict(version=1, sha256='declared-by-rust', run=dict(
        runtime='test-runtime',
        config=dict(population=2, generations=budget, seed=1),
        task=dict(id='test-graded', contract={}, configuration=dict(
            plan_utf8=json.dumps(dict(fitness_order=T.GRADED_ORDER)))),
        generation=len(history) - int(complete), population=[{}, {}],
        fitness=scores, history=history))


class GuidanceObserverTests(unittest.TestCase):
    def test_focus_orders_reverse_progress_but_protect_current_successes(self):
        broad = dict.fromkeys(T.FOCUS_BROAD_ORDER, 0)
        broad.update(protected_completed=616, precision_position_points=10)
        focused = dict(broad, precision_position_points=0, remaining_speed_points=10)
        for order, winner, other in ((T.FOCUS_BROAD_ORDER, broad, focused),
                                      (T.FOCUS_TARGETED_ORDER, focused, broad)):
            document = fixture([generation(0, 'a', winner, other)])
            document['run']['task']['configuration']['fitness_order'] = order
            self.assertEqual(T.summarize(document)['fitness_order'], list(order))
            document['run']['history'][0].update(best_id='z', best_fitness=other)
            with self.assertRaises(ValueError):
                T.summarize(document)
            regression = dict(winner, protected_completed=615, remaining_completed=5)
            self.assertGreater(T.score_key(winner, order), T.score_key(regression, order))

    def test_joint_order_values_total_retention_before_precision(self):
        balanced = dict.fromkeys(T.JOINT_ORDER, 0)
        balanced.update(focal_completed=2, retained_completed=600,
                        precision_completed=149, retained_match_completed=451)
        precision = dict(balanced, retained_completed=595,
                         precision_completed=156, retained_match_completed=439)
        document = fixture([generation(0, 'a', balanced, precision)])
        document['run']['task']['configuration']['fitness_order'] = T.JOINT_ORDER
        self.assertEqual(T.summarize(document)['fitness_order'], list(T.JOINT_ORDER))
        document['run']['history'][0].update(best_id='z', best_fitness=precision)
        with self.assertRaises(ValueError):
            T.summarize(document)

    def test_repair_order_protects_arrival_before_precision(self):
        focal = dict.fromkeys(T.REPAIR_ORDER, 0)
        focal['approach_completed'] = 1
        precision = dict(focal, approach_completed=0, retained_completed=5)
        document = fixture([generation(0, 'a', focal, precision)])
        document['run']['task']['configuration']['fitness_order'] = T.REPAIR_ORDER
        self.assertEqual(T.summarize(document)['fitness_order'], list(T.REPAIR_ORDER))
        document['run']['task']['configuration']['fitness_order'] = T.MATCH_ORDER
        with self.assertRaises(ValueError):
            T.summarize(document)

    def test_explicit_progress_order_validates_the_actual_winner(self):
        physical = dict(score(157, 30, 500), approach_speed_points=101)
        proxy = dict(score(157, 30, 600), approach_speed_points=100)
        document = fixture([generation(0, 'a', physical, proxy)])
        document['run']['task']['configuration']['fitness_order'] = T.PROGRESS_FIRST_ORDER
        self.assertEqual(T.summarize(document)['fitness_order'], list(T.PROGRESS_FIRST_ORDER))
        document['run']['task']['configuration']['fitness_order'] = T.GRADED_ORDER
        with self.assertRaises(ValueError):
            T.summarize(document)

    def test_pending_candidates_count_without_inventing_a_generation(self):
        document = fixture(pending=[score(100), None])
        result = T.summarize(document)
        self.assertFalse(result['complete'])
        self.assertEqual(result['pending_generation'], 1)
        self.assertEqual(result['pending_candidates_evaluated'], 1)
        self.assertEqual(result['saved_candidate_evaluations'], 1)
        self.assertEqual(result['history'], [])
        self.assertIsNone(result['best_so_far'])
        self.assertIsNone(result['generations_since_improvement'])

    def test_final_population_is_not_double_counted(self):
        history = [generation(0, 'b', score(157), score(150)),
                   generation(1, 'a', score(157, 1), score(157))]
        result = T.summarize(fixture(history, budget=2))
        self.assertTrue(result['complete'])
        self.assertIsNone(result['pending_generation'])
        self.assertEqual(result['pending_candidates_evaluated'], 0)
        self.assertEqual(result['saved_candidate_evaluations'], 4)
        self.assertEqual([row['generation'] for row in result['history']], [1, 2])

    def test_ordered_improvement_is_not_component_maximum_or_genome_change(self):
        history = [generation(0, 'c', score(157, 0, 500), score(156, 8, 900)),
                   generation(1, 'b', score(157, 1, 100), score(157, 0, 600)),
                   generation(2, 'a', score(157, 1, 100), score(157, 0, 700))]
        result = T.summarize(fixture(history, budget=3))
        self.assertEqual(result['last_improvement_generation'], 2)
        self.assertEqual(result['generations_since_improvement'], 1)
        self.assertEqual(result['best_so_far']['selected_genome'], 'a')
        second = result['history'][1]
        self.assertEqual(second['selected_fitness']['brake_progress_points'], 100)
        self.assertEqual(second['population']['brake_progress_points'],
                         dict(q25=225, median=350, q75=475))

    def test_lower_priority_terms_still_count_as_ordered_improvement(self):
        first = score(157)
        second = dict(first, spare_ticks=1)
        result = T.summarize(fixture([
            generation(0, 'b', first, score(150)),
            generation(1, 'a', second, first)]))
        self.assertEqual(result['last_improvement_generation'], 2)

    def test_score_key_order_is_not_json_key_order(self):
        document = fixture([generation(0, 'a', score(157, 0), score(156, 8))])
        entry = document['run']['history'][0]
        for index, value in enumerate(entry['fitness']):
            entry['fitness'][index] = dict(reversed(list(value.items())))
        entry['best_fitness'] = dict(reversed(list(entry['best_fitness'].items())))
        self.assertEqual(T.summarize(document)['fitness_order'], list(T.GRADED_ORDER))

    def test_invalid_selection_and_partial_boundaries_are_rejected(self):
        valid = fixture([generation(0, 'a', score(157), score(156, 8))])
        for failure in ['winner', 'tie_id', 'counter', 'all_pending_done', 'history_gap',
                        'missing_score', 'unknown_score', 'non_integer', 'declared_order']:
            document = copy.deepcopy(valid)
            run = document['run']
            entry = run['history'][0]
            if failure == 'winner':
                entry.update(best_id='z', best_fitness=entry['fitness'][1])
            elif failure == 'tie_id':
                entry['fitness'][1] = entry['fitness'][0]
                entry['population_ids'][1] = '0'
            elif failure == 'counter':
                run['generation'] += 1
            elif failure == 'all_pending_done':
                run['fitness'] = [score(157), score(157)]
            elif failure == 'history_gap':
                entry['generation'] = 1
            elif failure == 'missing_score':
                del entry['fitness'][1]['spare_ticks']
            elif failure == 'unknown_score':
                entry['best_fitness']['new_metric'] = 1
            elif failure == 'non_integer':
                entry['fitness'][1]['spare_ticks'] = float('nan')
            else:
                run['task']['configuration']['plan_utf8'] = json.dumps(
                    dict(fitness_order=list(reversed(T.GRADED_ORDER))))
            with self.subTest(failure=failure), self.assertRaises(ValueError):
                T.summarize(document)

    def test_no_history_or_score_can_be_observed_before_recipe_order_is_known(self):
        document = fixture()
        document['run']['task']['configuration'] = {}
        result = T.summarize(document)
        self.assertEqual(result['fitness_order'], [])
        self.assertIsNone(result['validation'])

    def test_monitor_exports_without_writing_the_input(self):
        document = fixture([generation(0, 'a', score(157), score(156))], budget=1)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'run' / 'checkpoint.json'
            source.parent.mkdir()
            payload = json.dumps(document).encode()
            source.write_bytes(payload)
            modified = source.stat().st_mtime_ns
            with patch.object(T, 'plot', return_value=b'png-placeholder'):
                T.observe(source, root / 'observation')
            result = json.loads((root / 'observation' / 'summary.json').read_text())
            self.assertEqual(result['source_sha256'], hashlib.sha256(payload).hexdigest())
            self.assertFalse(result['declared_run_sha256_verified'])
            self.assertEqual(source.read_bytes(), payload)
            self.assertEqual(source.stat().st_mtime_ns, modified)
            self.assertEqual(list(source.parent.iterdir()), [source])
            self.assertIn('Validation is not present',
                          (root / 'observation' / 'index.html').read_text())
            with self.assertRaises(ValueError):
                T.observe(source, source.parent / 'observation')

    def test_watch_updates_partial_progress_and_stops_on_complete(self):
        initial = fixture(pending=[score(157), None], budget=1)
        final = fixture([generation(0, 'a', score(157), score(156))], budget=1)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'run' / 'checkpoint.json'
            source.parent.mkdir()
            source.write_text(json.dumps(initial))
            def advance(_):
                source.write_text(json.dumps(final))
            with patch.object(T, 'plot', return_value=b'png'), patch.object(T.time, 'sleep', side_effect=advance) as sleep:
                T.observe(source, root / 'view', watch=1)
            self.assertEqual(sleep.call_count, 1)
            result = json.loads((root / 'view' / 'summary.json').read_text())
            self.assertTrue(result['complete'])
            self.assertEqual(result['saved_candidate_evaluations'], 2)
            self.assertNotIn('http-equiv="refresh"', (root / 'view' / 'index.html').read_text())

    def test_watch_marks_read_errors_stale_and_recovers(self):
        final = fixture([generation(0, 'a', score(157), score(156))], budget=1)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'run' / 'checkpoint.json'
            source.parent.mkdir()
            source.write_text('{')
            output = root / 'view'
            def repair(_):
                self.assertIn('previous exports are stale', (output / 'index.html').read_text())
                source.write_text(json.dumps(final))
            with patch.object(T, 'plot', return_value=b'png'), patch.object(T.time, 'sleep', side_effect=repair):
                T.observe(source, output, watch=1)
            self.assertNotIn('Checkpoint unavailable', (output / 'index.html').read_text())
            self.assertTrue(json.loads((output / 'summary.json').read_text())['complete'])

    def test_bad_checkpoint_does_not_leave_a_current_looking_graph(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'run' / 'checkpoint.json'
            source.parent.mkdir()
            source.write_text('{')
            output = root / 'view'
            with self.assertRaises(ValueError):
                T.observe(source, output)
            page = (output / 'index.html').read_text()
            self.assertIn('Checkpoint unavailable', page)
            self.assertIn('previous exports are stale', page)
            self.assertNotIn('<img', page)


if __name__ == '__main__':
    unittest.main()
