"""Create a private owner-controlled OTA authority and a verified empty feed.

Run once outside the source/build tree. Existing outputs are never overwritten,
including incomplete attempts. No network, GitHub publication or device changes.
Only public/ may be distributed; keys/ must stay with the owner. Co-located keys
are suitable for the supervised rollout, not isolated production signers.
"""
import argparse
import datetime as dt
import json
import os
from pathlib import Path
import stat
import subprocess

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, NoEncryption, PrivateFormat

import ota_delivery as delivery
import ota_publisher_journal as journal

REPO = Path(__file__).resolve().parents[2]
MEMBERS = dict(root=['root-a', 'root-b', 'root-c'], targets=['targets'],
               snapshot=['snapshot'], timestamp=['timestamp'],
               release=['release-a', 'release-b'], nightly=['nightly'])
THRESHOLDS = dict(root=2, targets=1, snapshot=1, timestamp=1, release=2, nightly=1)
FEED = f'https://github.com/{delivery.REPOSITORY}/releases/download/{delivery.TAG}/'


def save(path, data):
    with open(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600), 'wb') as out:
        out.write(data)
        out.flush()
        os.fsync(out.fileno())


def sync(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def create(output, inventory, inventory_sha256, now):
    output = Path(output)
    if not output.is_absolute() or output.name in ('', '.', '..'):
        raise ValueError('Choose a new absolute authority directory')
    parent = output.parent
    metadata = parent.lstat()
    resolved = parent.resolve()
    if (not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.geteuid()
            or metadata.st_mode & 0o022 or output.resolve().is_relative_to(REPO)
            or any((ancestor / '.git').exists() for ancestor in (resolved, *resolved.parents))):
        raise ValueError('Authority parent must be owner-controlled and outside the repository')
    now = journal.timestamp(now)
    stamp = now.strftime('%Y-%m-%dT%H:%M:%SZ')
    if not delivery.sha(inventory_sha256):
        raise ValueError('Select the verifier SHA-256 independently')
    verifier = os.open(inventory, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        with os.fdopen(os.dup(verifier), 'rb') as file:
            if (not stat.S_ISREG(os.fstat(file.fileno()).st_mode)
                    or delivery.digest(file.read()) != inventory_sha256):
                raise ValueError('Inventory verifier differs from selected bytes')
        # mkdir is exclusive even for an empty or interrupted existing output.
        output.mkdir(mode=0o700)
        sync(parent)
        keys_dir, public = output / 'keys', output / 'public'
        keys_dir.mkdir(mode=0o700)
        public.mkdir(mode=0o700)
        genesis = public / 'genesis'
        genesis.mkdir(mode=0o700)
        keys = {name: Ed25519PrivateKey.generate() for names in MEMBERS.values() for name in names}
        public_keys = {name: dict(keytype='ed25519', scheme='ed25519',
                                 keyval=dict(public=key.public_key().public_bytes_raw().hex()))
                       for name, key in keys.items()}
        ids = {name: delivery.digest(journal.encode(key)) for name, key in public_keys.items()}
        if len(set(ids.values())) != len(ids):
            raise ValueError('Signing roles must use distinct keys')
        for name, key in keys.items():
            save(keys_dir / (name + '.pkcs8'),
                 key.private_bytes(Encoding.DER, PrivateFormat.PKCS8, NoEncryption()))
        sync(keys_dir)

        def signed(body, role):
            raw = journal.encode(body)
            signatures = sorted([dict(keyid=ids[name], sig=keys[name].sign(raw).hex())
                                 for name in MEMBERS[role][:THRESHOLDS[role]]],
                                key=lambda value: value['keyid'])
            return journal.encode(dict(signed=body, signatures=signatures))

        def base(role, days):
            return dict(_type=role, spec_version='1.0.0', version=1,
                        expires=(now + dt.timedelta(days=days)).strftime('%Y-%m-%dT%H:%M:%SZ'))

        def reference(raw):
            return dict(version=1, length=len(raw), hashes=dict(sha256=delivery.digest(raw)))

        top = ('root', 'targets', 'snapshot', 'timestamp')
        root = signed(dict(base('root', 365), consistent_snapshot=True,
                           keys={ids[n]: public_keys[n] for role in top for n in MEMBERS[role]},
                           roles={role: dict(keyids=[ids[n] for n in MEMBERS[role]], threshold=THRESHOLDS[role])
                                  for role in top}), 'root')
        channels = ('release', 'nightly')
        delegation = dict(keys={ids[n]: public_keys[n] for role in channels for n in MEMBERS[role]},
                          roles=[dict(name=role, keyids=[ids[n] for n in MEMBERS[role]],
                                      threshold=THRESHOLDS[role], paths=[role + '.*'], terminating=True)
                                 for role in channels])
        roles = {role: signed(dict(base('targets', 30), targets={}), role) for role in channels}
        roles['targets'] = signed(dict(base('targets', 30), targets={}, delegations=delegation), 'targets')
        roles['snapshot'] = signed(dict(base('snapshot', 30),
                                       meta={role + '.json': reference(roles[role])
                                             for role in ('targets', 'release', 'nightly')}), 'snapshot')
        roles['timestamp'] = signed(dict(base('timestamp', 7),
                                        meta={'snapshot.json': reference(roles['snapshot'])}), 'timestamp')
        save(genesis / '1.root.json', root)
        for role, raw in roles.items():
            save(genesis / ('timestamp.json' if role == 'timestamp' else '1.' + role + '.json'), raw)
        sync(genesis)
        # The native TUF verifier checks signatures, namespace/threshold policy,
        # references and expiry without having access to private-key arguments.
        result = subprocess.run(['/proc/self/fd/' + str(verifier), str(genesis / '1.root.json'),
                                 str(genesis), stamp], pass_fds=(verifier,), stdin=subprocess.DEVNULL,
                                capture_output=True, timeout=120, env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'})
        if result.returncode:
            raise ValueError('Native genesis verification failed; incomplete authority retained')
        publication = delivery.Bundle(genesis, result.stdout, delivery.digest(result.stdout))
        publication.verify()
        if len(publication.files) != 6 or publication.record['version'] != 1:
            raise ValueError('Expected only six metadata files in the empty genesis feed')
        state = journal.initialize(root, {role: raw.decode() for role, raw in roles.items()}, publication.record)
        bootstrap = journal.encode(dict(schema=1, root=json.loads(root), location=dict(metadata=FEED, targets=FEED)))
        save(public / 'ota-bootstrap.json', bootstrap)
        save(public / 'genesis-inventory.json', result.stdout)
        save(public / 'publisher-state.json', journal.encode(state))
        report = dict(schema=1, created_utc=stamp, feed=FEED, root_sha256=delivery.digest(root),
                      bundle_sha256=delivery.digest(bootstrap), inventory_sha256=delivery.digest(result.stdout),
                      verifier_sha256=inventory_sha256,
                      roles={role: dict(keyids=[ids[n] for n in names], threshold=THRESHOLDS[role])
                             for role, names in MEMBERS.items()},
                      images=0, published=False, devices_provisioned=False,
                      scope='Owner-local authority; keys co-located, no isolated signing service or production rollout claim')
        save(public / 'authority.json', journal.encode(report))
        sync(public)
        sync(output)
        save(output / 'COMPLETE.json', journal.encode(dict(schema=1, authority_sha256=delivery.digest(journal.encode(report)))))
        sync(output)
        return report
    finally:
        os.close(verifier)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--inventory', required=True, type=Path)
    parser.add_argument('--inventory-sha256', required=True)
    args = parser.parse_args()
    report = create(args.output, args.inventory, args.inventory_sha256,
                    dt.datetime.now(dt.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'))
    print(json.dumps(report, indent=2))
