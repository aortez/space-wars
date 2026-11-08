"""LF-062 read-only velocity-feedback audit; no optimizer or physics steps."""
import argparse
import copy
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from tools.flight_motor import aligned,sustained,storage
from tools.flight_motor.model import MotorModel

ARMS=['acquire','sustain'];SEEDS=[17,29,43]
UPDATES=[0,1,4,16,32,64,128]
CASES=[0,16,32];TICKS=[0,360,600]
COMMANDS=[0,4,6,10,12,16,18,20];ERRORS=[-12,-6,-3,-1,0,1,3,6,12]
read=storage.read_json;load=storage.load_torch


def direction(x):return torch.stack((-x[:,0],x[:,1]),dim=1)


def alter(x,command,forward):
    """Translate COM and origin together, preserving lateral/spin offsets."""
    out=x.clone();q=direction(x);current=(x[:,3:5]*20*q).sum(1)
    delta=(torch.as_tensor(forward)-current)[:,None]*q/20
    out[:,2]=torch.as_tensor(command)/20
    out[:,3:5]+=delta;out[:,5:7]+=delta
    return out


def grid(anchors,commands=COMMANDS,errors=ERRORS):
    rows=[];meta=[]
    for index,anchor in enumerate(anchors):
        x=torch.tensor([anchor['inputs']],dtype=torch.float32)
        for command in commands:
            for error in errors:
                rows.append(alter(x,[command],[command+error])[0])
                meta.append([index,command,error])
    return torch.stack(rows),torch.tensor(meta,dtype=torch.int64)


def acceleration(x,act):
    """Instantaneous native control term for fixed open-wing stationary frame.

    This is an algebraic force estimate, not a new environment simulation.
    Native braking uses origin velocity; keep its COM offset in counterfactuals.
    """
    q=direction(x);v=x[:,5:7]*20;speed=v.norm(dim=1)
    governor=((70-v[:,1])/10).clamp(0,1)
    thrust=act[:,0]*(x[:,10]*45)*governor*q[:,1]
    brake=act[:,1]*torch.minimum(speed*(x[:,14]*4),x[:,13]*40)*(v*q).sum(1)/speed.clamp_min(1e-12)
    return thrust-brake


def components(model,x):
    base,value=MotorModel.forward(model,x)
    f=sustained.residual_features(x)
    z1=model.residual1(f);h1=z1.tanh();z2=model.residual2(h1);h2=z2.tanh()
    residual=sustained.gate(x)[:,None]*model.residual(h2)
    total=base[:,1:]+residual
    return {'base_logits':base[:,1:],'residual_logits':residual,'logits':total,
            'actions':total.sigmoid(),'base_actions':base[:,1:].sigmoid(),
            'acceleration':acceleration(x,total.sigmoid()),
            'hidden_saturation':torch.stack(((h1.abs()>.95).float().mean(1),(h2.abs()>.95).float().mean(1)),dim=1)}


def derivatives(model,x):
    x=x.clone().requires_grad_(True);q=direction(x).detach()
    base,_=MotorModel.forward(model,x);total,_=model(x)
    f=sustained.residual_features(x).detach().requires_grad_(True)
    residual=sustained.gate(x).detach()[:,None]*model.residual(model.residual2(model.residual1(f).tanh()).tanh())
    rows={k:[] for k in ['base','residual_raw_velocity','residual_error','total','command','previous_action']}
    for channel in [1,2]:
        gb=torch.autograd.grad(base[:,channel].sum(),x,retain_graph=True)[0]
        gt=torch.autograd.grad(total[:,channel].sum(),x,retain_graph=True)[0]
        gf=torch.autograd.grad(residual[:,channel-1].sum(),f,retain_graph=True)[0]
        rows['base'].append(((gb[:,3:5]+gb[:,5:7])*q).sum(1)/20)
        rows['residual_raw_velocity'].append(((gf[:,3:5]+gf[:,5:7])*q).sum(1)/20)
        rows['residual_error'].append(-(gf[:,18:20]*q).sum(1)/20)
        rows['total'].append(((gt[:,3:5]+gt[:,5:7])*q).sum(1)/20)
        rows['command'].append(gt[:,2]/20)
        rows['previous_action'].append(gt[:,15:18])
    out={k:torch.stack(v,dim=1).detach() for k,v in rows.items()}
    torch.testing.assert_close(out['total'],out['base']+out['residual_raw_velocity']+out['residual_error'],atol=1e-7,rtol=1e-5)
    velocity=(x.detach()[:,3:5]*20*q).sum(1);command=x.detach()[:,2]*20
    with torch.no_grad():
        lo=alter(x.detach(),command,velocity-.25);hi=alter(x.detach(),command,velocity+.25)
        expected=(model(hi)[0][:,1:]-model(lo)[0][:,1:])/.5
        torch.testing.assert_close(expected,out['total'],atol=1e-5,rtol=.03)
        action=total[:,1:].sigmoid().detach()
        out['action_velocity']=out['total']*action*(1-action)
        out['acceleration_velocity']=(acceleration(hi,model(hi)[0][:,1:].sigmoid())-acceleration(lo,model(lo)[0][:,1:].sigmoid()))/.5
        out['passive_acceleration_velocity']=(acceleration(hi,action)-acceleration(lo,action))/.5
    return out


