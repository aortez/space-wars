"""Frozen matched-arm physical gates and action persistence diagnostics."""
import argparse
from collections import Counter
import json
from pathlib import Path
import statistics
import torch
from .bridge import Bridge
from .checkpoint import atomic_json
from .learning_metrics import seed_gates, aggregate
from .learning_report import parity, inspect_trace


def training_diagnostics(run, state):
    traces=[json.loads(p.read_text()) for p in sorted((run/'training-traces').glob('*.json'))]+state['collector']['traces']
    ticks=thrust=brake=both=entered=qualified=maximum_dwell=0
    runs=[]
    for trace in traces:
        steps=trace['steps'];length=0
        entered+=any(s['frame']['errors']['position']<=3 for s in steps)
        qualified+=any(s['frame']['errors']['position']<=3 and s['frame']['errors']['velocity']<=1 and s['frame']['errors']['spin']<=.2 for s in steps)
        maximum_dwell=max(maximum_dwell,max((s['progress']['dwell_ticks'] for s in steps),default=0))
        for s in steps:
            a=s['action'];ticks+=1;thrust+=a['thrust'];brake+=a['brake'];both+=a['thrust'] and a['brake']
            if a['thrust']:length+=1
            elif length:runs.append(length);length=0
        if length:runs.append(length)
    assert ticks==131072
    return {'attempts':len(traces),'ticks':ticks,'entered_goal_radius':entered,
            'met_all_instantaneous_tolerances':qualified,'maximum_dwell_ticks':maximum_dwell,
            'thrust_fraction':thrust/ticks,'brake_fraction':brake/ticks,'both_fraction':both/ticks,
            'mean_thrust_on_ticks':sum(runs)/len(runs) if runs else 0,
            'half_second_thrust_runs':sum(n>=30 for n in runs),'maximum_thrust_on_ticks':max(runs,default=0)}


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--archive',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    arms={};initial_states={}
    with Bridge(args.archive/'runtime/flight_ppo_bridge') as bridge:
        for arm in ['independent','persistent']:
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
                if arm=='independent':initial_states[seed]=initial['model']
                else:
                    assert all(torch.equal(v,initial_states[seed][k]) for k,v in initial['model'].items())
                    other=args.archive/'independent'/f'seed-{seed}'
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
                    'evaluation_trace_inspection':{phase:[inspect_trace(p) for p in sorted((run/f'evaluation-{phase}').glob('clear-*.json'))] for phase in ['initial','final']}})
            arms[arm]={'seeds':records,'gates':aggregate(records)}
    completed=all(s['status']=='completed' for a in arms.values() for s in a['seeds'])
    comparison={'all_runs_completed':completed}
    if completed:
        control=arms['independent']['seeds'];persistent=arms['persistent']['seeds']
        gains=[b['final']['successes']-a['final']['successes'] for a,b in zip(control,persistent)]
        ratio=statistics.median(s['diagnostics']['mean_thrust_on_ticks'] for s in persistent)/max(1e-12,statistics.median(s['diagnostics']['mean_thrust_on_ticks'] for s in control))
        comparison.update(paired_success_gains=gains,median_thrust_run_ratio=ratio,
            persistence_manipulation_gate=ratio>=2 and all(s['diagnostics']['half_second_thrust_runs']>=1 for s in persistent),
            arrival_benefit_gate=sum(g>=4 for g in gains)>=2 and arms['persistent']['gates']['learning_evidence'])
    atomic_json(args.out,{'experiment_id':args.archive.name,'arms':arms,'comparison':comparison,
                         'confirmation_status':'pending iff an arm passes its progression gate','obstacle_navigation_claim':False})


if __name__=='__main__':
    main()
