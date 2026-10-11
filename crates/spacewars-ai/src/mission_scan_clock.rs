//! Conditional native scan schedule; selection can fail or be interrupted.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferScanClock {
    pub model: &'static str,
    pub source_tick: u64,
    pub actor: PlayerId,
    pub destination: usize,
    pub revision: u64,
    pub form: ShipForm,
    pub cadence: LandingSurveyCadence,
    pub last_survey: Option<LandingSurveyStamp>,
    pub match_remaining_ticks: Option<u64>,
    pub complete: bool,
    pub handoff_tick: Option<u64>,
    pub request_tick: Option<u64>,
    pub opportunity_tick: Option<u64>,
    pub handoff_to_scan_ticks: Option<u64>,
    pub unknown: Option<&'static str>,
    pub conditions: &'static str,
    /// A scan can be empty or all candidates can be rejected. Never filled by
    /// this schedule, even when the conditional scan is the very next tick.
    pub site_selection_seconds: Option<f32>,
}

impl TransferScanClock {
    pub(super) fn new(
        bot: &MaterialMissionPilot,
        o: &MissionObservationV1,
        destination: usize,
        cadence: LandingSurveyCadence,
    ) -> Self {
        let p = &o.local.combat.recovery.flight.pilot;
        let planet = o.planets.iter().find(|p| p.index == destination).unwrap();
        let last_survey = bot.sensor_request().last_survey;
        // Native matches accumulate integer nanoseconds. Dividing seconds by
        // the f64 tick can round an exact deadline upward (e.g. 31 ticks).
        let remaining = o
            .match_context
            .as_ref()
            .and_then(|m| m.remaining_seconds)
            .map(|seconds| {
                let duration = std::time::Duration::try_from_secs_f64(seconds)
                    .map_err(|_| "match duration unsupported")?;
                u64::try_from(duration.as_nanos().div_ceil(16_666_667))
                    .map_err(|_| "match duration unsupported")
            })
            .transpose();
        let unknown = if !arrival_local::neutral_claim(planet) {
            Some("claim outside neutral-idle scan domain")
        } else if last_survey.is_some_and(|s| s.tick > p.tick) {
            Some("survey history later than source")
        } else if o.match_context.as_ref().is_some_and(|m| {
            m.finished
                || m.pilots_alive.iter().any(|alive| !alive)
                || m.remaining_seconds
                    .is_some_and(|s| !s.is_finite() || s < 0.0)
        }) {
            Some("match unavailable for scan clock")
        } else if remaining.is_err() {
            Some("match duration unsupported")
        } else {
            None
        };
        Self {
            model: "conditional_neutral_scan_v1",
            source_tick: p.tick,
            actor: p.owner,
            destination,
            revision: planet.revision,
            form: p.ship_form,
            cadence,
            last_survey,
            match_remaining_ticks: remaining.unwrap_or(None),
            complete: false,
            handoff_tick: None,
            request_tick: None,
            opportunity_tick: None,
            handoff_to_scan_ticks: None,
            unknown,
            conditions: "query-ready native handoff at the predicted tick; uninterrupted capture requests in the same approach frame and ship form; unchanged neutral claim and material; supplied native cadence; no intervening survey",
            site_selection_seconds: None,
        }
    }

