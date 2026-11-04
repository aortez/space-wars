"""LF-072: repeated frozen-policy rollouts and pre-optimizer gradient directions."""
import argparse
import copy
from pathlib import Path
import random
import shutil
import time

import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from tools.flight_motor import aligned, storage, sustained
from tools.flight_motor.sustained_run import CompressedCollector
from . import motor_error_gradient as audit
from . import motor_error_math as region
from . import motor_feedback as feedback
from . import motor_gradient_math as math

SEEDS = [17, 29, 43]
UPDATES = [1, 16, 43, 64, 86]
REPEATS = 16
WALL_SECONDS = 900
read, load = storage.read_json, storage.load_torch


def noise_seed(seed, update, repeat):
    if seed not in SEEDS or update not in UPDATES or not 0 <= repeat < REPEATS:
        raise ValueError('undeclared rollout noise seed')
    return 720000000 + seed*10000 + update*100 + repeat


def model_from(state):
    c = state['contract']['config']
    assert c['lambda'] == 1 and c['gamma'] == .99 and c['rollout_ticks'] == 256
    assert c['actuator_sampler'] == 'independent-gaussian' and 'motor_exploration' not in c
    with torch.random.fork_rng(devices=[]):
        model = sustained.ResidualModel(state['contract']['schema'])
    model.load_state_dict(state['model'])
    return model


class CountedBridge:
    def __init__(self, bridge):
        self.bridge = bridge
        self.counts = {'probe_ticks': 0, 'restore_ticks': 0, 'neutral_setups': 0}

    def call(self, op, **kw):
        result = self.bridge.call(op, **kw)
        if op == 'step': self.counts['probe_ticks'] += len(kw['actions'])
        if op == 'restore':
            self.counts['restore_ticks'] += sum(len(t['steps']) for t in kw['traces'])
            self.counts['neutral_setups'] += len(kw['traces'])
        if op == 'reset_at': self.counts['neutral_setups'] += 1
        return result


def collect(bridge, model, before, directory, seed, deadline, config=None):
    """Each trial owns collector state; fork_rng confines sampling to this trial."""
    c = before['contract']['config'] if config is None else config
    counted = CountedBridge(bridge)
    collector = CompressedCollector.restore(counted, copy.deepcopy(before['collector']), directory/'training-traces')
    assert equal_tree(collector.state(), before['collector'])
    with torch.random.fork_rng(devices=[]):
        if seed is None: torch.set_rng_state(before['torch_rng'])
        else: torch.manual_seed(seed)
        start_rng = torch.get_rng_state().clone()
        rollout = collector.collect(model, c, deadline)
        end_rng = torch.get_rng_state().clone()
    return rollout, {'collector': collector.state()}, start_rng, end_rng, counted.counts


def panel(model, original, a, seed, update):
    x, metadata = feedback.grid(read(a/'inputs/feedback-anchors.json'), [0,16], [-6,-1,0,1,6])
    saved = audit.probes(model, x, metadata, original)
    groups = [{k:v for k,v in g.items() if k != 'indices'} for g in saved['groups']]
    jacobians = [j for j in saved['jacobians']]
    inputs = [x]; _, params = math.parameters(model)
    anchors = [v for v in read(a/'inputs/return-anchors.json') if v['seed']==seed and v['update']==update]
    for anchor in anchors:
        trace = read(a/f"inputs/return-anchors/anchor-{anchor['index']:02}.json.gz")
        item = trace['steps'][-1]['item'] if trace['steps'] else trace['initial']
        row = torch.tensor([item['inputs']]); inputs.append(row)
        force = feedback.acceleration(row, model(row)[0][:,1:].sigmoid()).mean()
        jacobians.append(math.gradient(force, params))
        groups.append({'panel':'LF071_anchor','anchor':anchor['index'],'command':16,'region':anchor['region'],
                       'count':1,'force':float(force.detach())})
    return {'groups':groups, 'jacobians':torch.stack(jacobians), 'parity_inputs':torch.cat(inputs)}


def diagnostics(model, parent, rollout, evidence, fixed, c):
    _, params = math.parameters(model)
    x = rollout['observations'].flatten(0,1); actor, _ = model(x)
    f = {k:v.flatten(0,1) for k,v in rollout.items()}
    ratio = (model.log_prob(actor,f['latent'],f['actuator_latents'],x)-f['logp']).exp()
    assert ((ratio-1).abs()<c['clip']).all()
    adv = aligned.advantages(f['advantages'],x[:,2]>0)
    gradients = audit.full_gradients(model,rollout)
    combined = math.gradient(-.5*(ratio*adv)[x[:,2]>0].mean(),params)
    torch.testing.assert_close(gradients['pooled'].sum(0),combined,atol=1e-6,rtol=1e-4)
    stopping = math.gradient(-.5*c['stopping_actor_weight']*(ratio*adv)[x[:,2]==0].mean(),params)
    assert not stopping.count_nonzero()
    with torch.no_grad():
        stop = x[:,2]==0; old = parent(x)[0]
        assert torch.equal(actor[stop],old[stop])
        assert torch.equal(model.distribution(actor[stop],x[stop]).scale,parent.distribution(old[stop],x[stop]).scale)
    credit = region.credit(model,rollout,evidence)
    projections = {name:-fixed['jacobians'].double() @ g.double().T for name,g in gradients.items()}
    return {'gradients':gradients, 'credit':credit, 'projections':projections,
            'protected_stopping_rows':int(stop.sum())}


