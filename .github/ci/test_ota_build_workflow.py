"""Keep candidate production separate from publication/signing authority."""
from pathlib import Path
import unittest

import yaml


class BuildWorkflowTests(unittest.TestCase):
    def test_main_only_unsigned_candidate_contract(self):
        path = Path(__file__).resolve().parents[1] / 'workflows/ota-build.yml'
        raw = path.read_text()
        workflow = yaml.load(raw, Loader=yaml.BaseLoader)
        self.assertEqual(set(workflow['on']), {'schedule', 'workflow_dispatch'})
        self.assertEqual(len(workflow['on']['schedule']), 1)
        self.assertEqual(workflow['permissions'], {'contents': 'read'})
        self.assertEqual(set(workflow['jobs']), {'candidate'})
        job = workflow['jobs']['candidate']
        self.assertEqual(job['runs-on'], 'ubuntu-24.04')
        self.assertIn("github.repository == 'aortez/space-wars'", job['if'])
        self.assertIn("github.ref == 'refs/heads/main'", job['if'])
        self.assertNotIn('environment', job)
        self.assertNotIn('permissions', job)
        self.assertNotIn('secrets.', raw)
        self.assertNotIn('id-token:', raw)
        steps = job['steps']
        for step in steps:
            if 'uses' in step:
                self.assertRegex(step['uses'], r'^[a-zA-Z0-9_/-]+@[0-9a-f]{40}$')
        checkout = steps[0]
        self.assertTrue(checkout['uses'].startswith('actions/checkout@'))
        self.assertEqual(checkout['with'], {'ref': '${{ github.sha }}', 'persist-credentials': 'false'})
        self.assertIn('ota_runner_cleanup.py', steps[1]['run'])
        build = next(i for i, step in enumerate(steps) if 'ota_container_build.py' in step.get('run', ''))
        qualify = next(i for i, step in enumerate(steps) if 'ota_candidate.py' in step.get('run', ''))
        self.assertLess(1, build)
        self.assertLess(build, qualify)
        upload = steps[-1]
        self.assertTrue(upload['uses'].startswith('actions/upload-artifact@'))
        self.assertNotIn('if', upload)  # Default success condition; never always().
        self.assertEqual(upload['with']['name'],
                         'ota-candidate-${{ github.sha }}-${{ github.run_id }}-${{ github.run_attempt }}')
        members = {p.rsplit('/', 1)[-1] for p in upload['with']['path'].splitlines()}
        self.assertEqual(members, {'image.ext4.zst', 'candidate.json', 'qualification.json', 'build.json'})
        self.assertEqual(upload['with']['if-no-files-found'], 'error')
        self.assertEqual(upload['with']['compression-level'], '0')
        self.assertLessEqual(int(upload['with']['retention-days']), 7)


if __name__ == '__main__':
    unittest.main()
