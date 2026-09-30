//! Read-only consumption gate for the experimental destination policy. A
//! historical timing reference may choose a journey, never authorize a landing.
use super::*;

/// Only the evaluator can construct a proposal, after checking its published
/// dependencies against this control tick. The controller still owns priority,
/// descent commitment, deferred destinations and switch hysteresis.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CaptureSelection {
    pub tick: u64,
    pub source_tick: u64,
    pub current: usize,
    pub selected_tick: u64,
    pub site: Option<LandingSiteId>,
    pub destination: usize,
    pub current_seconds: f32,
    pub destination_seconds: f32,
    pub value: Option<ValueDecision>,
    pub landing: Option<CostedLandingReference>,
}

/// A nomination, never live landing permission. Its original identity and age
/// survive transfer; the native capture controller must acquire fresh geometry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CostedLandingReference {
    pub site: LandingSiteId,
    pub evidence_tick: u64,
    key: PlanetKey,
}
impl CostedLandingReference {
    #[cfg(test)]
    pub(crate) fn fixture(planet: &PilotPlanetObservation, site: LandingSiteId, tick: u64) -> Self {
        Self {
            key: PlanetKey::read(planet),
            site,
            evidence_tick: tick,
        }
    }
    pub(crate) fn rejection(
        &self,
        o: &MissionObservationV1,
        acquired: bool,
        landed: bool,
    ) -> Option<&'static str> {
        let p = &o.local.combat.recovery.flight.pilot;
        if self.evidence_tick > p.tick
            || (!acquired && p.tick - self.evidence_tick > MAX_EVIDENCE_AGE)
        {
            Some("landing reference expired")
        } else if o
            .planets
            .iter()
            .find(|planet| planet.index == self.site.planet)
            .is_none_or(|planet| {
                if landed {
                    !self.key.capture_progress_matches(planet, p.owner)
                } else {
                    !self.key.reference_matches(&PlanetKey::read(planet))
                }
            })
        {
            Some("landing reference identity changed")
        } else {
            None
        }
    }
}

