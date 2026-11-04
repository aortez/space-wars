"""LF-073 fixed-state response and within-batch gradient dispersion; no updates."""
import argparse
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_audit.motor_feedback import acceleration, native_parity
from tools.flight_audit.motor_gradient_math import gradient, parameters, cosine
from . import effective_batch as stage, aligned, storage
from .single_speed_audit import feedback_inputs


def partition_gradients(model, rollout, seed):
    flat={k:v.flatten(0,1) for k,v in rollout.items()};x=flat['observations'];move=x[:,2]>0
    adv=aligned.advantages(flat['advantages'],move);params=parameters(model)[1]
    generator=torch.Generator().manual_seed(seed);order=torch.randperm(len(x),generator=generator)
    result={}
    for size in [256,1024,len(x)]:
        rows=[];counts=[]
        for indices in order.split(size):
            actor=model(x[indices])[0]
            ratio=(model.log_prob(actor,flat['latent'][indices],flat['actuator_latents'][indices],x[indices])-flat['logp'][indices]).exp()
            # Evaluate policy loss at saved behavior weights, before PPO epochs/Adam.
            loss=aligned.actor_losses(ratio,adv[indices],torch.zeros_like(ratio),move[indices],{'clip':.2,'stopping_actor_weight':1.,'entropy_weight':0.})[0]
            rows.append(gradient(loss,params));counts.append(int(move[indices].sum()))
        result[str(size)]={'gradients':torch.stack(rows),'movement_counts':counts}
    return result


def run(a):
    start=time.monotonic();x,metadata=feedback_inputs(storage.read_json(a/'inputs/feedback-anchors.json'))
    feedback=[];diagnostics=[];parity_count=0;zero_rows=0
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        for arm in stage.ARMS:
            for seed in stage.SEEDS:
                c=stage.config(seed,arm);run=a/f'pilot/{arm}/seed-{seed}';initial=None
                for phase,u in [('initial',0),('final',c['updates'])]:
                    s=storage.load_torch(run/f'checkpoint-{u:04}.pt.gz')
                    with torch.random.fork_rng(devices=[]):model=stage.make_model(s['contract']['schema'],c)
                    model.load_state_dict(s['model']);model.requires_grad_(False)
                    parity=native_parity(bridge,run/f'policy-{u:04}.json',model,x);parity_count+=len(x)
                    with torch.no_grad():
                        actor=model(x)[0];actions=actor[:,1:].sigmoid();force=acceleration(x,actions);scale=model.distribution(actor,x).scale
                    stop=x[:,2]==0
                    if initial is None:initial=(actor[stop].clone(),scale[stop].clone())
                    else:
                        assert torch.equal(initial[0],actor[stop]) and torch.equal(initial[1],scale[stop]);zero_rows+=int(stop.sum())
                    slopes=[]
                    for i in range(0,len(x),5):
                        slope=(actions[i+3]-actions[i+1])/.5
                        slopes.append({'anchor':metadata[i]['anchor'],'command':metadata[i]['command'],
                            'throttle_per_speed':float(slope[0]),'brake_per_speed':float(slope[1]),
                            'force_per_speed':float((force[i+3]-force[i+1])/.5),
                            'force_under':float(force[i]),'force_target':float(force[i+2]),'force_over':float(force[i+4])})
                    feedback.append({'arm':arm,'seed':seed,'phase':phase,'slopes':slopes,'actions':actions.tolist(),'force':force.tolist(),'parity':parity})
                if arm!='large':continue
                for u in stage.target_updates(c):
                    before=storage.load_torch(run/f'checkpoint-{u-1:04}.pt.gz');rollout=storage.load_torch(run/f'rollout-{u:04}.pt.gz')
                    with torch.random.fork_rng(devices=[]):model=stage.make_model(before['contract']['schema'],c)
                    model.load_state_dict(before['model']);params=parameters(model)[1]
                    force=acceleration(x,model(x)[0][:,1:].sigmoid())
                    masks=[torch.tensor([m['command']==16 and m['error']==error for m in metadata]) for error in [-6,0,6]]
                    jac=torch.stack([gradient(force[mask].mean(),params) for mask in masks])
                    batches=partition_gradients(model,rollout,730000+seed*100+u)
                    full=batches['6144']['gradients'][0]
                    evidence={'jacobian':jac,'partitions':batches,'observations':x,'metadata':metadata}
                    storage.dump_torch(a/f'gradient-evidence/seed-{seed}-update-{u:04}.pt.gz',evidence)
                    sizes={}
                    for size in ['256','1024']:
                        g=batches[size]['gradients'];projection=-(g.double()@jac.double().T)
                        cosines=[cosine(v,full) for v in g]
                        sizes[size]={'batch_count':len(g),'movement_counts':batches[size]['movement_counts'],
                            'force_projections':projection.tolist(),'projection_mean':projection.mean(0).tolist(),
                            'projection_std':projection.std(0,unbiased=False).tolist(),
                            'under_corrective':int((projection[:,0]>1e-6).sum()),'over_corrective':int((projection[:,2]<-1e-6).sum()),
                            'cosines_to_full':cosines,'opposite_full_gradient':sum(v is not None and v<0 for v in cosines)}
                    diagnostics.append({'seed':seed,'update':u,'collected_segments':u*4,'sizes':sizes,
                        'full_force_projection':(-(full.double()@jac.double().T)).tolist()})
                    assert all(torch.equal(before['model'][k],v) for k,v in model.state_dict().items())
    result={'feedback':feedback,'gradient_diagnostics':diagnostics,'inputs':x.tolist(),'metadata':metadata,
        'native_python_comparisons':parity_count,'zero_command_feedback_rows':zero_rows,
        'new_physics_ticks':0,'optimizer_steps':0,'seconds':time.monotonic()-start,
        'interpretation':'One fixed permutation of each large-arm saved batch; shared pooled normalization; each optimizer-sized gradient uses its own movement denominator. Fixed force projections exclude entropy, critic, clipping, Adam and actual policy change. Correlated partitions are not independent training seeds.'}
    assert result['seconds']<600 and parity_count==6480 and zero_rows==1620
    atomic_json(a/'analysis.json',result)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);args=p.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True);run(args.archive)
