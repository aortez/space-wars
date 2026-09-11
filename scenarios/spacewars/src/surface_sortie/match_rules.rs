//! Opt-in round rules. A pilot survives asset loss, but never respawns after death.
use super::*;

pub const PILOT_HEALTH: f32 = 100.0;
const EJECTION_PROTECTION_TICKS: u64 = 180;
const SAFE_IMPACT_SPEED: f32 = 12.0;
const IMPACT_DAMAGE_PER_SPEED: f32 = 4.0;
const MISSILE_DAMAGE: f32 = 40.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotDamageCause {
    Laser,
    Missile,
    Impact,
    SolarHeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PilotDamageEvent {
    pub tick: u64,
    pub cause: PilotDamageCause,
    pub amount: f32,
    pub contact: Option<PilotDamageContact>,
}

/// The last damaging contact, captured before debris cleanup. This bounded
/// record describes the solver input; it neither changes damage nor runs queries.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PilotDamageContact {
    pub on_foot: bool,
    pub other_kind: &'static str,
    pub other_id: Option<u64>,
    pub point: Option<Vec2>,
    /// From the pilot toward the other body.
    pub normal: Vec2,
    pub closing_speed: f32,
    pub impulse: f32,
    pub actor_motion: Option<pilot::PilotMotion>,
    pub other_motion: Option<pilot::PilotMotion>,
}

/// Persistent through boarding, ejection and rebuilding; flags do not heal it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PilotVitals {
    pub health: f32,
    pub protected_until_tick: u64,
    pub last_damage: Option<PilotDamageEvent>,
    pub death_tick: Option<u64>,
}

impl Default for PilotVitals {
    fn default() -> Self {
        Self {
            health: PILOT_HEALTH,
            protected_until_tick: 0,
            last_damage: None,
            death_tick: None,
        }
    }
}

impl PilotVitals {
    pub fn alive(self) -> bool {
        self.death_tick.is_none()
    }

    pub(super) fn protect_ejection(&mut self, tick: u64) {
        self.protected_until_tick = tick + EJECTION_PROTECTION_TICKS;
    }

