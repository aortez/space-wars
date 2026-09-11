//! Expedition-only vehicle loss and replacement. No actor duplication, old
//! docking services, invincibility changes, or additional physics steps.

use super::rebuild_placement::RebuildPlacementReport;
use super::*;

const SCUTTLE_TIME: Duration = Duration::from_secs(3);
const REBUILD_TIME: Duration = Duration::from_secs(8);
const PLACEMENT_RETRY: Duration = Duration::from_millis(500);
const REBUILD_MAX_SPEED: f32 = 1.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceRecoveryStatus {
    #[default]
    ShipAvailable,
    Scuttling,
    LandPod,
    NeedSupport,
    NeedBalance,
    NeedSettle,
    NeedOwnedPlanet,
    Rebuilding,
    ClearanceBlocked,
    HatchBlocked,
}

impl SurfaceRecoveryStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::ShipAvailable => "ship available",
            Self::Scuttling => "release to cancel scuttle",
            Self::LandPod => "land pod, then B to exit",
            Self::NeedSupport => "stand on a planet to rebuild",
            Self::NeedBalance => "recover your balance to rebuild",
            Self::NeedSettle => "stand still to rebuild",
            Self::NeedOwnedPlanet => "claim this planet to rebuild",
            Self::Rebuilding => "rebuilding; stand still",
            Self::ClearanceBlocked => "build space blocked; move along surface",
            Self::HatchBlocked => "hatch access blocked; move along surface",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SurfaceRecoveryObservation {
    pub status: SurfaceRecoveryStatus,
    pub planet: Option<usize>,
    pub rebuild_progress: f32,
    pub rebuild_required_seconds: f32,
    pub scuttle_progress: f32,
    pub scuttle_required_seconds: f32,
    pub ships_lost: u64,
    pub pod_ejections: u64,
    pub rebuilds: u64,
    pub rebuild_interruptions: u64,
    pub blocked_attempts: u64,
    pub placement: Option<RebuildPlacementReport>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct SurfaceRecovery {
    status: SurfaceRecoveryStatus,
    planet: Option<usize>,
    elapsed: Duration,
    retry: Duration,
    scuttle: Duration,
    ships_lost: u64,
    pod_ejections: u64,
    rebuilds: u64,
    rebuild_interruptions: u64,
    blocked_attempts: u64,
    placement: Option<RebuildPlacementReport>,
}

impl SurfaceRecovery {
    pub(super) fn observation(&self) -> SurfaceRecoveryObservation {
        SurfaceRecoveryObservation {
            status: if self.scuttle.is_zero() {
                self.status
            } else {
                SurfaceRecoveryStatus::Scuttling
            },
            planet: self.planet,
            rebuild_progress: self.elapsed.as_secs_f32() / REBUILD_TIME.as_secs_f32(),
            rebuild_required_seconds: REBUILD_TIME.as_secs_f32(),
            scuttle_progress: self.scuttle.as_secs_f32() / SCUTTLE_TIME.as_secs_f32(),
            scuttle_required_seconds: SCUTTLE_TIME.as_secs_f32(),
            ships_lost: self.ships_lost,
            pod_ejections: self.pod_ejections,
            rebuilds: self.rebuilds,
            rebuild_interruptions: self.rebuild_interruptions,
            blocked_attempts: self.blocked_attempts,
            placement: self.placement.clone(),
        }
    }

    fn reset_rebuild(&mut self) {
        self.rebuild_interruptions += u64::from(!self.elapsed.is_zero());
        self.planet = None;
        self.elapsed = Duration::ZERO;
        self.retry = Duration::ZERO;
        self.placement = None;
    }
}

impl SurfacePilot {
    pub(crate) fn recovery_enabled(&self) -> bool {
        self.recovery.is_some()
    }

    pub(crate) fn vehicle_destroyed(&mut self, ship: &mut ShipState, tick: u64) {
        let recovery = self.recovery.as_mut().expect("Expedition recovery");
        recovery.ships_lost += 1;
        recovery.scuttle = Duration::ZERO;
        recovery.reset_rebuild();
        self.controls_armed = false;
        self.control = SpacelingControl::default();
        self.landing = LandingTelemetry::default();
        if self.body.is_none() {
            // Keep the completed body's origin and physical spin;
            // the legacy mesh pivots/control omega scales differ between forms.
            let origin = ship.position + physics::ship_pivot(ship.form);
            let velocity = ship.velocity;
            let spin = physics::physical_angular_velocity(ship);
            ship.change_to_escape_pod();
            if let Some(vitals) = &mut self.vitals {
                vitals.protect_ejection(tick);
            }
            ship.position = origin - physics::ship_pivot(ship.form);
            // Combat uses Rapier's resolved velocity: the contact impulse has
            // already been applied. Keep the older asteroid/recovery fixtures'
            // ejection behavior until their landing policy is adapted as well.
            if self.combat.is_some() {
                ship.velocity = velocity;
            }
            ship.omega = physics::control_angular_velocity(ship, spin);
            recovery.pod_ejections += 1;
            recovery.status = SurfaceRecoveryStatus::LandPod;
        } else {
            // The already external creature is the survivor. Keep the vehicle
            // slot dead, with one breakup, and remove its physical assembly.
            ship.life = 0.0;
            recovery.status = SurfaceRecoveryStatus::NeedSupport;
        }
        ship.set_thrust(0.0);
        ship.set_turn(0.0);
        ship.set_brake(0.0);
        ship.set_laser(false);
        ship.set_cannon(false);
        ship.laser_beam = None;
        ship.exhaust_trails.clear();
    }
}

impl SurfaceSortieState {
    pub(super) fn enable_recovery(&mut self) {
        self.world.physics.enable_surface_recovery();
        for pilot in &mut self.pilots {
            pilot.recovery = Some(SurfaceRecovery::default());
        }
    }

    pub(super) fn reconcile_recovery_vehicles(&mut self) {
        for pilot in &self.pilots {
            let index = pilot.vehicle.0;
            let ship = &self.world.ships[index];
            if pilot.recovery.is_some() && self.world.physics.surface_vehicle_changed(index, ship) {
                let changed = self.world.physics.reconcile_surface_vehicle(index, ship);
                self.world.last_step_metrics.added += changed.added;
                self.world.last_step_metrics.removed += changed.removed;
            }
        }
    }

    /// Deliberate lab drill, using the existing minimal gamepad controls.
    /// The chord consumes thrust/jump/transfer while held, and must be released
    /// after destruction before any input can drive the surviving pod/pilot.
    pub(super) fn update_scuttle_input(
        &mut self,
        player: usize,
        input: SurfaceSortieAction,
        dt: Duration,
    ) -> bool {
        let available = self.vehicle_available(player);
        let Some(recovery) = self.pilots[player].recovery.as_mut() else {
            return false;
        };
        let chord = available
            && input.primary_held
            && input.interact_held
            && input.brake_held
            && input.horizontal.abs() < 0.01;
        if !chord {
            recovery.scuttle = Duration::ZERO;
            return false;
        }
        recovery.scuttle = recovery.scuttle.saturating_add(dt).min(SCUTTLE_TIME);
        if recovery.scuttle == SCUTTLE_TIME {
            let ship = &mut self.world.ships[self.pilots[player].vehicle.0];
            ship.translate_life_with_impulse(-ship.life_max, Vec2::ZERO);
        }
        true
    }

    fn rebuild_candidate(
        &self,
        player: usize,
    ) -> Result<(usize, Vec2, Vec2), SurfaceRecoveryStatus> {
        let snapshot = self
            .spaceling_snapshot(player)
            .ok_or(SurfaceRecoveryStatus::LandPod)?;
        let support = snapshot.support.ok_or(SurfaceRecoveryStatus::NeedSupport)?;
        let planet = physics::planet_surface_support_index(support.collider)
            .filter(|&index| index < self.world.planets.len())
            .ok_or(SurfaceRecoveryStatus::NeedSupport)?;
        if snapshot.balance != SpacelingBalance::Balanced {
            return Err(SurfaceRecoveryStatus::NeedBalance);
        }
        let velocity = self
            .world
            .physics
            .world
            .velocity_at_point(
                self.pilots[player].body.as_ref().unwrap().body(),
                support.position,
            )
            .unwrap();
        if (velocity - support.velocity).length() > REBUILD_MAX_SPEED {
            return Err(SurfaceRecoveryStatus::NeedSettle);
        }
        if self.world.planets[planet].owner_id != Some(self.pilots[player].owner.index()) {
            return Err(SurfaceRecoveryStatus::NeedOwnedPlanet);
        }
        Ok((planet, support.position, support.normal))
    }

    pub(super) fn update_recovery(&mut self, dt: Duration) {
        for player in 0..self.player_count() {
            if self.pilots[player].recovery.is_none() {
                continue;
            }
            let candidate = if self.vehicle_available(player) {
                Err(SurfaceRecoveryStatus::ShipAvailable)
            } else {
                self.rebuild_candidate(player)
            };
            let recovery = self.pilots[player].recovery.as_mut().unwrap();
            let (planet, point, normal) = match candidate {
                Ok(candidate) => candidate,
                Err(status) => {
                    recovery.reset_rebuild();
                    recovery.status = status;
                    continue;
                }
            };
            recovery.status = SurfaceRecoveryStatus::Rebuilding;
            if recovery.planet != Some(planet) {
                recovery.reset_rebuild();
                recovery.planet = Some(planet);
                // Ownership/support must exist for a fresh full build interval.
                continue;
            }
            recovery.elapsed = recovery.elapsed.saturating_add(dt).min(REBUILD_TIME);
            if recovery.elapsed < REBUILD_TIME {
                continue;
            }
            recovery.retry = recovery.retry.saturating_sub(dt);
            if !recovery.retry.is_zero() {
                recovery.status = recovery.placement.as_ref().map_or(
                    SurfaceRecoveryStatus::ClearanceBlocked,
                    RebuildPlacementReport::blocked_status,
                );
                continue;
            }
            if !self.try_rebuild_vehicle(player, planet, point, normal) {
                let recovery = self.pilots[player].recovery.as_mut().unwrap();
                recovery.status = recovery.placement.as_ref().map_or(
                    SurfaceRecoveryStatus::ClearanceBlocked,
                    RebuildPlacementReport::blocked_status,
                );
                recovery.blocked_attempts += 1;
                recovery.retry = PLACEMENT_RETRY;
            }
        }
    }

    fn try_rebuild_vehicle(&mut self, player: usize, planet: usize, point: Vec2, up: Vec2) -> bool {
        let index = self.pilots[player].vehicle.0;
        let map = self.rebuild_ground_map(player, planet, point);
        let (pose, report) = self.find_rebuild_placement(player, planet, point, up, map.as_ref());
        self.pilots[player].recovery.as_mut().unwrap().placement = Some(report);
        let Some(pose) = pose else {
            return false;
        };
        let center = pose.center;
        let mut replacement = self.replacement_ship(player);
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        replacement.position = center - SHIP_PIVOT;
        replacement.rotation_radians = rotation_for_direction(pose.normal);
        replacement.direction = pose.normal;
        replacement.velocity = motion::point_velocity(frame, center);
        replacement.omega = physics::control_angular_velocity(&replacement, frame.angular_velocity);
        self.world.ships[index] = replacement;
        self.reconcile_recovery_vehicles();
        // Rapier stores COM velocity, while the surface frame is evaluated
        // at the assembly origin. Account for the off-center hull mass.
        let body = self.world.physics.ship_body(index);
        let origin_velocity = self
            .world
            .physics
            .world
            .velocity_at_point(body, center)
            .unwrap();
        self.world.physics.world.apply_velocity_delta(
            body,
            motion::point_velocity(frame, center) - origin_velocity,
            true,
        );
        self.world.ships[index].velocity = self
            .world
            .physics
            .world
            .motion(body)
            .unwrap()
            .linear_velocity;
        let pilot = &mut self.pilots[player];
        pilot.planet = planet;
        pilot.landing = LandingTelemetry::default();
        pilot.controls_armed = false;
        let recovery = pilot.recovery.as_mut().unwrap();
        recovery.planet = None;
        recovery.elapsed = Duration::ZERO;
        recovery.retry = Duration::ZERO;
        recovery.rebuilds += 1;
        recovery.status = SurfaceRecoveryStatus::ShipAvailable;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocation_surveys_are_read_only_local_and_between_full_surveys() {
        let mut state = SurfaceSortieScenario::init_material(42, 1);
        let dt = Duration::from_nanos(16_666_667);
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1)],
            dt,
        );
        for _ in 0..240 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        state.world.ships[0].translate_life(-state.world.ships[0].life_max);
        SurfaceSortieScenario::step(&mut state, &[], dt);
        let mut ground_ticks = Vec::new();
        let mut rebuild_ticks = Vec::new();
        for _ in 0..60 {
            // Isolate the blocked-site sensor contract from the build timer.
            state.pilots[0].recovery.as_mut().unwrap().status = SurfaceRecoveryStatus::HatchBlocked;
            let before = state.world.physics.snapshot_bytes();
            let tick = state.world.tick;
            let o = state.recovery_task_observation(0, None);
            assert_eq!(state.world.physics.snapshot_bytes(), before);
            assert_eq!(state.world.tick, tick);
            assert!(o.ground.is_none() || o.rebuild.is_none());
            if o.ground.is_some() {
                ground_ticks.push(tick);
            }
            if let Some(survey) = o.rebuild {
                assert!(survey.checked <= 8 && survey.attempts.len() <= 8);
                rebuild_ticks.push(tick);
            }
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        assert_eq!(ground_ticks.len(), 2);
        assert_eq!(rebuild_ticks.len(), 2);
        assert!(ground_ticks.iter().all(|tick| tick % 30 == 0));
        assert!(rebuild_ticks.iter().all(|tick| tick % 30 == 15));
        state.world.physics.material_queries_dirty = true;
        assert!(state.rebuild_relocation_survey(0).is_none());
        assert!(state.world.physics.material_queries_dirty);
    }
}
