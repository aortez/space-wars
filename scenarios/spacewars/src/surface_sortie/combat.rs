//! Opt-in weapons and read-only tactical sensors for the material flight scale.
//! Shots, occlusion, damage and ship loss use the shared Spacewars step.
use super::*;
use pilot::{LandingSiteId, MaterialFlightStart, PilotMotion};
use recovery_sensors::RecoveryTaskObservationV1;

const WEAPON_ACTION: u32 = 0x5355_0005;
pub const CANNON_RECOIL: f32 = 8.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SurfaceWeaponAction {
    pub laser: bool,
    pub cannon: bool,
}
impl SurfaceWeaponAction {
    pub fn encode(self, owner: PlayerId) -> Action {
        Action::scenario(
            WEAPON_ACTION,
            vec![owner.index() as u8, self.laser as u8, self.cannon as u8],
        )
    }
    pub fn decode(action: &Action) -> Option<(PlayerId, Self)> {
        let Action::Scenario {
            kind: WEAPON_ACTION,
            payload,
        } = action
        else {
            return None;
        };
        if payload.len() != 3 || payload[1..].iter().any(|v| *v > 1) {
            return None;
        }
        Some((
            PlayerId::from_index(payload[0] as usize)?,
            Self {
                laser: payload[1] != 0,
                cannon: payload[2] != 0,
            },
        ))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct CombatTelemetry {
    pub shells_fired: u64,
    pub cannon_hits: u64,
    pub laser_hit_ticks: u64,
    pub last_hit_taken_tick: Option<u64>,
    pub last_hit_source: Option<&'static str>,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct CombatSeat {
    pub input: SurfaceWeaponAction,
    pub telemetry: CombatTelemetry,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CombatTarget {
    pub owner: PlayerId,
    pub motion: PilotMotion,
    pub health: f32,
    /// First-solid query to the target center, excluding the observing ship.
    pub visible: bool,
    pub ground_occluded: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CombatObservationV2 {
    pub version: u32,
    pub recovery: RecoveryTaskObservationV1,
    /// Only a living, occupied full ship is an aerial combat target.
    pub target: Option<CombatTarget>,
    pub laser_available: bool,
    pub cannon_ready: bool,
    pub supply: Option<weapons::WeaponSupplyObservation>,
    pub weapons: CombatTelemetry,
}

impl SurfaceSortieScenario {
    pub fn init_material_combat(seed: u64) -> SurfaceSortieState {
        Self::init_material_combat_flight(
            seed,
            &[
                (
                    PlayerId::PLAYER_1,
                    MaterialFlightStart {
                        bearing: -0.5,
                        altitude: 90.0,
                        radial_speed: 0.0,
                        lateral_speed: 0.0,
                        heading_offset: 0.0,
                    },
                ),
                (
                    PlayerId::PLAYER_2,
                    MaterialFlightStart {
                        bearing: 0.5,
                        altitude: 90.0,
                        radial_speed: 0.0,
                        lateral_speed: 0.0,
                        heading_offset: 0.0,
                    },
                ),
            ],
        )
    }
    /// Initial placements only; no scripted damage, ownership or recovery.
    pub fn init_material_combat_flight(
        seed: u64,
        starts: &[(PlayerId, MaterialFlightStart)],
    ) -> SurfaceSortieState {
        let mut state = Self::init_material_flight(seed, 2, starts);
        for pilot in &mut state.pilots {
            pilot.combat = Some(CombatSeat::default());
            state.world.ships[pilot.vehicle.0].enable_weapon_supply();
        }
        state
    }
}
impl SurfaceSortieState {
    pub fn combat_enabled(&self) -> bool {
        self.pilots.first().is_some_and(|p| p.combat.is_some())
    }
    pub fn combat_telemetry(&self, player: usize) -> CombatTelemetry {
        self.pilots[player]
            .combat
            .as_ref()
            .map_or(CombatTelemetry::default(), |c| c.telemetry)
    }
    pub(super) fn read_weapon_actions(&mut self, actions: &[Action]) {
        for (owner, action) in actions.iter().filter_map(SurfaceWeaponAction::decode) {
            if let Some(combat) = self
                .pilots
                .get_mut(owner.index())
                .and_then(|p| p.combat.as_mut())
            {
                combat.input = action;
            }
        }
    }
    pub fn combat_observation(
        &self,
        player: usize,
        site: Option<LandingSiteId>,
    ) -> CombatObservationV2 {
        let recovery = self.recovery_task_observation(player, site);
        let p = &recovery.flight.pilot;
        let ship = &self.world.ships[p.vehicle.0];
        let target = self.pilots.iter().enumerate().find_map(|(seat, other)| {
            let target = &self.world.ships[other.vehicle.0];
            if seat == player
                || other.body.is_some()
                || target.dead
                || target.form != ShipForm::Ship
            {
                return None;
            }
            let motion = self
                .world
                .physics
                .world
                .motion(self.world.physics.ship_body(other.vehicle.0))?;
            let delta = motion.position - p.ship.position;
            let trace = p
                .queries_ready
                .then(|| {
                    self.world.physics.cast_laser(
                        p.vehicle.0,
                        p.ship.position,
                        delta.normalized(),
                        delta.length(),
                    )
                })
                .flatten();
            let visible = trace
                .is_some_and(|hit| hit.target == Some(MechanicalEntity::Ship(other.vehicle.0)));
            let ground_occluded = trace.is_some_and(|hit| {
                matches!(
                    hit.target,
                    Some(MechanicalEntity::Body(_) | MechanicalEntity::TerrainFragment(_))
                )
            });
            Some(CombatTarget {
                owner: other.owner,
                health: target.life,
                visible,
                ground_occluded,
                motion: PilotMotion {
                    position: motion.position,
                    velocity: motion.linear_velocity,
                    angle: motion.angle,
                    spin: motion.angular_velocity,
                },
            })
        });
        let available = self.combat_enabled()
            && p.controls_armed
            && p.ship_available
            && p.ship_form == ShipForm::Ship
            && matches!(p.location, PilotLocation::Aboard(_));
        CombatObservationV2 {
            version: 2,
            target,
            laser_available: available
                && ship
                    .armament
                    .as_ref()
                    .is_some_and(|a| a.laser_ready(ship.delta_time)),
            cannon_ready: available
                && ship.cannon_cooldown_remaining <= 0.0
                && ship.armament.as_ref().is_some_and(|a| a.cannon_ready()),
            supply: ship.weapon_supply(),
            weapons: self.combat_telemetry(player),
            recovery,
        }
    }
}

/// Read contact indices before debris cleanup compacts the shared collection.
pub(crate) fn record_hits(world: &SpacewarsState, pilots: &mut [SurfacePilot]) {
    for hit in &world.laser_hits {
        if let LaserTarget::Ship(target) = hit.target {
            if let Some(c) = pilots
                .iter_mut()
                .find(|p| p.vehicle.0 == hit.shooter)
                .and_then(|p| p.combat.as_mut())
            {
                c.telemetry.laser_hit_ticks += 1;
            }
            record_taken(pilots, target, world.tick + 1, "laser");
        }
    }
    for hit in &world.ship_debris_collisions {
        let Some(debris) = world.debris.get(hit.debris) else {
            continue;
        };
        if debris.kind == DebrisKind::Shell {
            if let Some(c) = pilots
                .iter_mut()
                .find(|p| Some(p.owner.index()) == debris.owner_id)
                .and_then(|p| p.combat.as_mut())
            {
                c.telemetry.cannon_hits += 1;
            }
            record_taken(pilots, hit.ship, world.tick + 1, "cannon");
        }
    }
}
fn record_taken(pilots: &mut [SurfacePilot], target: usize, tick: u64, source: &'static str) {
    if let Some(c) = pilots
        .iter_mut()
        .find(|p| p.vehicle.0 == target)
        .and_then(|p| p.combat.as_mut())
    {
        c.telemetry.last_hit_taken_tick = Some(tick);
        c.telemetry.last_hit_source = Some(source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: Duration = Duration::from_nanos(16_666_667);
    #[test]
    fn surface_pod_keeps_resolved_motion_without_legacy_damage_kick() {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        let ship = &mut state.world.ships[0];
        ship.velocity = Vec2::new(24.0, -18.0);
        let origin = ship.position + physics::ship_pivot(ship.form);
        ship.translate_life_with_impulse(-1000.0, Vec2::new(1000.0, -1000.0));
        state.pilots[0].vehicle_destroyed(ship);
        assert_eq!(ship.form, ShipForm::EscapePod);
        assert_eq!(ship.velocity, Vec2::new(24.0, -18.0));
        assert_eq!(ship.position + physics::ship_pivot(ship.form), origin);
        assert!(ship.weapon_supply().is_none());
    }
    fn face_target(state: &mut SurfaceSortieState) {
        let direction = (state.pilot_observation(1, None).ship.position
            - state.pilot_observation(0, None).ship.position)
            .normalized();
        let ship = &mut state.world.ships[0];
        ship.direction = direction;
        ship.rotation_radians = rotation_for_direction(direction);
        let body = state.world.physics.ship_body(0);
        state.world.physics.world.set_pose(
            body,
            ship.position + SHIP_PIVOT,
            ship.rotation_radians,
            true,
        );
    }
    #[test]
    fn weapons_use_shared_hits_recoil_and_transfer_release_gate() {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], DT);
        face_target(&mut state);
        let before = state.world.ships[1].life;
        let laser = SurfaceWeaponAction {
            laser: true,
            cannon: false,
        }
        .encode(PlayerId::PLAYER_1);
        for _ in 0..20 {
            SurfaceSortieScenario::step(&mut state, std::slice::from_ref(&laser), DT);
        }
        assert!(state.world.ships[1].life < before);
        assert!(state.combat_telemetry(0).laser_hit_ticks > 0);
        assert_eq!(state.damage_observation(1).last_source, Some("laser"));
        let before = state.world.ships[0].velocity;
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceWeaponAction {
                laser: false,
                cannon: true,
            }
            .encode(PlayerId::PLAYER_1)],
            DT,
        );
        assert_eq!(state.combat_telemetry(0).shells_fired, 1);
        let recoil = state.world.ships[0].velocity.distance_to(before);
        assert!((7.0..9.0).contains(&recoil), "{recoil}");
        state.world.ships[0].translate_life(-1000.0);
        SurfaceSortieScenario::step(&mut state, &[], DT);
        assert_eq!(state.world.ships[0].form, ShipForm::EscapePod);
        assert!(state.combat_observation(0, None).supply.is_none());
        assert!(!state.pilots[0].controls_armed);
        SurfaceSortieScenario::step(&mut state, &[], DT);
        assert!(
            !state.pilots[0].controls_armed,
            "held weapon must not rearm the pod"
        );
        assert!(state.world.ships[0].laser_beam.is_none());
        assert_eq!(state.combat_telemetry(0).shells_fired, 1);
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceWeaponAction::default().encode(PlayerId::PLAYER_1)],
            DT,
        );
        assert!(state.pilots[0].controls_armed);
        assert!(state.combat_observation(1, None).target.is_none());
    }
    #[test]
    fn material_occludes_lasers_and_cannon_uses_queued_terrain_damage() {
        let start = |bearing| MaterialFlightStart {
            bearing,
            altitude: 30.0,
            radial_speed: 0.0,
            lateral_speed: 0.0,
            heading_offset: 0.0,
        };
        let mut state = SurfaceSortieScenario::init_material_combat_flight(
            7,
            &[
                (PlayerId::PLAYER_1, start(0.0)),
                (PlayerId::PLAYER_2, start(std::f32::consts::PI)),
            ],
        );
        SurfaceSortieScenario::step(&mut state, &[], DT);
        face_target(&mut state);
        assert!(!state.combat_observation(0, None).target.unwrap().visible);
        let before = state.world.ships[1].life;
        for _ in 0..10 {
            SurfaceSortieScenario::step(
                &mut state,
                &[SurfaceWeaponAction {
                    laser: true,
                    cannon: false,
                }
                .encode(PlayerId::PLAYER_1)],
                DT,
            );
        }
        assert_eq!(state.world.ships[1].life, before);
        assert!(
            state
                .world
                .laser_hits
                .iter()
                .any(|h| matches!(h.target, LaserTarget::Body(BodyId::Planet(0))))
        );
        let initial = state.terrain_diagnostics().occupied_cells;
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceWeaponAction {
                laser: false,
                cannon: true,
            }
            .encode(PlayerId::PLAYER_1)],
            DT,
        );
        for _ in 0..60 {
            SurfaceSortieScenario::step(
                &mut state,
                &[SurfaceWeaponAction::default().encode(PlayerId::PLAYER_1)],
                DT,
            );
        }
        assert!(state.terrain_diagnostics().occupied_cells < initial);
        assert!(state.terrain_diagnostics().issues.is_empty());
    }
    #[test]
    fn combat_sensors_and_actions_are_opt_in_and_read_only() {
        let mut state = SurfaceSortieScenario::init_material(42, 2);
        SurfaceSortieScenario::step(&mut state, &[], DT);
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceWeaponAction {
                laser: true,
                cannon: true,
            }
            .encode(PlayerId::PLAYER_1)],
            DT,
        );
        assert!(!state.combat_enabled());
        assert!(state.combat_observation(0, None).supply.is_none());
        assert!(
            state
                .world
                .debris
                .iter()
                .all(|d| d.kind != DebrisKind::Shell)
        );
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        let tick = state.world.tick;
        let bodies = state.world.physics.world.body_count();
        assert!(!state.combat_observation(0, None).target.unwrap().visible);
        assert_eq!(state.world.tick, tick);
        assert_eq!(state.world.physics.world.body_count(), bodies);
        state.world.physics.material_queries_dirty = false;
        let action = SurfaceWeaponAction {
            laser: true,
            cannon: false,
        };
        assert_eq!(
            SurfaceWeaponAction::decode(&action.encode(PlayerId::PLAYER_2)),
            Some((PlayerId::PLAYER_2, action))
        );
        assert!(
            SurfaceWeaponAction::decode(&Action::scenario(WEAPON_ACTION, vec![0, 2, 0])).is_none()
        );
    }

    #[test]
    fn held_weapons_share_supply_and_pause_clone_and_replay_preserve_reload() {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], DT);
        let action = SurfaceWeaponAction {
            laser: true,
            cannon: true,
        }
        .encode(PlayerId::PLAYER_1);
        SurfaceSortieScenario::step(&mut state, std::slice::from_ref(&action), DT);
        assert_eq!(state.combat_telemetry(0).shells_fired, 1);
        assert!(state.world.ships[0].laser_beam.is_none());
        assert_eq!(
            state
                .combat_observation(0, None)
                .supply
                .unwrap()
                .energy_percent,
            100.0
        );
        SurfaceSortieScenario::step(&mut state, std::slice::from_ref(&action), DT);
        assert!(state.world.ships[0].laser_beam.is_some());
        let o = state.combat_observation(0, None);
        let supply = o.supply.unwrap();
        assert_eq!(supply.rounds_loaded, 1);
        assert!((supply.energy_percent - 74.8).abs() < 0.001);
        assert_eq!(supply.reload_progress, Some(0.0));
        let opponent = state.combat_observation(1, None).supply.unwrap();
        assert_eq!(opponent.rounds_loaded, 2);
        assert_eq!(opponent.energy_percent, 100.0);
        SurfaceSortieScenario::step(&mut state, &[], Duration::ZERO);
        assert_eq!(state.combat_observation(0, None), o);
        let mut other = state.clone();
        for _ in 0..180 {
            SurfaceSortieScenario::step(&mut state, std::slice::from_ref(&action), DT);
            SurfaceSortieScenario::step(&mut other, std::slice::from_ref(&action), DT);
            assert_eq!(
                state.combat_observation(0, None),
                other.combat_observation(0, None)
            );
        }
        assert!(state.combat_telemetry(0).shells_fired <= 3);
    }
}
