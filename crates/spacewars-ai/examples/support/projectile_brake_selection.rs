//! Initial motor-direction guard, not a trajectory or collision forecast.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct BrakeWarning {
    id: DebrisId,
    entry_seconds: f64,
    brake_toward: f64,
    change_toward: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct BrakeSelection {
    pub action: Mode,
    reason: &'static str,
    warnings: Vec<BrakeWarning>,
}

pub(super) fn select(
    o: &MissionObservationV1,
    d: &ProjectileDiagnostics,
    native: &CombatIntent,
) -> BrakeSelection {
    let mut result = BrakeSelection {
        action: Mode::Observe,
        reason: "unsupported motor observation",
        warnings: Vec::new(),
    };
    if d.unavailable_shells != 0 || d.shells_in_range > d.capacity {
        result.reason = "incomplete projectile sample";
        return result;
    }
    let f = &o.local.combat.recovery.flight;
    let p = &f.pilot;
    let ship = p.ship;
    let frame = p.planet.motion;
    let limits = f.flight.limits;
    let finite_vec = |v: engine_core::Vec2| v.x.is_finite() && v.y.is_finite();
    if ![
        f.flight.sweep,
        p.landing.assist_strength,
        ship.angle,
        frame.spin,
        limits.thrust_acceleration,
        limits.brake_acceleration,
        limits.brake_gain,
        limits.cruise_speed,
        limits.turn_speed,
        limits.turn_acceleration,
        d.observer_radius,
    ]
    .into_iter()
    .all(f32::is_finite)
        || ![ship.position, ship.velocity, frame.position, frame.velocity]
            .into_iter()
            .all(finite_vec)
        || limits.thrust_acceleration <= 0.0
        || limits.brake_acceleration <= 0.0
        || limits.brake_gain <= 0.0
        || !d.projectiles.iter().all(|q| {
            q.collision_radius.is_finite()
                && finite_vec(q.relative_position)
                && finite_vec(q.relative_velocity)
        })
    {
        return result;
    }
    if f.flight.sweep != 0.0 || f.flight.wings_closed || native.flight.wings.closed {
        result.reason = "wing transition or closed command";
        return result;
    }
    if p.landing.assist_strength != 0.0 {
        result.reason = "landing assist active";
        return result;
    }
    if native.flight.controls.brake_held {
        result.reason = "native braking already active";
        return result;
    }
    let warnings: Vec<_> = d
        .projectiles
        .iter()
        .filter_map(|q| {
            circle_entry(
                q.relative_position,
                q.relative_velocity,
                f64::from(d.observer_radius) + f64::from(q.collision_radius),
            )
            .map(|t| (q, t))
        })
        .collect();
    assert!(!warnings.is_empty(), "selection requires the first warning");
    if warnings
        .iter()
        .any(|(_, t)| *t < RESPONSE_TICKS as f64 / 60.0)
    {
        result.reason = "warning shorter than pulse";
        return result;
    }
    // f64 consumes the exact serialized f32 values for an independent audit.
    // The observed planet rotates: its center velocity is not the motor frame.
    let offset = [
        f64::from(ship.position.x) - f64::from(frame.position.x),
        f64::from(ship.position.y) - f64::from(frame.position.y),
    ];
    let relative = [
        f64::from(ship.velocity.x) - f64::from(frame.velocity.x)
            + f64::from(frame.spin) * offset[1],
        f64::from(ship.velocity.y)
            - f64::from(frame.velocity.y)
            - f64::from(frame.spin) * offset[0],
    ];
    let speed = relative[0].hypot(relative[1]);
    let brake = relative.map(|v| {
        if speed == 0.0 {
            0.0
        } else {
            -v / speed
                * (speed * f64::from(limits.brake_gain)).min(f64::from(limits.brake_acceleration))
        }
    });
    let angle = f64::from(ship.angle);
    let forward = [-angle.sin(), angle.cos()];
    let governor =
        ((f64::from(limits.cruise_speed) - relative[0] * forward[0] - relative[1] * forward[1])
            / 10.0)
            .clamp(0.0, 1.0);
    let acceleration = if native.flight.controls.primary_held {
        f64::from(limits.thrust_acceleration) * governor
    } else {
        0.0
    };
    for (q, entry_seconds) in warnings {
        let r = [
            f64::from(q.relative_position.x),
            f64::from(q.relative_position.y),
        ];
        let distance = r[0].hypot(r[1]);
        result.warnings.push(BrakeWarning {
            id: q.id,
            entry_seconds,
            brake_toward: (brake[0] * r[0] + brake[1] * r[1]) / distance,
            change_toward: ((brake[0] - forward[0] * acceleration) * r[0]
                + (brake[1] - forward[1] * acceleration) * r[1])
                / distance,
        });
    }
    if result
        .warnings
        .iter()
        .any(|q| !(q.brake_toward < 0.0 && q.change_toward < 0.0))
    {
        result.reason = "braking does not move away from every warning";
        return result;
    }
    result.action = Mode::Brake;
    result.reason = "pulse fits and braking initially moves away";
    result
}

#[cfg(test)]
#[path = "projectile_brake_selection_tests.rs"]
mod tests;
