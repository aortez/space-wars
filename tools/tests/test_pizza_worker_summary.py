"""Simulation-worker comparison units on synthetic Pi captures."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "pizza_worker", Path(__file__).parents[1] / "pizza-worker-summary.py"
)
SUMMARY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SUMMARY)


def capture(directory, samples):
    """Status ticks and CPU-sample uptimes, keyed by sample index."""
    blocks = []
    for index, (uptime, tick) in enumerate(samples):
        blocks.append(f"BEGIN {index}\n{uptime} 0\nEND\n")
        status = f"pizza={json.dumps(dict(tick=tick))}\nhost_updates_per_callback={1 + index}.5\n"
        (directory / f"status-{index}.txt").write_text(status)
    (directory / "cpu.txt").write_text("".join(blocks))


class WorkerSummaryUnits(unittest.TestCase):
    def test_game_speed_interpolates_wall_time_between_samples(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            capture(path, [(0.0, 0), (10.0, 600), (20.0, 1100), (30.0, 1700)])
            speed = SUMMARY.game_speed(path, 120, 1020)
            ticks = SUMMARY.status_series(path, "pizza")
            per_frame = SUMMARY.status_series(path, "host_updates_per_callback")
            with self.assertRaises(ValueError):
                SUMMARY.game_speed(path, 120, 1800)

        # Tick 120 falls at 2.0 s and tick 1020 at 18.4 s: 15 s of play in 16.4 s.
        self.assertAlmostEqual(speed, 15 / 16.4)
        self.assertEqual(ticks, {0: 0, 1: 600, 2: 1100, 3: 1700})
        self.assertEqual(max(per_frame.values()), 4.5)

    def test_the_client_must_report_the_requested_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            (path / "simulation.txt").write_text("worker\n")
            (path / "status-0.txt").write_text("pizza_simulation_worker=1\n")
            self.assertEqual(SUMMARY.simulation_mode(path), "worker")
            # A client without the worker ignores the switch and reports nothing.
            (path / "status-0.txt").write_text("fps=60.0\n")
            with self.assertRaises(AssertionError):
                SUMMARY.simulation_mode(path)
            (path / "simulation.txt").write_text("direct\n")
            self.assertEqual(SUMMARY.simulation_mode(path), "direct")

    def test_a_pair_must_replay_the_same_updates(self):
        same = {"direct": ["1", "2"], "worker": ["1", "2"]}
        self.assertEqual(SUMMARY.compare_pair("on-r1", same),
                         {"updates": 2, "fingerprints_identical": True})
        with self.assertRaises(AssertionError):
            SUMMARY.compare_pair("on-r1", {"direct": ["1", "2"], "worker": ["1", "3"]})


if __name__ == "__main__":
    unittest.main()
