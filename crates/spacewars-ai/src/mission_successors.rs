//! Counterfactual flight options from a pinned handoff observation. No forecast
//! grants physical permissions or mutates the controlling pilot. One charged
//! graph-work unit advances one own/opponent pair by one motor tick.
use super::*;
use engine_core::planning::{PlanningJob, WorkKind};
use opponent::{OpponentResponse, ResponseState};
use scenario_spacewars::surface_sortie::destination_cover::{
    CoverCandidate, CoverFinding, MAX_COVER_CANDIDATES,
};
use scenario_spacewars::weapons::WeaponSupplyObservation;

const APPROACH_HEIGHT: f32 = 85.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Successor {
    Transfer { site: LandingSiteId },
    ContinueEscape,
    Combat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastEnd {
    Horizon,
    ApproachReached,
    EscapeDeadline,
    OwnBoundaryMargin,
    OwnObstacleMargin,
    OpponentBoundaryMargin,
    OpponentObstacleMargin,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SuccessorResources {
    pub ship_health: f32,
    pub opponent_health: f32,
    pub supply: Option<WeaponSupplyObservation>,
    pub laser_available_now: bool,
    pub cannon_ready_now: bool,
    pub owned_planets: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureOpportunity {
    pub observed_owner: Option<PlayerId>,
    pub ownership_known: bool,
    pub adds_owned_planet: bool,
    pub first_rebuild_foothold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SiteApproach {
    pub site: LandingSiteId,
    /// Measured body origin plus approach height, in the sampled material frame.
    pub local_position: Vec2,
}
impl SiteApproach {
    fn position(self, o: &MissionObservationV1) -> Vec2 {
        let planet = o
            .planets
            .iter()
            .find(|p| p.index == self.site.planet)
            .unwrap();
        planet.motion.position + self.local_position.rotate_radians(planet.motion.angle)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SuccessorBranch {
    pub response: OpponentResponse,
    pub ticks: u64,
    pub end: ForecastEnd,
    pub minimum_range: f32,
    pub first_inside_tick: Option<u64>,
    pub boundary: BoundaryForecast,
    pub minimum_nominal_clearance: f32,
    pub initial_approach_distance: Option<f32>,
    pub final_approach_distance: Option<f32>,
    pub samples: Vec<TransferForecastSample>,
    pub opponent: OpponentForecast,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SuccessorOption {
    pub successor: Successor,
    pub evidence: Option<CoverCandidate>,
    pub evidence_age_ticks: Option<u64>,
    pub approach: Option<SiteApproach>,
    pub capture: Option<CaptureOpportunity>,
    /// No flight is evaluated for an unusable proposal. A stale but compatible
    /// sample can propose an endpoint; it cannot authorize the subsequent trip.
    pub not_evaluated: Option<&'static str>,
    pub branches: Vec<SuccessorBranch>,
    pub validation_required: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SuccessorComparison {
    pub source_tick: u64,
    pub escape_started_tick: u64,
    pub escape_deadline_tick: u64,
    pub resources: SuccessorResources,
    pub options: Vec<SuccessorOption>,
    pub paired_motor_ticks: u64,
    /// The handoff tick's control has already been emitted. Each branch that
    /// advances starts with that common command, then chooses its alternative.
    pub committed_prefix_ticks: u64,
    pub assumptions: [&'static str; 4],
    /// The host must revalidate the live state before acting on a historical
    /// job result. Even a current flight forecast has unmeasured surface legs.
    pub diagnostic_only: bool,
}

#[derive(Clone)]
struct Branch {
    option: usize,
    bot: MaterialMissionPilot,
    predicted: MissionObservationV1,
    enemy: ResponseState,
    approach: Option<SiteApproach>,
    direction: Vec2,
    limit: u64,
    report: SuccessorBranch,
    done: bool,
}

#[derive(Clone)]
pub struct SuccessorComparisonJob {
    report: SuccessorComparison,
    branches: Vec<Branch>,
    cursor: usize,
}

impl MaterialMissionPilot {
    /// Called by an opt-in diagnostic host just after the handoff's controls
    /// are emitted. The job pins that observation and a copy of controller
    /// memory; the returned alternatives never feed back into this pilot.
    pub fn successor_comparison(&self, o: &MissionObservationV1) -> Option<SuccessorComparisonJob> {
        let p = &o.local.combat.recovery.flight.pilot;
        let attempt = self.telemetry.disengagement.as_ref()?.last?;
        if attempt.finished_tick != Some(p.tick)
            || self.previous_tick != Some(p.tick)
            || attempt.reason != Some("separation established")
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || !matches!(p.location, PilotLocation::Aboard(_))
            || !o.local.combat.target.is_some_and(|target| {
                target.health > 0.0 && target.ship_form == Some(ShipForm::Ship)
            })
        {
            return None;
        }
        Some(SuccessorComparisonJob::new(self, o, attempt))
    }
}

impl SuccessorComparisonJob {
    fn new(
        bot: &MaterialMissionPilot,
        o: &MissionObservationV1,
        attempt: DisengagementAttempt,
    ) -> Self {
        let p = &o.local.combat.recovery.flight.pilot;
        let owned = o
            .planets
            .iter()
            .filter(|planet| {
                planet
                    .claim
                    .as_ref()
                    .is_some_and(|claim| claim.owner == Some(p.owner))
            })
            .count();
        let mut job = Self {
            report: SuccessorComparison {
                source_tick: p.tick,
                escape_started_tick: attempt.started_tick,
                escape_deadline_tick: attempt.deadline_tick,
                resources: SuccessorResources {
                    ship_health: p.ship_health,
                    opponent_health: o.local.combat.target.unwrap().health,
                    supply: o.local.combat.supply,
                    laser_available_now: o.local.combat.laser_available,
                    cannon_ready_now: o.local.combat.cannon_ready,
                    owned_planets: owned,
                },
                options: Vec::new(),
                paired_motor_ticks: 0,
                committed_prefix_ticks: 1,
                assumptions: [
                    "own measured gravity held constant; planets translate and spin",
                    "opponent starts with open wings and zero gravity",
                    "combat steering assumes clear sight; no firing or damage forecast",
                    "material cover and weapon supply describe the source observation only",
                ],
                diagnostic_only: true,
            },
            branches: Vec::new(),
            cursor: 0,
        };
        for candidate in o
            .destination_cover
            .iter()
            .flat_map(|r| &r.candidates)
            .take(MAX_COVER_CANDIDATES)
        {
            let mut option = SuccessorOption {
                successor: Successor::Transfer { site: candidate.id },
                evidence: Some(candidate.clone()),
                evidence_age_ticks: candidate
                    .measurement
                    .as_ref()
                    .map(|m| p.tick.saturating_sub(m.tick)),
                approach: None,
                capture: None,
                not_evaluated: None,
                branches: Vec::new(),
                validation_required: vec![
                    "current landing geometry and material cover",
                    "flight under live gravity, obstacles and opponent motion",
                    "capture and boarding round trip",
                ],
            };
            if let Some(planet) = o
                .planets
                .iter()
                .find(|planet| planet.index == candidate.id.planet)
            {
                let owner = planet.claim.as_ref().and_then(|claim| claim.owner);
                let gain = planet.claim.is_some() && owner != Some(p.owner);
                option.capture = Some(CaptureOpportunity {
                    observed_owner: owner,
                    ownership_known: planet.claim.is_some(),
                    adds_owned_planet: gain,
                    first_rebuild_foothold: gain && owned == 0,
                });
                if owner == Some(p.owner) {
                    option.not_evaluated = Some("destination already owned");
                } else if let Some(m) = &candidate.measurement {
                    if m.tick > p.tick
                        || m.revision != planet.revision
                        || m.ship_form != p.ship_form
                    {
                        option.not_evaluated = Some("incompatible material or vehicle sample");
                    } else if let Some(site) = m.site.filter(|site| {
                        m.finding == CoverFinding::Measured
                            && site.id == candidate.id
                            && site.revision == m.revision
                    }) {
                        option.approach = Some(SiteApproach {
                            site: candidate.id,
                            local_position: (site.vehicle_position + site.normal * APPROACH_HEIGHT
                                - m.planet.position)
                                .rotate_radians(-m.planet.angle),
                        });
                    } else {
                        option.not_evaluated = Some("no measured landing endpoint");
                    }
                } else {
                    option.not_evaluated = Some("landing measurement pending");
                }
            } else {
                option.not_evaluated = Some("destination absent");
            }
            job.add_option(bot, o, attempt, option);
        }
        for successor in [Successor::ContinueEscape, Successor::Combat] {
            job.add_option(
                bot,
                o,
                attempt,
                SuccessorOption {
                    successor,
                    evidence: None,
                    evidence_age_ticks: None,
                    approach: None,
                    capture: None,
                    not_evaluated: None,
                    branches: Vec::new(),
                    validation_required: match successor {
                        Successor::Combat => vec![
                            "current firing line and weapon readiness",
                            "combat outcome and incoming damage",
                        ],
                        _ => vec![
                            "remaining escape deadline",
                            "an executable successor after further separation",
                        ],
                    },
                },
            );
        }
        job
    }

    fn add_option(
        &mut self,
        bot: &MaterialMissionPilot,
        o: &MissionObservationV1,
        attempt: DisengagementAttempt,
        option: SuccessorOption,
    ) {
        let index = self.report.options.len();
        if option.not_evaluated.is_none() {
            for response in [
                OpponentResponse::Coast,
                OpponentResponse::BrakeToAim,
                OpponentResponse::Pursue,
            ] {
                let mut preview = bot.clone();
                preview.capture = None;
                preview.recovery = None;
                preview.telemetry.events.clear();
                preview.telemetry.capture = None;
                preview.telemetry.recovery = None;
                if let Some(d) = &mut preview.telemetry.disengagement {
                    d.last = None;
                    d.handoff = None;
                    d.handoff_probe = false;
                    d.cover_probe = false;
                    d.cover_request = None;
                }
                preview.prepare_handoff_transfer(
                    self.report.source_tick,
                    option.approach.map_or(0, |a| a.site.planet),
                );
                let mut predicted = o.clone();
                // The job has no authority to ask for ground surveys, issue a
                // shot, or carry live material permissions into predicted time.
                predicted.destination_cover = None;
                predicted.local.cover.clear();
                predicted.local.landing_objective = None;
                predicted.local.objective_work = None;
                predicted.local.combat.recovery.ground = None;
                predicted.local.combat.recovery.flight.pilot.sites.clear();
                predicted.local.combat.laser_available = false;
                predicted.local.combat.cannon_ready = false;
                // Combat steering assumes a clear sight line. It never turns
                // this hypothesis into a material ray result or damage estimate.
                let target = predicted.local.combat.target.as_mut().unwrap();
                target.ground_occluded = false;
                target.visible = true;
                let enemy = ResponseState::new(response, target.motion, &predicted);
                let own = predicted.local.combat.recovery.flight.pilot.ship;
                let initial_distance = option
                    .approach
                    .map(|a| own.position.distance_to(a.position(&predicted)));
                let limit = if option.successor == Successor::ContinueEscape {
                    attempt
                        .deadline_tick
                        .saturating_sub(self.report.source_tick)
                        .min(FORECAST_TICKS)
                } else {
                    FORECAST_TICKS
                };
                let mut branch = Branch {
                    option: index,
                    bot: preview,
                    report: SuccessorBranch {
                        response,
                        ticks: 0,
                        end: ForecastEnd::Horizon,
                        minimum_range: own.position.distance_to(enemy.motion().position),
                        first_inside_tick: enemy.forecast().first_inside_tick,
                        boundary: BoundaryForecast::new(o.boundary, own.position),
                        minimum_nominal_clearance: nominal_clearance(&predicted, own.position),
                        initial_approach_distance: initial_distance,
                        final_approach_distance: initial_distance,
                        samples: Vec::new(),
                        opponent: enemy.forecast().clone(),
                    },
                    predicted,
                    enemy,
                    approach: option.approach,
                    direction: attempt.direction,
                    limit,
                    done: false,
                };
                branch.check_end(option.successor, attempt.deadline_tick);
                self.branches.push(branch);
            }
        }
        self.report.options.push(option);
        self.publish_finished();
    }

    fn publish_finished(&mut self) {
        for option in &mut self.report.options {
            option.branches.clear();
        }
        for branch in &self.branches {
            if branch.done {
                self.report.options[branch.option]
                    .branches
                    .push(branch.report.clone());
            }
        }
    }
}

impl PlanningJob for SuccessorComparisonJob {
    type Output = SuccessorComparison;
    fn next_work(&self) -> Option<WorkKind> {
        self.branches
            .iter()
            .any(|b| !b.done)
            .then_some(WorkKind::Graph)
    }
    fn step(&mut self) {
        let count = self.branches.len();
        let index = (0..count)
            .map(|n| (self.cursor + n) % count)
            .find(|&i| !self.branches[i].done)
            .expect("finished successor job");
        let branch = &mut self.branches[index];
        let successor = self.report.options[branch.option].successor;
        let o = &mut branch.predicted;
        o.local.combat.target.as_mut().unwrap().motion = branch.enemy.motion();
        if let Some(opponent) = &mut o.opponent {
            opponent.motion = branch.enemy.motion();
        }
        branch.bot.telemetry.avoidance = None;
        let intent = if branch.report.ticks == 0 {
            branch.bot.previous_intent
        } else {
            match successor {
                Successor::Transfer { .. } => {
                    branch.bot.site_approach_intent(o, branch.approach.unwrap())
                }
                Successor::ContinueEscape => branch.bot.escape_flight(o, branch.direction),
                Successor::Combat => branch.bot.hunt(o),
            }
        };
        branch.enemy.advance(o, branch.report.ticks + 1);
        advance_forecast(o, intent);
        branch.report.ticks += 1;
        branch.enemy.record(o, branch.report.ticks);
        branch.report.opponent = branch.enemy.forecast().clone();
        let own = o.local.combat.recovery.flight.pilot.ship;
        branch.report.minimum_range = branch.enemy.forecast().minimum_range;
        branch.report.first_inside_tick = branch.enemy.forecast().first_inside_tick;
        branch
            .report
            .boundary
            .record(o.boundary, own.position, branch.report.ticks);
        branch.report.minimum_nominal_clearance = branch
            .report
            .minimum_nominal_clearance
            .min(nominal_clearance(o, own.position));
        branch.report.final_approach_distance = branch
            .approach
            .map(|a| own.position.distance_to(a.position(o)));
        branch.check_end(successor, self.report.escape_deadline_tick);
        if branch.done || branch.report.ticks.is_multiple_of(60) {
            branch.report.samples.push(TransferForecastSample {
                after_ticks: branch.report.ticks,
                position: own.position,
                velocity: own.velocity,
                opponent: branch.enemy.motion().position,
            });
        }
        self.report.paired_motor_ticks += 1;
        self.cursor = (index + 1) % count;
        if self.next_work().is_none() {
            self.publish_finished();
        }
    }
    fn output(&self) -> Option<&Self::Output> {
        self.next_work().is_none().then_some(&self.report)
    }
}

impl Branch {
    fn check_end(&mut self, successor: Successor, deadline: u64) {
        let p = &self.predicted.local.combat.recovery.flight.pilot;
        let end = if self.report.boundary.first_margin_tick.is_some() {
            Some(ForecastEnd::OwnBoundaryMargin)
        } else if self.report.minimum_nominal_clearance <= 0.0 {
            Some(ForecastEnd::OwnObstacleMargin)
        } else if self.enemy.forecast().boundary.first_margin_tick.is_some() {
            Some(ForecastEnd::OpponentBoundaryMargin)
        } else if self.enemy.forecast().minimum_nominal_clearance <= 0.0 {
            Some(ForecastEnd::OpponentObstacleMargin)
        } else if successor == Successor::ContinueEscape && p.tick >= deadline {
            Some(ForecastEnd::EscapeDeadline)
        } else if self.approach.is_some_and(|a| {
            let planet = self
                .predicted
                .planets
                .iter()
                .find(|p| p.index == a.site.planet)
                .unwrap();
            self.report.final_approach_distance.unwrap() < 10.0
                && (p.ship.velocity - planet.velocity_at(a.position(&self.predicted))).length()
                    < 18.0
        }) {
            Some(ForecastEnd::ApproachReached)
        } else if self.report.ticks >= self.limit {
            Some(ForecastEnd::Horizon)
        } else {
            None
        };
        if let Some(end) = end {
            self.report.end = end;
            self.done = true;
        }
    }
}

fn nominal_clearance(o: &MissionObservationV1, position: Vec2) -> f32 {
    o.planets
        .iter()
        .map(|p| (p.motion.position, p.radius))
        .chain(o.sun.map(|sun| (sun.position, sun.radius)))
        .map(|(center, radius)| position.distance_to(center) - radius - 65.0)
        .fold(f32::MAX, f32::min)
}

impl MaterialMissionPilot {
    fn site_approach_intent(
        &mut self,
        o: &MissionObservationV1,
        approach: SiteApproach,
    ) -> CombatIntent {
        let p = &o.local.combat.recovery.flight.pilot;
        let planet = o
            .planets
            .iter()
            .find(|p| p.index == approach.site.planet)
            .unwrap();
        self.goal(MissionGoal::Transfer, p.tick);
        let entry = approach.position(o);
        // Include the destination itself: a straight chord toward a sheltered
        // far-side site is not a flight route around its retained material.
        let waypoint = self.route_waypoint(o, entry, None);
        let delta = waypoint - p.ship.position;
        self.guide(
            o,
            self.detour_velocity(o, planet.velocity_at(entry))
                + delta.normalized() * (delta.length() * 0.7).min(55.0),
        )
    }
}

#[cfg(test)]
#[path = "mission_successors/tests.rs"]
mod tests;
