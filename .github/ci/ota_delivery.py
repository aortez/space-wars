"""Timestamp-last delivery of an already authorized, verified OTA publication.

This is a library, not a signing/approval command or an enabled workflow. Its
caller must run the pinned offline verifier, admit the exact manifest digest
through the build/promotion policy, and serialize ALL writers to ota-feed (one
shared workflow concurrency group). The release reservation detects stale and
interrupted attempts; GitHub's API does not provide a compare-and-swap lock.

No credentials or paths from candidate artifacts are executed. The only mutable
asset is timestamp.json. All history stays in place; retention is a separate
operation and a full release fails without evicting live or historic assets.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import urllib.error
import urllib.parse
import urllib.request

REPOSITORY = 'aortez/space-wars'
TAG = 'ota-feed'
TITLE = 'Space Wars OTA feed'
MARKER = '<!-- spacewars-ota-delivery-v1\n'
MAX_ASSETS = 1000
MAX_ASSET_BYTES = 2 * 1024 ** 3 - 1
MAX_METADATA_BYTES = 256 * 1024
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_VERSION = 2 ** 64 - 1
CHUNK = 64 * 1024
HEX = re.compile(r'[0-9a-f]{64}\Z')
METADATA = re.compile(r'(?:[1-9][0-9]*\.(?:root|snapshot|targets|release|nightly)|timestamp)\.json\Z')
TARGET = re.compile(r'([0-9a-f]{64})\.(?:release|nightly)\.\1\.ext4\.zst\Z')


def digest(data):
    return hashlib.sha256(data).hexdigest()


def integer(value, lower=1, upper=MAX_VERSION):
    return type(value) is int and lower <= value <= upper


def sha(value):
    return isinstance(value, str) and HEX.fullmatch(value) is not None


def strict_json(data):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError('Duplicate JSON field')
            result[key] = value
        return result
    return json.loads(data, object_pairs_hook=pairs)


def valid_blob(blob, limit=MAX_ASSET_BYTES):
    return (isinstance(blob, dict) and set(blob) == {'bytes', 'sha256'}
            and integer(blob['bytes'], upper=limit) and sha(blob['sha256']))


def verify_stream(stream, blob):
    remaining = blob['bytes']
    hashed = hashlib.sha256()
    while remaining:
        chunk = stream.read(min(CHUNK, remaining))
        if not chunk or len(chunk) > remaining:
            raise ValueError('Asset length differs from verified manifest')
        remaining -= len(chunk)
        hashed.update(chunk)
    if stream.read(1) or hashed.hexdigest() != blob['sha256']:
        raise ValueError('Asset bytes differ from verified manifest')


class Bundle:
    """Exact output of feed_inventory plus its flat local artifact directory.

    expected_sha256 is selected by the trusted caller, not read from the bundle.
    This only preserves that selection; it does not mint a build authorization.
    """
    def __init__(self, directory, manifest_bytes, expected_sha256):
        if (len(manifest_bytes) > MAX_MANIFEST_BYTES or not sha(expected_sha256)
                or digest(manifest_bytes) != expected_sha256):
            raise ValueError('Manifest differs from admitted digest')
        manifest = strict_json(manifest_bytes)
        if (not isinstance(manifest, dict)
                or set(manifest) != {'schema', 'trusted_root_sha256', 'timestamp_version', 'files'}
                or type(manifest['schema']) is not int or manifest['schema'] != 1
                or not sha(manifest['trusted_root_sha256'])
                or not integer(manifest['timestamp_version'])
                or not isinstance(manifest['files'], dict)
                or not 6 <= len(manifest['files']) <= MAX_ASSETS):
            raise ValueError('Invalid delivery manifest')
        for name, blob in manifest['files'].items():
            metadata, target = METADATA.fullmatch(name), TARGET.fullmatch(name)
            if (not (metadata or target) or not valid_blob(blob)
                    or (metadata and blob['bytes'] > MAX_METADATA_BYTES)
                    or (target and target.group(1) != blob['sha256'])):
                raise ValueError('Invalid delivery asset')
        if 'timestamp.json' not in manifest['files']:
            raise ValueError('Missing timestamp')
        self.manifest_bytes = bytes(manifest_bytes)
        self.directory = Path(directory)
        self.files = manifest['files']
        self.record = dict(manifest_sha256=expected_sha256,
                           version=manifest['timestamp_version'],
                           timestamp=self.files['timestamp.json'])
        self.archive_name = f"timestamp-{self.record['version']}-{self.record['timestamp']['sha256']}.json"

    def open(self, name):
        if name not in self.files:
            raise ValueError('Asset is outside the manifest')
        flags = os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)
        descriptor = os.open(self.directory / name, flags)
        stream = os.fdopen(descriptor, 'rb')
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            stream.close()
            raise ValueError('Asset is not a regular file')
        return stream

    def verify(self):
        if {entry.name for entry in self.directory.iterdir()} != set(self.files):
            raise ValueError('Publication directory differs from manifest')
        for name, blob in self.files.items():
            with self.open(name) as stream:
                verify_stream(stream, blob)
        with self.open('timestamp.json') as stream:
            timestamp = strict_json(stream.read(MAX_METADATA_BYTES + 1))
        if timestamp['signed']['version'] != self.record['version']:
            raise ValueError('Timestamp version differs from manifest')


def valid_record(record):
    return (isinstance(record, dict)
            and set(record) == {'manifest_sha256', 'version', 'timestamp'}
            and sha(record['manifest_sha256']) and integer(record['version'])
            and valid_blob(record['timestamp'], MAX_METADATA_BYTES))


def body(state):
    return ('Device OTA metadata and immutable images. Managed by the isolated OTA publisher.\n\n'
            + MARKER + json.dumps(state, sort_keys=True) + '\n-->')


def parse_state(release):
    if (not integer(release.get('id'))
            or release.get('tag_name') != TAG or release.get('name') != TITLE
            or release.get('draft') is not False or release.get('prerelease') is not True):
        raise ValueError('Conflicting OTA release')
    content = release.get('body') or ''
    if len(content) > 16384 or content.count(MARKER) != 1 or not content.endswith('\n-->'):
        raise ValueError('Unmanaged OTA release')
    state = strict_json(content.split(MARKER)[1][:-4])
    if (not isinstance(state, dict) or set(state) != {'schema', 'current', 'pending'}
            or type(state['schema']) is not int or state['schema'] != 1
            or any(item is not None and not valid_record(item)
                   for item in (state['current'], state['pending']))
            or (state['current'] is None and state['pending'] is None)):
        raise ValueError('Invalid publication reservation')
    current, pending = state['current'], state['pending']
    if current and pending and (pending['version'] < current['version']
                               or (pending['version'] == current['version'] and pending != current)):
        raise ValueError('Conflicting publication reservation')
    return state


def asset_map(api, release):
    result = {}
    ids = set()
    for asset in api.assets(release):
        if (not isinstance(asset, dict) or not integer(asset.get('id'))
                or not isinstance(asset.get('name'), str) or not asset['name']
                or asset.get('state') not in ('uploaded', 'starter')
                or not integer(asset.get('size'), 0, MAX_ASSET_BYTES)
                or asset['name'] in result or asset['id'] in ids
                or len(result) >= MAX_ASSETS):
            raise ValueError('Invalid, duplicate or over-capacity release assets')
        result[asset['name']] = asset
        ids.add(asset['id'])
    return result


def reservation(api, expected):
    release = api.get_release()
    if release is None or parse_state(release) != expected:
        raise ValueError('Publication reservation changed; stop this writer')
    return release


def known_timestamp(api, asset, state):
    if asset is None:
        return None
    # A failed alias upload may leave a zero-byte starter after the old alias
    # was removed. No valid metadata can be an empty starter.
    if asset['state'] == 'starter' and asset['size'] == 0 and state['pending']:
        return None
    for record in (state['pending'], state['current']):
        if record and asset['size'] == record['timestamp']['bytes']:
            try:
                api.verify(asset, record['timestamp'])
                return record
            except ValueError:
                pass
    raise ValueError('Current timestamp differs from every reserved publication')


def ensure(api, release, assets, name, bundle, source_name):
    expected = bundle.files[source_name]
    existing = assets.get(name)
    if existing and existing['state'] == 'starter' and existing['size'] == 0:
        # Only unfinished empty uploads are replaceable. An uploaded immutable
        # object with different bytes is evidence of a conflict, never garbage.
        api.delete_asset(existing)
        existing = None
    if existing is None:
        with bundle.open(source_name) as stream:
            existing = api.upload(release, name, stream, expected['bytes'])
        if (existing.get('name') != name or not integer(existing.get('id'))
                or existing.get('state') != 'uploaded'
                or existing.get('size') != expected['bytes']):
            raise ValueError('Upload response differs from requested asset')
        assets[name] = existing
    api.verify(existing, expected)


def publish(api, bundle, publisher_commit):
    """Deliver under the caller's exclusive, cross-channel publication lock."""
    if (api.repo != REPOSITORY or api.tag != TAG
            or not re.fullmatch('[0-9a-f]{40}', publisher_commit)):
        raise ValueError('Unexpected publisher destination or source')
    bundle.verify()  # No network mutation until all local bytes agree.
    candidate = bundle.record
    release = api.get_release()
    state = parse_state(release) if release else dict(schema=1, current=None, pending=None)
    for record in (state['current'], state['pending']):
        if record and (record['version'] > candidate['version']
                       or (record['version'] == candidate['version'] and record != candidate)):
            raise ValueError('Stale or conflicting publication')
    if state['pending'] is not None and state['pending'] != candidate:
        raise ValueError('Resume the pending publication before admitting another')
    assets = asset_map(api, release) if release else {}
    required = set(bundle.files) | {bundle.archive_name}
    if len(set(assets) | required) > MAX_ASSETS:
        raise ValueError('OTA release capacity exhausted; no assets were evicted')
    known_timestamp(api, assets.get('timestamp.json'), state)
    unchanged = (state['current'] == candidate and state['pending'] is None
                 and required <= set(assets)
                 and all(assets[name]['state'] == 'uploaded' for name in required))
    if not unchanged:
        state['pending'] = candidate
        if release is None:
            release = api.create_release(publisher_commit, body(state))
            if parse_state(release) != state:
                raise ValueError('Release creation differs from reservation')
        else:
            api.update_release(release, body(state))
        release = reservation(api, state)

    for name in sorted(bundle.files):
        if name != 'timestamp.json':
            ensure(api, release, assets, name, bundle, name)
    ensure(api, release, assets, bundle.archive_name, bundle, 'timestamp.json')
    release = reservation(api, state)
    # Reobserve the alias immediately before replacement. Only this one asset
    # is mutable, and its previous contents must match retained reservation state.
    current = asset_map(api, release).get('timestamp.json')
    ready = known_timestamp(api, current, state)
    if ready != candidate:
        if unchanged:
            raise ValueError('Committed timestamp changed during verification')
        if current:
            api.delete_asset(current)
        assets.pop('timestamp.json', None)
        ensure(api, release, assets, 'timestamp.json', bundle, 'timestamp.json')
    if unchanged:
        return 'unchanged'
    release = reservation(api, state)
    api.update_release(release, body(dict(schema=1, current=candidate, pending=None)))
    return 'published'


