//! Bounded solar forecasts for an ordinary material landing and departure.
use engine_core::Vec2;
use scenario_spacewars::surface_sortie::{
    combat::TacticalSortieObservationV1, pilot::PilotLandingSite,
};
use serde::Serialize;

const HULL_MARGIN: f32 = 10.0;
const CIRCLE_HEIGHT: f32 = 60.0;
const CIRCLE_SPEED: f32 = 30.0;
// A normal descent, claim and boarding. Delayed approaches are reassessed.
const SURFACE_SECONDS: f32 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SolarLandingPlan {
    pub forecast_tick: u64,
    pub arrival_seconds: f32,
    pub surface_seconds: f32,
    pub side: f32,
    /// Clearances beyond the observed heat boundary plus a hull margin.
    pub approach_clearance: f32,
    pub parked_clearance: f32,
    pub departure_clearance: f32,
    pub departure_side: f32,
}
impl SolarLandingPlan {
    pub fn safe(self) -> bool {
        self.approach_clearance >= 0.0
            && self.parked_clearance >= 0.0
            && self.departure_clearance >= 0.0
    }
}

pub(crate) fn directed_angle(short: f32, side: f32) -> f32 {
    // Once aligned, allow small corrections in either direction without
    // commanding an extra lap after overshooting the selected bearing.
    if short.abs() < 0.2 || short.signum() == side.signum() {
        short
    } else {
        short + side * std::f32::consts::TAU
    }
}

pub(crate) fn distance_to_segment(point: Vec2, a: Vec2, b: Vec2) -> f32 {
    let delta = b - a;
    let t = ((point - a).dot(delta) / delta.length_squared().max(0.0001)).clamp(0.0, 1.0);
    point.distance_to(a + delta * t)
}

fn forecast(o: &TacticalSortieObservationV1, offset: Vec2, seconds: f32) -> Vec2 {
    let motion = o.combat.recovery.flight.pilot.planet.motion;
    let center = if let (Some(sun), Some(omega)) = (o.sun, o.planet_orbit_omega) {
        sun.position + (motion.position - sun.position).rotate_radians(omega * seconds)
    } else {
        motion.position + motion.velocity * seconds
    };
    center + offset.rotate_radians(motion.spin * seconds)
}

pub(crate) fn assess(
    o: &TacticalSortieObservationV1,
    site: PilotLandingSite,
    side: f32,
    circling: bool,
) -> Option<SolarLandingPlan> {
    let sun = o.sun?;
    let p = &o.combat.recovery.flight.pilot;
    let safe_radius = sun.heat_radius.max(sun.radius) + HULL_MARGIN;
    let up = (p.ship.position - p.planet.motion.position).normalized();
    let site_offset = site.vehicle_position - p.planet.motion.position;
    let direction = site_offset.normalized();
    let short = (up.x * direction.y - up.y * direction.x).atan2(up.dot(direction));
    let angle = directed_angle(short, side);
    let circle_radius = p.planet.radius + CIRCLE_HEIGHT;
    let travel_seconds = if circling {
        angle.abs() * circle_radius / CIRCLE_SPEED
    } else {
        p.ship.position.distance_to(site.vehicle_position) / 12.0
    };
    let clearance = |a, b| distance_to_segment(sun.position, a, b) - safe_radius;
    let mut approach = f32::MAX;
    if circling {
        let mut previous = forecast(o, up * circle_radius, 0.0);
        approach = approach.min(clearance(p.ship.position, previous));
        // Short segments include their chords, making the sampling conservative
        // for the circular arc rather than missing a crossing between points.
        let steps = (angle.abs() / 0.08).ceil().max(1.0) as u32;
        for step in 1..=steps {
            let fraction = step as f32 / steps as f32;
            let point = forecast(
                o,
                up.rotate_radians(angle * fraction) * circle_radius,
                travel_seconds * fraction,
            );
            approach = approach.min(clearance(previous, point));
            previous = point;
        }
        approach = approach.min(clearance(
            previous,
            forecast(o, site_offset, travel_seconds),
        ));
    } else {
        approach = clearance(p.ship.position, forecast(o, site_offset, travel_seconds));
    }
    let mut parked = f32::MAX;
    let mut departures = [f32::MAX; 2];
    for step in 0..=12 {
        let seconds = travel_seconds + SURFACE_SECONDS * step as f32 / 12.0;
        let center = forecast(o, site_offset, seconds);
        parked = parked.min(center.distance_to(sun.position) - safe_radius);
        // The pilot lifts off its feet before banking. Check that short lift
        // and both ordinary 18-up/38-side departure corridors, rather than
        // assuming a long purely radial climb toward the sun.
        let lift = forecast(o, site_offset + site.normal * 20.0, seconds);
        let tangent = Vec2::new(-site.normal.y, site.normal.x);
        for (index, side) in [-1.0, 1.0].into_iter().enumerate() {
            let exit = forecast(
                o,
                site_offset + site.normal * 92.0 + tangent * side * 152.0,
                seconds,
            );
            departures[index] = departures[index]
                .min(clearance(center, lift))
                .min(clearance(lift, exit));
        }
    }
    Some(SolarLandingPlan {
        forecast_tick: p.tick,
        arrival_seconds: travel_seconds,
        surface_seconds: SURFACE_SECONDS,
        side,
        approach_clearance: approach,
        parked_clearance: parked,
        departure_clearance: departures[0].max(departures[1]),
        departure_side: if departures[0] > departures[1] {
            -1.0
        } else {
            1.0
        },
    })
}

