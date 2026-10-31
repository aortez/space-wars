"""Paired full/quarter exploration learning with fixed short-recovery gates."""
import argparse
import copy
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.evaluate import evaluate
from tools.flight_ppo.exploration import KIND,SCALED_KIND,amplitude
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.continuous_approach import read
from tools.flight_audit.hold_position import comparison as hold_comparison
from tools.flight_audit.settle_recover import SEEDS,candidates,parity,physical,inspect_evaluation
from tools.flight_audit.short_recovery import training_evidence,pilot_gate,progression

ARMS=['full','quarter']


def assert_parent_replay(current,parent):
    """Only the semantically equivalent full-noise descriptor and source hashes differ."""
    current=copy.deepcopy(current)
    expected={'kind':SCALED_KIND,'rho':0.,'std_multiplier':1.}
    assert current['contract']['config']['exploration']==expected
    assert parent['contract']['config']['exploration']=={'kind':KIND,'rho':0.}
    changed={k for k,v in current['contract']['python_sources'].items() if parent['contract']['python_sources'].get(k)!=v}
    assert changed=={'exploration.py','collector.py','ppo.py','run.py'}
    current['contract']['python_sources']=copy.deepcopy(parent['contract']['python_sources'])
    current['contract']['config']['exploration']=copy.deepcopy(parent['contract']['config']['exploration'])
    if current['collector']['exploration_state'] is not None:
        assert current['collector']['exploration_state']['spec']==expected
        current['collector']['exploration_state']['spec']=copy.deepcopy(parent['collector']['exploration_state']['spec'])
    assert equal_tree(current,parent)


def extra(a,phase,arm,seed):
    start=time.monotonic();deadline=start+600;run=a/f'{phase}/{arm}/seed-{seed}';n=16 if phase=='pilot' else 64
    old=[r['candidate'] for r in read(a/'inputs/legacy-admission.json')['attempts'] if r['admitted']]
    parities={}
    for endpoint,u in [('initial',0),('final',n)]:
        rows=evaluate(a/'runtime/flight_ppo_bridge',run/f'policy-{u:04}.json',old,run/f'legacy-hold-{endpoint}',deadline)
        if endpoint=='initial':
            for row in rows:assert read(run/f"legacy-hold-initial/clear-{row['index']}.json")==read(a/f"inputs/seed-{seed}/hold-baseline/clear-{row['index']}.json")
        parities[endpoint]=parity(a,run,u,candidates(run,'development'),run/f'parity-{endpoint}.json')
    for row in read(run/'evaluation-initial/results.json'):
        assert read(run/f"evaluation-initial/clear-{row['index']}.json")==read(a/f"inputs/seed-{seed}/recovery-baseline/clear-{row['index']}.json")
    atomic_json(run/'extra-summary.json',{'seconds':time.monotonic()-start,'parity':parities,'initial_traces_exact':True})


