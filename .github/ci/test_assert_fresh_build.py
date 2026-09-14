import json
import unittest

from assert_fresh_build import assert_fresh_build


def artifact(name="engine-client", fresh=True):
    return {"reason": "compiler-artifact", "target": {"name": name, "kind": ["bin"]},
            "fresh": fresh}


def report(*messages):
    return [json.dumps(message) for message in messages]


SUCCESS = {"reason": "build-finished", "success": True}


class FreshBuildTests(unittest.TestCase):
    def test_all_fresh_artifacts_pass(self):
        text = assert_fresh_build(report(artifact(), artifact("test"), SUCCESS))
        self.assertIn("all 2 compiler artifacts were fresh", text)

    def test_any_rebuilt_target_fails(self):
        for name in ["engine-client", "dependency"]:
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, name):
                assert_fresh_build(report(artifact(), artifact(name, False), SUCCESS))

    def test_empty_partial_failed_or_wrong_target_build_is_not_a_pass(self):
        for messages in [[], [artifact()], [SUCCESS],
                         [artifact("unrelated"), SUCCESS],
                         [artifact(), {"reason": "build-finished", "success": False}]]:
            with self.subTest(messages=messages), self.assertRaises(ValueError):
                assert_fresh_build(report(*messages))

    def test_invalid_input_is_not_a_pass(self):
        with self.assertRaises(ValueError):
            assert_fresh_build(["not cargo JSON"])


if __name__ == "__main__":
    unittest.main()
