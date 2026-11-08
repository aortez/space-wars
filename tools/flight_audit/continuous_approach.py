"""Frozen first continuous-control experiment: inspection, secondary evaluation, gates."""
import argparse
from collections import Counter
import gzip
import json
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.evaluate import evaluate
from tools.flight_ppo.execution import DETERMINISTIC, SAMPLED, episode_seed, load_export
from tools.flight_ppo.model import actions, tensor
from tools.flight_ppo.tests import equal_tree

SEEDS = [17, 29, 43]
SAMPLE_SPEC = {'mode': SAMPLED, 'repeats': 4, 'seed_base': 5310000}
RECIPES = [f'aligned-approach-v1/stratum-{i}' for i in range(4)]
CRITERIA = {'position': 3., 'velocity': 1., 'spin': 0.20000000298023224, 'dwell_ticks': 30}


def read(path):
    with (gzip.open if path.suffix == '.gz' else open)(path, 'rt') as stream:
        return json.load(stream)


def trace_metrics(trace, allow_active=False):
    if trace['version'] != 2 or trace['task']['arrival'] != CRITERIA or trace['task']['deadline_ticks'] != 1800:
        raise ValueError('wrong physical contract')
    steps = trace['steps']
    if not steps or len(steps) > 1800:
        raise ValueError('invalid trace length')
    def tolerated(frame):
        e = frame['errors']
        return e['position'] <= 3 and e['velocity'] <= 1 and abs(e['spin']) <= CRITERIA['spin']
    previous = trace['initial'];dwell = 0
    for tick, step in enumerate(steps, 1):
        frame = step['frame'];action = step['action'];progress = step['progress'];evidence = step['evidence']
        if frame is None or action is None or step.get('error'):
            raise ValueError('invalid controller/environment trace')
        for key, low in [('steering', -1), ('thrust', 0), ('brake', 0)]:
            if type(action[key]) not in (int, float) or not math.isfinite(action[key]) or not low <= action[key] <= 1:
                raise ValueError('non-continuous or invalid actuator')
        if not all(math.isfinite(v) for v in frame['errors'].values()):
            raise ValueError('nonfinite physical state')
        dwell = dwell+1 if tolerated(previous) and tolerated(frame) else 0
        failures = [k for k in ['ship_lost', 'collision', 'damage', 'boundary_escape'] if evidence[k]]
        outcome = failures[0] if failures else 'success' if dwell >= 30 else 'deadline' if tick == 1800 else None
        if progress != {'elapsed_ticks': tick, 'dwell_ticks': dwell, 'outcome': outcome}:
            raise ValueError('physical outcome/dwell reconstruction differs')
        if outcome is not None and tick != len(steps):
            raise ValueError('trace advances after terminal')
        previous = frame
    outcome = steps[-1]['progress']['outcome']
    if outcome is None and not allow_active:
        raise ValueError('incomplete evaluated episode')
    near = [s for s in steps if s['frame']['errors']['position'] <= 3]
    return {'outcome': outcome, 'ticks': len(steps), 'entered_goal_radius': bool(near),
        'met_all_tolerances': any(tolerated(s['frame']) for s in steps),
        'maximum_dwell_ticks': max(s['progress']['dwell_ticks'] for s in steps),
        'minimum_distance': min(s['frame']['errors']['position'] for s in steps),
        'final_errors': steps[-1]['frame']['errors'],
        'maximum_speed': max(s['frame']['errors']['velocity'] for s in steps),
        'maximum_spin': max(abs(s['frame']['errors']['spin']) for s in steps),
        'controls': {key: {'sum': sum(s['action'][key] for s in steps),
            'abs_sum': sum(abs(s['action'][key]) for s in steps),
            'minimum': min(s['action'][key] for s in steps),
            'maximum': max(s['action'][key] for s in steps),
            'near_zero_ticks': sum(abs(s['action'][key]) <= .05 for s in steps),
            'near_full_ticks': sum(abs(s['action'][key]) >= .95 for s in steps)}
            for key in ['steering', 'thrust', 'brake']},
        'simultaneous_demand_ticks': sum(s['action']['thrust'] > .05 and s['action']['brake'] > .05 for s in steps),
        'near_ticks': len(near), 'near_speed_compliant_ticks': sum(s['frame']['errors']['velocity'] <= 1 for s in near),
        'near_spin_compliant_ticks': sum(abs(s['frame']['errors']['spin']) <= CRITERIA['spin'] for s in near)}


