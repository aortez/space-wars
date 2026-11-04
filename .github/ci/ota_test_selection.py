"""Conservative PR selection; the complete suite owns nightly image admission."""
import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
CATALOG = Path(__file__).with_name('ota-test-suites.json')
SHA = re.compile(r'[0-9a-f]{40}\Z')
TEST = re.compile(r'[A-Za-z_][A-Za-z_0-9]*(?:::[A-Za-z_][A-Za-z_0-9]*)+\Z')


def catalog():
    value = json.loads(CATALOG.read_text())
    if (set(value) != {'schema', 'exhaustive', 'required_pr'} or type(value['schema']) is not int or value['schema'] != 1
            or any(not isinstance(value[key], list) or not value[key] for key in ('exhaustive', 'required_pr'))):
        raise ValueError('Invalid OTA coverage catalog')
    names = value['exhaustive'] + value['required_pr']
    if len(set(names)) != len(names) or any(not isinstance(n, str) or not TEST.fullmatch(n) for n in names):
        raise ValueError('Duplicate or invalid OTA coverage name')
    return value


def unrelated(path):
    # An allowlist makes new directories, shared services, CI, toolchains and
    # dependencies select full coverage automatically. Include deleted paths.
    item = PurePosixPath(path)
    if (not path or item.is_absolute() or '..' in item.parts or str(item) != path
            or item.name in {'Cargo.toml', 'Cargo.lock', 'build.rs'}):
        return False
    if path.startswith('docs/'):
        return True
    return (path.startswith(('scenarios/', 'crates/engine-client/src/ui_render_tests/'))
            and item.suffix in {'.rs', '.md', '.html', '.png', '.svg', '.jpg', '.gif', '.wav', '.ogg', '.nes'})


def classify(paths):
    return 'pr' if paths and all(unrelated(path) for path in paths) else 'full'


def git(*args, repo=ROOT):
    return subprocess.check_output(['git', *args], cwd=repo, stderr=subprocess.PIPE, timeout=30)


def selection(env=os.environ, repo=ROOT):
    result = {'schema': 1, 'mode': 'full', 'reason': 'Non-PR or unverified change scope', 'paths': []}
    if env.get('SPACEWARS_OTA_FULL', 'false') != 'false':
        return dict(result, reason='Explicit full validation requested')
    if env.get('GITHUB_EVENT_NAME') != 'pull_request':
        return result
    try:
        event = json.loads(Path(env['GITHUB_EVENT_PATH']).read_text())
        base, head = (event['pull_request'][side]['sha'] for side in ('base', 'head'))
        merge = env['GITHUB_SHA']
        if not all(isinstance(sha, str) and SHA.fullmatch(sha) for sha in (base, head, merge)):
            return result
        # checkout fetch-depth: 2 retains both parents of GitHub's tested merge.
        # Do not infer a complete diff from a shallow/missing merge base.
        parents = git('rev-list', '--parents', '-n', '1', 'HEAD', repo=repo).decode().split()
        if parents != [merge, base, head]:
            return result
        raw = git('diff', '--name-only', '--no-renames', '-z', base, merge, '--', repo=repo)
        if raw and not raw.endswith(b'\0'):
            return result
        paths = raw.decode('utf-8').split('\0')[:-1]
        mode = classify(paths)
        return dict(result, mode=mode, paths=paths,
                    reason='Only reviewed gameplay/documentation paths' if mode == 'pr'
                    else 'OTA, shared, build, dependency, unknown or empty change scope')
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        return result


def validate_discovery(output, suites, ignored_output=''):
    discovered = [line[:-6] for line in output.splitlines() if line.endswith(': test')]
    ignored = {line[:-6] for line in ignored_output.splitlines() if line.endswith(': test')}
    for name in suites['exhaustive'] + suites['required_pr']:
        # nextest filters exact names; new cases with similar names stay selected.
        matches = [case for case in discovered if name == case]
        if matches != [name] or name in ignored:
            raise ValueError(f'OTA coverage name missing, ignored or duplicated: {name}')
    return len(discovered)