def verify_sampling(model, rollout, start_rng, end_rng):
    with torch.random.fork_rng(devices=[]), torch.no_grad():
        torch.set_rng_state(start_rng)
        for t,x in enumerate(rollout['observations']):
            latent,z,logp,_=model.sample(x)
            assert torch.equal(latent,rollout['latent'][t])
            assert torch.equal(z,rollout['actuator_latents'][t])
            assert torch.equal(logp,rollout['logp'][t])
        assert torch.equal(torch.get_rng_state(),end_rng)


def run(a, seed):
    started=time.monotonic();deadline=started+WALL_SECONDS
    out=a/f'probe/seed-{seed}';out.mkdir()
    source=a/f'inputs/speed16/seed-{seed}'
    parent_state=load(source/'checkpoint-0000.pt.gz');parent=model_from(parent_state)
    outer_rng=torch.get_rng_state().clone();python_rng=random.getstate()
    records=[];counts={k:0 for k in ['probe_ticks','restore_ticks','neutral_setups','protected_stopping_rows','native_python_comparisons']}
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        for update in UPDATES:
            before=load(source/f'checkpoint-{update-1:04}.pt.gz');original_after=load(source/f'checkpoint-{update:04}.pt.gz')
            original=load(source/f'rollout-{update:04}.pt.gz');model=model_from(before)
            sustained.assert_parent_actor(parent_state['model'],model.state_dict())
            fixed=panel(model,original,a,seed,update)
            bridge.call('load_policy',path=str((a/f'inputs/policies/seed-{seed}-before-{update:04}.json').resolve()))
            native=bridge.call('infer',inputs=fixed['parity_inputs'].tolist())['outputs']
            with torch.no_grad():
                raw,value=model(fixed['parity_inputs']);expected=torch.cat((raw,value[:,None]),1)
                actual=torch.tensor([[n[k] for k in ['steering_latent','thrust_logit','brake_logit','value']] for n in native])
                torch.testing.assert_close(actual,expected,atol=1e-5,rtol=1e-5)
            counts['native_python_comparisons']+=len(native)
            storage.dump_torch(out/f'fixed-{update:04}.pt.gz',fixed)
            for repeat in [-1,*range(REPEATS)]:
                if time.monotonic()>deadline:raise TimeoutError('rollout-gradient budget')
                if shutil.disk_usage(out).free<512*1024**2:raise RuntimeError('retention reserve')
                name=f'update-{update:04}-'+('reference' if repeat==-1 else f'repeat-{repeat:02}')
                trial=out/name;trial.mkdir();sampling_seed=None if repeat==-1 else noise_seed(seed,update,repeat)
                try:
                    rollout,after,start_rng,end_rng,work=collect(bridge,model,before,trial,sampling_seed,deadline)
                    storage.dump_torch(trial/'rollout.pt.gz',rollout)
                    storage.dump_torch(trial/'collector.pt.gz',after)
                    if repeat==-1:
                        assert equal_tree(rollout,original) and equal_tree(after['collector'],original_after['collector'])
                    evidence=region.reconstruct(trial,before,after,256)
                    target_error=audit.verify_targets(model,before,after,rollout,evidence)
                    verify_sampling(model,rollout,start_rng,end_rng)
                    info=diagnostics(model,parent,rollout,evidence,fixed,before['contract']['config'])
                    assert equal_tree(model.state_dict(),before['model'])
                    storage.dump_torch(trial/'diagnostic.pt.gz',{**info,'evidence':evidence,'start_rng':start_rng,'end_rng':end_rng})
                    completed=after['collector']['completed'][len(before['collector']['completed']):]
                    outcomes={v:sum(e['outcome']==v for e in completed) for v in sorted({e['outcome'] for e in completed})}
                    summary={'update':update,'repeat':repeat,'noise_seed':sampling_seed,'directory':name,
                        'reference_exact':repeat==-1,'counts':{**work,'protected_stopping_rows':info['protected_stopping_rows']},
                        'target_max_rounding_error':target_error,'completed_outcomes':outcomes,
                        'credit':info['credit']['stats'],'projections':{k:v.tolist() for k,v in info['projections'].items()}}
                    atomic_json(trial/'results.json',summary);records.append(summary)
                    for k,v in summary['counts'].items():counts[k]+=v
                except Exception as error:
                    storage.dump_json(trial/'interruption.json.gz',{'error':str(error),'partial_traces':bridge.call('state')['traces']})
                    raise
            assert equal_tree(before,load(source/f'checkpoint-{update-1:04}.pt.gz'))
            print({'seed':seed,'update':update,'rollouts':len(records),'ticks':counts['probe_ticks']},flush=True)
    assert torch.equal(outer_rng,torch.get_rng_state()) and python_rng==random.getstate()
    assert counts['probe_ticks']==5*17*1536 and counts['protected_stopping_rows']==5*17*768
    seconds=time.monotonic()-started;assert seconds<WALL_SECONDS
    atomic_json(out/'results.json',{'seed':seed,'records':records,'counts':counts,'weights_rng_unchanged':True,'seconds':seconds})


