"""Fixed update-16 to update-32 continuation; original and boundary baselines stay distinct."""
import argparse
from collections import Counter
from pathlib import Path
import shutil
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.evaluate import evaluate
from tools.flight_ppo.exploration import amplitude
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.continuous_approach import read
from tools.flight_audit.hold_position import comparison as hold_comparison
from tools.flight_audit.settle_recover import SEEDS,candidates,parity,physical,inspect_evaluation
from tools.flight_audit.short_recovery import training_evidence,retention,progression
from tools.flight_audit.rate_training import ARMS,RATES,pilot_gate


def extension_gate(records):
    assert sorted(r['seed'] for r in records)==SEEDS
    per_seed=[]
    for r in records:
        gain=r['primary']['final']['successes']-r['boundary_primary']['successes']
        retained=retention(r['legacy_hold'])
        safe=r['primary']['final']['physical_failures']==0
        per_seed.append({'seed':r['seed'],'gain_from_16':gain,'gain_from_0':r['primary']['final']['successes']-r['primary']['initial']['successes'],
                         'retention':retained,'physical_safety':safe,'joint_improvement_and_retention':gain>=1 and retained and safe})
    return {'per_seed':per_seed,'passed':sum(x['joint_improvement_and_retention'] for x in per_seed)>=2,
            'scope':'eligibility to discuss a separately frozen update-64 extension; never automatic authorization'}


def block_outcomes(before,after):
    prefix=before['collector']['completed'];completed=after['collector']['completed']
    assert equal_tree(completed[:len(prefix)],prefix)
    assert after['history'][:16]==before['history'] and before['update']==16 and after['update']==32
    assert before['collector']['steps']==16384 and after['collector']['steps']==32768
    return dict(Counter(e['outcome'] for e in completed[len(prefix):]))


def materialize_history(source,destination):
    """Copy already completed pre-boundary traces; never replace newly collected evidence."""
    copied=[]
    for p in sorted(source.glob('episode-*.json')):
        target=destination/p.name
        if target.exists():raise FileExistsError(target)
        shutil.copy2(p,target);copied.append(p.name)
    return copied


def extra(a,arm,seed):
    started=time.monotonic();deadline=started+600;run=a/f'continued/{arm}/seed-{seed}';parent=a/f'inputs/{arm}/seed-{seed}'
    old=[r['candidate'] for r in read(a/'inputs/legacy-admission.json')['attempts'] if r['admitted']]
    parities={}
    for endpoint,u in [('initial',16),('final',32)]:
        rows=evaluate(a/'runtime/flight_ppo_bridge',run/f'policy-{u:04}.json',old,run/f'legacy-hold-{endpoint}',deadline)
        if endpoint=='initial':
            for row in rows:assert read(run/f"legacy-hold-initial/clear-{row['index']}.json")==read(parent/f"hold-boundary/clear-{row['index']}.json")
        parities[endpoint]=parity(a,run,u,candidates(run,'development'),run/f'parity-{endpoint}.json') if u==32 else boundary_parity(a,run,parent)
    for row in read(run/'evaluation-initial/results.json'):
        assert read(run/f"evaluation-initial/clear-{row['index']}.json")==read(parent/f"recovery-boundary/clear-{row['index']}.json")
    copied=materialize_history(parent/'training-traces',run/'training-traces')
    atomic_json(run/'extra-summary.json',{'seconds':time.monotonic()-started,'parity':parities,'boundary_traces_exact':True,'copied_historical_traces':copied})


def boundary_parity(a,run,parent):
    assert (run/'policy-0016.json').read_bytes()==(parent/'policy-0016.json').read_bytes()
    return parity(a,parent,16,candidates(run,'development'),run/'parity-initial.json')