    fn damage(&mut self, amount: f32, cause: PilotDamageCause, tick: u64) {
        if !self.alive() || tick < self.protected_until_tick || !amount.is_finite() || amount <= 0.0
        {
            return;
        }
        let amount = amount.min(self.health);
        self.health -= amount;
        self.last_damage = Some(PilotDamageEvent {
            tick,
            cause,
            amount,
            contact: None,
        });
        if self.health <= 0.0 {
            self.death_tick = Some(tick);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchOutcome {
    Winner(PlayerId),
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchEndReason {
    PilotDeath,
    SimultaneousDeaths,
    TimeLimit,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct MatchRound {
    time_limit: Option<Duration>,
    elapsed: Duration,
    reason: Option<MatchEndReason>,
    outcome: Option<MatchOutcome>,
    finished_tick: Option<u64>,
}

impl Default for MatchRound {
    fn default() -> Self {
        Self {
            time_limit: Some(Duration::from_secs(600)),
            elapsed: Duration::ZERO,
            reason: None,
            outcome: None,
            finished_tick: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MatchObservation {
    pub time_limit_seconds: Option<f64>,
    pub elapsed_seconds: f64,
    pub remaining_seconds: Option<f64>,
    pub reason: Option<MatchEndReason>,
    pub owned_planets: [usize; 2],
    pub version: u32,
    pub pilots: Vec<PilotVitals>,
    pub outcome: Option<MatchOutcome>,
    pub finished_tick: Option<u64>,
}

impl SurfaceSortieScenario {
    pub fn init_material_match(seed: u64) -> SurfaceSortieState {
        let mut state = Self::init_material_arena(seed);
        state.enable_match_rules();
        state
    }
}

impl SurfaceSortieState {
    /// Select once at initialization. Historical labs and endurance fixtures
    /// retain their survival rules unless their caller explicitly opts in.
    pub fn enable_match_rules(&mut self) {
        assert_eq!(self.world.tick, 0, "select round rules before stepping");
        assert_eq!(self.player_count(), SPACEWARS_PLAYER_COUNT);
        assert!(self.combat_enabled());
        assert!(self.pilots.iter().all(|p| p.recovery.is_some()));
        if self.round.is_none() {
            self.round = Some(MatchRound::default());
            for pilot in &mut self.pilots {
                pilot.vitals = Some(PilotVitals::default());
            }
        }
    }

    pub fn set_match_time_limit(&mut self, limit: Option<Duration>) {
        assert_eq!(self.world.tick, 0, "configure the match before stepping");
        if let Some(round) = &mut self.round {
            round.time_limit = limit;
        }
    }

    pub(super) fn advance_match_time(&mut self, dt: Duration) {
        if let Some(round) = &mut self.round {
            round.elapsed = round.elapsed.saturating_add(dt);
            if let Some(limit) = round.time_limit {
                round.elapsed = round.elapsed.min(limit);
            }
        }
    }

    pub fn match_result_message(&self) -> Option<String> {
        let round = self.round.as_ref()?;
        let outcome = round.outcome?;
        Some(match (outcome, round.reason?) {
            (MatchOutcome::Winner(owner), MatchEndReason::PilotDeath) => {
                format!("Player {} wins / opposing pilot lost", owner.index() + 1)
            }
            (MatchOutcome::Winner(owner), MatchEndReason::TimeLimit) => format!(
                "Player {} wins / time limit / more planets owned",
                owner.index() + 1
            ),
            (MatchOutcome::Draw, MatchEndReason::TimeLimit) => {
                "Draw / time limit / equal planets".into()
            }
            _ => "Draw / both pilots lost".into(),
        })
    }

    pub fn match_clock_label(&self) -> Option<String> {
        let round = self.round.as_ref()?;
        Some(match round.time_limit {
            Some(limit) => {
                let seconds = limit.saturating_sub(round.elapsed).as_secs_f64().ceil() as u64;
                format!("Time {:02}:{:02}", seconds / 60, seconds % 60)
            }
            None => "Time unlimited".into(),
        })
    }

    fn owned_planet_counts(&self) -> [usize; 2] {
        std::array::from_fn(|seat| {
            self.world
                .planets
                .iter()
                .filter(|planet| planet.owner_id == Some(seat))
                .count()
        })
    }

    pub fn match_outcome(&self) -> Option<MatchOutcome> {
        self.round.as_ref().and_then(|round| round.outcome)
    }

    pub fn match_observation(&self) -> Option<MatchObservation> {
        let round = self.round.as_ref()?;
        Some(MatchObservation {
            version: 2,
            time_limit_seconds: round.time_limit.map(|limit| limit.as_secs_f64()),
            elapsed_seconds: round.elapsed.as_secs_f64(),
            remaining_seconds: round
                .time_limit
                .map(|limit| limit.saturating_sub(round.elapsed).as_secs_f64()),
            reason: round.reason,
            owned_planets: self.owned_planet_counts(),
            pilots: self
                .pilots
                .iter()
                .map(|p| p.vitals.expect("match pilot"))
                .collect(),
            outcome: round.outcome,
            finished_tick: round.finished_tick,
        })
    }

    /// Evaluate both pilots only after all damage in the shared physics step.
    pub(super) fn finish_round(&mut self, evaluate_timeout: bool) -> bool {
        let owned = self.owned_planet_counts();
        let Some(round) = &mut self.round else {
            return false;
        };
        let alive: Vec<_> = self
            .pilots
            .iter()
            .filter(|p| p.vitals.unwrap().alive())
            .collect();
        let result = match alive.as_slice() {
            [] => Some((MatchOutcome::Draw, MatchEndReason::SimultaneousDeaths)),
            [pilot] => Some((
                MatchOutcome::Winner(pilot.owner),
                MatchEndReason::PilotDeath,
            )),
            _ if evaluate_timeout
                && round.time_limit.is_some_and(|limit| round.elapsed >= limit) =>
            {
                let outcome = match owned[0].cmp(&owned[1]) {
                    std::cmp::Ordering::Greater => MatchOutcome::Winner(PlayerId::PLAYER_1),
                    std::cmp::Ordering::Less => MatchOutcome::Winner(PlayerId::PLAYER_2),
                    std::cmp::Ordering::Equal => MatchOutcome::Draw,
                };
                Some((outcome, MatchEndReason::TimeLimit))
            }
            _ => None,
        };
        if let Some((outcome, reason)) = result {
            round.outcome = Some(outcome);
            round.reason = Some(reason);
        }
        if round.outcome.is_none() {
            return false;
        }
        round.finished_tick = Some(self.world.tick);
        self.world.winner = match round.outcome {
            Some(MatchOutcome::Winner(owner)) => Some(owner.index()),
            _ => None,
        };
        for pilot in &mut self.pilots {
            self.world.players[pilot.owner.index()].eliminated = !pilot.vitals.unwrap().alive();
            pilot.controls_armed = false;
            pilot.control = SpacelingControl::default();
            let ship = &mut self.world.ships[pilot.vehicle.0];
            ship.set_thrust(0.0);
            ship.set_turn(0.0);
            ship.set_brake(0.0);
            ship.set_laser(false);
            ship.set_cannon(false);
            ship.laser_beam = None;
        }
        true
    }
}

fn exposed_entity(world: &SpacewarsState, pilot: &SurfacePilot) -> Option<MechanicalEntity> {
    pilot.vitals?;
    if pilot.body.is_some() {
        Some(MechanicalEntity::Spaceling(pilot.owner.index()))
    } else {
        let ship = &world.ships[pilot.vehicle.0];
        (!ship.dead && ship.form == ShipForm::EscapePod)
            .then_some(MechanicalEntity::Ship(pilot.vehicle.0))
    }
}

/// Called before full-ship deaths change those vehicles into pods. A shot
/// that destroys the full ship never also damages its newly ejected pilot.
pub(crate) fn apply_lasers(world: &SpacewarsState, pilots: &mut [SurfacePilot]) {
    for pilot in pilots {
        let Some(entity) = exposed_entity(world, pilot) else {
            continue;
        };
        for hit in &world.laser_hits {
            let target = match hit.target {
                LaserTarget::Ship(index) => MechanicalEntity::Ship(index),
                LaserTarget::Spaceling(index) => MechanicalEntity::Spaceling(index),
                _ => continue,
            };
            if target == entity {
                pilot.vitals.as_mut().unwrap().damage(
                    hit.damage,
                    PilotDamageCause::Laser,
                    world.tick + 1,
                );
            }
        }
    }
}

/// Solver contacts supply actual closing speed; resting support and manifold
/// duplicates cannot drain health. Consume a missile once on an exposed actor.
pub(crate) fn apply_contacts(
    world: &mut SpacewarsState,
    pilots: &mut [SurfacePilot],
    contacts: &[MechanicalContact],
) {
    if !pilots.iter().any(|p| p.vitals.is_some()) {
        return;
    }
    let mut spent_shells = BTreeSet::new();
    let mut external_hits = Vec::new();
    for contact in strongest_entity_contacts(contacts)
        .into_values()
        .filter(|c| c.started)
    {
        for pilot in pilots.iter_mut() {
            let Some(entity) = exposed_entity(world, pilot) else {
                continue;
            };
            let other = if contact.a == entity {
                contact.b
            } else if contact.b == entity {
                contact.a
            } else {
                continue;
            };
            let debris = if let MechanicalEntity::Debris(id) = other {
                world
                    .debris
                    .iter()
                    .enumerate()
                    .find(|(_, d)| d.physics_id == id)
            } else {
                None
            };
            let (amount, cause) =
                if let Some((index, shell)) = debris.filter(|(_, d)| d.kind == DebrisKind::Shell) {
                    if shell.dead || !spent_shells.insert(index) {
                        continue;
                    }
                    if matches!(entity, MechanicalEntity::Spaceling(_)) {
                        external_hits.push((pilot.vehicle.0, shell.owner_id));
                    }
                    (MISSILE_DAMAGE, PilotDamageCause::Missile)
                } else if debris.is_some_and(|(_, d)| d.damage_scalar == 0.0) {
                    // Breakup pieces already have zero direct damage in
                    // Spacewars. Their physical shove can still cause a crash.
                    (0.0, PilotDamageCause::Impact)
                } else {
                    (
                        ((contact.closing_speed - SAFE_IMPACT_SPEED) * IMPACT_DAMAGE_PER_SPEED)
                            .clamp(0.0, PILOT_HEALTH),
                        PilotDamageCause::Impact,
                    )
                };
            let vitals = pilot.vitals.as_mut().unwrap();
            let before = vitals.health;
            vitals.damage(amount, cause, world.tick + 1);
            if vitals.health < before {
                let (other_kind, other_id) = match other {
                    MechanicalEntity::World => ("world", None),
                    MechanicalEntity::Body(BodyId::Sun) => ("sun", None),
                    MechanicalEntity::Body(BodyId::Planet(i)) => ("planet", Some(i as u64)),
                    MechanicalEntity::TerrainFragment(i) => ("terrain_fragment", Some(i)),
                    MechanicalEntity::Ship(i) => ("ship", Some(i as u64)),
                    MechanicalEntity::Spaceling(i) => ("spaceling", Some(i as u64)),
                    MechanicalEntity::Rover(i) => ("rover", Some(i)),
                    MechanicalEntity::Debris(i) => (
                        debris.map_or("debris", |(_, d)| match d.kind {
                            DebrisKind::Shell => "missile",
                            DebrisKind::Asteroid => "asteroid",
                            DebrisKind::Fragment => "wreckage",
                        }),
                        Some(i),
                    ),
                };
                let motion = |entity| {
                    world
                        .physics
                        .pre_step_motion(entity)
                        .map(|m| pilot::PilotMotion {
                            position: m.position,
                            velocity: m.linear_velocity,
                            angle: m.angle,
                            spin: m.angular_velocity,
                        })
                };
                vitals.last_damage.as_mut().unwrap().contact = Some(PilotDamageContact {
                    on_foot: pilot.body.is_some(),
                    other_kind,
                    other_id,
                    point: contact.point,
                    normal: if contact.a == entity {
                        contact.normal
                    } else {
                        -contact.normal
                    },
                    closing_speed: contact.closing_speed,
                    impulse: contact.impulse_magnitude,
                    actor_motion: motion(entity),
                    other_motion: motion(other),
                });
            }
        }
    }
    for index in spent_shells {
        let life = world.debris[index].life;
        world.debris[index].translate_life(-life);
    }
    for (target, shooter) in external_hits {
        combat::record_missile_hit(pilots, target, shooter, world.tick + 1);
    }
}

pub(crate) fn apply_heat(world: &SpacewarsState, pilots: &mut [SurfacePilot], dt: f32) {
    let Some(sun) = world.sun else { return };
    for pilot in pilots {
        if exposed_entity(world, pilot).is_none() {
            continue;
        }
        let position = pilot.snapshot(&world.physics).map_or_else(
            || world.ships[pilot.vehicle.0].position + physics::ship_pivot(ShipForm::EscapePod),
            |s| s.motion.position,
        );
        let intensity = (1.0
            - (position.distance_to(sun.position) - sun.radius) / solar::CORONA_WIDTH)
            .clamp(0.0, 1.0);
        pilot.vitals.as_mut().unwrap().damage(
            PILOT_HEALTH * 0.2 * intensity * dt,
            PilotDamageCause::SolarHeat,
            world.tick + 1,
        );
    }
}
