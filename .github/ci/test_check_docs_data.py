import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from check_docs_data import LIMIT, check, identifier_findings, is_placeholder, names, referenced_names

BIG = LIMIT + 1
SMALL = LIMIT


def run(files, texts=None, referenced=(), grandfathered=(), baseline=None):
    texts = texts or {}
    return check(files, lambda path: texts.get(path, ""), set(referenced), list(grandfathered), baseline)


class SizeTests(unittest.TestCase):
    def test_small_data_prose_and_images_pass(self):
        files = [("docs/data/summary.json", SMALL), ("docs/journal.md", BIG),
                 ("docs/screenshots/panel.png", BIG), ("docs/screenshots/flow.GIF", BIG)]
        self.assertIn("4 files", run(files))

    def test_oversized_data_that_no_source_names_fails(self):
        for path in ["docs/data/raw-v1.json", "docs/data/raw-v1.json.gz", "docs/notes/dump.txt",
                     "docs/screenshots/manifest.json"]:
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "no source file names"):
                run([(path, BIG)])

    def test_oversized_data_that_code_reads_passes(self):
        text = run([("docs/data/fixture-v1.json", BIG)], referenced={"fixture-v1.json"})
        self.assertIn("1 oversized data files read by code", text)

    def test_compressed_file_counts_as_read_by_its_stem(self):
        self.assertEqual(names("docs/data/trace-v1.json.gz"), {"trace-v1.json.gz", "trace-v1.json"})
        run([("docs/data/trace-v1.json.gz", BIG)], referenced={"trace-v1.json"})


class GrandfatheredTests(unittest.TestCase):
    def test_grandfathered_file_passes_while_it_stays_oversized(self):
        files = [("docs/data/old-v1.json", BIG)]
        self.assertIn("1 grandfathered", run(files, grandfathered=["docs/data/old-v1.json"]))
        with self.assertRaisesRegex(ValueError, "no longer exists"):
            run([], grandfathered=["docs/data/old-v1.json"])
        with self.assertRaisesRegex(ValueError, "no longer needs grandfathering"):
            run([("docs/data/old-v1.json", SMALL)], grandfathered=["docs/data/old-v1.json"])

    def test_list_may_only_shrink_against_the_baseline(self):
        files = [("docs/data/old-v1.json", BIG), ("docs/data/new-v1.json", BIG)]
        both = ["docs/data/new-v1.json", "docs/data/old-v1.json"]
        run(files, grandfathered=both, baseline=both)
        run([files[0]], grandfathered=both[1:], baseline=both)
        with self.assertRaisesRegex(ValueError, "new-v1.json: added to .*only shrinks"):
            run(files, grandfathered=both, baseline=both[1:])

    def test_without_a_baseline_additions_are_not_compared(self):
        run([("docs/data/new-v1.json", BIG)], grandfathered=["docs/data/new-v1.json"])


class ReferenceTests(unittest.TestCase):
    def test_only_source_files_count_as_readers(self):
        files = [("docs/data/big-results-v1.json", BIG)]
        with tempfile.TemporaryDirectory() as directory:
            cwd = os.getcwd()
            os.chdir(directory)
            try:
                subprocess.run(["git", "init", "-q"], check=True)
                Path("notes").mkdir()
                Path("notes/results-index.txt").write_text("big-results-v1.json\n")
                Path("README.md").write_text("See big-results-v1.json.\n")
                Path("docs").mkdir()
                Path("docs/index.py").write_text("'big-results-v1.json'\n")
                subprocess.run(["git", "add", "."], check=True)
                self.assertEqual(referenced_names(files), set())
                Path("tools").mkdir()
                Path("tools/read.py").write_text("load('docs/data/big-results-v1.json')\n")
                subprocess.run(["git", "add", "."], check=True)
                self.assertEqual(referenced_names(files), {"big-results-v1.json"})
            finally:
                os.chdir(cwd)


class IdentifierTests(unittest.TestCase):
    def test_device_identifiers_and_secrets_fail(self):
        cases = {
            "MAC address": 'path "/data/bluetooth/DC:A6:32:01:02:03/attributes"',
            "serial number": '"board_serial": "100000001a2b3c4d"',
            "credential": '"password": "hunter2!hunter2"',
            "private key": "-----BEGIN OPENSSH PRIVATE KEY-----",
        }
        for kind, line in cases.items():
            with self.subTest(kind=kind):
                self.assertEqual(identifier_findings("docs/x.md", line), [f"docs/x.md:1: {kind}"])
                with self.assertRaisesRegex(ValueError, kind):
                    run([("docs/x.md", 10)], {"docs/x.md": line})

    def test_cpuinfo_and_udev_serial_forms_fail(self):
        for line in ["Serial\t\t: 100000001a2b3c4d", "ID_SERIAL=SMI_USB_DISK_AA00000000012345-0:0"]:
            with self.subTest(line=line):
                self.assertEqual(identifier_findings("docs/x.md", line), ["docs/x.md:1: serial number"])

    def test_real_credentials_with_placeholder_fragments_fail(self):
        for line in ['"psk": "P9!m$T7qR2"', "password=Secret-7hR2xQ", 'wpa_passphrase="changeme-but-not-really"',
                     "psk: my$ecretkey42"]:
            with self.subTest(line=line):
                self.assertEqual(identifier_findings("docs/x.md", line), ["docs/x.md:1: credential"])

    def test_whole_placeholders_pass(self):
        for value in ["MySecretPassword", "<the owner's passphrase>", "$WIFI_PSK", "${WIFI_PSK}", "%PSK%",
                      "{{ wifi_psk }}", "...", "xxxxxxxx", "********", "changeme", "your-password-here",
                      "REDACTED", "none"]:
            with self.subTest(value=value):
                self.assertTrue(is_placeholder(value))
        for value in ["P9!m$T7qR2", "hunter2", "secret7", "my-secret-$value", "s3cret"]:
            with self.subTest(value=value):
                self.assertFalse(is_placeholder(value))

    def test_placeholders_and_prose_pass(self):
        lines = [
            "aa:bb:cc:dd:ee:ff and 00:00:00:00:00:00 stand in for a controller",
            '"password": "MySecretPassword"',
            "password=<the owner's passphrase>",
            "psk=$WIFI_PSK  # set by the owner",
            "password: '...'",
            "The serialized receipts and serial match results are identical.",
            "Serialization: the records keep their original shape.",
            "The run took 01:02:03 and the next 04:05:06.",
        ]
        self.assertEqual(identifier_findings("docs/x.md", "\n".join(lines)), [])

    def test_findings_name_the_line(self):
        text = "fine\n\nSerial: 100000001a2b3c4d\n"
        self.assertEqual(identifier_findings("docs/x.md", text), ["docs/x.md:3: serial number"])

    def test_compressed_and_image_files_are_not_scanned(self):
        texts = {"docs/data/raw.json.gz": "DC:A6:32:01:02:03", "docs/shot.svg": "DC:A6:32:01:02:03"}
        run([(path, 10) for path in texts], texts)


if __name__ == "__main__":
    unittest.main()
