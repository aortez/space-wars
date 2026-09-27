//! Stateless timing at a witnessed native choice. Cover is context, not timing.
use super::{MAX_PLANETS, MissionTelemetry, PhaseCosts, model, selection_tick};
use crate::{mission_pilot::MissionGoal, tactical_sortie::LandingChoiceComparison};
use engine_core::Vec2;
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        PilotLocation, PlanetClaimPhase, SpacelingId, VehicleId,
        combat::{CombatTarget, LandingCover},
        mission::MissionObservationV1,
        pilot::{PilotLandingSite, PilotMotion, PilotPlanetObservation},
    },
};
use serde::Serialize;

/// Existing successful-trip medians, conditional on execution. This is neither
/// a success probability nor remaining time, and never enters evaluator caches.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NeutralCaptureTiming {
    pub model: &'static str,
    pub policy: &'static str,
    pub actor: PlayerId,
    pub vehicle: VehicleId,
    pub spaceling: SpacelingId,
    pub source_tick: u64,
    pub visit_tick: Option<u64>,
    pub started_tick: Option<u64>,
    pub completed_sorties: u32,
    pub attempt_counters: Option<[u32; 8]>,
    pub planet: PilotPlanetObservation,
    pub ship: PilotMotion,
    pub gravity: Vec2,
    pub site: Option<PilotLandingSite>,
    pub selected: crate::tactical_sortie::LandingDirectionAssessment,
    pub opponent: Option<CombatTarget>,
    pub opponent_distance: Option<f32>,
    pub source_exposed: bool,
    pub cover: Option<LandingCover>,
    pub phases: Option<PhaseCosts>,
    pub total_seconds: Option<f32>,
    pub unknown: Option<&'static str>,
}

pub(crate) fn neutral_capture_timing(
    o: &MissionObservationV1,
    m: &MissionTelemetry,
    choice: &LandingChoiceComparison,
) -> NeutralCaptureTiming {
    let p = &o.local.combat.recovery.flight.pilot;
    let site = p
        .sites
        .iter()
        .find(|s| s.id == choice.selected.site)
        .copied();
    let unknown = domain(o, m, choice, site).err();
    let phases = unknown.is_none().then(model::no_flag_costs);
    NeutralCaptureTiming {
        model: "neutral_capture_timing_v1",
        policy: m.policy,
        actor: p.owner,
        vehicle: p.vehicle,
        spaceling: p.spaceling,
        source_tick: p.tick,
        visit_tick: selection_tick(m),
        started_tick: m.capture.as_ref().and_then(|c| c.started_tick),
        completed_sorties: m.completed_sorties,
        attempt_counters: m.capture.as_ref().map(|c| {
            [
                c.replans,
                c.invalidations,
                c.cover_replans,
                c.solar_replans,
                c.circling_replans,
                c.objective_replans,
                c.landing.invalidations,
                c.landing.landing_retries,
            ]
        }),
        planet: p.planet.clone(),
        ship: p.ship,
        gravity: p.gravity,
        site,
        selected: choice.selected,
        opponent: o.local.combat.target,
        opponent_distance: o
            .local
            .combat
            .target
            .map(|t| t.motion.position.distance_to(p.ship.position)),
        source_exposed: choice.exposed,
        cover: o
            .local
            .cover
            .iter()
            .find(|c| c.site == choice.selected.site)
            .copied(),
        total_seconds: phases.as_ref().map(PhaseCosts::total),
        phases,
        unknown,
    }
}

fn vector(v: Vec2) -> bool {
    v.x.is_finite() && v.y.is_finite()
}
fn motion(m: PilotMotion) -> bool {
    vector(m.position) && vector(m.velocity) && m.angle.is_finite() && m.spin.is_finite()
}

