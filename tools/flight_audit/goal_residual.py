"""Zero-output learned goal branch, with physical recovery and hold gates."""
import argparse
import copy
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from tools.flight_ppo.evaluate import evaluate
from tools.flight_ppo.initialization import initialize_policy
from tools.flight_ppo.model import FlightModel, SCHEMA_SCALING, GOAL_RESIDUAL_ARCHITECTURE
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.goal_response import read
from tools.flight_audit.input_scaling import infer
from tools.flight_ppo.execution import load_export
from tools.flight_audit.goal_scaling import benefit as scaling_benefit, response_probe
from tools.flight_audit.hold_position import comparison as hold_comparison
from tools.flight_audit.settle_recover import SEEDS, candidates, physical, inspect_evaluation
from tools.flight_audit.short_recovery import training_evidence, progression
from tools.flight_audit.rate_training import pilot_gate

ARMS = ['schema', 'residual']
CHANGED_SOURCES = {'model.py', 'initialization.py', 'run.py', 'checkpoint.py', 'execution.py'}


def models(run, update, schema):
    state = torch.load(run/f'checkpoint-{update:04}.pt', weights_only=True)
    config = state['contract']['config']
    model = FlightModel(schema, config['input_scaling'], config.get('architecture'))
    model.load_state_dict(state['model']); model.eval(); model.requires_grad_(False)
    folded = load_export(run/f'policy-{update:04}.json', schema)
    assert equal_tree(model.inference_state_dict(), folded.state_dict())
    return model, folded, state


def assert_control_replay(current, parent, current_bridge, parent_bridge):
    """Normalize only declared source identities for audit; resume stays strict."""
    current = copy.deepcopy(current)
    sources = current['contract']['python_sources']; old = parent['contract']['python_sources']
    assert set(sources) == set(old)
    assert {k for k in sources if sources[k] != old[k]} == CHANGED_SOURCES
    current['contract']['python_sources'] = copy.deepcopy(old)
    assert current['contract']['bridge_sha256'] == current_bridge
    assert parent['contract']['bridge_sha256'] == parent_bridge
    current['contract']['bridge_sha256'] = parent_bridge
    assert equal_tree(current, parent)


def benefit(records):
    mapped = copy.deepcopy(records)
    for row in mapped:
        if row['arm'] == 'residual': row['arm'] = 'goal'
    return scaling_benefit(mapped)


