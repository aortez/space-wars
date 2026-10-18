import hashlib
import json
from pathlib import Path
import unittest

import yaml

import ota_build_inputs as builder
import ota_image_policy as policy
import test_ota_build_inputs as fixtures


class ImagePolicyTests(unittest.TestCase):
    setUp = fixtures.InputsTests.setUp

    def source_config(self):
        config = yaml.safe_load((Path(__file__).resolve().parents[2] / 'yocto/kas-spacewars.yml').read_text())
        config['repos'] = self.configuration['repos']
        return config

    def select(self, config):
        (self.root / 'yocto/kas-spacewars.yml').write_text(yaml.safe_dump(config))
        self.source = fixtures.commit(self.root)
        self.configuration = config

    def test_expected_policy_matches_builder_inputs_without_reading_candidate_claims(self):
        self.select(self.source_config())
        raw = json.dumps(builder.snapshot(self.root, self.source, self.configuration),
                         sort_keys=True, separators=(',', ':')).encode()
        expected = policy.from_source(self.root, self.source)
        self.assertEqual(expected['build_inputs_sha256'], hashlib.sha256(raw).hexdigest())
        self.assertEqual(expected['source_commit'], self.source)
        self.assertEqual(expected['rust_version'], '1.94.0')
        self.assertEqual(expected['cc_version_line'], 'aarch64-poky-linux-gcc (GCC) 13.5.0')
        # Policy derives pins from reviewed source; it does not query or execute
        # potentially stale or compromised builder layer checkouts.
        (self.root / 'yocto/poky/recipe').write_text('irrelevant builder checkout')
        self.assertEqual(policy.from_source(self.root, self.source), expected)

    def test_wrong_or_dirty_source_is_not_an_independent_policy(self):
        self.select(self.source_config())
        with self.assertRaises(ValueError):
            policy.from_source(self.root, 'f' * 40)
        (self.root / 'Cargo.lock').write_text('edited dependency set')
        with self.assertRaises(ValueError):
            policy.from_source(self.root, self.source)

    def test_includes_overrides_or_expanded_environment_require_policy_review(self):
        config = self.source_config()
        for field, replacement in (('header', {'version': 14, 'includes': ['other.yml']}),
                                   ('overrides', {'repos': {'poky': {'commit': 'f' * 40}}}),
                                   ('env', {'SPACEWARS_CACHE_ROOT': '/builder/cache'})):
            self.select(config | {field: replacement})
            with self.assertRaises(ValueError):
                policy.from_source(self.root, self.source)


if __name__ == '__main__':
    unittest.main()