def workspace_filter(suites):
    cases = ' | '.join(f'test(={name})' for name in suites['exhaustive'])
    return f'not (package(=engine-ota) & ({cases}))'


def run_tests(mode, report):
    suites = catalog()
    command = ['cargo', 'test', '--locked', '--profile', 'ci', '-p', 'engine-ota',
               '--features', 'publisher', '--all-targets']
    started = time.monotonic()
    listing = subprocess.check_output(command + ['--', '--list', '--format', 'terse'], cwd=ROOT, text=True)
    ignored = subprocess.check_output(command + ['--', '--list', '--ignored', '--format', 'terse'], cwd=ROOT, text=True)
    discovered = validate_discovery(listing, suites, ignored)
    build_seconds = time.monotonic() - started
    skipped = suites['exhaustive'] if mode == 'pr' else []
    # Each test needs its own process: concurrent libtest threads can briefly
    # inherit another fixture's flock descriptor while launching a crash child.
    args = ['cargo', 'nextest', 'run', '--locked', '--cargo-profile', 'ci',
            '--profile', 'ci', '-p', 'engine-ota', '--features', 'publisher',
            '--all-targets', '--test-threads', '4', '--retries', '0',
            '--no-fail-fast', '--no-tests', 'fail', '--ignore-default-filter']
    if skipped:
        args += ['-E', workspace_filter(suites)]
    print(f'OTA coverage: {mode}; {discovered} discovered; {len(skipped)} exhaustive sweeps deferred. '
          'Existing ignored tests remain ignored.', flush=True)
    started = time.monotonic()
    result = subprocess.run(args, cwd=ROOT)
    value = dict(schema=1, mode=mode, runner='nextest', processes=4,
                 discovered=discovered, omitted_sweeps=skipped,
                 build_and_discovery_seconds=round(build_seconds, 3),
                 execution_seconds=round(time.monotonic() - started, 3), returncode=result.returncode)
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps(value, indent=2) + '\n')
    if summary := os.environ.get('GITHUB_STEP_SUMMARY'):
        with Path(summary).open('a') as stream:
            stream.write(f"\n### OTA coverage: {mode}\n\n"
                         f"Build/discovery: {value['build_and_discovery_seconds']:.2f}s; "
                         f"execution: {value['execution_seconds']:.2f}s. "
                         f"Deferred exhaustive sweeps: {len(skipped)}. Exit: {result.returncode}.\n")
    return result.returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    choose = sub.add_parser('select')
    choose.add_argument('--output', type=Path, required=True)
    run = sub.add_parser('run')
    mode = run.add_mutually_exclusive_group(required=True)
    mode.add_argument('--full', action='store_true')
    mode.add_argument('--selection', type=Path)
    run.add_argument('--report', type=Path, default=ROOT / 'target/ota-test-report.json')
    workspace = sub.add_parser('workspace')
    workspace.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.action == 'select':
        value = selection()
        args.output.write_text(json.dumps(value, indent=2) + '\n')
        print(json.dumps(value))
    elif args.action == 'run':
        selected = 'full'
        if not args.full:
            value = json.loads(args.selection.read_text())
            if value.get('schema') != 1 or value.get('mode') not in ('full', 'pr'):
                raise ValueError('Invalid OTA selection')
            # Recompute against current Git/event data; a stale selection cannot
            # weaken coverage after the checkout or event changes.
            if value != selection():
                raise ValueError('OTA selection no longer matches this checkout/event')
            selected = value['mode']
        raise SystemExit(run_tests(selected, args.report))
    else:
        command = args.command[1:] if args.command[:1] == ['--'] else args.command
        if (command[:3] != ['cargo', 'nextest', 'run']
                or any(arg == '-E' or arg.startswith('--filter') for arg in command)):
            raise ValueError('Expected an unfiltered cargo nextest run')
        print('The required OTA job owns the three exhaustive sweeps; all other workspace cases remain selected.', flush=True)
        raise SystemExit(subprocess.run(command + ['-E', workspace_filter(catalog())], cwd=ROOT).returncode)


if __name__ == '__main__':
    main()
