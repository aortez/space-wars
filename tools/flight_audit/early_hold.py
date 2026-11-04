"""Paired first-exit hold experiment; long-task evaluation remains unchanged."""
import argparse
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.continuous_approach import CRITERIA, read, combine
from tools.flight_audit.hold_position import SEEDS, parity, inspect_evaluation, comparison, aggregate

ARMS = ['long', 'early']
UPDATES = 32


def trace_metrics(trace, allow_active=False):
    early = trace['task'].get('hold_failure') == 'hold-first-exit-v1'
    if trace['task'].get('hold_failure') not in [None, 'hold-first-exit-v1']:
        raise ValueError('unknown terminal contract')
    if trace['version'] != (3 if early else 2) or trace['task']['arrival'] != CRITERIA or trace['task']['deadline_ticks'] != 1800:
        raise ValueError('wrong physical contract')
    steps = trace['steps']
    if not steps or len(steps) > 1800:
        raise ValueError('invalid trace length')
    def tolerated(frame):
        e = frame['errors']
        return e['position'] <= 3 and e['velocity'] <= 1 and abs(e['spin']) <= CRITERIA['spin']
    previous = trace['initial'];dwell = 0
    if previous['errors'] != {'position': 0., 'velocity': 0., 'spin': 0.}:
        raise ValueError('nonzero initial hold errors')
    qualified_before = 0
    for tick, step in enumerate(steps, 1):
        frame = step['frame'];action = step['action'];progress = step['progress'];evidence = step['evidence']
        if frame is None or action is None or step.get('error'):
            raise ValueError('invalid controller/environment trace')
        for key, low in [('steering', -1), ('thrust', 0), ('brake', 0)]:
            if type(action[key]) not in (int, float) or not math.isfinite(action[key]) or not low <= action[key] <= 1:
                raise ValueError('non-continuous or invalid actuator')
        if not all(math.isfinite(v) for v in frame['errors'].values()):
            raise ValueError('nonfinite physical state')
        qualified_before += tolerated(previous)
        dwell = dwell+1 if tolerated(previous) and tolerated(frame) else 0
        failures = [k for k in ['ship_lost', 'collision', 'damage', 'boundary_escape'] if evidence[k]]
        outcome = failures[0] if failures else 'hold_broken' if early and not (tolerated(previous) and tolerated(frame)) else 'success' if dwell >= 30 else 'deadline' if tick == 1800 else None
        if progress != {'elapsed_ticks': tick, 'dwell_ticks': dwell, 'outcome': outcome}:
            raise ValueError('physical outcome/dwell reconstruction differs')
        if outcome is not None and tick != len(steps):
            raise ValueError('trace advances after terminal')
        previous = frame
    outcome = steps[-1]['progress']['outcome']
    if outcome is None and not allow_active:
        raise ValueError('incomplete evaluated episode')
    near = [s for s in steps if s['frame']['errors']['position'] <= 3]
    return {'first_30_ticks': min(30, len(steps)), 'qualified_before_ticks': qualified_before, 'outcome': outcome, 'ticks': len(steps), 'entered_goal_radius': bool(near),
        'met_all_tolerances': any(tolerated(s['frame']) for s in steps),
        'maximum_dwell_ticks': max(s['progress']['dwell_ticks'] for s in steps),
        'minimum_distance': min(s['frame']['errors']['position'] for s in steps),
        'final_errors': steps[-1]['frame']['errors'],
        'maximum_speed': max(s['frame']['errors']['velocity'] for s in steps),
        'maximum_spin': max(abs(s['frame']['errors']['spin']) for s in steps),
        'controls': {key: {'sum': sum(s['action'][key] for s in steps),
            'abs_sum': sum(abs(s['action'][key]) for s in steps),
            'minimum': min(s['action'][key] for s in steps),
            'maximum': max(s['action'][key] for s in steps),
            'near_zero_ticks': sum(abs(s['action'][key]) <= .05 for s in steps),
            'near_full_ticks': sum(abs(s['action'][key]) >= .95 for s in steps)}
            for key in ['steering', 'thrust', 'brake']},
        'simultaneous_demand_ticks': sum(s['action']['thrust'] > .05 and s['action']['brake'] > .05 for s in steps),
        'near_ticks': len(near), 'near_speed_compliant_ticks': sum(s['frame']['errors']['velocity'] <= 1 for s in near),
        'near_spin_compliant_ticks': sum(abs(s['frame']['errors']['spin']) <= CRITERIA['spin'] for s in near)}



