use super::*;
use scenario_spacewars::surface_sortie::{
    landing_objective::LandingObjective, live_planning::FlagSurveyRequest,
    pilot::LANDING_SITE_COUNT,
};

#[derive(Clone)]
pub(super) struct RequestState {
    request: FlagSurveyRequest,
    key: PlanetKey,
    target: usize,
    visit: Option<u64>,
}

pub(super) fn request(
    retained: &mut Option<RequestState>,
    o: &MissionObservationV1,
    mission: &MissionTelemetry,
) -> Option<FlagSurveyRequest> {
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
            planet.index != target
                && planet.claim.as_ref().is_some_and(|c| {
                    c.owner.is_some_and(|owner| owner != p.owner)
                        && c.flag.is_some_and(|f| Some(f.player) == c.owner)
                        && (c.stage_required_seconds - 3.0).abs() <= 0.001
                })
        })
    else {
        *retained = None;
        return None;
    };
    let key = PlanetKey::read(planet);
    let visit = selection_tick(mission);
    let claim = planet.claim.as_ref().unwrap();
    let flag = claim.flag.unwrap();
    let local = (flag.position - planet.motion.position).rotate_radians(-planet.motion.angle);
    let objective = LandingObjective {
        planet: planet.index,
        revision: planet.revision,
        owner: flag.player,
        position: local,
        range: claim.flag_interaction_range - 0.2,
    };
    if retained.as_ref().is_none_or(|old| {
        old.target != target
            || old.visit != visit
            || !old.key.matches(&key)
            || !old.request.matches_objective(objective)
            || old.request.generation > p.tick
    }) {
        let bearing = (((-local.x).atan2(local.y).rem_euclid(std::f32::consts::TAU)
            * f32::from(LANDING_SITE_COUNT)
            / std::f32::consts::TAU)
            .round() as u8)
            % LANDING_SITE_COUNT;
        *retained = Some(RequestState {
            request: FlagSurveyRequest {
                generation: p.tick,
                objective,
                candidates: [1, LANDING_SITE_COUNT - 1].map(|offset| LandingSiteId {
                    planet: planet.index,
                    bearing: (bearing + offset) % LANDING_SITE_COUNT,
                }),
            },
            key,
            target,
            visit,
        });
    }
    // Keep an existing immutable job alive through local sensor demand. The
    // host starts live site measurements only while no local site is requested;
    // all incremental work is dispatched after local work and the evaluator.
    p.queries_ready.then(|| retained.as_ref().unwrap().request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scenario_spacewars::surface_sortie::PlanetFlagObservation;
    use scenario_spacewars::surface_sortie::pilot::LandingSiteQuery;

    #[test]
    fn only_an_enemy_alternative_is_requested_and_identity_survives_motion() {
        let (_, mut o, bot) = super::super::tests::fixture();
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.site_query = LandingSiteQuery::NotRequested;
        p.landing.supported_feet = 0;
        let mut mission = bot.telemetry().clone();
        mission.target = Some(p.planet.index);
        let mut host = MissionEvaluator::new(1);
        assert!(host.flag_request(&o, &mission).is_none());
        let planet = o
            .planets
            .iter_mut()
            .find(|p| Some(p.index) != mission.target)
            .unwrap();
        let claim = planet.claim.as_mut().unwrap();
        claim.owner = Some(PlayerId::PLAYER_2);
        claim.flag = Some(PlanetFlagObservation {
            player: PlayerId::PLAYER_2,
            position: planet.motion.position + Vec2::Y * planet.radius,
            normal: Vec2::Y,
            raised_fraction: 1.0,
        });
        let request = host.flag_request(&o, &mission).unwrap();
        assert_ne!(Some(request.objective.planet), mission.target);
        assert_ne!(request.candidates[0], request.candidates[1]);
        let planet = o
            .planets
            .iter_mut()
            .find(|p| p.index == request.objective.planet)
            .unwrap();
        planet.motion.position.x += 10.0;
        planet
            .claim
            .as_mut()
            .unwrap()
            .flag
            .as_mut()
            .unwrap()
            .position
            .x += 10.0;
        o.local.combat.recovery.flight.pilot.tick += 1;
        assert_eq!(host.flag_request(&o, &mission), Some(request));
        assert_eq!(host.clone().flag_request(&o, &mission), Some(request));
        o.planets
            .iter_mut()
            .find(|p| p.index == request.objective.planet)
            .unwrap()
            .claim
            .as_mut()
            .unwrap()
            .flag
            .as_mut()
            .unwrap()
            .position
            .x += 0.1;
        o.local.combat.recovery.flight.pilot.tick += 1;
        let moved = host.flag_request(&o, &mission).unwrap();
        assert_ne!(moved.generation, request.generation);
        assert!(!request.matches_objective(moved.objective));
        let request = moved;
        o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Survey;
        assert_eq!(host.flag_request(&o, &mission), Some(request));
        o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::NotRequested;
        assert_eq!(host.flag_request(&o, &mission), Some(request));
        o.planets
            .iter_mut()
            .find(|p| p.index == request.objective.planet)
            .unwrap()
            .revision += 1;
        assert_ne!(host.flag_request(&o, &mission), Some(request));
        host.reset();
        assert!(host.actors.is_empty());
    }
}
