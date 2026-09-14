from pathlib import Path
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

    def test_invalid_durations_are_rejected(self):
        for value in ["nan", "inf", "-1", "not a duration"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                seconds(value)


if __name__ == "__main__":
    unittest.main()