def verify(a,phase):
    start=time.monotonic();n=16 if phase=='pilot' else 64;records=[]
    for seed in SEEDS:
        zero=[];configs=[]
        for arm in ARMS:
            run=a/f'{phase}/{arm}/seed-{seed}';s=read(run/'summary.json')
            assert s['completed_updates']==n and s['environment_steps']==1024*n and s['parameter_count']==39447 and s['total_seconds']<1800
            state=torch.load(run/'checkpoint-0000.pt',weights_only=True);zero.append(state);config=state['contract']['config'];configs.append(copy.deepcopy(config))
            initial=torch.load(a/f'inputs/seed-{seed}/checkpoint.pt',weights_only=True)
            assert equal_tree(state['model'],initial['model']) and not state['optimizer']['state']
            assert config['exploration']=={'kind':SCALED_KIND,'rho':0.,'std_multiplier':1. if arm=='full' else .25}
            if phase=='pilot':
                assert equal_tree(torch.load(run/'checkpoint-0016.pt',weights_only=True),torch.load(a/f'resumed/{arm}/seed-{seed}/checkpoint-0016.pt',weights_only=True))
                if arm=='full':
                    for u in [0,16]:assert_parent_replay(torch.load(run/f'checkpoint-{u:04}.pt',weights_only=True),torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-{u:04}.pt',weights_only=True))
                    for suite,folder in [('primary','evaluation-final'),('hold','legacy-hold-final')]:
                        assert read(run/f'{folder}/results.json')==read(a/f'inputs/seed-{seed}/parent-{suite}-final.json')
            else:
                for u in [0,16]:assert equal_tree(torch.load(run/f'checkpoint-{u:04}.pt',weights_only=True),torch.load(a/f'pilot/{arm}/seed-{seed}/checkpoint-{u:04}.pt',weights_only=True))
            for partition in ['training','development']:
                assert read(run/f'{partition}-admission.json')==read(a/f'inputs/{partition}-admission.json')
            training=training_evidence(run,n);training['effective_latent_std']=[x*amplitude(config['exploration']) for x in training['latent_std']]
            primary={k:physical(s[k+'_evaluation']) for k in ['initial','final']}
            old=hold_comparison(read(run/'legacy-hold-initial/results.json'),read(run/'legacy-hold-final/results.json'))
            evaluation={suite:{k:inspect_evaluation(run/f'{folder}-{k}') for k in ['initial','final']} for suite,folder in [('primary','evaluation'),('hold','legacy-hold')]}
            position={k:{direction:sum(m['outcome']=='success' and m['index']==4*j+1 for m in evaluation['primary'][k]['episodes']) for j,direction in enumerate(['forward','left','backward','right'])} for k in ['initial','final']}
            records.append({'seed':seed,'arm':arm,'updates':n,'primary':primary,'legacy_hold':old,'training':training,'evaluation':evaluation,
                            'position_directions':position,'extra':read(run/'extra-summary.json'),'main_seconds':s['total_seconds'],
                            'optimizer_steps':sum(h['minibatches'] for h in s['history'])})
        configs[1]['exploration']['std_multiplier']=1.;assert configs[0]==configs[1]
        for k in ['model','optimizer','collector','torch_rng','python_rng']:assert equal_tree(zero[0][k],zero[1][k])
    gates={arm:pilot_gate([r for r in records if r['arm']==arm]) for arm in ARMS} if phase=='pilot' else read(a/'pilot-results.json')['pilot_gates']
    progresses={arm:progression([r for r in records if r['arm']==arm]) for arm in ARMS}
    pairs=[]
    for seed in SEEDS:
        f=next(r for r in records if r['seed']==seed and r['arm']=='full');q=next(r for r in records if r['seed']==seed and r['arm']=='quarter')
        assert f['primary']['initial']==q['primary']['initial']
        pairs.append({'seed':seed,'recovery_gain_over_full':q['primary']['final']['successes']-f['primary']['final']['successes'],
                      'retention_gain_over_full':q['legacy_hold']['final']['immediate_holds']-f['legacy_hold']['final']['immediate_holds']})
    result={'experiment_id':a.name,'phase':phase,'records':records,'pilot_gates':gates,'main_authorized_by_gate':gates['quarter']['passed'],
            'progression':progresses,'progression_qualified':phase=='main' and progresses['quarter']['passed'],'paired':pairs,
            'quarter_learning_benefit':phase=='main' and sum(p['recovery_gain_over_full']>=4 for p in pairs)>=2,
            'evaluation_ticks':sum(v['ticks'] for r in records for suite in r['evaluation'].values() for v in suite.values()),'evaluation_episodes':384,
            'report_seconds':time.monotonic()-start,'full_parent16_reproduction':True,'exact_resumes':True,'matched_initial_states':True,
            'confirmation_used':False,'model_promoted':False,'general_navigation_claim':False}
    assert result['report_seconds']<600 and result['evaluation_ticks']<=518400
    atomic_json(a/f'{phase}-results.json',result)


def main():
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--phase',choices=['pilot','main'],required=True)
    p.add_argument('--extra',choices=ARMS);p.add_argument('--seed',type=int,choices=SEEDS)
    args=p.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.extra:extra(args.archive,args.phase,args.extra,args.seed)
    else:verify(args.archive,args.phase)

if __name__=='__main__':main()
