"""GitHub protocol/failure fixtures; no remote refs or repository writes."""
import base64
import copy
import hashlib
import io
import json
import subprocess
import tempfile
import unittest
import urllib.error

import ota_publisher_checkpoint as checkpoint
import ota_publisher_journal as journal
from ota_delivery import REPOSITORY, digest
from test_ota_publisher_journal import NOW, genesis, nightly


class FakeGit:
    repo = REPOSITORY
    base = 'https://api.github.com/repos/' + REPOSITORY

    def __init__(self):
        self.ref = None
        self.blobs, self.trees, self.commits = {}, {}, {}
        self.calls = []
        self.failure = None
        self.hook = None

    def operation(self, method, path, body):
        if path == 'ref/heads/' + checkpoint.BRANCH:
            if self.ref is None:
                raise urllib.error.HTTPError(self.base, 404, 'missing', {}, io.BytesIO())
            return dict(ref=checkpoint.REF, object=dict(type='commit', sha=self.ref))
        kind, _, object_id = path.partition('/')
        tables = dict(blobs=self.blobs, trees=self.trees, commits=self.commits)
        if method == 'GET':
            return tables[kind][object_id]
        if path == 'blobs':
            raw = base64.b64decode(body['content'], validate=True)
            key = checkpoint.blob_id(raw)
            self.blobs[key] = dict(sha=key, encoding='base64', size=len(raw), content=body['content'])
            return dict(sha=key)
        if path in ('trees', 'commits'):
            key = hashlib.sha1(journal.encode(body)).hexdigest()
            result = dict(sha=key, **copy.deepcopy(body))
            if path == 'trees':
                result['truncated'] = False
            else:
                result['tree'] = {'sha': body['tree']}
                result['parents'] = [{'sha': p} for p in body['parents']]
            tables[path][key] = result
            return result
        if method == 'POST' and path == 'refs':
            if self.ref is not None or body['ref'] != checkpoint.REF:
                raise ValueError('Reference already exists')
            self.ref = body['sha']
        elif method == 'PATCH' and path == 'refs/heads/' + checkpoint.BRANCH:
            if body['force'] is not False:
                raise AssertionError('Force update is forbidden')
            # The fixture's only updates are a direct child of the observed head.
            if self.commits[body['sha']]['parents'] != [{'sha': self.ref}]:
                raise ValueError('Non-fast-forward update rejected')
            self.ref = body['sha']
        else:
            raise AssertionError((method, path))
        return dict(ref=checkpoint.REF, object=dict(type='commit', sha=self.ref))

    def request(self, method, url, body=None, **kwargs):
        path = url.removeprefix(self.base + '/git/')
        index = len(self.calls)
        self.calls.append((method, path, copy.deepcopy(body)))
        if self.failure == (index, 'before'):
            self.failure = None
            raise OSError('Request failed')
        if self.hook:
            hook, self.hook = self.hook, None
            hook(method, path, body)
        result = self.operation(method, path, body)
        if self.failure == (index, 'after'):
            self.failure = None
            raise OSError('Response lost')
        return copy.deepcopy(result)


def initialized():
    api = FakeGit()
    state = genesis()
    store = checkpoint.Store(api, state['root_sha256'])
    current = store.create(state, digest(journal.encode(state)))
    return api, store, current


