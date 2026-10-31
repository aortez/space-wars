"""LF-076: frozen teacher-initialized polishing comparison.

Arms: the current per-tick recipe against six-tick decision holds, both from
the LF-074 weights with critic warm-up, seeds 17/29/43, exactly the LF-075 warm
cells. Training must reproduce the LF-075 exports bit for bit; the new evidence
is the broader evaluation and the gates declared here before any run.
"""
import argparse
import json
from pathlib import Path
import random

import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from . import aligned, pilot, single_speed, teacher, velocity
from .model import MotorModel

KIND = 'motor-teacher-polish-v1'
ARMS = {'current': 'current', 'hold6': 'repeat'}
SEEDS = pilot.SEEDS
RANDOM_TASK_SEED = 7600001
RANDOM_TASKS = 200
POST_ACQUISITION_LIMIT = .5
QUALIFYING_SEEDS = 2
PILOTS = 'data/training-experiments/learned-flight-motor-learning-signal-pilots-v1-20261025'
SOURCES = ['tools/flight_motor/pilot.py', 'tools/flight_motor/polish.py', 'tools/flight_motor/teacher.py', 'tools/flight_motor/model.py',
           'tools/flight_ppo/ppo.py', 'tools/flight_ppo/model.py', 'tools/flight_ppo/bridge.py']


def random_rows(seed=RANDOM_TASK_SEED, count=RANDOM_TASKS):
    """Held-out random zero-gravity tasks never used for fitting or training."""
    rng = random.Random(seed)
    return [{'family': 'random', 'index': i, 'mode': None, 'level': None, 'direction': None, 'task': teacher.random_task(rng)} for i in range(count)]


def measures(speed16, aligned_results, random_results, lateral_results=()):
    """Declared beat-the-teacher measures and physical failure counts over the evaluation sets."""
    def mean(values):
        return sum(values) / len(values) if values else None
    tracking = [r for r in speed16 if r['family'] == 'track']
    braking = [r for r in speed16 if r['family'] == 'brake' and r['level'] > 0]
    lateral_results = list(lateral_results)
    everything = speed16 + aligned_results + random_results + lateral_results
    return {'tracking_passed': sum(r['outcome'] == 'success' for r in tracking), 'tracking_cases': len(tracking),
            'stopping_passed': sum(r['outcome'] == 'success' for r in speed16 if r['family'] != 'track'),
            'stopping_cases': sum(r['family'] != 'track' for r in speed16),
            'aligned_passed': sum(r['outcome'] == 'success' for r in aligned_results), 'aligned_cases': len(aligned_results),
            'lateral_passed': sum(r['outcome'] == 'success' for r in lateral_results), 'lateral_cases': len(lateral_results),
            'random_passed': sum(r['outcome'] == 'success' for r in random_results), 'random_cases': len(random_results),
            'mean_tracking_acquisition': mean([r['first_success_tick'] for r in tracking if r['first_success_tick'] is not None]),
            'tracking_acquired': sum(r['first_success_tick'] is not None for r in tracking),
            'mean_braking_acquisition': mean([r['first_success_tick'] for r in braking if r['first_success_tick'] is not None]),
            'braking_acquired': sum(r['first_success_tick'] is not None for r in braking),
            'max_post_acquisition_velocity_error': max(r['max_post_acquisition_velocity_error'] for r in tracking),
            'development_physical_failures': sum(r['outcome'] not in ['success', 'deadline'] for r in speed16 + aligned_results + lateral_results),
            'random_physical_failures': sum(r['outcome'] not in ['success', 'deadline'] for r in random_results),
            'physical_failures': sum(r['outcome'] not in ['success', 'deadline'] for r in everything)}


def gates(initial, final):
    """Every gate must pass for a seed to qualify; declared before the runs."""
    checks = {
        'no_development_physical_failure': final['development_physical_failures'] == 0,
        'random_physical_failures_not_increased': final['random_physical_failures'] <= initial['random_physical_failures'],
        'tracking_all_retained': final['tracking_passed'] == final['tracking_cases'],
        'stopping_not_lost': final['stopping_passed'] >= initial['stopping_passed'],
        'aligned_all_retained': final['aligned_passed'] == final['aligned_cases'],
        'lateral_not_lost': final.get('lateral_passed', 0) >= initial.get('lateral_passed', 0),
        'random_not_lost': final['random_passed'] >= initial['random_passed'],
        'faster_tracking_acquisition': final['tracking_acquired'] == final['tracking_cases'] and final['mean_tracking_acquisition'] < initial['mean_tracking_acquisition'],
        'faster_braking_acquisition': final['braking_acquired'] >= initial['braking_acquired'] and final['mean_braking_acquisition'] < initial['mean_braking_acquisition'],
        'post_acquisition_error_bounded': final['max_post_acquisition_velocity_error'] <= POST_ACQUISITION_LIMIT,
    }
    return {'checks': checks, 'passed': all(checks.values())}


