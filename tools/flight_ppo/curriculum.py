"""Declared clear-space sampling strata; no policy-dependent task additions."""
import math
import random

RECIPE = "clear-control-v1"
STRATA = [(4., 12., 1., .2), (12., 24., 2., .4),
          (24., 48., 4., .6), (48., 80., 6., .8)]


def candidate(seed, index):
    # Per-index RNG isolates samples from admission and other strata filling.
    rng = random.Random(f"{RECIPE}:{seed}:{index}")
    low, high, speed_max, spin_max = STRATA[index % 4]
    direction = rng.uniform(-math.pi, math.pi)
    distance = rng.uniform(low, high)
    speed = rng.uniform(0, speed_max)
    velocity_angle = rng.uniform(-math.pi, math.pi)
    task = {"start": {"seed": rng.randrange(1, 2**31),
                      "bearing": rng.uniform(-math.pi, math.pi),
                      "altitude": rng.uniform(140, 150),
                      "heading_offset": rng.uniform(-math.pi, math.pi),
                      "relative_velocity": {"x": speed*math.cos(velocity_angle), "y": speed*math.sin(velocity_angle)},
                      "spin": rng.uniform(-spin_max, spin_max), "gravity": False, "planet_spin": 0.},
            "goal_offset": {"x": distance*math.cos(direction), "y": distance*math.sin(direction)},
            "goal_velocity": {"x": 0., "y": 0.},
            "arrival": {"position": 3., "velocity": 1., "spin": .2, "dwell_ticks": 30},
            "deadline_ticks": 1800, "obstacles": [], "terrain_edits": []}
    # Version 1 is the existing candidate/admission schema, not the old sampler.
    return {"generator_version": 1, "seed": seed, "index": index,
            "family": "clear", "recipe": f"{RECIPE}/stratum-{index % 4}", "task": task}


ALIGNED_RECIPE = "aligned-approach-v1"
ALIGNED_STRATA = [(4., 6.), (6., 8.), (8., 10.), (10., 12.)]


def aligned_candidate(seed, index):
    rng = random.Random(f"{ALIGNED_RECIPE}:{seed}:{index}")
    distance = rng.uniform(*ALIGNED_STRATA[index % 4])
    bearing = rng.uniform(-math.pi, math.pi)
    heading = rng.uniform(-math.pi, math.pi)
    # Ship forward is local +Y, rotated by bearing + heading_offset.
    angle = bearing + heading
    task = {"start": {"seed": rng.randrange(1, 2**31), "bearing": bearing,
                      "altitude": rng.uniform(140, 150), "heading_offset": heading,
                      "relative_velocity": {"x": 0., "y": 0.}, "spin": 0.,
                      "gravity": False, "planet_spin": 0.},
            "goal_offset": {"x": -distance*math.sin(angle), "y": distance*math.cos(angle)},
            "goal_velocity": {"x": 0., "y": 0.},
            "arrival": {"position": 3., "velocity": 1., "spin": .2, "dwell_ticks": 30},
            "deadline_ticks": 1800, "obstacles": [], "terrain_edits": []}
    return {"generator_version": 1, "seed": seed, "index": index, "family": "clear",
            "recipe": f"{ALIGNED_RECIPE}/stratum-{index % 4}", "task": task}


HOLD_RECIPE = "hold-position-v1"
HOLD_EXIT_RECIPE = "hold-first-exit-v1"


def hold_candidate(seed, index):
    rng = random.Random(f"{HOLD_RECIPE}:{seed}:{index}")
    slot = index % 4
    low = -math.pi + slot * math.pi / 2
    task = {"start": {"seed": rng.randrange(1, 2**31),
                      "bearing": rng.uniform(-math.pi, math.pi),
                      "altitude": rng.uniform(140, 150),
                      "heading_offset": rng.uniform(low, low + math.pi / 2),
                      "relative_velocity": {"x": 0., "y": 0.}, "spin": 0.,
                      "gravity": False, "planet_spin": 0.},
            "goal_offset": {"x": 0., "y": 0.},
            "goal_velocity": {"x": 0., "y": 0.},
            "arrival": {"position": 3., "velocity": 1., "spin": .2, "dwell_ticks": 30},
            "deadline_ticks": 1800, "obstacles": [], "terrain_edits": []}
    return {"generator_version": 1, "seed": seed, "index": index, "family": "clear",
            "recipe": f"{HOLD_RECIPE}/stratum-{slot}", "task": task}


