import unittest

from check_docs_data import LIMIT, check, identifier_findings, names

BIG = LIMIT + 1
SMALL = LIMIT


def run(files, texts=None, referenced=(), grandfathered=()):
    texts = texts or {}
    return check(files, lambda path: texts.get(path, ""), set(referenced), list(grandfathered))


class SizeTests(unittest.TestCase):
    def test_small_data_prose_and_images_pass(self):
        files = [("docs/data/summary.json", SMALL), ("docs/journal.md", BIG),
                 ("docs/screenshots/panel.png", BIG), ("docs/screenshots/flow.GIF", BIG)]
        self.assertIn("4 files", run(files))

    def test_oversized_data_that_no_code_reads_fails(self):
        for path in ["docs/data/raw-v1.json", "docs/data/raw-v1.json.gz", "docs/notes/dump.txt",
                     "docs/screenshots/manifest.json"]:
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "no code reads"):
                run([(path, BIG)])

    def test_oversized_data_that_code_reads_passes(self):
        text = run([("docs/data/fixture-v1.json", BIG)], referenced={"fixture-v1.json"})
        self.assertIn("1 oversized data files read by code", text)

    def test_compressed_file_counts_as_read_by_its_stem(self):
        self.assertEqual(names("docs/data/trace-v1.json.gz"), {"trace-v1.json.gz", "trace-v1.json"})
        run([("docs/data/trace-v1.json.gz", BIG)], referenced={"trace-v1.json"})

    def test_grandfathered_file_passes_while_it_stays_oversized(self):
        files = [("docs/data/old-v1.json", BIG)]
        self.assertIn("1 grandfathered", run(files, grandfathered=["docs/data/old-v1.json"]))
        with self.assertRaisesRegex(ValueError, "no longer exists"):
            run([], grandfathered=["docs/data/old-v1.json"])
        with self.assertRaisesRegex(ValueError, "no longer needs grandfathering"):
            run([("docs/data/old-v1.json", SMALL)], grandfathered=["docs/data/old-v1.json"])


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

    def test_placeholders_and_prose_pass(self):
        lines = [
            "aa:bb:cc:dd:ee:ff and 00:00:00:00:00:00 stand in for a controller",
            '"password": "MySecretPassword"',
            "password=<the owner's passphrase>",
            "psk=$WIFI_PSK",
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
