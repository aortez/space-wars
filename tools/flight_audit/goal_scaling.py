"""Paired function-preserving goal scaling, with physical recovery and hold gates."""
import argparse
import copy
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.evaluate import evaluate
from tools.flight_ppo.initialization import initialize_policy
from tools.flight_ppo.model import FlightModel, SCHEMA_SCALING, GOAL_SCALING
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.goal_response import read
from tools.flight_audit.input_scaling import models, infer
from tools.flight_audit.recovery_response import goal_grid, response, grouped
from tools.flight_audit.hold_position import comparison as hold_comparison
from tools.flight_audit.settle_recover import SEEDS, candidates, physical, inspect_evaluation
from tools.flight_audit.short_recovery import training_evidence, progression, retention
from tools.flight_audit.rate_training import pilot_gate

ARMS = ['schema', 'goal']
SCALINGS = {'schema': SCHEMA_SCALING, 'goal': GOAL_SCALING}
CHANGED_SOURCES = {'model.py', 'initialization.py', 'run.py'}


def assert_control_replay(current, parent):
    """Normalize only declared source identities for audit; resume stays strict."""
    current = copy.deepcopy(current)
    sources = current['contract']['python_sources']; old = parent['contract']['python_sources']
    assert set(sources) == set(old)
    assert {k for k in sources if sources[k] != old[k]} == CHANGED_SOURCES
    current['contract']['python_sources'] = copy.deepcopy(old)
    assert equal_tree(current, parent)


def benefit(records):
    pairs = []
    for seed in SEEDS:
        base = next(r for r in records if r['seed'] == seed and r['arm'] == 'schema')
        goal = next(r for r in records if r['seed'] == seed and r['arm'] == 'goal')
        f = goal['primary']['final']; initial = goal['primary']['initial']; control = base['primary']['final']
        position_gain = f['by_family']['position'] - max(initial['by_family']['position'], control['by_family']['position'])
        overall_gain = f['successes'] - max(initial['successes'], control['successes'])
        nonforward = sum(goal['position_directions']['final'][d] for d in ['left', 'backward', 'right'])
        retained = retention(goal['legacy_hold']); safe = f['physical_failures'] == 0
        pairs.append({'seed': seed, 'position_gain_over_original_and_control': position_gain,
                      'recovery_gain_over_original_and_control': overall_gain, 'nonforward_successes': nonforward,
                      'holding_retained': retained, 'physical_safety': safe,
                      'joint_benefit': position_gain >= 1 and overall_gain >= 1 and nonforward >= 1 and retained and safe})
    return {'per_seed': pairs, 'passed': sum(p['joint_benefit'] for p in pairs) >= 2}


def response_probe(a, seed, model, folded, bridge, out, deadline):
    cases = [{'kind': 'original'}] + [{'kind': 'goal_intervention', **c} for c in goal_grid()]
    records = []; weights = copy.deepcopy(model.state_dict()); rng = torch.get_rng_state().clone()
    for directory in sorted((a/f'inputs/anchors/seed-{seed}').iterdir()):
        if time.monotonic() > deadline: raise TimeoutError('response wall budget')
        anchor = read(directory/'anchor.json'); inputs = read(directory/'inputs.json.gz')
        raw, commands, native, errors = infer(bridge, model, folded, inputs)
        rows = [{'case': c, 'python_raw': r, 'python_action': cmd, 'native': n} for c, r, cmd, n in zip(cases, raw, commands, native)]
        dest = out/directory.name; dest.mkdir(parents=True)
        rec = {'kind': anchor['kind'], 'index': anchor['index'], 'parity': errors, 'summary': response(rows)}
        atomic_json(dest/'outputs.json', rows); atomic_json(dest/'summary.json', rec); records.append(rec)
    assert len(records) == 8 and equal_tree(weights, model.state_dict()) and torch.equal(rng, torch.get_rng_state())
    return {'records': records, 'comparisons': 208,
            'groups': {kind: grouped([r for r in records if r['kind'] == kind]) for kind in ['start', 'later']}}


