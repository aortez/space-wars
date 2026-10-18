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
    ('OTA updater and publisher', ('Test updater, metadata, image and publisher fixtures',
        'Build publisher fixture tools', 'Exercise resumable publication with disposable keys', 'Lint OTA publisher')),
    ('Pi hardware profiles and update safety', ('Verify unprivileged installer sandbox',
        'Test Pi deployment helpers', 'Lint Pi deployment scripts')),
    ('NES core, golden tests, and benchmark', ('Check Rust formatting', 'Test core and scenarios',
        'Lint NES core and scenarios', 'Exercise bundled-ROM release benchmark')),
    ('Build Linux tests once', ('Test CI reports, build identity, and input diagnostic safety',
        'Test recorded bot-trip analysis', 'Build optimized Linux test executables',
        'Verify unchanged builds reuse compiled artifacts', 'Verify PR UI selection',
        'Archive tests for independent runners')),
    ('Linux workspace tests', ('Run every non-ignored workspace test without compiling',)),
    ('PR UI smoke', ('Run UI workflows without compiling',)),
    ('GPU kiosk and vendored renderers', (
        'Test vendored LinuxKMS input and presentation renderers',
        'Test exact round-join shortcut against vendored tessellator reference',
        'Check default GPU kiosk client (no display required)',
        'Test GPU defaults, startup safety, text and rotation (no display required)')),
    ('CCD physics diagnostics', ('Test CCD cache and diagnostic lifecycle regressions',)),
    ('Linux workspace', ('Require all Linux checks',)),
    ('Spacewars experimental recovery', ('Test recovery profiles and native ground/boarding behavior',
        'Test experimental supported-corridor observations', 'Lint recovery code and diagnostics')),
    ('Spacewars AI deterministic baselines', ('Verify navigation-v1 baseline', 'Verify strategy-v1 baseline')),
    ('Windows client', ('Test core and scenarios', 'Check Windows client')),
    ('AArch64 core and scenario', ('Check AArch64 core and scenarios',)),
    ('Windows release validation', ('Verify selected head', 'Build and validate extracted release')),
)
# Admission selects ordinary main CI, where these two opt-in jobs are explicitly
# skipped. Full nightly validation remains a separate publication gate.
CI_EXTENDED_JOBS = (
    ('Clock extended / ${{ matrix.suite }}', ('Run extended deterministic sweep',)),
    ('Extended normal-match and arena parity', ('Compare normal matches against the deterministic arena',)),
)
CI_UI_NAME = "${{ inputs.full && format('Full UI workflows ({0}/4)', matrix.shard) || 'PR UI smoke' }}"


def workflow(raw, path, events, jobs):
    if re.search(rb'\bsecrets\s*(?:\.|\[)', raw, re.IGNORECASE):
        raise ValueError('Build/source CI must not receive signing or other custom secrets')
    value = yaml.load(raw, Loader=yaml.BaseLoader)
    is_ci = path == '.github/workflows/ci.yml'
    if (not isinstance(value, dict) or not isinstance(value.get('jobs'), dict)
            or value.get('permissions') != {'contents': 'read'}
            or set(value.get('on', {})) != set(events) | ({'pull_request', 'workflow_call'} if is_ci else set())):
        raise ValueError('Reviewed workflow contract changed')
    expected = dict(jobs)
    extended = dict(CI_EXTENDED_JOBS) if is_ci else {}
    expected.update(extended)
    actual = {}
    for job in value['jobs'].values():
        name = job.get('name') if isinstance(job, dict) else None
        if is_ci and name == CI_UI_NAME:
            name = 'PR UI smoke'
        if (not isinstance(job, dict) or 'uses' in job or 'permissions' in job or 'environment' in job
                or job.get('continue-on-error', 'false') != 'false'
                or name not in expected or name in actual):
            raise ValueError('Reviewed job authority or identity changed')
        condition = ('inputs.full' if name in extended else
                     '${{ always() }}' if name == 'Linux workspace' else None)
        if is_ci and job.get('if') != condition:
            raise ValueError('Reviewed required/opt-in job condition changed')
        actual[name] = job
        names = set()
        for step in job.get('steps', []):
            if 'uses' in step and not re.fullmatch(r'[A-Za-z0-9_/-]+@[0-9a-f]{40}', step['uses']):
                raise ValueError('Every source CI/build action must use an immutable commit')
            step_name = step.get('name')
            if step_name in expected[name]:
                if step_name in names or step.get('continue-on-error', 'false') != 'false':
                    raise ValueError('Required qualification step is duplicate or allowed to fail')
                names.add(step_name)
        if names != set(expected[name]):
            raise ValueError('Reviewed required qualification step is missing')
    if set(actual) != set(expected):
        raise ValueError('Reviewed workflow job set changed')
    return Workflow(path, digest(raw), events, jobs, tuple(extended))


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
