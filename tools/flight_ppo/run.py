"""CLI for one declared CPU PPO experiment. No automatic budget extension."""
import argparse
import json
from pathlib import Path
import random
import time

import torch
from .bridge import Bridge
from .curriculum import sample, validate_position_curriculum
from .checkpoint import atomic_json, identity, load, save, sha
from .collector import Collector
from .evaluate import evaluate, selection_score, summarize
from .execution import evaluation_spec
from .model import FlightModel, ACTION_CONTRACT, value_gradient_mode, input_scaling, architecture
from .exploration import SCALED_KIND, validate as validate_exploration
from .ppo import update, advantage_centering, advantage_diagnostics, FAMILY_CENTERING
from .initialization import initialize_policy


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bridge", required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--resume", type=Path)
    parser.add_argument("--warm-start", type=Path)
    parser.add_argument("--stop-after", type=int)
    parser.add_argument("--skip-evaluation", action="store_true", help="Preflight restart test only")
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    if args.resume and args.warm_start:
        raise ValueError("warm start and exact resume are distinct operations")
    if config.get("action_contract") != ACTION_CONTRACT:
        raise ValueError("explicit continuous-flight-v2 action contract required")
    if "exploration" in config:
        validate_exploration(config["exploration"])
    value_gradient_mode(config.get("value_gradient"))
    input_scaling(config.get("input_scaling"))
    architecture(config.get("architecture"))
    validate_position_curriculum(config)
    if (advantage_centering(config.get("advantage_centering")) == FAMILY_CENTERING
            and config.get("task_recipe") != "settle-recover-v1"):
        raise ValueError("family centering requires settle-recover-v1")
    evaluation = evaluation_spec(config.get("evaluation"))
    if config.get("exploration", {}).get("kind") == SCALED_KIND and evaluation["mode"] != "continuous-mean-v2":
        raise ValueError("scaled training requires deterministic evaluation; export log_std does not encode its multiplier")
    args.out.mkdir()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    torch.manual_seed(config["learner_seed"])
    random.seed(config["learner_seed"])
    start = time.monotonic()
    deadline = start + config["wall_seconds"]
    atomic_json(args.out / "run-plan.json", {"config": config, "resume": str(args.resume) if args.resume else None,
                "warm_start": str(args.warm_start) if args.warm_start else None,
                "stop_after": args.stop_after, "evaluation_enabled": not args.skip_evaluation,
                "evaluation": evaluation})
    with Bridge(args.bridge) as bridge:
        contract = identity(config, bridge)
        atomic_json(args.out / "contract.json", contract)
        if args.resume:
            model, optimizer, collector, completed, history = load(args.resume, contract, bridge, args.out / "training-traces")
        else:
            batch, pools = sample(bridge, config, "train")
            atomic_json(args.out / "training-admission.json", batch)
            if not batch["complete"]:
                raise RuntimeError("training task quota incomplete; audit retained")
            collector = Collector(bridge, pools, args.out / "training-traces")
            model = FlightModel(bridge.contract["schema"], config.get("input_scaling"), config.get("architecture"))
            initialize_policy(model, config, args.warm_start)
            optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"], eps=1e-5)
            completed, history = 0, []
            save(args.out / "checkpoint-0000.pt", contract, model, optimizer, collector, 0, history)
        initial_policy = args.out / f"policy-{completed:04}.json"
        model.export(initial_policy)
        baseline = []
        candidates = []
        if not args.skip_evaluation:
            batch, pools = sample(bridge, config, "development")
            atomic_json(args.out / "development-admission.json", batch)
            if not batch["complete"]:
                raise RuntimeError("development task quota incomplete; audit retained")
            candidates = [c for pool in pools for c in pool]
            baseline = evaluate(args.bridge, initial_policy, candidates, args.out / "evaluation-initial", deadline,
                                execution=evaluation, learner_seed=config["learner_seed"])
        end = min(config["updates"], args.stop_after or config["updates"])
        train_start = time.monotonic()
        start_steps = collector.steps
        try:
            while completed < end:
                rollout = collector.collect(model, config, deadline)
                if "family_ids" in rollout:
                    atomic_json(args.out / f"advantages-{completed+1:04}.json", advantage_diagnostics(rollout, config))
                metrics = update(model, optimizer, rollout, config)
                completed += 1
                metrics.update(update=completed, environment_steps=collector.steps, completed_episodes=len(collector.completed))
                history.append(metrics)
                atomic_json(args.out / "progress.json", {"updates": history, "started": collector.started, "completed": collector.completed})
                save(args.out / f"checkpoint-{completed:04}.pt", contract, model, optimizer, collector, completed, history)
                print(json.dumps(metrics), flush=True)
            elapsed = time.monotonic() - train_start
            final_policy = args.out / f"policy-{completed:04}.json"
            model.export(final_policy)
            final = evaluate(args.bridge, final_policy, candidates, args.out / "evaluation-final", deadline,
                             execution=evaluation, learner_seed=config["learner_seed"]) if candidates else []
            best = final_policy if final and selection_score(final) > selection_score(baseline) else initial_policy
            if candidates:
                atomic_json(args.out / "best.json", {"policy": best.name, "sha256": sha(best),
                            "rule": "success count, then fewer physical failures, then lower mean final position error; ties keep initial",
                            "initial_score": selection_score(baseline), "final_score": selection_score(final), "scope": "development comparison; qualification requires experiment gates"})
            atomic_json(args.out / "summary.json", {"completed_updates": completed, "environment_steps": collector.steps,
                        "started_episodes": collector.started, "completed_episodes": collector.completed,
                        "unfinished_episodes": [{**episode, "outcome": "budget_censored", "final_errors": item["frame"]["errors"]}
                                                for episode, item in zip(collector.active, collector.items)],
                        "training_seconds_including_checkpoints": elapsed,
                        "training_steps_per_second": (collector.steps - start_steps) / max(elapsed, 1e-9),
                        "total_seconds": time.monotonic() - start, "initial_evaluation": baseline, "final_evaluation": final,
                        "parameter_count": sum(p.numel() for p in model.parameters()), "history": history,
                        "evaluation": evaluation,
                        "evaluation_summary": {"initial": summarize(baseline), "final": summarize(final)},
                        "capability_claim": False})
        except Exception as error:
            atomic_json(args.out / "interruption.json", {"error": str(error), "last_completed_update": completed,
                        "started": collector.started, "completed": collector.completed, "active": collector.active,
                        "partial_traces": bridge.call("state")["traces"]})
            raise


if __name__ == "__main__":
    main()
