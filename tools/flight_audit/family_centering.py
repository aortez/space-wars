"""Task-family advantage centering, with physical recovery and hold gates."""
import argparse
import copy
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.initialization import initialize_policy
from tools.flight_ppo.ppo import GLOBAL_CENTERING, FAMILY_CENTERING
from tools.flight_ppo.model import FlightModel, SCHEMA_SCALING, GOAL_RESIDUAL_ARCHITECTURE
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.goal_response import read
from tools.flight_ppo.execution import load_export
from tools.flight_audit.goal_scaling import benefit as scaling_benefit, response_probe
from tools.flight_audit.hold_position import comparison as hold_comparison
from tools.flight_audit.settle_recover import SEEDS, physical, inspect_evaluation
from tools.flight_audit.short_recovery import training_evidence, progression
from tools.flight_audit.rate_training import pilot_gate
from tools.flight_audit.goal_residual import extra

ARMS = ['global', 'family']
CHANGED_SOURCES = {'ppo.py', 'collector.py', 'run.py'}


def assert_control_replay(current, parent):
    """Normalize only declared source identities for audit; resume stays strict."""
    current = copy.deepcopy(current)
    sources = current['contract']['python_sources']; old = parent['contract']['python_sources']
    assert set(sources) == set(old)
    assert {k for k in sources if sources[k] != old[k]} == CHANGED_SOURCES
    current['contract']['python_sources'] = copy.deepcopy(old)
    assert equal_tree(current, parent)


def benefit(records):
    mapped = copy.deepcopy(records)
    for row in mapped:
        row['arm'] = {'global':'schema','family':'goal'}[row['arm']]
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
                assert equal_tree(model.state_dict(), original['model'])
                assert not torch.count_nonzero(model.goal_output.weight) and not torch.count_nonzero(model.goal_output.bias)
                out = a/f'preflight/{arm}/seed-{seed}'; out.mkdir(parents=True)
                policy = out/'policy-0000.json'; model.export(policy)
                assert policy.read_bytes() == (a/f'inputs/seed-{seed}/initial-policy.json').read_bytes()
                folded = load_export(policy, bridge.contract['schema'])
                bridge.call('load_policy', path=str(policy.resolve()))
                result = response_probe(a, seed, model, folded, bridge, out/'response', deadline)
                assert all(r['parity']['fold_error'] == 0 for r in result['records'])
                records.append({'arm': arm, 'seed': seed, 'response': result})
            assert torch.equal(*rngs) and torch.equal(rngs[0], original['torch_rng'])
            for directory in (a/f'preflight/global/seed-{seed}/response').iterdir():
                assert read(directory/'outputs.json') == read(a/f'preflight/family/seed-{seed}/response'/directory.name/'outputs.json')
    result = {'records': records, 'initial_weights_exact': True, 'initial_training_outputs_exact': True,
              'initial_native_outputs_exact': True, 'matched_initial_rng': True,
              'native_python_comparisons': 1248, 'optimizer_steps': 0, 'physics_ticks': 0,
              'seconds': time.monotonic()-start}
    assert result['seconds'] < 600
    atomic_json(a/'preflight-results.json', result)


def inspect_signals(run, mode):
    rows = []
    previous = torch.load(run/'checkpoint-0000.pt',weights_only=True)['model']
    for update in range(1,17):
        signal = read(run/f'advantages-{update:04}.json')
        assert signal['mode'] == mode
        raw = torch.tensor(signal['raw_advantages']); labels = torch.tensor(signal['family_ids'])
        assert labels.tolist() == list(range(4))*256 and len(raw)==1024
        divisor = raw.std(unbiased=False).clamp_min(1e-8)
        if mode == GLOBAL_CENTERING:
            expected = (raw-raw.mean())/divisor
        else:
            expected = torch.empty_like(raw)
            for i in range(4):
                mask=labels==i; expected[mask]=(raw[mask]-raw[mask].mean())/divisor
        assert torch.equal(expected,torch.tensor(signal['normalized_advantages']))
        assert signal['raw_mean']==float(raw.mean()) and signal['raw_std']==float(raw.std(unbiased=False)) and signal['divisor']==float(divisor)
        for i,name in enumerate(['settled','position','velocity','spin']):
            mask=labels==i; rr=raw[mask]; nn=expected[mask]
            assert signal['families'][name]=={'rows':256,'raw_mean':float(rr.mean()),'normalized_mean':float(nn.mean()),'raw_positive_rows':int((rr>0).sum()),'normalized_positive_rows':int((nn>0).sum())}
            if mode==FAMILY_CENTERING:assert abs(float(nn.mean()))<1e-5
        current=torch.load(run/f'checkpoint-{update:04}.pt',weights_only=True)['model']
        changes={k:float((v.double()-previous[k].double()).square().sum()) for k,v in current.items()}
        norms={group:sum(v for k,v in changes.items() if k.startswith(prefix))**.5 for group,prefix in [('goal_hidden','goal_hidden.'),('goal_output','goal_output.'),('actor','actor.'),('critic','critic.')]}
        norms['all_parameters']=sum(changes.values())**.5
        rows.append({'update':update,'raw_mean':signal['raw_mean'],'raw_std':signal['raw_std'],'families':signal['families'],'parameter_delta_norms':norms})
        previous=current
    assert read(run/'advantages-0016.json')==read(run.parents[2]/f"resumed/{run.parent.name}/{run.name}/advantages-0016.json")
    atomic_json(run/'learning-signals.json',rows)
    return rows


