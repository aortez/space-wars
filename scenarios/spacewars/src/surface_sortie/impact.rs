//! Controlled material-lab impacts. Only asteroid spawning is scripted; contact,
//! damage, breakup and pod ejection all use the shared Spacewars pipeline.
use super::*;
use engine_terrain::{Brush, EditMode, TerrainEdit};
const IMPACT_ACTION: u32 = 0x5355_0004;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImpactKind {
    Light,
    #[default]
    Heavy,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SurfaceImpactAction {
    pub held: bool,
    pub kind: ImpactKind,
    pub oblique: bool,
}

/// Bounded terrain edits for the recovery acceptance runner, applied through
/// the ordinary queued-edit boundary. Never called by a pilot policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryDisruption {
    FlagFooting,
    SpacelingSupport,
    LandingSite(pilot::LandingSiteId),
}

/// Incoming hazards for headless recovery trials, including follow-up hits on
/// pods. These spawn ordinary debris; they do not change the target's motion,
/// health or form. Interactive impact buttons keep their existing gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryHazard {
    LightAsteroid,
    HeavyAsteroid,
    Missile,
}
impl SurfaceImpactAction {
    pub fn encode(self, owner: PlayerId) -> Action {
        Action::scenario(
            IMPACT_ACTION,
            vec![
                owner.index() as u8,
                self.held as u8,
                (self.kind == ImpactKind::Heavy) as u8,
                self.oblique as u8,
            ],
        )
    }
    pub fn decode(action: &Action) -> Option<(PlayerId, Self)> {
        let Action::Scenario {
            kind: IMPACT_ACTION,
            payload,
        } = action
        else {
            return None;
        };
        if payload.len() != 4 || payload[1..].iter().any(|b| *b > 1) {
            return None;
        }
        Some((
            PlayerId::from_index(payload[0] as usize)?,
            Self {
                held: payload[1] != 0,
                kind: if payload[2] == 0 {
                    ImpactKind::Light
                } else {
                    ImpactKind::Heavy
                },
                oblique: payload[3] != 0,
            },
        ))
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct SurfaceDamageObservation {
    pub strikes: u64,
    pub hits: u64,
    pub last_strike_tick: Option<u64>,
    pub last_damage_tick: Option<u64>,
    pub last_damage_percent: f32,
    pub last_ship_lost: bool,
    pub last_source: Option<&'static str>,
    /// Physical debris contacts also count when a pod takes no health damage.
    pub debris_contacts: u64,
    pub last_contact_tick: Option<u64>,
    pub last_contact_source: Option<&'static str>,
    pub last_contact_spawn_tick: Option<u64>,
}
#[derive(Debug, Clone, Default)]
pub(super) struct SurfaceDamageState {
    pub(super) held: [bool; SPACEWARS_PLAYER_COUNT],
}

/// Read contact indices before finished projectiles are removed or fragments
/// change debris order. The outer sortie step runs after that cleanup.
pub(crate) fn record_contacts(world: &SpacewarsState, pilots: &mut [SurfacePilot]) {
    for hit in &world.ship_debris_collisions {
        let Some(debris) = world.debris.get(hit.debris) else {
            continue;
        };
        let Some(pilot) = pilots.iter_mut().find(|p| p.vehicle.0 == hit.ship) else {
            continue;
        };
        let d = &mut pilot.damage;
        d.debris_contacts += 1;
        d.last_contact_tick = Some(world.tick + 1);
        d.last_contact_source = Some(match debris.kind {
            DebrisKind::Shell => "cannon",
            DebrisKind::Asteroid => "asteroid",
            DebrisKind::Fragment => "fragment",
        });
        d.last_contact_spawn_tick = Some(debris.spawn_tick);
    }
}
impl SurfaceSortieState {
    pub fn spawn_recovery_hazard(
        &mut self,
        player: usize,
        hazard: RecoveryHazard,
        oblique: bool,
    ) -> bool {
        if player >= self.player_count() || !self.has_material_ground() {
            return false;
        }
        let vehicle = self.pilots[player].vehicle.0;
        if self.world.ships[vehicle].dead {
            return false;
        }
        let body = self.world.physics.ship_body(vehicle);
        let Some(m) = self.world.physics.world.motion(body) else {
            return false;
        };
        let up = (m.position - self.planet_motion(player).position).normalized();
        let pod = self.world.ships[vehicle].form == ShipForm::EscapePod;
        let axis = if pod { Vec2::new(up.y, -up.x) } else { up };
        let direction = -axis.rotate_radians(if oblique { 0.35 } else { 0.0 });
        // Nearby side impacts isolate pod response from the original radial
        // wreckage trail. Both colliders start apart; the hazard must travel/hit.
        let distance = if !pod {
            40.0
        } else if hazard == RecoveryHazard::LightAsteroid {
            6.0
        } else {
            12.0
        };
        let position = m.position - direction * distance;
        let speed = match hazard {
            RecoveryHazard::LightAsteroid => 25.0,
            RecoveryHazard::HeavyAsteroid => 160.0,
            RecoveryHazard::Missile => CANNON_SHELL_SPEED,
        };
        let velocity = m.linear_velocity + direction * speed;
        let mut debris = if hazard == RecoveryHazard::Missile {
            let mut shell = DebrisState::new_shell(
                (player + 1) % SPACEWARS_PLAYER_COUNT,
                self.world.tick,
                position,
                velocity,
                -direction.angle_radians(),
            );
            shell.rail_launched = true;
            shell
        } else {
            DebrisState::new(
                DebrisKind::Asteroid,
                position,
                velocity,
                2.0,
                1.0,
                Color::scale_255(200.0, 140.0, 70.0),
            )
        };
        debris.spawn_tick = self.world.tick;
        self.world.debris.push(debris);
        let d = &mut self.pilots[player].damage;
        d.strikes += 1;
        d.last_strike_tick = Some(self.world.tick);
        true
    }

    pub fn queue_recovery_disruption(
        &mut self,
        player: usize,
        disruption: RecoveryDisruption,
    ) -> bool {
        if player >= self.player_count()
            || !self.has_material_ground()
            || self.world.physics.material_queries_dirty
        {
            return false;
        }
        let planet = self.motion_planet_index(player);
        let point = match disruption {
            RecoveryDisruption::FlagFooting => self
                .claim_observation(planet, player)
                .and_then(|c| c.flag)
                .map(|f| f.position - f.normal * 0.08),
            RecoveryDisruption::SpacelingSupport
                if self.pilot_support_planet(player) == Some(planet) =>
            {
                self.spaceling_snapshot(player)
                    .and_then(|s| s.support)
                    .map(|s| s.position - s.normal * 0.08)
            }
            RecoveryDisruption::SpacelingSupport => None,
            RecoveryDisruption::LandingSite(id) if id.planet == planet => self
                .vehicle_landing_site(player, id, true)
                .map(|s| s.position - s.normal * 0.08),
            RecoveryDisruption::LandingSite(_) => None,
        };
        let Some(point) = point else {
            return false;
        };
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let Some(center) = self.world.terrain.planets[&planet]
            .field
            .local_to_cell((point - frame.position).rotate_radians(-frame.angle))
        else {
            return false;
        };
        self.world
            .queue_planet_edit(
                planet,
                TerrainEdit {
                    brush: Brush::Circle { center, radius: 2 },
                    mode: EditMode::Remove,
                },
            )
            .is_ok()
    }

    pub fn damage_observation(&self, player: usize) -> SurfaceDamageObservation {
        self.pilots[player].damage
    }
    pub(super) fn read_impact_actions(&mut self, actions: &[Action]) {
        if !self.has_material_ground() || self.combat_enabled() {
            return;
        }
        for (owner, action) in actions.iter().filter_map(SurfaceImpactAction::decode) {
            let seat = owner.index();
            if seat >= self.player_count() {
                continue;
            }
            let rising = action.held && !self.damage.held[seat];
            self.damage.held[seat] = action.held;
            if !rising || !self.pilots[seat].controls_armed {
                continue;
            }
            if self.pilots[seat]
                .damage
                .last_strike_tick
                .is_some_and(|t| self.world.tick.saturating_sub(t) < 180)
            {
                continue;
            }
            let ship = &self.world.ships[self.pilots[seat].vehicle.0];
            if ship.dead || ship.form != ShipForm::Ship {
                continue;
            }
            let body = self.world.physics.ship_body(self.pilots[seat].vehicle.0);
            let Some(m) = self.world.physics.world.motion(body) else {
                continue;
            };
            let up = (m.position - self.planet_motion(seat).position).normalized();
            let direction = -up.rotate_radians(if action.oblique { 0.35 } else { 0.0 });
            let speed = if action.kind == ImpactKind::Heavy {
                160.0
            } else {
                25.0
            };
            let velocity = self
                .world
                .physics
                .world
                .velocity_at_point(body, m.position)
                .unwrap();
            let mut rock = DebrisState::new(
                DebrisKind::Asteroid,
                m.position - direction * 40.0,
                velocity + direction * speed,
                2.0,
                1.0,
                Color::scale_255(200.0, 140.0, 70.0),
            );
            rock.spawn_tick = self.world.tick;
            self.world.debris.push(rock);
            self.pilots[seat].damage.strikes += 1;
            self.pilots[seat].damage.last_strike_tick = Some(self.world.tick);
        }
    }
    pub(super) fn damage_sample(&self) -> [(f32, f32, u64); SPACEWARS_PLAYER_COUNT] {
        std::array::from_fn(|seat| {
            self.pilots.get(seat).map_or((0.0, 1.0, 0), |p| {
                let ship = &self.world.ships[p.vehicle.0];
                (
                    ship.life,
                    ship.life_max,
                    p.recovery
                        .as_ref()
                        .map_or(0, |r| r.observation().ships_lost),
                )
            })
        })
    }
    pub(super) fn record_surface_damage(
        &mut self,
        before: [(f32, f32, u64); SPACEWARS_PLAYER_COUNT],
    ) {
        for (seat, (life, max_life, losses)) in
            before.into_iter().enumerate().take(self.player_count())
        {
            let pilot = &mut self.pilots[seat];
            let ship = &self.world.ships[pilot.vehicle.0];
            let lost = pilot
                .recovery
                .as_ref()
                .is_some_and(|r| r.observation().ships_lost > losses);
            let damage = if lost {
                life
            } else {
                (life - ship.life).max(0.0)
            };
            if damage <= 0.0 {
                continue;
            }
            let asteroid = self
                .world
                .ship_debris_collisions
                .iter()
                .any(|hit| hit.ship == pilot.vehicle.0);
            let surface = self
                .world
                .body_impacts
                .iter()
                .any(|hit| hit.ship == pilot.vehicle.0 && hit.damage > 0.0);
            let d = &mut pilot.damage;
            d.hits += 1;
            d.last_damage_tick = Some(self.world.tick);
            d.last_damage_percent = damage / max_life.max(f32::EPSILON) * 100.0;
            d.last_ship_lost = lost;
            let weapon = pilot
                .combat
                .as_ref()
                .filter(|c| c.telemetry.last_hit_taken_tick == Some(self.world.tick))
                .and_then(|c| c.telemetry.last_hit_source);
            d.last_source = Some(if let Some(weapon) = weapon {
                weapon
            } else if asteroid {
                "asteroid"
            } else if surface {
                "ground"
            } else {
                "ship loss / impact"
            });
        }
    }
}
