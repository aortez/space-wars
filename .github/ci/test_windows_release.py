"""Local contract regressions. These never substitute for Windows GUI evidence."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / '.github/ci/windows-release.ps1'


class PackagingTests(unittest.TestCase):
    def invoke(self, body, directory):
        pwsh = shutil.which('pwsh')
        self.assertIsNotNone(pwsh, 'PowerShell is required for packaging regressions')
        driver = directory / 'driver.ps1'
        driver.write_text(
            ". $env:RELEASE_SCRIPT -SourceCommit ('a' * 40) "
            "-OutputDirectory $env:TEST_DIRECTORY -FunctionsOnly\n" + body
        )
        return subprocess.run(
            [pwsh, '-NoProfile', '-File', str(driver)], capture_output=True, text=True,
            env={**os.environ, 'RELEASE_SCRIPT': str(SCRIPT), 'TEST_DIRECTORY': str(directory)},
        )

    def test_complete_checksums_include_nested_runtime_and_wrapper(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage = root / 'payload'
            (stage / 'assets').mkdir(parents=True)
            for name in ('engine-client.exe', 'run-software.cmd', 'build-info.json', 'assets/runtime.dll'):
                (stage / name).write_bytes(name.encode())
            result = self.invoke("Write-Checksums (Join-Path $env:TEST_DIRECTORY 'payload')", root)
            self.assertEqual(result.returncode, 0, result.stderr)
            lines = (stage / 'SHA256SUMS').read_text(encoding='utf-8-sig').splitlines()
            entries = dict(line.split('  ', 1)[::-1] for line in lines)
            self.assertEqual(set(entries), {'engine-client.exe', 'run-software.cmd', 'build-info.json', 'assets/runtime.dll'})
            for name, digest in entries.items():
                self.assertEqual(digest, hashlib.sha256((stage / name).read_bytes()).hexdigest())

    def test_reviewed_software_wrapper_bytes(self):
        expected = ('@echo off\nsetlocal\ncd /d "%~dp0"\n'
                    'set "SLINT_BACKEND=winit-software"\n'
                    '"%~dp0engine-client.exe" --renderer raster %*\n'
                    'exit /b %errorlevel%\n')
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result = self.invoke("Write-SoftwareLauncher (Join-Path $env:TEST_DIRECTORY 'run-software.cmd')", root)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / 'run-software.cmd').read_bytes().decode('ascii').replace('\r\n', '\n'), expected)

    def test_native_failure_propagates_to_process(self):
        with tempfile.TemporaryDirectory() as tmp:
            result = self.invoke("Assert-NativeExit 23 'simulated cargo failure'", Path(tmp))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('exit 23', result.stderr)

    def test_manifest_covers_every_evidence_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            evidence = root / 'evidence'
            evidence.mkdir()
            (evidence / 'release.json').write_text('{}')
            (evidence / 'build.log').write_text('actual build output')
            result = self.invoke("Write-EvidenceManifest ([ordered]@{source_commit=('a' * 40)}) (Join-Path $env:TEST_DIRECTORY 'evidence')", root)
            self.assertEqual(result.returncode, 0, result.stderr)
            manifest = json.loads((evidence / 'manifest.json').read_text(encoding='utf-8-sig'))
            self.assertEqual(set(manifest['files']), {'release.json', 'build.log'})
            for name, digest in manifest['files'].items():
                self.assertEqual(digest, hashlib.sha256((evidence / name).read_bytes()).hexdigest())


class WorkflowTests(unittest.TestCase):
    def load(self, name):
        # BaseLoader preserves "on" as a YAML string, rather than YAML 1.1 boolean.
        return yaml.load((ROOT / '.github/workflows' / name).read_text(), Loader=yaml.BaseLoader)

    def test_nightly_selects_main_for_both_events_and_never_skips_upload(self):
        workflow = self.load('windows-nightly.yml')
        self.assertEqual(set(workflow['on']), {'schedule', 'workflow_dispatch'})
        job = workflow['jobs']['windows-release']
        self.assertNotIn('if', job)
        checkout = job['steps'][0]
        self.assertEqual(checkout['with']['ref'], 'main')
        self.assertEqual(checkout['with']['persist-credentials'], 'false')
        for step in job['steps']:
            if 'run' in step:
                self.assertNotIn('if', step)
        upload = job['steps'][-1]
        self.assertEqual(upload['if'], 'always()')
        self.assertEqual(upload['with']['retention-days'], '2')
        self.assertEqual(upload['with']['if-no-files-found'], 'error')
        self.assertIn('github.run_id', upload['with']['name'])
        self.assertIn('github.run_attempt', upload['with']['name'])

    def test_candidate_head_and_shared_release_path(self):
        ci = self.load('ci.yml')
        job = ci['jobs']['windows-release']
        self.assertEqual(job['name'], 'Windows release validation')
        self.assertNotIn('if', job)  # Draft PRs must be included.
        self.assertEqual(job['steps'][0]['with']['ref'], '${{ github.event.pull_request.head.sha || github.sha }}')
        self.assertEqual(job['steps'][0]['with']['persist-credentials'], 'false')
        def release_command(workflow):
            steps = workflow['jobs']['windows-release']['steps']
            return next(s['run'] for s in steps if s.get('name') == 'Build and validate extracted release')
        self.assertEqual(release_command(ci), release_command(self.load('windows-nightly.yml')))
        diagnostics = [s for j in ci['jobs'].values() for s in j['steps']
                       if s.get('with', {}).get('name') in ('linux-test-timings', 'ui-functional-test-artifacts')]
        self.assertEqual(len(diagnostics), 2)
        self.assertTrue(all(s['with']['retention-days'] == '14' for s in diagnostics))


if __name__ == '__main__':
    unittest.main()