def abandon(api, current_record, pending_record):
    """Clear an unexposed attempt only with old jobs quiescent and writer lock held.

    The records come from the trusted journal's current and sealed publication.
    This never publishes expired metadata, removes images, or rolls back a
    visible timestamp. 'already-visible' requires journal confirmation instead.
    """
    if (api.repo != REPOSITORY or api.tag != TAG
            or not valid_record(current_record) or not valid_record(pending_record)
            or pending_record['version'] <= current_record['version']):
        raise ValueError('Invalid publication abandonment selection')
    release = api.get_release()
    if release is None:
        raise ValueError('Previously initialized OTA feed is missing')
    state = parse_state(release)
    if state['current'] == pending_record and state['pending'] in (None, pending_record):
        visible = known_timestamp(api, asset_map(api, release).get('timestamp.json'), state)
        if visible == pending_record:
            return 'already-visible'
        raise ValueError('Committed publication is not available for reconciliation')
    if state['current'] != current_record or state['pending'] not in (None, pending_record):
        raise ValueError('Feed reservation differs from the selected journal attempt')
    alias = asset_map(api, release).get('timestamp.json')
    visible = known_timestamp(api, alias, state)
    if visible == pending_record:
        return 'already-visible'
    if state['pending'] is None:
        return 'abandoned'  # A previous clearing update succeeded but lost its reply.
    if alias and alias['state'] == 'starter' and alias['size'] == 0:
        api.delete_asset(alias)
    release = reservation(api, state)
    visible = known_timestamp(api, asset_map(api, release).get('timestamp.json'), state)
    if visible == pending_record:
        return 'already-visible'
    cleared = dict(schema=1, current=current_record, pending=None)
    api.update_release(release, body(cleared))
    release = reservation(api, cleared)
    known_timestamp(api, asset_map(api, release).get('timestamp.json'), cleared)
    return 'abandoned'


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, newurl):
        raise ValueError('Authenticated API redirects are not supported')