    pub(super) fn finish(&mut self, end: TransferForecastEnd, transfer_ticks: u64) {
        self.complete = true;
        if end != TransferForecastEnd::KinematicHandoff {
            self.unknown.get_or_insert("no conditional handoff");
            return;
        }
        self.handoff_tick = self.source_tick.checked_add(transfer_ticks);
        if self.unknown.is_some() {
            return;
        }
        let result = (|| {
            let handoff = self.handoff_tick.ok_or("scan clock overflow")?;
            let request = handoff.checked_add(1).ok_or("scan clock overflow")?;
            self.request_tick = Some(request);
            let delay = self.cadence.survey_delay_ticks(
                request,
                self.actor.index(),
                (self.destination, self.form),
                self.last_survey,
                false,
            );
            let scan = request.checked_add(delay).ok_or("scan clock overflow")?;
            // Match completion precedes controller intent at the deadline tick.
            if self
                .match_remaining_ticks
                .is_some_and(|limit| scan - self.source_tick >= limit)
            {
                return Err("match ends before scan opportunity");
            }
            self.opportunity_tick = Some(scan);
            self.handoff_to_scan_ticks = Some(scan - handoff);
            Ok(())
        })();
        self.unknown = result.err();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_clock_reuses_both_seat_phases_and_retained_form_history() {
        let (_, bot, o) = super::super::tests::source();
        let destination = bot.telemetry.target.unwrap();
        for seat in 0..2 {
            for phase in 0..15 {
                for (same_planet, form, cadence) in [
                    (true, ShipForm::Ship, LandingSurveyCadence::FourHz),
                    (false, ShipForm::Ship, LandingSurveyCadence::FourHz),
                    (true, ShipForm::EscapePod, LandingSurveyCadence::FourHz),
                    (true, ShipForm::Ship, LandingSurveyCadence::EveryTick),
                ] {
                    let mut bot = bot.clone();
                    let mut o = o.clone();
                    let p = &mut o.local.combat.recovery.flight.pilot;
                    p.tick = 100;
                    p.owner = PlayerId::from_index(seat).unwrap();
                    bot.last_survey = Some(LandingSurveyStamp {
                        tick: 90,
                        planet: if same_planet {
                            destination
                        } else {
                            destination + 1
                        },
                        form,
                    });
                    let handoff = 120 + phase;
                    let mut clock = TransferScanClock::new(&bot, &o, destination, cadence);
                    assert!(clock.unknown.is_none());
                    assert!(!clock.complete && clock.opportunity_tick.is_none());
                    clock.finish(TransferForecastEnd::KinematicHandoff, handoff - 100);
                    let delay = if same_planet
                        && form == ShipForm::Ship
                        && cadence == LandingSurveyCadence::FourHz
                    {
                        (15 - (handoff + 1 + seat as u64 * 7) % 15) % 15
                    } else {
                        0
                    };
                    assert_eq!(clock.opportunity_tick, Some(handoff + 1 + delay));
                    assert_eq!(clock.handoff_to_scan_ticks, Some(1 + delay));
                    assert!(clock.complete && clock.unknown.is_none());
                    assert_eq!(clock.site_selection_seconds, None);
                }
            }
        }
    }

    #[test]
    fn clock_refuses_non_neutral_claims_and_future_history() {
        let (_, bot, o) = super::super::tests::source();
        let destination = bot.telemetry.target.unwrap();
        for mutation in 0..8 {
            let mut bot = bot.clone();
            let mut o = o.clone();
            let claim = o.planets[destination].claim.as_mut().unwrap();
            match mutation {
                0 => claim.owner = Some(bot.context.actor),
                1 => claim.claimant = Some(bot.context.actor),
                2 => claim.progress = 0.1,
                3 => claim.stage_required_seconds = f32::NAN,
                4 => claim.flag_interaction_range = 0.0,
                5 => o.planets[destination].claim = None,
                6 => {
                    bot.last_survey = Some(LandingSurveyStamp {
                        tick: o.local.combat.recovery.flight.pilot.tick + 1,
                        planet: destination,
                        form: ShipForm::Ship,
                    })
                }
                _ => o.match_context.as_mut().unwrap().finished = true,
            }
            let mut clock =
                TransferScanClock::new(&bot, &o, destination, LandingSurveyCadence::FourHz);
            assert!(clock.unknown.is_some(), "mutation {mutation}");
            clock.finish(TransferForecastEnd::KinematicHandoff, 30);
            assert!(clock.complete && clock.opportunity_tick.is_none());
            assert!(clock.site_selection_seconds.is_none());
        }
    }

    #[test]
    fn deadline_nonarrival_and_overflow_do_not_invent_an_opportunity() {
        let (_, bot, o) = super::super::tests::source();
        let mut template = TransferScanClock::new(
            &bot,
            &o,
            bot.telemetry.target.unwrap(),
            LandingSurveyCadence::FourHz,
        );
        template.last_survey = None;
        for end in [
            TransferForecastEnd::Horizon,
            TransferForecastEnd::ControllerInterrupted,
            TransferForecastEnd::MatchTimeLimit,
            TransferForecastEnd::PlanetEnvelope,
        ] {
            let mut clock = template.clone();
            clock.finish(end, 30);
            assert!(
                clock.complete && clock.handoff_tick.is_none() && clock.opportunity_tick.is_none()
            );
            assert_eq!(clock.unknown, Some("no conditional handoff"));
        }
        for limit in [30, 31, 32] {
            let mut clock = template.clone();
            clock.match_remaining_ticks = Some(limit);
            clock.finish(TransferForecastEnd::KinematicHandoff, 30);
            assert_eq!(clock.opportunity_tick.is_some(), limit == 32);
        }
        for source in [u64::MAX, u64::MAX - 30] {
            let mut clock = template.clone();
            clock.source_tick = source;
            clock.finish(TransferForecastEnd::KinematicHandoff, 30);
            assert_eq!(clock.unknown, Some("scan clock overflow"));
            assert!(clock.opportunity_tick.is_none());
        }
    }

    #[test]
    fn exact_native_deadline_is_not_rounded_into_an_extra_controller_tick() {
        let (_, bot, mut o) = super::super::tests::source();
        for nanos in [31 * 16_666_667 - 1, 31 * 16_666_667, 31 * 16_666_667 + 1] {
            o.match_context.as_mut().unwrap().remaining_seconds =
                Some(std::time::Duration::from_nanos(nanos).as_secs_f64());
            let mut clock = TransferScanClock::new(
                &bot,
                &o,
                bot.telemetry.target.unwrap(),
                LandingSurveyCadence::EveryTick,
            );
            clock.finish(TransferForecastEnd::KinematicHandoff, 30);
            assert_eq!(
                clock.match_remaining_ticks,
                Some(nanos.div_ceil(16_666_667))
            );
            assert_eq!(clock.opportunity_tick.is_some(), nanos > 31 * 16_666_667);
        }
    }
}