def combine(traces):
    ticks = sum(t['ticks'] for t in traces);near = sum(t['near_ticks'] for t in traces)
    return {'attempts': len(traces), 'ticks': ticks,
        'outcomes': dict(Counter(t['outcome'] or 'budget_censored' for t in traces)),
        'entered_goal_radius': sum(t['entered_goal_radius'] for t in traces),
        'met_all_tolerances': sum(t['met_all_tolerances'] for t in traces),
        'maximum_dwell_ticks': max(t['maximum_dwell_ticks'] for t in traces),
        'maximum_speed': max(t['maximum_speed'] for t in traces),
        'maximum_spin': max(t['maximum_spin'] for t in traces),
        'controls': {key: {'mean': sum(t['controls'][key]['sum'] for t in traces)/ticks,
            'mean_abs': sum(t['controls'][key]['abs_sum'] for t in traces)/ticks,
            'minimum': min(t['controls'][key]['minimum'] for t in traces),
            'maximum': max(t['controls'][key]['maximum'] for t in traces),
            'near_zero_fraction': sum(t['controls'][key]['near_zero_ticks'] for t in traces)/ticks,
            'near_full_fraction': sum(t['controls'][key]['near_full_ticks'] for t in traces)/ticks}
            for key in ['steering', 'thrust', 'brake']},
        'simultaneous_demand_fraction': sum(t['simultaneous_demand_ticks'] for t in traces)/ticks,
        'near_speed_compliance': sum(t['near_speed_compliant_ticks'] for t in traces)/near if near else None,
        'near_spin_compliance': sum(t['near_spin_compliant_ticks'] for t in traces)/near if near else None,
        'mean_minimum_distance': sum(t['minimum_distance'] for t in traces)/len(traces),
        'mean_final_distance': sum(t['final_errors']['position'] for t in traces)/len(traces)}


def physical(rows, seed, sampled=False):
    repeats = 4 if sampled else 1
    if len(rows) != 16*repeats:
        raise ValueError('incorrect episode count')
    identities = None;counts = []
    for repeat in range(repeats):
        group = [r for r in rows if r.get('repeat', 0) == repeat]
        identity = {(r['index'], r['recipe']) for r in group}
        if len(group) != 16 or len(identity) != 16 or len({r['index'] for r in group}) != 16:
            raise ValueError('duplicate or missing task')
        if identities is not None and identity != identities:
            raise ValueError('different tasks across repeats')
        identities = identity
        if any(sum(r['recipe'] == recipe for r in group) != 4 for recipe in RECIPES):
            raise ValueError('incorrect strata')
        counts.append(sum(r['outcome'] == 'success' for r in group))
        for row in group:
            if row['outcome'] not in ['success', 'deadline', 'collision', 'damage', 'ship_lost', 'boundary_escape']:
                raise ValueError('incomplete or invalid evaluation')
            if sampled and (row['execution_mode'] != SAMPLED or row['rng_seed'] != episode_seed(SAMPLE_SPEC, seed, repeat, row['index'])):
                raise ValueError('incorrect execution seed/mode')
    return {'attempts': len(rows), 'successes': sum(counts), 'successes_by_repeat': counts,
        'outcomes': dict(Counter(r['outcome'] for r in rows)),
        'physical_failures': sum(r['outcome'] not in ['success', 'deadline'] or r['collision'] or r['damage'] for r in rows),
        'successes_by_stratum': [sum(r['recipe'] == recipe and r['outcome'] == 'success' for r in rows) for recipe in RECIPES]}


def comparison(initial, final, seed, sampled=False):
    before, after = physical(initial, seed, sampled), physical(final, seed, sampled)
    keyed = [{(r.get('repeat', 0), r['index']): r for r in rows} for rows in [initial, final]]
    if keyed[0].keys() != keyed[1].keys():raise ValueError('different paired tasks')
    paired = Counter()
    for key, old in keyed[0].items():
        new = keyed[1][key]
        if any(old.get(k) != new.get(k) for k in ['recipe', 'initial_errors', 'rng_seed']):
            raise ValueError('incompatible paired episode')
        a, b = old['outcome'] == 'success', new['outcome'] == 'success'
        paired['retained' if a and b else 'lost' if a else 'gained' if b else 'neither'] += 1
    gain = after['successes']-before['successes']
    result = {'initial': before, 'final': after, 'gain': gain,
              'paired': {k: paired[k] for k in ['retained', 'gained', 'lost', 'neither']}}
    if not sampled:
        result.update(learning_evidence=after['successes'] >= 4 and gain >= 4,
            scoped_progression=after['successes'] >= 12 and gain >= 8 and after['physical_failures'] == 0
                and min(after['successes_by_stratum']) >= 2)
    return result


