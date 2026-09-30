//! Conditional native preference within a retained subset, never a full survey.
use super::{arrival_local::neutral_claim, remote_arrival::RemoteArrivalScreen};
use crate::tactical_sortie::{angle_between, approach_score, preferred_side};
use scenario_spacewars::surface_sortie::pilot::{LANDING_SITE_COUNT, LandingSiteId};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ArrivalSitePreference {
    pub site: LandingSiteId,
    pub side: f32,
    pub direction_order: usize,
    /// Native controller units, not elapsed seconds.
    pub approach_score: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArrivalSiteAssessment {
    pub site: LandingSiteId,
    pub measurement_tick: Option<u64>,
    pub arrival_tick: u64,
    pub short_angle: Option<f32>,
    /// Eligible directions in native preferred/opposite order. Without a sun,
    /// the native selector only considers the preferred direction.
    pub eligible: Vec<ArrivalSitePreference>,
    pub unknown: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArrivalPreferenceReport {
    pub model: &'static str,
    pub source_tick: u64,
    pub assessments: Vec<ArrivalSiteAssessment>,
    pub preferred: Option<ArrivalSitePreference>,
    /// Includes unavailable geometry, not just unrequested bearings. A failed
    /// climb measurement does not prove that native landing would reject it.
    pub unassessed_bearings: Vec<u8>,
    pub charged_graph: u64,
    pub complete: bool,
    pub unknown: Option<&'static str>,
    pub conditions: &'static str,
    pub native_choice: Option<ArrivalSitePreference>,
    pub native_choice_unknown: &'static str,
    pub acquisition_seconds: Option<f32>,
}

impl ArrivalPreferenceReport {
    pub(super) fn new(screen: &RemoteArrivalScreen, fresh_capture: bool) -> Self {
        let reference = screen.local_reference.as_ref().unwrap();
        let unknown = reference.unknown.or((!fresh_capture)
            .then_some("source capture already active; fresh selector history unknown"));
        Self {
            model: "arrival_subset_preference_v1",
            source_tick: screen.source_tick,
            assessments: Vec::new(),
            preferred: None,
            unassessed_bearings: (0..LANDING_SITE_COUNT).collect(),
            charged_graph: 0,
            complete: unknown.is_some(),
            unknown,
            conditions: "fresh committed v13 capture, no required/rejected/cooldown site or objective; unexposed neutral ground; projected geometry and solar assumptions persist",
            native_choice: None,
            native_choice_unknown: "full future survey, query readiness, acquisition state and exposure unknown; preference covers retained subset only",
            acquisition_seconds: None,
        }
    }

    pub(super) fn reject(&mut self, reason: &'static str) {
        self.unknown.get_or_insert(reason);
        self.complete = true;
    }

    /// One graph operation per recorded site, including refusals. At most two
    /// scores and four sites. Solar work is reused, never rerun or relabelled.
    pub(super) fn step(&mut self, screen: &RemoteArrivalScreen) {
        let site = &screen.sites[self.assessments.len()];
        let arrival = screen.arrival.as_ref().unwrap();
        let mut record = ArrivalSiteAssessment {
            site: site.source.id,
            measurement_tick: site.source.measurement.as_ref().map(|m| m.tick),
            arrival_tick: arrival.tick,
            short_angle: None,
            eligible: Vec::new(),
            unknown: site.unknown,
        };
        record.unknown = record.unknown.or_else(|| {
            if site.projected.is_none() || record.measurement_tick.is_none() {
                Some("arrival material site unavailable")
            } else if !neutral_claim(&arrival.planet) {
                Some("claim state outside arrival-local reference domain")
            } else if site.directions.len() != 2 {
                Some("arrival solar screen incomplete")
            } else {
                None
            }
        });
        if record.unknown.is_none() {
            let center = arrival.planet.motion.position;
            let up = (arrival.ship.position - center).normalized();
            let direction = (site.projected.unwrap().vehicle_position - center).normalized();
            if up.length_squared() < 0.5 || direction.length_squared() < 0.5 {
                record.unknown = Some("degenerate arrival approach geometry");
            } else {
                let short = angle_between(up, direction);
                record.short_angle = Some(short);
                let preferred = preferred_side(short);
                for (direction_order, side) in [preferred, -preferred].into_iter().enumerate() {
                    if direction_order == 1 && arrival.sun.is_none() {
                        continue;
                    }
                    let solar = site.directions.iter().find(|d| d.side == side).unwrap();
                    if !solar.solar_clear {
                        continue;
                    }
                    let candidate = ArrivalSitePreference {
                        site: site.source.id,
                        side,
                        direction_order,
                        approach_score: approach_score(
                            short,
                            side,
                            solar.solar.is_some(),
                            arrival.planet.radius,
                        ),
                    };
                    record.eligible.push(candidate);
                    // Native surveys enumerate ascending bearings. Retained
                    // evidence may arrive in any order; restore native ties.
                    if self.preferred.is_none_or(|best| {
                        candidate
                            .approach_score
                            .total_cmp(&best.approach_score)
                            .then(candidate.site.bearing.cmp(&best.site.bearing))
                            .then(candidate.direction_order.cmp(&best.direction_order))
                            .is_lt()
                    }) {
                        self.preferred = Some(candidate);
                    }
                }
                self.unassessed_bearings
                    .retain(|b| *b != site.source.id.bearing);
                if record.eligible.is_empty() {
                    record.unknown = Some("no eligible native arrival direction");
                }
            }
        }
        self.assessments.push(record);
        self.charged_graph += 1;
        self.complete = self.assessments.len() == screen.sites.len();
    }
}
