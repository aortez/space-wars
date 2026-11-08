"""Frozen fresh versus holding-warm initialization on the same stage-0 curriculum."""
import argparse
import copy
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.curriculum import bearing_candidate, generate, SETTLE_RECIPE, BEARING_CURRICULUM
from tools.flight_ppo.initialization import initialize_policy
from tools.flight_ppo.model import FlightModel
from tools.flight_ppo.execution import load_export
from tools.flight_ppo.evaluate import evaluate
from tools.flight_ppo.ppo import GLOBAL_CENTERING
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.goal_response import read
from tools.flight_audit.input_scaling import infer
from tools.flight_audit.settle_recover import SEEDS, candidates, physical, inspect_evaluation, parity
from tools.flight_audit.short_recovery import training_evidence
from tools.flight_audit.hold_position import comparison as hold_comparison
from tools.flight_audit.recovery_response import local_goal, trajectory

ARMS = ['warm', 'fresh']
UPDATES = 64


def stage_candidates(seed, stage):
    rows=[bearing_candidate(seed,4*j+1,stage) for j in range(16)]
    for c in rows:c['task']['arrival']['dwell_ticks']=30
    return rows


def assert_control_replay(current,parent):
    # Production sources, config and runtime are identical: normalize nothing.
    assert equal_tree(current,parent)


