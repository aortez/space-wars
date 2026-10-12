"""Local contract regressions. These never substitute for Windows GUI evidence."""
import hashlib
import json
import os
import re
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

    def test_os_and_redistributable_classification(self):
        with tempfile.TemporaryDirectory() as tmp:
            result = self.invoke(r"""
                @('msvcp_win.dll', 'MSVCP_WIN.DLL', 'ucrtbase.dll', 'VCRUNTIME140.dll', 'msvcp140_atomic_wait.dll') |
                    ForEach-Object { Get-RuntimeClass $_ }
                Assert-RuntimeModule @{name='msvcp_win.dll'; version='10.0.26100.33438 (WinBuild.160101.0800)'; path='C:\Windows\System32\msvcp_win.dll'} ([version]'14.51.36231') 'D:\app\engine-client.exe' 'C:\Windows'
            """, Path(tmp))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.splitlines(), ['windows', 'windows', 'windows', 'redistributable', 'redistributable'])

    def test_windows_msvcrt_classification_and_path_validation(self):
        for name in ('msvcrt.dll', 'MSVCRT.DLL'):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as tmp:
                result = self.invoke(
                    f"Get-RuntimeClass '{name}'; "
                    f"Assert-RuntimeModule @{{name='{name}'; version='7.0.26100.33438 (WinBuild.160101.0800)'; path='C:\\Windows\\System32\\{name}'}} ([version]'14.51.36231') 'D:\\app\\engine-client.exe' 'C:\\Windows'",
                    Path(tmp))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), 'windows')
        with tempfile.TemporaryDirectory() as tmp:
            result = self.invoke(r"""
                Assert-RuntimeModule @{name='msvcrt.dll'; version='7.0.26100.33438'; path='D:\tools\msvcrt.dll'} ([version]'14.51.36231') 'D:\app\engine-client.exe' 'C:\Windows'
            """, Path(tmp))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Unbundled dependency outside Windows', result.stderr)

    def test_actual_redistributable_minimum_is_enforced(self):
        for name in ('VCRUNTIME140.dll', 'msvcp140.dll', 'concrt140.dll'):
            for version, succeeds in [('14.44.35211.0', False), ('14.51.36247.0', True)]:
                with self.subTest(name=name, version=version), tempfile.TemporaryDirectory() as tmp:
                    result = self.invoke(
                        f"Assert-RuntimeModule @{{name='{name}'; version='{version}'; path='C:\\Windows\\System32\\{name}'}} ([version]'14.51.36231') 'D:\\app\\engine-client.exe' 'C:\\Windows'",
                        Path(tmp))
                    self.assertEqual(result.returncode == 0, succeeds, result.stderr)
                    if not succeeds:
                        self.assertIn('Runtime below documented minimum', result.stderr)

    def test_undeclared_dependencies_still_fail(self):
        for body, message in [
            ("Get-RuntimeClass 'msvcr120.dll'", 'Undeclared MSVC dependency'),
            ("Get-RuntimeClass 'msvcp120.dll'", 'Undeclared MSVC dependency'),
            ("Assert-RuntimeModule @{name='vcomp140.dll'; version='14.51.36247.0'; path='C:\\Windows\\System32\\vcomp140.dll'} ([version]'14.51.36231') 'D:\\app\\engine-client.exe' 'C:\\Windows'", 'Undeclared MSVC dependency'),
            ("Assert-RuntimeModule @{name='extra.dll'; version='1.0'; path='D:\\tools\\extra.dll'} ([version]'14.51.36231') 'D:\\app\\engine-client.exe' 'C:\\Windows'", 'Unbundled dependency outside Windows'),
        ]:
            with self.subTest(body=body), tempfile.TemporaryDirectory() as tmp:
                result = self.invoke(body, Path(tmp))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)

    def test_installer_guidance_matches_retained_toolset_minimum(self):
        # Operator audit, 2026-10-11: current v14 installer 14.51.36247.0;
        # VS 2022-specific installer 14.44.35211.0. Data, not an installer run.
        observed_installers = {
            'https://aka.ms/vc14/vc_redist.x64.exe': (14, 51, 36247),
            'https://aka.ms/vs/17/release/vc_redist.x64.exe': (14, 44, 35211),
        }
        with tempfile.TemporaryDirectory() as tmp:
            result = self.invoke("Get-VcInstructions ([version]'14.51.36231')", Path(tmp))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('version 14.51.36231 or newer', result.stdout)
            url = 'https://aka.ms/vc14/vc_redist.x64.exe'
            selected_url = re.search(r'https://aka.ms/[^ ]+', result.stdout).group()
            minimum = tuple(map(int, re.search(r'version ([0-9.]+) or newer', result.stdout).group(1).split('.')))
            self.assertGreaterEqual(observed_installers[selected_url], minimum)
            self.assertEqual(selected_url, url)
            self.assertIn(url, (ROOT / 'README.md').read_text())
            self.assertIn('Product version', result.stdout)
            self.assertNotIn('vs/17/release', result.stdout)

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

    def test_publication_permissions_and_success_gate(self):
        workflow = self.load('windows-nightly.yml')
        self.assertEqual(workflow['permissions'], {'contents': 'read'})
        build = workflow['jobs']['windows-release']
        self.assertNotIn('permissions', build)
        publish = workflow['jobs']['publish']
        self.assertEqual(publish['needs'], 'windows-release')
        self.assertEqual(publish['if'], "${{ needs.windows-release.result == 'success' }}")
        self.assertEqual(publish['permissions'], {'contents': 'write'})
        self.assertEqual(publish['concurrency'], {'group': 'windows-nightly-publication', 'cancel-in-progress': 'false'})
        download = publish['steps'][1]
        self.assertEqual(download['with']['artifact-ids'], '${{ needs.windows-release.outputs.artifact }}')
        self.assertEqual(publish['steps'][0]['with']['ref'], '${{ needs.windows-release.outputs.source }}')
        self.assertIn('steps.evidence.outputs.artifact-id', build['outputs']['artifact'])
        ci = self.load('ci.yml')
        self.assertNotIn('publish', ci['jobs'])
        self.assertNotIn('publish_nightly', str(ci))
        self.assertNotIn('contents: write', (ROOT / '.github/workflows/ci.yml').read_text())

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
