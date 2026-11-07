//! Learned legs in the mission: a host that owns the world flies the leg a
//! mission pilot publishes, a transfer to a destination's entry or a climb
//! out of the launch band or for a firing pass, with the retained v2 flight
//! stack, switching the ship to learned flight control for the leg and
//! returning the ship to the pilot's own controls the tick the leg is
//! withdrawn, at the mission's handoff or when a higher priority claims the
//! ship. The pilot keeps every decision but the flying, and its own controls
//! fly any tick the stack refuses the ship's state, so the mission never
//! waits on the stack.
use crate::combat_pilot::CombatIntent;
use crate::flight_stack::{
    FlightStack, FlightStackDriver, FlightStackError, GoalOutcome, MAX_DEADLINE_TICKS,
    NavigationGoal, StackDecision, flight_actions,
};
use crate::mission_pilot::{FlightLeg, LegKind};
use engine_common::Action;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        SurfaceSortieState, flight::FlightControlMode, flight_environment::ArrivalCriteria,
        flight_policy_observation::FlightPolicyObservation,
    },
};
use serde::Serialize;

/// The stack's view of the leg it is flying, for a HUD or a diagnostic line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LegStatus {
    pub tick: u64,
    pub kind: LegKind,
    pub target: usize,
    pub goal_id: u64,
    pub elapsed_ticks: u32,
    /// Center of mass to the goal.
    pub distance: f32,
    pub speed_error: f32,
    pub dwell_ticks: u32,
    pub outcome: Option<GoalOutcome>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LearnedTransferTelemetry {
    /// Transfer legs published, counted when the selected destination
    /// changes.
    pub legs: u32,
    /// Climb legs published, out of the launch band or for a firing pass;
    /// a climb interrupted and taken up again is another.
    pub climbs: u32,
    /// Times a withdrawn leg was published again for the same destination,
    /// whether or not a climb came between.
    pub resumptions: u32,
    /// Attempts the stack started: one per leg, and another when a withdrawn
    /// leg resumes, an attempt ends at its goal or deadline, or the stack
    /// refused the previous tick.
    pub attempts: u32,
    pub flown_ticks: u64,
    /// Flown ticks on climb legs.
    pub climb_ticks: u64,
    /// Flown ticks with the ship's spin above 1.5 radians per second, near
    /// the turn-rate limit: the motor's steering at saturation.
    pub spinning_ticks: u64,
    /// Ticks inside a leg the pilot's own controls flew after a refusal.
    pub native_ticks: u64,
    /// Ticks inside a leg the pilot's own controls flew because the ship
    /// touched a surface, with the contact handback on.
    pub contact_ticks: u64,
    pub arrivals: u32,
    pub deadlines: u32,
    pub refusals: u32,
    pub last_refusal: Option<String>,
    pub last: Option<LegStatus>,
}

struct ActiveLeg {
    kind: LegKind,
    started_tick: u64,
    target: usize,
    goal_id: u64,
    last_tick: u64,
    /// The attempt reached its goal or deadline; the next tick starts another.
    ended: bool,
}

struct Seat {
    driver: FlightStackDriver,
    active: Option<ActiveLeg>,
    /// The last transfer leg's start tick and destination, for counting legs
    /// across withdrawals and the climbs between them.
    last_transfer: Option<(u64, usize)>,
    /// The last climb leg's start tick and planet, as above.
    last_climb: Option<(u64, usize)>,
    telemetry: LearnedTransferTelemetry,
}

/// One stack shared by every seat it flies, with a driver per seat.
pub struct LearnedTransferHost {
    stack: FlightStack,
    next_goal_id: u64,
    seats: Vec<Seat>,
    contact_handback: bool,
}

impl LearnedTransferHost {
    pub fn new(seats: usize) -> Result<Self, FlightStackError> {
        Self::with_stack(FlightStack::retained_v2()?, seats)
    }

    /// A host flying a stack other than the retained one, for evaluating a
    /// candidate export in matches before it is retained.
    pub fn with_stack(stack: FlightStack, seats: usize) -> Result<Self, FlightStackError> {
        Ok(Self {
            stack,
            next_goal_id: 0,
            seats: (0..seats)
                .map(|seat| {
                    Ok(Seat {
                        driver: FlightStackDriver::new(seat)?,
                        active: None,
                        last_transfer: None,
                        last_climb: None,
                        telemetry: LearnedTransferTelemetry::default(),
                    })
                })
                .collect::<Result<_, _>>()?,
            contact_handback: false,
        })
    }

