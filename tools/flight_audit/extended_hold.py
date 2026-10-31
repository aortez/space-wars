"""Frozen 128-update repetition of the paired first-exit hold experiment."""
import argparse
import copy
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.continuous_approach import read
from tools.flight_audit.hold_position import SEEDS, parity, inspect_evaluation, comparison, aggregate
from tools.flight_audit.early_hold import ARMS, training_inspection, candidates, matched_tasks, preflight

UPDATES = 128


def verify_parent32(current, parent):
    """Require the complete old state, allowing only the predeclared update cap."""
    if current['update'] != 32 or parent['update'] != 32:
        raise ValueError('update32 checkpoint required')
    if current['contract']['config']['updates'] != 128 or parent['contract']['config']['updates'] != 32:
        raise ValueError('only the declared 32-to-128 update cap comparison is allowed')
    normalized = dict(current, contract=copy.deepcopy(current['contract']))
    normalized['contract']['config']['updates'] = 32
    if not equal_tree(normalized, parent):
        raise ValueError('parent update32 state differs beyond the update cap')


def report(a):
    start=time.monotonic();records=[];dev_reference=None
    for seed in SEEDS:
        initial=[]
        for arm in ARMS:
            run=a/f'runs/{arm}/seed-{seed}';s=read(run/'summary.json')
            assert s['completed_updates']==UPDATES and s['environment_steps']==UPDATES*1024 and s['total_seconds']<=1800
            verify_parent32(torch.load(run/'checkpoint-0032.pt',weights_only=True),
                            torch.load(a/f'inputs/{arm}-checkpoint32-{seed}.pt',weights_only=True))
            state=torch.load(run/f'checkpoint-{UPDATES:04}.pt',weights_only=True)
            assert state['history']==s['history']
            initial.append(torch.load(run/'checkpoint-0000.pt',weights_only=True))
            dev=candidates(run,'development')
            assert len(dev)==16 and all('hold_failure' not in c['task'] for c in dev)
            if dev_reference is None:dev_reference=dev
            assert dev==dev_reference
            if arm=='long':
                parent=torch.load(a/f'inputs/long-checkpoint128-{seed}.pt',weights_only=True)
                for key in ['model','optimizer','torch_rng','python_rng']:assert equal_tree(state[key],parent[key]),key
            evaluation={p:inspect_evaluation(run/f'evaluation-{p}') for p in ['initial','final']}
            parities={p:parity(a,run,n,dev,run/f'parity-{p}.json') for p,n in [('initial',0),('final',UPDATES)]}
            records.append({'arm':arm,'seed':seed,'mean':comparison(s['initial_evaluation'],s['final_evaluation']),
                'evaluation':evaluation,'training':training_inspection(run,UPDATES),'parity':parities,
                'training_seconds':s['training_seconds_including_checkpoints'],'total_seconds':s['total_seconds'],
                'parameter_count':s['parameter_count'],'optimizer_steps':sum(h['minibatches'] for h in s['history'])})
            if seed==17:
                for n in [0,2]:assert equal_tree(torch.load(run/f'checkpoint-{n:04}.pt',weights_only=True),torch.load(a/f'preflight/{arm}/checkpoint-{n:04}.pt',weights_only=True))
            if time.monotonic()-start>900:raise TimeoutError('report budget')
        for key in ['model','optimizer','torch_rng','python_rng']:assert equal_tree(initial[0][key],initial[1][key])
        matched_tasks(candidates(a/f'runs/long/seed-{seed}','training'),candidates(a/f'runs/early/seed-{seed}','training'))
    for arm in ARMS:
        admissions=[read(a/f'runs/{arm}/seed-{seed}/training-admission.json') for seed in SEEDS]
        assert all(v==admissions[0] for v in admissions)
    paired=[]
    for seed in SEEDS:
        arms={v['arm']:v for v in records if v['seed']==seed}
        assert arms['long']['evaluation']['initial']==arms['early']['evaluation']['initial']
        paired.append({'seed':seed,'immediate_hold_difference':arms['early']['mean']['final']['immediate_holds']-arms['long']['mean']['final']['immediate_holds']})
    r={'experiment_id':a.name,'records':records,'gates':{arm:aggregate([v for v in records if v['arm']==arm]) for arm in ARMS},
       'paired':paired,'early_hold_improvement':sum(p['immediate_hold_difference']>=4 for p in paired)>=2,
       'training_ticks':sum(v['training']['ticks'] for v in records),
       'evaluation_ticks':sum(v['evaluation'][p]['summary']['ticks'] for v in records for p in ['initial','final']),
       'preflight_training_ticks':6144,'parity_fixtures':192,'preflight_parity_fixtures':32,'parity_setup_ticks':224,
       'report_seconds':time.monotonic()-start,'matched_starts_and_initial_tensors':True,
       'all_six_parent32_states_exact_except_update_cap':True,
       'long_arm_parent128_model_optimizer_rng_exact':True,'exact_resume_and_preflight_main':True,
       'confirmation_used':False,'model_promoted':False,'approach_or_navigation_claim':False}
    assert r['training_ticks']==786432 and r['evaluation_ticks']<=345600 and r['report_seconds']<=900
    atomic_json(a/'results.json',r)


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);parser.add_argument('--preflight',action='store_true');args=parser.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.preflight:preflight(args.archive)
    else:report(args.archive)


if __name__=='__main__':main()
