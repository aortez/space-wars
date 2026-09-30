//! One neutral destination and two material bearings. V16 may refresh the
//! bearings as the ship moves and rank their measured approach geometry.
//! The host registers demand after controls through its existing query budget.
use super::*;
use scenario_spacewars::surface_sortie::{
    destination_cover::{CoverFinding, DestinationCoverRequest},
    pilot::{LANDING_SITE_COUNT, LandingSiteQuery},
};

#[derive(Clone)]
pub(super) struct AlternativeSurvey {
    request: DestinationCoverRequest,
    key: PlanetKey,
    target: usize,
    visit: Option<u64>,
    approach_aware: bool,
}

// Match the remote measurement cadence, limiting refresh to at most twice a
// second. This is a request stability bound, not another query allowance.
const RETARGET_TICKS: u64 = 30;

fn candidates(
    o: &MissionObservationV1,
    planet: &PilotPlanetObservation,
) -> [Option<LandingSiteId>; 4] {
    let bearing = |direction: Vec2| {
        let local = direction.rotate_radians(-planet.motion.angle);
        ((-local.x).atan2(local.y).rem_euclid(std::f32::consts::TAU)
            * f32::from(LANDING_SITE_COUNT)
            / std::f32::consts::TAU)
            .round() as u8
            % LANDING_SITE_COUNT
    };
    let arrival = o.local.combat.recovery.flight.pilot.ship.position - planet.motion.position;
    let shadow = o.local.combat.target.map_or(arrival, |enemy| {
        planet.motion.position - enemy.motion.position
    });
    let a = bearing(shadow);
    let b = bearing(arrival);
    [
        Some(LandingSiteId {
            planet: planet.index,
            bearing: a,
        }),
        Some(LandingSiteId {
            planet: planet.index,
            bearing: if a == b {
                (b + LANDING_SITE_COUNT / 4) % LANDING_SITE_COUNT
            } else {
                b
            },
        }),
        None,
        None,
    ]
}

pub(super) fn request(
    retained: &mut Option<AlternativeSurvey>,
    o: &MissionObservationV1,
    mission: &MissionTelemetry,
) -> Option<DestinationCoverRequest> {
    let p = &o.local.combat.recovery.flight.pilot;
    let eligible = o.planets.len() <= MAX_PLANETS
        && p.ship_available
        && p.ship_form == ShipForm::Ship
        && matches!(p.location, PilotLocation::Aboard(_))
        && !o
            .match_context
            .as_ref()
            .is_some_and(|m| m.finished || !m.pilots_alive[p.owner.index()])
        && mission
            .capture
            .as_ref()
            .is_none_or(|c| c.landing.landed_tick.is_none());
    let Some(target) = mission.target.filter(|_| eligible) else {
        *retained = None;
        return None;
    };
    let Some(planet) = candidate_planets(o, mission)
        .into_iter()
        .take(MAX_OPTIONS)
        .find(|planet| {
            (planet.index != target || flag_evidence::enabled(mission.policy))
                && planet.claim.as_ref().is_some_and(|c| {
                    c.owner.is_none()
                        && c.flag.is_none()
                        && (c.stage_required_seconds - 3.0).abs() <= 0.001
                })
        })
    else {
        *retained = None;
        return None;
    };
    let key = PlanetKey::read(planet);
    let visit = selection_tick(mission);
    let approach_aware =
        mission.policy == crate::mission_policy::MissionPolicy::ApproachSurveyPlanner.id();
    let available = p.queries_ready
        && p.landing.supported_feet == 0
        && p.site_query == LandingSiteQuery::NotRequested;
    if retained.as_ref().is_none_or(|old| {
        old.target != target
            || old.visit != visit
            || !old.key.matches(&key)
            || old.request.generation > p.tick
            || old.approach_aware != approach_aware
            || (approach_aware
                && available
                && p.tick.saturating_sub(old.request.generation) >= RETARGET_TICKS
                && old.request.candidates != candidates(o, planet))
    }) {
        *retained = Some(AlternativeSurvey {
            request: DestinationCoverRequest {
                generation: p.tick,
                candidates: candidates(o, planet),
                sample_climb: true,
            },
            key,
            target,
            visit,
            approach_aware,
        });
    }
    // Retain the request identity across local work, but never compete with it.
    available.then(|| retained.as_ref().unwrap().request)
}

