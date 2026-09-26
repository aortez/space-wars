//! A bounded staged reference, not a trajectory solver. No world queries or
//! rollout loops: at most eight body checks per leg and three analytic legs.
use super::*;
use scenario_spacewars::surface_sortie::{
    flight::FlightControlLimits,
    mission::{MissionBoundary, MissionObstacle},
    pilot::PilotMotion,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Body {
    index: usize,
    position: Vec2,
    velocity: Vec2,
    radius: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransferSource {
    ship: PilotMotion,
    frame: usize,
    gravity: Vec2,
    limits: FlightControlLimits,
    bodies: Vec<Body>,
    sun: Option<MissionObstacle>,
    boundary: MissionBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
pub struct TransferReference {
    pub continuing_approach: bool,
    pub settle_seconds: f32,
    pub turn_seconds: f32,
    pub climb_seconds: f32,
    pub cruise_seconds: f32,
}
impl TransferReference {
    pub fn total(self) -> f32 {
        self.settle_seconds + self.turn_seconds + self.climb_seconds + self.cruise_seconds
    }
}

impl TransferSource {
    pub(super) fn read(o: &MissionObservationV1) -> Self {
        let f = &o.local.combat.recovery.flight;
        Self {
            ship: f.pilot.ship,
            frame: f.pilot.planet.index,
            gravity: f.pilot.gravity,
            limits: f.flight.limits,
            bodies: o
                .planets
                .iter()
                .take(MAX_PLANETS)
                .map(|p| Body {
                    index: p.index,
                    position: p.motion.position,
                    velocity: p.motion.velocity,
                    radius: p.radius,
                })
                .collect(),
            sun: o.sun,
            boundary: o.boundary,
        }
    }

    /// Compare with the pinned source, never the previous tick. These are
    /// freshness tolerances, not an accuracy guarantee for the timing model.
    pub(super) fn is_current(&self, o: &MissionObservationV1) -> bool {
        let p = &o.local.combat.recovery.flight.pilot;
        self.frame == p.planet.index
            && self.ship.position.distance_to(p.ship.position) <= 2.0
            && self.ship.velocity.distance_to(p.ship.velocity) <= 2.0
            && angle_distance(self.ship.angle, p.ship.angle) <= 0.1
            && (self.ship.spin - p.ship.spin).abs() <= 0.2
            && self.gravity.distance_to(p.gravity) <= 0.5
            && self.limits == o.local.combat.recovery.flight.flight.limits
            && self.sun == o.sun
            && self.boundary == o.boundary
            && self.bodies.len() == o.planets.len()
            && self.bodies.iter().zip(&o.planets).all(|(a, b)| {
                a.index == b.index
                    && a.radius == b.radius
                    && a.position.distance_to(b.motion.position) <= 2.0
                    && a.velocity.distance_to(b.motion.velocity) <= 2.0
            })
    }

    pub(super) fn estimate(&self, destination: usize) -> Result<TransferReference, &'static str> {
        let frame = self
            .bodies
            .iter()
            .find(|b| b.index == self.frame)
            .ok_or("transfer frame unmeasured")?;
        let target = self
            .bodies
            .iter()
            .find(|b| b.index == destination)
            .ok_or("transfer destination unmeasured")?;
        let limits = self.limits;
        // Reserve the measured gravity magnitude instead of assuming full
        // thrust/brake authority in every direction. It is held constant.
        let acceleration = limits.thrust_acceleration - self.gravity.length();
        let brake = limits.brake_acceleration - self.gravity.length();
        if !(acceleration > 0.0
            && brake > 0.0
            && limits.turn_speed > 0.0
            && limits.turn_acceleration > 0.0
            && limits.cruise_speed > 10.0)
        {
            return Err("transfer control authority unmodelled");
        }
        let relative = self.ship.velocity - frame.velocity;
        if frame.index == destination
            && self.ship.position.distance_to(target.position) < target.radius + 105.0
            && relative.length() < 18.0
        {
            return Ok(TransferReference::default());
        }
        // Stage 1: settle momentum in the current frame. Stage 2: climb to the
        // ordinary controller's 70-unit transfer height, allowing for inward
        // stopping distance. Rest-to-rest stages deliberately include braking.
        let mut result = TransferReference {
            settle_seconds: relative.length() / brake,
            ..Default::default()
        };
        let up = (self.ship.position - frame.position).normalized();
        let falling = (-relative.dot(up)).max(0.0);
        let altitude = self.ship.position.distance_to(frame.position) - frame.radius;
        let climb = (70.0 + falling * falling / 50.0 - altitude).max(0.0);
        let launch = self.ship.position + up * climb;
        let mut heading = Vec2::Y.rotate_radians(self.ship.angle);
        if climb > 0.0 {
            result.turn_seconds += turn_seconds(heading, up, self.ship.spin, limits);
            result.climb_seconds = rest_to_rest(climb, acceleration.min(brake), 18.0);
            self.check_leg(self.ship.position, launch, frame.index)?;
            heading = up;
        }
        // Stage 3: transfer to the existing 85-unit entry ring, accounting for
        // a change of velocity frame. Moving-body/orbital detours are unknown.
        let entry =
            target.position + (launch - target.position).normalized() * (target.radius + 85.0);
        let delta = entry - launch;
        result.settle_seconds += (frame.velocity - target.velocity).length() / brake;
        result.turn_seconds += turn_seconds(
            heading,
            delta.normalized(),
            if climb > 0.0 { 0.0 } else { self.ship.spin },
            limits,
        );
        result.cruise_seconds = rest_to_rest(
            delta.length(),
            acceleration.min(brake),
            38.0_f32.min(limits.cruise_speed - 10.0),
        );
        self.check_leg(launch, entry, target.index)?;
        if !result.total().is_finite() || result.total() > 30.0 {
            return Err("transfer exceeds short direct reference horizon");
        }
        // Reject moving-body sweeps through the straight reference corridor.
        // This is not collision clearance: live guidance still owns safety.
        let elapsed = result.total();
        for body in &self.bodies {
            if body.index == target.index {
                continue;
            }
            let end = body.position + (body.velocity - target.velocity) * elapsed;
            if segment_distance(body.position, end, launch, entry) < body.radius + 65.0 {
                return Err("moving body requires an unmodelled transfer detour");
            }
        }
        Ok(result)
    }

    fn check_leg(&self, from: Vec2, to: Vec2, destination: usize) -> Result<(), &'static str> {
        let delta = to - from;
        for body in &self.bodies {
            if body.index == destination {
                continue;
            }
            if (body.position - from).length() < body.radius + 65.0
                && (body.position - from).dot(delta) <= 0.0
            {
                continue;
            }
            if crate::landing_safety::distance_to_segment(body.position, from, to)
                < body.radius + 65.0
            {
                return Err("transfer requires an unmodelled planet detour");
            }
        }
        if self.sun.is_some_and(|sun| {
            crate::landing_safety::distance_to_segment(sun.position, from, to) < sun.radius + 65.0
        }) {
            return Err("transfer requires an unmodelled solar detour");
        }
        if [from, to]
            .iter()
            .any(|point| point.distance_to(self.boundary.center) > self.boundary.radius - 65.0)
        {
            return Err("transfer requires unmodelled boundary guidance");
        }
        Ok(())
    }
}

fn angle_distance(a: f32, b: f32) -> f32 {
    (a - b).sin().atan2((a - b).cos()).abs()
}

fn turn_seconds(from: Vec2, to: Vec2, spin: f32, limits: FlightControlLimits) -> f32 {
    if to.length_squared() < 0.001 {
        return spin.abs() / limits.turn_acceleration;
    }
    let angle = from.dot(to).clamp(-1.0, 1.0).acos();
    rest_to_rest(angle, limits.turn_acceleration, limits.turn_speed)
        + spin.abs() / limits.turn_acceleration
}

fn segment_distance(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> f32 {
    use crate::landing_safety::distance_to_segment;
    let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    let ab = b - a;
    let cd = d - c;
    let denominator = cross(ab, cd);
    if denominator.abs() > f32::EPSILON {
        let along_ab = cross(c - a, cd) / denominator;
        let along_cd = cross(c - a, ab) / denominator;
        if (0.0..=1.0).contains(&along_ab) && (0.0..=1.0).contains(&along_cd) {
            return 0.0;
        }
    }
    distance_to_segment(a, c, d)
        .min(distance_to_segment(b, c, d))
        .min(distance_to_segment(c, a, b))
        .min(distance_to_segment(d, a, b))
}

/// Symmetric acceleration/braking, capped cruise. Analytic, constant work.
fn rest_to_rest(distance: f32, acceleration: f32, speed: f32) -> f32 {
    if distance <= speed * speed / acceleration {
        2.0 * (distance / acceleration).sqrt()
    } else {
        distance / speed + speed / acceleration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> TransferSource {
        TransferSource {
            ship: PilotMotion {
                position: Vec2::new(0.0, 90.0),
                velocity: Vec2::ZERO,
                angle: 0.0,
                spin: 0.0,
            },
            frame: 0,
            gravity: Vec2::new(0.0, -10.0),
            limits: FlightControlLimits::for_sweep(0.0),
            bodies: vec![
                Body {
                    index: 0,
                    position: Vec2::ZERO,
                    velocity: Vec2::ZERO,
                    radius: 50.0,
                },
                Body {
                    index: 1,
                    position: Vec2::new(350.0, 200.0),
                    velocity: Vec2::ZERO,
                    radius: 50.0,
                },
            ],
            sun: None,
            boundary: MissionBoundary {
                center: Vec2::ZERO,
                radius: 2000.0,
            },
        }
    }

    #[test]
    fn launch_costs_include_climb_turn_and_momentum_and_do_not_apply_to_arrival() {
        let s = source();
        let baseline = s.estimate(1).unwrap();
        assert!(baseline.climb_seconds > 0.0 && baseline.turn_seconds > 0.0);
        assert!(
            baseline.total() > (s.ship.position.distance_to(s.bodies[1].position) - 135.0) / 38.0
        );
        let mut falling = s.clone();
        falling.ship.velocity = -Vec2::Y * 20.0;
        let slowed = falling.estimate(1).unwrap();
        assert!(slowed.settle_seconds > baseline.settle_seconds);
        assert!(slowed.climb_seconds > baseline.climb_seconds);
        assert!(slowed.total() > baseline.total());
        assert_eq!(s.estimate(0).unwrap().total(), 0.0);
        let mut turned = s.clone();
        turned.ship.angle = std::f32::consts::PI;
        assert!(turned.estimate(1).unwrap().turn_seconds > baseline.turn_seconds);
    }

    #[test]
    fn unsupported_authority_obstructions_boundaries_and_horizons_stay_unknown() {
        for mutation in 0..5 {
            let mut s = source();
            match mutation {
                0 => s.gravity = Vec2::Y * 50.0,
                1 => {
                    s.sun = Some(MissionObstacle {
                        position: Vec2::new(120.0, 160.0),
                        radius: 20.0,
                    })
                }
                2 => s.boundary.radius = 200.0,
                3 => s.bodies[1].position = Vec2::new(1500.0, 200.0),
                4 => s.bodies.push(Body {
                    index: 2,
                    position: Vec2::new(120.0, 160.0),
                    velocity: Vec2::ZERO,
                    radius: 20.0,
                }),
                _ => unreachable!(),
            }
            assert!(s.estimate(1).is_err(), "mutation {mutation}");
        }
    }

    #[test]
    fn analytic_stages_include_acceleration_and_braking_on_short_and_long_legs() {
        assert_eq!(rest_to_rest(0.0, 10.0, 20.0), 0.0);
        assert_eq!(rest_to_rest(10.0, 10.0, 20.0), 2.0);
        assert_eq!(rest_to_rest(40.0, 10.0, 20.0), 4.0);
        assert_eq!(rest_to_rest(80.0, 10.0, 20.0), 6.0);
    }

    #[test]
    fn a_body_crossing_between_clear_endpoints_requires_a_detour() {
        let mut s = source();
        assert!(s.estimate(1).is_ok());
        s.bodies.push(Body {
            index: 2,
            position: Vec2::new(120.0, 400.0),
            velocity: Vec2::new(0.0, -50.0),
            radius: 20.0,
        });
        assert_eq!(
            s.estimate(1).unwrap_err(),
            "moving body requires an unmodelled transfer detour"
        );
        // Also cover an obstacle that starts behind the launch and crosses it.
        s.bodies.last_mut().unwrap().position = Vec2::new(-200.0, 140.0);
        s.bodies.last_mut().unwrap().velocity = Vec2::new(70.0, 0.0);
        assert_eq!(
            s.estimate(1).unwrap_err(),
            "moving body requires an unmodelled transfer detour"
        );
    }
}