def aggregate(records):
    if sorted(r['seed'] for r in records) != SEEDS:raise ValueError('three declared seeds required')
    complete = all(r['status'] == 'completed' for r in records)
    return {'all_runs_completed': complete,
        'learning_evidence': complete and sum(r['mean']['learning_evidence'] for r in records) >= 2,
        'scoped_progression': complete and sum(r['mean']['scoped_progression'] for r in records) >= 2}


def training_inspection(run, update):
    state = torch.load(run/f'checkpoint-{update:04}.pt', weights_only=True)
    summary = read(run/'summary.json')
    traces = [trace_metrics(read(p)) for p in sorted((run/'training-traces').glob('episode-*.json'))]
    traces += [trace_metrics(t, allow_active=True) for t in state['collector']['traces'] if t['steps']]
    result = combine(traces)
    assert result['ticks'] == summary['environment_steps'] == update*1024
    assert sum(e['ticks'] for e in summary['completed_episodes']+summary['unfinished_episodes']) == result['ticks']
    assert all(math.isfinite(v) for h in summary['history'] for v in h.values() if isinstance(v, float))
    result['started_episodes'] = summary['started_episodes']
    result['censored_episodes'] = len(summary['unfinished_episodes'])
    result['learned_latent_std'] = state['model']['log_std'].clamp(-5, 2).exp().tolist()
    return result


def evaluation_inspection(directory, seed, sampled):
    rows = read(directory/'results.json');physical(rows, seed, sampled)
    metadata = read(directory/'metadata/execution.json')
    assert metadata['specification'] == (SAMPLE_SPEC if sampled else {'mode': DETERMINISTIC, 'repeats': 1})
    traces = []
    for row in rows:
        folder = directory/f"repeat-{row['repeat']}" if sampled else directory
        stem = f"{row['family']}-{row['index']}"
        trace = read(folder/f'{stem}.json');metrics = trace_metrics(trace)
        assert metrics['outcome'] == row['outcome'] and metrics['ticks'] == row['ticks']
        assert metrics['final_errors'] == row['final_errors'] and trace['initial']['errors'] == row['initial_errors']
        for flag in ['collision', 'damage']:
            assert row[flag] == any(s['evidence'][flag] for s in trace['steps'])
        if sampled:
            controls = read(folder/'policy'/f'{stem}.json.gz')
            assert len(controls) == len(trace['steps'])
            z = torch.tensor([r['latent'] for r in controls])
            decoded = actions(z[:, 0], z[:, 1:])
            for record, command, step in zip(controls, decoded, trace['steps']):
                assert record['action'] == step['action']
                assert max(abs(command[k]-step['action'][k]) for k in command) <= 2e-7
        traces.append(dict(metrics, index=row['index'], recipe=row['recipe'], repeat=row.get('repeat', 0)))
    return {'summary': combine(traces), 'episodes': traces}


def parity(executable, run):
    with Bridge(executable) as bridge, torch.random.fork_rng(devices=[]):
        model = load_export(run/'policy-0128.json', bridge.contract['schema'])
        candidates = [r['candidate'] for r in read(run/'development-admission.json')['attempts'] if r['admitted']]
        items = [bridge.call('reset', tasks=[c['task']])['items'][0] for c in candidates]
        bridge.call('load_policy', path=str((run/'policy-0128.json').resolve()))
        native = bridge.call('infer', inputs=[item['inputs'] for item in items])['outputs']
        with torch.no_grad():actor, value = model(tensor(items))
        expected = torch.cat((actor, value[:, None]), dim=1)
        actual = torch.tensor([[r[k] for k in ['steering_latent', 'thrust_logit', 'brake_logit', 'value']] for r in native])
        torch.testing.assert_close(actual, expected, atol=1e-5, rtol=1e-5)
        commands = actions(actor[:, 0], actor[:, 1:])
        error = max(abs(a[k]-b['action'][k]) for a,b in zip(commands,native) for k in a)
        assert error <= 1e-5
        return {'fixtures': len(items), 'maximum_raw_error': float((actual-expected).abs().max()), 'maximum_action_error': error}


