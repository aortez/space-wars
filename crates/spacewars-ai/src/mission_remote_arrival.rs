//! Conditional solar screening of source-measured remote sites. No future
//! survey, acquisition duration, enemy forecast or landing permission is implied.
use super::arrival_local::ArrivalLocalReport;
use super::arrival_preference::ArrivalPreferenceReport;
use crate::{landing_safety, mission_evaluation::MAX_EVIDENCE_AGE};
use engine_core::Vec2;
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{
        SolarHazard,
        combat::TacticalSortieObservationV1,
        destination_cover::{CoverCandidate, CoverFinding, MAX_COVER_CANDIDATES},
        mission::MissionObservationV1,
        pilot::{LANDING_SITE_COUNT, PilotLandingSite, PilotMotion, PilotPlanetObservation},
    },
};
use serde::Serialize;

// A numerical domain, not a gameplay limit. Squared distances and products of
// rate, radius and travel time stay below f32 overflow during the bounded screen.
const MAX_SCREEN_MAGNITUDE: f32 = 1.0e6;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoteArrivalFrame {
    pub tick: u64,
    pub ship: PilotMotion,
    pub planet: PilotPlanetObservation,
    pub sun: Option<SolarHazard>,
    pub planet_orbit_omega: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoteArrivalDirection {
    pub side: f32,
    pub solar: Option<landing_safety::SolarLandingPlan>,
    /// Only the conditional solar screen; no terrain/route or combat guarantee.
    pub solar_clear: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoteArrivalSite {
    /// Includes historical cover and opponent at the measurement tick. Neither
    /// is projected forward or used to assert exposure at arrival.
    pub source: CoverCandidate,
    pub arrival_age_ticks: Option<u64>,
    pub projected: Option<PilotLandingSite>,
    pub directions: Vec<RemoteArrivalDirection>,
    pub unknown: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoteArrivalScreen {
    pub model: &'static str,
    pub source_tick: u64,
    pub destination: usize,
    pub generation: Option<u64>,
    pub source_planet: Option<PilotPlanetObservation>,
    pub arrival: Option<RemoteArrivalFrame>,
    pub sites: Vec<RemoteArrivalSite>,
    pub charged_graph: u64,
    pub complete: bool,
    pub unknown: Option<&'static str>,
    pub acquisition: &'static str,
    pub future_threat: &'static str,
    /// Separately charged, opt-in conditional medians. Never changes this
    /// screen's solar completion/charge accounting or old local cost evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_reference: Option<ArrivalLocalReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_preference: Option<ArrivalPreferenceReport>,
}

#[derive(Clone)]
pub(super) struct ArrivalScreenJob {
    pub report: RemoteArrivalScreen,
    local: Option<TacticalSortieObservationV1>,
}

#[derive(Clone)]
pub(super) struct ArrivalScreenContext {
    sun: Option<SolarHazard>,
    earliest: Option<u64>,
    local_claims: Vec<scenario_spacewars::surface_sortie::PlanetClaimObservation>,
}
impl ArrivalScreenContext {
    pub fn new(o: &MissionObservationV1, jobs: &[ArrivalScreenJob]) -> Self {
        Self {
            sun: o.local.sun,
            earliest: jobs
                .iter()
                .filter(|j| j.report.unknown.is_none())
                .flat_map(|j| &j.report.sites)
                .filter(|s| s.unknown.is_none())
                .filter_map(|s| s.source.measurement.as_ref().map(|m| m.tick))
                .min(),
            local_claims: jobs
                .iter()
                .filter(|j| {
                    j.report.unknown.is_none()
                        && j.report
                            .local_reference
                            .as_ref()
                            .is_some_and(|r| r.unknown.is_none())
                })
                .filter_map(|j| j.report.source_planet.as_ref())
                .filter(|p| super::arrival_local::neutral_claim(p))
                .filter_map(|p| p.claim.clone())
                .collect(),
        }
    }
    pub fn matches(&self, o: &MissionObservationV1) -> bool {
        let tick = o.local.combat.recovery.flight.pilot.tick;
        self.sun == o.local.sun
            && self
                .earliest
                .is_none_or(|t| tick >= t && tick - t <= MAX_EVIDENCE_AGE)
    }

    pub fn local_claims_match(&self, o: &MissionObservationV1) -> bool {
        self.local_claims.iter().all(|source| {
            o.planets
                .iter()
                .find(|p| p.index == source.planet)
                .is_some_and(|p| {
                    super::arrival_local::neutral_claim(p)
                        && p.claim.as_ref().is_some_and(|c| {
                            c.stage_required_seconds == source.stage_required_seconds
                                && c.flag_interaction_range == source.flag_interaction_range
                        })
                })
        })
    }
}

impl ArrivalScreenJob {
    pub fn new(o: &MissionObservationV1, destination: usize) -> Self {
        let p = &o.local.combat.recovery.flight.pilot;
        let mut report = RemoteArrivalScreen {
            model: "remote_arrival_screen_v1",
            source_tick: p.tick,
            destination,
            generation: o.destination_cover.as_ref().map(|c| c.generation),
            source_planet: o.planets.iter().find(|v| v.index == destination).cloned(),
            arrival: None,
            sites: Vec::new(),
            charged_graph: 0,
            complete: false,
            unknown: None,
            acquisition: "requires fresh native survey; choice and duration unknown",
            future_threat: "unmodeled; source cover and opponent are historical only",
            local_reference: None,
            site_preference: None,
        };
        report.unknown = if o.local.sun.is_some() != o.sun.is_some()
            || o.local
                .sun
                .zip(o.sun)
                .is_some_and(|(a, b)| a.position != b.position)
        {
            Some("source solar context inconsistent")
        } else if !p.queries_ready {
            Some("source queries unavailable")
        } else if report.source_planet.as_ref().is_none_or(|planet| {
            !valid_motion(planet.motion)
                || !bounded(planet.radius)
                || planet.radius <= 0.0
                || planet
                    .claim
                    .as_ref()
                    .is_none_or(|c| c.owner.is_some() || c.flag.is_some())
        }) {
            Some("source destination is not known neutral ground")
        } else if o
            .destination_cover
            .as_ref()
            .is_none_or(|c| c.generation > p.tick || c.candidates.len() > MAX_COVER_CANDIDATES)
        {
            Some("source remote survey unavailable or incompatible")
        } else {
            None
        };
        if report.unknown.is_none() {
            let cover = o.destination_cover.as_ref().unwrap();
            for c in &cover.candidates {
                if c.id.planet != destination {
                    continue;
                }
                let duplicate = cover
                    .candidates
                    .iter()
                    .filter(|other| other.id == c.id)
                    .count()
                    != 1;
                let unknown = if duplicate {
                    Some("duplicate source site")
                } else {
                    sample_reason(
                        c,
                        cover.generation,
                        p.tick,
                        report.source_planet.as_ref().unwrap(),
                    )
                };
                report.sites.push(RemoteArrivalSite {
                    source: c.clone(),
                    arrival_age_ticks: None,
                    projected: None,
                    directions: Vec::new(),
                    unknown,
                });
            }
            if report.sites.is_empty() {
                report.unknown = Some("no source remote sites for destination");
            }
        }
        Self {
            report,
            local: None,
        }
    }

    pub fn with_local_reference(mut self, policy: &'static str, o: &MissionObservationV1) -> Self {
        self.report.local_reference = Some(ArrivalLocalReport::new(policy, o));
        self.reject_unavailable_reference();
        self
    }

    pub fn with_site_preference(mut self, fresh_capture: bool) -> Self {
        self.report.site_preference =
            Some(ArrivalPreferenceReport::new(&self.report, fresh_capture));
        self
    }

    fn reject_unavailable_reference(&mut self) {
        if let Some(reference) = &mut self.report.local_reference
            && let Some(reason) = self.report.unknown
        {
            reference.reject(reason);
        }
        if let Some(preference) = &mut self.report.site_preference
            && let Some(reason) = self.report.unknown
        {
            preference.reject(reason);
        }
    }

    pub fn begin(&mut self, local: Option<TacticalSortieObservationV1>) {
        let Some(local) = local else {
            self.report
                .unknown
                .get_or_insert("conditional arrival unavailable");
            self.report.complete = true;
            self.reject_unavailable_reference();
            return;
        };
        let p = &local.combat.recovery.flight.pilot;
        let solar_valid = local.sun.is_none_or(|sun| {
            finite(sun.position)
                && bounded(sun.radius)
                && sun.radius > 0.0
                && bounded(sun.heat_radius)
                && sun.heat_radius >= sun.radius
        }) && local.planet_orbit_omega.is_none_or(bounded);
        if p.tick < self.report.source_tick
            || p.planet.index != self.report.destination
            || !valid_motion(p.ship)
            || !valid_motion(p.planet.motion)
            || !solar_valid
            || self.report.source_planet.as_ref().is_none_or(|source| {
                source.revision != p.planet.revision
                    || source.radius != p.planet.radius
                    || source.claim != p.planet.claim
            })
        {
            self.report
                .unknown
                .get_or_insert("conditional arrival frame incompatible");
            self.report.complete = true;
            self.reject_unavailable_reference();
            return;
        }
        self.report.arrival = Some(RemoteArrivalFrame {
            tick: p.tick,
            ship: p.ship,
            planet: p.planet.clone(),
            sun: local.sun,
            planet_orbit_omega: local.planet_orbit_omega,
        });
        if self.report.unknown.is_none() {
            for site in &mut self.report.sites {
                if site.unknown.is_some() {
                    continue;
                }
                let m = site.source.measurement.as_ref().unwrap();
                let age = p.tick - m.tick;
                site.arrival_age_ticks = Some(age);
                if age > MAX_EVIDENCE_AGE {
                    site.unknown = Some("source sample exceeds evidence horizon at arrival");
                    continue;
                }
                let projected = project(m.site.unwrap(), m.planet, &p.planet);
                if !valid_site(projected) {
                    site.unknown = Some("nonfinite projected site");
                    continue;
                }
                site.projected = Some(projected);
            }
        }
        self.local = Some(local);
        self.report.complete = !self.has_solar_work();
        self.reject_unavailable_reference();
    }

    pub fn has_work(&self) -> bool {
        self.has_solar_work()
            || (self.report.complete
                && (self
                    .report
                    .local_reference
                    .as_ref()
                    .is_some_and(|r| !r.complete)
                    || self
                        .report
                        .site_preference
                        .as_ref()
                        .is_some_and(|r| !r.complete)))
    }

    fn has_solar_work(&self) -> bool {
        self.local.is_some()
            && self.report.unknown.is_none()
            && self
                .report
                .sites
                .iter()
                .any(|s| s.projected.is_some() && s.directions.len() < 2)
    }

    /// One charged graph operation per site/direction. At most four sites and
    /// two directions; solar assessment itself has fixed bounded sampling.
    pub fn step(&mut self) {
        if !self.has_solar_work() {
            if self
                .report
                .local_reference
                .as_ref()
                .is_some_and(|r| !r.complete)
            {
                let mut reference = self.report.local_reference.take().unwrap();
                reference.step(&self.report);
                self.report.local_reference = Some(reference);
            } else {
                let mut preference = self.report.site_preference.take().unwrap();
                preference.step(&self.report);
                self.report.site_preference = Some(preference);
            }
            return;
        }
        let site = self
            .report
            .sites
            .iter_mut()
            .find(|s| s.projected.is_some() && s.directions.len() < 2)
            .unwrap();
        let side = [-1.0, 1.0][site.directions.len()];
        let local = self.local.as_ref().unwrap();
        let solar = landing_safety::assess(local, site.projected.unwrap(), side, true);
        let solar_clear = solar.is_none_or(|s| {
            [
                s.arrival_seconds,
                s.surface_seconds,
                s.approach_clearance,
                s.parked_clearance,
                s.departure_clearance,
            ]
            .into_iter()
            .all(f32::is_finite)
                && s.safe()
        });
        site.directions.push(RemoteArrivalDirection {
            side,
            solar,
            solar_clear,
        });
        self.report.charged_graph += 1;
        self.report.complete = !self.has_solar_work();
    }
}

fn finite(v: Vec2) -> bool {
    bounded(v.x) && bounded(v.y)
}
fn bounded(v: f32) -> bool {
    v.is_finite() && v.abs() <= MAX_SCREEN_MAGNITUDE
}
fn valid_motion(m: PilotMotion) -> bool {
    finite(m.position) && finite(m.velocity) && bounded(m.angle) && bounded(m.spin)
}
fn valid_site(s: PilotLandingSite) -> bool {
    [
        s.local_position,
        s.position,
        s.normal,
        s.velocity,
        s.vehicle_position,
        s.hatch_position,
    ]
    .into_iter()
    .all(finite)
        && (s.normal.length_squared() - 1.0).abs() < 0.001
        && s.boarding_hatches.into_iter().flatten().all(finite)
        && s.boarding_hatches.iter().any(Option::is_some)
}
fn sample_reason(
    c: &CoverCandidate,
    generation: u64,
    tick: u64,
    planet: &PilotPlanetObservation,
) -> Option<&'static str> {
    let Some(m) = c.measurement.as_ref() else {
        return Some("source site unmeasured");
    };
    if m.tick < generation
        || m.tick > tick
        || tick - m.tick > MAX_EVIDENCE_AGE
        || m.revision != planet.revision
        || m.ship_form != ShipForm::Ship
        || !valid_motion(m.planet)
    {
        return Some("source sample identity or age incompatible");
    }
    if m.finding != CoverFinding::Measured {
        return Some("source landing unavailable or incomplete");
    }
    let Some(s) = m.site else {
        return Some("source site geometry unavailable");
    };
    if c.id.bearing >= LANDING_SITE_COUNT
        || s.id != c.id
        || s.revision != m.revision
        || !valid_site(s)
        || m.cover.is_some_and(|cover| cover.site != c.id)
        || m.opponent
            .is_some_and(|opponent| !valid_motion(opponent.motion))
        || (s.position - m.planet.position)
            .rotate_radians(-m.planet.angle)
            .distance_to(s.local_position)
            > 0.002
    {
        return Some("source site geometry incompatible");
    }
    if m.climb_clear != Some(true) {
        return Some("source climb blocked or unmeasured");
    }
    None
}

fn project(
    mut site: PilotLandingSite,
    source: PilotMotion,
    planet: &PilotPlanetObservation,
) -> PilotLandingSite {
    let angle = planet.motion.angle - source.angle;
    let point = |p: Vec2| planet.motion.position + (p - source.position).rotate_radians(angle);
    site.position = point(site.position);
    site.normal = site.normal.rotate_radians(angle);
    site.vehicle_position = point(site.vehicle_position);
    site.hatch_position = point(site.hatch_position);
    site.boarding_hatches = site.boarding_hatches.map(|h| h.map(point));
    site.velocity = planet.velocity_at(site.vehicle_position);
    site
}

#[cfg(test)]
#[path = "mission_remote_arrival_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "mission_arrival_local_tests.rs"]
mod local_reference_tests;

#[cfg(test)]
#[path = "mission_arrival_preference_tests.rs"]
mod preference_tests;
