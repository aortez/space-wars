"""Admission expectations from the publisher's independently pinned checkout.

Never call this with an artifact's policy or its source checkout as a substitute
for the reviewed publisher revision. Source admission is checked separately.
"""
import re

import yaml

from ota_admission import Policy, Workflow
from ota_build_inputs import clean_repository
from ota_candidate import ARTIFACT_FILES, read
from ota_delivery import digest

BUILD_JOBS = (('Build and qualify Pi rootfs candidate', (
    'Check out selected main source', 'Reclaim unused SDK space with a short deadline',
    'Install image inspection dependencies', 'Install validator compiler',
    'Build image validator', 'Build selected source in isolated container',
    'Qualify and package exact image', 'Upload unsigned candidate')),)
CI_JOBS = (
    ('OTA updater and publisher', ('Test updater, metadata, image and publisher fixtures', 'Lint OTA publisher')),
    ('Pi hardware profiles and update safety', ('Verify unprivileged installer sandbox',
        'Test Pi deployment helpers', 'Lint Pi deployment scripts')),
    ('NES core, golden tests, and benchmark', ('Check Rust formatting', 'Test core and scenarios',
        'Lint NES core and scenarios', 'Exercise bundled-ROM release benchmark')),
    ('Linux workspace', ('Test CI reports, build identity, and input diagnostic safety',
        'Test recorded bot-trip analysis', 'Build optimized Linux test executables',
        'Verify unchanged builds reuse compiled artifacts', 'Run Linux workspace tests',
        'Verify PR UI selection', 'Run PR UI functional tests',
        'Test vendored LinuxKMS input and presentation renderers',
        'Test exact round-join shortcut against vendored tessellator reference',
        'Check default GPU kiosk client (no display required)',
        'Test GPU defaults, startup safety, text and rotation (no display required)',
        'Test CCD cache and diagnostic lifecycle regressions')),
    ('Spacewars experimental recovery', ('Test recovery profiles and native ground/boarding behavior',
        'Test experimental supported-corridor observations', 'Lint recovery code and diagnostics')),
    ('Spacewars AI deterministic baselines', ('Verify navigation-v1 baseline', 'Verify strategy-v1 baseline')),
    ('Windows client', ('Test core and scenarios', 'Check Windows client')),
    ('AArch64 core and scenario', ('Check AArch64 core and scenarios',)),
    ('Windows release validation', ('Verify selected head', 'Build and validate extracted release')),
)


def workflow(raw, path, events, jobs):
    if re.search(rb'\bsecrets\s*(?:\.|\[)', raw, re.IGNORECASE):
        raise ValueError('Build/source CI must not receive signing or other custom secrets')
    value = yaml.load(raw, Loader=yaml.BaseLoader)
    if (not isinstance(value, dict) or not isinstance(value.get('jobs'), dict)
            or value.get('permissions') != {'contents': 'read'}
            or set(value.get('on', {})) != set(events) | ({'pull_request'} if path.endswith('/ci.yml') else set())):
        raise ValueError('Reviewed workflow contract changed')
    expected = dict(jobs)
    actual = {}
    for job in value['jobs'].values():
        if (not isinstance(job, dict) or 'uses' in job or 'permissions' in job or 'environment' in job
                or job.get('continue-on-error', 'false') != 'false'
                or job.get('name') not in expected or job['name'] in actual):
            raise ValueError('Reviewed job authority or identity changed')
        actual[job['name']] = job
        names = set()
        for step in job.get('steps', []):
            if 'uses' in step and not re.fullmatch(r'[A-Za-z0-9_/-]+@[0-9a-f]{40}', step['uses']):
                raise ValueError('Every source CI/build action must use an immutable commit')
            name = step.get('name')
            if name in expected[job['name']]:
                if name in names or step.get('continue-on-error', 'false') != 'false':
                    raise ValueError('Required qualification step is duplicate or allowed to fail')
                names.add(name)
        if names != set(expected[job['name']]):
            raise ValueError('Reviewed required qualification step is missing')
    if set(actual) != set(expected):
        raise ValueError('Reviewed workflow job set changed')
    return Workflow(path, digest(raw), events, jobs)


def from_checkout(repo, reviewed_commit):
    repo = repo.resolve(strict=True)
    clean_repository(repo, reviewed_commit)
    result = Policy(
        build=workflow(read(repo / '.github/workflows/ota-build.yml', 128 * 1024),
            '.github/workflows/ota-build.yml', ('schedule', 'workflow_dispatch'), BUILD_JOBS),
        ci=workflow(read(repo / '.github/workflows/ci.yml', 128 * 1024),
            '.github/workflows/ci.yml', ('push', 'workflow_dispatch'), CI_JOBS),
        producer_job=BUILD_JOBS[0][0], files=tuple(ARTIFACT_FILES.items()))
    result.validate()
    clean_repository(repo, reviewed_commit)
    return result
