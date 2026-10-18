import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import yaml

import ota_test_selection as helper


class SelectionTests(unittest.TestCase):
    def test_only_known_unrelated_paths_get_smaller_suite(self):
        self.assertEqual(helper.classify(['scenarios/clock/src/events/duck.rs', 'docs/clock.md']), 'pr')
        self.assertEqual(helper.classify(['crates/engine-client/src/ui_render_tests/duck.rs']), 'pr')
        for path in ('crates/engine-ota/src/bootstrap.rs', 'crates/engine-os-manager/src/main.rs',
                     'crates/engine-client/src/settings.rs', 'yocto/scripts/update.sh',
                     '.github/workflows/ci.yml', '.config/nextest.toml', 'Cargo.toml', 'Cargo.lock',
                     'rust-toolchain.toml', 'scenarios/clock/Cargo.toml', 'scenarios/clock/build.rs',
                     'scenarios/clock/build.sh', 'scenarios/clock/config.toml',
                     'unknown/file.rs', '/docs/a', 'docs/../Cargo.lock', 'docs//a'):
            with self.subTest(path=path):
                self.assertEqual(helper.classify(['docs/clock.md', path]), 'full')
        self.assertEqual(helper.classify([]), 'full')

    def test_verified_merge_diff_includes_deleted_side_of_renames(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / 'repo'
            repo.mkdir()
            def git(*args):
                return subprocess.check_output(['git', *args], cwd=repo, stderr=subprocess.DEVNULL).decode().strip()
            def commit(message):
                git('add', '.')
                git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                    '-c', 'commit.gpgsign=false', 'commit', '-m', message)
            git('init', '-b', 'main')
            (repo / 'Cargo.lock').write_text('original dependency fixture')
            commit('base')
            base = git('rev-parse', 'HEAD')
            git('checkout', '-b', 'topic')
            (repo / 'docs').mkdir()
            (repo / 'docs/note.md').write_text('gameplay note')
            commit('docs')
            head = git('rev-parse', 'HEAD')
            git('checkout', 'main')
            git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                '-c', 'commit.gpgsign=false', 'merge', '--no-ff', 'topic', '-m', 'merge')
            event = Path(temporary) / 'event.json'
            event.write_text(json.dumps({'pull_request': {'base': {'sha': base}, 'head': {'sha': head}}}))
            env = dict(GITHUB_EVENT_NAME='pull_request', GITHUB_EVENT_PATH=str(event), GITHUB_SHA=git('rev-parse', 'HEAD'))
            self.assertEqual(helper.selection(env, repo)['mode'], 'pr')
            self.assertEqual(helper.selection(dict(env, SPACEWARS_OTA_FULL='true'), repo)['mode'], 'full')
            shallow = Path(temporary) / 'shallow'
            subprocess.run(['git', 'clone', '--depth=2', repo.as_uri(), str(shallow)],
                           check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            self.assertEqual(helper.selection(env, shallow)['mode'], 'pr')
            self.assertEqual(helper.selection(dict(env, GITHUB_SHA='a' * 40), repo)['mode'], 'full')
            for name in ('push', 'schedule', 'workflow_dispatch', 'pull_request_target', ''):
                self.assertEqual(helper.selection(dict(env, GITHUB_EVENT_NAME=name), repo)['mode'], 'full')
            # A dependency deleted/renamed into an allowed directory must still
            # select full coverage. Diffing both paths avoids a rename bypass.
            git('checkout', 'topic')
            git('mv', 'Cargo.lock', 'docs/old-lock')
            commit('rename dependency')
            head = git('rev-parse', 'HEAD')
            git('checkout', 'main')
            base = git('rev-parse', 'HEAD')
            git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                '-c', 'commit.gpgsign=false', 'merge', '--no-ff', 'topic', '-m', 'merge again')
            event.write_text(json.dumps({'pull_request': {'base': {'sha': base}, 'head': {'sha': head}}}))
            env['GITHUB_SHA'] = git('rev-parse', 'HEAD')
            result = helper.selection(env, repo)
            self.assertEqual(result['mode'], 'full')
            self.assertIn('Cargo.lock', result['paths'])
            self.assertIn('docs/old-lock', result['paths'])
            event.write_text('invalid JSON')
            self.assertEqual(helper.selection(env, repo)['mode'], 'full')

    def test_missing_history_or_event_defaults_to_full(self):
        self.assertEqual(helper.selection({'GITHUB_EVENT_NAME': 'pull_request'})['mode'], 'full')
        with tempfile.TemporaryDirectory() as temporary:
            event = Path(temporary) / 'event.json'
            event.write_text(json.dumps({'pull_request': {'base': {'sha': 'a' * 40}, 'head': {'sha': 'b' * 40}}}))
            env = dict(GITHUB_EVENT_NAME='pull_request', GITHUB_EVENT_PATH=str(event), GITHUB_SHA='c' * 40)
            with patch.object(helper, 'git', side_effect=subprocess.CalledProcessError(128, ['git'])):
                self.assertEqual(helper.selection(env)['mode'], 'full')

    def test_discovery_rejects_missing_representatives_and_ambiguous_skips(self):
        suites = helper.catalog()
        names = suites['exhaustive'] + suites['required_pr']
        listing = '\n'.join(name + ': test' for name in names)
        self.assertEqual(helper.validate_discovery(listing, suites), len(names))
        for name in names:
            with self.subTest(name=name):
                with self.assertRaises(ValueError):
                    helper.validate_discovery(listing.replace(name + ': test', ''), suites)
                with self.assertRaises(ValueError):
                    helper.validate_discovery(listing, suites, name + ': test')
        for extra in (names[0], names[0] + '_new_case'):
            with self.assertRaises(ValueError):
                helper.validate_discovery(listing + '\n' + extra + ': test', suites)
        self.assertEqual(helper.validate_discovery(listing + '\nnew::regression: test', suites), len(names) + 1)

    def test_workspace_delegates_only_cataloged_sweeps_to_required_ota_job(self):
        suites = helper.catalog()
        expression = helper.workspace_filter(suites)
        for name in suites['exhaustive']:
            self.assertIn(f'test(={name})', expression)
        for name in suites['required_pr']:
            self.assertNotIn(name, expression)
        workflow = yaml.load((helper.ROOT / '.github/workflows/ci.yml').read_text(), Loader=yaml.BaseLoader)
        ota = workflow['jobs']['ota-publisher']
        self.assertNotIn('if', ota)
        steps = {step.get('name'): step for step in ota['steps']}
        for name in ('Select OTA recovery coverage', 'Test updater, metadata, image and publisher fixtures'):
            self.assertNotIn('if', steps[name])
            self.assertNotIn('continue-on-error', steps[name])
        self.assertEqual(ota['steps'][0]['with']['fetch-depth'], '2')
        headless = workflow['jobs']['linux-headless']
        step = next(step for step in headless['steps'] if step.get('name') == 'Run workspace tests without compiling')
        self.assertIn('ota_test_selection.py workspace --', step['run'])
        self.assertIn('--archive-file', step['run'])


if __name__ == '__main__':
    unittest.main()
