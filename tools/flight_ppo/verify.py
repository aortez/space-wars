"""Verify a retained trained export and independently resumed checkpoint."""
import argparse
import copy
import json
from pathlib import Path
import time

import torch
from .bridge import Bridge, task_pools
from .checkpoint import atomic_json
from .model import FlightModel, flatten
from .tests import equal_tree


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bridge", required=True)
    parser.add_argument("--run", type=Path, required=True)
    parser.add_argument("--resumed", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    contract = json.loads((args.run / "contract.json").read_text())
    n = contract["config"]["updates"]
    current = torch.load(args.run / f"checkpoint-{n:04}.pt", weights_only=True)
    resumed = torch.load(args.resumed / f"checkpoint-{n:04}.pt", weights_only=True)
    assert equal_tree(current, resumed), "fresh-process continuation differed"
    initial = torch.load(args.run / "checkpoint-0000.pt", weights_only=True)
    deltas = {name: float((weight-initial["model"][name]).abs().max()) for name, weight in current["model"].items()}
    assert all(v > 0 for v in deltas.values()), "an expected parameter received no update"
    model = FlightModel(contract["schema"])
    model.load_state_dict(current["model"])
    fixtures = []
    with Bridge(args.bridge) as bridge:
        bridge.call("load_policy", path=str((args.run / f"policy-{n:04}.json").resolve()))
        for partition in ["training", "development"]:
            pools = task_pools(json.loads((args.run / f"{partition}-admission.json").read_text()))
            for pool in pools:
                for candidate in pool:
                    fixtures.append(bridge.call("reset", tasks=[candidate["task"]])["items"][0]["inputs"])
        for original in list(fixtures):
            changed = copy.deepcopy(original)
            changed["objects"].reverse()
            fixtures.append(changed)
        actual = bridge.call("infer", inputs=fixtures)["outputs"]
        x = torch.tensor([flatten(f) for f in fixtures])
        with torch.no_grad():
            actor, value = model(x)
        expected = torch.cat((actor, value[:, None]), dim=1)
        native = torch.tensor([[r["steering_latent"], r["thrust_logit"], r["brake_logit"], r["value"]] for r in actual])
        torch.testing.assert_close(native, expected, atol=1e-5, rtol=1e-5)
        for row, result in zip(actor, actual):
            assert abs(float(row[1].sigmoid())-result["action"]["thrust"]) < 1e-5
            assert abs(float(row[2].sigmoid())-result["action"]["brake"]) < 1e-5
            assert abs(float(row[0].tanh())-result["action"]["steering"]) < 1e-5
        timings = {}
        # Warm repeated single-observation calls; Rust includes JSON/IPC overhead.
        with torch.no_grad():
            for _ in range(10):
                model(x[:1])
                bridge.call("infer", inputs=fixtures[:1])
            for label in ["pytorch_cpu", "rust_with_json_ipc"]:
                start = time.perf_counter()
                for _ in range(100):
                    if label == "pytorch_cpu":
                        model(x[:1])
                    else:
                        bridge.call("infer", inputs=fixtures[:1])
                timings[label] = (time.perf_counter()-start)*1000/100
    atomic_json(args.out, {"checkpoint_full_state_bitwise_equal": True,
                          "parity_fixture_count": len(fixtures), "max_abs_output_error": float((native-expected).abs().max()),
                          "continuous_actions_match": True, "parameter_max_abs_changes": deltas,
                          "single_observation_mean_ms": timings,
                          "timing_scope": "100 warm calls, CPU one thread; Rust includes Python JSON/IPC; not gameplay profiling"})


if __name__ == "__main__":
    main()
