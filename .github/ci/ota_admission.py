"""Read-only GitHub origin gate for an OTA candidate, before image qualification.

The publisher supplies reviewed policy from its own pinned checkout. Never load
policy, code or signing keys from the candidate. A receipt proves GitHub origin
and exact archive bytes, NOT image safety, promotion approval or generation
freshness. The caller must qualify the extracted image and enforce publication
history separately before asking an isolated signer for a proposal.
"""
import base64
from contextlib import contextmanager
from dataclasses import asdict, dataclass
from datetime import datetime, timedelta, timezone
import hashlib
import json
from pathlib import Path
import re
import stat
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

from ota_delivery import CHUNK, PublicRedirect, digest, integer, sha, strict_json

REPOSITORY = 'aortez/space-wars'
REPOSITORY_ID = 3003186
MAX_ARCHIVE_BYTES = 2 * 1024 ** 3 - 1
MAX_JSON_BYTES = 1024 * 1024
MAX_WORKFLOW_BYTES = 128 * 1024
MAX_JOBS = 100
COMMIT = re.compile(r'[0-9a-f]{40}\Z')
NAME = re.compile(r'[a-z0-9][a-z0-9.-]{0,99}\Z')
UTC = timezone.utc


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def timestamp(value):
    if not isinstance(value, str) or not re.fullmatch(r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z', value):
        raise ValueError('Invalid GitHub UTC timestamp')
    return datetime.strptime(value, '%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=UTC)


@dataclass(frozen=True)
class Workflow:
    path: str
    sha256: str
    events: tuple
    # (job name, required successful step names), selected by workflow review.
    jobs: tuple
    # Explicitly reviewed opt-in jobs that must be skipped in an admitted run.
    skipped_jobs: tuple = ()

    def validate(self):
        if (not re.fullmatch(r'\.github/workflows/[a-z0-9-]+\.yml', self.path)
                or not sha(self.sha256) or not self.events
                or not set(self.events) <= {'push', 'schedule', 'workflow_dispatch'}
                or len(set(self.events)) != len(self.events)
                or not 1 <= len(self.jobs) <= MAX_JOBS
                or len(self.jobs) + len(self.skipped_jobs) > MAX_JOBS):
            raise ValueError('Invalid trusted workflow policy')
        names = set()
        for name, steps in self.jobs:
            if (not isinstance(name, str) or not 1 <= len(name) <= 200 or name in names
                    or not steps or len(set(steps)) != len(steps)
                    or any(not isinstance(step, str) or not 1 <= len(step) <= 500 for step in steps)):
                raise ValueError('Invalid required jobs/steps')
            names.add(name)
        for name in self.skipped_jobs:
            if not isinstance(name, str) or not 1 <= len(name) <= 200 or name in names:
                raise ValueError('Invalid reviewed skipped jobs')
            names.add(name)


@dataclass(frozen=True)
class Policy:
    build: Workflow
    ci: Workflow
    producer_job: str
    # Exact flat archive member names and maximum expanded sizes. Image metadata
    # is still untrusted data; these are transport limits, not its schema.
    files: tuple
    max_age_hours: int = 168

    def validate(self):
        self.build.validate()
        self.ci.validate()
        if (self.build.path != '.github/workflows/ota-build.yml'
                or self.build.skipped_jobs
                or not set(self.build.events) <= {'schedule', 'workflow_dispatch'}
                or self.ci.path != '.github/workflows/ci.yml'
                or not set(self.ci.events) <= {'push', 'workflow_dispatch'}
                or self.producer_job not in dict(self.build.jobs)
                or not integer(self.max_age_hours, upper=168)
                or not 1 <= len(self.files) <= 16
                or len(dict(self.files)) != len(self.files)
                or any(not NAME.fullmatch(name) or name == 'receipt.json'
                       or not integer(limit, upper=MAX_ARCHIVE_BYTES) for name, limit in self.files)
                or sum(limit for _, limit in self.files) > MAX_ARCHIVE_BYTES + 16 * MAX_JSON_BYTES):
            raise ValueError('Invalid trusted OTA admission policy')


@dataclass(frozen=True)
class Selection:
    source: str
    build_run: int
    build_attempt: int
    ci_run: int
    ci_attempt: int
    artifact: int

    def validate(self):
        if (not isinstance(self.source, str) or not COMMIT.fullmatch(self.source)
                or any(not integer(value) for value in (self.build_run, self.build_attempt,
                                                       self.ci_run, self.ci_attempt, self.artifact))):
            raise ValueError('Invalid exact GitHub selection')


def completed(value):
    return value.get('status') == 'completed' and value.get('conclusion') == 'success'


def run_identity(value, workflow, workflow_id, run_id, attempt, source, now, max_age_hours):
    expected = dict(id=run_id, run_attempt=attempt, workflow_id=workflow_id,
                    path=workflow.path, head_sha=source, head_branch='main')
    if (any(value.get(key) != item or type(value.get(key)) is not type(item)
            for key, item in expected.items()) or not completed(value)
            or value.get('event') not in workflow.events or value.get('referenced_workflows') != []
            or value.get('head_commit', {}).get('id') != source):
        raise ValueError('Run is not the selected successful main workflow attempt')
    for key in ('repository', 'head_repository'):
        repository = value.get(key, {})
        if repository.get('id') != REPOSITORY_ID or repository.get('full_name') != REPOSITORY:
            raise ValueError('Foreign or fork workflow run')
    created, started, finished = (timestamp(value.get(key))
                                  for key in ('created_at', 'run_started_at', 'updated_at'))
    if not now - timedelta(hours=max_age_hours) <= created <= started <= finished <= now:
        raise ValueError('Stale or invalid run times')
    return {**expected, 'event': value['event'], 'created_at': value['created_at'],
            'started_at': value['run_started_at'], 'completed_at': value['updated_at']}


def inspect_run(api, workflow, run_id, attempt, source, now, max_age_hours=168):
    """Query authoritative run and exact-attempt jobs; never accept build-result input."""
    workflow.validate()
    if (not integer(run_id) or not integer(attempt) or not COMMIT.fullmatch(source)
            or now.tzinfo != UTC or not integer(max_age_hours, upper=168)):
        raise ValueError('Invalid run inspection request')
    registered = api.workflow(workflow.path)
    if (not integer(registered.get('id')) or registered.get('path') != workflow.path
            or registered.get('state') != 'active'
            or digest(api.file(workflow.path, source)) != workflow.sha256):
        raise ValueError('Workflow differs from reviewed policy')
    current = run_identity(api.run(run_id), workflow, registered['id'], run_id, attempt,
                           source, now, max_age_hours)
    exact = run_identity(api.run(run_id, attempt), workflow, registered['id'], run_id, attempt,
                         source, now, max_age_hours)
    if current != exact:
        raise ValueError('Run changed or selected attempt is no longer current')
    jobs = api.jobs(run_id, attempt)
    expected = dict(workflow.jobs)
    skipped = set(workflow.skipped_jobs)
    if len(jobs) != len(expected) + len(skipped):
        raise ValueError('Required workflow job set differs')
    seen, ids, receipts, skipped_receipts = set(), set(), {}, {}
    for job in jobs:
        name = job.get('name')
        if (name not in expected.keys() | skipped or name in seen or not integer(job.get('id'))
                or job['id'] in ids or job.get('run_id') != run_id
                or job.get('run_attempt') != attempt or job.get('head_sha') != source):
            raise ValueError('Missing, failed or foreign required job')
        seen.add(name)
        ids.add(job['id'])
        started, finished = timestamp(job.get('started_at')), timestamp(job.get('completed_at'))
        if not timestamp(exact['started_at']) <= started <= finished <= timestamp(exact['completed_at']):
            raise ValueError('Job belongs outside the selected attempt')
        steps = job.get('steps')
        if name in skipped:
            if job.get('status') != 'completed' or job.get('conclusion') != 'skipped' or steps != []:
                raise ValueError('Opt-in job was not skipped as reviewed')
            skipped_receipts[name] = dict(id=job['id'], started_at=job['started_at'],
                                         completed_at=job['completed_at'])
            continue
        if not completed(job):
            raise ValueError('Required job did not succeed')
        if not isinstance(steps, list) or not 1 <= len(steps) <= 100:
            raise ValueError('Missing or excessive job steps')
        step_names, numbers, passed = set(), set(), set()
        for step in steps:
            step_name, number = step.get('name'), step.get('number')
            if (not isinstance(step_name, str) or step_name in step_names
                    or not integer(number) or number in numbers
                    or step.get('status') != 'completed'
                    or step.get('conclusion') not in ('success', 'skipped')):
                raise ValueError('Invalid, failed or duplicate job step')
            step_names.add(step_name)
            numbers.add(number)
            if completed(step):
                passed.add(step_name)
        if not set(expected[name]) <= passed:
            raise ValueError('Required qualification step did not succeed')
        receipts[name] = dict(id=job['id'], started_at=job['started_at'],
                              completed_at=job['completed_at'], steps=list(expected[name]))
    return {**exact, 'workflow_sha256': workflow.sha256, 'jobs': receipts,
            **({'skipped_jobs': skipped_receipts} if skipped_receipts else {})}


def inspect_artifact(api, selection, build, producer_job, now):
    artifact = api.artifact(selection.artifact)
    expected_name = f'ota-candidate-{selection.source}-{selection.build_run}-{selection.build_attempt}'
    origin = dict(id=selection.build_run, repository_id=REPOSITORY_ID,
                  head_repository_id=REPOSITORY_ID, head_branch='main', head_sha=selection.source)
    hashed = artifact.get('digest')
    if (artifact.get('id') != selection.artifact or artifact.get('name') != expected_name
            or artifact.get('expired') is not False
            or not integer(artifact.get('size_in_bytes'), upper=MAX_ARCHIVE_BYTES)
            or not isinstance(hashed, str) or not hashed.startswith('sha256:') or not sha(hashed[7:])
            or any(artifact.get('workflow_run', {}).get(key) != value for key, value in origin.items())):
        raise ValueError('Artifact differs from selected workflow origin')
    job = build['jobs'][producer_job]
    # GitHub artifacts have a run ID but no attempt field. Both the name and the
    # producer's time window are needed; reruns must upload a new artifact.
    created = timestamp(artifact.get('created_at'))
    if not (timestamp(job['started_at']) <= created <= timestamp(job['completed_at'])
            and timestamp(artifact.get('expires_at')) > now):
        raise ValueError('Artifact is expired or outside the producing job')
    return dict(id=artifact['id'], name=expected_name, bytes=artifact['size_in_bytes'],
                sha256=hashed[7:], created_at=artifact['created_at'], expires_at=artifact['expires_at'])


class BoundedZipReads:
    """Bound ZIP metadata allocation before ZipFile parses its central directory."""
    def __init__(self, stream):
        self.stream = stream
        self.length = stream.seek(0, 2)
        stream.seek(0)

    def seek(self, *args):
        return self.stream.seek(*args)

    def tell(self):
        return self.stream.tell()

    def seekable(self):
        return True

    def read(self, size=-1):
        actual = self.length - self.tell() if size < 0 else size
        if not 0 <= actual <= 128 * 1024:
            raise ValueError('ZIP metadata/read exceeds limit')
        return self.stream.read(actual)


def extract(stream, directory, limits):
    """Extract only bounded, flat regular data files; never trust ZIP paths/modes."""
    files = {}
    with zipfile.ZipFile(BoundedZipReads(stream)) as archive:
        members = archive.infolist()
        if len(members) != len(limits):
            raise ValueError('Archive member set differs from policy')
        for entry in members:
            mode = entry.external_attr >> 16
            if (entry.filename not in limits or entry.filename in files
                    or entry.orig_filename != entry.filename or entry.is_dir()
                    or entry.external_attr & 0x10
                    or entry.flag_bits & 1 or entry.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
                    or entry.create_system not in (0, 3)
                    or stat.S_IFMT(mode) not in (0, stat.S_IFREG)
                    or not integer(entry.file_size, upper=limits[entry.filename])):
                raise ValueError('Unsafe, duplicate or over-limit archive member')
            hashed, count = hashlib.sha256(), 0
            with archive.open(entry) as source, (directory / entry.filename).open('xb') as target:
                while chunk := source.read(min(CHUNK, entry.file_size - count + 1)):
                    count += len(chunk)
                    if count > entry.file_size:
                        raise ValueError('Archive member exceeds its declared length')
                    hashed.update(chunk)
                    target.write(chunk)
            if count != entry.file_size:
                raise ValueError('Archive member truncated')
            files[entry.filename] = dict(bytes=count, sha256=hashed.hexdigest())
    return files


def inspect_candidate(api, policy, selection, now):
    policy.validate()
    selection.validate()
    build = inspect_run(api, policy.build, selection.build_run, selection.build_attempt,
                        selection.source, now, policy.max_age_hours)
    # Compare attempts across runs: run_number survives reruns, whereas GitHub's
    # run_started_at resets. A later retry of an older run also supersedes this
    # artifact, regardless of that retry's outcome.
    runs = api.workflow_runs(policy.build.path, selection.source)
    if (not isinstance(runs, list) or not 1 <= len(runs) <= 100
            or any(not isinstance(run, dict) or not integer(run.get('run_number'))
                   or not integer(run.get('run_attempt'))
                   or not integer(run.get('id')) or run.get('path') != policy.build.path
                   or run.get('head_sha') != selection.source or run.get('head_branch') != 'main'
                   for run in runs)
            or len({run['id'] for run in runs}) != len(runs)
            or len({run['run_number'] for run in runs}) != len(runs)):
        raise ValueError('Missing, ambiguous or excessive candidate workflow history')
    attempts = [(timestamp(run.get('run_started_at')), run) for run in runs]
    for started, run in attempts:
        if not timestamp(run.get('created_at')) <= started <= timestamp(run.get('updated_at')) <= now:
            raise ValueError('Invalid candidate attempt chronology')
    latest_start = max(started for started, _ in attempts)
    latest = [run for started, run in attempts if started == latest_start]
    # API timestamps have second precision. A tied start does not establish
    # which attempt is newest; run numbers cannot resolve a rerun tie safely.
    if len(latest) != 1:
        raise ValueError('Ambiguous latest candidate attempt')
    latest = latest[0]
    if (latest['id'] != selection.build_run or latest.get('run_attempt') != selection.build_attempt
            or not completed(latest)):
        raise ValueError('A newer candidate build must pass before publication')
    if (latest['created_at'] != build['created_at'] or latest['run_started_at'] != build['started_at']
            or latest['updated_at'] != build['completed_at']):
        raise ValueError('Candidate history differs from the inspected attempt')
    ci = inspect_run(api, policy.ci, selection.ci_run, selection.ci_attempt,
                     selection.source, now, policy.max_age_hours)
    artifact = inspect_artifact(api, selection, build, policy.producer_job, now)
    return build, ci, artifact


def recheck(api, policy, selection, receipt, clock=lambda: datetime.now(UTC)):
    """Recheck live origin after expensive image validation, without downloading.

    The caller must already hold the verified bytes from candidate(). An
    unsigned receipt alone is not image qualification or signing approval.
    """
    if (receipt.get('scope') != 'github-artifact-origin'
            or receipt.get('source') != selection.source
            or receipt.get('repository') != REPOSITORY or receipt.get('repository_id') != REPOSITORY_ID
            or receipt.get('policy_sha256') != digest(canonical(asdict(policy)))):
        raise ValueError('Origin receipt differs from selected publisher policy')
    after = inspect_candidate(api, policy, selection, clock())
    if canonical(after) != canonical(tuple(receipt.get(name) for name in ('build', 'ci', 'artifact'))):
        raise ValueError('Build, CI or artifact changed during qualification')
    finished = clock()
    if (not timestamp(receipt['admitted_at']) <= finished < timestamp(after[2]['expires_at'])
            or any(timestamp(run['created_at']) < finished - timedelta(hours=policy.max_age_hours)
                   for run in after[:2])):
        raise ValueError('Qualification time changed or artifact expired')
    return finished.strftime('%Y-%m-%dT%H:%M:%SZ')


@contextmanager
def candidate(api, policy, selection, scratch, clock=lambda: datetime.now(UTC)):
    """Yield private verified data and an origin receipt; delete both on exit.

    Run without signing/publishing credentials. The trusted caller consumes the
    files inside this context and independently validates their image contents.
    Do not use the unsigned receipt as an authorization in a later job.
    """
    policy.validate()
    selection.validate()
    started = clock()

    def inspect():
        return inspect_candidate(api, policy, selection, clock())

    before = inspect()
    with tempfile.TemporaryDirectory(prefix='ota-admission-', dir=scratch) as temporary:
        directory = Path(temporary)
        with tempfile.TemporaryFile(dir=directory) as stream:
            api.download(selection.artifact, before[2], stream)
            # The gate rechecks even if a transport adapter claims verification.
            stream.seek(0)
            hashed, size = hashlib.sha256(), 0
            while chunk := stream.read(CHUNK):
                size += len(chunk)
                if size > before[2]['bytes']:
                    raise ValueError('Downloaded artifact is larger than admitted')
                hashed.update(chunk)
            if size != before[2]['bytes'] or hashed.hexdigest() != before[2]['sha256']:
                raise ValueError('Downloaded archive differs from GitHub digest')
            stream.seek(0)
            files = extract(stream, directory, dict(policy.files))
        after = inspect()
        if before != after:
            raise ValueError('Build, CI or artifact changed during admission')
        finished = clock()
        if (not started <= finished < timestamp(after[2]['expires_at'])
                or any(timestamp(run['created_at']) < finished - timedelta(hours=policy.max_age_hours)
                       for run in after[:2])):
            raise ValueError('Admission time changed or artifact expired')
        receipt = dict(schema=1, scope='github-artifact-origin', repository=REPOSITORY,
                       repository_id=REPOSITORY_ID, source=selection.source,
                       policy_sha256=digest(canonical(asdict(policy))),
                       admitted_at=finished.strftime('%Y-%m-%dT%H:%M:%SZ'),
                       build=after[0], ci=after[1], artifact=after[2], files=files)
        yield directory, receipt


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args):
        # Keep the 302 response for an explicit token-free artifact request.
        return None


class GitHub:
    """Fixed-repository, read-only REST adapter. No URL from JSON gets credentials."""
    def __init__(self, token):
        self.base = f'https://api.github.com/repos/{REPOSITORY}'
        self.token = token
        self.deadline = time.monotonic() + 30 * 60
        self.api_opener = urllib.request.build_opener(NoRedirect())
        self.public_opener = urllib.request.build_opener(PublicRedirect())

    def timeout(self):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('GitHub artifact admission deadline expired')
        return min(30, remaining)

    def request(self, suffix):
        if not suffix.startswith('/') or suffix.startswith('//') or '..' in suffix.split('/'):
            raise ValueError('Invalid API suffix')
        request = urllib.request.Request(self.base + suffix, headers={
            'Authorization': f'Bearer {self.token}', 'Accept': 'application/vnd.github+json',
            'Accept-Encoding': 'identity', 'X-GitHub-Api-Version': '2022-11-28'})
        return self.api_opener.open(request, timeout=self.timeout())

    def json(self, suffix):
        try:
            with self.request(suffix) as response:
                if response.status != 200 or response.headers.get('Content-Encoding', 'identity') != 'identity':
                    raise ValueError('Invalid GitHub JSON response')
                data = bytearray()
                while len(data) <= MAX_JSON_BYTES:
                    self.timeout()
                    # read1 returns after one buffered/socket read, allowing
                    # deadline checks even when the peer trickles a response.
                    chunk = response.read1(min(CHUNK, MAX_JSON_BYTES + 1 - len(data)))
                    if not chunk:
                        break
                    data.extend(chunk)
        except urllib.error.URLError as error:
            if isinstance(error, urllib.error.HTTPError):
                error.close()
            raise OSError('GitHub admission API read failed') from None
        if len(data) > MAX_JSON_BYTES:
            raise ValueError('GitHub JSON response exceeds limit')
        value = strict_json(data)
        if not isinstance(value, dict):
            raise ValueError('Invalid GitHub JSON object')
        return value

    def workflow(self, path):
        return self.json('/actions/workflows/' + urllib.parse.quote(Path(path).name, safe=''))

    def file(self, path, source):
        value = self.json('/contents/' + urllib.parse.quote(path, safe='/') + '?ref=' + source)
        if (value.get('path') != path or value.get('type') != 'file' or value.get('encoding') != 'base64'
                or not integer(value.get('size'), upper=MAX_WORKFLOW_BYTES)):
            raise ValueError('Invalid workflow file response')
        content = base64.b64decode(value['content'].replace('\n', ''), validate=True)
        if len(content) != value['size']:
            raise ValueError('Workflow file length mismatch')
        return content

    def run(self, run_id, attempt=None):
        suffix = f'/actions/runs/{run_id}'
        return self.json(suffix if attempt is None else suffix + f'/attempts/{attempt}')

    def workflow_runs(self, path, source):
        if not COMMIT.fullmatch(source):
            raise ValueError('Invalid workflow history source')
        workflow = urllib.parse.quote(Path(path).name, safe='')
        value = self.json(f'/actions/workflows/{workflow}/runs?head_sha={source}&branch=main&per_page=100')
        runs = value.get('workflow_runs')
        if (not isinstance(runs, list) or not integer(value.get('total_count'), upper=100)
                or len(runs) != value['total_count']):
            raise ValueError('Missing or excessive candidate workflow history')
        return runs

    def jobs(self, run_id, attempt):
        value = self.json(f'/actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100')
        jobs = value.get('jobs')
        if (not isinstance(jobs, list) or not integer(value.get('total_count'), upper=MAX_JOBS)
                or len(jobs) != value['total_count']):
            raise ValueError('Missing or excessive workflow jobs')
        return jobs

    def artifact(self, artifact_id):
        return self.json(f'/actions/artifacts/{artifact_id}')

    def download(self, artifact_id, expected, target):
        try:
            response = self.request(f'/actions/artifacts/{artifact_id}/zip')
        except urllib.error.HTTPError as error:
            try:
                if error.code != 302:
                    raise OSError('GitHub artifact download is unavailable') from None
                location = error.headers.get('Location', '')
            finally:
                error.close()
        except urllib.error.URLError:
            raise OSError('GitHub artifact download is unavailable') from None
        else:
            response.close()
            raise ValueError('Expected GitHub artifact redirect')
        parsed = urllib.parse.urlsplit(location)
        if (parsed.scheme != 'https' or not parsed.hostname or parsed.username is not None
                or parsed.password is not None or parsed.fragment or len(location) > 8192):
            raise ValueError('Invalid artifact download location')
        request = urllib.request.Request(location, headers={'Accept-Encoding': 'identity'})
        remaining, hashed = expected['bytes'], hashlib.sha256()
        try:
            # A separate opener/request never carries the API bearer token to
            # the artifact host or subsequent CDN redirects.
            with self.public_opener.open(request, timeout=self.timeout()) as response:
                if response.status != 200 or response.headers.get('Content-Encoding', 'identity') != 'identity':
                    raise ValueError('Invalid artifact body response')
                while remaining:
                    self.timeout()
                    chunk = response.read1(min(CHUNK, remaining))
                    if not chunk or len(chunk) > remaining:
                        raise ValueError('Truncated artifact download')
                    remaining -= len(chunk)
                    hashed.update(chunk)
                    target.write(chunk)
                if response.read(1) or hashed.hexdigest() != expected['sha256']:
                    raise ValueError('Artifact download differs from GitHub digest')
        except urllib.error.URLError as error:
            # HTTP errors may contain temporary signed CDN query credentials.
            if isinstance(error, urllib.error.HTTPError):
                error.close()
            raise OSError('OTA artifact body download failed') from None