def select(arms):
    """Arm with more qualifying seeds; ties go to the lower mean tracking acquisition over its seeds."""
    qualified = {name: a for name, a in arms.items() if a['qualifying_seeds'] >= QUALIFYING_SEEDS}
    if not qualified:
        return None
    return min(qualified, key=lambda name: (-qualified[name]['qualifying_seeds'], qualified[name]['mean_tracking_acquisition']))


def evaluate_policy(bridge, model, policy, out, hold, speed16_results, lateral=False):
    out.mkdir(); bridge.call('load_policy', path=str(policy.resolve()))
    aligned_results = teacher.evaluate(bridge, model, aligned.candidates(teacher.DEVELOPMENT_TASK_SEED, True), out / 'aligned', hold)
    random_results = teacher.evaluate(bridge, model, random_rows(), out / 'random', hold)
    lateral_results = teacher.evaluate(bridge, model, velocity.candidates(teacher.DEVELOPMENT_TASK_SEED, True), out / 'lateral', hold) if lateral else []
    return measures(speed16_results, aligned_results, random_results, lateral_results)


def reproduces_pilot(out, arm, seed, pilots=PILOTS):
    """Exact tensor equality between this run's final export and the matching reference cell, or None without a reference."""
    if pilots is None:
        return None
    mine = json.loads((out / 'policy-final.json').read_text())['weights']
    theirs = json.loads((Path(pilots) / ('warm-%s-seed-%d' % (ARMS[arm], seed)) / 'policy-final.json').read_text())['weights']
    return mine.keys() == theirs.keys() and all(mine[k]['values'] == theirs[k]['values'] for k in mine)


def run_cell(root, arm, seed, bridge_path, warm_checkpoint, pilots=PILOTS, lateral=False, steering_anchor=0., pools='speed16'):
    c = pilot.config(seed, 'warm', ARMS[arm], steering_anchor=steering_anchor, pools=pools); out = root / ('%s-seed-%d' % (arm, seed))
    pilot.run(c, out, bridge_path, warm_checkpoint)
    with Bridge(bridge_path) as bridge:
        model = MotorModel(bridge.contract['schema'])
        results = {}
        for stage in ['initial', 'final']:
            model.load_state_dict(torch.load(out / 'checkpoint-final.pt', weights_only=True)['state']) if stage == 'final' else None
            if stage == 'initial':
                model.load_state_dict(torch.load(warm_checkpoint, weights_only=True)['state'])
            speed16 = json.loads((out / ('evaluation-%s' % stage) / 'results.json').read_text())
            results[stage] = evaluate_policy(bridge, model, out / ('policy-%s.json' % stage), out / ('extended-%s' % stage), c['hold'], speed16, lateral)
    cell = {'arm': arm, 'seed': seed, 'hold': c['hold'], 'initial': results['initial'], 'final': results['final'],
            'gates': gates(results['initial'], results['final']), 'reproduces_pilot': reproduces_pilot(out, arm, seed, pilots)}
    atomic_json(out / 'cell.json', cell)
    print(json.dumps({k: cell[k] for k in ['arm', 'seed', 'reproduces_pilot']}), cell['gates'], flush=True)
    return cell


def freeze(root, bridge_path, warm_checkpoint, arms=None, pilots=PILOTS, lateral=False, kind=KIND, steering_anchor=0., pools='speed16'):
    root.mkdir(parents=True, exist_ok=False)
    arms = {name: ARMS[name] for name in (arms or ARMS)}
    plan = {'kind': kind, 'arms': arms, 'seeds': SEEDS, 'random_task_seed': RANDOM_TASK_SEED, 'random_tasks': RANDOM_TASKS,
            'post_acquisition_limit': POST_ACQUISITION_LIMIT, 'qualifying_seeds': QUALIFYING_SEEDS, 'lateral_set': lateral,
            'steering_anchor': steering_anchor, 'pools': pools,
            'configs': {arm: pilot.config(SEEDS[0], 'warm', recipe, steering_anchor=steering_anchor, pools=pools) for arm, recipe in arms.items()},
            'gates': ['no_development_physical_failure', 'random_physical_failures_not_increased', 'tracking_all_retained', 'stopping_not_lost',
                      'aligned_all_retained', 'lateral_not_lost', 'random_not_lost', 'faster_tracking_acquisition', 'faster_braking_acquisition',
                      'post_acquisition_error_bounded'],
            'selection': 'arm with more qualifying seeds; ties to the lower mean tracking acquisition tick over its seeds',
            'verification': ('each cell reproduces the reference cell export exactly; ' if pilots else 'no reference export; ') + 'native/Python parity on every first input',
            'sources': {s: sha(s) for s in SOURCES}, 'bridge_sha256': sha(bridge_path), 'warm_checkpoint_sha256': sha(warm_checkpoint),
            'reference_pilots': pilots,
            'pilot_exports': {'%s-seed-%d' % (recipe, seed): sha(Path(pilots) / ('warm-%s-seed-%d' % (recipe, seed)) / 'policy-final.json')
                              for recipe in arms.values() for seed in SEEDS} if pilots else {}}
    atomic_json(root / 'plan.json', plan)
    return plan


