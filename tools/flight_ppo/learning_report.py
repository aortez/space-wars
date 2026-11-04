"""Report all declared seeds, physical gates, traces and trained-export parity."""
import argparse
from collections import Counter
import json
from pathlib import Path
import torch
from .bridge import Bridge
from .checkpoint import atomic_json
from .learning_metrics import aggregate, seed_gates
from .model import FlightModel, flatten


def inspect_trace(path):
    trace = json.loads(path.read_text())
    steps = trace["steps"]
    frames = [trace["initial"]] + [s["frame"] for s in steps]
    return {"task": path.stem, "outcome": steps[-1]["progress"]["outcome"], "ticks": len(steps),
            "minimum_position_error": min(f["errors"]["position"] for f in frames),
            "final_errors": frames[-1]["errors"],
            "maximum_dwell_ticks": max(s["progress"]["dwell_ticks"] for s in steps),
            "thrust_fraction": sum(s["action"]["thrust"] for s in steps)/len(steps),
            "brake_fraction": sum(s["action"]["brake"] for s in steps)/len(steps),
            "mean_abs_steering": sum(abs(s["action"]["steering"]) for s in steps)/len(steps),
            "collision": any(s["evidence"]["collision"] for s in steps),
            "damage": any(s["evidence"]["damage"] for s in steps)}


def parity(bridge, run, state):
    config = state["contract"]["config"]
    model = FlightModel(state["contract"]["schema"])
    model.load_state_dict(state["model"])
    bridge.call("load_policy", path=str((run/f"policy-{config['updates']:04}.json").resolve()))
    admission = json.loads((run/"development-admission.json").read_text())
    fixtures = [bridge.call("reset", tasks=[r["candidate"]["task"]])["items"][0]["inputs"]
                for r in admission["attempts"] if r["admitted"]]
    actual = bridge.call("infer", inputs=fixtures)["outputs"]
    with torch.no_grad():
        actor, value = model(torch.tensor([flatten(x) for x in fixtures]))
    native = torch.tensor([[r["steering_latent"], r["thrust_logit"], r["brake_logit"], r["value"]] for r in actual])
    expected = torch.cat((actor, value[:, None]), dim=1)
    torch.testing.assert_close(native, expected, atol=1e-5, rtol=1e-5)
    for r, row in zip(actual, actor):
        assert r["action"]["thrust"] == bool(row[1]>=0)
        assert r["action"]["brake"] == bool(row[2]>=0)
        assert abs(r["action"]["steering"]-float(row[0].tanh())) < 1e-5
    return {"fixtures": len(fixtures), "maximum_abs_output_error": float((native-expected).abs().max()), "actions_match": True}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    results = []
    with Bridge(args.archive/"runtime/flight_ppo_bridge") as bridge:
        for seed in [17, 29, 43]:
            run = args.archive/f"seed-{seed}"
            if not (run/"summary.json").exists():
                results.append({"seed": seed, "status": "interrupted_or_failed", "learning_evidence": False, "basic_flight_gate": False})
                continue
            summary = json.loads((run/"summary.json").read_text())
            all_episodes = summary["completed_episodes"] + summary["unfinished_episodes"]
            assert len(all_episodes) == summary["started_episodes"]
            assert sum(e["ticks"] for e in all_episodes) == summary["environment_steps"] == 131072
            assert summary["completed_updates"] == 128
            state = torch.load(run/"checkpoint-0128.pt", weights_only=True)
            initial = torch.load(run/"checkpoint-0000.pt", weights_only=True)
            gates = seed_gates(summary["initial_evaluation"], summary["final_evaluation"])
            record = {"seed": seed, "status": "completed", **gates,
                      "training_outcomes": dict(Counter(e["outcome"] for e in summary["completed_episodes"])),
                      "budget_censored": len(summary["unfinished_episodes"]),
                      "training_ticks": summary["environment_steps"],
                      "training_seconds": summary["training_seconds_including_checkpoints"],
                      "training_ticks_per_second": summary["training_steps_per_second"], "total_seconds": summary["total_seconds"],
                      "initial_evaluation": summary["initial_evaluation"], "final_evaluation": summary["final_evaluation"],
                      "best": json.loads((run/"best.json").read_text()), "parity": parity(bridge, run, state),
                      "parameter_tensor_max_changes": {k:float((v-initial["model"][k]).abs().max()) for k,v in state["model"].items()},
                      "evaluation_trace_inspection": {phase:[inspect_trace(p) for p in sorted((run/f"evaluation-{phase}").glob('*.json')) if p.name != 'results.json'] for phase in ['initial', 'final']}}
            results.append(record)
    atomic_json(args.out, {"experiment_id": args.archive.name, "seeds": results,
                          "gates": aggregate(results), "confirmation_status": "pending_if_gate_passes_else_unused",
                          "obstacle_navigation_claim": False})


if __name__ == "__main__":
    main()
