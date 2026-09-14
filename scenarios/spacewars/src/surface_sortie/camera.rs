//! Read-only camera intent. Temporal presentation state belongs to the client.

use super::*;

pub const COMBAT_ENTER_DISTANCE: f32 = 260.0;
pub const COMBAT_EXIT_DISTANCE: f32 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraFocus {
    Vehicle,
    Spaceling,
}

/// A desired composition plus the active actor whose motion should be followed
/// without delay. Neither this value nor the client's camera history feeds AI
/// observations, physics, or gameplay decisions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraTarget {
    pub camera: Camera2,
    pub anchor: RenderPoint,
    pub focus: CameraFocus,
    pub framed_opponent: bool,
}

pub(super) fn target(
    state: &SurfaceSortieState,
    player: usize,
    hold_combat_frame: bool,
) -> CameraTarget {
    let snapshot = state.spaceling_snapshot(player);
    let ship = &state.world.ships[state.pilots[player].vehicle.0];
    let parked = state.vehicle_settled(player);
    let mut framed_opponent = false;
    let (center, height) = if let Some(snapshot) = snapshot {
        let pilot = snapshot.motion.position;
        let ship_center = ship.position + physics::ship_pivot(ship.form);
        if parked && pilot.distance_to(ship_center) < 32.0 {
            // North-up framing keeps a nearby parked ship and pilot together,
            // including on the sides and underside of a planet.
            let separation = ship_center - pilot;
            let height = 44.0_f32.max((separation.y.abs() + 12.0) / 0.48);
            ((pilot + ship_center) * 0.5, height)
        } else {
            (pilot + snapshot.up * 7.0, 44.0)
        }
    } else if parked {
        // Compose around the hull and nominal hatch from their transforms.
        // Camera selection must not run terrain/access-clearance queries at
        // every simulation tick. The boarding gate still tests the real floor.
        let hatch_offset = if ship.form == ShipForm::Ship {
            Vec2::new(8.0, -5.0)
        } else {
            Vec2::new(2.8, -0.65)
        };
        let hatch = ship.position
            + physics::ship_pivot(ship.form)
            + hatch_offset.rotate_radians(ship.rotation_radians);
        let up = Vec2::Y.rotate_radians(ship.rotation_radians);
        ((ship.position + hatch) * 0.5 + up * 2.0, 44.0)
    } else {
        let range = if hold_combat_frame {
            COMBAT_EXIT_DISTANCE
        } else {
            COMBAT_ENTER_DISTANCE
        };
        let opponent = state
            .combat_enabled()
            .then(|| {
                state.pilots.iter().enumerate().find_map(|(seat, pilot)| {
                    let other = &state.world.ships[pilot.vehicle.0];
                    (seat != player
                        && pilot.body.is_none()
                        && !other.dead
                        && other.form == ShipForm::Ship
                        && other.position.distance_to(ship.position) < range)
                        .then_some(other.position)
                })
            })
            .flatten();
        if let Some(opponent) = opponent {
            framed_opponent = true;
            let separation = opponent - ship.position;
            (
                (ship.position + opponent) * 0.5,
                (180.0_f32
                    .max(separation.x.abs() / 0.6)
                    .max(separation.y.abs() / 0.35))
                .min(440.0),
            )
        } else {
            (
                ship.position,
                if state.combat_enabled() && ship.form == ShipForm::Ship {
                    260.0
                } else {
                    100.0
                },
            )
        }
    };
    CameraTarget {
        camera: Camera2::new(render_point(center), height),
        anchor: render_point(snapshot.map_or(ship.position, |s| s.motion.position)),
        focus: if snapshot.is_some() {
            CameraFocus::Spaceling
        } else {
            CameraFocus::Vehicle
        },
        framed_opponent,
    }
}
