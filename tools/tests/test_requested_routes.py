import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('requested', Path(__file__).parents[1]/'validate-requested-routes.py')
M = importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)


class RequestedRoutesTests(unittest.TestCase):
    def test_trial_plan_preserves_all_focused_cases_and_changes_only_one_option(self):
        items = M.P.plan()
        self.assertEqual(len(items), 14)
        self.assertEqual(sum(i['kind'] == 'directed' for i in items), 6)
        self.assertEqual(sum(i['kind'] == 'armed' for i in items), 8)
        for item in items:
            self.assertEqual(M.arguments(item, False), M.P.arguments(item, True))
            self.assertEqual(M.arguments(item, True), M.arguments(item, False) + ['--requested-objective-routes', 'true'])

    def test_cover_witnesses_distinguish_waiting_selection_and_deadline(self):
        row = dict(capture=dict(cover_response=dict(required_since=5, measured_sites=0,
            selected_routes=0, deadlines=0, search=dict(started_tick=6, pending=[dict(bearing=0)], outcome=None))))
        waiting = M.cover_key(row)
        selected = copy.deepcopy(row)
        selected['capture']['cover_response']['selected_routes'] = 1
        selected['capture']['cover_response']['search']['outcome'] = 'selected_covered_route'
        self.assertNotEqual(waiting, M.cover_key(selected))
        expired = copy.deepcopy(row)
        expired['capture']['cover_response']['deadlines'] = 1
        self.assertNotEqual(waiting, M.cover_key(expired))
        self.assertIsNone(M.cover_key(dict(capture=None)))


if __name__ == '__main__':
    unittest.main()