def verify(a):
    started=time.monotonic();runs=[];counts={};targets=0
    for seed in SEEDS:
        out=a/f'probe/seed-{seed}';source=a/f'inputs/speed16/seed-{seed}';run=read(out/'results.json')
        parent=model_from(load(source/'checkpoint-0000.pt.gz'))
        assert run['weights_rng_unchanged'] and run['seconds']<WALL_SECONDS and len(run['records'])==85
        for update in UPDATES:
            before=load(source/f'checkpoint-{update-1:04}.pt.gz');model=model_from(before)
            original=load(source/f'rollout-{update:04}.pt.gz')
            fixed=load(out/f'fixed-{update:04}.pt.gz')
            assert equal_tree(fixed,panel(model,original,a,seed,update))
            selected=[v for v in run['records'] if v['update']==update]
            assert [v['repeat'] for v in selected]==[-1,*range(REPEATS)]
            for record in selected:
                directory=out/record['directory'];rollout=load(directory/'rollout.pt.gz')
                after=load(directory/'collector.pt.gz');saved=load(directory/'diagnostic.pt.gz')
                assert record==read(directory/'results.json')
                if record['repeat']==-1:
                    assert equal_tree(rollout,original)
                    assert equal_tree(after['collector'],load(source/f'checkpoint-{update:04}.pt.gz')['collector'])
                    assert torch.equal(saved['start_rng'],before['torch_rng'])
                else:
                    assert record['noise_seed']==noise_seed(seed,update,record['repeat'])
                    with torch.random.fork_rng(devices=[]):
                        torch.manual_seed(record['noise_seed']);assert torch.equal(saved['start_rng'],torch.get_rng_state())
                evidence=region.reconstruct(directory,before,after,256)
                assert equal_tree(evidence,saved['evidence'])
                assert audit.verify_targets(model,before,after,rollout,evidence)==record['target_max_rounding_error']
                verify_sampling(model,rollout,saved['start_rng'],saved['end_rng'])
                info=diagnostics(model,parent,rollout,evidence,fixed,before['contract']['config'])
                assert all(equal_tree(v,saved[k]) for k,v in info.items())
                assert record['credit']==info['credit']['stats']
                assert record['projections']=={k:v.tolist() for k,v in info['projections'].items()}
                assert record['counts']['probe_ticks']==1536 and record['counts']['protected_stopping_rows']==768
                assert record['counts']['restore_ticks']==sum(len(t['steps']) for t in before['collector']['traces'])
                completed=after['collector']['completed'][len(before['collector']['completed']):]
                assert record['counts']['neutral_setups']==6+len(completed)
                assert record['completed_outcomes']=={v:sum(e['outcome']==v for e in completed) for v in sorted({e['outcome'] for e in completed})}
                targets+=1536
                if time.monotonic()-started>600:raise TimeoutError('rollout-gradient verification budget')
            assert equal_tree(model.state_dict(),before['model'])
        for k,v in run['counts'].items():counts[k]=counts.get(k,0)+v
        runs.append(run)
    assert counts['probe_ticks']==391680 and targets==391680
    assert counts['protected_stopping_rows']==195840 and counts['native_python_comparisons']==8140
    atomic_json(a/'results.json',{'experiment_id':a.name,'verification_passed':True,'records':runs,'counts':counts,
        'reference_rollouts':15,'independent_rollouts':240,'verified_sampling_target_rows':targets,
        'training_ticks':0,'optimizer_steps':0,'new_checkpoints':0,'new_policy_exports':0,
        'confirmation_used':False,'model_promoted':False,'navigation_qualified':False,'seconds':time.monotonic()-started})


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True)
    parser.add_argument('--seed',type=int,choices=SEEDS);parser.add_argument('--verify',action='store_true');args=parser.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.verify:verify(args.archive)
    else:run(args.archive,args.seed)
