"""End-to-end LOCAL check of ota_release.py with disposable keys and a mocked GitHub.

Never use this module to bootstrap production authority. It builds a throwaway
authority, workspace and configuration, then drives the release command with
the real signer and inventory tools through a Nightly publication whose
delivery loses a reply and resumes, a Release promotion, a renewal signed and
then published, an abandoned Nightly planned again under its generation, and
the renewal of a feed whose timestamp has already expired. Candidate selection
is replaced by a synthetic one; test_ota_release.py covers the real checks.
"""
import argparse
from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from unittest.mock import patch

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, NoEncryption, PrivateFormat

import ota_delivery as delivery
import ota_publisher_journal as journal
import ota_release as release
from test_ota_delivery import FakeGitHub
from test_ota_publisher_journal import nightly

TOP = ('root', 'targets', 'snapshot', 'timestamp')
MEMBERS = dict(release.MEMBERS, root=['root'])


class Feed(FakeGitHub):
    """The delivery tests' fake release, with GitHub's asset digests and a read-only mode."""
    read_only = False

    def insert(self, name, data, state='uploaded'):
        asset = super().insert(name, data, state)
        asset['digest'] = 'sha256:' + delivery.digest(data)
        return asset

    def writable(self):
        if self.read_only:
            raise AssertionError('A read-only phase tried to change GitHub')

    def create_release(self, source, content):
        self.writable()
        return super().create_release(source, content)

    def update_release(self, value, content):
        self.writable()
        return super().update_release(value, content)

    def upload(self, value, name, stream, length):
        self.writable()
        return super().upload(value, name, stream, length)

    def delete_asset(self, asset):
        self.writable()
        return super().delete_asset(asset)


class Publisher(release.Publisher):
    """Publishes to the fake feed and reads synthetic candidates."""

    def __init__(self, config, feed):
        super().__init__(config)
        self.feed = feed

    def github(self, mutating):
        self.feed.read_only = not mutating
        return self.feed

    def gh_json(self, args):
        raise AssertionError('The fixture never asks GitHub about CI')

    def select_nightly(self, candidate):
        root = Path(candidate)
        raw = (root / 'intent.json').read_bytes()
        return (json.loads(raw), dict(candidate=str(root), preparation_sha256=delivery.digest(raw), ci='mocked'),
                root / 'image.ext4.zst')


