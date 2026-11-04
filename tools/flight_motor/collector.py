"""Persistent motor slots with full replayable evidence; four slots by default."""
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.model import actions
from tools.flight_ppo.ppo import gae
from .model import tensor
from .task import reward
from .velocity import reward as velocity_reward
from . import persistent


class Collector:
    def __init__(self, bridge, candidates, directory, pools=None):
        if pools is not None:
            if not 1 <= len(pools) <= 16 or any(not pool for pool in pools):
                raise ValueError('invalid motor collector pools')
            flattened = [row for pool in pools for row in pool]
            if sorted(flattened, key=lambda r:r['index']) != sorted(candidates, key=lambda r:r['index']):
                raise ValueError('motor pools must partition candidates exactly')
        self.bridge = bridge; self.directory = Path(directory); self.directory.mkdir()
        self.pools = [candidates[i::4] for i in range(4)] if pools is None else pools
        self.cursors = [0]*len(self.pools)
        self.started = 0; self.steps = 0; self.completed = []
        self.exploration = None
        self.active = [self.new_episode(i) for i in range(len(self.pools))]
        self.items = bridge.call('reset', tasks=[x['candidate']['task'] for x in self.active])['items']

    def new_episode(self, slot):
        pool = self.pools[slot]; candidate = pool[self.cursors[slot] % len(pool)]; self.cursors[slot] += 1
        self.started += 1
        return {'id': self.started, 'candidate': candidate, 'ticks': 0, 'reward': 0.}

    def save_trace(self, episode_id, trace):
        atomic_json(self.directory/f"episode-{episode_id:06}.json",trace)

    def collect(self, model, config, deadline):
        records = {k: [] for k in ['observations','latent','actuator_latents','logp','values','rewards','dones']}
        use_ar = 'motor_exploration' in config
        if use_ar:
            if self.exploration is None:self.exploration=persistent.initial_state(len(self.pools))
            for k in ['ar_previous_observations','ar_previous_latents','ar_previous_valid','ar_latents']:records[k]=[]
        for _ in range(config['rollout_ticks']):
            if time.monotonic() > deadline: raise TimeoutError('motor training wall budget')
            x = tensor(self.items)
            with torch.no_grad():
                if use_ar:
                    state=self.exploration;actor,values=model(x);previous=model(state['previous_observations'])[0]
                    dist=persistent.distribution(model,actor,previous,state['previous_latents'],state['previous_valid'],x,config['motor_exploration'])
                    sampled=dist.sample();latent=sampled[:,0];actuators=sampled[:,1:];logp=persistent.log_prob(model,dist,sampled)
                    for key in ['previous_observations','previous_latents','previous_valid']:records['ar_'+key].append(state[key].clone())
                    records['ar_latents'].append(sampled)
                else:latent,actuators,logp,values=model.sample(x)
            after = self.bridge.call('step', actions=actions(latent, actuators))['items']
            cost = velocity_reward if config['kind'] in ['motor-velocity-v1','motor-persistent-comparison-v1','motor-aligned-readout-v1','motor-protected-sustain-v1','motor-movement-comparison-v1','motor-gae-comparison-v1','motor-command-balance-v1','motor-single-speed-v1'] else reward
            terms = [cost(i) for i in after]; done = [i['progress']['outcome'] is not None for i in after]
            for k, v in [('observations',x),('latent',latent),('actuator_latents',actuators),('logp',logp),('values',values),
                         ('rewards',torch.tensor([t['total'] for t in terms])),('dones',torch.tensor(done,dtype=torch.float32))]: records[k].append(v)
            traces = self.bridge.call('state')['traces'] if any(done) else None
            for slot, terminal in enumerate(done):
                e = self.active[slot]; e['ticks'] += 1; e['reward'] += terms[slot]['total']
                if terminal:
                    e.update(outcome=after[slot]['progress']['outcome'], final_errors=after[slot]['errors'])
                    self.save_trace(e['id'],traces[slot])
                    self.completed.append(e); self.active[slot] = self.new_episode(slot)
            self.items = after
            for slot, terminal in enumerate(done):
                if terminal: self.items = self.bridge.call('reset_at',index=slot,task=self.active[slot]['candidate']['task'])['items']
            if use_ar:persistent.advance(self.exploration,x,sampled,done)
            self.steps += len(self.pools)
        records = {k: torch.stack(v) for k,v in records.items()}
        with torch.no_grad(): final = model(tensor(self.items))[1]
        adv, returns = gae(records.pop('rewards'), records.pop('dones'), records.pop('values'), final, config['gamma'], config['lambda'])
        return {**records, 'advantages': adv, 'returns': returns}

    def state(self):
        state={**{k:getattr(self,k) for k in ['pools','cursors','started','steps','completed','active','items']}, 'traces':self.bridge.call('state')['traces']}
        if self.exploration is not None:state['exploration']=self.exploration
        return state

    @classmethod
    def restore(cls, bridge, state, directory):
        obj=cls.__new__(cls);obj.bridge=bridge;obj.directory=Path(directory);obj.directory.mkdir()
        for k in ['pools','cursors','started','steps','completed','active']:setattr(obj,k,state[k])
        obj.exploration=state.get('exploration')
        if obj.exploration is not None:persistent.validate_state(obj.exploration,obj.active)
        obj.items=bridge.call('restore',traces=state['traces'])['items']
        if obj.items != state['items']:raise ValueError('motor collector restore mismatch')
        return obj