    /// A diagnostic: leave to the pilot's own controls every leg tick on
    /// which the ship touches a surface, the state the motor's training
    /// tasks end on, to measure what the stack's flying there costs.
    pub fn set_contact_handback(&mut self, on: bool) {
        self.contact_handback = on;
    }

    pub fn stack(&self) -> &FlightStack {
        &self.stack
    }

    pub fn telemetry(&self, seat: usize) -> &LearnedTransferTelemetry {
        &self.seats[seat].telemetry
    }

    /// Whether the stack flew the seat's last actions.
    pub fn flying(&self, seat: usize) -> bool {
        self.seats[seat].active.is_some()
    }

    /// The stack's decision behind the seat's last actions, while it flies.
    pub fn last_decision(&self, seat: usize) -> Option<&StackDecision> {
        self.flying(seat)
            .then(|| self.seats[seat].driver.last())
            .flatten()
    }

    /// The navigation network's observation of the published leg on the
    /// active attempt's clock, for diagnostics, while the stack flies.
    pub fn observation(
        &self,
        seat: usize,
        state: &SurfaceSortieState,
        leg: FlightLeg,
    ) -> Option<Result<FlightPolicyObservation, FlightStackError>> {
        let entry = &self.seats[seat];
        let active = entry.active.as_ref()?;
        Some(entry.driver.observation(
            state,
            NavigationGoal {
                id: active.goal_id,
                position: leg.goal,
                velocity: leg.goal_velocity,
                arrival: ArrivalCriteria::default(),
                deadline_ticks: MAX_DEADLINE_TICKS,
            },
        ))
    }

    /// The seat's actions for this tick: the stack's while it flies the
    /// published leg, the pilot's own otherwise. Call once per tick after the
    /// pilot's decision and before stepping the world. The ship is in learned
    /// flight control exactly on the ticks the stack flies it.
    pub fn actions(
        &mut self,
        seat: usize,
        state: &mut SurfaceSortieState,
        leg: Option<FlightLeg>,
        intent: CombatIntent,
    ) -> Vec<Action> {
        let owner = PlayerId::from_index(seat).expect("a seated player");
        let Some(leg) = leg else {
            self.release(seat, state);
            return intent.encode(owner).to_vec();
        };
        if self.contact_handback
            && state
                .free_flight_observation(seat)
                .is_some_and(|own| own.contact)
        {
            self.seats[seat].telemetry.contact_ticks += 1;
            self.release(seat, state);
            return intent.encode(owner).to_vec();
        }
        if state.flight_control_mode(seat) != Some(FlightControlMode::Learned) {
            state.set_flight_control_mode(seat, FlightControlMode::Learned);
        }
        let entry = &mut self.seats[seat];
        let identity = (leg.kind, leg.started_tick, leg.target);
        // Each kind keeps its own last leg, so a climb between a transfer's
        // withdrawal and its return leaves the return a resumption.
        let (last, published) = match leg.kind {
            LegKind::Transfer => (&mut entry.last_transfer, &mut entry.telemetry.legs),
            LegKind::Climb => (&mut entry.last_climb, &mut entry.telemetry.climbs),
        };
        if *last != Some((leg.started_tick, leg.target)) {
            *published += 1;
            *last = Some((leg.started_tick, leg.target));
        } else if entry
            .active
            .as_ref()
            .is_none_or(|a| (a.kind, a.started_tick, a.target) != identity)
        {
            entry.telemetry.resumptions += 1;
        }
        let continuing = entry.active.as_ref().is_some_and(|a| {
            (a.kind, a.started_tick, a.target) == identity
                && a.last_tick + 1 == leg.tick
                && !a.ended
        });
        if !continuing {
            self.next_goal_id += 1;
            entry.telemetry.attempts += 1;
            entry.active = Some(ActiveLeg {
                kind: leg.kind,
                started_tick: leg.started_tick,
                target: leg.target,
                goal_id: self.next_goal_id,
                last_tick: leg.tick,
                ended: false,
            });
        }
        let active = entry.active.as_mut().expect("an active leg");
        active.last_tick = leg.tick;
        let goal = NavigationGoal {
            id: active.goal_id,
            position: leg.goal,
            velocity: leg.goal_velocity,
            arrival: ArrivalCriteria::default(),
            deadline_ticks: MAX_DEADLINE_TICKS,
        };
        match entry.driver.decide(&self.stack, state, goal) {
            Ok(decision) => {
                let progress = decision.progress;
                entry.telemetry.flown_ticks += 1;
                if leg.kind == LegKind::Climb {
                    entry.telemetry.climb_ticks += 1;
                }
                if state
                    .free_flight_observation(seat)
                    .is_some_and(|own| own.ship.spin.abs() > 1.5)
                {
                    entry.telemetry.spinning_ticks += 1;
                }
                match progress.outcome {
                    Some(GoalOutcome::Arrived) => {
                        entry.telemetry.arrivals += 1;
                        active.ended = true;
                    }
                    Some(GoalOutcome::Deadline) => {
                        entry.telemetry.deadlines += 1;
                        active.ended = true;
                    }
                    None => {}
                }
                entry.telemetry.last = Some(LegStatus {
                    tick: leg.tick,
                    kind: leg.kind,
                    target: leg.target,
                    goal_id: goal.id,
                    elapsed_ticks: progress.elapsed_ticks,
                    distance: progress.errors.position,
                    speed_error: progress.errors.velocity,
                    dwell_ticks: progress.dwell_ticks,
                    outcome: progress.outcome,
                });
                let mut actions = flight_actions(decision.action, owner).to_vec();
                actions.push(intent.weapons.encode(owner));
                actions
            }
            Err(error) => {
                entry.telemetry.refusals += 1;
                entry.telemetry.native_ticks += 1;
                entry.telemetry.last_refusal = Some(error.to_string());
                self.release(seat, state);
                intent.encode(owner).to_vec()
            }
        }
    }

