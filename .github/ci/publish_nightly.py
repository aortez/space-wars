"""Publish one rolling nightly. API failures are visible; reruns reconcile state.

The workflow must serialize callers. The release body is a durable reservation:
reserve before asset mutations so even an interrupted newer run excludes older
runs. Stage and verify bytes before removing the previous stable download.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import urllib.error
import urllib.parse
import urllib.request
import zipfile

MARKER = '<!-- spacewars-nightly-v1\n'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_json(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def validate(directory, repo, sha, run, attempt, build_result):
    if (build_result != 'success' or not re.fullmatch('[0-9a-f]{40}', sha)
            or run <= 0 or attempt <= 0):
        raise ValueError('Build did not succeed or source is invalid')
    identity = dict(repository=repo, source_commit=sha, tested_commit=sha,
                    source_kind='head', run_id=run, run_attempt=attempt)
    manifest = read_json(directory / 'manifest.json')
    record = read_json(directory / 'release.json')
    for document in (manifest, record):
        if any(document.get(k) != v for k, v in identity.items()):
            raise ValueError('Evidence identity mismatch')
    if record.get('status') != 'captured' or record.get('required_mode') != 'software':
        raise ValueError('Required validation failed')
    required = {'release.json', 'build.log', 'windows-client.zip', 'software/result.json'}
    if not required <= manifest['files'].keys():
        raise ValueError('Missing evidence')
    for name, expected in manifest['files'].items():
        path = directory / name
        if Path(name).is_absolute() or '..' in Path(name).parts or not path.is_file():
            raise ValueError('Invalid evidence path')
        if digest(path.read_bytes()) != expected:
            raise ValueError('Evidence digest mismatch')
    data = (directory / 'windows-client.zip').read_bytes()
    package = record['package']
    if (package['file'] != 'windows-client.zip' or package['sha256'] != digest(data)
            or package['size_bytes'] != len(data)):
        raise ValueError('Package mismatch')
    probe = read_json(directory / 'software/result.json')
    for key in identity:
        if probe.get(key) != identity[key]:
            raise ValueError('Probe identity mismatch')
    if (probe.get('status') != 'captured' or probe.get('backend_requested') != 'software'
            or probe.get('help_exit_code') != 0 or record['software_launch']['help_exit_code'] != 0):
        raise ValueError('Software validation failed')
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        info = json.loads(archive.read('build-info.json').decode('utf-8-sig'))
        if (any(info.get(k) != v for k, v in identity.items())
                or info.get('target') != 'x86_64-pc-windows-msvc' or info.get('profile') != 'release'):
            raise ValueError('Bundled source identity mismatch')
        executable = digest(archive.read('engine-client.exe'))
        if executable != package['executable_sha256'] or executable != probe['executable_sha256']:
            raise ValueError('Probed executable differs from ZIP')
        if digest(archive.read('run-software.cmd')) != record['software_launch']['sha256']:
            raise ValueError('Software launcher differs from ZIP')
    return dict(source=sha, run=run, attempt=attempt, sha256=digest(data)), data


def body(state, repo):
    current = state.get('current')
    text = 'Rolling Windows nightly; replaced after successful validation.\n'
    if current:
        text += (f"\nSource: `{current['source']}`\n"
                 f"Run: https://github.com/{repo}/actions/runs/{current['run']} "
                 f"(attempt {current['attempt']})\nZIP SHA-256: `{current['sha256']}`\n")
    if state.get('pending'):
        text += '\nPublication in progress; the download may still be the previous build.\n'
    return text + '\n' + MARKER + json.dumps(state, sort_keys=True) + '\n-->'


def parse_state(release):
    if (release['name'] != 'Nightly release' or not release['prerelease']
            or release['draft'] or release['tag_name'] != 'nightly'):
        raise ValueError('Conflicting nightly release')
    content = release.get('body') or ''
    if content.count(MARKER) != 1 or not content.endswith('\n-->'):
        raise ValueError('Unmanaged nightly release')
    state = json.loads(content.split(MARKER)[1][:-4])
    if set(state) != {'current', 'pending'}:
        raise ValueError('Invalid publication state')
    for item in state.values():
        if item is not None:
            if (set(item) != {'source', 'run', 'attempt', 'sha256'}
                    or not re.fullmatch('[0-9a-f]{40}', item['source'])
                    or not re.fullmatch('[0-9a-f]{64}', item['sha256'])
                    or type(item['run']) is not int or item['run'] <= 0
                    or type(item['attempt']) is not int or item['attempt'] <= 0):
                raise ValueError('Invalid provenance')
    return state


def order(item):
    return item['run'], item['attempt']


def publish(api, candidate, data):
    release = api.get_release()
    if release is None:
        # A lost response is not blindly retried. Next invocation finds this
        # reservation by tag, avoiding a second release.
        state = dict(current=None, pending=candidate)
        release = api.create_release(candidate['source'], body(state, api.repo))
    else:
        state = parse_state(release)
        for item in state.values():
            if item and order(item) > order(candidate):
                return 'stale'
            if item and order(item) == order(candidate) and item != candidate:
                raise ValueError('Same run/attempt has conflicting source or hash')
        if state['current'] == candidate and state['pending'] is None:
            assets = api.assets(release)
            if len(assets) != 1 or assets[0]['name'] != 'windows-client.zip':
                raise ValueError('Committed asset set changed')
            verify_asset(api, assets[0], data)
            return 'unchanged'
        state['pending'] = candidate
        api.update_release(release, body(state, api.repo))

    stage_name = f"nightly-{candidate['run']}-{candidate['attempt']}-{candidate['sha256']}.zip"
    assets = api.assets(release)
    if any(a['name'] not in ('windows-client.zip', stage_name) and
           not re.fullmatch(r'nightly-\d+-\d+-[0-9a-f]{64}\.zip', a['name']) for a in assets):
        raise ValueError('Unexpected release asset')
    staged = [a for a in assets if a['name'] == stage_name]
    if len(staged) > 1:
        raise ValueError('Duplicate staged assets')
    stable = [a for a in assets if a['name'] == 'windows-client.zip']
    if len(stable) > 1:
        raise ValueError('Duplicate stable assets')
    # The rename may have succeeded before its response was lost.
    ready = stable[0] if stable and digest(api.download(stable[0])) == candidate['sha256'] else None
    if ready is None:
        ready = staged[0] if staged else api.upload(release, stage_name, data)
        try:
            verify_asset(api, ready, data)
        except ValueError:
            # Failed GitHub uploads can leave a zero-byte starter asset. Remove
            # this candidate's damaged stage, fail visibly, and let a rerun upload.
            api.delete_asset(ready)
            raise
    for asset in api.assets(release):
        if asset['id'] != ready['id']:
            api.delete_asset(asset)
    if ready['name'] != 'windows-client.zip':
        api.rename_asset(ready, 'windows-client.zip')
    verify_asset(api, ready, data)
    api.update_release(release, body(dict(current=candidate, pending=None), api.repo))
    return 'published'


def verify_asset(api, asset, data):
    if api.download(asset) != data:
        raise ValueError('Uploaded ZIP differs from validated ZIP')


class DownloadRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, msg, headers, newurl):
        if urllib.parse.urlsplit(newurl).scheme != 'https':
            raise ValueError('Insecure API redirect')
        redirected = super().redirect_request(request, fp, code, msg, headers, newurl)
        if redirected and urllib.parse.urlsplit(newurl).netloc != urllib.parse.urlsplit(request.full_url).netloc:
            redirected.remove_header('Authorization')
        return redirected


class GitHub:
    def __init__(self, repo, token):
        self.repo = repo
        self.token = token
        self.base = f'https://api.github.com/repos/{repo}'

    def request(self, method, url, payload=None, *, binary_payload=False, raw_response=False):
        # Uploads send ZIP bytes but return JSON asset metadata. Downloads
        # return ZIP bytes; response decoding is independent of payload encoding.
        raw = payload if binary_payload else (json.dumps(payload).encode() if payload is not None else None)
        request = urllib.request.Request(url, data=raw, method=method, headers={
            'Authorization': f'Bearer {self.token}',
            'Accept': 'application/octet-stream' if raw_response else 'application/vnd.github+json',
            'Content-Type': 'application/zip' if binary_payload else 'application/json',
            'X-GitHub-Api-Version': '2022-11-28',
        })
        with urllib.request.build_opener(DownloadRedirect()).open(request, timeout=120) as response:
            data = response.read()
        return data if raw_response else (json.loads(data) if data else None)

    def get_release(self):
        try:
            return self.request('GET', self.base + '/releases/tags/nightly')
        except urllib.error.HTTPError as error:
            if error.code == 404:
                return None
            raise

    def create_release(self, sha, content):
        return self.request('POST', self.base + '/releases', dict(
            tag_name='nightly', target_commitish=sha, name='Nightly release',
            prerelease=True, draft=False, body=content))

    def update_release(self, release, content):
        return self.request('PATCH', self.base + f"/releases/{release['id']}", {'body': content})

    def assets(self, release):
        result = []
        page = 1
        while True:
            batch = self.request('GET', self.base + f"/releases/{release['id']}/assets?per_page=100&page={page}")
            result.extend(batch)
            if len(batch) < 100:
                return result
            page += 1

    def upload(self, release, name, data):
        url = release['upload_url'].split('{')[0] + '?name=' + urllib.parse.quote(name)
        return self.request('POST', url, data, binary_payload=True)

    def download(self, asset):
        return self.request('GET', self.base + f"/releases/assets/{asset['id']}", raw_response=True)

    def delete_asset(self, asset):
        self.request('DELETE', self.base + f"/releases/assets/{asset['id']}")

    def rename_asset(self, asset, name):
        self.request('PATCH', self.base + f"/releases/assets/{asset['id']}", {'name': name})


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--build-result', required=True)
    args = parser.parse_args()
    repo = os.environ['GITHUB_REPOSITORY']
    candidate, data = validate(args.directory, repo, args.source,
                               int(os.environ['GITHUB_RUN_ID']), int(os.environ['GITHUB_RUN_ATTEMPT']),
                               args.build_result)
    print(publish(GitHub(repo, os.environ['GH_TOKEN']), candidate, data))


if __name__ == '__main__':
    main()