pub(super) fn evidence(
    retained: &Option<AlternativeSurvey>,
    o: &MissionObservationV1,
) -> Option<LocalEvidence> {
    let request = retained.as_ref()?;
    let result = o.destination_cover.as_ref()?;
    let p = &o.local.combat.recovery.flight.pilot;
    if !p.queries_ready || result.generation != request.request.generation {
        return None;
    }
    let planet = o
        .planets
        .iter()
        .take(MAX_PLANETS)
        .find(|v| v.index == request.key.planet)?;
    if !request.key.matches(&PlanetKey::read(planet)) {
        return None;
    }
    result
        .candidates
        .iter()
        .take(2)
        .filter_map(|candidate| {
            if !request.request.candidates.contains(&Some(candidate.id)) {
                return None;
            }
            let m = candidate.measurement.as_ref()?;
            if m.tick < request.request.generation
                || m.tick > p.tick
                || p.tick - m.tick > MAX_EVIDENCE_AGE
                || m.revision != planet.revision
                || m.ship_form != ShipForm::Ship
            {
                return None;
            }
            let approach = request
                .approach_aware
                .then(|| approach_angle(m, planet, p.ship.position))
                .flatten();
            let reason = if m.finding != CoverFinding::Measured {
                Some("alternative landing unavailable or incomplete")
            } else if m.site.is_none_or(|s| {
                s.id != candidate.id
                    || s.revision != m.revision
                    || !s.boarding_hatches.iter().any(Option::is_some)
            }) {
                Some("alternative exit or boarding unmeasured")
            } else if m.climb_clear != Some(true) {
                Some("alternative climb samples blocked or unmeasured")
            } else if request.approach_aware && approach.is_none() {
                Some("alternative approach geometry unavailable")
            } else {
                None
            };
            Some((
                LocalEvidence {
                    key: request.key,
                    site: candidate.id,
                    tick: m.tick,
                    gravity: 0.0,
                    costs: reason.is_none().then(model::no_flag_costs),
                    reason,
                    choice: None,
                    route_source_tick: None,
                    route_validated_tick: None,
                    route_objective: None,
                    remote: true,
                },
                approach,
            ))
        })
        .min_by(|a, b| {
            a.0.reason
                .is_some()
                .cmp(&b.0.reason.is_some())
                .then_with(|| a.1.unwrap_or(0.0).total_cmp(&b.1.unwrap_or(0.0)))
                .then_with(|| b.0.tick.cmp(&a.0.tick))
                .then(a.0.site.bearing.cmp(&b.0.site.bearing))
        })
        .map(|(sample, _)| sample)
}

/// Rank at the observed pose, not a predicted arrival. Reproject the measured
/// vehicle point from its immutable material frame; keep its original age.
/// Radians are only a tie-break between the equal neutral phase references,
/// never seconds or a live cover/solar certificate.
fn approach_angle(
    measurement: &scenario_spacewars::surface_sortie::destination_cover::CoverMeasurement,
    planet: &PilotPlanetObservation,
    ship: Vec2,
) -> Option<f32> {
    let site = measurement.site?;
    let local = (site.vehicle_position - measurement.planet.position)
        .rotate_radians(-measurement.planet.angle);
    let direction = local.rotate_radians(planet.motion.angle);
    let up = ship - planet.motion.position;
    let norms = [up.length_squared(), direction.length_squared()];
    if ![up.x, up.y, direction.x, direction.y]
        .into_iter()
        .all(f32::is_finite)
        || norms.into_iter().any(|n| !n.is_finite() || n < 0.001)
    {
        return None;
    }
    let angle =
        crate::tactical_sortie::angle_between(up.normalized(), direction.normalized()).abs();
    angle.is_finite().then_some(angle)
}
