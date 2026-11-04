"""Atomic update-boundary checkpoints; exact same-host replay continuation."""
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import tempfile

import torch
from .collector import Collector
from .model import FlightModel, ACTION_CONTRACT, input_scaling, architecture, export_version
from .execution import evaluation_spec, execution_identity


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def identity(config, bridge):
    sources = {p.name: sha(p) for p in sorted(Path(__file__).parent.glob("*.py"))}
    if config.get("action_contract") != ACTION_CONTRACT or bridge.contract["schema"].get("action_contract") != ACTION_CONTRACT:
        raise ValueError("continuous-flight-v2 action contract required; old runs cannot be reinterpreted")
    evaluation = evaluation_spec(config.get("evaluation"))
    spec = architecture(config.get("architecture"))
    return {"format_version": export_version(spec), "action_contract": ACTION_CONTRACT, "architecture": spec, "schema": bridge.contract["schema"],
            "config": config, "evaluation": evaluation, "evaluation_execution": execution_identity(evaluation),
            "bridge_sha256": sha(bridge.executable), "python_sources": sources,
            "python": platform.python_version(), "torch": str(torch.__version__),
            "threads": torch.get_num_threads(), "device": "cpu", "normalization": {"schema": "fixed native scales", "training_input_scaling": input_scaling(config.get("input_scaling")), "export": "factors folded into query/fusion weights", "running_statistics": False},
            "scheduler": None, "restart": "completed PPO update; replay active traces; restore sampler and CPU/Python RNG"}


def atomic_json(path, data):
    path = Path(path)
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as file:
        json.dump(data, file, allow_nan=False, indent=2)
        file.write("\n")
        file.flush()
        os.fsync(file.fileno())
        temporary = file.name
    os.replace(temporary, path)


def save(path, contract, model, optimizer, collector, update, history):
    path = Path(path)
    if path.exists():
        raise FileExistsError(path)
    state = {"contract": contract, "model": model.state_dict(), "optimizer": optimizer.state_dict(),
             "collector": collector.state(), "update": update, "history": history,
             "torch_rng": torch.get_rng_state(), "python_rng": random.getstate()}
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as file:
        torch.save(state, file)
        file.flush()
        os.fsync(file.fileno())
        temporary = file.name
    os.replace(temporary, path)
    atomic_json(path.parent / "latest.json", {"checkpoint": path.name, "sha256": sha(path), "update": update})


def load(path, expected, bridge, trace_dir=None):
    state = torch.load(path, map_location="cpu", weights_only=True)
    if state["contract"] != expected:
        raise ValueError("checkpoint contract/runtime/source mismatch; exact resume refused")
    model = FlightModel(bridge.contract["schema"], expected["config"].get("input_scaling"), expected["config"].get("architecture"))
    model.load_state_dict(state["model"], strict=True)
    optimizer = torch.optim.Adam(model.parameters(), lr=expected["config"]["learning_rate"], eps=1e-5)
    optimizer.load_state_dict(state["optimizer"])
    collector = Collector.restore(bridge, state["collector"], trace_dir)
    torch.set_rng_state(state["torch_rng"])
    random.setstate(state["python_rng"])
    return model, optimizer, collector, state["update"], state["history"]