def preflight(a):
    start = time.monotonic(); deadline = start + 600; records = []
    for seed in SEEDS:
        with Bridge(a/'runtime/flight_ppo_bridge') as bridge, torch.random.fork_rng(devices=[]):
            original = torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-0000.pt', weights_only=True)
            rngs = []
            for arm in ARMS:
                config = read(a/f'configs/{arm}/{seed}.json')
                torch.manual_seed(seed); model = FlightModel(bridge.contract['schema'], config['input_scaling'], config.get('architecture'))
                initialize_policy(model, config, a/f'inputs/seed-{seed}/policy.json')
                rngs.append(torch.get_rng_state().clone())
                assert equal_tree({k:v for k,v in model.state_dict().items() if not k.startswith('goal_')}, original['model'])
                if arm == 'residual':
                    assert not torch.count_nonzero(model.goal_output.weight) and not torch.count_nonzero(model.goal_output.bias)
                out = a/f'preflight/{arm}/seed-{seed}'; out.mkdir(parents=True)
                policy = out/'policy-0000.json'; model.export(policy)
                if arm == 'schema': assert policy.read_bytes() == (a/f'inputs/seed-{seed}/policy.json').read_bytes()
                folded = load_export(policy, bridge.contract['schema'])
                bridge.call('load_policy', path=str(policy.resolve()))
                result = response_probe(a, seed, model, folded, bridge, out/'response', deadline)
                assert all(r['parity']['fold_error'] == 0 for r in result['records'])
                records.append({'arm': arm, 'seed': seed, 'response': result})
            assert torch.equal(*rngs) and torch.equal(rngs[0], original['torch_rng'])
            for directory in (a/f'preflight/schema/seed-{seed}/response').iterdir():
                assert read(directory/'outputs.json') == read(a/f'preflight/residual/seed-{seed}/response'/directory.name/'outputs.json')
    result = {'records': records, 'initial_base_weights_exact': True, 'initial_training_outputs_exact': True,
              'initial_native_outputs_exact': True, 'matched_initial_rng': True,
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
    pre = read(a/'preflight-results.json'); assert pre['initial_base_weights_exact'] and pre['initial_training_outputs_exact']
    current_bridge = sha(a/'runtime/flight_ppo_bridge'); parent_bridge = read(a/'inputs/parent-contract.json')['bridge_sha256']
    for seed in SEEDS:
        zeros = []; configs = []
        parent_first = torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-0001.pt', weights_only=True)
        for arm in ARMS:
            run = a/f'pilot/{arm}/seed-{seed}'; s = read(run/'summary.json')
            count = 39546 if arm == 'residual' else 39447
            assert s['completed_updates'] == 16 and s['environment_steps'] == 16384 and s['parameter_count'] == count and s['total_seconds'] < 1800
            zero = torch.load(run/'checkpoint-0000.pt', weights_only=True); zeros.append(zero)
            config = zero['contract']['config']; configs.append(copy.deepcopy(config))
            assert config['input_scaling'] == SCHEMA_SCALING and config['value_gradient'] == 'head-only-v1'
            if arm == 'residual': assert config['architecture'] == GOAL_RESIDUAL_ARCHITECTURE
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
                                          torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-{update:04}.pt', weights_only=True), current_bridge, parent_bridge)
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
                            'kl_early_stops': sum(h['kl_early_stop'] for h in s['history']), 'parameter_count': count,
                            'branch_parameter_changes': {k:float((v-zero['model'][k]).abs().max()) for k,v in final['model'].items() if k.startswith('goal_')}})
        del configs[1]['architecture']; assert configs[0] == configs[1]
        for k in ['collector', 'torch_rng', 'python_rng']: assert equal_tree(zeros[0][k], zeros[1][k])
        # Empty Adam moments and identical hyperparameters; the branch adds four tensors.
        base_optimizer = copy.deepcopy(zeros[0]['optimizer'])
        branch_optimizer = copy.deepcopy(zeros[1]['optimizer'])
        assert len(base_optimizer['param_groups']) == len(branch_optimizer['param_groups']) == 1
        bp = base_optimizer['param_groups'][0]['params']; rp = branch_optimizer['param_groups'][0]['params']
        assert rp == list(range(len(bp)+4)) and rp[:len(bp)] == bp
        branch_optimizer['param_groups'][0]['params'] = bp
        assert equal_tree(base_optimizer, branch_optimizer)
        base = {k:v for k,v in zeros[1]['model'].items() if not k.startswith('goal_')}
        assert equal_tree(zeros[0]['model'], base)
        assert not torch.count_nonzero(zeros[1]['model']['goal_output.weight']) and not torch.count_nonzero(zeros[1]['model']['goal_output.bias'])
    progress = {arm: progression([r for r in records if r['arm']==arm]) for arm in ARMS}
    result = {'experiment_id': a.name, 'records': records, 'paired_benefit': benefit(records),
              'pilot_gates': {arm: pilot_gate([r for r in records if r['arm']==arm]) for arm in ARMS},
              'progression': progress, 'progression_qualified': progress['residual']['passed'],
              'training_ticks': 104448, 'restore_ticks': restore_ticks,
              'optimizer_steps': sum(r['optimizer_steps'] for r in records) + resume_steps,
              'evaluation_episodes': 384, 'evaluation_ticks': sum(v['ticks'] for r in records for suite in r['evaluation'].values() for v in suite.values()),
              'parity_comparisons': 192, 'response_comparisons': 2496, 'total_native_python_comparisons': 2688,
              'initial_functions_and_traces_exact': True, 'first_collections_exact': True, 'exact_resumes': True,
              'schema_control_reproduces_parent_except_declared_sources_and_runtime': True, 'report_seconds': time.monotonic()-start,
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
