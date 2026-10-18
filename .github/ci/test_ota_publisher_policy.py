import copy
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml

import ota_publisher_policy as helper

ROOT = Path(__file__).resolve().parents[2]


class PublisherPolicyTests(unittest.TestCase):
    def test_reviewed_workflows_require_all_jobs_and_pinned_actions(self):
        for path, events, jobs in (('.github/workflows/ota-build.yml', ('schedule', 'workflow_dispatch'), helper.BUILD_JOBS),
                                   ('.github/workflows/ci.yml', ('push', 'workflow_dispatch'), helper.CI_JOBS)):
            raw = (ROOT / path).read_bytes()
            result = helper.workflow(raw, path, events, jobs)
            self.assertEqual(result.sha256, helper.digest(raw))
            self.assertEqual(result.jobs, jobs)
        self.assertEqual(len(helper.CI_JOBS), 9)

    def test_ui_smoke_check_cannot_be_omitted_or_allowed_to_fail(self):
        path = '.github/workflows/ci.yml'
        original = yaml.load((ROOT / path).read_bytes(), Loader=yaml.BaseLoader)
        for remove in (True, False):
            with self.subTest(remove=remove):
                value = copy.deepcopy(original)
                steps = value['jobs']['linux-workspace']['steps']
                step = next(s for s in steps if s.get('name') == 'Run PR UI smoke tests')
                if remove:
                    steps.remove(step)
                else:
                    step['continue-on-error'] = 'true'
                with self.assertRaises(ValueError):
                    helper.workflow(yaml.safe_dump(value).encode(), path,
                                    ('push', 'workflow_dispatch'), helper.CI_JOBS)

    def test_mutable_actions_missing_tests_or_additional_authority_require_review(self):
        path = '.github/workflows/ota-build.yml'
        original = yaml.load((ROOT / path).read_bytes(), Loader=yaml.BaseLoader)
        for change in (lambda v: v['jobs']['candidate']['steps'][0].update(uses='actions/checkout@v7'),
                       lambda v: v['jobs']['candidate']['steps'].pop(-2),
                       lambda v: v['jobs']['candidate']['steps'][-2].update({'continue-on-error': 'true'}),
                       lambda v: v['jobs']['candidate'].update(permissions={'contents': 'write'}),
                       lambda v: v['jobs']['candidate'].update(environment='release-signing'),
                       lambda v: v['jobs']['candidate'].update(env={'KEY': '${{ secrets.OTA_KEY }}'}),
                       lambda v: v['jobs'].update(unexpected=v['jobs']['candidate']),
                       lambda v: v['on'].update(pull_request='')):
            value = copy.deepcopy(original)
            change(value)
            with self.assertRaises(ValueError):
                helper.workflow(yaml.safe_dump(value).encode(), path,
                                 ('schedule', 'workflow_dispatch'), helper.BUILD_JOBS)

    def test_policy_requires_exact_clean_independently_selected_tooling_checkout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def git(*args):
                return subprocess.check_output(['git', '-C', str(root), *args],
                    stderr=subprocess.DEVNULL, text=True).strip()
            git('init', '-b', 'fixture')
            (root / '.github/workflows').mkdir(parents=True)
            for name in ('ci.yml', 'ota-build.yml'):
                (root / '.github/workflows' / name).write_bytes((ROOT / '.github/workflows' / name).read_bytes())
            git('add', '.')
            git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                '-c', 'commit.gpgsign=false', 'commit', '-m', 'reviewed workflow fixture')
            source = git('rev-parse', 'HEAD')
            policy = helper.from_checkout(root, source)
            policy.validate()
            self.assertEqual(policy.producer_job, helper.BUILD_JOBS[0][0])
            with self.assertRaises(ValueError):
                helper.from_checkout(root, 'f' * 40)
            (root / '.github/workflows/ci.yml').write_text('unreviewed changes')
            with self.assertRaises(ValueError):
                helper.from_checkout(root, source)


if __name__ == '__main__':
    unittest.main()
