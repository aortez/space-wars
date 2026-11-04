"""Paired physical outcomes for the frozen steering-noise ablation."""
import argparse
from collections import Counter
import json
from pathlib import Path
from tools.flight_ppo.checkpoint import atomic_json
from .execution_report import summarize,diagnostics,material_improvement


def paired_outcomes(baseline,ablated):
    left={(r['repeat'],r['index']):r for r in baseline}
    right={(r['repeat'],r['index']):r for r in ablated}
    assert len(left)==len(right)==64 and left.keys()==right.keys()
    counts=Counter()
    for key,a in left.items():
        b=right[key];assert a['rng_seed']==b['rng_seed'] and a['recipe']==b['recipe']
        old,new=a['outcome']=='success',b['outcome']=='success'
        counts['both_success' if old and new else 'gained_success' if new else 'lost_success' if old else 'neither_success']+=1
    return dict(counts)


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();seeds=[];pooled={'fully_sampled':[],'mean_steering':[]}
    for seed in [17,29,43]:
        result=json.loads((args.archive/f'seed-{seed}/results.json').read_text());rows=result['episodes'];assert len(rows)==64
        old=json.loads((args.archive/f'baseline/seed-{seed}/results.json').read_text())
        baseline=[r for r in old['episodes'] if r['mode']=='sampled'];assert len(baseline)==64
        assert all(r['outcome'] is not None for r in rows)
        rates={k:summarize(v) for k,v in [('fully_sampled',baseline),('mean_steering',rows)]}
        record={k:result[k] for k in ['seed','physics_ticks','seconds','model_unchanged','optimizer_steps','checkpoint_sha256']}
        record.update(**rates,paired=paired_outcomes(baseline,rows),
            repeats={k:[summarize([r for r in v if r['repeat']==i]) for i in range(4)] for k,v in [('fully_sampled',baseline),('mean_steering',rows)]},
            diagnostics={k:diagnostics(v) for k,v in [('fully_sampled',baseline),('mean_steering',rows)]},
            material_arrival_improvement=material_improvement(rows,rates['fully_sampled']['success_rate']),
            matched_innovation_ticks=sum(r['pairing']['shared_ticks'] for r in rows),
            maximum_abs_standardized_innovation_error=max(r['pairing']['maximum_abs_standardized_innovation_error'] for r in rows),episodes=rows)
        seeds.append(record);pooled['fully_sampled'].extend(baseline);pooled['mean_steering'].extend(rows)
    ticks=sum(s['physics_ticks'] for s in seeds);assert ticks<=345600
    atomic_json(args.out,{'experiment_id':args.archive.name,'seeds':seeds,'physics_ticks':ticks,'preflight_physics_ticks':128,
        'baseline_reused_physics_ticks':sum(r['ticks'] for r in pooled['fully_sampled']),
        'material_arrival_improvement':sum(s['material_arrival_improvement'] for s in seeds)>=2,
        'pooled_descriptive_only':{k:{**summarize(v),'diagnostics':diagnostics(v)} for k,v in pooled.items()},
        'qualification_gate_changed':False,'confirmation_used':False,'optimizer_steps':0,'general_flight_claim':False})


if __name__=='__main__':main()
