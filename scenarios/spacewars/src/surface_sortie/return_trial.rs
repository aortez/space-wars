//! Initial conditions for a return leg, independent of the preceding flight.
//! Only construction places actors. Subsequent claims, losses and transfers use
//! the ordinary shared step and controls, with no forced ownership or landing.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnTrial {
    Reachable,
    TippedShip,
    OtherPlanet,
}

impl SurfaceSortieScenario {
    pub fn init_material_return_trial(
        seed: u64,
        player: usize,
        mirror: bool,
        bearing: f32,
        trial: ReturnTrial,
    ) -> SurfaceSortieState {
        assert!(player < SPACEWARS_PLAYER_COUNT && bearing.is_finite());
        let mut state = Self::init_material_travel_trial(seed, mirror, bearing);
        // Complete the initial material edit before querying retained ground.
        Self::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let ship_planet = if trial == ReturnTrial::OtherPlanet {
            1 - player
        } else {
            player
        };
        let surface = motion::SurfaceFrame::read(&state.world.physics, ship_planet);
        let ship_up = Vec2::Y.rotate_radians(
            bearing
                + if trial == ReturnTrial::OtherPlanet {
                    std::f32::consts::PI
                } else {
                    0.0
                },
        );
        let center = surface.position + ship_up * (state.world.planets[ship_planet].radius + 7.0);
        let angle = rotation_for_direction(ship_up)
            + if trial == ReturnTrial::TippedShip {
                std::f32::consts::FRAC_PI_2
            } else {
                0.0
            };
        let velocity = motion::point_velocity(surface, center);
        let ship = &mut state.world.ships[player];
        ship.position = center - SHIP_PIVOT;
        ship.rotation_radians = angle;
        ship.direction = Vec2::Y.rotate_radians(angle);
        ship.velocity = velocity;
        ship.omega = physics::control_angular_velocity(ship, surface.angular_velocity);
        let ship_body = state.world.physics.ship_body(player);
        assert!(
            state
                .world
                .physics
                .world
                .set_pose(ship_body, center, angle, true)
        );
        assert!(state.world.physics.world.set_velocity(
            ship_body,
            velocity,
            surface.angular_velocity,
            true
        ));
        state.pilots[player].planet = ship_planet;

        let surface = motion::SurfaceFrame::read(&state.world.physics, player);
        let up = Vec2::Y.rotate_radians(bearing - 0.2);
        let ground = state
            .world
            .physics
            .material_ground_ray(player, surface.position + up * 80.0, -up, 100.0)
            .expect("initial retained standing ground");
        let center = ground.point + up * (SurfaceSortieState::spec().half_height() + 0.12);
        let body = SpacelingAssembly::insert(
            &mut state.world.physics.world,
            pilot_physics_id(state.pilots[player].owner),
            center,
            rotation_for_direction(up),
            SurfaceSortieState::spec(),
        )
        .expect("one initial spaceling");
        state.world.physics.world.set_velocity(
            body.body(),
            motion::point_velocity(surface, center),
            surface.angular_velocity,
            true,
        );
        state.pilots[player].body = Some(body);
        state.pilots[player].controls_armed = false;
        state.world.physics.material_queries_dirty = true;
        state
    }
}