def preflight(a):
    start = time.monotonic(); deadline = start + 600; records = []
    for seed in SEEDS:
        with Bridge(a/'runtime/flight_ppo_bridge') as bridge, torch.random.fork_rng(devices=[]):
            original = torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-0000.pt', weights_only=True)
            for arm in ARMS:
                config = read(a/f'configs/{arm}/{seed}.json')
                torch.manual_seed(seed); model = FlightModel(bridge.contract['schema'], config['input_scaling'])
                initialize_policy(model, config, a/f'inputs/seed-{seed}/policy.json')
                assert equal_tree(model.inference_state_dict(), original['model'])
                out = a/f'preflight/{arm}/seed-{seed}'; out.mkdir(parents=True)
                policy = out/'policy-0000.json'; model.export(policy)
                assert policy.read_bytes() == (a/f'inputs/seed-{seed}/policy.json').read_bytes()
                from tools.flight_ppo.execution import load_export
                folded = load_export(policy, bridge.contract['schema'])
                bridge.call('load_policy', path=str(policy.resolve()))
                result = response_probe(a, seed, model, folded, bridge, out/'response', deadline)
                assert all(r['parity']['fold_error'] == 0 for r in result['records'])
                records.append({'arm': arm, 'seed': seed, 'response': result})
    result = {'records': records, 'initial_exports_exact': True, 'initial_training_outputs_exact': True,
              'native_python_comparisons': 1248, 'optimizer_steps': 0, 'physics_ticks': 0,
              'seconds': time.monotonic()-start}
    assert result['seconds'] < 600
    atomic_json(a/'preflight-results.json', result)


def parity(a, run, update, fixtures, dest):
    with Bridge(a/'runtime/flight_ppo_bridge') as bridge, torch.random.fork_rng(devices=[]):
        model, folded, _ = models(run, update, bridge.contract['schema'])
        before = copy.deepcopy(model.state_dict()); rng = torch.get_rng_state().clone()
        bridge.call('load_policy', path=str((run/f'policy-{update:04}.json').resolve()))
        items = bridge.call('reset', tasks=[c['task'] for c in fixtures])['items']
        raw, commands, native, errors = infer(bridge, model, folded, [i['inputs'] for i in items])
        assert equal_tree(before, model.state_dict()) and torch.equal(rng, torch.get_rng_state())
        atomic_json(dest, {'items': items, 'python_raw': raw, 'python_actions': commands, 'native': native, 'errors': errors})
        return {'fixtures': len(items), **errors}


def extra(a, arm, seed):
    start = time.monotonic(); deadline = start + 600; run = a/f'pilot/{arm}/seed-{seed}'
    old = [r['candidate'] for r in read(a/'inputs/legacy-admission.json')['attempts'] if r['admitted']]
    parities = {}
    for endpoint, update in [('initial', 0), ('final', 16)]:
        rows = evaluate(a/'runtime/flight_ppo_bridge', run/f'policy-{update:04}.json', old, run/f'legacy-hold-{endpoint}', deadline)
        if endpoint == 'initial':
            for row in rows:
                assert read(run/f"legacy-hold-initial/clear-{row['index']}.json") == read(a/f"inputs/seed-{seed}/hold-baseline/clear-{row['index']}.json")
        parities[endpoint] = parity(a, run, update, candidates(run, 'development'), run/f'parity-{endpoint}.json')
    for row in read(run/'evaluation-initial/results.json'):
        assert read(run/f"evaluation-initial/clear-{row['index']}.json") == read(a/f"inputs/seed-{seed}/recovery-baseline/clear-{row['index']}.json")
    with Bridge(a/'runtime/flight_ppo_bridge') as bridge, torch.random.fork_rng(devices=[]):
        model, folded, _ = models(run, 16, bridge.contract['schema'])
        bridge.call('load_policy', path=str((run/'policy-0016.json').resolve()))
        sensitivity = response_probe(a, seed, model, folded, bridge, run/'response-final', deadline)
    atomic_json(run/'extra-summary.json', {'seconds': time.monotonic()-start, 'parity': parities,
                                         'initial_traces_exact': True, 'response_final': sensitivity})


