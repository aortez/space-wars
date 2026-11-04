"""Physical outcomes for the frozen aligned-approach diagnostic; no checkpoint search."""
import argparse
from collections import Counter
import json
import time
import torch
from pathlib import Path
from .bridge import Bridge
from .checkpoint import atomic_json
from .curriculum import ALIGNED_RECIPE, sample
from .evaluate import evaluate
from .learning_metrics import aggregate, physical, seed_gates
from .learning_report import parity, inspect_trace
from .persistence_report import training_diagnostics
from .velocity_report import behavior


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--confirm', action='store_true')
    args = parser.parse_args()
    torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
    records = []
    with Bridge(args.archive/'runtime/flight_ppo_bridge') as bridge:
        for seed in [17,29,43]:
            run = args.archive/f'seed-{seed}'
            s = json.loads((run/'summary.json').read_text())
            assert s['completed_updates'] == 128 and s['environment_steps'] == 131072
            assert sum(e['ticks'] for e in s['completed_episodes']+s['unfinished_episodes']) == 131072
            state = torch.load(run/'checkpoint-0128.pt', weights_only=True)
            assert state['contract']['config']['task_recipe'] == ALIGNED_RECIPE
            traces = {phase:[inspect_trace(p) for p in sorted((run/f'evaluation-{phase}').glob('clear-*.json'))] for phase in ['initial','final']}
            records.append({'seed':seed, 'status':'completed', **seed_gates(s['initial_evaluation'], s['final_evaluation'], ALIGNED_RECIPE),
                'initial_evaluation':s['initial_evaluation'], 'final_evaluation':s['final_evaluation'],
                'training_ticks':s['environment_steps'], 'total_seconds':s['total_seconds'],
                'training_seconds':s['training_seconds_including_checkpoints'],
                'training_outcomes':dict(Counter(e['outcome'] for e in s['completed_episodes'])),
                'censored':len(s['unfinished_episodes']), 'diagnostics':training_diagnostics(run,state),
                'parity':parity(bridge,run,state), 'evaluation_trace_inspection':traces,
                'development_behavior':{phase:behavior(run/f'evaluation-{phase}') for phase in ['initial','final']},
                'best':json.loads((run/'best.json').read_text()),
                'kl_early_stopped_updates':sum(h['kl_early_stop'] for h in s['history']),
                'optimization_summary':{k:{'first':s['history'][0][k], 'last':s['history'][-1][k],
                    'mean':sum(h[k] for h in s['history'])/128, 'maximum':max(h[k] for h in s['history'])}
                    for k in ['value_loss','policy_loss','entropy','gradient_norm','clip_fraction','kl']}})
        gates = aggregate(records)
        confirmation_path = args.archive/'confirmation.json'
        if args.confirm:
            assert gates['basic_flight_gate'] and not confirmation_path.exists()
            config = json.loads((args.archive/'configs/17.json').read_text())
            batch, pools = sample(bridge,config,'heldout')
            atomic_json(args.archive/'confirmation-admission.json',batch)
            assert batch['complete']
            candidates = [c for p in pools for c in p]
            confirmation = []
            for seed in [17,29,43]:
                rows = evaluate(args.archive/'runtime/flight_ppo_bridge',args.archive/f'seed-{seed}/policy-0128.json',
                                candidates,args.archive/f'confirmation-{seed}',time.monotonic()+300)
                outcome = physical(rows, ALIGNED_RECIPE)
                passed = outcome['successes']>=12 and outcome['physical_failures']==0 and all(s.get('success',0)>=2 for s in outcome['strata'])
                confirmation.append({'seed':seed, 'passed':passed, 'physical':outcome, 'episodes':rows})
            atomic_json(confirmation_path, {'seeds':confirmation, 'passed':sum(s['passed'] for s in confirmation)>=2})
    confirmation = json.loads(confirmation_path.read_text()) if confirmation_path.exists() else None
    status = ('completed: passed' if confirmation['passed'] else 'completed: failed') if confirmation else ('pending: progression gate passed' if gates['basic_flight_gate'] else 'unused: progression gate failed')
    outcomes = Counter(); final = Counter()
    for s in records:
        outcomes.update(s['training_outcomes']); final.update(s['final']['outcomes'])
    atomic_json(args.out, {'experiment_id':args.archive.name, 'seeds':records, 'gates':gates,
        'total_training_ticks':sum(s['training_ticks'] for s in records),
        'total_development_ticks':sum(e['ticks'] for s in records for phase in ['initial_evaluation','final_evaluation'] for e in s[phase]),
        'totals':{'training_outcomes':dict(outcomes), 'final_outcomes':dict(final),
            'training_attempts':sum(s['diagnostics']['attempts'] for s in records),
            'training_entered_goal_radius':sum(s['diagnostics']['entered_goal_radius'] for s in records),
            'training_met_all_instantaneous_tolerances':sum(s['diagnostics']['met_all_instantaneous_tolerances'] for s in records),
            'maximum_training_dwell_ticks':max(s['diagnostics']['maximum_dwell_ticks'] for s in records),
            'censored':sum(s['censored'] for s in records)},
        'confirmation_status':status, 'confirmation':confirmation, 'general_flight_claim':False, 'obstacle_navigation_claim':False})


if __name__ == '__main__':main()
