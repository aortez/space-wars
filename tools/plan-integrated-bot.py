#!/usr/bin/env python3
"""Freeze the first integrated bot comparison, or check configuration startup.

This tool does not run the evaluation matrix. `smoke` runs one simulated second
per configuration, outside the evaluation seeds, and checks the reported flags.
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import shutil
import subprocess


ROOT = Path(__file__).resolve().parents[1]
CANDIDATE = 'mission_execution_candidate_v1'
CONTROL = 'mission_value_control_v1'
HOST = 'shared_execution_routes_v1'
OPPONENTS = (9, 10, 16)
SEAT_OPTIONS = ('admit-flag-costs', 'survey-current-neutral',
                'destination-retry-seats', 'powered-capture-seats')
DISABLED_SEAT_OPTIONS = (
    'bounded-acquisition-seats', 'cover-retry-seats', 'cover-response-seats',
    'initial-cover-seats', 'covered-request-handoff-seats',
    'actual-route-recovery-seats', 'capture-escape-seats', 'escape-travel-seats',
    'transfer-approach-seats', 'transfer-speed-seats', 'pursuit-health-seats',
    'disengagement-seats',
)
ROUTE_OPTIONS = ('reuse-objective-ground', 'early-objective-routes',
                 'focused-objective-routes', 'requested-objective-routes',
                 'extended-objective-routes', 'cover-walk-feedback',
                 'cover-walk-bounds', 'powered-objective-routes')
MODELS = dict(flag_cost_model='capture_value_published_flags_v1',
              current_neutral_model='capture_value_current_neutral_v1',
              destination_retry_model='destination_failure_context_v1')
PROFILES = dict(powered_capture='mission_powered_capture_v1',
                active_flight_checks='vehicle_flight_continuation_v1',
                destination_retry='destination_failure_context_v1')


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def seed(namespace):
    return int.from_bytes(hashlib.sha256(namespace.encode()).digest()[:8], 'little')


def flags(item):
    """One value per option; never append overrides to the first-wins parser."""
    seat, candidate = item['seat'], item['arm'] == 'candidate'
    if seat not in (0, 1) or item['arm'] not in ('candidate', 'control'):
        raise ValueError('expected seat 0/1 and candidate/control arm')
    if item['opponent'] not in OPPONENTS:
        raise ValueError('opponent is outside the frozen comparison')
    policies = [f"material_mission_v{item['opponent']}"] * 2
    policies[seat] = 'material_mission_v13'
    result = {
        '--world': item['world'], '--seed': str(item['seed']),
        '--seconds': str(item['seconds']), '--match': 'true',
        '--mode': 'quiet' if item['world'] == 'value-destination' else 'duel',
        '--seat': str(seat) if item['world'] == 'value-destination' else '0',
        '--mirror': str(item.get('mirror', False)).lower(),
        '--asteroid-interval': str(item['interval']),
        '--require-finish': str(item['stage'] == 'held_out'
                                or item['seconds'] == 600).lower(),
        '--p1-policy': policies[0], '--p2-policy': policies[1],
        '--break-interval': '15', '--break-duration': '4',
        '--landing-survey-hz': '4', '--live-objective-planning': 'true',
        '--live-objective-seats': 'both', '--objective-graph-budget': '4',
        '--objective-query-budget': '384', '--objective-dependencies': 'routes',
        '--mission-evaluation-budget': '4', '--evaluate-missions': 'true',
        '--survey-capture-alternative': 'true', '--survey-capture-flags': 'true',
        '--measure-mission-progress': 'true', '--trace': 'true',
        '--trace-capture-evidence': 'true', '--trace-destination-behavior': 'true',
        '--active-flight-checks': str(candidate).lower(),
        '--probe-projectile-response': 'none', '--probe-successors': 'false',
        '--probe-destination-cover': 'false', '--probe-disengagement-handoff': 'false',
        '--disengagement-boundary': 'false',
    }
    result.update((f'--{name}', 'true') for name in ROUTE_OPTIONS)
    result.update((f'--{name}', str(seat) if candidate else 'none')
                  for name in SEAT_OPTIONS)
    result.update((f'--{name}', 'none') for name in DISABLED_SEAT_OPTIONS)
    if 'bearing' in item:
        result['--flag-bearing'] = str(item['bearing'])
    return result


def paired(group, order=0, **kwargs):
    arms = ('control', 'candidate') if order % 2 == 0 else ('candidate', 'control')
    return [dict(name=f'{group}-{arm}', group=group, arm=arm, **kwargs) for arm in arms]


def plan():
    cases = []
    # Known successes and failures qualify the new combination; not held-out wins.
    for seat, bearing in itertools.product((0, 1), (-0.8, 0.8)):
        cases += paired(f'known-destination-p{seat+1}-bearing{bearing}',
                        stage='qualification', world='value-destination', seat=seat,
                        opponent=10, seed=42, interval=0, seconds=180,
                        mirror=seat == 1, bearing=bearing)
    for world, seat in itertools.product(range(2), (0, 1)):
        namespace = f'powered-mission-integration-v1:{world}'
        cases += paired(f'known-powered-world{world}-p{seat+1}', order=world+seat,
                        stage='qualification', world='generated', seat=seat,
                        opponent=10, seed=seed(namespace), seed_namespace=namespace,
                        interval=0, seconds=600)
    # Four world clusters, not 48 independent samples. Quiet means no asteroids;
    # both pilots remain armed and use the duel controller in every fresh match.
    for world, opponent, interval, seat in itertools.product(range(4), OPPONENTS, (0, 3), (0, 1)):
        namespace = f'{CANDIDATE}:held-out:{world}'
        cases += paired(f'fresh-world{world}-v{opponent}-asteroids{interval}-p{seat+1}',
                        order=world+opponent+interval+seat, stage='held_out',
                        world='generated', seat=seat, opponent=opponent,
                        seed=seed(namespace), seed_namespace=namespace,
                        interval=interval, seconds=600)
    return cases


def smoke_plan():
    return [case for opponent, seat in itertools.product(OPPONENTS, (0, 1))
            for case in paired(f'smoke-v{opponent}-p{seat+1}', stage='smoke',
                               world='generated', seat=seat, opponent=opponent,
                               seed=42, interval=0, seconds=1)]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def check_configuration(report, item):
    """Check consumed configuration, not merely our requested command line."""
    require(report['physics_ok'] and not report['audit_failures'], 'physics audit failed')
    candidate, seat = item['arm'] == 'candidate', item['seat']
    enabled = [candidate and s == seat for s in (0, 1)]
    expected = flags(item)
    descriptors = report['policy_configuration']
    require(len(descriptors) == 2, 'expected two policy descriptors')
    for s, descriptor in enumerate(descriptors):
        require(descriptor['policy'] == expected[f'--p{s+1}-policy'], 'wrong seat policy')
        require(('jetpack' in descriptor['sensor_profile']) == enabled[s],
                'wrong powered sensor profile')
        for key, model in MODELS.items():
            require(descriptor.get(key) == (model if enabled[s] else None), f'wrong {key} seat/model')
        require('cover_response_model' not in descriptor, 'cover response unexpectedly enabled')
        require('cover_retry_model' not in descriptor, 'cover retry unexpectedly enabled')
    for key, profile in PROFILES.items():
        require(report.get(key, {}).get('enabled_seats', [False, False]) == enabled,
                f'wrong {key} configuration')
        require(report.get(key, {}).get('profile') == (profile if candidate else None),
                f'wrong {key} profile')
    evaluation = report['mission_evaluation']
    for key in ('flag_cost_admission', 'current_neutral_survey'):
        require(evaluation.get(key, {}).get('enabled_seats', [False, False]) == enabled,
                f'wrong {key} configuration')
    for key in ('cover_response', 'cover_retry_cooldown', 'bounded_acquisition',
                'initial_cover', 'actual_route_recovery', 'capture_escape',
                'escape_travel', 'transfer_approach', 'transfer_speed', 'pursuit_health',
                'projectile_response'):
        require(key not in report, f'{key} unexpectedly enabled')
    require(report['pursuit_disengagement'] == dict(
        enabled_seats=[False, False], probe_handoff=False,
        boundary_guidance=False, destination_cover_probe=False),
        'disengagement or its probes unexpectedly enabled')
    live = report['live_objective_planning']
    require(live['enabled_seats'] == [0, 1], 'wrong live planner seats')
    require(live['allowance'] == dict(graph=4, physics_queries=384), 'wrong shared quota')
    require(live['objective_dependencies'] == 'routes', 'wrong dependency mode')
    for name in ROUTE_OPTIONS:
        require(live.get(name.replace('-', '_')) is True, f'{name} not enabled')
    require('covered_request_handoff' not in live, 'covered handoff unexpectedly enabled')
    require('actual_failure_feedback' not in live, 'actual failure feedback unexpectedly enabled')
    require(report['seed'] == item['seed'], 'wrong seed')
    require(report['combat_breaks'] == dict(interval_seconds=15, duration_seconds=4),
            'wrong combat break configuration')
    require(report['landing_survey_hz'] == 4, 'wrong landing survey cadence')
    require(report['mode'] == expected['--mode'], 'wrong control mode')
    require(report['seat'] == int(expected['--seat']), 'wrong reporting seat')
    if item['stage'] == 'smoke':
        require(report['elapsed_ticks'] == 60, 'smoke must run exactly one simulated second')
    if expected['--require-finish'] == 'true':
        require(report['termination'] == 'round_finished', 'unfinished match')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('plan', 'smoke'))
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    require(not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT,
                                        text=True).strip(), 'commit the configuration and plan first')
    source_binary = args.binary.resolve(strict=True)
    out = args.out.resolve()
    binary_hash = digest(source_binary)
    out.mkdir(parents=True, exist_ok=False)
    binary = out / 'surface_mission_soak'
    shutil.copy2(source_binary, binary)
    require(digest(binary) == binary_hash, 'binary changed while freezing the plan')
    items = plan() if args.mode == 'plan' else smoke_plan()
    jobs = []
    for item in items:
        configuration = flags(item)
        argv = [part for pair in configuration.items() for part in pair]
        identities = [f"retained_v{item['opponent']}"] * 2
        identities[item['seat']] = CANDIDATE if item['arm'] == 'candidate' else CONTROL
        jobs.append(dict(item=item, configuration_by_seat=identities, host=HOST,
                         command=[str(binary), *argv, '--out', str(out / item['name'])]))
    record = dict(schema=1, purpose=args.mode, candidate=CANDIDATE, control=CONTROL,
                  host=HOST, source_commit=subprocess.check_output(
                      ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                  binary=dict(path=str(binary), source_path=str(source_binary), sha256=binary_hash),
                  inputs={str(p.relative_to(ROOT)): digest(p) for p in (
                      Path(__file__).resolve(), ROOT / 'docs/integrated-bot-candidate.md')},
                  cases=jobs, results={})
    (out / 'plan.json').write_text(json.dumps(record, indent=2, allow_nan=False) + '\n')
    print(f'Frozen {len(jobs)} {args.mode} cases: {out / "plan.json"}', flush=True)
    if args.mode == 'plan':
        return
    try:
        for job in jobs:
            item = job['item']
            with (out / f'{item["name"]}.log').open('x') as log:
                subprocess.run(job['command'], check=True, stdout=log, stderr=log, timeout=120)
            path = out / item['name'] / 'report.json'
            check_configuration(json.loads(path.read_text()), item)
            record['results'][item['name']] = dict(configuration_verified=True, report_sha256=digest(path))
            print(item['name'] + ': configuration verified', flush=True)
        require(digest(binary) == record['binary']['sha256'], 'binary changed during smoke')
        record['complete'] = True
    finally:
        (out / 'smoke-summary.json').write_text(json.dumps(record, indent=2, allow_nan=False) + '\n')


if __name__ == '__main__':
    main()
