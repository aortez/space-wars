"""End-to-end LOCAL check of ota_release.py with disposable keys and a mocked GitHub.

Never use this module to bootstrap production authority. It builds a throwaway
authority, workspace and configuration, then drives the release command with
the real signer and inventory tools: Nightly publications, a Release promotion
and renewals; deliveries that lose a reply and resume, or that devices can
already see and are finished days later; abandoned operations, one of which
must put the timestamp alias back; a missing alias repaired by a renewal; and
the renewal of a feed whose timestamp has already expired. Only the image
verifier and the CI query are mocked; test_ota_release.py covers their checks.
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
from test_ota_release import CI_URL, prepared_candidate

TOP = ('root', 'targets', 'snapshot', 'timestamp')
MEMBERS = dict(release.MEMBERS, root=['root'])
# Old enough that a publication devices cannot see yet is refused, young enough that its timestamp has not expired.
LATE = timedelta(days=6, hours=12)


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
    """Publishes to the fake feed on a clock that can run ahead; image_candidate and CI are mocked."""

    def __init__(self, config, feed, ahead=timedelta()):
        super().__init__(config)
        self.feed, self.ahead = feed, ahead

    def github(self, mutating):
        self.feed.read_only = not mutating
        return self.feed

    def now(self):
        return super().now() + self.ahead

    def run(self, name, *args, timeout=120):
        if name != 'image_candidate':
            return super().run(name, *args, timeout=timeout)
        self.tool(name)
        return (Path(args[2]).parent / 'qualification.json').read_bytes()  # It accepts the candidate's own record.

    def gh_json(self, args):
        return dict(status='completed', conclusion='success', event='push', headBranch='main', headSha='a' * 40,
                    url=CI_URL)


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
    (tools / 'image_candidate').write_bytes(b'Never run: the fixture mocks the image verifier.\n')
    path = directory / 'release.json'
    write_private(path, json.dumps(dict(
        schema=1, authority=str(home), workspace=str(workspace), root_sha256=delivery.digest(root),
        tools_directory=str(tools), tools={name: release.file_blob(tools / name)['sha256'] for name in release.TOOLS},
        code=release.code_digests())).encode())
    return release.Config.load(path), feed


def lose_reply(feed, name, occurrence=1):
    """Applies the occurrence-th next call named `name`, then loses its reply."""
    seen = 0

    def hook(called):
        nonlocal seen
        if called == name:
            seen += 1
            if seen == occurrence:
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


def fails(action, text):
    """Runs an operation whose GitHub call fails part way, as a lost reply or a broken upload does."""
    try:
        action()
    except OSError as error:
        check(text in str(error), 'Unexpected failure: ' + str(error))
        return
    raise AssertionError('The interrupted operation finished')


def run(signer, inventory):
    signer, inventory = (Path(p).resolve(strict=True) for p in (signer, inventory))
    now = datetime.now(timezone.utc).replace(microsecond=0)
    with tempfile.TemporaryDirectory(prefix='ota-release-fixture-') as scratch:
        directory = Path(scratch)
        config, feed = authority(directory / 'current', signer, inventory, now, now + timedelta(days=7))
        receipts = []

        def call(action, *args, ahead=timedelta()):
            with Publisher(config, feed, ahead) as publisher:
                result = getattr(publisher, action)(*args)
            if result.get('published'):
                receipts.append(result)
            return result

        def public():
            return delivery.parse_state(feed.release)

        first, second = directory / 'first', directory / 'second'
        prepared_candidate(first, b'first fixture image')
        prepared_candidate(second, b'second fixture image')
        plan = call('nightly', first)
        check((plan['version'], plan['generation'], plan['targets_after']) == (2, 1, dict(release=0, nightly=1)),
              'Unexpected Nightly plan')
        digest = plan['plan_sha256']
        refuses(lambda: call('proceed', 'sign', 2, '0' * 64), 'No plan for v2 has that digest')
        refuses(lambda: call('proceed', 'publish', 2, digest), 'Sign the plan before publishing')
        with patch.object(release, 'code_digests', side_effect=[release.code_digests(), {}]):
            refuses(lambda: call('proceed', 'sign', 2, digest), 'release command changed')
        call('proceed', 'sign', 2, digest)
        check(public()['current']['version'] == 1, 'Signing changed the feed')

        # Devices can fetch the new timestamp once the alias is swapped, so even days later the
        # publication is finished, never abandoned.
        lose_reply(feed, 'upload:timestamp.json')
        fails(lambda: call('proceed', 'publish', 2, digest), 'lost the reply')
        check(public()['pending']['version'] == 2, 'The feed is not mid-publication')
        refuses(lambda: call('abandon', ahead=LATE), 'finish it with ota_release.py publish 2 --plan-sha256 ' + digest)
        check(call('proceed', 'publish', 2, digest, ahead=LATE)['result'] == 'published', 'The late finish failed')
        check(call('proceed', 'publish', 2, digest)['already_published'], 'A finished plan published again')

        with Publisher(config, feed) as publisher:
            payload = plan['payload']
            image = publisher.state['images'][payload]['image']
        record, notes = directory / 'qualification.json', directory / 'notes.txt'
        record.write_text(json.dumps(dict(purpose='release-qualification', image=dict(image, generation=1))))
        notes.write_text('Fixture promotion only; no physical rollout.\n')
        plan = call('promote', payload, record, notes)
        call('proceed', 'sign', 3, plan['plan_sha256'])
        lose_reply(feed, 'update', occurrence=2)  # The final release update, after the reservation.
        fails(lambda: call('proceed', 'publish', 3, plan['plan_sha256']), 'lost the reply')
        check(public()['pending'] is None and public()['current']['version'] == 3, 'The final update was not applied')
        check(call('proceed', 'publish', 3, plan['plan_sha256'])['result'] == 'unchanged', 'The promotion stalled')

        # A plan left unsigned neither blocks a renewal nor survives it.
        stale = call('nightly', second)
        check(call('renew', 'sign')['published'] is False, 'renew sign published')
        lose_reply(feed, 'upload:4.snapshot.json')
        fails(lambda: call('renew', 'publish'), 'lost the reply')
        call('renew', 'publish')
        refuses(lambda: call('proceed', 'sign', 4, stale['plan_sha256']), 'changed since this operation was prepared')

        plan = call('nightly', second)
        call('proceed', 'sign', 5, plan['plan_sha256'])
        check(call('abandon')['timestamp_restored'] is False, 'An undelivered abandon touched the alias')
        plan = call('nightly', second)
        check((plan['version'], plan['generation']) == (6, 2), 'The abandoned Nightly lost its generation')
        call('proceed', 'sign', 6, plan['plan_sha256'])
        call('proceed', 'publish', 6, plan['plan_sha256'])

        # A broken alias upload leaves an empty starter in place of timestamp.json. Days later the
        # attempt can no longer be published; abandoning it puts the current timestamp back.
        call('renew', 'sign')
        feed.starter = 'timestamp.json'
        fails(lambda: call('renew', 'publish'), 'empty starter')
        refuses(lambda: call('renew', 'publish', ahead=LATE), 'too old to publish; abandon it first')
        check(call('abandon', ahead=LATE)['timestamp_restored'], 'The alias was not restored')
        check(call('status')['public_feed_matches'], 'The restored feed does not match')
        call('renew', 'publish')

        # With no alias at all, the archived timestamp proves the feed and a renewal publishes a new alias.
        feed.files.pop(feed.named('timestamp.json')[0]['id'])
        check(not call('status')['public_feed_matches'], 'status missed the missing alias')
        call('renew', 'publish')
        status = call('status')
        check(status['version'] == 9 and status['pending'] is None and status['public_feed_matches']
              and (status['release_targets'], status['nightly_targets'], status['generation']) == (1, 2, 2),
              'Unexpected final feed: ' + json.dumps(status))
        pointer = json.loads(config.pointer.read_bytes())
        manifest = subprocess.check_output([str(inventory), str(config.root_path), pointer['bundle'],
                                            release.stamp(now)], env=release.TOOL_ENV, timeout=120)
        check(delivery.digest(manifest) == public()['current']['manifest_sha256'],
              'The pointer does not name the published bundle')
        unreviewed = release.Config(dict(json.loads((directory / 'current/release.json').read_bytes()),
                                         code={name: '0' * 64 for name in release.CODE}))
        refuses(lambda: Publisher(unreviewed, feed).__enter__(), 'Not the reviewed code')

        config, feed = authority(directory / 'expired', signer, inventory, now, now - timedelta(hours=1))
        check(call('status')['hours_left'] < 0, 'The fixture feed has not expired')
        call('renew', 'publish')
        check(call('status')['hours_left'] > 7 * 24 - 1, 'The renewal did not restore the timestamp')
        return dict(schema=1, scope='ephemeral keys and MOCK GitHub only',
                    operations=[dict(kind=r['kind'], version=r['version'], result=r['result']) for r in receipts],
                    abandoned_versions=[5, 7], late_visible_publication_finished=True, alias_restored=True,
                    missing_alias_repaired=True, expired_feed_renewed=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--signer', required=True)
    parser.add_argument('--inventory', required=True)
    args = parser.parse_args()
    print(json.dumps(run(args.signer, args.inventory), indent=2))
