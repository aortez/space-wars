//! A forecast suggests where to measure; it never supplies terrain evidence.
use super::*;
use scenario_spacewars::surface_sortie::destination_cover::DestinationCoverRequest;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArrivalSurveyPlan {
    pub token: RequestToken,
    pub source_tick: u64,
    pub completed_tick: u64,
    pub forecast_model: &'static str,
    pub predicted_arrival_tick: u64,
    pub site: LandingSiteId,
    pub request: Option<DestinationCoverRequest>,
    pub deferred: Option<&'static str>,
}

pub(super) fn blocked(o: &MissionObservationV1) -> Option<&'static str> {
    let p = &o.local.combat.recovery.flight.pilot;
    if p.site_query != LandingSiteQuery::NotRequested || p.landing.supported_feet > 0 {
        Some("local landing demand")
    } else if !p.queries_ready {
        Some("material queries unavailable")
    } else {
        None
    }
}

fn predicted_site(forecast: &TransferForecastReport) -> Option<LandingSiteId> {
    if forecast.end != Some(TransferForecastEnd::KinematicHandoff) {
        return None;
    }
    let sample = forecast.samples.last()?;
    if sample.after_ticks != forecast.ticks {
        return None;
    }
    let local =
        (sample.ship.position - sample.target.position).rotate_radians(-sample.target.angle);
    if !local.x.is_finite() || !local.y.is_finite() || local.length_squared() == 0.0 {
        return None;
    }
    let bearing = ((-local.x).atan2(local.y).rem_euclid(std::f32::consts::TAU)
        * f32::from(LANDING_SITE_COUNT)
        / std::f32::consts::TAU)
        .round() as u8
        % LANDING_SITE_COUNT;
    Some(LandingSiteId {
        planet: forecast.destination,
        bearing,
    })
}

impl TransferForecastQueue<TransferComparisonJob> {
    /// Call after this tick's real observation, before dispatch. Only an earlier
    /// completed comparison may request a later, separately timestamped query.
    /// The current neutral destination takes priority over neutral alternatives.
    pub fn arrival_survey(&mut self, token: RequestToken, tick: u64) -> Option<ArrivalSurveyPlan> {
        if !matches!(self.poll(token, tick), JobPoll::Ready(_)) {
            return None;
        }
        let slot = self.actors.get_mut(&token.actor)?;
        let completed_tick = slot.state.completed_tick.filter(|&t| t < tick)?;
        let source = slot.source.as_ref()?;
        let report = self.queue.job(token)?.output()?;
        let (forecast, site) = select(report, &source.planets)?;
        let request = if source.survey_blocked.is_none() {
            Some(*slot.survey_request.get_or_insert(DestinationCoverRequest {
                generation: tick,
                candidates: [Some(site), None, None, None],
                sample_climb: true,
            }))
        } else {
            // Keep identity through temporary deferral. This does not renew
            // either the source lifetime or any actual measurement epoch.
            None
        };
        Some(ArrivalSurveyPlan {
            token,
            source_tick: report.source_tick,
            completed_tick,
            forecast_model: forecast.model,
            predicted_arrival_tick: report.source_tick.checked_add(forecast.ticks)?,
            site,
            request,
            deferred: source.survey_blocked,
        })
    }
}