def write_private(path, data):
    with open(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb') as stream:
        stream.write(data)


def authority(directory, signer, inventory, now, timestamp_expires):
    """A disposable authority, workspace and configuration, with the genesis feed published to a fake."""
    keys = {name: Ed25519PrivateKey.generate() for names in MEMBERS.values() for name in names}
    public = {name: dict(keytype='ed25519', scheme='ed25519',
                         keyval=dict(public=key.public_key().public_bytes_raw().hex())) for name, key in keys.items()}
    ids = {name: delivery.digest(journal.encode(key)) for name, key in public.items()}

    def signed(body, role):
        raw = journal.encode(body)
        signatures = sorted((dict(keyid=ids[k], sig=keys[k].sign(raw).hex()) for k in MEMBERS[role]),
                            key=lambda value: value['keyid'])
        return journal.encode(dict(signed=body, signatures=signatures))

    def base(role, expires):
        return dict(_type=role, spec_version='1.0.0', version=1, expires=release.stamp(expires))

    def meta(raw):
        return dict(version=1, length=len(raw), hashes=dict(sha256=delivery.digest(raw)))

    month = now + timedelta(days=30)
    root = signed(dict(base('root', now + timedelta(days=365)), consistent_snapshot=True,
                       keys={ids[r]: public[r] for r in TOP},
                       roles={r: dict(keyids=[ids[r]], threshold=1) for r in TOP}), 'root')
    delegations = dict(keys={ids[k]: public[k] for c in release.CHANNELS for k in MEMBERS[c]},
                       roles=[dict(name=c, keyids=[ids[k] for k in MEMBERS[c]], threshold=len(MEMBERS[c]),
                                   paths=[c + '.*'], terminating=True) for c in release.CHANNELS])
    roles = {c: signed(dict(base('targets', month), targets={}), c) for c in release.CHANNELS}
    roles['targets'] = signed(dict(base('targets', month), targets={}, delegations=delegations), 'targets')
    components = {r + '.json': meta(roles[r]) for r in ('targets', 'release', 'nightly')}
    roles['snapshot'] = signed(dict(base('snapshot', month), meta=components), 'snapshot')
    roles['timestamp'] = signed(dict(base('timestamp', timestamp_expires),
                                     meta={'snapshot.json': meta(roles['snapshot'])}), 'timestamp')

    directory.mkdir(mode=0o700)
    home = directory / 'authority'
    for path in (home, home / 'public', home / 'public/genesis', home / 'keys'):
        path.mkdir(mode=0o700)
    (home / 'public/genesis/1.root.json').write_bytes(root)
    (directory / 'publication.lock').touch()
    for name, key in keys.items():
        if name != 'root':
            write_private(home / 'keys' / (name + '.pkcs8'),
                          key.private_bytes(Encoding.DER, PrivateFormat.PKCS8, NoEncryption()))

    workspace = directory / 'workspace'
    genesis = workspace / 'genesis'
    genesis.mkdir(mode=0o700, parents=True)
    (genesis / '1.root.json').write_bytes(root)
    for role, raw in roles.items():
        (genesis / ('timestamp.json' if role == 'timestamp' else f'1.{role}.json')).write_bytes(raw)
    at = release.stamp(min(now, timestamp_expires - timedelta(seconds=1)))
    manifest = subprocess.check_output([str(inventory), str(home / 'public/genesis/1.root.json'), str(genesis), at],
                                       env=release.TOOL_ENV, timeout=120)
    bundle = delivery.Bundle(genesis, manifest, delivery.digest(manifest))
    feed = Feed()
    delivery.publish(feed, bundle, 'a' * 40)
    state = journal.initialize(root, {r: raw.decode() for r, raw in roles.items()}, bundle.record)
    write_private(home / 'public/publisher-state.json', journal.encode(state))
    (workspace / 'renewal').mkdir(mode=0o700)
    (workspace / 'renewal/latest-bundle.json').write_text(json.dumps(dict(
        schema=1, version=1, manifest_sha256=bundle.record['manifest_sha256'], bundle=str(genesis))))

    tools = directory / 'tools'
    tools.mkdir(mode=0o700)
    shutil.copy2(signer, tools / 'feed_sign')
    shutil.copy2(inventory, tools / 'feed_inventory')
    (tools / 'image_candidate').write_bytes(b'Never run: the fixture selects candidates itself.\n')
    path = directory / 'release.json'
    write_private(path, json.dumps(dict(
        schema=1, authority=str(home), workspace=str(workspace), root_sha256=delivery.digest(root),
        tools_directory=str(tools),
        tools={name: release.file_blob(tools / name)['sha256'] for name in release.TOOLS})).encode())
    return release.Config.load(path), feed


def candidate(directory, payload):
    """A synthetic prepared candidate: a payload and its Nightly intent."""
    directory.mkdir(mode=0o700)
    selected = nightly(payload)
    (directory / 'image.ext4.zst').write_bytes(payload)
    (directory / 'intent.json').write_bytes(journal.encode(selected))
    return directory, selected


def lose_reply(feed, name):
    """Applies the next call named `name`, then loses its reply."""
    def hook(called):
        if called == name:
            feed.hook = None
            raise OSError('Fixture: lost the reply to ' + name)
    feed.hook = hook


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def refuses(action, text):
    try:
        action()
    except ValueError as error:
        check(text in str(error), f'Expected a refusal naming {text!r}, not: {error}')
        return
    raise AssertionError('Expected a refusal naming ' + repr(text))


def run(signer, inventory):
    signer, inventory = (Path(p).resolve(strict=True) for p in (signer, inventory))
    now = datetime.now(timezone.utc).replace(microsecond=0)
    with tempfile.TemporaryDirectory(prefix='ota-release-fixture-') as scratch:
        directory = Path(scratch)
        config, feed = authority(directory / 'current', signer, inventory, now, now + timedelta(days=7))

        def session():
            return Publisher(config, feed)

        first, selected = candidate(directory / 'first', b'first fixture image')
        payload = journal.identity(selected['image'])
        with session() as publisher:
            plan = publisher.nightly(first)
        check((plan['version'], plan['generation'], plan['targets_after']) == (2, 1, dict(release=0, nightly=1)),
              'Unexpected Nightly plan')
        with session() as publisher:
            refuses(lambda: publisher.proceed('sign', 2, '0' * 64), 'No plan for v2 has that digest')
            refuses(lambda: publisher.proceed('publish', 2, plan['plan_sha256']), 'Sign the plan before publishing')
            with patch.object(release, 'code_digests', return_value={}):
                refuses(lambda: publisher.proceed('sign', 2, plan['plan_sha256']), 'release command changed')
            publisher.proceed('sign', 2, plan['plan_sha256'])
        check(delivery.parse_state(feed.release)['current']['version'] == 1, 'Signing changed the feed')
        lose_reply(feed, 'upload:timestamp.json')
        try:
            with session() as publisher:
                publisher.proceed('publish', 2, plan['plan_sha256'])
            raise AssertionError('The interrupted publication finished')
        except OSError as error:
            check(str(error).startswith('Fixture: lost the reply'), 'Unexpected failure: ' + str(error))
        check(delivery.parse_state(feed.release)['pending']['version'] == 2, 'The feed is not mid-publication')
        with session() as publisher:
            nightly_receipt = publisher.proceed('publish', 2, plan['plan_sha256'])
        with session() as publisher:
            check(publisher.proceed('publish', 2, plan['plan_sha256'])['already_published'],
                  'A finished plan published again')

        record, notes = directory / 'qualification.json', directory / 'notes.txt'
        named = dict(selected['image'], generation=1)
        record.write_text(json.dumps(dict(purpose='release-qualification', image=named)))
        notes.write_text('Fixture promotion only; no physical rollout.\n')
        with session() as publisher:
            plan = publisher.promote(payload, record, notes)
        with session() as publisher:
            publisher.proceed('sign', 3, plan['plan_sha256'])
        with session() as publisher:
            promotion = publisher.proceed('publish', 3, plan['plan_sha256'])

        # A plan left unsigned neither blocks a renewal nor survives it.
        second, _ = candidate(directory / 'second', b'second fixture image')
        with session() as publisher:
            stale = publisher.nightly(second)
        with session() as publisher:
            check(publisher.renew('sign')['published'] is False, 'renew sign published')
        with session() as publisher:
            renewal = publisher.renew('publish')
        with session() as publisher:
            refuses(lambda: publisher.proceed('sign', 4, stale['plan_sha256']),
                    'changed since this operation was prepared')

        with session() as publisher:
            plan = publisher.nightly(second)
        with session() as publisher:
            publisher.proceed('sign', 5, plan['plan_sha256'])
        with session() as publisher:
            abandoned = publisher.abandon()
        with session() as publisher:
            plan = publisher.nightly(second)
        check((plan['version'], plan['generation']) == (6, 2), 'The abandoned Nightly lost its generation')
        with session() as publisher:
            publisher.proceed('sign', 6, plan['plan_sha256'])
        with session() as publisher:
            replanned = publisher.proceed('publish', 6, plan['plan_sha256'])
        with session() as publisher:
            status = publisher.status()
        check(status['version'] == 6 and status['pending'] is None and status['public_feed_matches']
              and (status['release_targets'], status['nightly_targets'], status['generation']) == (1, 2, 2),
              'Unexpected final feed: ' + json.dumps(status))
        pointer = json.loads(config.pointer.read_bytes())
        manifest = subprocess.check_output([str(inventory), str(config.root_path), pointer['bundle'],
                                            release.stamp(now)], env=release.TOOL_ENV, timeout=120)
        check(delivery.digest(manifest) == delivery.parse_state(feed.release)['current']['manifest_sha256'],
              'The pointer does not name the published bundle')

        config, feed = authority(directory / 'expired', signer, inventory, now, now - timedelta(hours=1))
        with Publisher(config, feed) as publisher:
            check(publisher.status()['hours_left'] < 0, 'The fixture feed has not expired')
        with Publisher(config, feed) as publisher:
            revived = publisher.renew('publish')
        with Publisher(config, feed) as publisher:
            check(publisher.status()['hours_left'] > 7 * 24 - 1, 'The renewal did not restore the timestamp')
        return dict(schema=1, scope='ephemeral keys and MOCK GitHub only',
                    operations=[dict(kind=r['kind'], version=r['version'], result=r.get('result'))
                                for r in (nightly_receipt, promotion, renewal, replanned, revived)],
                    abandoned_version=abandoned['version'], interrupted_publication_resumed=True,
                    expired_feed_renewed=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--signer', required=True)
    parser.add_argument('--inventory', required=True)
    args = parser.parse_args()
    print(json.dumps(run(args.signer, args.inventory), indent=2))