def verify(a):
    started=time.monotonic();records=[];old_records=read(a/'inputs/parent-results.json')['records'];restore_ticks=0;resume_steps=0
    for arm in ARMS:
        for seed in SEEDS:
            run=a/f'continued/{arm}/seed-{seed}';parent=a/f'inputs/{arm}/seed-{seed}';s=read(run/'summary.json')
            before=torch.load(parent/'checkpoint-0016.pt',weights_only=True);after=torch.load(run/'checkpoint-0032.pt',weights_only=True)
            again=torch.load(a/f'resumed/{arm}/seed-{seed}/checkpoint-0032.pt',weights_only=True)
            assert equal_tree(after,again) and before['contract']==after['contract']
            assert read(run/'contract.json')==before['contract']
            assert (run/'policy-0016.json').read_bytes()==(parent/'policy-0016.json').read_bytes()
            assert s['completed_updates']==32 and s['environment_steps']==32768 and s['parameter_count']==39447 and s['total_seconds']<1800
            assert before['contract']['config']['learning_rate']==RATES[arm]
            assert read(run/'development-admission.json')==read(a/'inputs/development-admission.json')
            outcomes=block_outcomes(before,after)
            training=training_evidence(run,32);training['effective_latent_std']=[x*amplitude(after['contract']['config']['exploration']) for x in training['latent_std']]
            old=next(r for r in old_records if r['arm']==arm and r['seed']==seed)
            assert physical(s['initial_evaluation'])==old['primary']['final']
            primary={'initial':old['primary']['initial'],'final':physical(s['final_evaluation'])}
            hold_boundary=hold_comparison(read(run/'legacy-hold-initial/results.json'),read(run/'legacy-hold-final/results.json'))
            assert hold_boundary['initial']==old['legacy_hold']['final']
            legacy={'initial':old['legacy_hold']['initial'],'final':hold_boundary['final']}
            evaluation={suite:{k:inspect_evaluation(run/f'{folder}-{k}') for k in ['initial','final']} for suite,folder in [('primary','evaluation'),('hold','legacy-hold')]}
            position={k:{direction:sum(m['outcome']=='success' and m['index']==4*j+1 for m in evaluation['primary'][k]['episodes']) for j,direction in enumerate(['forward','left','backward','right'])} for k in ['initial','final']}
            h=s['history'][16:];assert len(h)==16
            restored31=torch.load(run/'checkpoint-0031.pt',weights_only=True)
            restore_ticks+=sum(len(t['steps']) for state in [before,restored31] for t in state['collector']['traces'])
            resume_steps+=h[-1]['minibatches']
            records.append({'arm':arm,'seed':seed,'updates':32,'new_updates':16,'primary':primary,'boundary_primary':old['primary']['final'],
                            'legacy_hold':legacy,'boundary_hold':old['legacy_hold']['final'],'training_cumulative':training,
                            'new_block_outcomes':outcomes,'new_training_ticks':16384,'evaluation':evaluation,'position_directions':position,
                            'extra':read(run/'extra-summary.json'),'main_seconds':s['total_seconds'],
                            'optimizer_steps':sum(x['minibatches'] for x in h),'kl_early_stops':sum(x['kl_early_stop'] for x in h),
                            'steps_by_update':[x['minibatches'] for x in h]})
    extensions={arm:extension_gate([r for r in records if r['arm']==arm]) for arm in ARMS}
    progresses={arm:progression([r for r in records if r['arm']==arm]) for arm in ARMS}
    # Exposure is cumulative through update 32; newly collected successes are separate.
    gates={arm:pilot_gate([dict(r,training=r['training_cumulative']) for r in records if r['arm']==arm]) for arm in ARMS}
    result={'experiment_id':a.name,'records':records,'extension_gates':extensions,'cumulative_pilot_gates':gates,'progression':progresses,
            'progression_qualified':progresses['smaller']['passed'],'extension_review_eligible':extensions['smaller']['passed'],
            'training_ticks':104448,'restore_ticks':restore_ticks,'optimizer_steps':sum(r['optimizer_steps'] for r in records)+resume_steps,
            'evaluation_ticks':sum(v['ticks'] for r in records for suite in r['evaluation'].values() for v in suite.values()),
            'evaluation_episodes':384,'parity_comparisons':192,'report_seconds':time.monotonic()-started,
            'exact_resumes':True,'boundary_traces_exact':True,'history_prefix_exact':True,
            'update_64_executed':False,'confirmation_used':False,'model_promoted':False,'general_navigation_claim':False}
    assert result['restore_ticks']<=43200 and result['optimizer_steps']<=1632 and result['evaluation_ticks']<=518400 and result['report_seconds']<600
    atomic_json(a/'results.json',result)


def main():
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--extra',choices=ARMS);p.add_argument('--seed',type=int,choices=SEEDS)
    args=p.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.extra:extra(args.archive,args.extra,args.seed)
    else:verify(args.archive)

if __name__=='__main__':main()