def training_groups(x):
    q=direction(x);v=x[:,3:5]*20;command=x[:,2]*20;error=(v*q).sum(1)-command
    lateral=(v*torch.stack((q[:,1],-q[:,0]),dim=1)).sum(1).abs()
    ready=(torch.atan2(x[:,0],x[:,1]).abs()<=math.radians(5))&(x[:,7].abs()*2<=.2)&(lateral<=1)
    move=command>0
    return error,{'movement':move,'stopping':~move,'ready_under':move&ready&(error < -1),
                  'ready_near':move&ready&(error.abs()<=1),'ready_over':move&ready&(error>1),
                  **{f'ready_over_command_{s}':move&ready&(error>1)&((command-s).abs()<1e-5) for s in [4,10,16]}}


def summarize_training(rollout,model,after):
    f={k:v.flatten(0,1) for k,v in rollout.items()};x=f['observations'];error,masks=training_groups(x)
    with torch.no_grad():
        actor,_=model(x);future,_=after(x);dist=model.distribution(actor,x)
        logp=model.log_prob(actor,f['latent'],f['actuator_latents'],x)
        torch.testing.assert_close(logp,f['logp'],atol=1e-4,rtol=1e-5)
        adv=aligned.advantages(f['advantages'],x[:,2]>0)
        mean=actor[:,1:].sigmoid();sample=f['actuator_latents'].sigmoid();delta=future[:,1:].sigmoid()-mean
        score=adv[:,None]*(f['actuator_latents']-actor[:,1:])/dist.scale.square()
        am=acceleration(x,mean);ass=acceleration(x,sample)
        fields={'advantage':adv,'mean_acceleration':am,'sample_acceleration':ass,'mean_thrust':mean[:,0],'mean_brake':mean[:,1],
                'delta_thrust':delta[:,0],'delta_brake':delta[:,1],'thrust_score':score[:,0],'brake_score':score[:,1],
                'sample_more_corrective':((ass-am)*error < 0).float(),'mean_corrective':(am*error<0).float(),
                'sample_more_corrective_positive_advantage':(((ass-am)*error<0)&(adv>0)).float()}
        out={}
        for name,mask in masks.items():
            n=int(mask.sum());out[name]={'count':n,**{k:float(v[mask].double().sum()) for k,v in fields.items()}}
        return out,float((logp-f['logp']).abs().max())


def native_parity(bridge,policy,model,x):
    bridge.call('load_policy',path=str(policy.resolve()));errors=[]
    for batch in x.split(256):
        native=bridge.call('infer',inputs=batch.tolist())['outputs']
        with torch.no_grad():actor,value=model(batch)
        expected=torch.cat((actor,value[:,None]),dim=1)
        actual=torch.tensor([[n[k] for k in ['steering_latent','thrust_logit','brake_logit','value']] for n in native])
        torch.testing.assert_close(actual,expected,atol=1e-5,rtol=1e-5)
        errors.append(float((actual-expected).abs().max()))
    return {'count':len(x),'max_raw_error':max(errors)}


