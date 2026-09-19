//! Bounded, observation-only comparison of the flight after breaking contact.
use super::*;
use scenario_spacewars::surface_sortie::flight::{FlightControlLimits, WING_TRANSITION_SECONDS};

const FORECAST_TICKS: u64 = 6 * 60;
const MAX_DESTINATIONS: usize = 4;
const DT: f32 = 1.0 / 60.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HandoffTelemetry {
    pub tick: u64,
    pub evaluated_ticks: u64,
    pub candidates: Vec<TransferForecast>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferForecast {
    pub planet: usize,
    pub ticks: u64,
    pub minimum_range: f32,
    pub minimum_clearance: f32,
    pub final_distance: f32,
    pub completed: bool,
    pub samples: Vec<TransferForecastSample>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TransferForecastSample {
    pub after_ticks: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub opponent: Vec2,
}

impl MaterialMissionPilot {
    pub(crate) fn configure_handoff_probe(&mut self, enabled: bool) {
        if let Some(d) = &mut self.telemetry.disengagement {
            d.handoff_probe = enabled;
            d.handoff = None;
        } else {
            assert!(!enabled, "enable disengagement before its handoff probe");
        }
    }

    pub(super) fn probe_disengagement_handoff(&mut self, o: &MissionObservationV1) {
        if !self.telemetry.disengagement.as_ref().unwrap().handoff_probe {
            return;
        }
        let p = &o.local.combat.recovery.flight.pilot;
        let mut destinations: Vec<_> = o
            .planets
            .iter()
            .filter(|planet| {
                planet
                    .claim
                    .as_ref()
                    .is_none_or(|claim| claim.owner != Some(p.owner))
                    && !self
                        .deferred
                        .iter()
                        .any(|(index, until)| *index == planet.index && p.tick < *until)
            })
            .collect();
        destinations.sort_by(|a, b| {
            a.motion
                .position
                .distance_to(p.ship.position)
                .total_cmp(&b.motion.position.distance_to(p.ship.position))
                .then(a.index.cmp(&b.index))
        });
        let candidates: Vec<_> = destinations
            .into_iter()
            .take(MAX_DESTINATIONS)
            .map(|planet| self.forecast_transfer(o, planet.index))
            .collect();
        let evaluated_ticks = candidates.iter().map(|f| f.ticks).sum();
        self.telemetry.disengagement.as_mut().unwrap().handoff = Some(HandoffTelemetry {
            tick: p.tick,
            evaluated_ticks,
            candidates,
        });
    }

    fn prepare_handoff_transfer(&mut self, tick: u64, planet: usize) {
        self.telemetry.target = Some(planet);
        self.selected_tick = tick;
        self.progress_tick = tick;
        self.best_distance = f32::INFINITY;
        self.solar_detour = None;
        self.pursuit_climb = None;
        self.departure_obstacle = None;
    }

    fn forecast_transfer(&self, o: &MissionObservationV1, destination: usize) -> TransferForecast {
        let mut preview = self.clone();
        preview.telemetry.disengagement = None;
        preview.telemetry.pursuit = None;
        preview.next_pursuit_tick = u64::MAX;
        preview.capture = None;
        preview.recovery = None;
        let mut predicted = o.clone();
        let start_tick = o.local.combat.recovery.flight.pilot.tick;
        preview.prepare_handoff_transfer(start_tick, destination);
        let enemy = o.local.combat.target.unwrap().motion;
        let mut report = TransferForecast {
            planet: destination,
            ticks: 0,
            minimum_range: enemy
                .position
                .distance_to(o.local.combat.recovery.flight.pilot.ship.position),
            minimum_clearance: f32::MAX,
            final_distance: f32::MAX,
            completed: false,
            samples: Vec::new(),
        };
        for tick in 0..FORECAST_TICKS {
            let intent = preview.choose(&predicted);
            // This estimator covers free flight only. Actual local tasks retain
            // their material queries, landing gates and separate planners.
            if !matches!(
                preview.telemetry.goal,
                MissionGoal::Transfer | MissionGoal::Launch
            ) {
                break;
            }
            advance_forecast(&mut predicted, intent);
            let elapsed = (tick + 1) as f32 * DT;
            let p = &predicted.local.combat.recovery.flight.pilot;
            let opponent = enemy.position + enemy.velocity * elapsed;
            report.minimum_range = report
                .minimum_range
                .min(p.ship.position.distance_to(opponent));
            for (center, radius) in predicted
                .planets
                .iter()
                .map(|planet| (planet.motion.position, planet.radius))
                .chain(predicted.sun.map(|sun| (sun.position, sun.radius)))
            {
                report.minimum_clearance = report
                    .minimum_clearance
                    .min(p.ship.position.distance_to(center) - radius - 65.0);
            }
            report.ticks = tick + 1;
            if (tick + 1) % 60 == 0 {
                report.samples.push(TransferForecastSample {
                    after_ticks: tick + 1,
                    position: p.ship.position,
                    velocity: p.ship.velocity,
                    opponent,
                });
            }
        }
        report.completed = report.ticks == FORECAST_TICKS;
        if let Some(target) = predicted
            .planets
            .iter()
            .find(|planet| planet.index == destination)
        {
            report.final_distance = predicted
                .local
                .combat
                .recovery
                .flight
                .pilot
                .ship
                .position
                .distance_to(target.motion.position);
        }
        report
    }
}

// Reuse the observed control envelope and ordinary motor commands. This omits
// contacts, changing gravity, asteroid impacts and opponent decisions; it never
// advances the authoritative world or manufactures material query results.
fn advance_forecast(o: &mut MissionObservationV1, intent: CombatIntent) {
    let f = &mut o.local.combat.recovery.flight;
    let p = &mut f.pilot;
    let controls = intent.flight.controls;
    let closed = intent.flight.wings.closed;
    f.flight.sweep += ((if closed { 1.0 } else { 0.0 }) - f.flight.sweep)
        .clamp(-DT / WING_TRANSITION_SECONDS, DT / WING_TRANSITION_SECONDS);
    let limits = FlightControlLimits::for_sweep(f.flight.sweep);
    let forward = Vec2::Y.rotate_radians(p.ship.angle);
    let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
    let thrust = controls.primary_held || closed && !controls.brake_held;
    let acceleration = p.gravity
        + if thrust {
            forward * limits.thrust_acceleration * limits.thrust_fraction(relative.dot(forward))
        } else {
            Vec2::ZERO
        }
        + if controls.brake_held {
            limits.braking(relative)
        } else {
            Vec2::ZERO
        };
    let strength = if controls.horizontal != 0.0 || controls.brake_held {
        1.0
    } else {
        0.25
    };
    let desired_spin = -controls.horizontal * limits.turn_speed;
    p.ship.spin += (desired_spin - p.ship.spin).clamp(
        -limits.turn_acceleration * strength * DT,
        limits.turn_acceleration * strength * DT,
    );
    p.ship.angle += p.ship.spin * DT;
    p.ship.velocity += acceleration * DT;
    p.ship.position += p.ship.velocity * DT;
    p.tick += 1;
    for planet in &mut o.planets {
        planet.motion.position += planet.motion.velocity * DT;
        planet.motion.angle += planet.motion.spin * DT;
    }
    if let Some(planet) = o.planets.iter().min_by(|a, b| {
        (p.ship.position.distance_to(a.motion.position) - a.radius)
            .total_cmp(&(p.ship.position.distance_to(b.motion.position) - b.radius))
    }) {
        p.planet = planet.clone();
    }
    p.landing.assist_strength = 0.0;
    f.flight.wings_closed = closed;
    f.flight.limits = limits;
    let relative = p.ship.velocity - p.planet.velocity_at(p.ship.position);
    f.flight.relative_speed = relative.length();
    f.flight.forward_speed = relative.dot(Vec2::Y.rotate_radians(p.ship.angle));
    f.flight.stopping_distance = relative.length_squared() / (2.0 * limits.brake_acceleration);
    if let Some(target) = &mut o.local.combat.target {
        target.motion.position += target.motion.velocity * DT;
    }
    if let Some(target) = &mut o.opponent {
        target.motion.position += target.motion.velocity * DT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn following_enemy() -> (MaterialMissionPilot, MissionObservationV1) {
        let (mut bot, mut o) = super::super::tests::fixture(true);
        bot.configure_handoff_probe(true);
        o.planets.truncate(1);
        o.planets[0].motion.position = Vec2::new(1000.0, 500.0);
        o.planets[0].motion.spin = 0.0;
        let p = &mut o.local.combat.recovery.flight.pilot;
        p.planet = o.planets[0].clone();
        p.ship.position = Vec2::new(500.0, 500.0);
        p.ship.velocity = Vec2::new(-110.0, 0.0);
        p.ship.spin = 0.0;
        p.landing.assist_strength = 0.0;
        o.local.combat.target.as_mut().unwrap().motion.position = Vec2::new(900.0, 500.0);
        o.local.combat.target.as_mut().unwrap().motion.velocity = Vec2::new(-95.0, 0.0);
        o.local.combat.weapons.last_hit_taken_tick = None;
        (bot, o)
    }

    #[test]
    fn brake_and_turn_forecast_distinguishes_closing_and_separating_transfers() {
        let (mut bot, mut o) = following_enemy();
        let original = o.clone();
        let pwm = bot.pwm;
        let target = bot.telemetry.target;
        let pursuit = bot.telemetry.pursuit;
        bot.probe_disengagement_handoff(&o);
        assert_eq!(o, original);
        assert_eq!(
            (bot.pwm, bot.telemetry.target, bot.telemetry.pursuit),
            (pwm, target, pursuit)
        );
        let h = bot
            .telemetry
            .disengagement
            .as_ref()
            .unwrap()
            .handoff
            .as_ref()
            .unwrap();
        assert!(h.candidates[0].minimum_range < 250.0);
        assert!(h.candidates[0].minimum_clearance > 0.0);
        assert_eq!(h.evaluated_ticks, FORECAST_TICKS);
        o.planets[0].motion.position = Vec2::new(-500.0, 500.0);
        o.local.combat.recovery.flight.pilot.planet = o.planets[0].clone();
        o.local.combat.target.as_mut().unwrap().motion.velocity = Vec2::ZERO;
        bot.probe_disengagement_handoff(&o);
        let h = bot
            .telemetry
            .disengagement
            .as_ref()
            .unwrap()
            .handoff
            .as_ref()
            .unwrap();
        assert!(h.candidates[0].minimum_range >= 400.0);
        assert!(h.candidates[0].completed);
        assert_eq!(bot.telemetry.target, target);
        assert!(bot.capture.is_none());
    }

    #[test]
    fn probe_is_read_only_repeatable_and_configuration_survives_reset() {
        let (mut bot, o) = following_enemy();
        let mut copy = bot.clone();
        let telemetry = bot.telemetry.clone();
        bot.probe_disengagement_handoff(&o);
        copy.probe_disengagement_handoff(&o);
        assert_eq!(bot.telemetry, copy.telemetry);
        let mut after = bot.telemetry.clone();
        after.disengagement.as_mut().unwrap().handoff = None;
        assert_eq!(after, telemetry);
        let cached = bot.telemetry.clone();
        bot.probe_disengagement_handoff(&o);
        assert_eq!(bot.telemetry, cached);
        bot.reset(bot.context);
        let d = bot.telemetry.disengagement.as_ref().unwrap();
        assert!(d.handoff_probe);
        assert_eq!(d.handoff, None);
        bot.configure_handoff_probe(false);
        bot.probe_disengagement_handoff(&o);
        assert_eq!(bot.telemetry.disengagement.as_ref().unwrap().handoff, None);
    }

    #[test]
    fn candidate_work_is_capped_and_excludes_owned_or_deferred_destinations() {
        let (mut bot, mut o) = following_enemy();
        let template = o.planets[0].clone();
        o.planets = (0..10)
            .map(|index| {
                let mut planet = template.clone();
                planet.index = index;
                planet.motion.position.x += index as f32 * 1000.0;
                planet
            })
            .collect();
        o.planets[0].claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_1);
        bot.deferred
            .push((1, o.local.combat.recovery.flight.pilot.tick + 100));
        bot.probe_disengagement_handoff(&o);
        let h = bot
            .telemetry
            .disengagement
            .as_ref()
            .unwrap()
            .handoff
            .as_ref()
            .unwrap();
        assert_eq!(h.candidates.len(), MAX_DESTINATIONS);
        assert!(h.evaluated_ticks <= MAX_DESTINATIONS as u64 * FORECAST_TICKS);
        assert!(h.candidates.iter().all(|f| f.planet >= 2));
    }
}