class PublisherCheckpointTests(unittest.TestCase):
    def test_missing_state_never_silently_initializes_or_writes(self):
        api = FakeGit()
        store = checkpoint.Store(api, 'a' * 64)
        with self.assertRaisesRegex(ValueError, 'automatic initialization'):
            store.read()
        self.assertTrue(all(method == 'GET' for method, _, _ in api.calls))
        self.assertFalse(api.commits)

    def test_bootstrap_is_explicit_root_bound_and_cannot_replace_existing_history(self):
        api, store, current = initialized()
        self.assertIsNone(current.parent)
        self.assertEqual(current.state, genesis())
        with self.assertRaises(ValueError):
            store.create(genesis(), digest(journal.encode(genesis())))
        with self.assertRaises(ValueError):
            checkpoint.Store(api, 'a' * 64).read()
        api = FakeGit()
        with self.assertRaises(ValueError):
            checkpoint.Store(api, genesis()['root_sha256']).create(genesis(), 'a' * 64)
        self.assertFalse(api.commits)

    def test_every_request_failure_resumes_exact_checkpoint_without_reallocation(self):
        api, store, previous = initialized()
        selected = journal.reserve(previous.state, nightly(), NOW)
        api.calls.clear()
        expected = store.advance(previous, selected)
        count = len(api.calls)
        self.assertTrue(all(body['force'] is False for method, _, body in api.calls if method == 'PATCH'))
        for index in range(count):
            for when in ('before', 'after'):
                with self.subTest(index=index, when=when):
                    api, store, previous = initialized()
                    selected = journal.reserve(previous.state, nightly(), NOW)
                    api.calls.clear()
                    api.failure = (index, when)
                    with self.assertRaises(OSError):
                        store.advance(previous, selected)
                    recovered = store.advance(previous, selected)
                    self.assertEqual(recovered.raw, expected.raw)
                    self.assertEqual(recovered.parent, previous.head)
                    self.assertEqual(recovered.state['generation'], 1)
                    self.assertEqual(recovered.state['versions']['timestamp'], 2)

    def test_stale_writer_cannot_rebase_or_overwrite_a_competing_reservation(self):
        api, store, previous = initialized()
        winner = store.advance(previous, journal.reserve(previous.state, nightly(), NOW))
        before = len(api.commits)
        with self.assertRaisesRegex(ValueError, 'Stale'):
            store.advance(previous, journal.reserve(previous.state, nightly(b'competitor'), NOW))
        self.assertEqual(api.ref, winner.head)
        self.assertEqual(len(api.commits), before)

    def test_race_after_preflight_is_rejected_by_nonforced_ref_update(self):
        api, store, previous = initialized()
        contender = journal.reserve(previous.state, nightly(b'other'), NOW)
        winning_head = store.write_objects(journal.encode(contender), previous.head)
        original = api.operation

        def race(method, path, body):
            if method == 'PATCH':
                api.ref = winning_head
            return original(method, path, body)

        api.operation = race
        with self.assertRaisesRegex(ValueError, 'Non-fast-forward'):
            store.advance(previous, journal.reserve(previous.state, nightly(), NOW))
        self.assertEqual(store.read().head, winning_head)

    def test_history_survives_abandoned_attempts_in_parent_commits(self):
        api, store, initial = initialized()
        reserved = store.advance(initial, journal.reserve(initial.state, nightly(), NOW))
        abandoned = store.advance(reserved, journal.abandon(reserved.state, reserved.state['current']))
        retry = store.advance(abandoned, journal.reserve(abandoned.state, nightly(), NOW))
        self.assertEqual(retry.state['versions']['timestamp'], 3)
        self.assertEqual(retry.state['generation'], 1)
        api.ref = reserved.head  # Read historical fixture object, not a production ref update.
        self.assertEqual(store.read().raw, reserved.raw)
        self.assertIsNotNone(store.read().state['pending'])

    def test_corrupted_blob_tree_or_commit_is_rejected(self):
        for kind in ('blob', 'mode', 'extra-file', 'message', 'parents'):
            with self.subTest(kind=kind):
                api, store, current = initialized()
                commit = api.commits[current.head]
                tree = api.trees[commit['tree']['sha']]
                blob = api.blobs[tree['tree'][0]['sha']]
                if kind == 'blob':
                    blob['content'] = base64.b64encode(b'corrupt').decode()
                elif kind == 'mode':
                    tree['tree'][0]['mode'] = '120000'
                elif kind == 'extra-file':
                    tree['tree'].append(copy.deepcopy(tree['tree'][0]))
                elif kind == 'message':
                    commit['message'] = 'Unrelated commit'
                else:
                    commit['parents'] = [{'sha': 'a' * 40}, {'sha': 'b' * 40}]
                with self.assertRaises(ValueError):
                    store.read()

    def test_lowered_counters_or_lost_image_history_reject_before_network_mutation(self):
        api, store, initial = initialized()
        current = store.advance(initial, journal.reserve(initial.state, nightly(), NOW))
        after = journal.abandon(current.state, current.state['current'])
        changes = [lambda s: s.update(generation=0), lambda s: s['images'].clear(),
                   lambda s: s['versions'].update(timestamp=1), lambda s: s.update(revision=1)]
        for mutate in changes:
            selected = copy.deepcopy(after)
            mutate(selected)
            api.calls.clear()
            with self.assertRaises(ValueError):
                store.advance(current, selected)
            self.assertFalse(api.calls)

    def test_real_git_keeps_only_one_sibling_on_a_nonforced_branch(self):
        # Check the graph assumption against Git itself, independent of FakeGit.
        with tempfile.TemporaryDirectory() as scratch:
            def git(*args, raw=None, check=True):
                return subprocess.run(['git', '-C', scratch, '-c', 'user.name=OTA fixture',
                                       '-c', 'user.email=ota-fixture@invalid', *args], input=raw,
                                      check=check, capture_output=True)
            git('init', '--bare', '--quiet')
            tree = git('mktree', raw=b'').stdout.decode().strip()
            parent = git('commit-tree', tree, raw=b'genesis\n').stdout.decode().strip()
            siblings = [git('commit-tree', tree, '-p', parent, raw=name).stdout.decode().strip()
                        for name in (b'reservation A\n', b'reservation B\n')]
            git('update-ref', checkpoint.REF, parent)
            git('update-ref', checkpoint.REF, siblings[0], parent)
            self.assertNotEqual(git('merge-base', '--is-ancestor', siblings[0], siblings[1], check=False).returncode, 0)
            self.assertNotEqual(git('update-ref', checkpoint.REF, siblings[1], parent, check=False).returncode, 0)
            self.assertEqual(git('rev-parse', checkpoint.REF).stdout.decode().strip(), siblings[0])


if __name__ == '__main__':
    unittest.main()