def audit(a,arm,seed):
    start=time.monotonic();out=a/f'audit/{arm}/seed-{seed}';out.mkdir()
    run=a/f'inputs/{arm}/seed-{seed}';anchors=read(a/'anchors.json')
    x,meta=grid(anchors);hx,hmeta=grid(anchors,[6,12,18],[-3,0,3]);dx=x[meta[:,2]==0]
    first=load(run/'checkpoint-0000.pt.gz');rng=torch.get_rng_state().clone()
    assert torch.equal(x[:,10],torch.ones(len(x))) and not x[:,8:10].count_nonzero()
    with torch.random.fork_rng(devices=[]):
        model=sustained.ResidualModel(first['contract']['schema']);after=sustained.ResidualModel(first['contract']['schema'])
    model.requires_grad_(False);after.requires_grad_(False)
    history=[];grids=[];training=[];parity=[];maximum=0.;prior=first
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        for update in range(129):
            current=first if update==0 else load(run/f'checkpoint-{update:04}.pt.gz')
            sustained.assert_parent_actor(first['model'],current['model']);model.load_state_dict(current['model'])
            snapshot=copy.deepcopy(model.state_dict())
            with torch.no_grad():history.append({'update':update,**components(model,hx)})
            if update in UPDATES:
                with torch.no_grad():grids.append({'update':update,**components(model,x)})
            if update in [0,128]:parity.append({'update':update,**native_parity(bridge,run/f'policy-{update:04}.json',model,x)})
            if update:
                after.load_state_dict(current['model']);model.load_state_dict(prior['model'])
                stats,error=summarize_training(load(run/f'rollout-{update:04}.pt.gz'),model,after)
                training.append({'update':update,'groups':stats});maximum=max(maximum,error)
                model.load_state_dict(current['model'])
            assert equal_tree(snapshot,model.state_dict())
            prior=current
            if time.monotonic()-start>600:raise TimeoutError('feedback audit budget')
    d=derivatives(model,dx)
    neutral=x.clone();neutral[:,15:18]=0
    with torch.no_grad():neutral_result=components(model,neutral)
    assert torch.equal(rng,torch.get_rng_state()) and equal_tree(prior['model'],model.state_dict())
    storage.dump_torch(out/'responses.pt.gz',{'grid_inputs':x,'grid_metadata':meta,'grids':grids,'derivative_inputs':dx,'derivatives':d,
        'neutral_history':neutral_result,'history_inputs':hx,'history_metadata':hmeta,'history':history})
    atomic_json(out/'training.json',training)
    final=grids[-1];initial=grids[0];positive=meta[:,1]>0;over=positive&(meta[:,2]>1);under=positive&(meta[:,2]<-1)
    aggregate={}
    for row in training:
        for group,fields in row['groups'].items():
            target=aggregate.setdefault(group,{k:0 for k in fields})
            for k,v in fields.items():target[k]+=v
    for group,fields in aggregate.items():
        n=fields['count']
        aggregate[group]={'count':n,**{k:v/n if n else None for k,v in fields.items() if k!='count'}}
    dm=dx[:,2]>0
    result={'arm':arm,'seed':seed,'parity':parity,'native_python_comparisons':sum(v['count'] for v in parity),
        'saved_likelihood_rows':128*1024,'max_log_probability_error':maximum,'training':aggregate,
        'final_overspeed_corrective_acceleration':int((final['acceleration'][over]<0).sum()),'overspeed_grid_rows':int(over.sum()),
        'final_underspeed_corrective_acceleration':int((final['acceleration'][under]>0).sum()),'underspeed_grid_rows':int(under.sum()),
        'velocity_derivatives':{k:v[dm].double().mean(0).tolist() for k,v in d.items()},
        'initial_final_mean_actions':[g['actions'][positive].double().mean(0).tolist() for g in [initial,final]],
        'residual_log_std':model.residual_log_std.tolist(),
        'weights_rng_unchanged':True,'training_ticks':0,'physics_ticks':0,'optimizer_steps':0,'seconds':time.monotonic()-start}
    assert result['seconds']<600;atomic_json(out/'results.json',result)


def verify(a):
    start=time.monotonic();records=[]
    for arm in ARMS:
        for seed in SEEDS:
            out=a/f'audit/{arm}/seed-{seed}';r=read(out/'results.json');data=load(out/'responses.pt.gz')
            assert len(data['grids'])==7 and len(data['history'])==129 and len(data['grid_inputs'])==3888
            assert r['weights_rng_unchanged'] and r['seconds']<600
            assert r['training']['movement']['count']==r['training']['stopping']['count']==65536
            stop=data['grid_metadata'][:,1]==0
            for g in data['grids']:
                # Match sigmoid memory layout: strided base views can differ by
                # one float32 ULP despite exactly identical stopping logits.
                assert torch.equal(g['logits'][stop],g['base_logits'][stop]) and not g['residual_logits'][stop].count_nonzero()
                assert torch.equal(g['actions'][stop],g['base_logits'].contiguous().sigmoid()[stop])
            records.append(r)
    result={'experiment_id':a.name,'records':records,'verification_passed':True,'source_policy_runs':6,
        'common_anchors':54,'grid_rows':163296,'history_rows':376164,'neutral_history_rows':23328,'derivative_rows':2592,
        'native_python_comparisons':sum(v['native_python_comparisons'] for v in records),
        'saved_likelihood_rows':sum(v['saved_likelihood_rows'] for v in records),'frozen_parent_checkpoints':774,
        'training_ticks':0,'physics_ticks':0,'optimizer_steps':0,'new_checkpoints':0,'new_policy_exports':0,
        'confirmation_used':False,'model_promoted':False,'navigation_qualified':False,
        'verification_revision':'consistent-sigmoid-layout-v1','seconds':time.monotonic()-start}
    assert result['native_python_comparisons']==46656 and result['saved_likelihood_rows']==786432 and result['seconds']<300
    atomic_json(a/'results.json',result)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--arm',choices=ARMS);p.add_argument('--seed',type=int,choices=SEEDS)
    args=p.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.arm:audit(args.archive,args.arm,args.seed)
    else:verify(args.archive)