class PublicRedirect(urllib.request.HTTPRedirectHandler):
    max_repeats = 5
    max_redirections = 5

    def redirect_request(self, request, fp, code, message, headers, newurl):
        parsed = urllib.parse.urlsplit(newurl)
        if (parsed.scheme != 'https' or not parsed.hostname or parsed.username is not None
                or parsed.password is not None or parsed.fragment or len(newurl) > 8192):
            raise ValueError('Invalid public asset redirect')
        redirected = super().redirect_request(request, fp, code, message, headers, newurl)
        if redirected:
            redirected.remove_header('Authorization')
        return redirected


class LimitedReader:
    def __init__(self, stream, length):
        self.stream, self.remaining = stream, length

    def read(self, size=-1):
        if size < 0:
            size = CHUNK
        value = self.stream.read(min(size, self.remaining))
        self.remaining -= len(value)
        return value


class GitHub:
    repo = REPOSITORY
    tag = TAG

    def __init__(self, token):
        self.token = token
        self.base = f'https://api.github.com/repos/{self.repo}'

    def request(self, method, url, payload=None, *, length=None, response_limit=MAX_MANIFEST_BYTES):
        raw = LimitedReader(payload, length) if length is not None else (
            json.dumps(payload).encode() if payload is not None else None)
        headers = {'Authorization': f'Bearer {self.token}',
                   'Accept': 'application/vnd.github+json',
                   'Content-Type': 'application/octet-stream' if length is not None else 'application/json',
                   'X-GitHub-Api-Version': '2022-11-28'}
        if length is not None:
            headers['Content-Length'] = str(length)
        request = urllib.request.Request(url, data=raw, method=method, headers=headers)
        # Never retry a mutation after a lost response. Reconciliation belongs to
        # the next serialized invocation, which reads reservation and asset state.
        try:
            with urllib.request.build_opener(NoRedirect()).open(request, timeout=120) as response:
                data = response.read(response_limit + 1)
        except urllib.error.HTTPError as error:
            error.close()
            raise
        if len(data) > response_limit:
            raise ValueError('GitHub JSON response exceeds limit')
        return strict_json(data) if data else None

    def get_release(self):
        try:
            return self.request('GET', self.base + f'/releases/tags/{self.tag}')
        except urllib.error.HTTPError as error:
            if error.code == 404:
                return None
            raise

    def create_release(self, source, content):
        return self.request('POST', self.base + '/releases', dict(
            tag_name=self.tag, target_commitish=source, name=TITLE,
            prerelease=True, draft=False, body=content))

    def update_release(self, release, content):
        return self.request('PATCH', self.base + f"/releases/{release['id']}", {'body': content})

    def assets(self, release):
        result = []
        for page in range(1, MAX_ASSETS // 100 + 2):
            batch = self.request('GET', self.base + f"/releases/{release['id']}/assets?per_page=100&page={page}")
            if not isinstance(batch, list) or len(batch) > 100:
                raise ValueError('Invalid release asset page')
            result.extend(batch)
            if len(result) > MAX_ASSETS:
                raise ValueError('Release asset limit exceeded')
            if len(batch) < 100:
                return result
        raise ValueError('Release asset pagination exceeds limit')

    def upload(self, release, name, stream, length):
        url = (f"https://uploads.github.com/repos/{self.repo}/releases/{release['id']}/assets?name="
               + urllib.parse.quote(name, safe=''))
        return self.request('POST', url, stream, length=length)

    def delete_asset(self, asset):
        return self.request('DELETE', self.base + f"/releases/assets/{asset['id']}")

    def verify(self, asset, expected):
        if asset.get('state') != 'uploaded' or asset.get('size') != expected['bytes']:
            raise ValueError('Remote asset length/state differs from manifest')
        url = (f'https://github.com/{self.repo}/releases/download/{self.tag}/'
               + urllib.parse.quote(asset['name'], safe=''))
        if asset['name'] == 'timestamp.json':
            if not integer(asset.get('id')):
                raise ValueError('Invalid mutable timestamp asset identity')
            # GitHub can cache the name's old CDN redirect after replacement.
            # Bind this read to the API-observed replacement, retaining the
            # normal public download route and exact signed-byte verification.
            url += '?asset_id=' + str(asset['id'])
        request = urllib.request.Request(url, headers={'Accept-Encoding': 'identity'})
        # Deliberately credential-free, including all CDN redirects.
        try:
            with urllib.request.build_opener(PublicRedirect()).open(request, timeout=120) as response:
                if response.status != 200 or response.headers.get('Content-Encoding', 'identity') != 'identity':
                    raise ValueError('Invalid public asset response')
                verify_stream(response, expected)
        except urllib.error.URLError as error:
            # Redirect URLs can contain temporary CDN query credentials.
            if isinstance(error, urllib.error.HTTPError):
                error.close()
            raise OSError('Public OTA asset download failed') from None
