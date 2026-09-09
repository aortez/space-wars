//! Shared rebuild placement and read-only relocation previews. A proposal is
//! checked again by recovery at the actual build tick; it reserves no space.
use super::*;
use ground_navigation::{GROUND_SAMPLES, GroundMap, GroundRouteDiagnostics};

const LOCAL_HALF_SPAN: i32 = 56;
pub const MAX_REBUILD_WALK: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RebuildRejection {
    QueriesPending,
    NoGround,
    HullObstructed,
    NoHatchFooting,
    NoHatchRoute,
    HatchRouteTooLong,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RebuildAttempt {
    pub offset: f32,
    pub rejection: Option<RebuildRejection>,
    /// Proposed poses in the planet's local frame.
    pub center: Option<Vec2>,
    pub hatch: Option<Vec2>,
    pub route: Option<GroundRouteDiagnostics>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RebuildPlacementReport {
    pub tick: u64,
    pub planet: usize,
    pub revision: Option<u64>,
    pub standing: Vec2,
    pub selected_offset: Option<f32>,
    pub attempts: Vec<RebuildAttempt>,
}
impl RebuildPlacementReport {
    pub(super) fn blocked_status(&self) -> SurfaceRecoveryStatus {
        if self.attempts.iter().any(|a| {
            matches!(
                a.rejection,
                Some(
                    RebuildRejection::NoHatchFooting
                        | RebuildRejection::NoHatchRoute
                        | RebuildRejection::HatchRouteTooLong
                )
            )
        }) {
            SurfaceRecoveryStatus::HatchBlocked
        } else {
            SurfaceRecoveryStatus::ClearanceBlocked
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RebuildStandingSite {
    pub planet: usize,
    pub revision: u64,
    pub position: Vec2,
    pub walk_length: f32,
    pub hatch_walk_length: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RebuildRelocationAttempt {
    pub bearing: u16,
    pub route: Option<GroundRouteDiagnostics>,
    pub placement: Option<RebuildPlacementReport>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RebuildRelocationSurvey {
    pub tick: u64,
    pub checked: usize,
    pub attempts: Vec<RebuildRelocationAttempt>,
    pub site: Option<RebuildStandingSite>,
}
pub(super) struct RebuildPose {
    pub center: Vec2,
    pub normal: Vec2,
}

impl SurfaceSortieState {
    pub(super) fn replacement_ship(&self, player: usize) -> ShipState {
        let owner = self.pilots[player].owner;
        let mut ship = ShipState::new(
            owner.index(),
            Vec2::ZERO,
            self.world.players[owner.index()].color,
            self.world.players[owner.index()].health_percent,
            1.0 / 60.0,
        );
        if self.pilots[player].combat.is_some() {
            ship.enable_weapon_supply();
        }
        ship
    }

    pub(super) fn rebuild_ground_map(
        &self,
        player: usize,
        planet: usize,
        point: Vec2,
    ) -> Option<GroundMap> {
        self.local_ground_map(player, planet, point, true)
    }

    fn local_ground_map(
        &self,
        player: usize,
        planet: usize,
        point: Vec2,
        replacing: bool,
    ) -> Option<GroundMap> {
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let local = (point - frame.position).rotate_radians(-frame.angle);
        let bearing = (rotation_for_direction(local) * GROUND_SAMPLES as f32
            / std::f32::consts::TAU)
            .round() as i32;
        self.survey_ground(
            player,
            planet,
            (-LOCAL_HALF_SPAN..=LOCAL_HALF_SPAN)
                .map(|offset| (bearing + offset).rem_euclid(GROUND_SAMPLES as i32) as u16),
            replacing,
        )
    }

    pub(super) fn find_rebuild_placement(
        &self,
        player: usize,
        planet: usize,
        point: Vec2,
        up: Vec2,
        map: Option<&GroundMap>,
    ) -> (Option<RebuildPose>, RebuildPlacementReport) {
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let local = |point: Vec2| (point - frame.position).rotate_radians(-frame.angle);
        let world_point = |point: Vec2| frame.position + point.rotate_radians(frame.angle);
        let material = self.world.terrain.planets.get(&planet);
        let mut report = RebuildPlacementReport {
            tick: self.world.tick,
            planet,
            revision: material.map(|t| t.field.revision()),
            standing: local(point),
            selected_offset: None,
            attempts: Vec::new(),
        };
        if self.world.physics.material_queries_dirty || material.is_some() && map.is_none() {
            report.attempts.push(RebuildAttempt {
                offset: 0.0,
                rejection: Some(RebuildRejection::QueriesPending),
                center: None,
                hatch: None,
                route: None,
            });
            return (None, report);
        }
        let replacement = self.replacement_ship(player);
        let radius = physics::SpacewarsPhysics::surface_vehicle_clearance_radius(&replacement);
        let spec = Self::spec();
        let preview_clear = self.world.physics.replacement_capsule_clearance(
            self.pilots[player].vehicle.0,
            &replacement,
            spec.half_segment,
            spec.radius + 0.02,
        );
        let right = Vec2::new(up.y, -up.x);
        let ground = |origin, direction, length| {
            if material.is_some() {
                self.world
                    .physics
                    .material_ground_ray(planet, origin, direction, length)
            } else {
                self.world
                    .physics
                    .world
                    .cast_ray(
                        origin,
                        direction,
                        engine_rapier::world::RayCastOptions {
                            max_distance: length,
                            collision_groups: physics::spaceling_collision_groups(),
                            ..Default::default()
                        },
                    )
                    .filter(|hit| physics::is_planet_surface_support(hit.collider, planet))
            }
        };
        let mut best = None;
        let mut best_cost = f32::INFINITY;
        for offset in [-8.0, -14.0, 8.0, 14.0] {
            let mut attempt = RebuildAttempt {
                offset,
                rejection: None,
                center: None,
                hatch: None,
                route: None,
            };
            type Candidate = (RebuildPose, Option<Vec2>, Option<GroundRouteDiagnostics>);
            let evaluate = || -> Result<Candidate, RebuildRejection> {
                let hit = ground(point + right * offset + up * 12.0, -up, 24.0)
                    .ok_or(RebuildRejection::NoGround)?;
                if hit.normal.dot(up) < 0.8 {
                    return Err(RebuildRejection::NoGround);
                }
                let mut normal = hit.normal;
                let mut floor = hit.point;
                // Predict the resting pose from both actual feet, just as a
                // landing survey does, instead of a single staircase normal.
                if material.is_some() {
                    let left = ground(hit.point - right * 3.0 + up * 8.0, -up, 16.0)
                        .ok_or(RebuildRejection::NoGround)?;
                    let right_hit = ground(hit.point + right * 3.0 + up * 8.0, -up, 16.0)
                        .ok_or(RebuildRejection::NoGround)?;
                    let tangent = (right_hit.point - left.point).normalized();
                    normal = Vec2::new(-tangent.y, tangent.x);
                    if normal.dot(up) < 0.98
                        || left.normal.dot(up) < 0.65
                        || right_hit.normal.dot(up) < 0.65
                    {
                        return Err(RebuildRejection::NoGround);
                    }
                    floor = left.point.midpoint(right_hit.point);
                }
                let center = floor + normal * (radius + 0.6);
                if !self.world.physics.surface_vehicle_space_is_clear(
                    &replacement,
                    center,
                    radius,
                    self.pilots[player].vehicle.0,
                    pilot_physics_id(self.pilots[player].owner),
                ) {
                    return Err(RebuildRejection::HullObstructed);
                }
                // Same-tick transfers/rebuilds are not in the completed index.
                // Only this seat's replaced pod disappears in the proposed world.
                let standing = point + (point - frame.position).normalized() * spec.half_height();
                let other_seat_occupied = self.pilots.iter().enumerate().any(|(seat, pilot)| {
                    if seat == player {
                        return false;
                    }
                    let actor_near = pilot.snapshot(&self.world.physics).is_some_and(|s| {
                        s.motion.position.distance_to(center) < radius + spec.half_height()
                    });
                    let vehicle_near = self
                        .world
                        .physics
                        .world
                        .motion(self.world.physics.ship_body(pilot.vehicle.0))
                        .is_some_and(|m| {
                            m.position.distance_to(center)
                                < radius
                                    + physics::SpacewarsPhysics::surface_vehicle_clearance_radius(
                                        &self.world.ships[pilot.vehicle.0],
                                    )
                        });
                    actor_near || vehicle_near
                });
                if standing.distance_to(center) < radius + spec.half_height() || other_seat_occupied
                {
                    return Err(RebuildRejection::HullObstructed);
                }
                let Some(map) = map else {
                    return Ok((RebuildPose { center, normal }, None, None));
                };
                let settled = floor + normal * 5.45;
                let angle = rotation_for_direction(normal);
                let hatch = self
                    .material_access_at(planet, ShipForm::Ship, settled, angle)
                    .ok_or(RebuildRejection::NoHatchFooting)?
                    .point;
                let avoiding = map.avoiding(self.pilots[player].gravity.length(), |position| {
                    let world = world_point(position);
                    let axis = rotation_for_direction(position.rotate_radians(frame.angle));
                    // Check the resting and raised spawn poses. The route is a
                    // forecast; actual landing and transfer gates still apply.
                    preview_clear(world, axis, settled, angle)
                        && preview_clear(world, axis, center, angle)
                });
                let route = avoiding.route_to_hatch(local(point), local(hatch));
                Ok((
                    RebuildPose { center, normal },
                    Some(local(hatch)),
                    Some(route.diagnostics),
                ))
            };
            match evaluate() {
                Err(reason) => attempt.rejection = Some(reason),
                Ok((pose, hatch, route)) => {
                    attempt.center = Some(local(pose.center));
                    attempt.hatch = hatch;
                    attempt.route = route;
                    if attempt.route.as_ref().is_some_and(|r| r.failure.is_some()) {
                        attempt.rejection = Some(RebuildRejection::NoHatchRoute);
                    } else if attempt
                        .route
                        .as_ref()
                        .is_some_and(|r| r.length > MAX_REBUILD_WALK)
                    {
                        attempt.rejection = Some(RebuildRejection::HatchRouteTooLong);
                    } else {
                        let cost = attempt
                            .route
                            .as_ref()
                            .map_or(0.0, |r| r.length + r.jumps as f32 * 2.0);
                        if cost < best_cost {
                            best_cost = cost;
                            best = Some(pose);
                            report.selected_offset = Some(offset);
                        }
                    }
                }
            }
            report.attempts.push(attempt);
        }
        (best, report)
    }

    pub(super) fn rebuild_relocation_survey(
        &self,
        player: usize,
    ) -> Option<RebuildRelocationSurvey> {
        // Alternate with the full ground survey. A relocation only needs this
        // local patch, keeping the two expensive observations off the same frame.
        if !(self.world.tick + player as u64 * 15 + 15)
            .is_multiple_of(ground_navigation::GROUND_REFRESH_TICKS)
            || self.world.physics.material_queries_dirty
        {
            return None;
        }
        let recovery = self.pilots[player].recovery.as_ref()?.observation();
        if !matches!(
            recovery.status,
            SurfaceRecoveryStatus::ClearanceBlocked | SurfaceRecoveryStatus::HatchBlocked
        ) {
            return None;
        }
        let actor = self.spaceling_snapshot(player)?;
        let planet = self.motion_planet_index(player);
        let map = self.local_ground_map(player, planet, actor.motion.position, false)?;
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let foot = (actor.motion.position - actor.up * Self::spec().half_height() - frame.position)
            .rotate_radians(-frame.angle);
        let base = self.rebuild_ground_map(player, map.planet, actor.motion.position)?;
        let bearing = (rotation_for_direction(foot) * GROUND_SAMPLES as f32 / std::f32::consts::TAU)
            .round() as i32;
        let mut survey = RebuildRelocationSurvey {
            tick: self.world.tick,
            checked: 0,
            attempts: Vec::new(),
            site: None,
        };
        for offset in [-8, 8, -16, 16, -24, 24, -32, 32] {
            let id = (bearing + offset).rem_euclid(GROUND_SAMPLES as i32) as u16;
            survey.attempts.push(RebuildRelocationAttempt {
                bearing: id,
                route: None,
                placement: None,
            });
            let Some(node) = map.nodes.iter().find(|n| n.id == id) else {
                continue;
            };
            let route = map.route(foot, node.position, 0.8);
            survey.attempts.last_mut().unwrap().route = Some(route.diagnostics.clone());
            if route.diagnostics.failure.is_some() || route.diagnostics.length > MAX_REBUILD_WALK {
                continue;
            }
            survey.checked += 1;
            let (pose, report) = self.find_rebuild_placement(
                player,
                map.planet,
                frame.position + node.position.rotate_radians(frame.angle),
                node.normal.rotate_radians(frame.angle),
                Some(&base),
            );
            survey.attempts.last_mut().unwrap().placement = Some(report.clone());
            if pose.is_some() {
                let attempt = report
                    .attempts
                    .iter()
                    .find(|a| Some(a.offset) == report.selected_offset)
                    .unwrap();
                survey.site = Some(RebuildStandingSite {
                    planet: map.planet,
                    revision: map.revision,
                    position: node.position,
                    walk_length: route.diagnostics.length,
                    hatch_walk_length: attempt.route.as_ref().unwrap().length,
                });
                break;
            }
        }
        Some(survey)
    }
}
