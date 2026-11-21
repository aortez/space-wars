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
        self.assertEqual(sorted(report), ['launcher-clock'])
        for panel, found in report.items():
            self.assertFalse(found, '%s overlaps: %s' % (panel, [(w, a[0], a[5], b[0], b[5]) for w, a, b in found]))

    def test_the_launcher_panel_measures_its_whole_grid(self):
        lines = SLINT.read_text().split('\n')
        for name, start, end in L.panels(lines):
            for width in L.WIDTHS:
                measured = L.boxes(lines, start, end, width)
                self.assertGreaterEqual(len(measured), 15, '%s measured only %d controls at %d' % (name, len(measured), width))
                self.assertIn('Slot Machine', [item[0] for item in measured])

    def test_the_launcher_gives_time_date_lamps_theme_and_fonts_a_fifth_each(self):
        # The five share their row as equal buttons, the width of the event
        # switches' columns below them.
        lines = SLINT.read_text().split('\n')
        [(start, end)] = [(start, end) for name, start, end in L.panels(lines) if name == 'launcher-clock']
        for width in L.WIDTHS:
            row = sorted((item for item in L.boxes(lines, start, end, width) if item[2] == 52), key=lambda item: item[1])
            self.assertEqual([item[0] for item in row], ['Time', 'Date', 'Lamps', 'Theme', 'Fonts'], width)
            self.assertEqual({item[3] for item in row}, {(width - 32) / 5}, width)
            self.assertEqual({item[6] for item in row}, {'MenuButton'}, width)

    def test_an_overlapping_button_is_reported(self):
        # A guard that cannot fail is no guard: move one button onto another.
        source = SLINT.read_text()
        moved = source.replace('''                            x: 4 * (parent.width + 8px) / 5;
                            y: 156px;''', '''                            x: 0px;
                            y: 208px;''', 1)
        self.assertNotEqual(source, moved)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'main.slint'
            path.write_text(moved)
            found = L.audit(path)['launcher-clock']
        self.assertTrue(found)
        self.assertIn('Slot Machine', [a[0] for _, a, _ in found])


if __name__ == '__main__':
    unittest.main()
