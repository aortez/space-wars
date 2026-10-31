"""Learning gain measured with the same mixed execution rule on both checkpoints."""
import argparse
import json
from pathlib import Path
from tools.flight_ppo.checkpoint import atomic_json
from .execution_report import summarize,diagnostics,material_improvement
from .steering_report import paired_outcomes


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();seeds=[];pooled={'initial':[],'trained':[]}
    for seed in [17,29,43]:
        new=json.loads((args.archive/f'seed-{seed}/results.json').read_text());initial=new['episodes'];assert len(initial)==64
        old=json.loads((args.archive/f'trained/seed-{seed}/results.json').read_text());trained=old['episodes'];assert len(trained)==64
        assert all(e['mode']=='mean_steering' for e in trained)
        assert all(e['outcome'] is not None for e in initial)
        rates={k:summarize(v) for k,v in [('initial',initial),('trained',trained)]}
        record={k:new[k] for k in ['seed','physics_ticks','seconds','model_unchanged','optimizer_steps','checkpoint_sha256','execution_contract']}
        record.update(**rates,paired=paired_outcomes(initial,trained),
            repeats={k:[summarize([r for r in v if r['repeat']==i]) for i in range(4)] for k,v in [('initial',initial),('trained',trained)]},
            diagnostics={k:diagnostics(v) for k,v in [('initial',initial),('trained',trained)]},
            material_learning_gain=material_improvement(trained,rates['initial']['success_rate']),
            matched_innovation_ticks=sum(r['pairing']['shared_ticks'] for r in initial),
            maximum_abs_standardized_innovation_error=max(r['pairing']['maximum_abs_standardized_innovation_error'] for r in initial),episodes=initial)
        seeds.append(record);pooled['initial'].extend(initial);pooled['trained'].extend(trained)
    ticks=sum(s['physics_ticks'] for s in seeds);assert ticks<=345600
    atomic_json(args.out,{'experiment_id':args.archive.name,'execution_contract':'mean-steering-sampled-buttons-v1','seeds':seeds,
        'physics_ticks':ticks,'preflight_physics_ticks':128,'trained_reused_physics_ticks':sum(e['ticks'] for e in pooled['trained']),
        'material_learning_gain':sum(s['material_learning_gain'] for s in seeds)>=2,
        'pooled_descriptive_only':{k:{**summarize(v),'diagnostics':diagnostics(v)} for k,v in pooled.items()},
        'qualification_gate_changed':False,'confirmation_used':False,'optimizer_steps':0,'general_flight_claim':False})


if __name__=='__main__':main()
