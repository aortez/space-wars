//! Conditional opponent responses to one predicted own-flight trajectory.
//! These are hypotheses, not bounds on all possible enemy motion. The current
//! observation lacks enemy gravity, wing state and private controller memory.
use super::*;
use crate::{RuleShipBrainConfig, combat_solution};

const RANGE_THRESHOLD: f32 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpponentResponse {
    Coast,
    BrakeToAim,
    Pursue,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpponentForecast {
    pub response: OpponentResponse,
    pub minimum_range: f32,
    pub minimum_nominal_clearance: f32,
    /// Geometric range entry only: no aim, line of sight or weapon readiness.
    pub first_inside_tick: Option<u64>,
    pub boundary: BoundaryForecast,
    pub samples: Vec<OpponentSample>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OpponentSample {
    pub after_ticks: u64,
    pub motion: PilotMotion,
    pub range: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RangeEntryEnvelope {
    pub threshold: f32,
    pub entering_responses: usize,
    pub earliest_tick: Option<u64>,
    /// None if at least one response never enters during the evaluated horizon.
    pub latest_tick: Option<u64>,
}

impl RangeEntryEnvelope {
    pub(super) fn from_forecasts(forecasts: &[OpponentForecast; 3]) -> Self {
        let entries: Vec<_> = forecasts
            .iter()
            .filter_map(|f| f.first_inside_tick)
            .collect();
        Self {
            threshold: RANGE_THRESHOLD,
            entering_responses: entries.len(),
            earliest_tick: entries.iter().copied().min(),
            latest_tick: (entries.len() == forecasts.len())
                .then(|| entries.iter().copied().max().unwrap()),
        }
    }
}

#[derive(Clone)]
pub(super) struct ResponseState {
    initial: PilotMotion,
    motion: PilotMotion,
    sweep: f32,
    pwm: f32,
    forecast: OpponentForecast,
}

pub(super) struct OpponentResponses([ResponseState; 3]);

impl OpponentResponses {
    pub(super) fn new(enemy: PilotMotion, o: &MissionObservationV1) -> Self {
        Self(
            [
                OpponentResponse::Coast,
                OpponentResponse::BrakeToAim,
                OpponentResponse::Pursue,
            ]
            .map(|response| ResponseState::new(response, enemy, o)),
        )
    }

    pub(super) fn forecasts(&self) -> [OpponentForecast; 3] {
        std::array::from_fn(|i| self.0[i].forecast.clone())
    }

    pub(super) fn advance(&mut self, o: &MissionObservationV1, tick: u64) {
        for response in &mut self.0 {
            response.advance(o, tick);
        }
    }

    pub(super) fn record(&mut self, o: &MissionObservationV1, tick: u64) {
        for response in &mut self.0 {
            response.record(o, tick);
        }
    }
}

impl ResponseState {
    pub(super) fn new(
        response: OpponentResponse,
        enemy: PilotMotion,
        o: &MissionObservationV1,
    ) -> Self {
        let mut state = Self {
            initial: enemy,
            motion: enemy,
            // Open wings and zero enemy gravity are hypotheses, not sensed state.
            sweep: 0.0,
            pwm: 0.0,
            forecast: OpponentForecast {
                response,
                minimum_range: f32::MAX,
                minimum_nominal_clearance: f32::MAX,
                first_inside_tick: None,
                boundary: BoundaryForecast::new(o.boundary, enemy.position),
                samples: Vec::new(),
            },
        };
        state.record(o, 0);
        state
    }

    pub(super) fn motion(&self) -> PilotMotion {
        self.motion
    }

    pub(super) fn forecast(&self) -> &OpponentForecast {
        &self.forecast
    }

    pub(super) fn advance(&mut self, o: &MissionObservationV1, tick: u64) {
        if self.forecast.response == OpponentResponse::Coast {
            self.motion.position =
                self.initial.position + self.initial.velocity * (tick as f32 * DT);
            self.motion.angle = self.initial.angle + self.initial.spin * (tick as f32 * DT);
            return;
        }
        let frame_velocity = o
            .planets
            .iter()
            .min_by(|a, b| {
                (self.motion.position.distance_to(a.motion.position) - a.radius)
                    .total_cmp(&(self.motion.position.distance_to(b.motion.position) - b.radius))
            })
            .map_or(Vec2::ZERO, |planet| {
                planet.velocity_at(self.motion.position)
            });
        let intent = self.intent(o.local.combat.recovery.flight.pilot.ship, frame_velocity);
        advance_motor(
            &mut self.motion,
            &mut self.sweep,
            intent,
            frame_velocity,
            Vec2::ZERO,
        );
    }

    pub(super) fn record(&mut self, o: &MissionObservationV1, tick: u64) {
        let own = o.local.combat.recovery.flight.pilot.ship.position;
        let f = &mut self.forecast;
        let range = own.distance_to(self.motion.position);
        f.minimum_range = f.minimum_range.min(range);
        f.boundary.record(o.boundary, self.motion.position, tick);
        if range <= RANGE_THRESHOLD {
            f.first_inside_tick.get_or_insert(tick);
        }
        for (center, radius) in o
            .planets
            .iter()
            .map(|p| (p.motion.position, p.radius))
            .chain(o.sun.map(|sun| (sun.position, sun.radius)))
        {
            f.minimum_nominal_clearance = f
                .minimum_nominal_clearance
                .min(self.motion.position.distance_to(center) - radius - 65.0);
        }
        if tick > 0 && tick % 60 == 0 {
            f.samples.push(OpponentSample {
                after_ticks: tick,
                motion: self.motion,
                range,
            });
        }
    }
    fn intent(&mut self, own: PilotMotion, frame_velocity: Vec2) -> FlightIntent {
        let limits = FlightControlLimits::for_sweep(self.sweep);
        let delta = own.position - self.motion.position;
        let relative = self.motion.velocity - frame_velocity;
        let (direction, brake, thrust) = if self.forecast.response == OpponentResponse::BrakeToAim
            || delta.length() < 250.0
        {
            let solution = combat_solution(
                delta.rotate_radians(-self.motion.angle),
                (own.velocity - self.motion.velocity).rotate_radians(-self.motion.angle),
                self.motion.spin,
                false,
                false,
                &RuleShipBrainConfig {
                    arrival_distance: 55.0,
                    ..Default::default()
                },
            );
            let closing = -(own.velocity - self.motion.velocity).dot(delta.normalized());
            let brake = self.forecast.response == OpponentResponse::BrakeToAim
                || relative.length() > 30.0
                || delta.length() < 70.0
                || closing > 35.0;
            (
                solution.aim.rotate_radians(self.motion.angle).normalized(),
                brake,
                solution.intent.thrust > 0.0 && !brake,
            )
        } else {
            // The mission's free-flight pursuit speed and ordinary velocity
            // guidance, without its hidden route/climb/break state or queries.
            let entry = own.position + own.velocity;
            let offset = entry - self.motion.position;
            let desired = own.velocity + offset.normalized() * (offset.length() * 0.7).min(55.0);
            let brake =
                desired.length() < 10.0 || self.motion.velocity.length() > desired.length() + 4.0;
            let acceleration = (desired - self.motion.velocity) * 2.0
                - if brake {
                    limits.braking(relative)
                } else {
                    Vec2::ZERO
                };
            let direction = acceleration.normalized();
            let forward = Vec2::Y.rotate_radians(self.motion.angle);
            let available =
                limits.thrust_acceleration * limits.thrust_fraction(relative.dot(forward));
            let duty = if forward.dot(direction) > 0.97 && available > 0.01 {
                (acceleration.length() / available).clamp(0.0, 1.0)
            } else {
                0.0
            };
            self.pwm = (self.pwm + duty).min(2.0);
            let thrust = self.pwm >= 1.0;
            if thrust {
                self.pwm -= 1.0;
            }
            (direction, brake, thrust)
        };
        let error = shortest_heading_error(direction.rotate_radians(-self.motion.angle));
        let desired_spin = -error * 3.0;
        let horizontal = (-(desired_spin - (self.motion.spin - desired_spin) * 0.2)
            / limits.turn_speed)
            .clamp(-1.0, 1.0);
        FlightIntent {
            controls: SurfaceSortieAction {
                horizontal,
                primary_held: thrust,
                brake_held: brake,
                interact_held: false,
            },
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_preserve_inertia_and_brake_with_finite_turn_and_acceleration() {
        let (_, mut o) = super::super::super::tests::fixture(true);
        o.planets.clear();
        o.sun = None;
        o.local.combat.recovery.flight.pilot.ship.position = Vec2::new(500.0, 0.0);
        let enemy = PilotMotion {
            position: Vec2::ZERO,
            velocity: Vec2::new(80.0, 0.0),
            angle: 0.0,
            spin: 0.0,
        };
        let mut responses = OpponentResponses::new(enemy, &o);
        responses.advance(&o, 1);
        assert!(responses.0[1].motion.velocity.length() > 79.0);
        assert!(responses.0[1].motion.angle.abs() < 0.01);
        assert!(responses.0[2].motion.angle.abs() < 0.01);
        for tick in 2..=FORECAST_TICKS {
            responses.advance(&o, tick);
            responses.record(&o, tick);
        }
        let forecasts = responses.forecasts();
        assert!(
            responses.0[0]
                .motion
                .position
                .distance_to(Vec2::new(480.0, 0.0))
                < 0.001
        );
        assert!(responses.0[1].motion.velocity.length() < 0.01);
        assert!(responses.0[1].motion.position.x < 90.0);
        assert!(forecasts[0].first_inside_tick.is_some());
        assert!(forecasts[1].first_inside_tick.is_none());
        let range = RangeEntryEnvelope::from_forecasts(&forecasts);
        assert!(range.earliest_tick.is_some());
        assert!(range.latest_tick.is_none());
    }

    #[test]
    fn pursuit_can_close_when_constant_velocity_never_reaches_range() {
        let (_, mut o) = super::super::super::tests::fixture(true);
        o.planets.clear();
        o.sun = None;
        let own = &mut o.local.combat.recovery.flight.pilot.ship;
        own.position = Vec2::new(420.0, 0.0);
        own.velocity = Vec2::ZERO;
        let enemy = PilotMotion {
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            angle: -std::f32::consts::FRAC_PI_2,
            spin: 0.0,
        };
        let mut responses = OpponentResponses::new(enemy, &o);
        for tick in 1..=FORECAST_TICKS {
            responses.advance(&o, tick);
            responses.record(&o, tick);
        }
        let forecasts = responses.forecasts();
        assert_eq!(forecasts[0].minimum_range, 420.0);
        assert_eq!(forecasts[1].minimum_range, 420.0);
        assert!(forecasts[2].minimum_range < RANGE_THRESHOLD);
        let range = RangeEntryEnvelope::from_forecasts(&forecasts);
        assert_eq!(range.entering_responses, 1);
        assert!(range.earliest_tick.unwrap() > 60);
        assert_eq!(range.latest_tick, None);
    }
}