def hold_exit_candidate(seed, index):
    # Exactly matched physical starts; only the explicit training terminal rule differs.
    result = hold_candidate(seed, index)
    result["recipe"] = f"{HOLD_EXIT_RECIPE}/stratum-{index % 4}"
    result["task"]["hold_failure"] = HOLD_EXIT_RECIPE
    return result


SETTLE_RECIPE = "settle-recover-v1"
BEARING_CURRICULUM = "arrival-corridor-bearing-v1"
BEARING_MARGINS_DEGREES = (5., 20., 60., 100.)


def validate_position_curriculum(config):
    if "position_curriculum" not in config:
        return None
    spec = config["position_curriculum"]
    if (not isinstance(spec, dict) or set(spec) != {"kind", "stage"}
            or spec["kind"] != BEARING_CURRICULUM or type(spec["stage"]) is not int
            or not 0 <= spec["stage"] <= len(BEARING_MARGINS_DEGREES)
            or config.get("task_recipe") != SETTLE_RECIPE
            or config.get("recovery_dwell_ticks") != 30):
        raise ValueError("invalid position curriculum: settle-recover-v1, 30 dwell ticks and stage 0..4 required")
    return spec


def settle_candidate(seed, index):
    rng = random.Random(f"{SETTLE_RECIPE}:{seed}:{index}")
    slot, round_index = index % 4, index // 4
    task = hold_candidate(seed, index)["task"]
    start = task["start"]
    start.update(seed=rng.randrange(1, 2**31), bearing=rng.uniform(-math.pi, math.pi),
                 altitude=rng.uniform(140, 150), heading_offset=rng.uniform(-math.pi, math.pi))
    angle = start["bearing"] + start["heading_offset"] + (round_index % 4)*math.pi/2 + rng.uniform(-math.pi/8, math.pi/8)
    if slot in [1, 2]:
        size = rng.uniform(4., 8.) if slot == 1 else rng.uniform(1.2, 2.)
        vector = {"x": -size*math.sin(angle), "y": size*math.cos(angle)}
        if slot == 1:task["goal_offset"] = vector
        else:start["relative_velocity"] = vector
    elif slot == 3:
        start["spin"] = rng.uniform(.3, .5) * (1 if round_index % 2 == 0 else -1)
    task["arrival"]["dwell_ticks"] = 300
    task["deadline_ticks"] = 900
    return {"generator_version": 1, "seed": seed, "index": index, "family": "clear",
            "recipe": f"{SETTLE_RECIPE}/stratum-{slot}", "task": task}


