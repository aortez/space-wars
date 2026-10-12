//! Retained 2026-10-11 ground check for reproducing frozen diagnostic prefixes.
//! Keep this out of the default driving path; the course runner explicitly opts in.
use super::*;

pub(super) fn safe_drive_direction(state: &ScorchedState, player: usize, direction: f32) -> bool {
    ground_check(state, player, direction).reason.is_none()
}

pub(super) fn ground_check(state: &ScorchedState, player: usize, direction: f32) -> GroundCheck {
    let mut result = GroundCheck {
        direction,
        reason: None,
        probes: [None; driving::ROUTE_SAMPLES],
    };
    let tank = state.tanks[player];
    let motion = state.physics.motion(tank.body).unwrap();
    let clearance = if state.running_gear[player].is_some() {
        1.2
    } else {
        0.8
    };
    let foot = motion.position + Vec2::new(0.0, -clearance).rotate_radians(motion.angle);
    let mut previous = foot;
    for (index, distance) in [2.0, 3.25].into_iter().enumerate() {
        let x = foot.x + direction * distance;
        let probe = result.probes[index].insert(GroundProbe {
            x,
            point: None,
            normal: None,
        });
        if x.abs() > 46.0 {
            result.reason = Some("world edge");
            return result;
        }
        let Some(hit) = state.physics.cast_ray(
            Vec2::new(x, foot.y + 3.0),
            -Vec2::Y,
            RayCastOptions {
                max_distance: 6.0,
                exclude_entity: Some(tank.body.entity),
                ..Default::default()
            },
        ) else {
            result.reason = Some("no ground ahead");
            return result;
        };
        probe.point = Some([hit.point.x, hit.point.y]);
        probe.normal = Some([hit.normal.x, hit.normal.y]);
        result.reason = if state
            .tanks
            .iter()
            .any(|other| other.body.entity == hit.collider.entity)
        {
            Some("tank ahead")
        } else if hit.normal.y < 40.0_f32.to_radians().cos() {
            Some("steep surface")
        } else if (hit.point.y - previous.y).abs()
            > (hit.point.x - previous.x).abs() * 40.0_f32.to_radians().tan() + 0.35
        {
            Some("height change")
        } else {
            None
        };
        if result.reason.is_some() {
            return result;
        }
        previous = hit.point;
    }
    result
}