def training_inspection(run, update):
    state = torch.load(run/f'checkpoint-{update:04}.pt', weights_only=True)
    summary = read(run/'summary.json')
    traces = [trace_metrics(read(p)) for p in sorted((run/'training-traces').glob('episode-*.json'))]
    traces += [trace_metrics(t, True) for t in state['collector']['traces'] if t['steps']]
    result = combine(traces)
    assert result['ticks'] == summary['environment_steps'] == update*1024
    assert sum(e['ticks'] for e in summary['completed_episodes']+summary['unfinished_episodes']) == result['ticks']
    assert all(math.isfinite(v) for h in summary['history'] for v in h.values() if isinstance(v, float))
    result.update(started_episodes=summary['started_episodes'], censored_episodes=len(summary['unfinished_episodes']),
                  learned_latent_std=state['model']['log_std'].clamp(-5, 2).exp().tolist(),
                  first_30_ticks=sum(t['first_30_ticks'] for t in traces),
                  qualified_before_ticks=sum(t['qualified_before_ticks'] for t in traces))
    result['mean_episode_ticks_including_censored'] = result['ticks']/len(traces)
    return result


def candidates(run, partition):
    batch = read(run/f'{partition}-admission.json')
    assert batch['complete']
    return [r['candidate'] for r in batch['attempts'] if r['admitted']]


def matched_tasks(long, early):
    assert len(long) == len(early)
    for a, b in zip(long, early):
        b = dict(b, recipe=b['recipe'].replace('hold-first-exit-v1/', 'hold-position-v1/'), task=dict(b['task']))
        assert b['task'].pop('hold_failure') == 'hold-first-exit-v1'
        assert a == b


def preflight(a):
    records = []
    for arm in ARMS:
        run=a/f'preflight/{arm}';resumed=a/f'preflight/{arm}-resumed'
        state=torch.load(run/'checkpoint-0002.pt',weights_only=True)
        assert equal_tree(state,torch.load(resumed/'checkpoint-0002.pt',weights_only=True))
        fixtures=candidates(run,'training')[:16]
        p=parity(a,run,2,fixtures,a/f'preflight/{arm}-parity.json')
        records.append({'arm':arm,'exact_resume':True,'parity':p,'training':training_inspection(run,2)})
    matched_tasks(candidates(a/'preflight/long','training'),candidates(a/'preflight/early','training'))
    atomic_json(a/'preflight/verification.json',{'records':records,'training_ticks':6144,'maximum_optimizer_steps':96})


def report(a):
    start=time.monotonic();records=[];dev_reference=None
    for seed in SEEDS:
        initial=[]
        for arm in ARMS:
            run=a/f'runs/{arm}/seed-{seed}';s=read(run/'summary.json')
            assert s['completed_updates']==UPDATES and s['environment_steps']==UPDATES*1024 and s['total_seconds']<=1800
            state=torch.load(run/f'checkpoint-{UPDATES:04}.pt',weights_only=True)
            assert state['history']==s['history']
            initial.append(torch.load(run/'checkpoint-0000.pt',weights_only=True))
            dev=candidates(run,'development')
            assert len(dev)==16 and all('hold_failure' not in c['task'] for c in dev)
            if dev_reference is None:dev_reference=dev
            assert dev==dev_reference
            if arm=='long':
                parent=torch.load(a/f'inputs/parent-checkpoint-{seed}.pt',weights_only=True)
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
       'long_arm_parent_model_optimizer_rng_exact':True,'exact_resume_and_preflight_main':True,
       'confirmation_used':False,'model_promoted':False,'approach_or_navigation_claim':False}
    assert r['training_ticks']==196608 and r['evaluation_ticks']<=345600 and r['report_seconds']<=900
    atomic_json(a/'results.json',r)


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);parser.add_argument('--preflight',action='store_true');args=parser.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.preflight:preflight(args.archive)
    else:report(args.archive)


if __name__=='__main__':main()