def verify(a):
    start = time.monotonic(); records = []; restore_ticks = 0; resume_steps = 0
    pre = read(a/'preflight-results.json'); assert pre['initial_weights_exact'] and pre['initial_training_outputs_exact']
    for seed in SEEDS:
        zeros = []; configs = []
        parent_first = torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-0001.pt', weights_only=True)
        for arm in ARMS:
            run = a/f'pilot/{arm}/seed-{seed}'; s = read(run/'summary.json')
            count = 39546
            assert s['completed_updates'] == 16 and s['environment_steps'] == 16384 and s['parameter_count'] == count and s['total_seconds'] < 1800
            zero = torch.load(run/'checkpoint-0000.pt', weights_only=True); zeros.append(zero)
            config = zero['contract']['config']; configs.append(copy.deepcopy(config))
            assert config['input_scaling'] == SCHEMA_SCALING and config['value_gradient'] == 'head-only-v1'
            assert config['architecture'] == GOAL_RESIDUAL_ARCHITECTURE
            assert config.get('advantage_centering',GLOBAL_CENTERING) == {'global':GLOBAL_CENTERING,'family':FAMILY_CENTERING}[arm]
            assert config['learning_rate'] == .000075 and config['exploration']['std_multiplier'] == .25 and config['exploration']['rho'] == 0
            assert not zero['optimizer']['state']
            assert (run/'policy-0000.json').read_bytes() == (a/f'preflight/{arm}/seed-{seed}/policy-0000.json').read_bytes()
            final = torch.load(run/'checkpoint-0016.pt', weights_only=True)
            again = torch.load(a/f'resumed/{arm}/seed-{seed}/checkpoint-0016.pt', weights_only=True)
            assert equal_tree(final, again)
            restored = torch.load(run/'checkpoint-0015.pt', weights_only=True)
            restore_ticks += sum(len(t['steps']) for t in restored['collector']['traces']); resume_steps += s['history'][-1]['minibatches']
            assert equal_tree(torch.load(run/'checkpoint-0001.pt', weights_only=True)['collector'], parent_first['collector'])
            if arm == 'global':
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
                            'kl_early_stops': sum(h['kl_early_stop'] for h in s['history']), 'parameter_count': count,
                            'branch_parameter_changes': {k:float((v-zero['model'][k]).abs().max()) for k,v in final['model'].items() if k.startswith('goal_')},
                            'learning_signals': inspect_signals(run, config.get('advantage_centering',GLOBAL_CENTERING))})
        del configs[1]['advantage_centering']; assert configs[0] == configs[1]
        for k in ['model','optimizer','collector','torch_rng','python_rng']:
            assert equal_tree(zeros[0][k],zeros[1][k])
        first = [read(a/f'pilot/{arm}/seed-{seed}/advantages-0001.json') for arm in ARMS]
        for key in ['raw_advantages','family_ids','raw_mean','raw_std','divisor']:
            assert first[0][key] == first[1][key]
    progress = {arm: progression([r for r in records if r['arm']==arm]) for arm in ARMS}
    result = {'experiment_id': a.name, 'records': records, 'paired_benefit': benefit(records),
              'pilot_gates': {arm: pilot_gate([r for r in records if r['arm']==arm]) for arm in ARMS},
              'progression': progress, 'progression_qualified': progress['family']['passed'],
              'training_ticks': 104448, 'restore_ticks': restore_ticks,
              'optimizer_steps': sum(r['optimizer_steps'] for r in records) + resume_steps,
              'evaluation_episodes': 384, 'evaluation_ticks': sum(v['ticks'] for r in records for suite in r['evaluation'].values() for v in suite.values()),
              'parity_comparisons': 192, 'response_comparisons': 2496, 'total_native_python_comparisons': 2688,
              'initial_functions_and_traces_exact': True, 'first_collections_exact': True, 'exact_resumes': True,
              'global_control_reproduces_parent_except_three_declared_sources': True, 'report_seconds': time.monotonic()-start,
              'advantage_records_verified': 102, 'update_deltas_measured': 96,
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
