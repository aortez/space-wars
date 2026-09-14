import unittest

from assert_ui_selection import DEFERRED, assert_ui_selection


def report(names, *, binary="ui_control_functional", ignored=True):
    return {"rust-suites": {"fixture": {
        "package-name": "engine-client",
        "binary-name": binary,
        "testcases": {name: {
            "ignored": ignored, "filter-match": {"status": "matches"},
        } for name in names},
    }}}


class UiSelectionTests(unittest.TestCase):
    def test_only_approved_long_cases_are_deferred(self):
        pr = {"launcher_navigation", "autostart", "settings", "new_ui_test"}
        text = assert_ui_selection(report(DEFERRED | pr), report(pr))
        self.assertIn("4 PR workflows; 4 deferred; 8 total", text)

    def test_new_ui_tests_cannot_be_silently_excluded(self):
        with self.assertRaisesRegex(ValueError, "new_ui_test"):
            assert_ui_selection(report(DEFERRED | {"launcher", "new_ui_test"}), report({"launcher"}))

    def test_stale_exclusion_names_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "missing from full suite"):
            assert_ui_selection(report({"launcher"}), report({"launcher"}))

    def test_long_cases_accidentally_included_on_prs_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "unexpected="):
            assert_ui_selection(report(DEFERRED | {"launcher"}), report(DEFERRED | {"launcher"}))

    def test_unrelated_or_nonignored_tests_are_rejected(self):
        for options in [{"binary": "other"}, {"ignored": False}]:
            with self.subTest(options=options), self.assertRaisesRegex(ValueError, "non-UI"):
                assert_ui_selection(report(DEFERRED | {"launcher"}), report({"launcher"}, **options))

    def test_empty_or_invalid_reports_are_rejected(self):
        for invalid in [report(set()), {}]:
            with self.subTest(invalid=invalid), self.assertRaises((ValueError, KeyError)):
                assert_ui_selection(report(DEFERRED | {"launcher"}), invalid)


if __name__ == "__main__":
    unittest.main()
