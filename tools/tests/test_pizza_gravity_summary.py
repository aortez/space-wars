"""Gravity-law comparison units on synthetic capacity-lab reports."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "pizza_gravity", Path(__file__).parents[1] / "pizza-gravity-summary.py"
)
SUMMARY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SUMMARY)


def sample(tick, exploded, balls=10, casts=None):
    births = dict(exploded=exploded, fragments_created=2 * exploded,
                  fragment_parents_skipped=0, exploded_by_generation=[exploded, 0, 0, 0])
    return dict(
        tick=tick, fingerprint=1000 + tick, contacts=4, candidate_pairs=9,
        escaped_walls=0, step_us=1500.0, gravity_us=300.0, physics_total_us=900.0,
        narrow_phase_us=200.0, solver_us=150.0, ccd_us=None if casts is None else 50.0,
        population=dict(balls=balls, limit=10, births=births),
    )


def cpu_sample(index):
    """One fully busy second for the client's main thread on four cores."""
    # Fields after comm start at Linux stat field 3; utime is field 14.
    stat = "123 (client) " + " ".join(["R"] + ["0"] * 10 + [str(100 * index)] + ["0"] * 10)
    lines = [f"BEGIN {index}", f"{index} 0", f"cpu {400 * index} 0 0 0 0 0 0 0"]
    lines += [f"cpu{core} {100 * index} 0 0 0 0 0 0 0" for core in range(4)]
    lines += [f"PROCESS {stat}", f"THREAD {stat}", "TEMP 70000", "FREQ 1500000", "THROTTLED 0",
              "END"]
    return "\n".join(lines) + "\n"


def report(samples, casts=None):
    result = dict(seed=7, field=True, initial=dict(balls=0), final=samples[-1]["population"],
                  samples=samples)
    if casts is not None:
        result["ccd_profiles"] = [
            dict(pairs={"Ball/Ball": dict(queries=count), "Ball/Cuboid": dict(queries=1)},
                 prefilter=dict(toi_calls=10 * count, thickness_rejected=count,
                                resweep_candidates=2 * count))
            for count in casts
        ]
    return result


