//! Two fixed material bearings per neutral planet. Ordinary policies request
//! one alternative; the opt-in experiment also requests the current neutral.
//! The host registers demand after controls and keeps its existing query budget.
use super::*;
use scenario_spacewars::surface_sortie::{
    destination_cover::{CoverFinding, DestinationCoverRequest},
    pilot::{LANDING_SITE_COUNT, LandingSiteQuery},
};

#[derive(Clone)]
pub(super) struct AlternativeSurvey {
    request: DestinationCoverRequest,
    keys: Vec<PlanetKey>,
    target: usize,
    visit: Option<u64>,
}

pub(super) fn request(
    retained: &mut Option<AlternativeSurvey>,
    o: &MissionObservationV1,
    mission: &MissionTelemetry,
    include_current: bool,
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
    let options: Vec<_> = candidate_planets(o, mission)
        .into_iter()
        .take(MAX_OPTIONS)
        .filter(|planet| {
            planet.claim.as_ref().is_some_and(|c| {
                c.owner.is_none()
                    && c.flag.is_none()
                    && (c.stage_required_seconds - 3.0).abs() <= 0.001
            })
        })
        .collect();
    let current = options
        .iter()
        .copied()
        .find(|planet| include_current && planet.index == target);
    let alternative = options
        .iter()
        .copied()
        .find(|planet| planet.index != target);
    let planets: Vec<_> = current.into_iter().chain(alternative).collect();
    if planets.is_empty() {
        *retained = None;
        return None;
    }
    let keys: Vec<_> = planets
        .iter()
        .map(|planet| PlanetKey::read(planet))
        .collect();
    let visit = selection_tick(mission);
    if retained.as_ref().is_none_or(|old| {
        old.target != target
            || old.visit != visit
            || old.keys.len() != keys.len()
            || !old
                .keys
                .iter()
                .zip(&keys)
                .all(|(old, new)| old.matches(new))
            || old.request.generation > p.tick
    }) {
        let mut candidates = [None; 4];
        for (index, planet) in planets.iter().enumerate() {
            let bearing = |direction: Vec2| {
                let local = direction.rotate_radians(-planet.motion.angle);
                ((-local.x).atan2(local.y).rem_euclid(std::f32::consts::TAU)
                    * f32::from(LANDING_SITE_COUNT)
                    / std::f32::consts::TAU)
                    .round() as u8
                    % LANDING_SITE_COUNT
            };
            let arrival = p.ship.position - planet.motion.position;
            let shadow = o.local.combat.target.map_or(arrival, |enemy| {
                planet.motion.position - enemy.motion.position
            });
            let a = bearing(shadow);
            let b = bearing(arrival);
            candidates[index * 2..index * 2 + 2].copy_from_slice(&[
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
            ]);
        }
        *retained = Some(AlternativeSurvey {
            request: DestinationCoverRequest {
                generation: p.tick,
                candidates,
                sample_climb: true,
            },
            keys,
            target,
            visit,
        });
    }
    // Retain the request identity across local work, but never compete with it.
    (p.queries_ready
        && p.landing.supported_feet == 0
        && p.site_query == LandingSiteQuery::NotRequested)
        .then(|| retained.as_ref().unwrap().request)
}

pub(super) fn evidence(
    retained: &Option<AlternativeSurvey>,
    o: &MissionObservationV1,
) -> Vec<LocalEvidence> {
    let Some(request) = retained.as_ref() else {
        return Vec::new();
    };
    let Some(result) = o.destination_cover.as_ref() else {
        return Vec::new();
    };
    let p = &o.local.combat.recovery.flight.pilot;
    if !p.queries_ready || result.generation != request.request.generation {
        return Vec::new();
    }
    request
        .keys
        .iter()
        .filter_map(|key| {
            let planet = o
                .planets
                .iter()
                .take(MAX_PLANETS)
                .find(|v| v.index == key.planet)?;
            if !key.matches(&PlanetKey::read(planet)) {
                return None;
            }
            result
                .candidates
                .iter()
                .take(4)
                .filter_map(|candidate| {
                    if candidate.id.planet != key.planet
                        || !request.request.candidates.contains(&Some(candidate.id))
                    {
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
                    } else {
                        None
                    };
                    Some(LocalEvidence {
                        key: key.clone(),
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
                    })
                })
                .min_by(|a, b| {
                    a.reason
                        .is_some()
                        .cmp(&b.reason.is_some())
                        .then_with(|| b.tick.cmp(&a.tick))
                        .then(a.site.bearing.cmp(&b.site.bearing))
                })
        })
        .collect()
}