fn domain(
    o: &MissionObservationV1,
    m: &MissionTelemetry,
    choice: &LandingChoiceComparison,
    site: Option<PilotLandingSite>,
) -> Result<(), &'static str> {
    let p = &o.local.combat.recovery.flight.pilot;
    let c = m.capture.as_ref().ok_or("capture unavailable")?;
    if m.policy != "material_mission_v13"
        || c.policy != "tactical_sortie_v11"
        || !choice.commit_descent
    {
        return Err("policy outside timing domain");
    }
    if o.version != 1
        || o.local.version != 1
        || o.local.combat.version != 2
        || o.local.combat.recovery.version != 1
        || o.local.combat.recovery.flight.version != 2
        || o.local.combat.recovery.flight.flight.version != 1
        || p.version != 1
        || !p.controls_armed
        || !p.queries_ready
        || !p.ship_available
        || p.ship_form != ShipForm::Ship
        || !p.ship_health.is_finite()
        || p.ship_health <= 0.0
        || p.location != PilotLocation::Aboard(p.vehicle)
    {
        return Err("source ship unavailable");
    }
    if m.goal != MissionGoal::Capture
        || m.target != Some(p.planet.index)
        || m.recovery.is_some()
        || selection_tick(m).is_none_or(|t| t > p.tick)
        || c.started_tick.is_none_or(|t| t > p.tick)
        || c.failed_tick.is_some()
        || c.failure.is_some()
        || c.completed_tick.is_some()
        || c.landing.landed_tick.is_some()
        || c.landing.claimed_tick.is_some()
        || c.landing.boarded_tick.is_some()
        || choice.tick != p.tick
        || choice.planet != p.planet.index
        || choice.revision != p.planet.revision
        || c.site != Some(choice.selected.site)
        || c.acquisition.as_ref().is_none_or(|a| {
            a.tick != p.tick
                || a.reason != "selected_site"
                || a.planet != p.planet.index
                || a.revision != p.planet.revision
                || a.selected_site != c.site
        })
    {
        return Err("not a fresh capture choice");
    }
    if o.planets.len() > MAX_PLANETS
        || o.planets
            .iter()
            .filter(|q| q.index == p.planet.index)
            .count()
            != 1
        || o.planets.iter().find(|q| q.index == p.planet.index) != Some(&p.planet)
        || !motion(p.ship)
        || !motion(p.planet.motion)
        || !vector(p.gravity)
        || !p.planet.radius.is_finite()
        || p.planet.radius <= 0.0
    {
        return Err("source geometry unavailable");
    }
    let claim = p.planet.claim.as_ref().ok_or("ownership unknown")?;
    if claim.planet != p.planet.index
        || claim.owner.is_some()
        || claim.flag.is_some()
        || claim.claimant.is_some()
        || claim.phase != PlanetClaimPhase::Idle
        || claim.progress != 0.0
        || !claim.stage_required_seconds.is_finite()
        || (claim.stage_required_seconds - 3.0).abs() > 0.001
        || !claim.flag_interaction_range.is_finite()
        || claim.flag_interaction_range <= 0.0
    {
        return Err("claim state outside neutral timing domain");
    }
    if o.local.combat.target.is_some_and(|t| {
        !motion(t.motion) || !t.motion.position.distance_to(p.ship.position).is_finite()
    }) {
        return Err("opponent geometry unavailable");
    }
    if choice.exposed {
        return Err("source exposed to opponent");
    }
    let site = site.ok_or("selected material site unavailable")?;
    if site.id.planet != p.planet.index
        || site.revision != p.planet.revision
        || ![
            site.local_position,
            site.position,
            site.normal,
            site.velocity,
            site.vehicle_position,
            site.hatch_position,
        ]
        .into_iter()
        .all(vector)
        || site.normal.length_squared() < 0.0001
        || !site.boarding_hatches.into_iter().flatten().all(vector)
        || !site.boarding_hatches.iter().any(Option::is_some)
    {
        return Err("selected material or hatch unavailable");
    }
    if o.local.planet_orbit_omega.is_some_and(|v| !v.is_finite())
        || o.local.sun.is_some_and(|s| {
            !vector(s.position)
                || !s.radius.is_finite()
                || s.radius <= 0.0
                || !s.heat_radius.is_finite()
                || s.heat_radius < 0.0
        })
    {
        return Err("solar geometry unavailable");
    }
    match (o.local.sun, choice.selected.solar) {
        (None, None) => (),
        (Some(_), Some(s))
            if s.forecast_tick == p.tick
                && Some(s.side) == choice.selected.side
                && s.safe()
                && s.arrival_seconds >= 0.0
                && s.surface_seconds > 0.0
                && [
                    s.arrival_seconds,
                    s.surface_seconds,
                    s.side,
                    s.approach_clearance,
                    s.parked_clearance,
                    s.departure_clearance,
                    s.departure_side,
                ]
                .into_iter()
                .all(f32::is_finite) => {}
        _ => return Err("selected solar plan unavailable"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BrainReset,
        mission_policy::{MissionBot, MissionPolicy},
    };
    use engine_common::Scenario;
    use scenario_spacewars::surface_sortie::{SurfaceSortieScenario, combat::LandingCover};
    use std::time::Duration;

    fn fixture() -> (MissionBot, MissionObservationV1, LandingChoiceComparison) {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let mut o = state.mission_observation(0, None);
        o.local.combat.target = None;
        o.opponent = None;
        o.sun = None;
        o.local.sun = None;
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.controls_armed = true;
        o.planets = vec![p.planet.clone()];
        o.local.cover = p
            .sites
            .iter()
            .map(|s| LandingCover {
                site: s.id,
                grounded: false,
                approach: false,
                departure: false,
            })
            .collect();
        let mut bot = MissionBot::new(
            MissionPolicy::ValuePlanner,
            BrainReset {
                actor: p.owner,
                episode_seed: 42,
            },
            Default::default(),
        );
        bot.intent(&o);
        o.local.combat.recovery.flight.pilot.tick += 1;
        bot.intent(&o);
        let site = bot
            .telemetry()
            .capture
            .as_ref()
            .expect("capture")
            .site
            .expect("choice");
        let choice = bot.landing_choice_comparison(&o, site).unwrap();
        (bot, o, choice)
    }

    #[test]
    fn cover_is_separate_and_observation_does_not_mutate_future_controls() {
        let (mut bot, mut o, choice) = fixture();
        let before = format!("{bot:?}");
        let mut baseline = bot.clone();
        let (c, t) = bot
            .landing_choice_with_neutral_timing(&o, choice.selected.site)
            .unwrap();
        assert_eq!(c, choice);
        assert_eq!(t.unknown, None);
        assert_eq!(t.phases, Some(model::no_flag_costs()));
        assert!(!t.cover.unwrap().grounded);
        assert_eq!(format!("{bot:?}"), before);
        o.local.combat.recovery.flight.pilot.tick += 1;
        assert!(
            bot.landing_choice_with_neutral_timing(&o, choice.selected.site)
                .is_err()
        );
        assert_eq!(bot.intent(&o), baseline.intent(&o));
        assert_eq!(bot.telemetry(), baseline.telemetry());
    }

    #[test]
    fn unknowns_do_not_gain_numeric_costs_from_missing_or_changed_evidence() {
        for mutation in 0..17 {
            let (bot, mut o, mut choice) = fixture();
            let mut m = bot.telemetry().clone();
            let p = &mut o.local.combat.recovery.flight.pilot;
            match mutation {
                0 => m.policy = "material_mission_v12",
                1 => p.queries_ready = false,
                2 => p.ship_health = f32::NAN,
                3 => p.gravity.x = f32::NAN,
                4 => p.planet.radius = -1.0,
                5 => o.planets[0].revision += 1,
                6 => p
                    .sites
                    .iter_mut()
                    .for_each(|s| s.boarding_hatches = [None, None]),
                7 => p
                    .sites
                    .iter_mut()
                    .for_each(|s| s.boarding_hatches = [Some(Vec2::new(f32::INFINITY, 0.0)), None]),
                8 => choice.exposed = true,
                9 => choice.tick += 1,
                10 => m.capture.as_mut().unwrap().sortie.started_tick = Some(p.tick + 1),
                11 => m.capture.as_mut().unwrap().sortie.landing.landed_tick = Some(p.tick),
                12 => m.events.clear(),
                13 => p.planet.claim.as_mut().unwrap().owner = Some(p.owner),
                14 => p.planet.claim.as_mut().unwrap().progress = 0.1,
                15 => p.planet.claim.as_mut().unwrap().stage_required_seconds = f32::NAN,
                16 => p.planet.claim.as_mut().unwrap().flag_interaction_range = 0.0,
                _ => unreachable!(),
            }
            if mutation != 5 {
                o.planets[0] = p.planet.clone();
            }
            let t = neutral_capture_timing(&o, &m, &choice);
            assert!(
                t.phases.is_none() && t.total_seconds.is_none() && t.unknown.is_some(),
                "mutation {mutation}"
            );
        }
    }

    #[test]
    fn absent_cover_and_one_clear_hatch_still_have_timing() {
        let (bot, mut o, choice) = fixture();
        o.local.cover.clear();
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.sites
            .iter_mut()
            .for_each(|s| s.boarding_hatches = [None, Some(s.hatch_position)]);
        let t = neutral_capture_timing(&o, bot.telemetry(), &choice);
        assert_eq!(t.unknown, None);
        assert!(t.cover.is_none() && t.phases.is_some());
    }

    #[test]
    fn public_wrapper_rejects_unsupported_nested_versions_and_disarmed_source() {
        for mutation in 0..5 {
            let (bot, mut o, choice) = fixture();
            match mutation {
                0 => o.local.combat.version += 1,
                1 => o.local.combat.recovery.version += 1,
                2 => o.local.combat.recovery.flight.version += 1,
                3 => o.local.combat.recovery.flight.flight.version += 1,
                4 => o.local.combat.recovery.flight.pilot.controls_armed = false,
                _ => unreachable!(),
            }
            let (_, t) = bot
                .landing_choice_with_neutral_timing(&o, choice.selected.site)
                .unwrap();
            assert!(t.phases.is_none() && t.unknown.is_some());
        }
    }

    #[test]
    fn solar_requires_current_finite_safe_direction() {
        use crate::landing_safety::SolarLandingPlan;
        use scenario_spacewars::surface_sortie::SolarHazard;
        for mutation in 0..10 {
            let (bot, mut o, mut choice) = fixture();
            o.local.sun = Some(SolarHazard {
                position: Vec2::ZERO,
                radius: 5.0,
                heat_radius: 10.0,
            });
            let mut s = SolarLandingPlan {
                forecast_tick: choice.tick,
                arrival_seconds: 2.0,
                surface_seconds: 25.0,
                side: choice.selected.side.unwrap(),
                approach_clearance: 1.0,
                parked_clearance: 1.0,
                departure_clearance: 1.0,
                departure_side: 1.0,
            };
            match mutation {
                0 => (),
                1 => s.forecast_tick += 1,
                2 => s.side = -s.side,
                3 => s.parked_clearance = -0.001,
                4 => s.departure_clearance = f32::INFINITY,
                5 => s.arrival_seconds = f32::NAN,
                6 => o.local.sun.as_mut().unwrap().radius = f32::NAN,
                7 => o.local.sun.as_mut().unwrap().position.x = f32::INFINITY,
                8 => o.local.planet_orbit_omega = Some(f32::NAN),
                9 => o.local.sun.as_mut().unwrap().heat_radius = -1.0,
                _ => unreachable!(),
            }
            choice.selected.solar = Some(s);
            let t = neutral_capture_timing(&o, bot.telemetry(), &choice);
            assert_eq!(t.phases.is_some(), mutation == 0);
        }
    }

    #[test]
    fn overflowing_opponent_distance_stays_unknown() {
        let (bot, mut o, choice) = fixture();
        o.local.combat.target = Some(CombatTarget {
            owner: PlayerId::PLAYER_2,
            motion: PilotMotion {
                position: Vec2::new(f32::MAX, 0.0),
                ..o.local.combat.recovery.flight.pilot.ship
            },
            health: 100.0,
            health_fraction: 1.0,
            ship_form: Some(ShipForm::Ship),
            visible: true,
            ground_occluded: false,
        });
        let (_, t) = bot
            .landing_choice_with_neutral_timing(&o, choice.selected.site)
            .unwrap();
        assert_eq!(t.unknown, Some("opponent geometry unavailable"));
        assert!(t.phases.is_none());
    }
}
