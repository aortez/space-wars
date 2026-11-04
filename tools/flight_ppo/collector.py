"""Four persistent sampling slots; rollout boundaries never terminate episodes."""
import json
from pathlib import Path
import time

import torch
from .model import actions, tensor
from .reward import deadline_reward, reward
from .ppo import gae, advantage_centering, FAMILY_CENTERING
from . import exploration


class Collector:
    def __init__(self, bridge, pools, trace_dir=None):
        if len(pools) != 4 or any(not pool for pool in pools):
            raise ValueError("four nonempty task pools required")
        self.bridge, self.pools = bridge, pools
        self.trace_dir = Path(trace_dir) if trace_dir else None
        if self.trace_dir:
            self.trace_dir.mkdir(exist_ok=True)
        self.exploration_state = None
        self.cursors = [0] * 4
        self.started = 0
        self.steps = 0
        self.completed = []
        self.active = [self.new_episode(i) for i in range(4)]
        self.items = bridge.call("reset", tasks=[e["candidate"]["task"] for e in self.active])["items"]

    def new_episode(self, slot):
        candidate = self.pools[slot][self.cursors[slot] % len(self.pools[slot])]
        self.cursors[slot] += 1
        episode = {"id": self.started, "candidate": candidate, "ticks": 0,
                   "reward": {"terminal": 0.0, "time": 0.0, "potential": 0.0, "total": 0.0},
                   "thrust_amount_sum": 0, "brake_amount_sum": 0, "steering_abs_sum": 0.0,
                   "collision": False, "damage": False}
        self.started += 1
        return episode

    def collect(self, model, config, deadline=None):
        deadline_weight = config.get("deadline_distance_weight", 0.0)
        deadline_reward(0.0, deadline_weight)  # Validate before collecting any physics.
        centering = advantage_centering(config.get("advantage_centering"))
        record_families = config.get("task_recipe") == "settle-recover-v1"
        if centering == FAMILY_CENTERING and not record_families:
            raise ValueError("family centering requires settle-recover-v1")
        if record_families:
            for slot, pool in enumerate(self.pools):
                for candidate in pool:
                    if (candidate.get("recipe") != f"settle-recover-v1/stratum-{slot}"
                            or candidate.get("index", -1) % 4 != slot):
                        raise ValueError("recovery pool/family mapping mismatch")
        families = []
        observations, latent_values, actuator_latents_values, logps, values, rewards, dones = ([] for _ in range(7))
        spec = config.get("exploration")
        std_multiplier = exploration.amplitude(spec) if spec else 1.
        ar_records = {key: [] for key in ["ar_latents", "ar_previous_observations", "ar_previous_latents", "ar_previous_valid"]}
        if spec:
            if self.exploration_state is None:
                self.exploration_state = exploration.initial_state(len(self.items), spec)
            if self.exploration_state["spec"] != spec:
                raise ValueError("exploration state/config mismatch")
        elif self.exploration_state is not None:
            raise ValueError("cannot discard persistent exploration state")
        for _ in range(config["rollout_ticks"]):
            if deadline is not None and time.monotonic() > deadline:
                raise TimeoutError("declared smoke wall-time budget exhausted")
            if record_families:
                labels = []
                for slot, episode in enumerate(self.active):
                    candidate = episode["candidate"]
                    if (candidate.get("recipe") != f"settle-recover-v1/stratum-{slot}"
                            or candidate.get("index", -1) % 4 != slot):
                        raise ValueError("active recovery family mismatch")
                    labels.append(candidate["index"] % 4)
                families.append(torch.tensor(labels, dtype=torch.int64))
            x = tensor(self.items)
            with torch.no_grad():
                if spec:
                    state = self.exploration_state
                    actor, value = model(x)
                    previous_actor = model(state["previous_observations"])[0]
                    dist = exploration.distribution(model, actor, previous_actor,
                        state["previous_latents"], state["previous_valid"], spec["rho"], std_multiplier)
                    full_latent = dist.sample()
                    latent, actuator_latents = full_latent[:, 0], full_latent[:, 1:]
                    logp = exploration.log_prob(model, dist, full_latent)
                    ar_records["ar_latents"].append(full_latent)
                    for key in ["previous_observations", "previous_latents", "previous_valid"]:
                        ar_records["ar_"+key].append(state[key].clone())
                else:
                    latent, actuator_latents, logp, value = model.sample(x)
            command = actions(latent, actuator_latents)
            after = self.bridge.call("step", actions=command)["items"]
            terms = [reward(before, new, config["gamma"], config.get("velocity_shaping_weight", 0.25), deadline_weight, config.get("regulation_weight", 0.0))
                     for before, new in zip(self.items, after)]
            ended = [new["progress"]["outcome"] is not None for new in after]
            if spec:
                exploration.advance(self.exploration_state, x, full_latent, ended)
            observations.append(x)
            latent_values.append(latent)
            actuator_latents_values.append(actuator_latents)
            logps.append(logp)
            values.append(value)
            rewards.append(torch.tensor([r["total"] for r in terms]))
            dones.append(torch.tensor(ended, dtype=torch.float32))
            terminal_traces = self.bridge.call("state")["traces"] if any(ended) and self.trace_dir else None
            for i, (term, action, new) in enumerate(zip(terms, command, after)):
                episode = self.active[i]
                episode["ticks"] += 1
                for key, v in term.items():
                    episode["reward"][key] = episode["reward"].get(key, 0.0) + v
                episode["thrust_amount_sum"] += action["thrust"]
                episode["brake_amount_sum"] += action["brake"]
                episode["steering_abs_sum"] += abs(action["steering"])
                evidence = new["evidence"] or {}
                for key in ["collision", "damage"]:
                    episode[key] |= evidence.get(key, False)
                if ended[i]:
                    episode["outcome"] = new["progress"]["outcome"]
                    episode["final_errors"] = new["frame"]["errors"]
                    if terminal_traces:
                        (self.trace_dir / f"episode-{episode['id']:06}.json").write_text(json.dumps(terminal_traces[i], allow_nan=False))
                    self.completed.append(episode)
                    self.active[i] = self.new_episode(i)
            # Reset only after terminal reward/outcome/trace have been captured.
            self.items = after
            for i, done in enumerate(ended):
                if done:
                    self.items = self.bridge.call("reset_at", index=i, task=self.active[i]["candidate"]["task"])["items"]
            self.steps += len(after)
        with torch.no_grad():
            final_value = model(tensor(self.items))[1]
        rewards, dones, values = map(torch.stack, (rewards, dones, values))
        advantages, returns = gae(rewards, dones, values, final_value, config["gamma"], config["lambda"])
        result = {"observations": torch.stack(observations), "latent": torch.stack(latent_values),
                "actuator_latents": torch.stack(actuator_latents_values), "logp": torch.stack(logps),
                "advantages": advantages, "returns": returns}
        if spec:
            result.update({k: torch.stack(v) for k,v in ar_records.items()})
        if record_families:
            result["family_ids"] = torch.stack(families)
        return result

    def state(self):
        return {"pools": self.pools, "cursors": self.cursors, "started": self.started,
                "steps": self.steps, "completed": self.completed, "active": self.active,
                "items": self.items, "traces": self.bridge.call("state")["traces"],
                "exploration_state": self.exploration_state}

    @classmethod
    def restore(cls, bridge, state, trace_dir=None):
        obj = cls.__new__(cls)
        obj.bridge, obj.trace_dir = bridge, Path(trace_dir) if trace_dir else None
        if obj.trace_dir:
            obj.trace_dir.mkdir(exist_ok=True)
        obj.exploration_state = state.get("exploration_state")
        for key in ["pools", "cursors", "started", "steps", "completed", "active"]:
            setattr(obj, key, state[key])
        obj.items = bridge.call("restore", traces=state["traces"])["items"]
        if obj.items != state["items"]:
            raise RuntimeError("restored observations differ from checkpoint")
        return obj
