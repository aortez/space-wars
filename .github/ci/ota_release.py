"""Owner-local OTA feed publisher: Nightly publication, Release promotion and renewal.

One command replaces the per-generation publication drivers and the renewal
script. Every operation re-signs all five online roles one version higher under
the shared publisher lock, checkpoints the journal around each signer call,
builds a bundle whose payloads are hard links to the previous bundle's, checks
it with feed_inventory and delivers it timestamp-last. A Nightly publication or
Release promotion first saves a plan; sign and publish require that plan's
digest, so the owner approves exactly what is signed. A renewal changes no
targets and needs no plan.

  ota_release.py status
  ota_release.py renew sign|publish
  ota_release.py nightly CANDIDATE_DIR
  ota_release.py promote --payload SHA256 --qualification RECORD --notes FILE
  ota_release.py sign VERSION --plan-sha256 SHA256
  ota_release.py publish VERSION --plan-sha256 SHA256
  ota_release.py abandon

Paths and the digests of the trusted tools and of the reviewed code come from
a local configuration file, ~/.config/spacewars-ota/release.json unless
--config names another; see docs/ota-updates.md#publication. Private keys never
leave the authority.
"""
import argparse
from datetime import datetime, timedelta, timezone
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

import ota_delivery as delivery
import ota_publisher_journal as journal

