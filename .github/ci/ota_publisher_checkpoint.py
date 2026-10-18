"""Durable public journal on a dedicated, fast-forward-only Git branch.

No checkout, hooks, candidate scripts, image bytes or signing keys are involved.
Git retains old checkpoint commits and their exact public signing responses.
One workflow concurrency group still serializes all feed writers. Non-forced
reference updates additionally reject competing siblings and stale writers.
The branch must disallow deletion/force-push before production activation.
"""
import base64
from dataclasses import dataclass
import hashlib
import re
import urllib.error

from ota_delivery import REPOSITORY, digest, sha
from ota_publisher_journal import MAX_BYTES, ROLES, encode, load, metadata

BRANCH = 'ota-publisher-state'
REF = 'refs/heads/' + BRANCH
GIT_SHA = re.compile(r'[0-9a-f]{40}\Z')


def git_sha(value):
    if not isinstance(value, str) or not GIT_SHA.fullmatch(value):
        raise ValueError('Invalid Git object identity')
    return value


def blob_id(raw):
    return hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()


@dataclass(frozen=True)
class Checkpoint:
    head: str
    parent: str | None
    raw: bytes

    @property
    def state(self):
        return load(self.raw)


class Store:
    def __init__(self, api, root_sha256):
        if api.repo != REPOSITORY or not sha(root_sha256):
            raise ValueError('Unexpected journal repository or trusted root')
        self.api, self.root = api, root_sha256

    def call(self, method, path, payload=None, **kwargs):
        return self.api.request(method, self.api.base + '/git/' + path, payload, **kwargs)

    def head(self):
        try:
            result = self.call('GET', 'ref/heads/' + BRANCH)
        except urllib.error.HTTPError as error:
            error.close()
            if error.code == 404:
                return None
            raise
        if result.get('ref') != REF or result.get('object', {}).get('type') != 'commit':
            raise ValueError('Unexpected publisher journal reference')
        return git_sha(result['object']['sha'])

    def read(self):
        head = self.head()
        if head is None:
            raise ValueError('Publisher journal missing; automatic initialization is forbidden')
        commit = self.call('GET', 'commits/' + head)
        parents = commit.get('parents')
        if (commit.get('sha') != head or not isinstance(parents, list) or len(parents) > 1):
            raise ValueError('Unexpected publisher journal commit')
        parent = git_sha(parents[0].get('sha')) if parents else None
        tree_id = git_sha(commit.get('tree', {}).get('sha'))
        tree = self.call('GET', 'trees/' + tree_id)
        items = tree.get('tree')
        if (tree.get('sha') != tree_id or tree.get('truncated') is not False
                or not isinstance(items, list) or len(items) != 1):
            raise ValueError('Unexpected publisher journal tree')
        entry = items[0]
        if entry.get('path') != 'state.json' or entry.get('type') != 'blob' or entry.get('mode') != '100644':
            raise ValueError('Unexpected publisher journal file')
        object_id = git_sha(entry.get('sha'))
        blob = self.call('GET', 'blobs/' + object_id, response_limit=MAX_BYTES * 2)
        if (blob.get('sha') != object_id or blob.get('encoding') != 'base64'
                or type(blob.get('size')) is not int or not 0 < blob['size'] <= MAX_BYTES
                or not isinstance(blob.get('content'), str)
                or len(blob['content']) > MAX_BYTES * 2):
            raise ValueError('Invalid journal blob response')
        # GitHub wraps base64 lines. Accept only those line separators.
        raw = base64.b64decode(blob['content'].replace('\n', ''), validate=True)
        if len(raw) != blob['size'] or blob_id(raw) != object_id:
            raise ValueError('Journal blob differs from selected Git object')
        state = load(raw)
        if state['root_sha256'] != self.root:
            raise ValueError('Journal authority differs from independently pinned root')
        if commit.get('message') != 'OTA publisher checkpoint ' + digest(raw):
            raise ValueError('Journal commit does not bind the exact checkpoint')
        return Checkpoint(head, parent, raw)

    def write_objects(self, raw, parent):
        blob = self.call('POST', 'blobs', dict(content=base64.b64encode(raw).decode(), encoding='base64'))
        if blob.get('sha') != blob_id(raw):
            raise ValueError('Created blob differs from checkpoint bytes')
        tree = self.call('POST', 'trees', dict(tree=[dict(
            path='state.json', mode='100644', type='blob', sha=blob['sha'])]))
        tree_id = git_sha(tree.get('sha'))
        commit = self.call('POST', 'commits', dict(
            message='OTA publisher checkpoint ' + digest(raw), tree=tree_id,
            parents=[parent] if parent is not None else []))
        if (commit.get('tree', {}).get('sha') != tree_id
                or commit.get('message') != 'OTA publisher checkpoint ' + digest(raw)
                or [p.get('sha') for p in commit.get('parents', [])] != ([parent] if parent else [])):
            raise ValueError('Created commit differs from selected checkpoint and parent')
        return git_sha(commit.get('sha'))

    def create(self, state, admitted_genesis_sha256):
        """Explicit, separately approved bootstrap only; never called by read()."""
        raw = encode(state)
        state = load(raw)
        if (digest(raw) != admitted_genesis_sha256 or state['root_sha256'] != self.root
                or state['revision'] != 1 or state['generation'] != 0 or state['images']
                or state['pending'] is not None or self.head() is not None):
            raise ValueError('Invalid, unadmitted or already initialized publisher genesis')
        head = self.write_objects(raw, None)
        self.call('POST', 'refs', dict(ref=REF, sha=head))
        result = self.read()
        if result.head != head or result.raw != raw:
            raise ValueError('Journal initialization changed during readback')
        return result

    def advance(self, previous, state):
        raw = encode(state)
        state = load(raw)
        old = previous.state
        if (state['root_sha256'] != self.root or old['root_sha256'] != self.root
                or state['revision'] != old['revision'] + 1
                or state['generation'] < old['generation']
                or any(state['versions'][r] < old['versions'][r] for r in ROLES)
                or not set(old['images']) <= set(state['images'])):
            raise ValueError('Checkpoint would reset publisher history')
        for payload, entry in old['images'].items():
            new = state['images'][payload]
            if (any(new[k] != entry[k] for k in ('image', 'generation', 'evidence_sha256'))
                    or any(entry[c] is not None and new[c] is None for c in ('nightly', 'release'))):
                raise ValueError('Checkpoint would reassign or erase image history')
        for role in ROLES:
            before, after = metadata(old['roles'][role]), metadata(state['roles'][role])
            if (after['version'] < before['version']
                    or (after['version'] == before['version'] and state['roles'][role] != old['roles'][role])):
                raise ValueError('Checkpoint would roll back or replace a published role')
        if (state['current']['version'] < old['current']['version']
                or (state['current']['version'] == old['current']['version']
                    and state['current'] != old['current'])):
            raise ValueError('Checkpoint would roll back or replace a publication')
        current = self.read()
        if current.parent == previous.head and current.raw == raw:
            return current  # Reconcile an already applied update with a lost response.
        if current.head != previous.head or current.raw != previous.raw:
            raise ValueError('Stale publisher checkpoint; reload before any signing')
        head = self.write_objects(raw, previous.head)
        # Never use force or rebase a stale reservation onto another writer.
        self.call('PATCH', 'refs/heads/' + BRANCH, dict(sha=head, force=False))
        result = self.read()
        if result.head != head or result.parent != previous.head or result.raw != raw:
            raise ValueError('Journal update changed during readback')
        return result