    fn release(&mut self, seat: usize, state: &mut SurfaceSortieState) {
        if state.flight_control_mode(seat) == Some(FlightControlMode::Learned) {
            state.set_flight_control_mode(seat, FlightControlMode::Standard);
        }
        self.seats[seat].active = None;
    }

    /// A short HUD line while the stack flies the seat.
    pub fn label(&self, seat: usize) -> Option<String> {
        let entry = &self.seats[seat];
        let active = entry.active.as_ref()?;
        let status = entry
            .telemetry
            .last
            .filter(|s| s.tick == active.last_tick)?;
        Some(format!(
            "{} {:.0} units {:.1} s",
            match active.kind {
                LegKind::Transfer => "stack",
                LegKind::Climb => "climb",
            },
            status.distance,
            status.elapsed_ticks as f32 / 60.0
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrainReset;
    use crate::flight_stack::STACK_ID_V2;
    use crate::mission_pilot::MissionGoal;
    use crate::mission_policy::{MissionBot, MissionPolicy};
    use engine_common::Scenario;
    use engine_core::Vec2;
    use scenario_spacewars::surface_sortie::SurfaceSortieScenario;
    use std::time::Duration;

    const STEP: Duration = Duration::from_nanos(16_666_667);

    #[test]
    fn the_stack_flies_the_published_leg_and_the_pilot_takes_the_handoff() {
        let seat = 1;
        let mut state = SurfaceSortieScenario::init_material_match(42);
        let context = BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: 42,
        };
        let mut bot = MissionBot::new(MissionPolicy::Legacy, context, Default::default())
            .with_learned_transfer(true);
        assert_eq!(bot.telemetry().learned_transfer, Some(STACK_ID_V2));
        assert!(bot.learned_transfer());
        let mut host = LearnedTransferHost::new(2).unwrap();
        assert_eq!(host.stack().contract(), 2);
        let mut first_leg = None;
        let mut handoff = None;
        let mut distances = Vec::new();
        let mut climbs = Vec::new();
        let mut climb_planet = None;
        for tick in 0..90 * 60 {
            let o = state.mission_observation_with_cadence(
                seat,
                bot.sensor_request(),
                Default::default(),
            );
            let intent = bot.intent(&o);
            let leg = bot.flight_leg();
            let actions = host.actions(seat, &mut state, leg, intent);
            match leg {
                Some(leg) => {
                    assert_eq!(leg.tick, state.tick());
                    match leg.kind {
                        LegKind::Climb => {
                            assert_eq!(bot.telemetry().goal, MissionGoal::Launch);
                            assert!(first_leg.is_none(), "the climb comes before the transfer");
                            if climbs.is_empty() {
                                climb_planet = o
                                    .planets
                                    .iter()
                                    .find(|planet| planet.index == leg.target)
                                    .cloned();
                            }
                            climbs.push(leg);
                        }
                        LegKind::Transfer => {
                            assert_eq!(bot.telemetry().goal, MissionGoal::Transfer);
                            assert!(leg.goal_velocity.length() > 5.0, "the entry orbits");
                            first_leg.get_or_insert(tick);
                        }
                    }
                    if host.flying(seat) {
                        assert_eq!(
                            state.flight_control_mode(seat),
                            Some(FlightControlMode::Learned)
                        );
                        assert_eq!(actions.len(), 4);
                        if leg.kind == LegKind::Transfer {
                            distances.push(host.telemetry(seat).last.unwrap().distance);
                        }
                    }
                }
                None => {
                    assert_eq!(
                        state.flight_control_mode(seat),
                        Some(FlightControlMode::Standard)
                    );
                    assert!(!host.flying(seat));
                    assert_eq!(actions.len(), 3);
                }
            }
            SurfaceSortieScenario::step(&mut state, &actions, STEP);
            if first_leg.is_some() && bot.telemetry().goal == MissionGoal::Capture {
                handoff = Some(tick);
                break;
            }
        }
        let first = first_leg.expect("a transfer leg was published");
        let handoff = handoff.expect("the pilot took the handoff");
        let telemetry = host.telemetry(seat);
        assert_eq!(telemetry.legs, 1);
        // The launch band's climb: one leg from the first tick the feet are
        // off the ground, with a goal fixed in the planet's frame that the
        // ship closes on until the band releases it to the transfer.
        assert_eq!(telemetry.climbs, 1, "{telemetry:?}");
        assert!(
            telemetry.climb_ticks > 60 && telemetry.climb_ticks < telemetry.flown_ticks / 2,
            "{telemetry:?}"
        );
        let planet = climb_planet.expect("the planet the climb left");
        assert!(
            climbs
                .iter()
                .all(|c| c.started_tick == climbs[0].started_tick)
        );
        assert!(climbs.iter().all(|c| c.target == climbs[0].target));
        let reach = (climbs[0].goal - planet.motion.position).length();
        assert!((reach - (planet.radius + 130.0)).abs() < 0.01, "{reach}");
        assert!(climbs.last().unwrap().distance + 40.0 < climbs[0].distance);
        assert!(telemetry.flown_ticks > 120, "{telemetry:?}");
        assert!(
            telemetry.native_ticks * 10 < telemetry.flown_ticks,
            "{telemetry:?}"
        );
        assert!(distances.first().unwrap() > &100.0);
        assert!(distances.last().unwrap() < &40.0, "{distances:?}");
        assert!(handoff > first + 120);
        assert!(!host.flying(seat));
        assert_eq!(
            state.flight_control_mode(seat),
            Some(FlightControlMode::Standard)
        );
        assert_eq!(host.label(seat), None);
        assert!(
            bot.telemetry()
                .events
                .iter()
                .any(|e| e.kind == "arrived" && e.tick as usize <= handoff + 1)
        );
    }

    /// The leg bookkeeping across a climb: a transfer withdrawn, a climb out
    /// of the band, then the same transfer again is one leg resumed, not two
    /// legs. The legs are synthetic, flown from the match's first ticks.
    #[test]
    fn a_climb_between_a_transfers_withdrawal_and_its_return_leaves_one_leg_resumed() {
        let seat = 0;
        let mut state = SurfaceSortieScenario::init_material_match(42);
        let mut host = LearnedTransferHost::new(1).unwrap();
        // The first step synchronizes the flight geometry the stack reads.
        for _ in 0..2 {
            let actions = host.actions(seat, &mut state, None, CombatIntent::default());
            SurfaceSortieScenario::step(&mut state, &actions, STEP);
        }
        let start = state.tick();
        let goal =
            state.free_flight_observation(seat).unwrap().ship.position + Vec2::new(0.0, 200.0);
        let leg = |kind, tick, started_tick| FlightLeg {
            kind,
            tick,
            started_tick,
            target: 0,
            goal,
            goal_velocity: Vec2::ZERO,
            distance: 200.0,
        };
        // The transfer selected on the first tick flies three ticks and is
        // withdrawn for two; a climb begun on the sixth flies three; the
        // transfer returns on the tick after the climb's last.
        for offset in 0..11u64 {
            let tick = start + offset;
            let published = match offset {
                0..=2 | 8..=10 => Some(leg(LegKind::Transfer, tick, start)),
                3 | 4 => None,
                _ => Some(leg(LegKind::Climb, tick, start + 5)),
            };
            let actions = host.actions(seat, &mut state, published, CombatIntent::default());
            assert_eq!(
                host.flying(seat),
                published.is_some(),
                "tick {tick}: {:?}",
                host.telemetry(seat)
            );
            SurfaceSortieScenario::step(&mut state, &actions, STEP);
        }
        let telemetry = host.telemetry(seat);
        assert_eq!(
            (
                telemetry.legs,
                telemetry.climbs,
                telemetry.resumptions,
                telemetry.attempts
            ),
            (1, 1, 1, 3),
            "{telemetry:?}"
        );
    }
}
