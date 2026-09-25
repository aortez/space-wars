use super::*;
use scenario_spacewars::surface_sortie::landing_objective::LandingObjective;

#[derive(Clone, PartialEq)]
pub(super) struct PlanetKey {
    pub planet: usize,
    revision: u64,
    owner: Option<PlayerId>,
    known: bool,
    flag: Option<(PlayerId, Vec2)>,
    stage_seconds: Option<f32>,
}
impl PlanetKey {
    pub fn read(planet: &PilotPlanetObservation) -> Self {
        Self {
            planet: planet.index,
            revision: planet.revision,
            owner: planet.claim.as_ref().and_then(|c| c.owner),
            known: planet.claim.is_some(),
            flag: planet.claim.as_ref().and_then(|c| c.flag).map(|f| {
                (
                    f.player,
                    (f.position - planet.motion.position).rotate_radians(-planet.motion.angle),
                )
            }),
            stage_seconds: planet.claim.as_ref().map(|c| c.stage_required_seconds),
        }
    }
    pub fn matches(&self, other: &Self) -> bool {
        self.planet == other.planet
            && self.revision == other.revision
            && self.owner == other.owner
            && self.known == other.known
            && self.stage_seconds == other.stage_seconds
            && match (self.flag, other.flag) {
                (Some((a, x)), Some((b, y))) => a == b && x.distance_to(y) < 0.5,
                (None, None) => true,
                _ => false,
            }
    }
}

#[derive(Clone)]
pub(super) struct LocalEvidence {
    pub remote: bool,
    pub key: PlanetKey,
    pub site: LandingSiteId,
    pub tick: u64,
    pub gravity: f32,
    pub costs: Option<PhaseCosts>,
    pub reason: Option<&'static str>,
    pub choice: Option<(u64, u64)>,
    pub route_source_tick: Option<u64>,
    pub route_validated_tick: Option<u64>,
}

pub(super) fn no_flag_costs() -> PhaseCosts {
    PhaseCosts {
        landing: 17.866_667,
        exit: 1.0 / 60.0,
        outbound: 4.0 / 60.0,
        claim: 3.0 + 1.0 / 60.0,
        return_board: 2.0 / 60.0,
        departure: 3.766_667,
    }
}

/// The existing empirical v1 phase medians, without extrapolating walking
/// domains. These predict successful completion, not success probability.
pub(super) fn local_costs(
    o: &MissionObservationV1,
    site: LandingSiteId,
) -> Result<PhaseCosts, &'static str> {
    let p = &o.local.combat.recovery.flight.pilot;
    let claim = p.planet.claim.as_ref().ok_or("ownership unknown")?;
    if claim.owner == Some(p.owner) {
        return Err("planet already owned");
    }
    if claim.flag.is_none() {
        if claim.owner.is_some() || (claim.stage_required_seconds - 3.0).abs() > 0.001 {
            return Err("claim state outside no-flag calibration");
        }
        return Ok(no_flag_costs());
    }
    let survey = o
        .local
        .landing_objective
        .as_ref()
        .ok_or("objective route unmeasured")?;
    if survey.version != 1
        || survey.actor != p.owner
        || !survey.is_current(p.tick)
        || LandingObjective::read(p).is_none_or(|objective| !survey.objective.matches(objective))
    {
        return Err("objective evidence stale or incompatible");
    }
    let route = survey
        .sites
        .iter()
        .take(8)
        .find(|r| r.site == Some(site))
        .ok_or("site round trip unmeasured")?;
    if route.cost().is_none() {
        return Err("round trip incomplete");
    }
    let returning = route.returning.as_ref().unwrap();
    if route.crossing.is_some()
        || route.outbound.jumps != 0
        || returning.jumps != 0
        || route.outbound.flights != 0
        || returning.flights != 0
    {
        return Err("powered or jumping trip outside timing calibration");
    }
    let outbound = route.outbound.length / 5.0;
    let returning = returning.length / 5.0;
    if !(0.0..=10.599_817).contains(&outbound)
        || !(0.0..=10.118_012).contains(&returning)
        || (claim.stage_required_seconds - 3.0).abs() > 0.001
    {
        return Err("walking or claim reference outside calibration");
    }
    Ok(PhaseCosts {
        landing: 23.033_333,
        exit: 1.0 / 60.0,
        outbound: outbound + 0.158_333_33,
        claim: 6.0 - 0.5 / 60.0,
        return_board: returning + 2.0 / 60.0,
        departure: 3.816_666_6,
    })
}

pub(super) fn observe_local(
    o: &MissionObservationV1,
    mission: &MissionTelemetry,
) -> Option<LocalEvidence> {
    let p = &o.local.combat.recovery.flight.pilot;
    if !p.queries_ready
        || !p.ship_available
        || p.ship_form != ShipForm::Ship
        || !matches!(p.location, PilotLocation::Aboard(_))
    {
        return None;
    }
    let selected = mission.capture.as_ref().and_then(|c| c.site);
    let site = p
        .sites
        .iter()
        .take(64)
        .filter(|s| {
            s.id.planet == p.planet.index
                && s.revision == p.planet.revision
                && s.boarding_hatches.iter().any(Option::is_some)
                && o.local
                    .cover
                    .iter()
                    .take(64)
                    .any(|c| c.site == s.id && c.grounded && c.approach && c.departure)
        })
        .min_by(|a, b| {
            (Some(b.id) == selected)
                .cmp(&(Some(a.id) == selected))
                .then_with(|| {
                    let cost = |id| local_costs(o, id).map_or(f32::INFINITY, |c| c.total());
                    cost(a.id).total_cmp(&cost(b.id))
                })
                .then_with(|| a.id.bearing.cmp(&b.id.bearing))
        })?;
    let costs = local_costs(o, site.id);
    Some(LocalEvidence {
        remote: false,
        key: PlanetKey::read(&p.planet),
        site: site.id,
        tick: p.tick,
        gravity: p.gravity.length(),
        costs: costs.as_ref().ok().cloned(),
        reason: costs.err(),
        choice: (selected == Some(site.id)).then(|| {
            let visit = mission
                .events
                .iter()
                .rev()
                .find(|e| e.kind == "selected" && e.planet == mission.target)
                .map_or(p.tick, |e| e.tick);
            (visit, p.tick)
        }),
        route_source_tick: o.local.landing_objective.as_ref().map(|s| s.tick),
        route_validated_tick: o
            .local
            .landing_objective
            .as_ref()
            .map(|s| s.validated_tick.unwrap_or(s.tick)),
    })
}
