//! Read-only context for ordinary CPU-match reviews.
use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct CombatTank {
    pub drive: DriveSnapshot,
    pub away_ground: GroundCheck,
    pub range: f32,
    pub next_shot: u64,
    pub own_shots: u64,
    pub barrel_blocked: bool,
    pub ready_to_shoot: bool,
    pub elevation_degrees: f32,
    pub facing: f32,
    pub controller: serde_json::Value,
    pub footing_candidate: Option<driving::FootingTarget>,
}

pub fn combat_tanks(state: &ScorchedState) -> [CombatTank; 2] {
    std::array::from_fn(|player| {
        let tank = state.tanks[player];
        let x = state.physics.motion(tank.body).unwrap().position.x;
        let other_x = state
            .physics
            .motion(state.tanks[1 - player].body)
            .unwrap()
            .position
            .x;
        let toward = if other_x >= x { 1.0 } else { -1.0 };
        let (pivot, muzzle) = state.muzzle(player);
        CombatTank {
            drive: snapshot(state, player, toward),
            away_ground: state.check_drive_direction(player, -toward),
            range: (other_x - x).abs(),
            next_shot: tank.next_shot,
            own_shots: tank.shots,
            barrel_blocked: state
                .physics
                .cast_ray(
                    pivot,
                    muzzle - pivot,
                    RayCastOptions {
                        max_distance: (muzzle - pivot).length(),
                        exclude_entity: Some(tank.body.entity),
                        ..Default::default()
                    },
                )
                .is_some(),
            ready_to_shoot: state.ready_to_shoot(player, ScorchedState::has_track_support),
            elevation_degrees: tank.elevation.to_degrees(),
            facing: tank.facing,
            controller: serde_json::to_value(&state.driving[player]).unwrap(),
            footing_candidate: state.firing_footing_target(player),
        }
    })
}