class GravitySummaryUnits(unittest.TestCase):
    def extracted(self, data):
        with tempfile.TemporaryDirectory() as directory:
            reports = Path(directory) / "reports"
            out = Path(directory) / "out"
            reports.mkdir()
            (reports / "run.json").write_text(json.dumps(data))
            SUMMARY.extract(reports, out)
            meta = json.loads((out / "run.meta.json").read_text())
            return SUMMARY.read_rows(out / "run.csv"), meta

    def test_explosion_updates_and_limit_follow_the_lab(self):
        rows, meta = self.extracted(report([
            sample(1, 0), sample(2, 3, balls=9), sample(3, 3), sample(4, 5),
        ]))
        stats = SUMMARY.run_stats(rows)

        self.assertEqual(meta["seed"], 7)
        self.assertEqual(stats["explosion_updates"], 2)
        self.assertEqual(stats["updates_at_limit"], 3)
        self.assertEqual(stats["exploded"], 5)
        self.assertNotIn("max_shape_casts", stats)
        self.assertEqual(rows[0]["fingerprint"], "1001")

    def test_ccd_work_counts_every_pair_type_and_reports_the_first_worst_tick(self):
        casts = [3, 40, 40, 2]
        samples = [sample(tick, 0, casts=count) for tick, count in enumerate(casts, start=1)]
        rows, _ = self.extracted(report(samples, casts=casts))
        stats = SUMMARY.run_stats(rows)

        self.assertEqual(stats["max_shape_casts"], 41)
        self.assertEqual(stats["max_shape_casts_tick"], 2)
        self.assertEqual(stats["total_shape_casts"], 89)
        self.assertEqual(stats["total_toi_calls"], 850)
        self.assertEqual(stats["max_resweep_candidates"], 80)
        self.assertEqual(stats["updates_over_1000_shape_casts"], 0)

    def test_seed_ratios_use_geometric_mean_and_count_both_directions(self):
        def stats(exploded):
            return dict(exploded=exploded, exploded_by_generation=[exploded, 0, 0, 0],
                        max_shape_casts_tick=1)

        per_seed = [
            dict(baseline=stats(100), candidate=stats(200)),
            dict(baseline=stats(100), candidate=stats(50)),
            dict(baseline=stats(100), candidate=stats(100)),
        ]
        result = SUMMARY.compare_seeds(per_seed)

        self.assertAlmostEqual(result["exploded_ratio"]["geometric_mean"], 1.0)
        self.assertEqual(result["exploded_ratio"]["seeds_higher"], 1)
        self.assertEqual(result["exploded_ratio"]["seeds_lower"], 1)
        self.assertEqual(result["candidate"]["exploded"]["max"], 200)
        self.assertEqual(result["baseline"]["exploded_by_generation"], [300, 0, 0, 0])
        self.assertNotIn("max_shape_casts_tick", result["baseline"])

    def test_seed_timing_averages_each_seed_once(self):
        def rows(*steps):
            return [dict(step_us=step, gravity_us=100.0) for step in steps]

        per_seed = [
            dict(baseline=rows(1000.0, 3000.0), candidate=rows(4000.0, 4000.0)),
            dict(baseline=rows(2000.0, 2000.0), candidate=rows(1000.0, 1000.0)),
        ]
        result = SUMMARY.seed_timing(per_seed)

        self.assertAlmostEqual(result["baseline"]["step_mean_ms"], 2.0)
        self.assertAlmostEqual(result["candidate"]["step_mean_ms"], 2.5)
        self.assertAlmostEqual(result["step_mean_ratio"]["geometric_mean"], 1.0)
        self.assertAlmostEqual(result["baseline"]["step_max_ms"]["max"], 3.0)

    def test_replay_divergence_is_rejected(self):
        reference = [dict(fingerprint="1"), dict(fingerprint="2")]
        SUMMARY.check_replay(reference, [list(reference)], "same")
        with self.assertRaises(AssertionError):
            SUMMARY.check_replay(reference, [[dict(fingerprint="1"), dict(fingerprint="3")]],
                                 "diverged")

    def test_device_flips_pool_pre_export_windows_and_must_replay_the_desktop(self):
        ticks = range(1, 5)
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory) / "point-on-r1"
            run.mkdir()
            samples = [sample(tick, 0, casts=1) for tick in ticks]
            (run / "simulation.json").write_text(
                json.dumps(dict(seed=7, ordinary=True, samples=samples))
            )
            (run / "cpu.txt").write_text("".join(cpu_sample(index) for index in range(4)))
            for index, tick in enumerate(ticks):
                (run / f"status-{index}.txt").write_text(f"pizza={json.dumps(dict(tick=tick))}\n")
            # Windows ending at the final update may include export, so 3 and final drop out.
            for name, flips in [("1", [0, 16667, 33334]), ("2", [0, 50000]),
                                ("3", [0, 100000]), ("final", [0, 200000])]:
                table = "".join(f"{loop},1,{flip}\n" for loop, flip in enumerate(flips, start=1))
                (run / f"trace-{name}.txt").write_text("loop,marker,flip_kernel_us\n" + table)
            desktop = [dict(fingerprint=str(1000 + tick)) for tick in ticks]
            result = SUMMARY.pi_run(run, desktop)
            with self.assertRaises(AssertionError):
                SUMMARY.pi_run(run, desktop[:-1] + [dict(fingerprint="0")])

        self.assertEqual(result["kms_windows"], 2)
        self.assertEqual(result["flip_intervals"], 3)
        self.assertEqual(result["flip_ms"]["max"], 50)
        self.assertEqual(result["flip_intervals_over_25ms"], 1)
        self.assertEqual(result["cpu_intervals"], 2)
        self.assertEqual(result["ui_core_mean_pct"], 100)
        self.assertEqual(result["longest_ui_core_ge95_s"], 2)
        self.assertEqual(result["phase_mean_ms"]["gravity"], 0.3)

    def test_numbers_keep_their_type(self):
        self.assertEqual(SUMMARY.number("12"), 12)
        self.assertEqual(SUMMARY.number("1e-05"), 1e-05)
        self.assertEqual(SUMMARY.number("2.5"), 2.5)


if __name__ == "__main__":
    unittest.main()
