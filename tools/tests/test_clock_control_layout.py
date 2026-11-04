import importlib.util
from pathlib import Path
import tempfile
import unittest


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


L = module('clock_control_layout', Path(__file__).parents[1] / 'audit-clock-control-layout.py')
SLINT = Path(__file__).parents[2] / 'crates/engine-client/ui/main.slint'


class ClockControlLayoutTests(unittest.TestCase):
    def test_every_clock_control_keeps_its_own_place_at_each_width(self):
        report = L.audit(SLINT)
        self.assertEqual(sorted(report), ['ingame-clock', 'launcher-clock'])
        for panel, found in report.items():
            self.assertFalse(found, '%s overlaps: %s' % (panel, [(w, a[0], a[5], b[0], b[5]) for w, a, b in found]))

    def test_both_panels_measure_their_whole_grid(self):
        lines = SLINT.read_text().split('\n')
        for name, start, end in L.panels(lines):
            measured = L.boxes(lines, start, end, 800)
            self.assertGreaterEqual(len(measured), 15, '%s measured only %d controls' % (name, len(measured)))
            self.assertIn('Slot Machine', [item[0] for item in measured])

    def test_an_overlapping_button_is_reported(self):
        # A guard that cannot fail is no guard: move one button onto another.
        source = SLINT.read_text()
        moved = source.replace('''                    x: 4 * (parent.width + 10px) / 5;
                    y: 56px;''', '''                    x: 0px;
                    y: 112px;''', 1)
        self.assertNotEqual(source, moved)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'main.slint'
            path.write_text(moved)
            found = L.audit(path)['ingame-clock']
        self.assertTrue(found)
        self.assertIn('Slot Machine', [a[0] for _, a, _ in found])


if __name__ == '__main__':
    unittest.main()