DEFAULT_CONFIG = Path.home() / '.config/spacewars-ota/release.json'
# The configuration pins these files, and a plan binds them, so only reviewed code signs.
CODE = ('ota_release.py', 'ota_publisher_journal.py', 'ota_delivery.py')
TOOLS = ('feed_sign', 'feed_inventory', 'image_candidate')
TOOL_ENV = {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'}
CHANNELS = ('release', 'nightly')
# The signer's lifetimes; each signed role must expire exactly this long after preparation.
LIFETIME = dict(timestamp=timedelta(days=7), snapshot=timedelta(days=30), targets=timedelta(days=30),
                release=timedelta(days=30), nightly=timedelta(days=30))
# A pending operation resumes only while its timestamp keeps a day of validity.
RESUME_MARGIN = timedelta(days=1)
# The signer accepts a proposal for 24 hours after its preparation; stop a little short of that.
SIGNING_WINDOW = timedelta(hours=24) - timedelta(minutes=10)
# Private key files per role, as ota_authority.py names them.
MEMBERS = dict(targets=['targets'], release=['release-a', 'release-b'], nightly=['nightly'],
               snapshot=['snapshot'], timestamp=['timestamp'])
RENEW = {'kind': 'renew'}
TIME_FORMAT = '%Y-%m-%dT%H:%M:%SZ'
COMMIT = re.compile(r'[0-9a-f]{40}\Z')
CI_RUN = re.compile(r'https://github\.com/' + re.escape(delivery.REPOSITORY) + r'/actions/runs/([0-9]+)\Z')
CI_FIELDS = 'status,conclusion,event,headBranch,headSha,url'


class Refused(ValueError):
    """A pinned tool refused its input; the message is the tool's own."""


def require(condition, message):
    if not condition:
        raise ValueError(message)


def refusal(name, error):
    detail = (error.stderr or b'').decode(errors='replace').strip() or f'exit status {error.returncode}'
    return Refused(f'{name} refused: {detail}')


def signing(step):
    """Runs a journal step that calls feed_sign, raising the signer's own refusal."""
    try:
        return step()
    except subprocess.CalledProcessError as error:
        raise refusal('feed_sign', error) from None


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def stamp(moment):
    return moment.strftime(TIME_FORMAT)


def parse(value):
    return datetime.strptime(value, TIME_FORMAT).replace(tzinfo=timezone.utc)


def progress(message):
    print(message, file=sys.stderr, flush=True)


def sync(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def retain(path, raw):
    """Writes evidence once; an existing file must already hold the same bytes."""
    if path.exists() or path.is_symlink():
        require(path.is_file() and not path.is_symlink() and path.read_bytes() == raw,
                'Retained evidence differs: ' + str(path))
        return
    with open(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600), 'wb') as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    sync(path.parent)


def replace(path, raw, unchanged=None):
    """Atomically replaces a file with flushed bytes, after an optional last check."""
    with tempfile.NamedTemporaryFile(prefix='.' + path.name + '-', dir=path.parent, delete=False) as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    if unchanged:
        unchanged()
    os.replace(stream.name, path)
    sync(path.parent)


def file_blob(path):
    hashed, size = hashlib.sha256(), 0
    with open(os.open(path, os.O_RDONLY | os.O_NOFOLLOW), 'rb') as stream:
        while chunk := stream.read(1 << 20):
            hashed.update(chunk)
            size += len(chunk)
    return dict(bytes=size, sha256=hashed.hexdigest())


def link(source, target, blob):
    """Hard-links one immutable payload after checking its bytes; never copies it."""
    require(source.is_file() and not source.is_symlink() and file_blob(source) == blob,
            'Payload differs: ' + str(source))
    if target.exists() or target.is_symlink():
        require(target.is_file() and not target.is_symlink() and os.path.samefile(source, target),
                'Bundle payload differs: ' + target.name)
        return
    os.link(source, target)
    sync(target.parent)


def code_digests():
    """Digests of the code that runs: this command and the two modules it imported."""
    files = {'ota_release.py': __file__, 'ota_publisher_journal.py': journal.__file__,
             'ota_delivery.py': delivery.__file__}
    return {name: digest(Path(path).read_bytes()) for name, path in files.items()}


def targets(raw):
    return journal.metadata(raw)['targets']


def payload_of(intent):
    return journal.identity(intent['image']) if intent['kind'] == 'nightly' else intent['payload_sha256']


def newest_source(state):
    """A real commit for delivery, which names one only if the feed release had to be created."""
    images = sorted(state['images'].values(), key=lambda entry: entry['generation'])
    return images[-1]['image']['source_commit'] if images else '0' * 40


def check_targets(old, new, intent, generation):
    """Only the intended channel change: none, one new Nightly image, or one promoted image."""
    kind = intent['kind']
    if kind == 'renew':
        require(new == old, 'A renewal changed targets')
    elif kind == 'nightly':
        image = intent['image']
        name = 'nightly.' + image['compressed']['sha256'] + '.ext4.zst'
        require(new['release'] == old['release'], 'Release targets changed')
        require(name not in old['nightly'] and set(new['nightly']) == set(old['nightly']) | {name},
                'Nightly must gain exactly the selected image')
        require(all(new['nightly'][key] == value for key, value in old['nightly'].items()),
                'An existing Nightly target changed')
        expected = dict(length=image['compressed']['bytes'], hashes=dict(sha256=image['compressed']['sha256']),
                        custom=dict(spacewars=dict(image, channel='nightly', generation=generation)))
        require(new['nightly'][name] == expected, 'The new Nightly target differs from the selected image')
    else:
        payload = intent['payload_sha256']
        source = old['nightly'].get('nightly.' + payload + '.ext4.zst')
        require(source is not None, 'The promoted image is not a Nightly target')
        promoted = dict(source, custom=dict(spacewars=dict(source['custom']['spacewars'], channel='release')))
        require(new['nightly'] == old['nightly'], 'Nightly targets changed')
        require(new['release'] == dict(old['release'], **{'release.' + payload + '.ext4.zst': promoted}),
                'Release must gain exactly the promoted Nightly image')


def check_signed(state, base):
    """Every role one version higher with the signer's exact lifetime, and the intended targets."""
    pending = state['pending']
    prepared = parse(pending['prepared_at'])
    for role in journal.ROLES:
        signed = journal.metadata(journal.signed(state, role))
        require(signed['version'] == pending['versions'][role] > journal.metadata(base['roles'][role])['version'],
                'Unexpected version: ' + role)
        require(parse(signed['expires']) == prepared + LIFETIME[role], 'Unexpected expiry: ' + role)
    check_targets({c: targets(base['roles'][c]) for c in CHANNELS},
                  {c: targets(journal.signed(state, c)) for c in CHANNELS}, pending['intent'], pending['generation'])


def check_resumable(pending, moment):
    expires = parse(pending['prepared_at']) + LIFETIME['timestamp']
    require(expires - moment > RESUME_MARGIN, 'The pending ' + pending['intent']['kind'] + ' operation from '
            + pending['prepared_at'] + ' is too old to publish; abandon it first')


def check_signable(pending, moment):
    """Roles still unsigned can be signed only within the signer's window after the reservation."""
    unsigned = [r for r in journal.ROLES if pending['stages'].get(r, {}).get('signed') is None]
    require(not unsigned or moment - parse(pending['prepared_at']) < SIGNING_WINDOW,
            f"The pending {pending['intent']['kind']} operation from {pending['prepared_at']} still has unsigned "
            'roles, and the signer accepts them only for 24 hours; abandon it and start again')


def phase(state, base, plan):
    """Where a planned operation stands: 'initial', 'pending' or 'complete'. Anything else rejects."""
    pending, intent = state['pending'], plan['intent']
    if pending is None:
        if state == base:
            return 'initial'
        entry = state['images'].get(payload_of(intent))
        published = entry and entry[intent['kind']]
        require(published and published['record']['version'] == plan['version'],
                'Publisher changed since this operation was prepared')
        return 'complete'
    require(pending['intent'] == intent and pending['versions'] == {r: plan['version'] for r in journal.ROLES}
            and state['current'] == base['current'] and state['roles'] == base['roles'],
            'Another publisher operation is pending')
    return 'pending'


def pins(value, names):
    return isinstance(value, dict) and set(value) == set(names) and all(delivery.sha(v) for v in value.values())


class Config:
    """Local paths and the digests of the trusted tools and the reviewed code, kept outside the repository."""
    FIELDS = {'schema', 'authority', 'workspace', 'root_sha256', 'tools_directory', 'tools', 'code'}

    def __init__(self, value):
        require(isinstance(value, dict) and set(value) == self.FIELDS and value['schema'] == 1
                and delivery.sha(value['root_sha256']) and pins(value['tools'], TOOLS) and pins(value['code'], CODE),
                'Invalid release configuration')
        self.authority, self.workspace, self.tools_directory = (
            Path(value[key]) for key in ('authority', 'workspace', 'tools_directory'))
        require(all(path.is_absolute() for path in (self.authority, self.workspace, self.tools_directory)),
                'Configured paths must be absolute')
        self.root_sha256, self.tools, self.code = value['root_sha256'], dict(value['tools']), dict(value['code'])
        self.public = self.authority / 'public'
        self.keys = self.authority / 'keys'
        self.state_path = self.public / 'publisher-state.json'
        self.root_path = self.public / 'genesis/1.root.json'
        self.lock_path = self.authority.parent / 'publication.lock'
        self.operations = self.workspace / 'feed'
        # The older renewal script and drivers find the published bundle here too.
        self.pointer = self.workspace / 'renewal/latest-bundle.json'

    @classmethod
    def load(cls, path):
        info = path.lstat()
        require(stat.S_ISREG(info.st_mode) and info.st_uid == os.geteuid() and not info.st_mode & 0o022,
                'The release configuration must be a regular file you own that others cannot write: ' + str(path))
        return cls(json.loads(path.read_bytes()))


class ReadOnlyGitHub(delivery.GitHub):
    def request(self, method, *args, **kwargs):
        require(method == 'GET', 'This phase cannot change GitHub')
        return super().request(method, *args, **kwargs)


class Publisher:
    """One session under the shared publisher lock. Tests replace github(), gh_json() and now()."""

    def __init__(self, config):
        self.config = config

    def github(self, mutating):
        token = subprocess.check_output(['gh', 'auth', 'token'], timeout=60).decode().strip()
        return (delivery.GitHub if mutating else ReadOnlyGitHub)(token)

    def gh_json(self, args):
        return json.loads(subprocess.check_output(['gh', *args], timeout=120))

    def now(self):
        return datetime.now(timezone.utc).replace(microsecond=0)

    def tool(self, name):
        path = self.config.tools_directory / name
        require(path.is_file() and not path.is_symlink() and file_blob(path)['sha256'] == self.config.tools[name],
                'Trusted executable changed: ' + name)
        return str(path)

    def run(self, name, *args, timeout=120):
        """Runs a pinned tool and returns its output; a refusal raises the tool's own message."""
        try:
            return subprocess.run([self.tool(name), *(str(a) for a in args)], check=True, capture_output=True,
                                  timeout=timeout, env=TOOL_ENV).stdout
        except subprocess.CalledProcessError as error:
            raise refusal(name, error) from None

    def __enter__(self):
        self.umask = os.umask(0o077)
        self.lock = None
        try:
            running = code_digests()
            changed = [name for name in CODE if running[name] != self.config.code[name]]
            require(not changed, 'Not the reviewed code pinned in the release configuration: '
                    + ', '.join(f'{name} is {running[name]}' for name in changed))
            for path in (self.config.authority, self.config.public, self.config.keys):
                info = path.lstat()
                require(stat.S_ISDIR(info.st_mode) and info.st_uid == os.geteuid()
                        and stat.S_IMODE(info.st_mode) == 0o700, 'Unexpected authority directory ownership/mode')
            self.root = self.config.root_path.read_bytes()
            require(digest(self.root) == self.config.root_sha256, 'Trusted root changed')
            self.lock = os.open(self.config.lock_path, os.O_RDWR | os.O_NOFOLLOW)
            try:
                fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                raise ValueError('Another publisher operation holds the lock') from None
            self.state_raw = self.config.state_path.read_bytes()
            self.state = journal.load(self.state_raw)
            require(self.state['root_sha256'] == self.config.root_sha256, 'The journal names another root')
        except BaseException:
            self.__exit__()
            raise
        return self

    def __exit__(self, *details):
        if self.lock is not None:
            os.close(self.lock)
            self.lock = None
        os.umask(self.umask)

    def unchanged(self):
        require(self.config.state_path.read_bytes() == self.state_raw, 'Another publisher changed the journal')

    def advance(self, updated, history):
        """Persists the next journal checkpoint, keeping both sides of the transition as evidence."""
        raw = journal.encode(updated)
        if raw == journal.encode(self.state):
            return
        require(updated['revision'] == self.state['revision'] + 1, 'Unexpected checkpoint revision')
        self.unchanged()
        history.mkdir(mode=0o700, exist_ok=True)
        for value, data in ((self.state, self.state_raw), (updated, raw)):
            retain(history / f"{value['revision']}-{digest(data)}.json", data)
        replace(self.config.state_path, raw, self.unchanged)
        self.state_raw = self.config.state_path.read_bytes()
        require(self.state_raw == raw, 'Checkpoint readback differs')
        self.state = journal.load(raw)

    def observe(self, api):
        release = api.get_release()
        require(release is not None, 'The feed release is missing')
        return release, delivery.parse_state(release)

    def exposed(self, api, release, observed):
        """Whether devices can already fetch the pending publication's timestamp, so it can only be finished."""
        pending = self.state['pending']
        sealed = pending['publication'] and journal.publication_record(self.state, pending['publication'])
        require(observed['current'] in (self.state['current'], sealed) and observed['pending'] in (None, sealed),
                'The public feed differs from the journal')
        if not sealed:
            return False
        if observed['current'] == sealed:
            return True
        if observed['pending'] != sealed:
            return False
        return delivery.known_timestamp(api, delivery.asset_map(api, release).get('timestamp.json'), observed) == sealed

    def finishing(self):
        """Whether publish can only finish the pending publication, because devices can already fetch it."""
        api = self.github(mutating=False)
        release, observed = self.observe(api)
        return self.exposed(api, release, observed)

    def finish_command(self):
        pending = self.state['pending']
        kind, version = pending['intent']['kind'], pending['versions']['timestamp']
        if kind == 'renew':
            return 'ota_release.py renew publish'
        plan = self.config.operations / f'v{version}-{kind}' / 'plan.json'
        if plan.is_file():
            return f'ota_release.py publish {version} --plan-sha256 {digest(plan.read_bytes())}'
        return 'the tool that started it'

    def alias_holds(self, api, release, blob):
        """Whether timestamp.json serves exactly these bytes."""
        alias = delivery.asset_map(api, release).get('timestamp.json')
        if alias is None or alias['state'] != 'uploaded' or alias['size'] != blob['bytes']:
            return False
        try:
            api.verify(alias, blob)
        except ValueError:
            return False
        return True

    def restore_alias(self, api):
        """Puts the current timestamp back at timestamp.json when an abandoned attempt left none there."""
        release = api.get_release()
        raw = self.state['roles']['timestamp'].encode()
        blob = dict(bytes=len(raw), sha256=digest(raw))
        if self.alias_holds(api, release, blob):
            return False
        alias = delivery.asset_map(api, release).get('timestamp.json')
        if alias is not None:
            require(alias['state'] == 'starter' and alias['size'] == 0,
                    'timestamp.json holds bytes of no reserved publication')
            api.delete_asset(alias)
        uploaded = api.upload(release, 'timestamp.json', io.BytesIO(raw), len(raw))
        require(uploaded.get('name') == 'timestamp.json' and uploaded.get('state') == 'uploaded'
                and uploaded.get('size') == len(raw), 'Upload response differs from the restored timestamp')
        api.verify(uploaded, blob)
        return True

    def previous_bundle(self, override=None):
        """Finds the local bundle of the published feed and authenticates it.

        An expired feed is checked at the last second its timestamp was valid, so it
        can still be renewed. Every new bundle is checked at the current time.
        """
        current = self.state['current']
        expires = parse(journal.metadata(self.state['roles']['timestamp'])['expires'])
        at = stamp(min(self.now(), expires - timedelta(seconds=1)))
        if override:
            candidates = [Path(override).resolve()]
        else:
            candidates = sorted(self.config.operations.glob(f"v{current['version']}-*/bundle"))
            if self.config.pointer.exists():
                candidates.append(Path(json.loads(self.config.pointer.read_bytes())['bundle']))
        rejected = []
        for path in candidates:
            if not path.is_dir():
                rejected.append(f'{path}: missing')
                continue
            try:
                manifest = self.run('feed_inventory', self.config.root_path, path, at)
            except Refused as error:
                rejected.append(f'{path}: {error}')
                continue
            if digest(manifest) == current['manifest_sha256']:
                return path, manifest
            rejected.append(f'{path}: another feed version')
        raise ValueError('No local bundle of the published feed; pass --previous-bundle. Tried: '
                         + ('; '.join(rejected) or 'nothing'))

    def begin(self, kind, previous_override):
        """Checks the public feed against the previous bundle, then opens the next version's directory."""
        require(self.state['pending'] is None, 'Finish or abandon the pending publisher operation first')
        versions = set(self.state['versions'].values())
        require(len(versions) == 1, 'Role versions are not in lockstep')
        version = versions.pop() + 1
        api = self.github(mutating=False)
        release, observed = self.observe(api)
        require(observed == dict(schema=1, current=self.state['current'], pending=None),
                'The public feed differs from the journal')
        previous, manifest = self.previous_bundle(previous_override)
        old = delivery.Bundle(previous, manifest, self.state['current']['manifest_sha256'])
        require(old.record == self.state['current'], 'Previous bundle does not match the journal')
        assets = delivery.asset_map(api, release)
        for name, blob in old.files.items():
            if name == 'timestamp.json' and name not in assets:
                # An attempt stopped during the timestamp swap can leave no alias. The archived copy proves the
                # current timestamp, and this operation publishes a new alias.
                name = old.archive_name
            require(name in assets and assets[name]['size'] == blob['bytes']
                    and assets[name].get('digest') == 'sha256:' + blob['sha256'], 'Remote asset differs: ' + name)
            if name.endswith('.json'):
                api.verify(assets[name], blob)
        self.unchanged()
        operation = self.config.operations / f'v{version}-{kind}'
        self.config.operations.mkdir(mode=0o700, exist_ok=True)
        # No version above the journal's has been reserved, so an existing directory holds only an
        # unsigned plan or a renewal that stopped before reserving; retain() checks it is the same.
        operation.mkdir(mode=0o700, exist_ok=True)
        if (operation / 'plan.json').exists():
            raise ValueError(f'An unsigned plan for v{version} is waiting in {operation}; sign it, '
                             'or delete that directory to plan again')
        retain(operation / 'base-state.json', self.state_raw)
        retain(operation / 'previous-inventory.json', manifest)
        retain(operation / 'previous-bundle.txt', str(previous).encode())
        return version, operation, previous

    def sign_roles(self, evidence, history):
        tooling = journal.Tooling(self.tool('feed_sign'), self.root)
        for role in journal.ROLES:
            self.advance(signing(lambda: journal.prepare(self.state, role, tooling)), history)
            stage = self.state['pending']['stages'][role]
            if stage['signed'] is not None:
                continue
            request, authority = evidence / (role + '-request.json'), evidence / (role + '-authority.json')
            retain(request, journal.encode(stage['request']))
            retain(authority, (self.state['roles']['targets'] if role == 'targets'
                               else journal.signed(self.state, 'targets')).encode())
            retain(evidence / (role + '-proposal.json'), stage['proposal'].encode())
            keys = [self.config.keys / (name + '.pkcs8') for name in MEMBERS[role]]
            for key in keys:
                info = key.lstat()
                require(stat.S_ISREG(info.st_mode) and info.st_uid == os.geteuid()
                        and stat.S_IMODE(info.st_mode) == 0o600 and info.st_nlink == 1, 'Unsafe private signing file')
            signed = self.run('feed_sign', self.config.root_path, authority, request,
                              digest(stage['proposal'].encode()), *keys)
            self.advance(signing(lambda: journal.retain(self.state, role, signed, tooling)), history)
            progress(role + ': signature verified and retained')

    def build_bundle(self, directory, previous, intent, image):
        """Lays out the signed roles and hard-links every payload; a new Nightly payload comes from its candidate."""
        directory.mkdir(mode=0o700, exist_ok=True)
        retain(directory / '1.root.json', self.root)
        pending = self.state['pending']
        for role in journal.ROLES:
            name = 'timestamp.json' if role == 'timestamp' else f"{pending['versions'][role]}.{role}.json"
            retain(directory / name, journal.signed(self.state, role).encode())
        for channel in CHANNELS:
            for name, value in targets(journal.signed(self.state, channel)).items():
                hashed = value['hashes']['sha256']
                if intent['kind'] == 'nightly' and hashed == payload_of(intent):
                    source = image
                elif intent['kind'] == 'release' and channel == 'release' and hashed == intent['payload_sha256']:
                    source = previous / f'{hashed}.nightly.{hashed}.ext4.zst'  # The same bytes, Nightly's name.
                else:
                    source = previous / (hashed + '.' + name)
                link(source, directory / (hashed + '.' + name), dict(bytes=value['length'], sha256=hashed))
        return directory

    def point(self, record, bundle):
        """Names the published bundle, which the next operation starts from."""
        if (self.config.pointer.exists()
                and json.loads(self.config.pointer.read_bytes()).get('manifest_sha256') == record['manifest_sha256']):
            return
        self.config.pointer.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        value = dict(schema=1, version=record['version'], manifest_sha256=record['manifest_sha256'],
                     bundle=str(bundle), written_by='ota_release.py publish at ' + stamp(self.now()))
        replace(self.config.pointer, (json.dumps(value, indent=2) + '\n').encode())

    def execute(self, action, operation, evidence, intent, base, previous, source, generation=None, image=None,
                extra=None):
        """Reserves or resumes, signs, checks and seals one operation; publish also delivers it.

        A sealed publication whose timestamp devices can already fetch is finished as sealed, at any age.
        """
        evidence.mkdir(mode=0o700, exist_ok=True)
        sync(self.config.public)
        history = evidence / 'checkpoints'
        api = self.github(mutating=action == 'publish')
        release, observed = self.observe(api)
        if self.state['pending'] is None:
            require(observed == dict(schema=1, current=self.state['current'], pending=None),
                    'The public feed differs from the journal')
            self.advance(journal.reserve(self.state, intent, stamp(self.now())), history)
        elif not self.exposed(api, release, observed):
            check_resumable(self.state['pending'], self.now())
        pending = self.state['pending']
        require(pending['generation'] == generation, 'The reserved generation differs from the plan')
        retain(evidence / 'intent.json', journal.encode(intent))
        directory = operation / 'bundle'
        if pending['publication'] is None:
            check_signable(pending, self.now())
            self.sign_roles(evidence, history)
            check_signed(self.state, base)
            self.build_bundle(directory, previous, intent, image)
            inventory = self.run('feed_inventory', self.config.root_path, directory, stamp(self.now()))
            retain(evidence / 'inventory.json', inventory)
            self.advance(journal.seal(self.state, inventory), history)
            pending = self.state['pending']
        manifest = pending['publication'].encode()
        bundle = delivery.Bundle(directory, manifest, digest(manifest))
        bundle.verify()
        receipt = dict(kind=intent['kind'], version=pending['versions']['timestamp'], current=bundle.record,
                       prepared_at=pending['prepared_at'],
                       expires={r: journal.metadata(journal.signed(self.state, r))['expires']
                                for r in ('timestamp', 'snapshot')}, **(extra or {}))
        if action == 'sign':
            receipt = dict(receipt, signed=True, published=False)
            retain(operation / 'signing-result.json', journal.encode(receipt))
            return receipt
        result = delivery.publish(api, bundle, source)
        release, observed = self.observe(api)
        require(observed == dict(schema=1, current=bundle.record, pending=None), 'Public feed readback differs')
        # delivery.publish downloaded and checked every asset; reread the metadata and both timestamp names.
        assets = delivery.asset_map(api, release)
        for name, blob in bundle.files.items():
            if name.endswith('.json'):
                api.verify(assets[name], blob)
        api.verify(assets[bundle.archive_name], bundle.files['timestamp.json'])
        self.advance(journal.confirm(self.state, bundle.record, observed['current'], stamp(self.now())), history)
        receipt = dict(receipt, schema=1, signed=True, published=True, result=result,
                       release_url=release.get('html_url'), root_sha256=self.config.root_sha256, device_accessed=False)
        retain(evidence / 'publication.json', journal.encode(receipt))
        retain(operation / 'publication-result.json', journal.encode(receipt))
        self.point(bundle.record, directory)
        return receipt

    def status(self):
        api = self.github(mutating=False)
        release, observed = self.observe(api)
        roles = {r: journal.metadata(self.state['roles'][r]) for r in journal.ROLES}
        expires = parse(roles['timestamp']['expires'])
        pending, current = self.state['pending'], self.state['current']
        assets = len(api.assets(release))
        return dict(version=current['version'], timestamp_expires=roles['timestamp']['expires'],
                    hours_left=round((expires - self.now()).total_seconds() / 3600, 1),
                    other_roles_expire=sorted({roles[r]['expires'] for r in journal.ROLES if r != 'timestamp'}),
                    release_targets=len(roles['release']['targets']), nightly_targets=len(roles['nightly']['targets']),
                    generation=self.state['generation'],
                    pending=pending and dict(kind=pending['intent']['kind'], prepared_at=pending['prepared_at'],
                                             sealed=pending['publication'] is not None),
                    public_feed_matches=(observed == dict(schema=1, current=current, pending=None)
                                         and self.alias_holds(api, release, current['timestamp'])),
                    release_assets=assets, asset_headroom=delivery.MAX_ASSETS - assets)

    def renew(self, action, previous_override=None):
        pending = self.state['pending']
        if pending is None:
            version, operation, previous = self.begin('renew', previous_override)
            base = self.state
        else:
            require(pending['intent'] == RENEW, 'Another publisher operation is pending; finish or abandon it')
            version = pending['versions']['timestamp']
            operation = self.config.operations / f'v{version}-renew'
            require((operation / 'base-state.json').is_file(),
                    'The pending renewal was started by another tool; finish it with that tool')
            base = journal.load((operation / 'base-state.json').read_bytes())
            require(base['current'] == self.state['current'] and base['roles'] == self.state['roles'],
                    'The pending renewal does not continue this run')
            previous = Path((operation / 'previous-bundle.txt').read_text())
        return self.execute(action, operation, self.config.public / f'renew-v{version}', RENEW, base, previous,
                            newest_source(self.state))

    def select_nightly(self, candidate):
        """Checks a prepared candidate directory; returns its intent, selection record and payload path."""
        root = Path(candidate).resolve(strict=True)
        raw = (root / 'preparation-result.json').read_bytes()
        report = json.loads(raw)
        source = report['source']
        require(isinstance(source, str) and COMMIT.fullmatch(source), 'Invalid preparation source')
        for name in ('policy.json', 'main-ci-run.json', 'candidate/candidate.json', 'candidate/qualification.json',
                     'candidate/image.ext4.zst'):
            require(name in report['artifacts'], 'The preparation record does not pin ' + name)
        for name, expected in report['artifacts'].items():
            path = root / name
            require(path.is_file() and not path.is_symlink(), 'Missing preparation artifact: ' + name)
            require(file_blob(path) == expected, 'Preparation artifact changed: ' + name)
        files = root / 'candidate'
        with tempfile.TemporaryDirectory(prefix='verify-', dir=self.config.workspace) as scratch:
            verified = json.loads(self.run('image_candidate', 'verify', root / 'policy.json', files / 'candidate.json',
                                           files / 'image.ext4.zst', scratch, timeout=600))
        evidence = (files / 'qualification.json').read_bytes()
        require(verified == json.loads(evidence), 'Independent image verification differs')
        image = verified['image']
        require(image['source'] == dict(schema=1, commit=source, dirty=False, revision=source[:12]),
                'Wrong embedded source')
        descriptor = json.loads((files / 'candidate.json').read_bytes())
        require(descriptor == report['descriptor'], 'Payload differs from the preparation record')
        run = CI_RUN.fullmatch(str(json.loads((root / 'main-ci-run.json').read_bytes()).get('url')))
        require(run, 'Unexpected CI run record')
        ci = self.gh_json(['run', 'view', run.group(1), '--repo', delivery.REPOSITORY, '--json', CI_FIELDS])
        require(ci.get('status') == 'completed' and ci.get('conclusion') == 'success' and ci.get('event') == 'push'
                and ci.get('headBranch') == 'main' and ci.get('headSha') == source,
                'The selected source has not passed CI on main')
        intent = dict(kind='nightly', image=dict(schema=1, source_commit=source, build_id=image['build_id'],
            **{k: image[k] for k in ('machine', 'architecture', 'layout', 'boot_identity', 'data')},
            **{k: descriptor[k] for k in ('format', 'compressed', 'expanded')},
            min_updater_protocol=image['updater_protocol'], min_recovery_protocol=image['recovery_protocol']),
            evidence_sha256=digest(evidence))
        journal.intent(intent)
        return intent, dict(candidate=str(root), preparation_sha256=digest(raw), ci=ci), files / 'image.ext4.zst'

    def select_release(self, payload, qualification, notes):
        """Checks the Pi qualification record of a published Nightly image; returns intent, selection, record."""
        entry = self.state['images'].get(payload)
        require(entry is not None and entry['nightly'] is not None, 'Only a published Nightly image can be promoted')
        path = Path(qualification).resolve(strict=True)
        require(path.is_file(), 'The qualification record must be a file')
        evidence = path.read_bytes()
        record = json.loads(evidence)
        named = record.get('image') if isinstance(record, dict) else None
        require(isinstance(named, dict) and record.get('purpose') == 'release-qualification'
                and named == dict(entry['image'], generation=entry['generation']),
                'The qualification record names another image')
        intent = dict(kind='release', payload_sha256=payload, notes=notes, pi_qualification_sha256=digest(evidence))
        journal.intent(intent)
        selection = dict(qualification=dict(path=str(path), bytes=len(evidence), sha256=digest(evidence)),
                         nightly_publication=entry['nightly'])
        return intent, selection, evidence

    def plan(self, intent, selection, generation, previous_override, evidence=None):
        version, operation, previous = self.begin(intent['kind'], previous_override)
        before = {c: len(targets(self.state['roles'][c])) for c in CHANNELS}
        after = {c: count + (c == intent['kind']) for c, count in before.items()}
        value = dict(schema=1, kind=intent['kind'], version=version, generation=generation, intent=intent,
                     intent_sha256=digest(journal.encode(intent)), selection=selection,
                     base_state_sha256=digest(self.state_raw), base_revision=self.state['revision'],
                     existing_publication=self.state['current'], root_sha256=self.config.root_sha256,
                     previous_bundle=str(previous), bundle=str(operation / 'bundle'), targets_before=before,
                     targets_after=after, code=code_digests(), tools=self.config.tools, prepared_at=stamp(self.now()),
                     signed=False, published=False, device_accessed=False)
        raw = journal.encode(value)
        retain(operation / 'plan.json', raw)
        if evidence is not None:
            retain(operation / 'qualification.json', evidence)
        plan_sha256 = digest(raw)
        return dict(prepared=True, kind=intent['kind'], version=version, generation=generation,
                    payload=payload_of(intent), targets_before=before, targets_after=after,
                    plan=str(operation / 'plan.json'), plan_sha256=plan_sha256,
                    next=f'ota_release.py sign {version} --plan-sha256 {plan_sha256}')

    def nightly(self, candidate, previous_override=None):
        intent, selection, _ = self.select_nightly(candidate)
        known = self.state['images'].get(payload_of(intent))
        require(known is None or known['nightly'] is None, 'This image is already on Nightly')
        # A reservation that was abandoned keeps the generation it allocated.
        generation = known['generation'] if known else self.state['generation'] + 1
        return self.plan(intent, selection, generation, previous_override)

    def promote(self, payload, qualification, notes, previous_override=None):
        intent, selection, evidence = self.select_release(payload, qualification,
                                                          Path(notes).read_text(encoding='utf-8').strip())
        require(self.state['images'][payload]['release'] is None, 'This image is already on Release')
        return self.plan(intent, selection, self.state['images'][payload]['generation'], previous_override, evidence)

    def reselect(self, plan):
        """Repeats the selection checks; their inputs must not have changed since the plan."""
        intent = plan['intent']
        if intent['kind'] == 'nightly':
            selected, selection, image = self.select_nightly(plan['selection']['candidate'])
            require(selected == intent and selection['preparation_sha256'] == plan['selection']['preparation_sha256'],
                    'The candidate changed since the plan was prepared')
            return image, None
        selected, selection, evidence = self.select_release(
            intent['payload_sha256'], plan['selection']['qualification']['path'], intent['notes'])
        require(selected == intent and selection['nightly_publication'] == plan['selection']['nightly_publication'],
                'The promoted image or its qualification changed since the plan was prepared')
        return None, evidence

    def proceed(self, action, version, plan_sha256):
        """Signs or publishes the plan the owner approved by its digest."""
        found = []
        for path in sorted(self.config.operations.glob(f'v{version}-*/plan.json')):
            raw = path.read_bytes()
            if digest(raw) == plan_sha256:
                found.append((path.parent, raw))
        require(found, f'No plan for v{version} has that digest')
        operation, raw = found[0]  # The approved bytes themselves; the file is not read again.
        plan = json.loads(raw)
        require(plan['version'] == version and operation.name == f"v{version}-{plan['kind']}", 'Misplaced plan')
        base_raw = (operation / 'base-state.json').read_bytes()
        require(digest(base_raw) == plan['base_state_sha256'], 'The baseline record changed')
        base = journal.load(base_raw)
        stage = phase(self.state, base, plan)
        intent = plan['intent']
        if stage == 'complete':
            record = self.state['images'][payload_of(intent)][intent['kind']]['record']
            if action == 'publish' and record == self.state['current']:
                self.point(record, Path(plan['bundle']))
            return dict(already_published=True, kind=intent['kind'], version=version, generation=plan['generation'],
                        current=record)
        image = evidence = None
        # A publication devices can already fetch is only finished; nothing about it is decided again.
        if not (stage == 'pending' and action == 'publish' and self.finishing()):
            require(plan['code'] == code_digests(), 'The release command changed since this plan was prepared')
            require(plan['tools'] == self.config.tools and plan['root_sha256'] == self.config.root_sha256,
                    'Trusted tools or root changed since this plan was prepared')
            image, evidence = self.reselect(plan)
            require(action == 'sign' or stage == 'pending', 'Sign the plan before publishing it')
        folder = self.config.public / f"{intent['kind']}-v{version}"
        folder.mkdir(mode=0o700, exist_ok=True)
        retain(folder / 'plan.json', raw)
        if evidence is not None:
            retain(folder / 'qualification.json', evidence)
        source = (self.state['images'][intent['payload_sha256']]['image'] if intent['kind'] == 'release'
                  else intent['image'])['source_commit']
        return self.execute(action, operation, folder, intent, base, Path(plan['previous_bundle']),
                            source, plan['generation'], image,
                            dict(generation=plan['generation'], plan_sha256=plan_sha256))

    def abandon(self):
        """Drops a pending operation that never became public; its reserved versions stay used."""
        pending = self.state['pending']
        require(pending is not None, 'No publisher operation is pending')
        kind, version = pending['intent']['kind'], pending['versions']['timestamp']
        current = self.state['current']
        api = self.github(mutating=pending['publication'] is not None)
        release, observed = self.observe(api)
        if self.exposed(api, release, observed):
            raise ValueError('That publication is already public, so it cannot be abandoned; finish it with '
                             + self.finish_command())
        restored = False
        if pending['publication'] is not None:
            sealed = journal.publication_record(self.state, pending['publication'])
            if observed['pending'] is not None and delivery.abandon(api, current, sealed) != 'abandoned':
                raise ValueError('That publication became public; finish it with ' + self.finish_command())
            # An attempt stopped during the timestamp swap can leave no alias; the current timestamp goes back.
            restored = self.restore_alias(api)
        _, observed = self.observe(api)
        require(observed == dict(schema=1, current=current, pending=None), 'The public feed differs from the journal')
        evidence = self.config.public / f'{kind}-v{version}'
        evidence.mkdir(mode=0o700, exist_ok=True)
        self.advance(journal.abandon(self.state, observed['current']), evidence / 'checkpoints')
        return dict(abandoned=True, kind=kind, version=version, prepared_at=pending['prepared_at'], current=current,
                    timestamp_restored=restored)

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--config', type=Path, default=DEFAULT_CONFIG)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('status', help='versions, expiry, pending work and release capacity; read-only')
    renew = commands.add_parser('renew', help='re-sign every role with unchanged targets')
    renew.add_argument('action', choices=('sign', 'publish'))
    nightly = commands.add_parser('nightly', help='plan the Nightly publication of a prepared candidate')
    nightly.add_argument('candidate', type=Path)
    promote = commands.add_parser('promote', help='plan the Release promotion of a published Nightly image')
    promote.add_argument('--payload', required=True, help='compressed SHA-256 of the published Nightly image')
    promote.add_argument('--qualification', type=Path, required=True, help='its release-qualification record')
    promote.add_argument('--notes', type=Path, required=True, help='file holding the release notes')
    for command in (renew, nightly, promote):
        command.add_argument('--previous-bundle', type=Path, help='bundle of the published feed, when none is recorded')
    for name in ('sign', 'publish'):
        command = commands.add_parser(name, help=name + ' a prepared plan')
        command.add_argument('version', type=int)
        command.add_argument('--plan-sha256', required=True)
    commands.add_parser('abandon', help='drop a pending operation that is not public; its versions stay used')
    args = parser.parse_args(argv)
    with Publisher(Config.load(args.config)) as publisher:
        if args.command == 'status':
            result = publisher.status()
        elif args.command == 'renew':
            result = publisher.renew(args.action, args.previous_bundle)
        elif args.command == 'nightly':
            result = publisher.nightly(args.candidate, args.previous_bundle)
        elif args.command == 'promote':
            result = publisher.promote(args.payload, args.qualification, args.notes, args.previous_bundle)
        elif args.command == 'abandon':
            result = publisher.abandon()
        else:
            result = publisher.proceed(args.command, args.version, args.plan_sha256)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
