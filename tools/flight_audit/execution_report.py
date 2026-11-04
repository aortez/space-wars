"""Per-model, per-repeat physical results; repeats are not independent learned models."""
import argparse
from collections import Counter
import json
from pathlib import Path
from tools.flight_ppo.checkpoint import atomic_json


def summarize(rows):
    counts=dict(Counter(r['outcome'] for r in rows))
    return {'attempts':len(rows),'outcomes':counts,'successes':counts.get('success',0),
            'success_rate':counts.get('success',0)/len(rows),
            'physical_failures':sum(v for k,v in counts.items() if k not in ['success','deadline']),
            'strata':[dict(Counter(r['outcome'] for r in rows if r['recipe'].endswith(f'stratum-{i}'))) for i in range(4)]}


def material_improvement(rows, deterministic_rate):
    assert len(rows)==64 and all(sum(r['repeat']==i for r in rows)==16 for i in range(4))
    rate=summarize(rows)['success_rate']
    return rate>=.25 and rate-deterministic_rate>=.25 and sum(summarize([r for r in rows if r['repeat']==i])['successes']>=4 for i in range(4))>=3


def diagnostics(rows):
    ticks=sum(r['ticks'] for r in rows)
    return {'ticks':ticks,'goal_entries':sum(r['entered_goal_radius'] for r in rows),
            'met_all_instantaneous_tolerances':sum(r['met_all_instantaneous_tolerances'] for r in rows),
            'maximum_dwell_ticks':max(r['maximum_dwell_ticks'] for r in rows),
            'mean_final_distance_reduction':sum(r['final_distance_reduction'] for r in rows)/len(rows),
            'thrust_fraction':sum(r['all_ticks']['executed_thrust_fraction']*r['ticks'] for r in rows)/ticks,
            'brake_fraction':sum(r['all_ticks']['executed_brake_fraction']*r['ticks'] for r in rows)/ticks,
            'mean_thrust_probability':sum(r['all_ticks']['mean_thrust_probability']*r['ticks'] for r in rows)/ticks,
            'thrust_probability_below_half_fraction':sum(r['all_ticks']['thrust_probability_below_half_fraction']*r['ticks'] for r in rows)/ticks,
            'inside_radius':weighted_near(rows)}


def weighted_near(rows):
    rows=[r['inside_radius'] for r in rows if r['inside_radius']['ticks']]
    ticks=sum(r['ticks'] for r in rows)
    if not ticks:return {'ticks':0}
    return {'ticks':ticks, **{k:sum(r[k]*r['ticks'] for r in rows)/ticks for k in rows[0] if k!='ticks'}}


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();seeds=[];all_rows=[]
    for seed in [17,29,43]:
        result=json.loads((args.archive/f'seed-{seed}/results.json').read_text())
        rows=result['episodes'];assert len(rows)==80
        deterministic=[r for r in rows if r['mode']=='deterministic'];sampled=[r for r in rows if r['mode']=='sampled']
        assert len(deterministic)==16 and len(sampled)==64
        assert all(r['matches_parent_trace'] for r in deterministic)
        assert len({(r['repeat'],r['index']) for r in sampled})==64
        assert all(r['outcome'] is not None for r in rows)
        entry={k:result[k] for k in ['seed','physics_ticks','seconds','model_unchanged','optimizer_steps','checkpoint_sha256']}
        entry.update(deterministic=summarize(deterministic),sampled=summarize(sampled),
            repeats=[summarize([r for r in sampled if r['repeat']==i]) for i in range(4)],
            diagnostics={mode:diagnostics([r for r in rows if r['mode']==mode]) for mode in ['deterministic','sampled']},
            material_arrival_improvement=material_improvement(sampled,summarize(deterministic)['success_rate']),episodes=rows)
        seeds.append(entry);all_rows.extend(rows)
    total=sum(s['physics_ticks'] for s in seeds);assert total<=432000
    atomic_json(args.out,{'experiment_id':args.archive.name,'seeds':seeds,'physics_ticks':total,'preflight_physics_ticks':128,
        'material_arrival_improvement':sum(s['material_arrival_improvement'] for s in seeds)>=2,
        'pooled_descriptive_only':{mode:{**summarize([r for r in all_rows if r['mode']==mode]),'diagnostics':diagnostics([r for r in all_rows if r['mode']==mode])} for mode in ['deterministic','sampled']},
        'qualification_gate_changed':False,'confirmation_used':False,'optimizer_steps':0,'general_flight_claim':False})


if __name__=='__main__':main()
