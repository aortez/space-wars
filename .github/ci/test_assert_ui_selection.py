import copy
import unittest

from assert_ui_selection import assert_ui_selection


def report(names, *, binary="ui_control_functional", package="engine-client", ignored=True):
    return {"rust-suites": {"fixture": {
        "package-name": package,
        "binary-name": binary,
        "testcases": {name: {
            "ignored": ignored, "filter-match": {"status": "matches"},
        } for name in names},
    }}}


class UiSelectionTests(unittest.TestCase):
    def setUp(self):
        self.manifest = {
            "smoke": ["launcher", "settings"],
            "full_only": [{"test": "long_lifecycle", "reason": "Repeated real-time visits"}],
        }
        self.full = report({"launcher", "settings", "long_lifecycle"})
        self.pr = report({"launcher", "settings"})

    def test_explicit_smoke_and_full_only_classification(self):
        text = assert_ui_selection(self.full, self.pr, self.manifest)
        self.assertIn("2 PR smoke workflows; 1 full-only; 3 total", text)

    def test_new_tests_require_classification_even_if_selected_on_prs(self):
        full = report({"launcher", "settings", "long_lifecycle", "new_ui_test"})
        for pr in [self.pr, report({"launcher", "settings", "new_ui_test"})]:
            with self.subTest(pr=pr), self.assertRaisesRegex(ValueError, "unclassified=.*new_ui_test"):
                assert_ui_selection(full, pr, self.manifest)

    def test_stale_smoke_and_full_only_names_are_rejected(self):
        for stale in ["launcher", "long_lifecycle"]:
            full = copy.deepcopy(self.full)
            del full["rust-suites"]["fixture"]["testcases"][stale]
            with self.subTest(stale=stale), self.assertRaisesRegex(ValueError, "stale=.*" + stale):
                assert_ui_selection(full, self.pr, self.manifest)

    def test_full_suite_cannot_filter_out_an_eligible_case(self):
        self.full["rust-suites"]["fixture"]["testcases"]["long_lifecycle"]["filter-match"] = {
            "status": "mismatch", "reason": "expression",
        }
        with self.assertRaisesRegex(ValueError, "Full UI selection omitted.*long_lifecycle"):
            assert_ui_selection(self.full, self.pr, self.manifest)

    def test_missing_smoke_and_unexpected_pr_cases_are_rejected(self):
        for pr, error in [
            (report({"launcher"}), "missing=.*settings"),
            (self.full, "unexpected=.*long_lifecycle"),
            (report({"launcher", "settings", "unknown"}), "unexpected=.*unknown"),
        ]:
            with self.subTest(pr=pr), self.assertRaisesRegex(ValueError, error):
                assert_ui_selection(self.full, pr, self.manifest)

    def test_unrelated_or_nonignored_tests_are_rejected_in_either_selection(self):
        for options in [{"binary": "other"}, {"package": "other"}, {"ignored": False}]:
            invalid = report({"launcher", "settings"}, **options)
            for full, pr in [(invalid, self.pr), (self.full, invalid)]:
                with self.subTest(options=options), self.assertRaisesRegex(ValueError, "non-UI"):
                    assert_ui_selection(full, pr, self.manifest)

    def test_duplicate_discovered_selection_is_rejected(self):
        self.pr["rust-suites"]["duplicate"] = self.pr["rust-suites"]["fixture"]
        with self.assertRaisesRegex(ValueError, "Duplicate UI test"):
            assert_ui_selection(self.full, self.pr, self.manifest)

    def test_empty_or_invalid_reports_are_rejected(self):
        for invalid in [report(set()), {}]:
            for full, pr in [(invalid, self.pr), (self.full, invalid)]:
                with self.subTest(invalid=invalid), self.assertRaises((ValueError, KeyError)):
                    assert_ui_selection(full, pr, self.manifest)

    def test_duplicate_and_overlapping_classifications_are_rejected(self):
        manifests = []
        for key in ["smoke", "full_only"]:
            manifest = copy.deepcopy(self.manifest)
            manifest[key].append(manifest[key][0])
            manifests.append(manifest)
        overlap = copy.deepcopy(self.manifest)
        overlap["smoke"].append("long_lifecycle")
        manifests.append(overlap)
        for manifest in manifests:
            with self.subTest(manifest=manifest), self.assertRaisesRegex(ValueError, "Duplicate"):
                assert_ui_selection(self.full, self.pr, manifest)

    def test_deferral_requires_a_nonempty_reason(self):
        for reason in ["", "  ", None, 3]:
            self.manifest["full_only"][0]["reason"] = reason
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, "deferral reason"):
                assert_ui_selection(self.full, self.pr, self.manifest)
        del self.manifest["full_only"][0]["reason"]
        with self.assertRaisesRegex(ValueError, "test name and reason"):
            assert_ui_selection(self.full, self.pr, self.manifest)

    def test_manifest_shape_and_names_are_checked(self):
        for invalid in [
            {}, [], {"smoke": [], "full_only": []},
            {"smoke": "launcher", "full_only": []},
            {"smoke": ["launcher"], "full_only": {}},
            {"smoke": [""], "full_only": []},
            {"smoke": [" launcher "], "full_only": []},
            {"smoke": [7], "full_only": []},
        ]:
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                assert_ui_selection(self.full, self.pr, invalid)


if __name__ == "__main__":
    unittest.main()