impl MissionEvaluator {
    pub(crate) fn selection(
        &self,
        o: &MissionObservationV1,
        mission: &MissionTelemetry,
    ) -> Option<CaptureSelection> {
        let p = &o.local.combat.recovery.flight.pilot;
        let state = self.actors.get(&(p.owner.index() as u64))?;
        let report = state.latest.as_ref()?;
        let dependencies = state.latest_dependencies.as_ref()?;
        let completed = report.completed_tick?;
        if !p.queries_ready
            || o.local.objective_work
                == Some(
                    scenario_spacewars::surface_sortie::live_planning::ObjectiveWorkState::Stale,
                )
            || report.actor != p.owner
            || report.policy != mission.policy
            || completed < report.source_tick
            || completed > p.tick
            || p.tick.saturating_sub(report.source_tick) > MAX_RESULT_AGE
            || report.inactive_reason.is_some()
            || report.candidates_truncated
            || report.current_target != mission.target
            || report.selected_tick != selection_tick(mission)
            || dependencies.selected_site != mission.capture.as_ref().and_then(|c| c.site)
            || dependencies.handoff_invalidation
                != handoff_invalidation(mission).map(|(h, t)| (h.switch_tick, t))
            || dependencies.location != p.location
            || dependencies.form != p.ship_form
            || report
                .transfer_source
                .as_ref()
                .is_some_and(|source| !source.is_current(o))
            || (report.value_comparison.is_some()
                && report.match_context.as_ref().map(|m| m.owned_planets)
                    != o.match_context.as_ref().map(|m| m.owned_planets))
            || !p.ship_available
            || p.ship_form != ShipForm::Ship
            || !matches!(p.location, PilotLocation::Aboard(_))
            || o.match_context
                .as_ref()
                .is_some_and(|m| m.finished || !m.pilots_alive[p.owner.index()])
            || dependencies.planets.len() != o.planets.len()
            || dependencies
                .planets
                .iter()
                .zip(&o.planets)
                .any(|(old, now)| !old.matches(&PlanetKey::read(now)))
        {
            return None;
        }
        // Pin the result's evidence, not a newer pending refresh. In particular,
        // route replacement and accumulated gravity drift revoke local costs.
        for sample in &state.latest_evidence {
            if !supports_handoff(mission, sample)
                || sample.tick > p.tick
                || p.tick - sample.tick > MAX_EVIDENCE_AGE
            {
                return None;
            }
            if flag_evidence::is_flag(sample)
                && (!(flag_evidence::enabled(mission.policy)
                    || (self.uses_flag_costs(p.owner) && flag_costs::enabled(mission.policy)))
                    || o.planets
                        .iter()
                        .find(|v| v.index == sample.key.planet)
                        .is_none_or(|planet| {
                            let key = PlanetKey::read(planet);
                            !sample.key.reference_matches(&key)
                                || (flag_evidence::enabled(mission.policy)
                                    && !key.flag_identity_matches(
                                        sample.route_objective.unwrap(),
                                        planet.radius,
                                    ))
                        }))
            {
                return None;
            }
            if !sample.remote && sample.key.planet == p.planet.index && sample.costs.is_some() {
                if !sample.key.matches(&PlanetKey::read(&p.planet))
                    || (sample.gravity - o.local.objective_gravity).abs() > 0.01
                {
                    return None;
                }
                if !p.site_query.is_deferred()
                    && p.site_query
                        != scenario_spacewars::surface_sortie::pilot::LandingSiteQuery::NotRequested
                    && (p.sites.iter().all(|site| {
                        site.id != sample.site
                            || site.revision != p.planet.revision
                            || !site.boarding_hatches.iter().any(Option::is_some)
                    }) || !o.local.cover.iter().any(|cover| {
                        cover.site == sample.site
                            && cover.grounded
                            && cover.approach
                            && cover.departure
                    }))
                {
                    return None;
                }
                if let Some(source) = sample.route_source_tick
                    && !model::route_cadence_gap(o, sample)
                    && o.local.landing_objective.as_ref().is_none_or(|route| {
                        route.tick != source
                            || !route.is_current(p.tick)
                            || model::local_costs(o, sample.site).ok() != sample.costs
                    })
                {
                    return None;
                }
            }
        }
        let destination = if let Some(value) = &report.value_comparison {
            value.preferred?
        } else {
            report.preferred_by_time?
        };
        let current = report.current_target?;
        if destination == current {
            return None;
        }
        let cost = |planet| {
            let candidate = report.candidates.iter().find(|c| c.planet == planet)?;
            let total = candidate.total_seconds?;
            (candidate.unknown_reason.is_none()
                && candidate.adds_ownership
                && total.is_finite()
                && total >= 0.0)
                .then_some(total)
        };
        let current_seconds = cost(current)?;
        let destination_seconds = cost(destination)?;
        let value = if report.value_comparison.is_some() {
            Some(value::decision(
                report.candidates.iter().find(|c| c.planet == current)?,
                report.candidates.iter().find(|c| c.planet == destination)?,
            )?)
        } else {
            None
        };
        // Deliberately coarse references need a meaningful improvement, not a
        // tick-by-tick race. The margin is a policy guard, not fitted accuracy.
        if value.map_or(current_seconds - destination_seconds, |v| {
            v.equivalent_seconds_saved
        }) < 5.0_f32.max(current_seconds * 0.2)
            || o.match_context
                .as_ref()
                .and_then(|m| m.remaining_seconds)
                .is_some_and(|left| f64::from(destination_seconds) > left)
        {
            return None;
        }
        Some(CaptureSelection {
            tick: p.tick,
            source_tick: report.source_tick,
            current,
            selected_tick: report.selected_tick?,
            site: dependencies.selected_site,
            destination,
            current_seconds,
            destination_seconds,
            value,
            landing: (mission.policy
                == crate::mission_policy::MissionPolicy::LandingPlanPlanner.id())
            .then(|| {
                let candidate = report.candidates.iter().find(|c| c.planet == destination)?;
                let sample = state.latest_evidence.iter().find(|s| {
                    Some(s.site) == candidate.site
                        && Some(s.tick) == candidate.evidence_tick
                        && s.key.planet == destination
                })?;
                Some(CostedLandingReference {
                    site: sample.site,
                    evidence_tick: sample.tick,
                    key: sample.key,
                })
            })
            .flatten(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mission_pilot::MissionEvent, mission_policy::MissionPolicy};

    fn fixture() -> (MissionEvaluator, MissionObservationV1, MissionTelemetry) {
        let (_, mut o, bot) = super::super::tests::fixture();
        o.planets.truncate(2);
        let mut mission = bot.telemetry().clone();
        mission.policy = MissionPolicy::DestinationPlanner.id();
        mission.target = Some(0);
        mission.events.push(MissionEvent {
            tick: 1,
            planet: Some(0),
            kind: "selected",
            reason: None,
        });
        let mut evaluator = MissionEvaluator::new(1);
        let evidence = o
            .planets
            .iter()
            .enumerate()
            .map(|(i, planet)| {
                let mut sample =
                    super::super::tests::known(planet, 1, if i == 0 { 90.0 } else { 10.0 });
                sample.remote = true;
                sample
            })
            .collect();
        evaluator.actors.insert(
            0,
            ActorState {
                evidence,
                ..Default::default()
            },
        );
        o.local.combat.recovery.flight.pilot.sites.clear();
        for tick in 1..=2 {
            o.local.combat.recovery.flight.pilot.tick = tick;
            evaluator.observe(&o, &mission);
            evaluator.advance(tick, DEFAULT_WORK);
        }
        assert_eq!(
            evaluator
                .latest(PlayerId::PLAYER_1)
                .unwrap()
                .preferred_by_time,
            Some(1)
        );
        assert!(evaluator.selection(&o, &mission).is_some());
        (evaluator, o, mission)
    }

    #[test]
    fn consumption_rechecks_dependencies_before_controls_without_observe_or_queries() {
        let (evaluator, original, mission) = fixture();
        for mutation in 0..15 {
            let mut o = original.clone();
            let mut m = mission.clone();
            let p = &mut o.local.combat.recovery.flight.pilot;
            match mutation {
                0 => o.planets[1].revision += 1,
                1 => o.planets[1].claim.as_mut().unwrap().owner = Some(p.owner),
                2 => o.planets[1].claim.as_mut().unwrap().stage_required_seconds += 1.0,
                3 => m.target = Some(1),
                4 => m.events.last_mut().unwrap().tick += 1,
                5 => p.ship_form = ShipForm::EscapePod,
                6 => p.ship_available = false,
                7 => p.location = PilotLocation::OnFoot,
                8 => p.queries_ready = false,
                9 => p.tick += MAX_RESULT_AGE,
                10 => p.tick = 0,
                11 => o.match_context.as_mut().unwrap().pilots_alive[0] = false,
                12 => o.match_context.as_mut().unwrap().remaining_seconds = Some(0.1),
                13 => o.planets.push(o.planets[0].clone()),
                14 => o.local.objective_work = Some(
                    scenario_spacewars::surface_sortie::live_planning::ObjectiveWorkState::Stale,
                ),
                _ => unreachable!(),
            }
            assert!(evaluator.selection(&o, &m).is_none(), "mutation {mutation}");
        }
        assert!(evaluator.selection(&original, &mission).is_some());
    }

    #[test]
    fn unknown_small_advantage_and_unfinished_comparisons_do_not_choose() {
        let (evaluator, o, mission) = fixture();
        for mutation in 0..4 {
            let mut e = evaluator.clone();
            let report = e.actors.get_mut(&0).unwrap().latest.as_mut().unwrap();
            match mutation {
                0 => report.preferred_by_time = None,
                1 => report.candidates_truncated = true,
                2 => report.completed_tick = None,
                3 => {
                    report.candidates[0].total_seconds = Some(20.0);
                    report.candidates[1].total_seconds = Some(19.0);
                }
                _ => unreachable!(),
            }
            assert!(e.selection(&o, &mission).is_none());
        }
    }

    #[test]
    fn published_evidence_is_not_renewed_by_pending_refresh() {
        let (mut evaluator, mut o, mission) = fixture();
        // Isolate evidence expiry from the shorter report lifetime, retaining
        // the report's original dependency while a fresh request is pending.
        let state = evaluator.actors.get_mut(&0).unwrap();
        state.latest_evidence[0].tick = 0;
        state.latest.as_mut().unwrap().source_tick = MAX_EVIDENCE_AGE;
        state.latest.as_mut().unwrap().completed_tick = Some(MAX_EVIDENCE_AGE);
        state.evidence[0].tick = MAX_EVIDENCE_AGE;
        state.submitted_evidence[0].tick = MAX_EVIDENCE_AGE;
        o.local.combat.recovery.flight.pilot.tick = MAX_EVIDENCE_AGE + 1;
        assert!(evaluator.selection(&o, &mission).is_none());
    }

    #[test]
    fn local_cost_consumption_rechecks_route_cover_hatch_and_ground_gravity() {
        use scenario_spacewars::surface_sortie::{combat::LandingCover, pilot::LandingSiteQuery};
        let (mut evaluator, mut o, mission) = fixture();
        o.local.combat.recovery.flight.pilot.planet = o.planets[0].clone();
        let site = super::super::tests::add_flagged_route(&mut o);
        o.planets[0] = o.local.combat.recovery.flight.pilot.planet.clone();
        // Keep the remote reference decisively cheaper; this test exercises
        // consumption of a published local route, not destination geometry.
        o.planets[1].motion.position =
            o.local.combat.recovery.flight.pilot.ship.position + Vec2::X * 200.0;
        let (_, measured, _) = super::super::tests::fixture();
        let mut landing = measured.local.combat.recovery.flight.pilot.sites[0];
        landing.id = site;
        landing.revision = o.planets[0].revision;
        landing.boarding_hatches = [Some(Vec2::Y), None];
        o.local.combat.recovery.flight.pilot.sites = vec![landing];
        o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Selected(site);
        o.local.cover = vec![LandingCover {
            site,
            grounded: true,
            approach: true,
            departure: true,
        }];
        o.local.objective_gravity = 5.0;
        for tick in 3..=4 {
            o.local.combat.recovery.flight.pilot.tick = tick;
            o.local.landing_objective.as_mut().unwrap().validated_tick = Some(tick);
            evaluator.observe(&o, &mission);
            evaluator.advance(tick, DEFAULT_WORK);
        }
        assert!(
            evaluator.selection(&o, &mission).is_some(),
            "{:?}",
            evaluator.latest(PlayerId::PLAYER_1)
        );
        assert!(
            evaluator.actors[&0]
                .latest_evidence
                .iter()
                .any(|s| !s.remote)
        );
        for mutation in 0..10 {
            let mut changed = o.clone();
            match mutation {
                0 => changed.local.objective_gravity += 0.02,
                1 => changed.local.cover[0].grounded = false,
                2 => changed.local.cover[0].approach = false,
                3 => changed.local.cover[0].departure = false,
                4 => {
                    changed.local.combat.recovery.flight.pilot.sites[0].boarding_hatches = [None; 2]
                }
                5 => changed.local.combat.recovery.flight.pilot.sites.clear(),
                6 => changed.local.landing_objective.as_mut().unwrap().tick += 1,
                7 => changed.local.landing_objective.as_mut().unwrap().sites[0].returning = None,
                8 => {
                    changed.local.landing_objective.as_mut().unwrap().sites[0]
                        .outbound
                        .length += 1.0
                }
                9 => changed.local.combat.recovery.flight.pilot.planet.revision += 1,
                _ => unreachable!(),
            }
            assert!(
                evaluator.selection(&changed, &mission).is_none(),
                "mutation {mutation}"
            );
        }
        // Ordinary ship gravity drift and a short route-survey gap are allowed,
        // but cannot renew the route or conceal newly blocked boarding/cover.
        o.local.combat.recovery.flight.pilot.gravity += Vec2::X * 100.0;
        o.local.landing_objective = None;
        assert!(evaluator.selection(&o, &mission).is_some());
        o.local.cover[0].departure = false;
        assert!(evaluator.selection(&o, &mission).is_none());
        o.local.cover[0].departure = true;
        o.local.combat.recovery.flight.pilot.tick = 32;
        assert!(evaluator.selection(&o, &mission).is_none());
    }
    #[test]
    fn refused_handoff_revokes_ready_and_pending_costs_without_waiting_for_refresh() {
        for ready in [false, true] {
            let (mut e, mut o, mut m) = fixture();
            m.policy = MissionPolicy::LandingPlanPlanner.id();
            for tick in 3..=4 {
                o.local.combat.recovery.flight.pilot.tick = tick;
                e.observe(&o, &m);
                if ready {
                    e.advance(tick, DEFAULT_WORK);
                }
            }
            assert_eq!(e.latest(PlayerId::PLAYER_1).is_some(), ready);
            let old = e.actors[&0]
                .evidence
                .iter()
                .find(|s| s.key.planet == 0)
                .unwrap()
                .clone();
            let old_pending = e.actors[&0].pending;
            m.destination_planning = Some(crate::mission_pilot::DestinationPlanningTelemetry {
                landing_handoff: Some(crate::mission_pilot::LandingHandoff {
                    site: old.site,
                    evidence_tick: old.tick,
                    source_tick: 1,
                    switch_tick: 1,
                    started_tick: Some(3),
                    accepted_tick: None,
                    landed_tick: None,
                    completed_tick: None,
                    invalidated_tick: Some(5),
                    reason: Some("test native refusal"),
                }),
                ..Default::default()
            });
            o.local.combat.recovery.flight.pilot.tick = 5;
            assert!(e.selection(&o, &m).is_none());
            e.observe(&o, &m);
            e.advance(5, Work::default());
            assert!(e.latest(PlayerId::PLAYER_1).is_none());
            if let Some(old) = old_pending {
                assert_ne!(e.actors[&0].pending, Some(old));
            }
            for tick in 6..=7 {
                e.advance(tick, DEFAULT_WORK);
            }
            assert!(
                e.latest(PlayerId::PLAYER_1)
                    .unwrap()
                    .candidates
                    .iter()
                    .find(|c| c.current)
                    .unwrap()
                    .total_seconds
                    .is_none()
            );
            assert!(!supports_handoff(&m, &old));
            let mut native = old.clone();
            native.remote = false;
            native.tick = 5;
            native.choice = Some((1, 5));
            let mut capture = crate::tactical_capture::TacticalCapturePilot::with_planning(
                crate::BrainReset { actor: PlayerId::PLAYER_1, episode_seed: 42 }, Default::default(),
                scenario_spacewars::surface_sortie::landing_objective::ObjectivePlanning::JointRoundTrip
            ).telemetry().clone();
            capture.sortie.site = Some(native.site);
            m.capture = Some(capture);
            assert!(supports_handoff(&m, &native));
            native.tick = 4;
            assert!(!supports_handoff(&m, &native));
            m.capture = None;
            // The refusal is scoped to this visit, not a permanent blacklist.
            m.events.push(MissionEvent {
                tick: 8,
                planet: Some(0),
                kind: "selected",
                reason: None,
            });
            assert!(supports_handoff(&m, &old));
            e.actors.get_mut(&0).unwrap().evidence.push(old);
            o.local.combat.recovery.flight.pilot.tick = 8;
            e.observe(&o, &m);
            for tick in 8..=9 {
                e.advance(tick, DEFAULT_WORK);
            }
            assert!(
                e.latest(PlayerId::PLAYER_1)
                    .unwrap()
                    .candidates
                    .iter()
                    .find(|c| c.current)
                    .unwrap()
                    .total_seconds
                    .is_some()
            );
        }
    }
    #[test]
    fn landed_reference_allows_own_capture_but_rejects_unrelated_objective_changes() {
        use scenario_spacewars::surface_sortie::{PlanetClaimPhase, PlanetFlagObservation};
        let (_, mut o, _) = fixture();
        let actor = o.local.combat.recovery.flight.pilot.owner;
        let site = LandingSiteId {
            planet: 0,
            bearing: 0,
        };
        let neutral = CostedLandingReference::fixture(&o.planets[0], site, 1);
        let flag = PlanetFlagObservation {
            player: actor.opponent(),
            position: o.planets[0].motion.position + Vec2::Y * o.planets[0].radius,
            normal: Vec2::Y,
            raised_fraction: 1.0,
        };
        let claim = o.planets[0].claim.as_mut().unwrap();
        claim.owner = Some(actor.opponent());
        claim.flag = Some(flag);
        assert!(neutral.rejection(&o, true, true).is_some());
        let enemy = CostedLandingReference::fixture(&o.planets[0], site, 1);
        o.planets[0]
            .claim
            .as_mut()
            .unwrap()
            .flag
            .as_mut()
            .unwrap()
            .position
            .x += 1.0;
        assert!(enemy.rejection(&o, true, true).is_some());
        let claim = o.planets[0].claim.as_mut().unwrap();
        claim.flag = Some(flag);
        claim.claimant = Some(actor);
        claim.phase = PlanetClaimPhase::Lowering;
        assert!(enemy.rejection(&o, true, true).is_none());
        let claim = o.planets[0].claim.as_mut().unwrap();
        claim.owner = None;
        claim.phase = PlanetClaimPhase::Raising;
        claim.flag.as_mut().unwrap().player = actor;
        assert!(enemy.rejection(&o, true, true).is_none());
        let claim = o.planets[0].claim.as_mut().unwrap();
        claim.owner = Some(actor);
        claim.phase = PlanetClaimPhase::Idle;
        claim.claimant = None;
        assert!(enemy.rejection(&o, true, true).is_none());
        o.planets[0].claim.as_mut().unwrap().claimant = Some(actor.opponent());
        assert!(enemy.rejection(&o, true, true).is_some());
        o.planets[0].claim.as_mut().unwrap().claimant = None;
        o.planets[0].revision += 1;
        assert!(enemy.rejection(&o, true, true).is_some());
    }
}