pub(crate) fn departure_side(o: &TacticalSortieObservationV1, preferred: f32) -> f32 {
    let Some(sun) = o.sun else { return preferred };
    let p = &o.combat.recovery.flight.pilot;
    let up = (p.ship.position - p.planet.motion.position).normalized();
    let tangent = Vec2::new(-up.y, up.x);
    let clearance = |side| {
        let velocity = p.planet.velocity_at(p.ship.position) + up * 18.0 + tangent * side * 38.0;
        distance_to_segment(
            sun.position,
            p.ship.position,
            p.ship.position + velocity * 3.0,
        ) - sun.heat_radius
            - HULL_MARGIN
    };
    if clearance(preferred) < 20.0 && clearance(-preferred) > clearance(preferred) {
        -preferred
    } else {
        preferred
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use scenario_spacewars::surface_sortie::{SolarHazard, SurfaceSortieScenario};
    use std::time::Duration;

    fn fixture() -> (TacticalSortieObservationV1, PilotLandingSite) {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let mut o = state.tactical_sortie_observation(0, None);
        o.sun = Some(SolarHazard {
            position: Vec2::ZERO,
            radius: 200.0,
            heat_radius: 224.0,
        });
        o.planet_orbit_omega = None;
        let p = &mut o.combat.recovery.flight.pilot;
        p.planet.motion.position = Vec2::new(320.0, 0.0);
        p.planet.motion.velocity = Vec2::ZERO;
        p.planet.motion.spin = 0.0;
        p.planet.radius = 60.0;
        p.ship.position = Vec2::new(320.0, 120.0);
        let site = p.sites[0];
        (o, site)
    }

    #[test]
    fn safe_footing_does_not_make_a_sunward_departure_safe() {
        let (o, mut site) = fixture();
        site.vehicle_position = Vec2::new(260.0, 0.0);
        site.normal = -Vec2::X;
        let plan = assess(&o, site, 1.0, false).unwrap();
        assert!(plan.parked_clearance > 0.0);
        assert!(plan.departure_clearance < 0.0);
        assert!(!plan.safe());
        site.vehicle_position = Vec2::new(380.0, 0.0);
        site.normal = Vec2::X;
        assert!(assess(&o, site, -1.0, true).unwrap().safe());
    }

    #[test]
    fn longer_arc_can_avoid_the_sun_when_the_short_arc_crosses_it() {
        let (mut o, mut site) = fixture();
        let p = &mut o.combat.recovery.flight.pilot;
        p.ship.position =
            p.planet.motion.position + Vec2::X.rotate_radians(100_f32.to_radians()) * 120.0;
        site.normal = Vec2::X.rotate_radians(-100_f32.to_radians());
        site.vehicle_position = p.planet.motion.position + site.normal * 60.0;
        assert!(!assess(&o, site, 1.0, true).unwrap().safe());
        assert!(assess(&o, site, -1.0, true).unwrap().safe());
    }

    #[test]
    fn parked_forecast_accounts_for_both_surface_spin_and_orbit() {
        let (mut o, mut site) = fixture();
        site.normal = Vec2::X;
        site.vehicle_position = Vec2::new(380.0, 0.0);
        o.combat.recovery.flight.pilot.ship.position = Vec2::new(440.0, 0.0);
        assert!(assess(&o, site, 1.0, true).unwrap().safe());
        o.combat.recovery.flight.pilot.planet.motion.spin = std::f32::consts::PI / 20.0;
        assert!(!assess(&o, site, 1.0, true).unwrap().safe());
        o.planet_orbit_omega = Some(std::f32::consts::PI / 20.0);
        assert!(assess(&o, site, 1.0, true).unwrap().safe());
        o.sun = None;
        assert_eq!(assess(&o, site, 1.0, true), None);
    }

    #[test]
    fn forecast_uses_the_observed_orbit_and_matches_completed_world_motion() {
        let mut state = SurfaceSortieScenario::init_material_arena_trial(7, false, 0.0);
        let dt = Duration::from_nanos(16_666_667);
        SurfaceSortieScenario::step(&mut state, &[], dt);
        let o = state.tactical_sortie_observation(1, None);
        let sun = o.sun.unwrap();
        assert_eq!(sun.radius, 200.0);
        assert_eq!(sun.heat_radius, 224.0);
        let planet = o.combat.recovery.flight.pilot.planet.index;
        let before = state.observation(1);
        assert_eq!(state.tactical_sortie_observation(1, None), o);
        assert_eq!(state.observation(1), before);
        let position = forecast(&o, Vec2::ZERO, 1.0);
        for _ in 0..60 {
            SurfaceSortieScenario::step(&mut state, &[], dt);
        }
        let after = state.mission_observation(1, None);
        let actual = after.planets.iter().find(|p| p.index == planet).unwrap();
        assert!(actual.motion.position.distance_to(position) < 0.1);
    }
}