def main():
    parser = argparse.ArgumentParser();parser.add_argument('--archive', type=Path, required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--preflight', action='store_true');mode.add_argument('--sampled-seed', type=int, choices=SEEDS)
    mode.add_argument('--report', type=Path)
    args = parser.parse_args();a = args.archive
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.preflight:
        result = training_inspection(a/'preflight/seed-17', 2)
        atomic_json(a/'preflight/inspection.json', result);print(json.dumps(result));return
    if args.sampled_seed:
        seed = args.sampled_seed;run = a/f'seed-{seed}';start = time.monotonic();deadline = start+900
        candidates = [r['candidate'] for r in read(run/'development-admission.json')['attempts'] if r['admitted']]
        for phase, update in [('initial', 0), ('final', 128)]:
            evaluate(a/'runtime/flight_ppo_bridge', run/f'policy-{update:04}.json', candidates,
                     run/f'sampled-{phase}', deadline, execution=SAMPLE_SPEC, learner_seed=seed)
        atomic_json(run/'sampled-summary.json', {'seconds': time.monotonic()-start});return
    records = []
    for seed in SEEDS:
        run = a/f'seed-{seed}'
        if not (run/'summary.json').exists() or not (run/'sampled-summary.json').exists():
            records.append({'seed': seed, 'status': 'incomplete_or_failed'});continue
        s = read(run/'summary.json');assert s['completed_updates'] == 128 and s['environment_steps'] == 131072
        assert s['total_seconds'] <= 1800 and read(run/'sampled-summary.json')['seconds'] <= 900
        mean = comparison(s['initial_evaluation'], s['final_evaluation'], seed)
        sampled = comparison(read(run/'sampled-initial/results.json'), read(run/'sampled-final/results.json'), seed, True)
        inspection = {mode: {phase: evaluation_inspection(run/f'{prefix}-{phase}', seed, mode == 'sampled')
                      for phase in ['initial', 'final']} for mode, prefix in [('mean', 'evaluation'), ('sampled', 'sampled')]}
        records.append({'seed': seed, 'status': 'completed', 'mean': mean, 'sampled': sampled,
            'training': training_inspection(run, 128), 'evaluation': inspection, 'parity': parity(a/'runtime/flight_ppo_bridge', run),
            'training_seconds': s['training_seconds_including_checkpoints'], 'total_training_primary_seconds': s['total_seconds'],
            'sampled_seconds': read(run/'sampled-summary.json')['seconds'],
            'optimizer': {'kl_stopped_updates': sum(h['kl_early_stop'] for h in s['history']),
                'steps': sum(h['minibatches'] for h in s['history']),
                'final_metrics': s['history'][-1]}})
    if all(r['status'] == 'completed' for r in records):
        for n in [0,2]:
            assert equal_tree(torch.load(a/f'preflight/seed-17/checkpoint-{n:04}.pt', weights_only=True),
                              torch.load(a/f'seed-17/checkpoint-{n:04}.pt', weights_only=True)), 'preflight/full training mismatch'
        admissions = [read(a/f'seed-{s}/development-admission.json') for s in SEEDS]
        assert admissions[0] == admissions[1] == admissions[2]
    result = {'experiment_id': a.name, 'seeds': records, 'gates': aggregate(records),
        'preflight': read(a/'preflight/inspection.json'), 'matched_preflight_updates': [0,2] if len(records)==3 and all(r['status']=='completed' for r in records) else [],
        'training_ticks': sum(r['training']['ticks'] for r in records if r['status']=='completed'),
        'evaluation_ticks': sum(r['evaluation'][mode][phase]['summary']['ticks'] for r in records if r['status']=='completed' for mode in ['mean','sampled'] for phase in ['initial','final']),
        'confirmation_used': False, 'model_promoted': False, 'general_flight_claim': False}
    assert result['training_ticks'] <= 393216 and result['evaluation_ticks'] <= 864000
    atomic_json(args.report, result)


if __name__ == '__main__':main()