def stage_summary(rows):
    assert len(rows)==16 and sorted(r['index'] for r in rows)==[4*j+1 for j in range(16)]
    return {'successes':sum(r['outcome']=='success' for r in rows),
            'left':sum(r['outcome']=='success' and (r['index']//4)%2==0 for r in rows),
            'right':sum(r['outcome']=='success' and (r['index']//4)%2==1 for r in rows),
            'physical_failures':sum(r['outcome'] not in ['success','deadline'] for r in rows)}


def benefit(records):
    pairs=[]
    for seed in SEEDS:
        base=next(v for v in records if v['seed']==seed and v['arm']=='warm')
        candidate=next(v for v in records if v['seed']==seed and v['arm']=='fresh')
        s=candidate['stage']['final'];initial=candidate['stage']['initial'];h=candidate['legacy_hold']['final']
        competence=s['successes']>=12 and min(s['left'],s['right'])>=6
        gain=s['successes']>=initial['successes']+2 and s['successes']>=base['stage']['final']['successes']+2
        retained=h['immediate_holds']>=12 and min(h['immediate_holds_by_stratum'])>=2
        broad=candidate['primary']['final']['successes']>=max(candidate['primary']['initial']['successes'],base['primary']['initial']['successes'])
        safe=all(candidate[key]['final']['physical_failures']==0 for key in ['stage','primary','legacy_hold'])
        exposed=all(v['all_position_training_cases_completed'] for v in [base,candidate])
        pairs.append({'seed':seed,'stage_competence':competence,'paired_learning_gain':gain,'holding_qualified':retained,
                      'broad_recovery_floor_met':broad,'physical_safety':safe,'full_position_exposure':exposed,
                      'joint_initialization_benefit':competence and gain and retained and broad and safe and exposed})
    return {'per_seed':pairs,'passed':sum(v['joint_initialization_benefit'] for v in pairs)>=2}


def preflight(a):
    start=time.monotonic();records=[]
    with Bridge(a/'runtime/flight_ppo_bridge') as bridge:
        spec={'kind':BEARING_CURRICULUM,'stage':0}
        train,_=generate(bridge,4510001,16,recipe=SETTLE_RECIPE,recovery_dwell_ticks=30,position_curriculum=spec)
        assert train==read(a/'inputs/training-admission.json') and train['complete']
        atomic_json(a/'preflight/training-admission.json',train)
        development,_=generate(bridge,4510002,4,recipe=SETTLE_RECIPE,recovery_dwell_ticks=30)
        assert development==read(a/'inputs/development-admission.json') and development['complete']
        atomic_json(a/'preflight/development-admission.json',development)
        reports=[bridge.call('inspect',candidate=c) for c in stage_candidates(4510002,0)]
        assert reports==read(a/'inputs/stage-admission.json') and all(v['admitted'] for v in reports)
        atomic_json(a/'preflight/stage-0-evaluation-admission.json',reports)
        holds=[bridge.call('inspect',candidate=v['candidate']) for v in read(a/'inputs/legacy-admission.json')['attempts'] if v['admitted']]
        assert all(v['admitted'] for v in holds);atomic_json(a/'preflight/hold-admission.json',holds)
        for seed in SEEDS:
            original=torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-0000.pt',weights_only=True)
            states=[];outputs=[];rngs=[]
            for arm in ARMS:
                config=read(a/f'configs/{arm}/{seed}.json')
                with torch.random.fork_rng(devices=[]):
                    torch.manual_seed(seed);model=FlightModel(bridge.contract['schema'],config['input_scaling'],config['architecture'])
                    standard=copy.deepcopy(model.state_dict());initial_rng=torch.get_rng_state().clone()
                    path=a/f'inputs/seed-{seed}/policy.json' if arm=='warm' else None
                    initialize_policy(model,config,path)
                    assert torch.equal(initial_rng,torch.get_rng_state())
                    if arm=='warm':assert equal_tree(model.state_dict(),original['model'])
                    else:assert equal_tree(model.state_dict(),standard)
                    states.append(copy.deepcopy(model.state_dict()));rngs.append(torch.get_rng_state().clone())
                    out=a/f'preflight/{arm}/seed-{seed}';out.mkdir(parents=True)
                    policy=out/'policy-0000.json';model.export(policy)
                    if arm=='warm':assert policy.read_bytes()==(a/f'inputs/seed-{seed}/initial-policy.json').read_bytes()
                    folded=load_export(policy,bridge.contract['schema']);bridge.call('load_policy',path=str(policy.resolve()))
                    raw,commands,native,errors=infer(bridge,model,folded,[v['inputs'] for v in original['collector']['items']])
                    record={'arm':arm,'seed':seed,'python_raw':raw,'python_actions':commands,'native':native,'errors':errors,'latent_std':model.log_std.clamp(-5,2).exp().tolist()}
                    atomic_json(out/'outputs.json',record);records.append(record);outputs.append([raw,commands,native])
            assert torch.equal(rngs[0],rngs[1]) and torch.equal(rngs[0],original['torch_rng'])
            assert not equal_tree(states[0],states[1]) and outputs[0]!=outputs[1]
            assert all(torch.equal(v,states[1][k]) for k,v in states[0].items() if k.startswith('goal_'))
    result={'records':records,'warm_initial_weights_exports_exact':True,'fresh_matches_standard_initialization':True,
            'paired_sampler_rng_exact':True,'same_initial_goal_branch':True,'native_python_comparisons':24,
            'same_stage0_matrix_admitted':True,'admission_setups':112,'seconds':time.monotonic()-start}
    assert result['seconds']<600;atomic_json(a/'preflight-results.json',result)


def extra(a,arm,seed):
    start=time.monotonic();deadline=start+600;run=a/f'pilot/{arm}/seed-{seed}'
    holds=[v['candidate'] for v in read(a/'inputs/legacy-admission.json')['attempts'] if v['admitted']]
    stages=[v['candidate'] for v in read(a/'preflight/stage-0-evaluation-admission.json')]
    suites={'primary':candidates(run,'development'),'hold':holds,'stage':stages};parities={}
    for endpoint,update in [('initial',0),('final',UPDATES)]:
        for suite,cs in [('hold',holds),('stage',stages)]:
            evaluate(a/'runtime/flight_ppo_bridge',run/f'policy-{update:04}.json',cs,run/f'{suite}-evaluation-{endpoint}',deadline)
        parities[endpoint]={suite:parity(a,run,update,cs,run/f'parity-{suite}-{endpoint}.json') for suite,cs in suites.items()}
    exact=False
    if arm=='warm':
        for suite,folder in [('primary','evaluation'),('hold','hold-evaluation'),('stage','stage-evaluation')]:
            for endpoint in ['initial','final']:
                for row in read(run/f'{folder}-{endpoint}/results.json'):
                    name=f"clear-{row['index']}.json"
                    assert read(run/f'{folder}-{endpoint}'/name)==read(a/f'inputs/seed-{seed}/warm-reference/{suite}-{endpoint}'/name)
        exact=True
    atomic_json(run/'extra-summary.json',{'seconds':time.monotonic()-start,'parity':parities,'warm_parent_all_traces_exact':exact})


def signals(run):
    previous=torch.load(run/'checkpoint-0000.pt',weights_only=True)['model'];rows=[]
    for update in range(1,UPDATES+1):
        s=read(run/f'advantages-{update:04}.json');raw=torch.tensor(s['raw_advantages']);labels=torch.tensor(s['family_ids'])
        expected=(raw-raw.mean())/raw.std(unbiased=False).clamp_min(1e-8)
        assert s['mode']==GLOBAL_CENTERING and labels.tolist()==list(range(4))*256
        assert torch.equal(expected,torch.tensor(s['normalized_advantages']))
        for i,name in enumerate(['settled','position','velocity','spin']):
            mask=labels==i;rr=raw[mask];nn=expected[mask]
            assert s['families'][name]=={'rows':256,'raw_mean':float(rr.mean()),'normalized_mean':float(nn.mean()),'raw_positive_rows':int((rr>0).sum()),'normalized_positive_rows':int((nn>0).sum())}
        current=torch.load(run/f'checkpoint-{update:04}.pt',weights_only=True)['model']
        deltas={k:float((v.double()-previous[k].double()).square().sum()) for k,v in current.items()}
        rows.append({'update':update,'families':s['families'],'parameter_delta_norms':{
            'all_parameters':sum(deltas.values())**.5,'goal_branch':sum(v for k,v in deltas.items() if k.startswith('goal_'))**.5}})
        previous=current
    assert read(run/'advantages-0064.json')==read(run.parents[2]/f'resumed/{run.parent.name}/{run.name}/advantages-0064.json')
    atomic_json(run/'learning-signals.json',rows);return rows


def verify(a):
    start=time.monotonic();records=[];restore_ticks=0;resume_steps=0
    assert read(a/'preflight-results.json')['paired_sampler_rng_exact']
    for seed in SEEDS:
        zeros=[];configs=[]
        for arm in ARMS:
            run=a/f'pilot/{arm}/seed-{seed}';summary=read(run/'summary.json')
            assert summary['completed_updates']==UPDATES and summary['environment_steps']==65536 and summary['parameter_count']==39546 and summary['total_seconds']<1800
            zero=torch.load(run/'checkpoint-0000.pt',weights_only=True);zeros.append(zero);config=zero['contract']['config'];configs.append(copy.deepcopy(config))
            assert not zero['optimizer']['state'] and config['exploration']['rho']==0 and config['exploration']['std_multiplier']==.25
            assert (run/'policy-0000.json').read_bytes()==(a/f'preflight/{arm}/seed-{seed}/policy-0000.json').read_bytes()
            final=torch.load(run/'checkpoint-0064.pt',weights_only=True)
            assert equal_tree(final,torch.load(a/f'resumed/{arm}/seed-{seed}/checkpoint-0064.pt',weights_only=True))
            previous=torch.load(run/'checkpoint-0063.pt',weights_only=True)
            restore_ticks+=sum(len(v['steps']) for v in previous['collector']['traces']);resume_steps+=summary['history'][-1]['minibatches']
            if arm=='warm':
                for update in [0,1,16,64]:assert_control_replay(torch.load(run/f'checkpoint-{update:04}.pt',weights_only=True),torch.load(a/f'inputs/seed-{seed}/parent-checkpoint-{update:04}.pt',weights_only=True))
            expected_training=a/'inputs/training-admission.json'
            assert read(run/'training-admission.json')==read(expected_training)
            assert read(run/'development-admission.json')==read(a/'inputs/development-admission.json')
            evaluation={suite:{phase:inspect_evaluation(run/f'{folder}-{phase}') for phase in ['initial','final']} for suite,folder in [('primary','evaluation'),('hold','hold-evaluation'),('stage','stage-evaluation')]}
            primary={phase:physical(summary[phase+'_evaluation']) for phase in ['initial','final']}
            stage={phase:stage_summary(read(run/f'stage-evaluation-{phase}/results.json')) for phase in ['initial','final']}
            holds=hold_comparison(read(run/'hold-evaluation-initial/results.json'),read(run/'hold-evaluation-final/results.json'))
            training=training_evidence(run,UPDATES)
            training['coverage']['position']['round_mod4']=training['coverage']['position'].pop('directions')
            position_completed=[e for e in final['collector']['completed'] if e['candidate']['index']%4==1]
            exposed={e['candidate']['index'] for e in position_completed}=={4*j+1 for j in range(16)}
            assert exposed
            position_training=[]
            for e in position_completed:
                trace=read(run/f"training-traces/episode-{e['id']:06}.json")
                position_training.append({'episode':e['id'],'index':e['candidate']['index'],'outcome':e['outcome'],
                                          'initial_bearing_degrees':math.degrees(math.atan2(*local_goal(trace['initial']))),'ticks':e['ticks']})
            movements={phase:[{'index':row['index'],**trajectory(read(run/f"stage-evaluation-{phase}/clear-{row['index']}.json"))} for row in read(run/f'stage-evaluation-{phase}/results.json')] for phase in ['initial','final']}
            atomic_json(run/'stage-trajectories.json',movements)
            records.append({'arm':arm,'seed':seed,'primary':primary,'stage':stage,'legacy_hold':holds,'evaluation':evaluation,
                            'training':training,'position_training_episodes':position_training,'all_position_training_cases_completed':exposed,
                            'learning_signals':signals(run),'stage_trajectories':movements,'extra':read(run/'extra-summary.json'),
                            'optimizer_steps':sum(h['minibatches'] for h in summary['history']),'kl_early_stops':sum(h['kl_early_stop'] for h in summary['history']),
                            'main_seconds':summary['total_seconds']})
        assert configs[0].pop('warm_start_sha256')==read(a/f'inputs/seed-{seed}/config.json')['warm_start_sha256'] and configs[0]==configs[1]
        for key in ['optimizer','collector','torch_rng','python_rng']:assert equal_tree(zeros[0][key],zeros[1][key])
        assert not equal_tree(zeros[0]['model'],zeros[1]['model'])
        with Bridge(a/'runtime/flight_ppo_bridge') as bridge,torch.random.fork_rng(devices=[]):
            torch.manual_seed(seed);config=zeros[1]['contract']['config']
            standard=FlightModel(bridge.contract['schema'],config['input_scaling'],config['architecture'])
            assert equal_tree(standard.state_dict(),zeros[1]['model'])
            assert torch.equal(torch.get_rng_state(),zeros[1]['torch_rng'])
    result={'experiment_id':a.name,'records':records,'paired_benefit':benefit(records),'training_ticks':399360,'restore_ticks':restore_ticks,
            'optimizer_steps':sum(v['optimizer_steps'] for v in records)+resume_steps,'evaluation_episodes':576,
            'evaluation_ticks':sum(v['ticks'] for row in records for suite in row['evaluation'].values() for v in suite.values()),
            'parity_comparisons':576,'preflight_comparisons':24,'total_native_python_comparisons':600,
            'initial_conditions_verified':True,'paired_initial_collectors_optimizers_rng_exact':True,'warm_control_parent_0_1_16_64_exact':True,
            'exact_resumes':True,'advantage_records_verified':390,'update_deltas_measured':384,
            'stage_executed':0,'later_stages_executed':False,'confirmation_used':False,'model_promoted':False,'general_navigation_claim':False,'report_seconds':time.monotonic()-start}
    assert restore_ticks<=21600 and result['optimizer_steps']<=6240 and result['evaluation_ticks']<=691200 and result['report_seconds']<600
    atomic_json(a/'results.json',result)


def main():
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--preflight',action='store_true')
    p.add_argument('--extra',choices=ARMS);p.add_argument('--seed',type=int,choices=SEEDS);args=p.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.preflight:preflight(args.archive)
    elif args.extra:extra(args.archive,args.extra,args.seed)
    else:verify(args.archive)


if __name__=='__main__':main()