def bearing_candidate(seed, index, stage):
    if type(stage) is not int or not 0 <= stage <= len(BEARING_MARGINS_DEGREES):
        raise ValueError("invalid bearing stage")
    result = settle_candidate(seed, index)
    if index % 4 != 1 or stage == len(BEARING_MARGINS_DEGREES):
        return result
    task = result["task"]; start = task["start"]
    distance = math.hypot(task["goal_offset"]["x"], task["goal_offset"]["y"])
    # The arrival disk subtends asin(radius/distance). Require additional turning
    # beyond that straight-flight corridor, with balanced left/right round indices.
    magnitude = math.asin(task["arrival"]["position"] / distance) + math.radians(BEARING_MARGINS_DEGREES[stage])
    direction = magnitude * (1 if (index // 4) % 2 == 0 else -1)
    angle = start["bearing"] + start["heading_offset"] + direction
    task["goal_offset"] = {"x": -distance*math.sin(angle), "y": distance*math.cos(angle)}
    return result


def generate(bridge, seed, per_stratum, max_rounds=128, recipe=RECIPE, recovery_dwell_ticks=None, position_curriculum=None):
    if not 1 <= per_stratum <= 128 or not 1 <= max_rounds <= 4096:
        raise ValueError("invalid clear-control quota")
    if recipe not in [RECIPE, ALIGNED_RECIPE, HOLD_RECIPE, HOLD_EXIT_RECIPE, SETTLE_RECIPE]:
        raise ValueError("unknown task recipe")
    if position_curriculum is not None:
        validate_position_curriculum({"position_curriculum": position_curriculum, "task_recipe": recipe,
                                      "recovery_dwell_ticks": recovery_dwell_ticks})
    if recovery_dwell_ticks is not None:
        if recipe != SETTLE_RECIPE or type(recovery_dwell_ticks) is not int or recovery_dwell_ticks not in [30, 300]:
            raise ValueError("recovery dwell override requires settle-recover-v1 and 30 or 300 ticks")
    make_candidate = {RECIPE: candidate, ALIGNED_RECIPE: aligned_candidate, HOLD_RECIPE: hold_candidate, HOLD_EXIT_RECIPE: hold_exit_candidate, SETTLE_RECIPE: settle_candidate}[recipe]
    if recipe == SETTLE_RECIPE:
        # The declared signed-direction matrix is fixed; no rejected-case replacements.
        max_rounds = min(max_rounds, per_stratum)
    pools = [[] for _ in STRATA]
    attempts = []
    for round_index in range(max_rounds):
        for slot in range(4):
            if len(pools[slot]) == per_stratum:
                continue
            task_candidate = make_candidate(seed, round_index*4+slot)
            if position_curriculum is not None:
                task_candidate = bearing_candidate(seed, round_index*4+slot, position_curriculum["stage"])
            if recovery_dwell_ticks is not None:
                task_candidate["task"]["arrival"]["dwell_ticks"] = recovery_dwell_ticks
            report = bridge.call("inspect", candidate=task_candidate)
            attempts.append(report)
            if report["admitted"]:
                pools[slot].append(report["candidate"])
        if all(len(pool) == per_stratum for pool in pools):
            break
    batch = {"recipe": recipe, "seed": seed, "per_stratum": per_stratum,
             "max_rounds": max_rounds, "complete": all(len(p) == per_stratum for p in pools),
             "attempts": attempts}
    if position_curriculum is not None:
        batch["position_curriculum"] = dict(position_curriculum)
    return batch, pools


def sample(bridge, config, partition):
    position_curriculum = validate_position_curriculum(config)
    if "recovery_dwell_ticks" in config and config.get("task_recipe") != SETTLE_RECIPE:
        raise ValueError("recovery dwell override requires settle-recover-v1")
    mode = config.get("training_hold_failure", "none")
    if mode not in ["none", HOLD_EXIT_RECIPE]:
        raise ValueError("unknown training hold terminal rule")
    if mode != "none" and config.get("task_recipe") != HOLD_RECIPE:
        raise ValueError("hold terminal rule requires the hold-position recipe")
    if mode == HOLD_EXIT_RECIPE and partition == "train":
        return generate(bridge, config["train_task_seed"], config["train_tasks_per_stratum"], recipe=HOLD_EXIT_RECIPE)
    if config.get("task_recipe") in [RECIPE, ALIGNED_RECIPE, HOLD_RECIPE, SETTLE_RECIPE]:
        return generate(bridge, config[f"{partition}_task_seed"], config[f"{partition}_tasks_per_stratum"],
                        recipe=config["task_recipe"], recovery_dwell_ticks=config.get("recovery_dwell_ticks"),
                        position_curriculum=position_curriculum if partition == "train" else None)
    if "task_recipe" in config:
        raise ValueError("unknown task recipe")
    from .bridge import task_pools
    quota = config["train_tasks_per_family" if partition == "train" else "eval_tasks_per_family"]
    batch = bridge.call("generate", seed=config[f"{partition}_task_seed"], per_family=quota, max_rounds=128)
    return batch, task_pools(batch)