def verify(a):
    start = time.monotonic(); records = []; restore_ticks = 0; resume_steps = 0
    pre = read(a/'preflight-results.json'); assert pre['initial_exports_exact'] and pre['initial_training_outputs_exact']
    for seed in SEEDS:
        zeros = []; configs = []
        parent_first = torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-0001.pt', weights_only=True)
        for arm in ARMS:
            run = a/f'pilot/{arm}/seed-{seed}'; s = read(run/'summary.json')
            assert s['completed_updates'] == 16 and s['environment_steps'] == 16384 and s['parameter_count'] == 39447 and s['total_seconds'] < 1800
            zero = torch.load(run/'checkpoint-0000.pt', weights_only=True); zeros.append(zero)
            config = zero['contract']['config']; configs.append(copy.deepcopy(config))
            assert config['input_scaling'] == SCALINGS[arm] and config['value_gradient'] == 'head-only-v1'
            assert config['learning_rate'] == .000075 and config['exploration']['std_multiplier'] == .25 and config['exploration']['rho'] == 0
            assert not zero['optimizer']['state']
            assert (run/'policy-0000.json').read_bytes() == (a/f'preflight/{arm}/seed-{seed}/policy-0000.json').read_bytes()
            final = torch.load(run/'checkpoint-0016.pt', weights_only=True)
            again = torch.load(a/f'resumed/{arm}/seed-{seed}/checkpoint-0016.pt', weights_only=True)
            assert equal_tree(final, again)
            restored = torch.load(run/'checkpoint-0015.pt', weights_only=True)
            restore_ticks += sum(len(t['steps']) for t in restored['collector']['traces']); resume_steps += s['history'][-1]['minibatches']
            assert equal_tree(torch.load(run/'checkpoint-0001.pt', weights_only=True)['collector'], parent_first['collector'])
            if arm == 'schema':
                for update in [0, 1, 16]:
                    assert_control_replay(torch.load(run/f'checkpoint-{update:04}.pt', weights_only=True),
                                          torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-{update:04}.pt', weights_only=True))
                for suite, folder in [('primary', 'evaluation-final'), ('hold', 'legacy-hold-final')]:
                    assert read(run/f'{folder}/results.json') == read(a/f'inputs/seed-{seed}/parent-{suite}-final.json')
            for partition in ['training', 'development']:
                assert read(run/f'{partition}-admission.json') == read(a/f'inputs/{partition}-admission.json')
            evaluation = {suite: {phase: inspect_evaluation(run/f'{folder}-{phase}') for phase in ['initial', 'final']}
                          for suite, folder in [('primary', 'evaluation'), ('hold', 'legacy-hold')]}
            primary = {phase: physical(s[phase+'_evaluation']) for phase in ['initial', 'final']}
            hold = hold_comparison(read(run/'legacy-hold-initial/results.json'), read(run/'legacy-hold-final/results.json'))
            positions = {phase: {d: sum(e['outcome']=='success' and e['index']==4*j+1 for e in evaluation['primary'][phase]['episodes'])
                                 for j, d in enumerate(['forward','left','backward','right'])} for phase in ['initial','final']}
            records.append({'arm': arm, 'seed': seed, 'primary': primary, 'legacy_hold': hold, 'position_directions': positions,
                            'training': training_evidence(run, 16), 'evaluation': evaluation, 'extra': read(run/'extra-summary.json'),
                            'response_initial': next(r['response'] for r in pre['records'] if r['seed']==seed and r['arm']==arm),
                            'main_seconds': s['total_seconds'], 'optimizer_steps': sum(h['minibatches'] for h in s['history']),
                            'kl_early_stops': sum(h['kl_early_stop'] for h in s['history'])})
        configs[1]['input_scaling'] = SCHEMA_SCALING; assert configs[0] == configs[1]
        for k in ['optimizer', 'collector', 'torch_rng', 'python_rng']: assert equal_tree(zeros[0][k], zeros[1][k])
        transformed = copy.deepcopy(zeros[1]['model'])
        for k in ['query.weight', 'fusion1.weight']: transformed[k][:, :2] *= 32
        assert equal_tree(zeros[0]['model'], transformed)
    progress = {arm: progression([r for r in records if r['arm']==arm]) for arm in ARMS}
    result = {'experiment_id': a.name, 'records': records, 'paired_benefit': benefit(records),
              'pilot_gates': {arm: pilot_gate([r for r in records if r['arm']==arm]) for arm in ARMS},
              'progression': progress, 'progression_qualified': progress['goal']['passed'],
              'training_ticks': 104448, 'restore_ticks': restore_ticks,
              'optimizer_steps': sum(r['optimizer_steps'] for r in records) + resume_steps,
              'evaluation_episodes': 384, 'evaluation_ticks': sum(v['ticks'] for r in records for suite in r['evaluation'].values() for v in suite.values()),
              'parity_comparisons': 192, 'response_comparisons': 2496, 'total_native_python_comparisons': 2688,
              'initial_functions_and_traces_exact': True, 'first_collections_exact': True, 'exact_resumes': True,
              'schema_control_reproduces_parent_except_declared_sources': True, 'report_seconds': time.monotonic()-start,
              'extension_executed': False, 'confirmation_used': False, 'model_promoted': False, 'general_navigation_claim': False}
    assert result['restore_ticks'] <= 21600 and result['optimizer_steps'] <= 1632 and result['evaluation_ticks'] <= 518400 and result['report_seconds'] < 600
    atomic_json(a/'results.json', result)


def main():
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--preflight',action='store_true')
    p.add_argument('--extra',choices=ARMS);p.add_argument('--seed',type=int,choices=SEEDS);args=p.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.preflight: preflight(args.archive)
    elif args.extra: extra(args.archive,args.extra,args.seed)
    else: verify(args.archive)


if __name__=='__main__': main()
