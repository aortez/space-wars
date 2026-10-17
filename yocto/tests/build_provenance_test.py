import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "meta-spacewars/lib"))
from spacewars.build_provenance import capture, write


class BuildProvenanceTest(unittest.TestCase):
    def setUp(self):
        self.source = dict(schema=1, commit="a" * 40, dirty=False, revision="a" * 12)
        self.calls = []
        self.output = [
            "rustc 1.94.0 (fixture)\nhost: x86_64-unknown-linux-gnu\nrelease: 1.94.0\n",
            "cargo 1.94.0 (fixture)\nrelease: 1.94.0\n",
            "aarch64-poky-linux-gcc (GCC) 13.3.0\n",
        ]

    def fake_run(self, command, **kwargs):
        self.calls.append((command, kwargs))
        return SimpleNamespace(stdout=self.output[len(self.calls) - 1])

    def capture(self, **kwargs):
        values = dict(source=self.source, environment={"PATH": "/recipe/tools"},
                      c_compiler='aarch64-poky-linux-gcc -mcpu=cortex-a72 "-DNAME=a b"',
                      rust_target="aarch64-unknown-linux-gnu", profile="release", feature="pi-kiosk-gpu",
                      run=self.fake_run)
        return capture(**(values | kwargs))

    def test_records_actual_versions_and_uses_recipe_environment_without_shell(self):
        result = self.capture()
        self.assertEqual(result["tools"]["rustc"]["version"], "1.94.0")
        self.assertEqual(result["tools"]["cargo"]["version"], "1.94.0")
        self.assertEqual(result["source"], self.source)
        self.assertEqual(result["rust_target"], "aarch64-unknown-linux-gnu")
        self.assertEqual(self.calls[-1][0], ["aarch64-poky-linux-gcc", "-mcpu=cortex-a72", "-DNAME=a b", "--version"])
        for _, kwargs in self.calls:
            self.assertEqual(kwargs["env"], {"PATH": "/recipe/tools"})
            self.assertTrue(kwargs["check"])
            self.assertNotIn("shell", kwargs)
            self.assertEqual(kwargs["timeout"], 15)

    def test_invalid_source_target_profile_and_tool_responses_fail_build(self):
        for values in (dict(source={}), dict(rust_target="x86_64-unknown-linux-gnu"),
                       dict(profile="dev"), dict(feature="unknown"), dict(c_compiler="")):
            with self.assertRaises(ValueError):
                self.capture(**values)
        for output in ("", "x" * 8193, "rustc nightly", "rustc\x00untrusted"):
            self.calls.clear()
            self.output[0] = output
            with self.assertRaises(ValueError):
                self.capture()

    def test_compiler_failure_is_not_replaced_with_expected_version(self):
        def failure(*args, **kwargs):
            raise subprocess.CalledProcessError(1, args[0])
        with self.assertRaises(subprocess.CalledProcessError):
            self.capture(run=failure)

    def test_record_roundtrip_and_symlink_rejection(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "build.json"
            value = self.capture()
            write(path, value)
            self.assertEqual(json.loads(path.read_text()), value)
            self.assertEqual(path.stat().st_mode & 0o777, 0o644)
            link = Path(temporary) / "link"
            link.symlink_to(path)
            with self.assertRaises(ValueError):
                write(link, {})
            self.assertEqual(json.loads(path.read_text()), value)


if __name__ == "__main__":
    unittest.main()
