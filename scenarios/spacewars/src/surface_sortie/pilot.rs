//! Read-only, bounded sensors for a pilot controller. No Rapier handles cross
//! this boundary, and observations never advance physics or flush its queries.
use super::*;
use engine_rapier::world::RayHit;

pub const PILOT_OBSERVATION_VERSION: u32 = 1;
pub const LANDING_SITE_COUNT: u8 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LandingSiteId {
    pub planet: usize,
    /// Fixed bearing in the material body's local frame, not world north.
    pub bearing: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PilotMotion {
    pub position: Vec2,
    pub velocity: Vec2,
    pub angle: f32,
    pub spin: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PilotLandingSite {
    /// The hatch stays usable across small combined slide/tilt corrections.
    pub hatch_has_settling_margin: bool,
    pub id: LandingSiteId,
    pub revision: u64,
    pub local_position: Vec2,
    pub position: Vec2,
    pub normal: Vec2,
    pub velocity: Vec2,
    /// Suggested body origin with both rear feet on the measured surface.
    pub vehicle_position: Vec2,
    pub hatch_position: Vec2,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PilotPlanetObservation {
    pub index: usize,
    pub motion: PilotMotion,
    /// A conservative routing bound, never proof of standing or landing.
    pub radius: f32,
    pub revision: u64,
    pub claim: Option<PlanetClaimObservation>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PilotObservationV1 {
    pub version: u32,
    pub tick: u64,
    pub owner: PlayerId,
    pub spaceling: SpacelingId,
    pub vehicle: VehicleId,
    pub location: PilotLocation,
    pub controls_armed: bool,
    pub queries_ready: bool,
    pub ship: PilotMotion,
    /// Last shared gravity solve expressed as acceleration at the scenario's
    /// fixed 60 Hz. Zero before the first completed tick.
    pub gravity: Vec2,
    pub ship_available: bool,
    pub ship_form: ShipForm,
    pub ship_health: f32,
    pub actor: Option<PilotMotion>,
    pub actor_up: Vec2,
    pub supported_planet: Option<usize>,
    pub balanced: bool,
    pub relative_speed: f32,
    pub landing: LandingTelemetry,
    pub hatch: Option<Vec2>,
    pub transfer: TransferResult,
    pub last_transfer: TransferResult,
    pub transfers: u64,
    pub recovery: Option<SurfaceRecoveryObservation>,
    pub planet: PilotPlanetObservation,
    /// With a requested ID, revalidates only that site. Otherwise surveys at
    /// most 64 bearings. Empty while queries are dirty does not mean no ground.
    pub sites: Vec<PilotLandingSite>,
}

impl PilotPlanetObservation {
    pub fn velocity_at(&self, position: Vec2) -> Vec2 {
        let offset = position - self.motion.position;
        self.motion.velocity + Vec2::new(-offset.y, offset.x) * self.motion.spin
    }
}

impl SurfaceSortieState {
    pub fn pilot_observation(
        &self,
        player: usize,
        requested_site: Option<LandingSiteId>,
    ) -> PilotObservationV1 {
        let pilot = &self.pilots[player];
        let ship = &self.world.ships[pilot.vehicle.0];
        let body = self.world.physics.ship_body(pilot.vehicle.0);
        let ship_motion = self.world.physics.world.motion(body);
        let snapshot = self.spaceling_snapshot(player);
        let index = self.motion_planet_index(player);
        let frame = motion::SurfaceFrame::read(&self.world.physics, index);
        let ready = !self.world.physics.material_queries_dirty;
        let revision = self
            .world
            .terrain
            .planets
            .get(&index)
            .map_or(0, |p| p.field.revision());
        let sites = if !ready || !self.has_material_ground() || ship.form != ShipForm::Ship {
            Vec::new()
        } else if let Some(id) = requested_site {
            self.pilot_landing_site(player, id).into_iter().collect()
        } else {
            (0..LANDING_SITE_COUNT)
                .filter_map(|bearing| {
                    self.pilot_landing_site(
                        player,
                        LandingSiteId {
                            planet: index,
                            bearing,
                        },
                    )
                })
                .collect()
        };
        PilotObservationV1 {
            version: PILOT_OBSERVATION_VERSION,
            tick: self.world.tick,
            owner: pilot.owner,
            spaceling: pilot.id,
            vehicle: pilot.vehicle,
            location: self.location(player),
            controls_armed: pilot.controls_armed,
            queries_ready: ready,
            ship: PilotMotion {
                position: ship_motion.map_or(ship.position + physics::ship_pivot(ship.form), |m| {
                    m.position
                }),
                velocity: ship_motion
                    .and_then(|m| self.world.physics.world.velocity_at_point(body, m.position))
                    .unwrap_or(ship.velocity),
                angle: ship_motion.map_or(ship.rotation_radians, |m| m.angle),
                spin: ship_motion.map_or(0.0, |m| m.angular_velocity),
            },
            ship_available: self.vehicle_accessible(player),
            gravity: pilot.ship_gravity_delta * 60.0,
            ship_form: ship.form,
            ship_health: ship.life,
            actor: snapshot.map(|s| PilotMotion {
                position: s.motion.position,
                velocity: s.motion.linear_velocity,
                angle: s.motion.angle,
                spin: s.motion.angular_velocity,
            }),
            actor_up: snapshot.map_or(Vec2::Y, |s| s.up),
            supported_planet: self.pilot_support_planet(player),
            balanced: snapshot.is_some_and(|s| s.balance == SpacelingBalance::Balanced),
            relative_speed: snapshot.map_or(0.0, |s| s.relative_speed),
            landing: pilot.landing,
            hatch: self.material_access(player).map(|hit| hit.point),
            transfer: if ready {
                self.transfer_readiness(player)
            } else {
                TransferResult::ExitBlocked
            },
            last_transfer: pilot.last_transfer,
            transfers: pilot.transfers,
            recovery: pilot.recovery.as_ref().map(|r| r.observation()),
            planet: PilotPlanetObservation {
                index,
                motion: PilotMotion {
                    position: frame.position,
                    velocity: frame.linear_velocity,
                    angle: frame.angle,
                    spin: frame.angular_velocity,
                },
                radius: self.world.planets[index].radius,
                revision,
                claim: self.claim_observation(index, player),
            },
            sites,
        }
    }

    fn pilot_landing_site(&self, player: usize, id: LandingSiteId) -> Option<PilotLandingSite> {
        self.vehicle_landing_site(player, id, false)
    }

    pub(super) fn vehicle_landing_site(
        &self,
        player: usize,
        id: LandingSiteId,
        pod: bool,
    ) -> Option<PilotLandingSite> {
        if id.bearing >= LANDING_SITE_COUNT || self.world.physics.material_queries_dirty {
            return None;
        }
        let terrain = self.world.terrain.planets.get(&id.planet)?;
        let frame = motion::SurfaceFrame::read(&self.world.physics, id.planet);
        let up = Vec2::Y.rotate_radians(
            frame.angle
                + f32::from(id.bearing) * std::f32::consts::TAU / f32::from(LANDING_SITE_COUNT),
        );
        let right = Vec2::new(up.y, -up.x);
        let origin = frame.position + up * (self.world.planets[id.planet].radius + 10.0);
        let ground = |origin, direction, length| {
            self.world
                .physics
                .material_ground_ray(id.planet, origin, direction, length)
        };
        let foot_span = if pod { 0.7 } else { 3.0 };
        let left = ground(origin - right * foot_span, -up, 35.0)?;
        let right_hit = ground(origin + right * foot_span, -up, 35.0)?;
        if left.normal.dot(up) < 0.65 || right_hit.normal.dot(up) < 0.65 {
            return None;
        }
        let tangent = (right_hit.point - left.point).normalized();
        let normal = Vec2::new(-tangent.y, tangent.x);
        // A pod recovery also needs a short walk to the rebuilt ship. Prefer
        // flatter ground so it does not strand the pilot beside a cell ledge.
        if normal.dot(up) < if pod { 0.995 } else { 0.98 } {
            return None;
        }
        let position = left.point.midpoint(right_hit.point);
        let vehicle_position = position + normal * if pod { 0.85 } else { 5.45 };
        // Rays can miss a step under the hull between the feet. Allow modest
        // sideways drift and settling depth when checking the complete hull.
        if !pod
            && ![-0.75, 0.0, 0.75].into_iter().all(|offset| {
                self.world.physics.surface_hull_fits_at(
                    self.pilots[player].vehicle.0,
                    vehicle_position + Vec2::new(normal.y, -normal.x) * offset - normal * 0.2,
                    rotation_for_direction(normal),
                )
            })
        {
            return None;
        }
        // Other pilots' vehicles occupy space even though they cannot be
        // mistaken for material ground by the footing rays.
        if self.pilots.iter().enumerate().any(|(index, pilot)| {
            index != player
                && !self.world.ships[pilot.vehicle.0].dead
                && self
                    .world
                    .physics
                    .world
                    .motion(self.world.physics.ship_body(pilot.vehicle.0))
                    .is_some_and(|body| body.position.distance_to(vehicle_position) < 16.0)
        }) {
            return None;
        }
        // Check the belly and the exit floor separately. Rays stop on fragments,
        // which therefore cannot masquerade as retained planet material.
        let belly = ground(vehicle_position, -normal, 8.0)?;
        if belly.distance < if pod { 0.6 } else { 4.4 } {
            return None;
        }
        if pod {
            // The pod hull is wider than its feet. A cell step under either
            // lower corner can hold both feet off the ground even when the
            // central belly and the two foot rays look suitable.
            let tangent = Vec2::new(normal.y, -normal.x);
            for side in [-1.0, 1.0] {
                let corner = ground(vehicle_position + tangent * side, -normal, 3.0)?;
                if corner.distance < 0.42 {
                    return None;
                }
            }
        }
        let spec = Self::spec();
        // Forecast the exit beside the proposed landed vehicle. Testing against
        // this ship's current airborne pose can reject its own destination and
        // repeatedly interrupt touchdown. Keep the proposed hull/feet and every
        // other obstacle in the clearance test; this grants no real transfer.
        let vehicle = self.pilots[player].vehicle.0;
        let world_clear = self.world.physics.world.capsule_clearance_test_excluding(
            spec.half_segment,
            spec.radius + 0.04,
            spec.collision_groups,
            vec![
                self.world.physics.surface_vehicle_entity(vehicle),
                pilot_physics_id(self.pilots[player].owner),
            ],
        );
        let vehicle_clear = self.world.physics.surface_vehicle_capsule_clearance(
            vehicle,
            &self.world.ships[vehicle],
            spec.half_segment,
            spec.radius + 0.04,
        );
        let hatch_clear = |hatch: RayHit, position: Vec2, angle: f32| {
            let radial = (position - frame.position).normalized();
            hatch.normal.dot(radial) >= 0.65
                && [hatch.normal, radial].into_iter().all(|axis| {
                    let point = hatch.point + axis * (spec.half_height() + 0.12);
                    let rotation = rotation_for_direction(axis);
                    world_clear(point, rotation) && vehicle_clear(point, rotation, position, angle)
                })
        };
        let mut hatch_has_settling_margin = false;
        let hatch = if pod {
            let origin = vehicle_position + Vec2::new(normal.y, -normal.x) * 2.8;
            let hatch = ground(origin, -normal, 5.0)?;
            hatch_clear(hatch, vehicle_position, rotation_for_direction(normal)).then_some(hatch)?
        } else {
            // Use the real exit pose and leave room to stand radially upright.
            // Require room for lateral drift, then report whether a combined
            // slide and tilt is also safe. A bot may need to retry a one-foot
            // stop when rotating here would close the hatch.
            let hatch_at = |position: Vec2, angle: f32| {
                self.material_access_with_clearance(
                    id.planet,
                    ShipForm::Ship,
                    position,
                    angle,
                    |point, rotation| {
                        world_clear(point, rotation)
                            && vehicle_clear(point, rotation, position, angle)
                    },
                )
                .filter(|hit| hatch_clear(*hit, position, angle))
            };
            let angle = rotation_for_direction(normal);
            for offset in [-0.75, 0.75] {
                hatch_at(
                    vehicle_position + Vec2::new(normal.y, -normal.x) * offset,
                    angle,
                )?;
            }
            hatch_has_settling_margin = [-0.75, 0.0, 0.75].into_iter().all(|offset| {
                [-0.1, 0.0, 0.1].into_iter().all(|turn| {
                    hatch_at(
                        vehicle_position + Vec2::new(normal.y, -normal.x) * offset,
                        angle + turn,
                    )
                    .is_some()
                })
            });
            hatch_at(vehicle_position, angle)?
        };
        Some(PilotLandingSite {
            hatch_has_settling_margin,
            id,
            revision: terrain.field.revision(),
            local_position: (position - frame.position).rotate_radians(-frame.angle),
            position,
            normal,
            velocity: motion::point_velocity(frame, vehicle_position),
            vehicle_position,
            hatch_position: hatch.point,
        })
    }
}

/// Initial conditions for a material flight acceptance case. These only place
/// the initial vehicle; all landing, transfers and claims use ordinary actions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MaterialFlightStart {
    pub bearing: f32,
    pub altitude: f32,
    pub radial_speed: f32,
    pub lateral_speed: f32,
    pub heading_offset: f32,
}

impl SurfaceSortieScenario {
    pub fn init_material_flight(
        seed: u64,
        players: usize,
        starts: &[(PlayerId, MaterialFlightStart)],
    ) -> SurfaceSortieState {
        let mut state = Self::init_material(seed, players);
        for &(owner, start) in starts {
            assert!(owner.index() < players);
            assert!(
                [
                    start.bearing,
                    start.altitude,
                    start.radial_speed,
                    start.lateral_speed,
                    start.heading_offset
                ]
                .into_iter()
                .all(f32::is_finite)
            );
            assert!((15.0..=150.0).contains(&start.altitude));
            let frame = state.planet_motion(owner.index());
            let up = Vec2::Y.rotate_radians(start.bearing);
            let position = frame.position + up * (SURFACE_RADIUS + start.altitude + 5.45);
            let velocity = motion::point_velocity(frame, position)
                + up * start.radial_speed
                + Vec2::new(up.y, -up.x) * start.lateral_speed;
            let angle = rotation_for_direction(up) + start.heading_offset;
            let ship = &mut state.world.ships[owner.index()];
            ship.position = position - SHIP_PIVOT;
            ship.velocity = velocity;
            ship.rotation_radians = angle;
            ship.direction = Vec2::Y.rotate_radians(angle);
            let body = state.world.physics.ship_body(owner.index());
            assert!(
                state
                    .world
                    .physics
                    .world
                    .set_pose(body, position, angle, true)
            );
            assert!(state.world.physics.world.set_velocity(
                body,
                velocity,
                frame.angular_velocity,
                true
            ));
        }
        state.world.physics.material_queries_dirty = true;
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_rapier::world::{
        BodyId as PhysicsBodyId, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
    };
    use engine_terrain::{Brush, EditMode, TerrainEdit};
    const DT: Duration = Duration::from_nanos(16_666_667);

    #[test]
    fn landing_forecast_moves_its_own_ship_but_keeps_other_obstacles() {
        let mut state = SurfaceSortieScenario::init_material_surface(
            42,
            1,
            engine_terrain::TerrainSurface::Interpolated,
        );
        SurfaceSortieScenario::step(&mut state, &[], DT);
        let site = state.pilot_observation(0, None).sites[0];
        let body = state.world.physics.ship_body(0);
        let right = Vec2::new(site.normal.y, -site.normal.x);
        // The arriving hull overlaps the forecast exit, while both feet remain
        // airborne. The ship will occupy a different pose after touchdown.
        state.world.physics.world.set_pose(
            body,
            site.vehicle_position + right * 8.0,
            rotation_for_direction(site.normal),
            true,
        );
        state
            .world
            .physics
            .world
            .set_velocity(body, Vec2::ZERO, 0.0, true);
        state.world.physics.world.step(DT.as_secs_f32());
        let spec = SurfaceSortieState::spec();
        let hatch = site.hatch_position + site.normal * (spec.half_height() + 0.12);
        let clear = |excluded| {
            state.world.physics.world.capsule_is_clear_except(
                hatch,
                rotation_for_direction(site.normal),
                spec.half_segment,
                spec.radius + 0.04,
                spec.collision_groups,
                excluded,
            )
        };
        assert!(
            !clear(None),
            "the live ship really obstructs the proposed exit"
        );
        assert!(clear(Some(body.entity)));
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let forecast = state.pilot_observation(0, Some(site.id));
        assert_eq!(
            forecast.sites.len(),
            1,
            "own airborne hull must not invalidate the site"
        );
        assert_ne!(forecast.landing.phase, LandingPhase::Landed);
        assert_ne!(forecast.transfer, TransferResult::Ready);
        assert_eq!(before, state.world.physics.world.snapshot_bytes().unwrap());

        let obstacle = PhysicsId::new(45_124);
        assert!(state.world.physics.world.insert_body(
            PhysicsBodyId::new(obstacle, BodyRole::PRIMARY),
            BodySpec {
                kind: engine_rapier::world::BodyKind::Fixed,
                position: hatch,
                ..BodySpec::default()
            },
            &[ColliderSpec::ball(
                ColliderId::new(obstacle, ColliderRole::PRIMARY, 0),
                2.0
            )],
        ));
        state.world.physics.world.step(DT.as_secs_f32());
        assert!(
            state.pilot_observation(0, Some(site.id)).sites.is_empty(),
            "a different solid obstacle must still reject the forecast"
        );
    }

    #[test]
    fn sensors_are_read_only_and_sites_follow_material_frames_and_revisions() {
        let mut state = SurfaceSortieScenario::init_material_flight(
            42,
            1,
            &[(
                PlayerId::PLAYER_1,
                MaterialFlightStart {
                    bearing: 0.0,
                    altitude: 45.0,
                    radial_speed: 0.0,
                    lateral_speed: 0.0,
                    heading_offset: 0.0,
                },
            )],
        );
        let dirty = state.pilot_observation(0, None);
        assert!(!dirty.queries_ready && dirty.sites.is_empty());
        SurfaceSortieScenario::step(&mut state, &[], DT);
        let before = SurfaceSortieScenario::observe(&state);
        let survey = state.pilot_observation(0, None);
        assert!(survey.queries_ready && !survey.sites.is_empty());
        assert!(survey.sites.len() <= LANDING_SITE_COUNT as usize);
        assert_eq!(
            before.payload,
            SurfaceSortieScenario::observe(&state).payload
        );
        let site = survey.sites[0];
        for _ in 0..30 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
        let moved = state.pilot_observation(0, Some(site.id));
        assert_eq!(moved.sites.len(), 1);
        assert!(site.position.distance_to(moved.sites[0].position) > 0.1);
        assert!(
            site.local_position
                .distance_to(moved.sites[0].local_position)
                < 0.01
        );
        let point = site.local_position - (site.local_position.normalized() * 0.1);
        let cell = state.world.terrain.planets[&0]
            .field
            .local_to_cell(point)
            .unwrap();
        state
            .world
            .queue_planet_edit(
                0,
                TerrainEdit {
                    brush: Brush::Circle {
                        center: cell,
                        radius: 10,
                    },
                    mode: EditMode::Remove,
                },
            )
            .unwrap();
        // A survey cannot force the queued edit to commit or update the index.
        assert_eq!(
            state.pilot_observation(0, Some(site.id)).planet.revision,
            site.revision
        );
        SpacewarsScenario::prepare_terrain(&mut state.world, &[]);
        let dirty = state.pilot_observation(0, Some(site.id));
        assert!(!dirty.queries_ready && dirty.sites.is_empty());
        SurfaceSortieScenario::step(&mut state, &[], DT);
        let changed = state.pilot_observation(0, Some(site.id));
        assert!(changed.queries_ready);
        assert!(changed.planet.revision > site.revision);
        assert!(
            changed.sites.is_empty()
                || changed.sites[0]
                    .local_position
                    .distance_to(site.local_position)
                    > 2.0
        );
        assert!(state.terrain_diagnostics().issues.is_empty());
    }
}
