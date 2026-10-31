"""Bounded potential shaping, distinct from independent physical success."""
import math


def potential(frame, velocity_weight=0.25):
    if not math.isfinite(velocity_weight) or velocity_weight < 0:
        raise ValueError("velocity shaping weight must be finite and nonnegative")
    errors = frame["errors"]
    return (-0.5 * math.tanh(errors["position"] / 100)
            - velocity_weight * math.tanh(errors["velocity"] / 20)
            - 0.25 * math.tanh(errors["spin"]))


def deadline_reward(distance, weight=0.0):
    """Bounded distance-only grade for the fixed 3-unit clear-control arrival radius."""
    if not math.isfinite(weight) or not 0 <= weight <= 4:
        raise ValueError("deadline distance weight must be finite and in [0, 4]")
    if not math.isfinite(distance) or distance < 0:
        raise ValueError("deadline distance must be finite and nonnegative")
    excess = max(distance - 3.0, 0.0)
    return -2.0 - weight * excess / (30.0 + excess)


def regulation_cost(frame, weight=0.0):
    """Bounded near-goal state cost, separate from the physical success criteria."""
    if not math.isfinite(weight) or not 0 <= weight <= .003:
        raise ValueError("regulation weight must be finite and in [0, .003]")
    e = frame["errors"]
    if not all(math.isfinite(e[k]) and e[k] >= 0 for k in ["position", "velocity", "spin"]):
        raise ValueError("invalid physical error")
    def grade(value, scale):
        ratio = value / scale
        return 1 / (1 + (1 / ratio)**2) if ratio > 1 else ratio * ratio / (1 + ratio * ratio)
    return -weight * (.5 * grade(e["position"], 3.)
                      + .25 * grade(e["velocity"], 1.) + .25 * grade(e["spin"], .2))


def reward(before, after, gamma, velocity_weight=0.25, deadline_distance_weight=0.0, regulation_weight=0.0):
    outcome = after["progress"]["outcome"]
    if outcome == "environment_error":
        raise RuntimeError("environment error is not a learning terminal")
    terminal = {None: 0.0, "success": 10.0, "deadline": -2.0,
                "collision": -10.0, "damage": -10.0, "ship_lost": -10.0,
                "boundary_escape": -10.0, "controller_error": -10.0, "hold_broken": -10.0}[outcome]
    if outcome == "deadline":
        terminal = deadline_reward(after["frame"]["errors"]["position"], deadline_distance_weight)
    shaping = gamma * (potential(after["frame"], velocity_weight) if outcome is None else 0.0) - potential(before["frame"], velocity_weight)
    result = {"terminal": terminal, "time": -0.001, "potential": shaping,
              "total": terminal - 0.001 + shaping}
    cost = regulation_cost(after["frame"], regulation_weight)
    if regulation_weight:
        result["regulation"] = cost
        result["total"] += cost
    return result
