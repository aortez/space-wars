"""Aggregate all preselected diagnostic cases; groups are descriptive, not causal."""
import argparse
import gzip
import json
from pathlib import Path
from .signal import summary,write,UPDATES


def summarize_rows(rows):
    groups={}
    for name in sorted({label for row in rows for label in row['labels']}):
        selected=[row for row in rows if name in row['labels']]
        groups[name]={'count':len(selected),**{k:summary([r[k] for r in selected]) for k in
            ['advantage','normalized_advantage','thrust_score_pressure','brake_score_pressure','critic_value','critic_target']},
            'reward_means':{k:sum(r['reward'][k] for r in selected)/len(selected) for k in rows[0]['reward']}}
    return groups


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();arms={};all_ticks=0;cases=0;continuations=0
    for arm in ['control','long_credit']:
        stages={update:[] for update in UPDATES};gradient_cases=[];frozen=[];run_times=[]
        for seed in [17,29,43]:
            run=args.archive/arm/f'seed-{seed}'
            result=json.loads((run/'results.json').read_text())
            assert [c['update_before'] for c in result['cases']]==UPDATES
            all_ticks+=result['replay_ticks'];assert result['replay_ticks']<=65000
            run_times.append(result['elapsed_seconds'])
            for case in result['cases']:
                update=case['update_before'];cases+=1
                assert case['collector_reconstruction_exact'] and case['model_optimizer_unchanged']
                directory=run/f'update-{update:04}'
                with gzip.open(directory/'rows.json.gz','rt') as f:rows=json.load(f)
                assert len(rows)==1024;stages[update].extend(rows)
                gradient_cases.append({'seed':seed,'update_before':update,**case['gradients']})
                for row in case['frozen_continuations']:
                    frozen.append({'seed':seed,'update_before':update,**row});continuations+=1
        states=[]
        for seed in [17,29,43]:
            for update in [32,96]:
                for slot in [0,3]:
                    rows=[r for r in frozen if r['seed']==seed and r['update_before']==update and r['slot']==slot]
                    assert len(rows)==2 and rows[0]['predicted_value']==rows[1]['predicted_value']
                    mean=sum(r['realized_discounted_return'] for r in rows)/2
                    states.append({'seed':seed,'update_before':update,'slot':slot,
                        'prediction':rows[0]['predicted_value'],'mean_realized_return':mean,
                        'prediction_minus_mean_return':rows[0]['predicted_value']-mean,
                        'sample_return_spread':abs(rows[0]['realized_discounted_return']-rows[1]['realized_discounted_return']),
                        'outcomes':[r['outcome'] for r in rows]})
        errors=[s['prediction_minus_mean_return'] for s in states]
        arms[arm]={'stages':{str(u):summarize_rows(rows) for u,rows in stages.items()},
            'all_rows':summarize_rows([r for rows in stages.values() for r in rows]),
            'gradient_cases':gradient_cases,'frozen_continuations':frozen,'boundary_states':states,
            'boundary_value_error':{'signed':summary(errors),'absolute':summary([abs(x) for x in errors])},
            'worker_elapsed_seconds':run_times}
    assert cases==36 and continuations==48 and all_ticks<=390000
    write(args.out,{'experiment_id':args.archive.name,'kind':'diagnostic; no policy updates',
        'cases':cases,'reconstructed_action_ticks':cases*1024,'frozen_continuations':continuations,
        'retained_replay_ticks_including_restore':all_ticks,'preflight_ticks':1024,'arms':arms,
        'all_reconstructed_collectors_exact':True,'all_models_and_optimizers_unchanged':True,
        'confirmation_partition':'unused','capability_claim':False})


if __name__=='__main__':main()
