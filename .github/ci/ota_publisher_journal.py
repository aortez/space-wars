"""Public publisher state; all mutations return a new checkpoint to persist.

The orchestrator must commit each result durably under the cross-channel writer
lock BEFORE exposing a proposal to a signer. No private keys live here. Tooling
is independently pinned publisher code, never supplied by the image candidate.
Missing state is an error; initialization requires an explicitly verified empty
genesis feed. This module does not enable a workflow or provision authority.
"""
import copy
import datetime as dt
import json
from pathlib import Path
import re
import subprocess
import tempfile

from ota_delivery import Bundle, digest, integer, sha, strict_json, valid_blob, valid_record

ROLES = ('targets', 'release', 'nightly', 'snapshot', 'timestamp')
MAX_BYTES = 16 * 1024 * 1024
MAX_METADATA = 256 * 1024
MAX_VERSION = 2 ** 64 - 1


def encode(value):
    data = json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()
    if len(data) > MAX_BYTES:
        raise ValueError('Publisher checkpoint exceeds its bound')
    return data


def timestamp(value):
    if not isinstance(value, str) or not re.fullmatch(r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z', value):
        raise ValueError('Expected whole-second UTC time')
    return dt.datetime.strptime(value, '%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=dt.timezone.utc)


def metadata(data):
    if not isinstance(data, str) or not 0 < len(data.encode()) <= MAX_METADATA:
        raise ValueError('Invalid retained metadata length')
    value = strict_json(data)
    if (not isinstance(value, dict) or not isinstance(value.get('signed'), dict)
            or not integer(value['signed'].get('version'))):
        raise ValueError('Invalid retained metadata')
    return value['signed']


def identity(image):
    # Cryptographic/image qualification stays with the Rust verifier. Preserve
    # its entire channel-neutral identity, not just the compressed digest.
    fields = {'schema', 'source_commit', 'build_id', 'machine', 'architecture', 'layout',
              'format', 'compressed', 'expanded', 'boot_identity', 'min_updater_protocol',
              'min_recovery_protocol', 'data'}
    if (not isinstance(image, dict) or set(image) != fields
            or not valid_blob(image['compressed']) or not valid_blob(image['expanded'], 3 * 1024 ** 3)
            or not sha(image['boot_identity']) or len(encode(image)) > 16384):
        raise ValueError('Expected a qualified channel-neutral image identity')
    return image['compressed']['sha256']


def intent(value):
    if not isinstance(value, dict):
        raise ValueError('Invalid publisher intent')
    kind = value.get('kind')
    if kind == 'renew' and set(value) == {'kind'}:
        return
    if (kind == 'nightly' and set(value) == {'kind', 'image', 'evidence_sha256'}
            and sha(value['evidence_sha256'])):
        identity(value['image'])
        return
    if (kind == 'release' and set(value) == {'kind', 'payload_sha256', 'notes', 'pi_qualification_sha256'}
            and sha(value['payload_sha256']) and sha(value['pi_qualification_sha256'])
            and isinstance(value['notes'], str) and value['notes'].strip()
            and len(value['notes'].encode()) <= 16384):
        return
    raise ValueError('Invalid publisher intent or missing Release evidence')


def load(data):
    if len(data) > MAX_BYTES:
        raise ValueError('Publisher checkpoint exceeds its bound')
    state = strict_json(data)
    required = {'schema', 'revision', 'root_sha256', 'versions', 'generation', 'images',
                'roles', 'current', 'pending'}
    if (not isinstance(state, dict) or set(state) != required or type(state['schema']) is not int
            or state['schema'] != 1 or not integer(state['revision']) or not sha(state['root_sha256'])
            or not integer(state['generation'], 0) or not isinstance(state['images'], dict)
            or not isinstance(state['versions'], dict) or set(state['versions']) != set(ROLES)
            or not all(integer(v) for v in state['versions'].values())
            or not isinstance(state['roles'], dict) or set(state['roles']) != set(ROLES)
            or not valid_record(state['current'])):
        raise ValueError('Invalid publisher checkpoint')
    for role in ROLES:
        if metadata(state['roles'][role])['version'] > state['versions'][role]:
            raise ValueError('Retained role exceeds reserved high-water mark')
    raw = state['roles']['timestamp'].encode()
    if (state['current']['version'] != metadata(state['roles']['timestamp'])['version']
            or state['current']['timestamp'] != {'bytes': len(raw), 'sha256': digest(raw)}):
        raise ValueError('Current publication differs from retained timestamp')
    generations = set()
    for payload, entry in state['images'].items():
        if (not isinstance(entry, dict)
                or set(entry) != {'image', 'generation', 'evidence_sha256', 'nightly', 'release'}
                or payload != identity(entry['image']) or not integer(entry['generation'])
                or entry['generation'] > state['generation'] or entry['generation'] in generations
                or not sha(entry['evidence_sha256'])):
            raise ValueError('Invalid or reused image generation')
        generations.add(entry['generation'])
        for channel in ('nightly', 'release'):
            publication = entry[channel]
            if publication is not None:
                if (not isinstance(publication, dict) or set(publication) != {'at', 'record'}
                        or not valid_record(publication['record'])
                        or publication['record']['version'] > state['current']['version']):
                    raise ValueError('Invalid image publication history')
                timestamp(publication['at'])
        if entry['release'] is not None and entry['nightly'] is None:
            raise ValueError('Release has no successful Nightly publication')
    pending = state['pending']
    if pending is not None:
        if (not isinstance(pending, dict)
                or set(pending) != {'intent', 'prepared_at', 'versions', 'generation', 'stages', 'publication'}
                or pending['versions'] != state['versions'] or not isinstance(pending['stages'], dict)
                or not set(pending['stages']) <= set(ROLES)):
            raise ValueError('Invalid pending publication')
        intent(pending['intent'])
        timestamp(pending['prepared_at'])
        for role in ROLES:
            if pending['versions'][role] <= metadata(state['roles'][role])['version']:
                raise ValueError('Pending versions do not advance the published roles')
        kind = pending['intent']['kind']
        if kind == 'renew':
            if pending['generation'] is not None:
                raise ValueError('Renewal cannot allocate an image generation')
        else:
            payload = (identity(pending['intent']['image']) if kind == 'nightly'
                       else pending['intent']['payload_sha256'])
            entry = state['images'].get(payload)
            if (entry is None or entry['generation'] != pending['generation']
                    or (kind == 'nightly' and entry['image'] != pending['intent']['image'])
                    or (kind == 'release' and entry['nightly'] is None)):
                raise ValueError('Pending image differs from reserved history')
        for role, stage in pending['stages'].items():
            if (not isinstance(stage, dict) or set(stage) != {'request', 'proposal', 'signed'}
                    or stage['request'] != request(state, role)
                    or not isinstance(stage['proposal'], str)
                    or not 0 < len(stage['proposal'].encode()) <= MAX_METADATA * 2):
                raise ValueError('Retained signing stage differs from reservation')
            strict_json(stage['proposal'])
            if stage['signed'] is not None:
                if metadata(stage['signed'])['version'] != pending['versions'][role]:
                    raise ValueError('Signed role differs from reserved version')
        if pending['publication'] is not None:
            publication_record(state, pending['publication'])
    return state


def changed(state):
    state['revision'] += 1
    return load(encode(state))


def initialize(root, roles, verified_record):
    """Explicit bootstrap, after feed_inventory verifies an EMPTY genesis feed."""
    if set(roles) != set(ROLES) or any(metadata(roles[c]).get('targets') != {} for c in ('release', 'nightly')):
        raise ValueError('Initialization requires an empty verified genesis feed')
    return load(encode(dict(schema=1, revision=1, root_sha256=digest(root),
                            versions={r: metadata(roles[r])['version'] for r in ROLES},
                            generation=0, images={}, roles=roles, current=verified_record, pending=None)))


def reserve(state, selected, prepared_at):
    state = load(encode(state))
    intent(selected)
    timestamp(prepared_at)
    if state['pending'] is not None:
        if state['pending']['intent'] != selected:
            raise ValueError('Reconcile the pending publication before admitting another')
        return state  # Preserve preparation time, versions and every exact signature.
    generation = None
    if selected['kind'] == 'nightly':
        payload = identity(selected['image'])
        known = state['images'].get(payload)
        if known and known['image'] != selected['image']:
            raise ValueError('An existing payload cannot change immutable identity')
        if known is None:
            state['generation'] += 1
            known = dict(image=copy.deepcopy(selected['image']), generation=state['generation'],
                         evidence_sha256=selected['evidence_sha256'], nightly=None, release=None)
            state['images'][payload] = known
        generation = known['generation']
    elif selected['kind'] == 'release':
        known = state['images'].get(selected['payload_sha256'])
        if known is None or known['nightly'] is None:
            raise ValueError('Release requires an already published Nightly image')
        generation = known['generation']
    state['versions'] = {r: v + 1 for r, v in state['versions'].items()}
    state['pending'] = dict(intent=copy.deepcopy(selected), prepared_at=prepared_at,
                            versions=state['versions'].copy(), generation=generation, stages={}, publication=None)
    return changed(state)


def signed(state, role):
    stage = state['pending']['stages'].get(role)
    if stage is None or stage['signed'] is None:
        raise ValueError('A prerequisite signed role is not retained')
    return stage['signed']


def request(state, role):
    pending = state['pending']
    if pending is None or role not in ROLES:
        raise ValueError('No reserved signing operation')
    prior = state['roles']
    if role == 'targets':
        change = {'role': 'targets-renewal'}
    elif role in ('release', 'nightly'):
        signed(state, 'targets')
        image = None
        if pending['intent']['kind'] == role:
            payload = (identity(pending['intent']['image']) if role == 'nightly'
                       else pending['intent']['payload_sha256'])
            image = dict(state['images'][payload]['image'], generation=pending['generation'], channel=role)
        change = dict(role='channel', channel=role, previous=prior[role], image=image)
    elif role == 'snapshot':
        change = dict(role=role, previous=prior[role],
                      **{r: signed(state, r) for r in ('targets', 'release', 'nightly')})
    else:
        change = dict(role=role, previous=prior[role], snapshot=signed(state, 'snapshot'))
    return dict(prepared_at=pending['prepared_at'], version=pending['versions'][role], change=change)


def prepare(state, role, tooling):
    state = load(encode(state))
    selected = request(state, role)
    if role in state['pending']['stages']:
        return state
    authority = state['roles']['targets'] if role == 'targets' else signed(state, 'targets')
    proposal = tooling.preview(state['root_sha256'], authority, encode(selected)).decode()
    state['pending']['stages'][role] = dict(request=selected, proposal=proposal, signed=None)
    return changed(state)


def retain(state, role, response, tooling):
    state = load(encode(state))
    stage = state['pending']['stages'][role]
    raw = response.decode()
    if stage['signed'] is not None:
        if raw != stage['signed']:
            raise ValueError('Cannot replace retained signature bytes')
        return state
    authority = state['roles']['targets'] if role == 'targets' else signed(state, 'targets')
    tooling.verify(state['root_sha256'], authority, encode(stage['request']),
                   digest(stage['proposal'].encode()), response)
    stage['signed'] = raw
    return changed(state)


def publication_record(state, manifest):
    if not isinstance(manifest, str):
        raise ValueError('Invalid retained publication manifest')
    raw = manifest.encode()
    bundle = Bundle('.', raw, digest(raw))  # Structural checks; no filesystem access.
    value = strict_json(raw)
    if (value['trusted_root_sha256'] != state['root_sha256']
            or bundle.record['version'] != state['pending']['versions']['timestamp']):
        raise ValueError('Publication manifest differs from reserved authority/version')
    expected = {}
    for role in ROLES:
        data = signed(state, role).encode()
        version = state['pending']['versions'][role]
        name = 'timestamp.json' if role == 'timestamp' else f'{version}.{role}.json'
        expected[name] = dict(bytes=len(data), sha256=digest(data))
    for channel in ('release', 'nightly'):
        for name, target in metadata(signed(state, channel))['targets'].items():
            hashed = target['hashes']['sha256']
            expected[hashed + '.' + name] = dict(bytes=target['length'], sha256=hashed)
    roots = {name: blob for name, blob in bundle.files.items() if name.endswith('.root.json')}
    if not any(blob['sha256'] == state['root_sha256'] for blob in roots.values()):
        raise ValueError('Publication manifest omits the independently trusted root')
    if bundle.files != {**expected, **roots}:
        raise ValueError('Publication manifest differs from exact retained roles and payloads')
    return bundle.record


def seal(state, verified_manifest):
    """Persist the exact feed_inventory output before any delivery mutation."""
    state = load(encode(state))
    raw = verified_manifest.decode()
    publication_record(state, raw)
    prior = state['pending']['publication']
    if prior is not None:
        if prior != raw:
            raise ValueError('Cannot replace an already sealed publication manifest')
        return state
    state['pending']['publication'] = raw
    return changed(state)


def confirm(state, verified_record, observed_record, published_at):
    """After inventory + delivery + fresh public readback, under the writer lock."""
    state = load(encode(state))
    pending = state['pending']
    if pending is None or not valid_record(verified_record) or observed_record != verified_record:
        raise ValueError('Publication has not been verified at the public feed')
    if publication_record(state, pending['publication']) != verified_record:
        raise ValueError('Public observation differs from the durably sealed manifest')
    raw = signed(state, 'timestamp').encode()
    if (verified_record['version'] != pending['versions']['timestamp']
            or verified_record['timestamp'] != {'bytes': len(raw), 'sha256': digest(raw)}
            or timestamp(published_at) < timestamp(pending['prepared_at'])):
        raise ValueError('Published timestamp differs from the reserved transaction')
    state['roles'] = {r: signed(state, r) for r in ROLES}
    kind = pending['intent']['kind']
    if kind != 'renew':
        payload = (identity(pending['intent']['image']) if kind == 'nightly'
                   else pending['intent']['payload_sha256'])
        state['images'][payload][kind] = dict(at=published_at, record=verified_record)
    state['current'] = copy.deepcopy(verified_record)
    state['pending'] = None
    return changed(state)


def abandon(state, observed_record):
    """Only after old signer/delivery jobs are quiescent under the shared lock."""
    state = load(encode(state))
    if state['pending'] is None or observed_record != state['current']:
        raise ValueError('Reconcile an advanced or unknown public timestamp first')
    state['pending'] = None
    return changed(state)  # All version and generation reservations survive.


class Tooling:
    """Fixed trusted signer binary; these calls never receive private keys."""
    def __init__(self, signer, root):
        self.signer = str(Path(signer).resolve(strict=True))
        self.root = root

    def run(self, root_hash, authority, request_bytes, operation, response=None):
        if digest(self.root) != root_hash:
            raise ValueError('Journal root differs from independently selected authority')
        with tempfile.TemporaryDirectory(prefix='ota-role-') as scratch:
            directory = Path(scratch)
            for name, data in (('root', self.root), ('targets', authority.encode()), ('request', request_bytes)):
                (directory / name).write_bytes(data)
            arguments = [self.signer, *(str(directory / n) for n in ('root', 'targets', 'request')), *operation]
            if response is not None:
                (directory / 'response').write_bytes(response)
                arguments.append(str(directory / 'response'))
            result = subprocess.run(arguments, check=True, capture_output=True, timeout=120,
                                    env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'})
            if len(result.stdout) > MAX_METADATA * 2:
                raise ValueError('Trusted signer output exceeds its bound')
            return result.stdout

    def preview(self, root_hash, authority, request_bytes):
        return self.run(root_hash, authority, request_bytes, ['preview'])

    def verify(self, root_hash, authority, request_bytes, approved, response):
        result = strict_json(self.run(root_hash, authority, request_bytes, ['verify', approved], response))
        if result != dict(schema=1, proposal_sha256=approved, signed_sha256=digest(response)):
            raise ValueError('Trusted signer receipt differs from retained response')
