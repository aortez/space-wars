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
            for width in L.WIDTHS:
                measured = L.boxes(lines, start, end, width)
                self.assertGreaterEqual(len(measured), 15, '%s measured only %d controls at %d' % (name, len(measured), width))
                self.assertIn('Slot Machine', [item[0] for item in measured])

    def test_the_pause_grid_stacks_its_selectors_on_a_portrait_panel(self):
        # Conditional geometry is measured on both sides of the threshold.
        lines = SLINT.read_text().split('\n')
        [(start, end)] = [(start, end) for name, start, end in L.panels(lines) if name == 'ingame-clock']
        for width, stacked in ((400, True), (552, False)):
            rows = {item[0]: item for item in L.boxes(lines, start, end, width)}
            preview, recipe, coins = rows['Preview'], rows['Recipe'], rows['Coins']
            self.assertEqual(preview[3], width if stacked else width - 262)
            self.assertEqual((coins[2], recipe[2]), (224, 336) if stacked else (168, 224))
            self.assertEqual(recipe[3], width if stacked else 2 * (width - 24) / 5)

    def test_every_selector_keeps_room_for_its_value(self):
        report = L.audit_values(SLINT)
        self.assertEqual(sorted(report), ['ingame-clock'])
        for panel, found in report.items():
            self.assertFalse(found, '%s squeezed: %s' % (panel, [(w, item[0], item[5], room) for w, item, room in found]))

    def test_a_squeezed_selector_is_reported(self):
        # Give the recipe two fifths of the row on a portrait panel again.
        source = SLINT.read_text()
        squeezed = source.replace('width: clock-grid.stacked ? parent.width : 2 * (parent.width - 24px) / 5;', 'width: 2 * (parent.width - 24px) / 5;', 1)
        self.assertNotEqual(source, squeezed)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'main.slint'
            path.write_text(squeezed)
            found = L.audit_values(path)['ingame-clock']
        self.assertEqual(sorted({(width, item[0]) for width, item, _ in found}), [(400, 'Recipe'), (480, 'Recipe')])

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
