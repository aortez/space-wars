from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest

from test_report import seconds, summarize


class ReportTests(unittest.TestCase):
    def test_wall_time_is_separate_from_overlapping_test_durations(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "build.seconds").write_text("3.50\n")
            (root / "workspace.seconds").write_text("Command exited with non-zero status 100\n5.25\n")
            report = root / "junit.xml"
            report.write_text('''<testsuites time="5.0"><testsuite>
                <testcase classname="physics" name="slow" time="4.5"/>
                <testcase classname="physics" name="broken" time="4"><failure/></testcase>
                <testcase classname="ui" name="error" time="0.1"><error/></testcase>
                <testcase classname="ui" name="ignored"><skipped/></testcase>
                </testsuite></testsuites>''')
            text = summarize(root, [report], "false")
            self.assertIn("| Execute workspace tests | 5.25s |", text)
            self.assertIn("| 1 | 2 | 1 | 5.00s |", text)
            self.assertIn("| physics | slow | PASS | 4.500s |", text)
            self.assertLess(text.index("| slow |"), text.index("| broken |"))
            self.assertNotIn("| ignored |", text)
            self.assertIn("non-exact hit can still restore", text)

    def test_missing_reports_are_explicit_not_reported_as_passing(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "workspace.seconds").write_text("")
            (root / "empty.xml").write_text("")
            text = summarize(root, [root / "absent.xml", root / "empty.xml"])
            self.assertIn("Not produced", text)
            self.assertEqual(text.count("Not produced"), 2)
            self.assertEqual(text.count("Not recorded"), 4)
            self.assertNotIn("| PASS |", text)

    def test_test_names_are_escaped_and_slowest_list_is_bounded(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            report = root / "junit.xml"
            report.write_text('''<testsuites time="3"><testsuite>
                <testcase classname="&lt;binary&gt;" name="a|b&#10;c" time="2"/>
                <testcase classname="ignored-by-limit" name="fast" time="1"/>
                </testsuite></testsuites>''')
            text = summarize(root, [report], limit=1)
            self.assertIn("&lt;binary&gt; | a&#124;b c |", text)
            self.assertNotIn("ignored-by-limit", text)

    def test_separate_workflows_report_only_their_expected_phases(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "build.seconds").write_text("3.50\n")
            (root / "ui.seconds").write_text("8.00\n")
            text = summarize(root, [], phases=("build", "workspace", "ui", "kms"))
            self.assertIn("| Compile workspace (optimized CI profile) | 3.50s |", text)
            self.assertEqual(text.count("Not recorded"), 2)
            self.assertIn("| Execute UI tests (serial, real-time application) | 8.00s |", text)
            text = summarize(root, [], phases=("build", "ui"))
            self.assertIn("| Execute UI tests (serial, real-time application) | 8.00s |", text)
            self.assertNotIn("Not recorded", text)
            self.assertNotIn("Execute workspace tests", text)
            self.assertNotIn("Build and test vendored LinuxKMS", text)

    def test_cli_accepts_repeated_phases_and_rejects_unknown_phases(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            command = [
                sys.executable, str(Path(__file__).with_name("test_report.py")),
                "--timings-dir", str(root), "--phase", "build", "--phase", "ui",
                str(root / "missing.xml"),
            ]
            result = subprocess.run(command, capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.count("Not recorded"), 2)
            self.assertIn("Not produced", result.stdout)
            self.assertNotIn("Execute workspace tests", result.stdout)
            result = subprocess.run(
                command + ["--phase", "unknown"], capture_output=True, text=True, timeout=10,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("invalid choice", result.stderr)

    def test_invalid_durations_are_rejected(self):
        for value in ["nan", "inf", "-1", "not a duration"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                seconds(value)


if __name__ == "__main__":
    unittest.main()
