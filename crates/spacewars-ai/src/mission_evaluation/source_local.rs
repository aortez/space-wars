//! Read-only local references captured at a transfer comparison's source.
//! These retain measurement ages; a completed evaluator report is never input.
use super::*;
use scenario_spacewars::surface_sortie::{
    landing_objective::LandingObjective, pilot::LandingSiteQuery,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalEvidenceSource {
    pub site: LandingSiteId,
    pub revision: u64,
    pub observed_owner: Option<PlayerId>,
    pub radius: f32,
    pub stage_seconds: f32,
    pub flag_range: f32,
    pub remote: bool,
    pub tick: u64,
    pub age_ticks: u64,
    pub gravity: f32,
    pub route_source_tick: Option<u64>,
    pub route_validated_tick: Option<u64>,
    pub route_objective: Option<LandingObjective>,
    pub choice: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalCostReference {
    pub model: &'static str,
    pub source_tick: u64,
    pub destination: usize,
    pub visit_tick: u64,
    pub selected_site: Option<LandingSiteId>,
    pub observed_choice_tick: Option<u64>,
    pub evidence: Option<LocalEvidenceSource>,
    /// Successful-trip medians conditional on using the recorded site. Neither
    /// the transfer endpoint nor a nomination commits the pilot to that site.
    pub full: Option<PhaseCosts>,
    pub remaining: Option<PhaseCosts>,
    pub elapsed_landing_ticks: u64,
    pub unknown: Option<&'static str>,
}

impl MissionEvaluator {
    /// Snapshot the existing empirical phases at the first native site choice.
    /// Read-only diagnostic: the host binds this to the same observation and
    /// landing-choice report. Direction/arrival geometry do not tune the medians.
    pub fn fresh_capture_reference(
        &self,
        o: &MissionObservationV1,
        mission: &MissionTelemetry,
    ) -> Result<LocalCostReference, &'static str> {
        let p = &o.local.combat.recovery.flight.pilot;
        let capture = mission.capture.as_ref().ok_or("capture unavailable")?;
        let acquisition = capture.acquisition.ok_or("native choice unavailable")?;
        let site = capture.site.ok_or("selected site unavailable")?;
        let visit = selection_tick(mission).ok_or("capture visit unavailable")?;
        if mission.goal != crate::mission_pilot::MissionGoal::Capture
            || mission.target != Some(p.planet.index)
            || mission.recovery.is_some()
            || capture.started_tick.is_none_or(|tick| tick > p.tick)
            || capture.failed_tick.is_some()
            || capture.landing.landed_tick.is_some()
            || acquisition.reason != "selected_site"
            || acquisition.tick != p.tick
            || acquisition.planet != p.planet.index
            || acquisition.revision != p.planet.revision
            || acquisition.selected_site != Some(site)
            || site.planet != p.planet.index
        {
            return Err("not the first current native capture choice");
        }
        let report = self.source_local_reference(o, mission, p.planet.index, visit, true);
        if report.observed_choice_tick != Some(p.tick) || report.elapsed_landing_ticks != 0 {
            return Err(report
                .unknown
                .unwrap_or("fresh choice reference clock unavailable"));
        }
        Ok(report)
    }

    pub(crate) fn source_local_reference(
        &self,
        o: &MissionObservationV1,
        mission: &MissionTelemetry,
        destination: usize,
        visit_tick: u64,
        continuing: bool,
    ) -> LocalCostReference {
        let p = &o.local.combat.recovery.flight.pilot;
        let selected = mission.capture.as_ref().and_then(|c| c.site);
        let mut report = LocalCostReference {
            model: "source_local_reference_v1",
            source_tick: p.tick,
            destination,
            visit_tick,
            selected_site: selected,
            observed_choice_tick: None,
            evidence: None,
            full: None,
            remaining: None,
            elapsed_landing_ticks: 0,
            unknown: None,
        };
        let result = (|| {
            let planet = o
                .planets
                .iter()
                .take(MAX_PLANETS)
                .find(|v| v.index == destination)
                .ok_or("local planet absent")?;
            let claim = planet.claim.as_ref().ok_or("local ownership unknown")?;
            if claim.owner == Some(p.owner) {
                return Err("local planet already owned");
            }
            if !p.queries_ready
                || !p.ship_available
                || p.ship_form != ShipForm::Ship
                || !matches!(p.location, PilotLocation::Aboard(_))
                || visit_tick > p.tick
            {
                return Err("local source state unsupported");
            }
            let key = PlanetKey::read(planet);
            let valid = |s: &&LocalEvidence| {
                s.tick <= p.tick
                    && p.tick - s.tick <= MAX_EVIDENCE_AGE
                    && s.key.reference_matches(&key)
                    && (s.remote
                        || (destination == p.planet.index
                            && s.gravity.is_finite()
                            && o.local.objective_gravity.is_finite()
                            && (s.gravity - o.local.objective_gravity).abs() <= 0.01))
                    && (claim.flag.is_none()
                        || (s
                            .route_source_tick
                            .is_some_and(|t| t <= s.tick && p.tick - t <= MAX_EVIDENCE_AGE)
                            && s.route_validated_tick.is_some_and(|t| {
                                t <= s.tick && s.route_source_tick.is_some_and(|start| start <= t)
                            })
                            && s.route_objective
                                .is_some_and(|obj| key.flag_identity_matches(obj, planet.radius))))
            };
            let state = self.actors.get(&(p.owner.index() as u64));
            if state.is_some_and(|s| s.last_tick.is_some_and(|t| t > p.tick)) {
                return Err("local evaluator source is in the future");
            }
            let old = state
                .and_then(|s| {
                    s.evidence
                        .iter()
                        .take(MAX_PLANETS)
                        .find(|e| e.key.planet == destination)
                })
                .filter(valid);
            let choice = continuing
                .then(|| {
                    LocalChoice::observe(
                        state.and_then(|s| s.local_choice.as_ref()),
                        o,
                        mission,
                        Some(visit_tick),
                    )
                })
                .flatten();
            report.observed_choice_tick = choice.as_ref().map(|c| c.first_tick);
            // Fresh local negatives replace old positives. A cadenced route
            // omission alone may retain its original source and validation age.
            let local = destination == p.planet.index;
            if local && claim.flag.is_some()
                && o.local.objective_work == Some(scenario_spacewars::surface_sortie::live_planning::ObjectiveWorkState::Stale) {
                return Err("local objective work stale");
            }
            let fresh = if local {
                model::observe_local(o, mission)
            } else {
                state
                    .and_then(|s| survey::evidence(&s.survey, o))
                    .filter(|s| s.key.planet == destination)
            };
            let sample = match fresh {
                Some(mut sample) => {
                    if let Some(old) = old.filter(|s| s.site == sample.site)
                        && sample.reason == Some("objective route unmeasured")
                        && model::route_cadence_gap(o, old)
                    {
                        sample = old.clone();
                    }
                    if !valid(&&sample) {
                        return Err("local source identity invalid");
                    }
                    sample
                }
                None => {
                    if local
                        && !p.site_query.is_deferred()
                        && p.site_query != LandingSiteQuery::NotRequested
                    {
                        return Err("requested local site or cover unavailable");
                    }
                    let old = old.ok_or("compatible local evidence unavailable at source")?;
                    if local
                        && claim.flag.is_some()
                        && !model::route_cadence_gap(o, old)
                        && o.local.landing_objective.as_ref().is_none_or(|s| {
                            Some(s.tick) != old.route_source_tick
                                || !s.is_current(p.tick)
                                || !key.flag_identity_matches(s.objective, planet.radius)
                                || model::local_costs(o, old.site).ok() != old.costs
                        })
                    {
                        return Err("local route unavailable or changed");
                    }
                    old.clone()
                }
            };
            if selected.is_some_and(|site| site != sample.site) {
                return Err("selected capture site has no matching local reference");
            }
            report.evidence = Some(LocalEvidenceSource {
                site: sample.site,
                revision: planet.revision,
                observed_owner: claim.owner,
                radius: planet.radius,
                stage_seconds: claim.stage_required_seconds,
                flag_range: claim.flag_interaction_range - 0.2,
                remote: sample.remote,
                tick: sample.tick,
                age_ticks: p.tick - sample.tick,
                gravity: sample.gravity,
                route_source_tick: sample.route_source_tick,
                route_validated_tick: sample.route_validated_tick,
                route_objective: sample.route_objective,
                choice: sample.choice,
            });
            if let Some(reason) = sample.reason {
                return Err(reason);
            }
            let full = sample.costs.ok_or("local phase costs unmeasured")?;
            if ![
                full.landing,
                full.exit,
                full.outbound,
                full.claim,
                full.return_board,
                full.departure,
            ]
            .into_iter()
            .all(|v| v.is_finite() && v >= 0.0)
                || !full.total().is_finite()
            {
                return Err("local phase costs invalid");
            }
            let mut remaining = full.clone();
            if selected == Some(sample.site)
                && let Some(choice) = choice
            {
                report.elapsed_landing_ticks = p.tick - choice.first_tick;
                remaining.landing =
                    (full.landing - report.elapsed_landing_ticks as f32 / 60.0).max(0.0);
            }
            report.full = Some(full);
            report.remaining = Some(remaining);
            Ok(())
        })();
        report.unknown = result.err();
        report
    }
}

/// Independent diagnostic clock. A missing observation, site, visit, material
/// or gravity change resets it; the ordinary evaluator's cached choice is not
/// proof of uninterrupted site selection. Never read by playing selection.
#[derive(Clone)]
pub(super) struct LocalChoice {
    first_tick: u64,
    observed_tick: u64,
    visit: u64,
    site: LandingSiteId,
    key: PlanetKey,
    gravity: f32,
}
impl LocalChoice {
    pub(super) fn observe(
        old: Option<&Self>,
        o: &MissionObservationV1,
        mission: &MissionTelemetry,
        visit: Option<u64>,
    ) -> Option<Self> {
        let p = &o.local.combat.recovery.flight.pilot;
        let capture = mission.capture.as_ref()?;
        let site = capture.site?;
        let visit = visit.filter(|t| *t <= p.tick)?;
        if mission.goal != crate::mission_pilot::MissionGoal::Capture
            || mission.target != Some(p.planet.index)
            || site.planet != p.planet.index
            || capture.landing.landed_tick.is_some()
            || capture.failure.is_some()
            || !matches!(
                capture.goal,
                crate::tactical_sortie::TacticalGoal::Survey
                    | crate::tactical_sortie::TacticalGoal::SeekCover
                    | crate::tactical_sortie::TacticalGoal::Approach
            )
        {
            return None;
        }
        let key = PlanetKey::read(&p.planet);
        let old = old.filter(|c| {
            c.observed_tick <= p.tick
                && p.tick - c.observed_tick <= 1
                && c.visit == visit
                && c.site == site
                && c.key.reference_matches(&key)
                && (c.gravity - o.local.objective_gravity).abs() <= 0.01
        });
        Some(Self {
            first_tick: old.map_or(p.tick, |c| c.first_tick),
            observed_tick: p.tick,
            visit,
            site,
            key: old.map_or(key, |c| c.key),
            gravity: old.map_or(o.local.objective_gravity, |c| c.gravity),
        })
    }
}

/// Extra dependencies of the local join; existing flight-only tokens keep
/// their original validation. This is historical compatibility, not live
/// permission at a predicted endpoint or a way to rebase remaining times.
#[derive(Clone)]
pub(crate) struct LocalReferenceContext {
    planets: Vec<PlanetKey>,
    gravity: Option<(usize, f32)>,
    evidence_deadline: Option<u64>,
}
impl LocalReferenceContext {
    pub(crate) fn read<'a>(
        o: &MissionObservationV1,
        references: impl Iterator<Item = &'a LocalCostReference>,
    ) -> Self {
        let mut gravity = None;
        let mut evidence_deadline = None;
        for r in references.filter(|r| r.remaining.is_some()) {
            if let Some(e) = &r.evidence {
                let tick = e.route_source_tick.map_or(e.tick, |t| t.min(e.tick));
                let deadline = tick.saturating_add(MAX_EVIDENCE_AGE);
                evidence_deadline =
                    Some(evidence_deadline.map_or(deadline, |old: u64| old.min(deadline)));
                if !e.remote {
                    gravity = Some((e.site.planet, e.gravity));
                }
            }
        }
        Self {
            planets: o
                .planets
                .iter()
                .take(MAX_PLANETS)
                .map(PlanetKey::read)
                .collect(),
            gravity,
            evidence_deadline,
        }
    }
    pub(crate) fn matches(&self, o: &MissionObservationV1) -> bool {
        self.planets.len() == o.planets.len()
            && self
                .planets
                .iter()
                .zip(&o.planets)
                .all(|(a, b)| a.reference_matches(&PlanetKey::read(b)))
            && self
                .evidence_deadline
                .is_none_or(|t| o.local.combat.recovery.flight.pilot.tick <= t)
            && self.gravity.is_none_or(|(frame, gravity)| {
                frame == o.local.combat.recovery.flight.pilot.planet.index
                    && gravity.is_finite()
                    && o.local.objective_gravity.is_finite()
                    && (gravity - o.local.objective_gravity).abs() <= 0.01
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BrainReset,
        mission_pilot::{MissionEvent, MissionGoal},
        tactical_capture::TacticalCapturePilot,
    };
    use scenario_spacewars::surface_sortie::combat::LandingCover;

    fn fixture() -> (MissionEvaluator, MissionObservationV1, MissionTelemetry) {
        let (_, mut o, bot) = super::super::tests::fixture();
        o.local.combat.recovery.flight.pilot.tick = 100;
        let site = super::super::tests::add_flagged_route(&mut o);
        let p = &mut o.local.combat.recovery.flight.pilot;
        o.planets[p.planet.index] = p.planet.clone();
        let mut landing = p.sites[0];
        landing.id = site;
        landing.revision = p.planet.revision;
        landing.boarding_hatches = [Some(Vec2::Y), None];
        p.sites = vec![landing];
        p.site_query = LandingSiteQuery::Selected(site);
        o.local.cover = vec![LandingCover {
            site,
            grounded: true,
            approach: true,
            departure: true,
        }];
        o.local.objective_gravity = 5.0;
        let mut mission = bot.telemetry().clone();
        mission.target = Some(p.planet.index);
        mission.goal = MissionGoal::Capture;
        mission.events.push(MissionEvent {
            tick: 90,
            planet: mission.target,
            kind: "selected",
            reason: None,
        });
        let mut capture = TacticalCapturePilot::new(
            BrainReset {
                actor: p.owner,
                episode_seed: 42,
            },
            Default::default(),
        )
        .telemetry()
        .clone();
        capture.sortie.site = Some(site);
        mission.capture = Some(capture);
        let mut evaluator = MissionEvaluator::new(1);
        evaluator.observe(&o, &mission);
        (evaluator, o, mission)
    }
    fn reference(
        e: &MissionEvaluator,
        o: &MissionObservationV1,
        m: &MissionTelemetry,
    ) -> LocalCostReference {
        e.source_local_reference(o, m, m.target.unwrap(), selection_tick(m).unwrap(), true)
    }
    fn tick(o: &mut MissionObservationV1, tick: u64) {
        o.local.combat.recovery.flight.pilot.tick = tick;
        if let Some(s) = &mut o.local.landing_objective {
            s.validated_tick = Some(tick);
        }
    }

    fn fresh_fixture() -> (MissionEvaluator, MissionObservationV1, MissionTelemetry) {
        use crate::tactical_sortie::AcquisitionTelemetry;
        let (e, o, mut m) = fixture();
        let p = &o.local.combat.recovery.flight.pilot;
        let c = &mut m.capture.as_mut().unwrap().sortie;
        c.started_tick = Some(p.tick);
        c.acquisition = Some(AcquisitionTelemetry {
            tick: p.tick,
            planet: p.planet.index,
            revision: p.planet.revision,
            objective: None,
            measurement_tick: None,
            generation: None,
            objective_work: None,
            site_query: p.site_query,
            sites_available: p.sites.len(),
            required_site: None,
            selected_site: c.site,
            reason: "selected_site",
            survey_rejected_by: None,
            checks: Default::default(),
        });
        (e, o, m)
    }

    #[test]
    fn fresh_capture_snapshot_preserves_existing_costs_and_evaluator_work() {
        let (e, o, m) = fresh_fixture();
        let before = reference(&e, &o, &m);
        let pending = e.pending(PlayerId::PLAYER_1);
        let r = e.fresh_capture_reference(&o, &m).unwrap();
        assert_eq!(r, before);
        assert!(r.full.is_some());
        assert_eq!(r.full, r.remaining);
        assert_eq!(r.elapsed_landing_ticks, 0);
        assert_eq!(r.observed_choice_tick, Some(100));
        assert_eq!(e.pending(PlayerId::PLAYER_1), pending);
        assert_eq!(reference(&e, &o, &m), before);
    }

    #[test]
    fn fresh_capture_snapshot_rejects_retained_mismatched_and_progressed_choices() {
        for mutation in 0..7 {
            let (e, o, mut m) = fresh_fixture();
            let c = &mut m.capture.as_mut().unwrap().sortie;
            match mutation {
                0 => c.acquisition.as_mut().unwrap().reason = "retained_site",
                1 => c.acquisition.as_mut().unwrap().tick -= 1,
                2 => c.acquisition.as_mut().unwrap().revision += 1,
                3 => c.acquisition.as_mut().unwrap().selected_site = None,
                4 => c.started_tick = Some(101),
                5 => c.landing.landed_tick = Some(100),
                _ => m.target = None,
            }
            assert!(
                e.fresh_capture_reference(&o, &m).is_err(),
                "mutation {mutation}"
            );
        }
    }

    #[test]
    fn delayed_first_choice_starts_its_cost_clock_at_selection() {
        let (e, o, mut m) = fresh_fixture();
        m.capture.as_mut().unwrap().sortie.started_tick = Some(95);
        let r = e.fresh_capture_reference(&o, &m).unwrap();
        assert_eq!(r.observed_choice_tick, Some(100));
        assert_eq!(r.elapsed_landing_ticks, 0);
        assert_eq!(r.full, r.remaining);
    }

    #[test]
    fn fresh_capture_snapshot_preserves_unknown_costs() {
        let (e, mut o, m) = fresh_fixture();
        o.local.cover.clear();
        let r = e.fresh_capture_reference(&o, &m).unwrap();
        assert!(r.full.is_none() && r.remaining.is_none());
        assert!(r.unknown.is_some());
        assert_eq!(r.observed_choice_tick, Some(100));
    }

    #[test]
    fn same_source_reads_raw_evidence_without_draining_or_using_old_reports() {
        let (mut e, mut o, m) = fixture();
        let original = reference(&e, &o, &m);
        assert!(original.remaining.is_some(), "{original:?}");
        assert!(e.pending(PlayerId::PLAYER_1));
        e.advance(
            100,
            Work {
                graph: 20,
                physics_queries: 0,
            },
        );
        e.advance(
            101,
            Work {
                graph: 20,
                physics_queries: 0,
            },
        );
        let s = e.actors.get_mut(&0).unwrap();
        let published = s.latest.as_mut().unwrap();
        published.source_tick = 1;
        for c in &mut published.candidates {
            c.local = None;
            c.total_seconds = Some(9999.0);
        }
        for t in 101..=105 {
            tick(&mut o, t);
            e.observe(&o, &m);
        }
        tick(&mut o, 106);
        let charged = e.charged_total;
        let r = reference(&e, &o, &m);
        assert_eq!(e.charged_total, charged);
        assert_eq!(r.source_tick, 106);
        assert_eq!(r.observed_choice_tick, Some(100));
        assert_eq!(r.elapsed_landing_ticks, 6);
        assert_eq!(r.full, original.full);
        assert_eq!(
            r.remaining.unwrap().landing,
            original.full.unwrap().landing - 0.1
        );
        assert_eq!(r.evidence.unwrap().route_source_tick, Some(100));
    }

    #[test]
    fn fresh_route_does_not_replace_original_flag_identity_or_renew_its_age() {
        let (e, original, m) = fixture();
        for mutation in 0..9 {
            let mut o = original.clone();
            let s = o.local.landing_objective.as_mut().unwrap();
            match mutation {
                0 => s.objective.position.x += 0.1,
                1 => s.objective.range += 0.001,
                2 => s.tick += 1,
                3 => s.validated_tick = Some(101),
                4 => s.validated_tick = Some(99),
                5 => s.actor = PlayerId::PLAYER_2,
                6 => s.version += 1,
                7 => {
                    tick(&mut o, 1901);
                }
                _ => s.sites[0].outbound.jumps = 1,
            }
            let r = reference(&e, &o, &m);
            assert!(r.remaining.is_none(), "mutation {mutation}: {r:?}");
            assert!(r.unknown.is_some());
        }
    }

    #[test]
    fn measured_negatives_and_stale_work_do_not_revive_cached_positives() {
        let (e, original, m) = fixture();
        for mutation in 0..4 {
            let mut o = original.clone();
            tick(&mut o, 101);
            match mutation {
                0 => o.local.cover.clear(),
                1 => o.local.combat.recovery.flight.pilot.sites.clear(),
                2 => o.local.combat.recovery.flight.pilot.sites[0].boarding_hatches = [None; 2],
                _ => {
                    o.local.landing_objective = None;
                    o.local.objective_work = Some(scenario_spacewars::surface_sortie::live_planning::ObjectiveWorkState::Stale);
                }
            }
            assert!(
                reference(&e, &o, &m).remaining.is_none(),
                "mutation {mutation}"
            );
        }
        let mut gap = original;
        gap.local.landing_objective = None;
        tick(&mut gap, 101);
        let r = reference(&e, &gap, &m);
        assert!(r.remaining.is_some(), "{r:?}");
        assert_eq!(r.evidence.as_ref().unwrap().tick, 100);
        assert_eq!(r.evidence.as_ref().unwrap().route_validated_tick, Some(100));
        gap.local.combat.recovery.flight.pilot.sites.clear();
        gap.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::NotRequested;
        assert!(reference(&e, &gap, &m).remaining.is_some());
        tick(&mut gap, 130);
        assert!(reference(&e, &gap, &m).remaining.is_none());
    }

    #[test]
    fn new_visits_site_changes_and_observation_gaps_never_inherit_elapsed_landing() {
        let (mut e, mut o, mut m) = fixture();
        tick(&mut o, 101);
        assert_eq!(reference(&e, &o, &m).elapsed_landing_ticks, 1);
        let nominated = e.source_local_reference(&o, &m, m.target.unwrap(), 101, false);
        assert_eq!(nominated.elapsed_landing_ticks, 0);
        assert!(nominated.observed_choice_tick.is_none());
        m.events.last_mut().unwrap().tick = 101;
        assert_eq!(reference(&e, &o, &m).elapsed_landing_ticks, 0);
        m.events.last_mut().unwrap().tick = 90;
        let site = m.capture.as_mut().unwrap().sortie.site.take();
        e.observe(&o, &m);
        m.capture.as_mut().unwrap().sortie.site = site;
        tick(&mut o, 102);
        assert_eq!(reference(&e, &o, &m).elapsed_landing_ticks, 0);
        tick(&mut o, 104);
        assert_eq!(reference(&e, &o, &m).elapsed_landing_ticks, 0);
    }

    #[test]
    fn reference_context_pins_expiry_flag_radius_and_observable_gravity() {
        let (e, o, m) = fixture();
        let r = reference(&e, &o, &m);
        let context = LocalReferenceContext::read(&o, [&r].into_iter());
        assert!(context.matches(&o));
        let mut copy = o.clone();
        tick(&mut copy, 1900);
        assert!(context.matches(&copy));
        tick(&mut copy, 1901);
        assert!(!context.matches(&copy));
        for mutation in 0..6 {
            let mut o = o.clone();
            let i = m.target.unwrap();
            match mutation {
                0 => {
                    o.planets[i]
                        .claim
                        .as_mut()
                        .unwrap()
                        .flag
                        .as_mut()
                        .unwrap()
                        .position
                        .x += 0.1
                }
                1 => o.planets[i].claim.as_mut().unwrap().flag_interaction_range += 0.001,
                2 => o.planets[i].radius += 0.1,
                3 => o.planets[i].claim.as_mut().unwrap().stage_required_seconds += 0.001,
                4 => o.local.objective_gravity += 0.011,
                _ => {
                    o.local.combat.recovery.flight.pilot.planet =
                        o.planets[(i + 1) % o.planets.len()].clone()
                }
            }
            assert!(!context.matches(&o), "mutation {mutation}");
        }
        let mut elsewhere = o;
        elsewhere.local.combat.recovery.flight.pilot.planet =
            elsewhere.planets[(m.target.unwrap() + 1) % elsewhere.planets.len()].clone();
        assert!(reference(&e, &elsewhere, &m).remaining.is_none());
    }

    #[test]
    fn cached_gravity_is_not_rebased_at_the_comparison_source() {
        let (e, mut o, m) = fixture();
        tick(&mut o, 101);
        o.local.landing_objective = None;
        o.local.objective_gravity = 5.009;
        let r = reference(&e, &o, &m);
        assert!(r.remaining.is_some());
        assert_eq!(r.evidence.as_ref().unwrap().gravity, 5.0);
        let context = LocalReferenceContext::read(&o, [&r].into_iter());
        assert!(context.matches(&o));
        o.local.objective_gravity = 5.018;
        assert!(!context.matches(&o));
    }
}
