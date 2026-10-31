"""Paired learning-rate pilot at quarter noise; no automatic longer training."""
import argparse
import copy
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.evaluate import evaluate
from tools.flight_ppo.exploration import SCALED_KIND,amplitude
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.continuous_approach import read
from tools.flight_audit.hold_position import comparison as hold_comparison
from tools.flight_audit.settle_recover import SEEDS,candidates,parity,physical,inspect_evaluation
from tools.flight_audit.short_recovery import training_evidence,pilot_gate as legacy_pilot_gate,progression
from tools.flight_audit.noise_update import ratio_metrics
from tools.flight_ppo.model import FlightModel

ARMS=['control','smaller']
RATES={'control':.0003,'smaller':.000075}


def pilot_gate(records):
    result=legacy_pilot_gate(records)
    exposure=all(all(len(c['directions'])==(2 if f=='spin' else 4) and
                    (f=='settled' or c['correction_rows']>=256 or c['successes']>=1)
                    for f,c in r['training']['coverage'].items()) for r in records)
    result['checks']['signed_exposure']=exposure
    result['passed']=all(result['checks'].values())
    return result


def assert_matched_initial(control,smaller):
    for key in ['model','collector','torch_rng','python_rng']:assert equal_tree(control[key],smaller[key])
    assert not control['optimizer']['state'] and not smaller['optimizer']['state']
    a=copy.deepcopy(control['optimizer']);b=copy.deepcopy(smaller['optimizer'])
    assert a['param_groups'][0]['lr']==RATES['control'] and b['param_groups'][0]['lr']==RATES['smaller']
    b['param_groups'][0]['lr']=RATES['control'];assert equal_tree(a,b)


def extra(a,phase,arm,seed):
    start=time.monotonic();deadline=start+600;run=a/f'{phase}/{arm}/seed-{seed}';n=16;assert phase=='pilot'
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
    start=time.monotonic();n=16;assert phase=='pilot';records=[]
    for seed in SEEDS:
        zero=[];configs=[]
        for arm in ARMS:
            run=a/f'{phase}/{arm}/seed-{seed}';s=read(run/'summary.json')
            assert s['completed_updates']==n and s['environment_steps']==1024*n and s['parameter_count']==39447 and s['total_seconds']<1800
            state=torch.load(run/'checkpoint-0000.pt',weights_only=True);zero.append(state);config=state['contract']['config'];configs.append(copy.deepcopy(config))
            initial=torch.load(a/f'inputs/seed-{seed}/checkpoint.pt',weights_only=True)
            assert equal_tree(state['model'],initial['model']) and not state['optimizer']['state']
            assert config['exploration']=={'kind':SCALED_KIND,'rho':0.,'std_multiplier':.25}
            assert config['learning_rate']==RATES[arm]
            assert equal_tree(torch.load(run/'checkpoint-0016.pt',weights_only=True),torch.load(a/f'resumed/{arm}/seed-{seed}/checkpoint-0016.pt',weights_only=True))
            if arm=='control':
                for u in [0,1,16]:
                    assert equal_tree(torch.load(run/f'checkpoint-{u:04}.pt',weights_only=True),torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-{u:04}.pt',weights_only=True))
                for suite,folder in [('primary','evaluation-final'),('hold','legacy-hold-final')]:
                    assert read(run/f'{folder}/results.json')==read(a/f'inputs/seed-{seed}/parent-{suite}-final.json')
            first=torch.load(run/'checkpoint-0001.pt',weights_only=True)
            parent_first=torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-0001.pt',weights_only=True)
            assert equal_tree(first['collector'],parent_first['collector'])
            model=FlightModel(first['contract']['schema'],config.get('input_scaling'));model.load_state_dict(first['model'])
            first_ratios=ratio_metrics(model,torch.load(a/f'inputs/seed-{seed}/first-rollout.pt',weights_only=True),config)
            for partition in ['training','development']:
                assert read(run/f'{partition}-admission.json')==read(a/f'inputs/{partition}-admission.json')
            training=training_evidence(run,n);training['effective_latent_std']=[x*amplitude(config['exploration']) for x in training['latent_std']]
            primary={k:physical(s[k+'_evaluation']) for k in ['initial','final']}
            old=hold_comparison(read(run/'legacy-hold-initial/results.json'),read(run/'legacy-hold-final/results.json'))
            evaluation={suite:{k:inspect_evaluation(run/f'{folder}-{k}') for k in ['initial','final']} for suite,folder in [('primary','evaluation'),('hold','legacy-hold')]}
            position={k:{direction:sum(m['outcome']=='success' and m['index']==4*j+1 for m in evaluation['primary'][k]['episodes']) for j,direction in enumerate(['forward','left','backward','right'])} for k in ['initial','final']}
            records.append({'seed':seed,'arm':arm,'updates':n,'primary':primary,'legacy_hold':old,'training':training,'evaluation':evaluation,
                            'position_directions':position,'extra':read(run/'extra-summary.json'),'main_seconds':s['total_seconds'],
                            'optimizer_steps':sum(h['minibatches'] for h in s['history']),
                            'kl_early_stops':sum(h['kl_early_stop'] for h in s['history']),
                            'steps_by_update':[h['minibatches'] for h in s['history']],
                            'first_update_end_batch_ratios':first_ratios})
        configs[1]['learning_rate']=RATES['control'];assert configs[0]==configs[1]
        assert_matched_initial(*zero)
    gates={arm:pilot_gate([r for r in records if r['arm']==arm]) for arm in ARMS}
    legacy_gates={arm:legacy_pilot_gate([r for r in records if r['arm']==arm]) for arm in ARMS}
    progresses={arm:progression([r for r in records if r['arm']==arm]) for arm in ARMS}
    pairs=[]
    for seed in SEEDS:
        f=next(r for r in records if r['seed']==seed and r['arm']=='control');q=next(r for r in records if r['seed']==seed and r['arm']=='smaller')
        assert f['primary']['initial']==q['primary']['initial']
        pairs.append({'seed':seed,'recovery_gain_over_control':q['primary']['final']['successes']-f['primary']['final']['successes'],
                      'retention_gain_over_control':q['legacy_hold']['final']['immediate_holds']-f['legacy_hold']['final']['immediate_holds']})
    result={'experiment_id':a.name,'phase':phase,'records':records,'pilot_gates':gates,'legacy_pilot_gates':legacy_gates,'main_executed':False,
            'progression':progresses,'progression_qualified':progresses['smaller']['passed'],'paired':pairs,
            'smaller_rate_learning_benefit':gates['smaller']['passed'] and progresses['smaller']['passed'] and sum(p['recovery_gain_over_control']>=4 and p['retention_gain_over_control']>=0 for p in pairs)>=2,
            'evaluation_ticks':sum(v['ticks'] for r in records for suite in r['evaluation'].values() for v in suite.values()),'evaluation_episodes':384,
            'report_seconds':time.monotonic()-start,'control_parent_states_exact':True,'exact_resumes':True,'matched_initial_states_except_optimizer_rate':True,
            'confirmation_used':False,'model_promoted':False,'general_navigation_claim':False}
    assert result['report_seconds']<600 and result['evaluation_ticks']<=518400
    atomic_json(a/f'{phase}-results.json',result)


def main():
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--phase',choices=['pilot'],required=True)
    p.add_argument('--extra',choices=ARMS);p.add_argument('--seed',type=int,choices=SEEDS)
    args=p.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.extra:extra(args.archive,args.phase,args.extra,args.seed)
    else:verify(args.archive,args.phase)

if __name__=='__main__':main()