def summarize(root):
    plan = json.loads((root / 'plan.json').read_text()); cells = []; plan_arms = plan['arms']
    for arm in plan_arms:
        for seed in SEEDS:
            path = root / ('%s-seed-%d' % (arm, seed)) / 'cell.json'
            if path.exists():
                cells.append(json.loads(path.read_text()))
    arms = {}
    for arm in plan_arms:
        mine = [c for c in cells if c['arm'] == arm]
        arms[arm] = {'cells': len(mine), 'qualifying_seeds': sum(c['gates']['passed'] for c in mine),
                     'reproduced': all(c['reproduces_pilot'] is not False for c in mine) and len(mine) == len(SEEDS),
                     'mean_tracking_acquisition': sum(c['final']['mean_tracking_acquisition'] or 900 for c in mine) / max(len(mine), 1)}
    summary = {'kind': plan['kind'], 'cells': cells, 'arms': arms, 'selected_arm': select(arms) if len(cells) == len(plan_arms) * len(SEEDS) else None,
               'verification_passed': all(a['reproduced'] for a in arms.values()), 'sources_unchanged': all(sha(s) == h for s, h in plan['sources'].items()),
               'confirmation_used': False, 'model_promoted': False, 'navigation_qualified': False}
    atomic_json(root / 'summary.json', summary)
    return summary


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root', type=Path, required=True); p.add_argument('--freeze', action='store_true'); p.add_argument('--summarize', action='store_true')
    p.add_argument('--arm', choices=list(ARMS)); p.add_argument('--seed', type=int, choices=SEEDS)
    p.add_argument('--bridge', default=teacher.DEFAULT_BRIDGE); p.add_argument('--warm-checkpoint', default=pilot.WARM_CHECKPOINT)
    p.add_argument('--arms', nargs='+', choices=list(ARMS)); p.add_argument('--no-reference', action='store_true', help='no LF-075 export to reproduce')
    p.add_argument('--lateral', action='store_true', help='also evaluate and gate the LF-055 matrix with lateral starts and 60-degree turns')
    p.add_argument('--kind', default=KIND); p.add_argument('--steering-anchor', type=float, default=0.); p.add_argument('--pools', choices=pilot.POOLS, default='speed16')
    args = p.parse_args()
    if args.freeze:
        print(json.dumps(freeze(args.root, args.bridge, args.warm_checkpoint, args.arms, None if args.no_reference else PILOTS, args.lateral, args.kind,
                                args.steering_anchor, args.pools)['sources'], indent=1)); return
    if args.summarize:
        s = summarize(args.root)
        for c in s['cells']:
            f = c['final']; i = c['initial']
            print('%-7s seed %2d  reproduced %s  gates %s  tracking %d/%d  stopping %d/%d  aligned %d/%d  lateral %d -> %d of %d  random %d -> %d  acq track %.1f -> %.1f  brake %.1f -> %.1f  post %.2f -> %.2f  failures %d' % (
                c['arm'], c['seed'], c['reproduces_pilot'], 'PASS' if c['gates']['passed'] else 'fail:' + ','.join(k for k, v in c['gates']['checks'].items() if not v),
                f['tracking_passed'], f['tracking_cases'], f['stopping_passed'], f['stopping_cases'], f['aligned_passed'], f['aligned_cases'],
                i.get('lateral_passed', 0), f.get('lateral_passed', 0), f.get('lateral_cases', 0), i['random_passed'], f['random_passed'],
                i['mean_tracking_acquisition'], f['mean_tracking_acquisition'], i['mean_braking_acquisition'] or 0, f['mean_braking_acquisition'] or 0,
                i['max_post_acquisition_velocity_error'], f['max_post_acquisition_velocity_error'], f['physical_failures']))
        print('arms', json.dumps(s['arms']), '| selected', s['selected_arm'], '| verification', s['verification_passed'], '| sources unchanged', s['sources_unchanged']); return
    plan = json.loads((args.root / 'plan.json').read_text())
    if any(sha(s) != h for s, h in plan['sources'].items()):
        raise ValueError('source changed after freeze')
    run_cell(args.root, args.arm, args.seed, args.bridge, args.warm_checkpoint, plan.get('reference_pilots'), plan.get('lateral_set', False),
             plan.get('steering_anchor', 0.), plan.get('pools', 'speed16'))


if __name__ == '__main__':
    main()