fn select<'a>(
    report: &'a TransferComparisonReport,
    planets: &[PilotPlanetObservation],
) -> Option<(&'a TransferForecastReport, LandingSiteId)> {
    report
        .candidates
        .iter()
        .filter(|c| {
            planets.iter().any(|p| {
                p.index == c.destination
                    && p.claim
                        .as_ref()
                        .is_some_and(|claim| claim.owner.is_none() && claim.flag.is_none())
            })
        })
        .filter_map(|c| {
            let forecast = c.forecast.as_ref()?;
            Some((c, forecast, predicted_site(forecast)?))
        })
        .min_by_key(|(c, _, _)| (!c.current, c.destination))
        .map(|(_, forecast, site)| (forecast, site))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_evaluation::MissionEvaluator;

    fn fixture() -> (
        TransferComparisonQueue,
        RequestToken,
        MaterialMissionPilot,
        MissionObservationV1,
        TransferEnvironment,
    ) {
        let (state, before, bot, mut o) = transfer_forecast::tests::source_with_before();
        // Isolate request selection from ownership policy. Flight/environment
        // come from the physical fixture; all candidates are neutral here.
        for planet in &mut o.planets {
            let c = planet.claim.as_mut().unwrap();
            c.owner = None;
            c.flag = None;
        }
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.planet = o.planets[p.planet.index].clone();
        p.site_query = LandingSiteQuery::NotRequested;
        let e = state.transfer_environment().unwrap();
        let mut q = TransferComparisonQueue::new(1);
        let t = q
            .submit_comparison(
                &before,
                &bot,
                &o,
                &MissionEvaluator::new(1),
                e.clone(),
                Some(false),
            )
            .unwrap();
        (q, t, bot, o, e)
    }

    fn next(
        q: &mut TransferComparisonQueue,
        t: RequestToken,
        bot: &mut MaterialMissionPilot,
        o: &mut MissionObservationV1,
        e: &mut TransferEnvironment,
    ) {
        super::super::tests::next(bot, o, e);
        q.observe(t, bot, o, e, Some(false));
    }

    fn ready(q: &mut TransferComparisonQueue, tick: u64) {
        q.advance(
            tick,
            Work {
                graph: 10801,
                physics_queries: 0,
            },
        );
        assert_eq!(
            q.state(PlayerId::PLAYER_1).unwrap().phase,
            TransferQueuePhase::Ready
        );
    }

    #[test]
    fn only_prior_completed_currently_validated_forecasts_issue_stable_requests() {
        let (mut q, t, mut bot, mut o, mut e) = fixture();
        assert!(q.arrival_survey(t, e.tick).is_none());
        ready(&mut q, e.tick);
        assert!(q.arrival_survey(t, e.tick).is_none());
        let source_tick = e.tick;
        next(&mut q, t, &mut bot, &mut o, &mut e);
        let first = q.arrival_survey(t, e.tick).unwrap();
        assert_eq!(first.site.planet, bot.telemetry.target.unwrap());
        assert_eq!(first.source_tick, source_tick);
        assert_eq!(first.completed_tick, source_tick);
        assert_eq!(first.request.unwrap().generation, source_tick + 1);
        assert_eq!(
            first.request.unwrap().candidates.iter().flatten().count(),
            1
        );
        let charged = q.charged_total;
        next(&mut q, t, &mut bot, &mut o, &mut e);
        assert_eq!(q.arrival_survey(t, e.tick), Some(first));
        assert_eq!(q.charged_total, charged);
        // Calling without the current observation permanently retires this token.
        assert!(q.arrival_survey(t, e.tick + 1).is_none());
        assert!(q.arrival_survey(t, e.tick).is_none());
    }

    #[test]
    fn local_work_and_dirty_queries_defer_without_backdating_requests() {
        for local in [true, false] {
            let (mut q, t, mut bot, mut o, mut e) = fixture();
            ready(&mut q, e.tick);
            let p = &mut o.local.combat.recovery.flight.pilot;
            if local {
                p.site_query = LandingSiteQuery::Selected(LandingSiteId {
                    planet: 0,
                    bearing: 0,
                });
            } else {
                p.queries_ready = false;
            }
            next(&mut q, t, &mut bot, &mut o, &mut e);
            let deferred = q.arrival_survey(t, e.tick).unwrap();
            assert!(deferred.request.is_none());
            assert_eq!(
                deferred.deferred,
                Some(if local {
                    "local landing demand"
                } else {
                    "material queries unavailable"
                })
            );
            let p = &mut o.local.combat.recovery.flight.pilot;
            p.site_query = LandingSiteQuery::NotRequested;
            p.queries_ready = true;
            next(&mut q, t, &mut bot, &mut o, &mut e);
            assert_eq!(
                q.arrival_survey(t, e.tick)
                    .unwrap()
                    .request
                    .unwrap()
                    .generation,
                e.tick
            );
        }
    }

    #[test]
    fn expiry_identity_change_replacement_and_reset_withhold_old_geometry_requests() {
        for change in 0..7 {
            let (mut q, t, mut bot, mut o, mut e) = fixture();
            ready(&mut q, e.tick);
            next(&mut q, t, &mut bot, &mut o, &mut e);
            let historical = q.arrival_survey(t, e.tick).unwrap();
            match change {
                0 => o.planets[0].revision += 1,
                1 => bot.context.episode_seed += 1,
                2 => o.local.combat.recovery.flight.pilot.vehicle = VehicleId(99),
                3 => {
                    for _ in 1..=MAX_RESULT_AGE {
                        next(&mut q, t, &mut bot, &mut o, &mut e);
                    }
                }
                4 => q.reset(),
                5 => {
                    q.cancel(t, e.tick, "replaced");
                }
                _ => {
                    o.planets[historical.site.planet]
                        .claim
                        .as_mut()
                        .unwrap()
                        .owner = Some(PlayerId::PLAYER_2)
                }
            }
            q.observe(t, &bot, &o, &e, Some(false));
            assert!(q.arrival_survey(t, e.tick).is_none(), "change {change}");
        }
    }

    #[test]
    fn predicted_bearing_uses_position_in_the_rotating_planet_frame() {
        let (mut q, t, _, _, e) = fixture();
        ready(&mut q, e.tick);
        let mut f = q
            .snapshot(t, e.tick)
            .unwrap()
            .candidates
            .into_iter()
            .filter_map(|c| c.forecast)
            .find(|f| f.end == Some(TransferForecastEnd::KinematicHandoff))
            .unwrap();
        let last = f.samples.last_mut().unwrap();
        last.target.position = Vec2::new(200.0, 300.0);
        last.target.angle = std::f32::consts::FRAC_PI_2;
        last.ship.position = Vec2::new(100.0, 300.0);
        last.ship.angle = 1.2;
        assert_eq!(predicted_site(&f).unwrap().bearing, 0);
        f.samples.last_mut().unwrap().ship.angle = -2.3;
        assert_eq!(predicted_site(&f).unwrap().bearing, 0);
        for kind in 0..4 {
            let mut bad = f.clone();
            match kind {
                0 => bad.end = Some(TransferForecastEnd::Horizon),
                1 => bad.samples.last_mut().unwrap().target.angle = f32::NAN,
                2 => {
                    let s = bad.samples.last_mut().unwrap();
                    s.ship.position = s.target.position;
                }
                _ => bad.samples.last_mut().unwrap().after_ticks -= 1,
            }
            assert!(predicted_site(&bad).is_none());
        }
    }
    #[test]
    fn deferral_preserves_generation_and_completion_at_expiry_never_issues() {
        let (mut q, t, mut bot, mut o, mut e) = fixture();
        ready(&mut q, e.tick);
        next(&mut q, t, &mut bot, &mut o, &mut e);
        let first = q.arrival_survey(t, e.tick).unwrap();
        o.local.combat.recovery.flight.pilot.queries_ready = false;
        next(&mut q, t, &mut bot, &mut o, &mut e);
        assert!(q.arrival_survey(t, e.tick).unwrap().request.is_none());
        o.local.combat.recovery.flight.pilot.queries_ready = true;
        next(&mut q, t, &mut bot, &mut o, &mut e);
        assert_eq!(q.arrival_survey(t, e.tick), Some(first));

        let (mut q, t, mut bot, mut o, mut e) = fixture();
        for _ in 0..MAX_RESULT_AGE {
            next(&mut q, t, &mut bot, &mut o, &mut e);
        }
        ready(&mut q, e.tick);
        assert!(q.arrival_survey(t, e.tick).is_none());
        next(&mut q, t, &mut bot, &mut o, &mut e);
        assert!(q.arrival_survey(t, e.tick).is_none());
        assert_eq!(
            q.state(bot.context.actor).unwrap().reason,
            Some("source expired")
        );
    }

    #[test]
    fn selection_prefers_current_then_lowest_valid_neutral_alternative() {
        let (mut q, t, _, o, e) = fixture();
        ready(&mut q, e.tick);
        let mut report = q.snapshot(t, e.tick).unwrap();
        let first = report
            .candidates
            .iter()
            .find(|c| c.current)
            .unwrap()
            .clone();
        assert_eq!(
            first.forecast.as_ref().unwrap().end,
            Some(TransferForecastEnd::KinematicHandoff)
        );
        // Synthetic shortlist isolates priority/eligibility from flight outcomes.
        report.candidates = (0..3)
            .rev()
            .map(|destination| {
                let mut c = first.clone();
                c.current = destination == 2;
                c.destination = destination;
                c.forecast.as_mut().unwrap().destination = destination;
                c
            })
            .collect();
        assert_eq!(select(&report, &o.planets).unwrap().1.planet, 2);
        for reason in 0..4 {
            let mut r = report.clone();
            let mut planets = o.planets.clone();
            match reason {
                0 => planets[2].claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_2),
                1 => r.candidates[0].forecast = None,
                2 => {
                    r.candidates[0].forecast.as_mut().unwrap().end =
                        Some(TransferForecastEnd::Horizon)
                }
                _ => planets[2].claim = None,
            }
            assert_eq!(select(&r, &planets).unwrap().1.planet, 0);
            planets[0].claim = None;
            assert_eq!(select(&r, &planets).unwrap().1.planet, 1);
            planets[1].claim = None;
            assert!(select(&r, &planets).is_none());
        }
    }
}
