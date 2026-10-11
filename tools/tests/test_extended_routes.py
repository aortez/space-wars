import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('extended', Path(__file__).parents[1]/'validate-extended-routes.py')
M = importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)


class ExtendedRoutesTests(unittest.TestCase):
    def test_frozen_extension_plan_keeps_every_case_and_changes_only_the_extension(self):
        items = M.P.plan()
        self.assertEqual(len(items), 14)
        self.assertEqual(sum(i['kind'] == 'directed' for i in items), 6)
        self.assertEqual(sum(i['kind'] == 'armed' for i in items), 8)
        for item in items:
            self.assertEqual(M.arguments(item, False), M.R.arguments(item, True))
            self.assertEqual(M.arguments(item, True), M.arguments(item, False) + ['--extended-objective-routes', 'true'])
            self.assertEqual(item['delivery'], 'shared')


if __name__ == '__main__':
    unittest.main()
