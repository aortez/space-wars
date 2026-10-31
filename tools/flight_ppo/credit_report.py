"""Frozen credit-assignment comparison and physical trajectory diagnostics."""
import argparse
from collections import Counter
import json
from pathlib import Path
import torch
from .bridge import Bridge
from .checkpoint import atomic_json
from .learning_metrics import seed_gates, aggregate
from .learning_report import parity, inspect_trace
from .persistence_report import training_diagnostics
from .velocity_report import behavior


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--archive',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    arms={};initial_states={}
    with Bridge(args.archive/'runtime/flight_ppo_bridge') as bridge:
        for arm in ['control','long_credit']:
            records=[]
            for seed in [17,29,43]:
                run=args.archive/arm/f'seed-{seed}'
                if not (run/'summary.json').exists():
                    records.append({'seed':seed,'status':'interrupted_or_failed','learning_evidence':False,'basic_flight_gate':False})
                    continue
                summary=json.loads((run/'summary.json').read_text())
                all_episodes=summary['completed_episodes']+summary['unfinished_episodes']
                assert len(all_episodes)==summary['started_episodes']
                assert sum(e['ticks'] for e in all_episodes)==summary['environment_steps']==131072
                assert summary['completed_updates']==128
                state=torch.load(run/'checkpoint-0128.pt',weights_only=True)
                initial=torch.load(run/'checkpoint-0000.pt',weights_only=True)
                if arm=='control':initial_states[seed]=initial['model']
                elif seed in initial_states:
                    assert all(torch.equal(v,initial_states[seed][k]) for k,v in initial['model'].items())
                    other=args.archive/'control'/f'seed-{seed}'
                    for name in ['training-admission.json','development-admission.json','evaluation-initial/results.json']:
                        assert json.loads((run/name).read_text())==json.loads((other/name).read_text())
                records.append({'seed':seed,'status':'completed',**seed_gates(summary['initial_evaluation'],summary['final_evaluation']),
                    'training_outcomes':dict(Counter(e['outcome'] for e in summary['completed_episodes'])),
                    'censored':len(summary['unfinished_episodes']),'training_ticks':summary['environment_steps'],
                    'training_seconds':summary['training_seconds_including_checkpoints'],
                    'training_ticks_per_second':summary['training_steps_per_second'],'total_seconds':summary['total_seconds'],
                    'initial_evaluation':summary['initial_evaluation'],'final_evaluation':summary['final_evaluation'],
                    'best':json.loads((run/'best.json').read_text()),'parity':parity(bridge,run,state),
                    'diagnostics':training_diagnostics(run,state),
                    'kl_early_stopped_updates':sum(h['kl_early_stop'] for h in summary['history']),
                    'optimizer_minibatches':sum(h['minibatches'] for h in summary['history']),
                    'optimization_summary':{key:{'first':summary['history'][0][key], 'last':summary['history'][-1][key],
                        'mean':sum(h[key] for h in summary['history'])/len(summary['history']),
                        'maximum':max(h[key] for h in summary['history'])}
                        for key in ['value_loss','policy_loss','entropy','gradient_norm','clip_fraction','kl']},
                    'evaluation_trace_inspection':{phase:[inspect_trace(p) for p in sorted((run/f'evaluation-{phase}').glob('clear-*.json'))] for phase in ['initial','final']}})
            arms[arm]={'seeds':records,'gates':aggregate(records)}
    completed=all(s['status']=='completed' for a in arms.values() for s in a['seeds'])
    comparison={'all_runs_completed':completed}
    if completed:
        control=arms['control']['seeds'];ablated=arms['long_credit']['seeds']
        gains=[b['final']['successes']-a['final']['successes'] for a,b in zip(control,ablated)]
        comparison.update(paired_success_gains=gains,
            arrival_benefit_gate=sum(g>=4 for g in gains)>=2 and arms['long_credit']['gates']['learning_evidence'])
    for name, arm in arms.items():
        for record in arm['seeds']:
            if record['status'] != 'completed':
                continue
            run=args.archive/name/f"seed-{record['seed']}"
            record['development_behavior']={phase:behavior(run/f'evaluation-{phase}') for phase in ['initial','final']}
    confirmation = 'pending: an arm passed progression' if any(a['gates']['basic_flight_gate'] for a in arms.values()) else 'unused: neither arm passed progression'
    atomic_json(args.out,{'experiment_id':args.archive.name,'arms':arms,'comparison':comparison,
                         'confirmation_status':confirmation,'obstacle_navigation_claim':False})


if __name__=='__main__':
    main()
