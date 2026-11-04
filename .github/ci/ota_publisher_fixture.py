"""End-to-end LOCAL publisher fixture, with disposable keys and mocked GitHub.

Never use this module to bootstrap production authority. It validates the real
signer/inventory tools across the journal and delivery adapters. Optional image
input must already have passed independent image qualification; supplying it
here does not establish CI provenance or physical Pi rollout evidence.
"""
import argparse
import datetime as dt
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, NoEncryption, PrivateFormat

import ota_delivery as delivery
import ota_publisher_checkpoint as checkpoint
import ota_publisher_journal as journal
from test_ota_delivery import FakeGitHub
from test_ota_publisher_checkpoint import FakeGit
from test_ota_publisher_journal import nightly


def run(signer, inventory, candidate=None):
    signer, inventory = (str(Path(p).resolve(strict=True)) for p in (signer, inventory))
    now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
    stamp = now.strftime('%Y-%m-%dT%H:%M:%SZ')
    expires = lambda days: (now + dt.timedelta(days=days)).strftime('%Y-%m-%dT%H:%M:%SZ')
    key_roles = ('root', 'targets', 'snapshot', 'timestamp', 'release-a', 'release-b', 'nightly')
    keys = {r: Ed25519PrivateKey.generate() for r in key_roles}
    public = {r: dict(keytype='ed25519', scheme='ed25519', keyval=dict(public=k.public_key().public_bytes_raw().hex()))
              for r, k in keys.items()}
    ids = {r: delivery.digest(journal.encode(k)) for r, k in public.items()}
    members = {r: [r] for r in ('root', 'targets', 'snapshot', 'timestamp', 'nightly')}
    members['release'] = ['release-a', 'release-b']

    def signed(body, role):
        raw = journal.encode(body)
        signatures = sorted([dict(keyid=ids[k], sig=keys[k].sign(raw).hex()) for k in members[role]],
                            key=lambda value: value['keyid'])
        return journal.encode(dict(signed=body, signatures=signatures))

    def base(role):
        return dict(_type=role, spec_version='1.0.0', version=1, expires=expires(7))

    def meta(raw):
        return dict(version=1, length=len(raw), hashes=dict(sha256=delivery.digest(raw)))

    root = signed(dict(base('root'), expires=expires(365), consistent_snapshot=True,
                       keys={ids[r]: public[r] for r in ('root', 'targets', 'snapshot', 'timestamp')},
                       roles={r: dict(keyids=[ids[r]], threshold=1)
                              for r in ('root', 'targets', 'snapshot', 'timestamp')}), 'root')
    delegations = dict(keys={ids[k]: public[k] for r in ('release', 'nightly') for k in members[r]},
                       roles=[dict(name=r, keyids=[ids[k] for k in members[r]], threshold=len(members[r]),
                                   paths=[r + '.*'], terminating=True) for r in ('release', 'nightly')])
    roles = {r: signed(dict(base('targets'), targets={}), r) for r in ('release', 'nightly')}
    roles['targets'] = signed(dict(base('targets'), targets={}, delegations=delegations), 'targets')
    roles['snapshot'] = signed(dict(base('snapshot'), meta={r + '.json': meta(roles[r])
                                  for r in ('targets', 'release', 'nightly')}), 'snapshot')
    roles['timestamp'] = signed(dict(base('timestamp'), meta={'snapshot.json': meta(roles['snapshot'])}), 'timestamp')
    roles = {r: b.decode() for r, b in roles.items()}
    with tempfile.TemporaryDirectory(prefix='ota-publisher-fixture-') as scratch:
        directory = Path(scratch)
        root_file = directory / 'root.json'
        root_file.write_bytes(root)
        key_files = {}
        for role, key in keys.items():
            path = directory / (role + '.pkcs8')
            with open(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb') as output:
                output.write(key.private_bytes(Encoding.DER, PrivateFormat.PKCS8, NoEncryption()))
            key_files[role] = str(path)
        tooling = journal.Tooling(signer, root)
        payload = directory / 'payload'
        selected = nightly()
        if candidate is None:
            payload.write_bytes(b'first image')
            qualification = 'synthetic payload; no rootfs qualification'
        else:
            candidate = Path(candidate)
            image = json.loads((candidate / 'qualification.json').read_bytes())['image']
            record = json.loads((candidate / 'candidate.json').read_bytes())
            selected = dict(kind='nightly', image=dict(schema=1,
                source_commit=image['source']['commit'], build_id=image['build_id'],
                **{k: image[k] for k in ('machine', 'architecture', 'layout', 'boot_identity', 'data')},
                **{k: record[k] for k in ('format', 'compressed', 'expanded')},
                min_updater_protocol=image['updater_protocol'], min_recovery_protocol=image['recovery_protocol']),
                evidence_sha256=delivery.digest((candidate / 'qualification.json').read_bytes()))
            shutil.copyfile(candidate / 'image.ext4.zst', payload)
            qualification = 'previously qualified rootfs; mocked origin and Pi evidence in this fixture'

        def bundle(values, name):
            target = directory / name
            target.mkdir()
            (target / '1.root.json').write_bytes(root)
            for role, raw in values.items():
                version = journal.metadata(raw)['version']
                (target / ('timestamp.json' if role == 'timestamp' else f'{version}.{role}.json')).write_text(raw)
            for channel in ('release', 'nightly'):
                for name, target_record in journal.metadata(values[channel])['targets'].items():
                    shutil.copyfile(payload, target / (target_record['hashes']['sha256'] + '.' + name))
            result = subprocess.run([inventory, str(root_file), str(target), stamp],
                                    capture_output=True, check=True, timeout=120,
                                    env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'})
            return delivery.Bundle(target, result.stdout, delivery.digest(result.stdout))

        initial = bundle(roles, 'genesis')
        api = FakeGitHub()
        delivery.publish(api, initial, 'a' * 40)
        state = journal.initialize(root, roles, initial.record)
        git = FakeGit()
        store = checkpoint.Store(git, delivery.digest(root))
        saved = store.create(state, delivery.digest(journal.encode(state)))
        saved = store.advance(saved, journal.reserve(saved.state, selected, stamp))
        saved = store.advance(saved, journal.abandon(saved.state, initial.record))
        transactions = [selected,
            dict(kind='release', payload_sha256=journal.identity(selected['image']),
                 notes='Ephemeral fixture promotion only; no physical rollout claim.',
                 pi_qualification_sha256=delivery.digest(b'mocked Pi qualification fixture')),
            {'kind': 'renew'}]
        observed = []
        for index, operation in enumerate(transactions):
            reserved = journal.reserve(saved.state, operation, stamp)
            # Lose the acknowledgement of one genuinely applied checkpoint.
            if index == 0:
                old_operation = git.operation

                def lost_reply(method, path, body):
                    result = old_operation(method, path, body)
                    if method == 'PATCH':
                        git.operation = old_operation
                        raise OSError('Fixture: lost successful checkpoint reply')
                    return result

                git.operation = lost_reply
                try:
                    store.advance(saved, reserved)
                except OSError:
                    pass
            saved = store.advance(saved, reserved)
            for role in journal.ROLES:
                saved = store.advance(saved, journal.prepare(saved.state, role, tooling))
                stage = saved.state['pending']['stages'][role]
                request_file = directory / 'request.json'
                request_file.write_bytes(journal.encode(stage['request']))
                authority_file = directory / 'targets.json'
                authority_file.write_text(saved.state['roles']['targets'] if role == 'targets'
                                          else journal.signed(saved.state, 'targets'))
                response = subprocess.run([signer, str(root_file), str(authority_file), str(request_file),
                    delivery.digest(stage['proposal'].encode()), *(key_files[k] for k in members[role])],
                    capture_output=True, check=True, timeout=120, env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'}).stdout
                saved = store.advance(saved, journal.retain(saved.state, role, response, tooling))
                if saved.raw != store.read().raw:
                    raise AssertionError('Retained signature did not survive checkpoint reload')
            publication = bundle({r: journal.signed(saved.state, r) for r in journal.ROLES}, f'publication-{index}')
            saved = store.advance(saved, journal.seal(saved.state, publication.manifest_bytes))
            delivery.publish(api, publication, 'a' * 40)
            remote = delivery.parse_state(api.get_release())['current']
            api.verify(delivery.asset_map(api, api.get_release())['timestamp.json'], remote['timestamp'])
            saved = store.advance(saved, journal.confirm(saved.state, publication.record, remote, stamp))
            observed.append(dict(kind=operation['kind'], timestamp=remote['version'], generation=saved.state['generation']))
        image = saved.state['images'][journal.identity(selected['image'])]
        assert image['generation'] == 1 and image['nightly'] and image['release']
        assert [item['timestamp'] for item in observed] == [3, 4, 5]
        return dict(schema=1, scope='ephemeral keys and MOCK GitHub only', image_scope=qualification,
                    compressed=selected['image']['compressed'], transactions=observed,
                    retained_checkpoints=len(git.commits), public_assets=len(api.files),
                    aborted_version=2, lost_checkpoint_reply_reconciled=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--signer', required=True)
    parser.add_argument('--inventory', required=True)
    parser.add_argument('--candidate')
    args = parser.parse_args()
    print(json.dumps(run(args.signer, args.inventory, args.candidate), indent=2))
