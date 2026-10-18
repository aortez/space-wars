"""Known CPU/DRM units, independent of any simulation implementation."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "pizza_profile", Path(__file__).parents[1] / "pizza-profile-summary.py"
)
REPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPORT)


def stat(ticks):
    # Fields after comm start at Linux stat field 3; utime is field 14.
    fields = ["0"] * 22
    fields[0] = "R"
    fields[11] = str(ticks)
    return "123 (name with ) space) " + " ".join(fields)


def sample(index, uptime, total, process, main):
    lines = [f"BEGIN {index}", f"{uptime} 0"]
    # Guest counters are already included in user time: they must not be added.
    lines += [f"cpu {total} 0 0 0 0 0 0 0 999999 999999"]
    lines += [f"cpu{i} {total // 4} 0 0 0 0 0 0 0 999999 999999" for i in range(4)]
    lines += ["PROCESS " + stat(process), "THREAD " + stat(main)]
    lines += ["TEMP 75000", "FREQ 1500000", "THROTTLED 0", "END"]
    return "\n".join(lines) + "\n"


class ProfileUnits(unittest.TestCase):
    def test_one_core_scale_and_duration_weighted_usage(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "cpu.txt"
            path.write_text(
                sample(0, 0, 0, 0, 0)
                + sample(1, 1, 400, 150, 100)
                + sample(2, 4, 1600, 150, 100)
            )
            result = REPORT.cpu(path)
        self.assertEqual(result["intervals"][0]["process_pct"], 150)
        self.assertEqual(result["intervals"][0]["main_thread_pct"], 100)
        # One busy second, three idle seconds; not the mean of two intervals.
        self.assertEqual(result["process_pct"]["mean"], 37.5)
        self.assertEqual(result["main_thread_pct"]["mean"], 25)
        self.assertEqual(result["longest_main_thread_ge95_seconds"], 1)

    def test_drm_microseconds_are_differenced_in_their_own_clock(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trace.txt"
            path.write_text(
                "kms_trace_version=6\n"
                "loop,marker,flip_kernel_us,submit_at_ms\n"
                "1,1,100000000,1\n"
                "2,1,100016667,17.667\n"
                "3,1,100050000,51\n"
            )
            result = REPORT.render([path])[0]
        self.assertEqual(result["flip_interval_samples"], 2)
        self.assertEqual(result["flip_intervals_over_25ms"], 1)
        self.assertEqual(result["flip_intervals_ms"]["max"], 33.333)
        self.assertEqual(result["flip_intervals_ms"]["mean"], 25)


if __name__ == "__main__":
    unittest.main()
