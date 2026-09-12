//! Running jumps with bounded two-link lookahead. Unlike the careful planner,
//! these candidates preserve horizontal speed through flight and touchdown.
//! Plans are proposals: the controller revalidates the actual takeoff, then
//! replans from actual support after landing. No physics rollout or global search.

use engine_core::Vec2;

use super::DT;
use super::planner::{Capabilities, Course, Plan};

#[derive(Clone, Copy)]
pub(super) struct Start {
    pub surface: usize,
    pub x: f32,
    pub velocity: f32,
    pub direction: f32,
}

pub(super) fn plan(course: &Course, start: Start, radius: f32, caps: Capabilities) -> Option<Plan> {
    let mut best = None;
    let mut best_cost = f32::INFINITY;
    for mut candidate in candidates(course, start, radius, caps)
        .into_iter()
        .flatten()
    {
        let arrival = Start {
            surface: candidate.target,
            // Allow a contact-confirmation tick before the next decision.
            x: candidate.landing.x + start.direction * candidate.cruise * DT * 2.0,
            velocity: start.direction * candidate.cruise,
            direction: start.direction,
        };
        let next = adjacent(course, arrival.surface, start.direction);
        let next_cost = if let Some(next) = next {
            let onward = candidates(course, arrival, radius, caps)
                .into_iter()
                .flatten()
                .map(|plan| cost(plan, arrival, radius))
                .reduce(f32::min);
            // No running continuation: the safe braking reserve still permits
            // the careful fallback. Prefer a genuine chain when available.
            if onward.is_some() {
                candidate.next_target = Some(next);
            }
            onward.unwrap_or(2.5)
        } else {
            0.0
        };
        let cost = cost(candidate, start, radius) + next_cost;
        if cost < best_cost {
            best = Some(candidate);
            best_cost = cost;
        }
    }
    // Compare equal progress: one direct jump versus the two adjacent links
    // above. Require a useful time saving, not merely another reachable arc.
    let skip_target = if start.direction > 0.0 {
        start.surface.checked_add(2)
    } else {
        start.surface.checked_sub(2)
    }
    .filter(|target| *target < course.surfaces.len());
    if let Some(target) = skip_target {
        let mut shortcut = None;
        let mut shortcut_cost = best_cost * 0.95;
        for candidate in candidates_to(course, start, target, radius, caps)
            .into_iter()
            .flatten()
        {
            let cost = cost(candidate, start, radius);
            if cost < shortcut_cost {
                shortcut = Some(candidate);
                shortcut_cost = cost;
            }
        }
        if let Some(mut candidate) = shortcut {
            let arrival = Start {
                surface: candidate.target,
                x: candidate.landing.x + start.direction * candidate.cruise * DT * 2.0,
                velocity: start.direction * candidate.cruise,
                direction: start.direction,
            };
            if candidates(course, arrival, radius, caps)
                .iter()
                .any(Option::is_some)
            {
                candidate.next_target = adjacent(course, arrival.surface, start.direction);
            }
            best = Some(candidate);
        }
    }
    best
}

fn cost(plan: Plan, start: Start, radius: f32) -> f32 {
    let run_up = (plan.takeoff.x - start.x).abs();
    run_up / ((start.velocity.abs() + plan.cruise) * 0.5).max(radius) + plan.flight
}

#[cfg(test)]
pub(super) fn single_jump_plan(
    course: &Course,
    start: Start,
    radius: f32,
    caps: Capabilities,
) -> Option<Plan> {
    candidates(course, start, radius, caps)
        .into_iter()
        .flatten()
        .min_by(|a, b| cost(*a, start, radius).total_cmp(&cost(*b, start, radius)))
}

fn adjacent(course: &Course, source: usize, direction: f32) -> Option<usize> {
    if direction > 0.0 {
        (source + 1 < course.surfaces.len()).then_some(source + 1)
    } else {
        source.checked_sub(1)
    }
}

// Three takeoff positions by three landing positions. Stack-only storage and
// one further link plus one shortcut bound work to 9 + 9*9 + 9 + 9 arcs.
fn candidates(course: &Course, start: Start, radius: f32, caps: Capabilities) -> [Option<Plan>; 9] {
    let Some(target) = adjacent(course, start.surface, start.direction) else {
        return [None; 9];
    };
    candidates_to(course, start, target, radius, caps)
}

fn candidates_to(
    course: &Course,
    start: Start,
    target: usize,
    radius: f32,
    caps: Capabilities,
) -> [Option<Plan>; 9] {
    let mut result = [None; 9];
    let from = course.surfaces[start.surface];
    let to = course.surfaces[target];
    let Some((left, right)) = from.inside(radius) else {
        return result;
    };
    let Some((land_left, land_right)) = to.inside(radius) else {
        return result;
    };
    // The controller can launch up to one cruise tick early. Keep edge
    // candidates that far inside the interval so re-anchoring remains safe.
    let landing_margin = caps.speed * DT;
    let near = if start.direction > 0.0 {
        land_left + landing_margin
    } else {
        land_right - landing_margin
    };
    let far = if start.direction > 0.0 { right } else { left };
    let middle = (land_left + land_right) * 0.5;
    for (i, fraction) in [0.1, 0.25, 0.4].into_iter().enumerate() {
        let takeoff = far - start.direction * (right - left) * fraction;
        for (j, landing) in [middle, (near + middle) * 0.5, near]
            .into_iter()
            .enumerate()
        {
            let Some(plan) = trajectory(
                course,
                start.surface,
                target,
                takeoff,
                landing,
                radius,
                caps,
            ) else {
                continue;
            };
            let run_up = (takeoff - start.x) * start.direction;
            let current_speed = start.velocity * start.direction;
            if current_speed < -radius * 0.1 {
                continue;
            }
            let speed_change_distance = (plan.cruise.powi(2) - current_speed.powi(2)).abs()
                / (2.0 * caps.acceleration * 0.65);
            if run_up >= speed_change_distance + radius * 0.35
                && (far - takeoff) * start.direction >= takeoff_reserve(plan, radius, caps)
            {
                result[i * 3 + j] = Some(plan);
            }
        }
    }
    result
}

fn trajectory(
    course: &Course,
    source: usize,
    target: usize,
    takeoff: f32,
    landing: f32,
    radius: f32,
    caps: Capabilities,
) -> Option<Plan> {
    let from = course.surfaces.get(source)?;
    let to = course.surfaces.get(target)?;
    let rise = to.height - from.height;
    if rise >= caps.height * 0.85 {
        return None;
    }
    let gravity = 8.0 * caps.height / caps.flight.powi(2);
    let launch = 4.0 * caps.height / caps.flight;
    let flight = (launch + (launch * launch - 2.0 * gravity * rise).sqrt()) / gravity;
    if !flight.is_finite() || flight <= 0.0 || flight > 2.0 {
        return None;
    }
    let cruise = (landing - takeoff).abs() / flight;
    if cruise < caps.speed * 0.15 || cruise > caps.speed * 0.9 {
        return None;
    }
    let result = Plan {
        source,
        target,
        takeoff: Vec2::new(takeoff, from.height),
        landing: Vec2::new(landing, to.height),
        flight,
        cruise,
        running: true,
        next_target: None,
    };
    safe(course, result, radius, caps).then_some(result)
}

/// A takeoff is triggered by actual position, not by teleporting to the planned
/// point. Re-anchor the constant-speed arc and recheck its entire landing reserve.
pub(super) fn at_takeoff(
    course: &Course,
    mut plan: Plan,
    x: f32,
    radius: f32,
    caps: Capabilities,
) -> Option<Plan> {
    let shift = x - plan.takeoff.x;
    plan.takeoff.x = x;
    plan.landing.x += shift;
    let (left, right) = course.surfaces[plan.source].inside(radius)?;
    let reserve = if plan.landing.x > x {
        right - x
    } else {
        x - left
    };
    if reserve < takeoff_reserve(plan, radius, caps) {
        return None;
    }
    safe(course, plan, radius, caps).then_some(plan)
}

fn takeoff_reserve(plan: Plan, radius: f32, caps: Capabilities) -> f32 {
    // A rejected takeoff can brake immediately; a landing first has to be
    // detected and confirmed. Keep the larger three-tick reserve for landing.
    plan.cruise.powi(2) / (2.0 * caps.acceleration * 0.85) + plan.cruise * DT + radius * 0.25
}

fn braking_reserve(plan: Plan, radius: f32, caps: Capabilities) -> f32 {
    plan.cruise.powi(2) / (2.0 * caps.acceleration * 0.65) + plan.cruise * DT * 3.0 + radius * 0.5
}

fn safe(course: &Course, plan: Plan, radius: f32, caps: Capabilities) -> bool {
    let Some((left, right)) = course.surfaces[plan.target].inside(radius) else {
        return false;
    };
    let direction = (plan.landing.x - plan.takeoff.x).signum();
    let reserve = if direction > 0.0 {
        right - plan.landing.x
    } else {
        plan.landing.x - left
    };
    // Enough room to confirm contact and brake even if the next running jump
    // is invalidated. This constraint must also hold for the actual takeoff.
    if plan.landing.x < left
        || plan.landing.x > right
        || reserve < braking_reserve(plan, radius, caps)
    {
        return false;
    }
    let steps = (plan.flight / DT).ceil() as u32;
    (1..=steps).all(|step| {
        let point = plan.sample(caps, (step as f32 * DT / plan.flight).min(1.0));
        course.surfaces.iter().enumerate().all(|(index, surface)| {
            let overlaps =
                point.x + radius * 1.2 > surface.start && point.x - radius * 1.2 < surface.end;
            !overlaps
                || point.y >= surface.height + radius * 0.15
                || (index == plan.target
                    && point.x >= left - radius * 0.02
                    && point.x <= right + radius * 0.02
                    && point.y >= surface.height - radius * 0.02)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_an_intermediate_platform_only_with_a_clear_useful_landing() {
        let radius = 8.0;
        let caps = Capabilities {
            height: 35.6,
            flight: 52.0 / 60.0,
            speed: 160.0,
            acceleration: 960.0,
        };
        let mut course = Course::authored(
            800.0,
            radius,
            engine_common::ClockDuckCoursePattern::Shortcut,
        );
        let start = Start {
            surface: 0,
            x: 120.0,
            velocity: 160.0,
            direction: 1.0,
        };
        assert!(
            course.valid_routes(radius, caps),
            "Careful retains the adjacent route"
        );
        let jump = plan(&course, start, radius, caps).unwrap();
        assert_eq!((jump.source, jump.target), (0, 2));
        let adjacent_cost = candidates(&course, start, radius, caps)
            .into_iter()
            .flatten()
            .map(|first| {
                let arrival = Start {
                    surface: 1,
                    x: first.landing.x + first.cruise * DT * 2.0,
                    velocity: first.cruise,
                    direction: 1.0,
                };
                cost(first, start, radius)
                    + candidates(&course, arrival, radius, caps)
                        .into_iter()
                        .flatten()
                        .map(|second| cost(second, arrival, radius))
                        .reduce(f32::min)
                        .unwrap_or(2.5)
            })
            .reduce(f32::min)
            .unwrap();
        assert!(cost(jump, start, radius) < adjacent_cost * 0.95);
        course.surfaces[1].height = caps.height * 2.0;
        assert!(
            candidates_to(&course, start, 2, radius, caps)
                .iter()
                .all(Option::is_none)
        );
        assert!(
            plan(&course, start, radius, caps).is_none(),
            "do not jump through a blocked shortcut"
        );
        course.surfaces[1].height = radius * 0.45;
        course.surfaces[2].end = course.surfaces[2].start + radius * 2.0;
        assert!(
            plan(&course, start, radius, caps).is_none_or(|jump| jump.target != 2),
            "reject an unsafe landing even if the gap is reachable"
        );
    }

    #[test]
    fn running_arcs_keep_speed_and_revalidate_actual_takeoff_in_both_directions() {
        let radius = 8.0;
        let caps = Capabilities {
            height: 35.6,
            flight: 52.0 / 60.0,
            speed: 160.0,
            acceleration: 960.0,
        };
        let course = Course::fixed(800.0, radius, 0);
        for start in [
            Start {
                surface: 0,
                x: 80.0,
                velocity: 160.0,
                direction: 1.0,
            },
            Start {
                surface: 2,
                x: 720.0,
                velocity: -160.0,
                direction: -1.0,
            },
        ] {
            let jump =
                plan(&course, start, radius, caps).expect("wide fixture has a running route");
            let end = jump.sample(caps, 1.0);
            assert!((end - jump.landing).length() < 0.001);
            let first = jump.sample(caps, 0.25);
            let second = jump.sample(caps, 0.75);
            assert!(
                ((second.x - first.x) / (jump.flight * 0.5) - start.direction * jump.cruise).abs()
                    < 0.001
            );
            assert!(at_takeoff(&course, jump, jump.takeoff.x, radius, caps).is_some());
            // Starting early/late is not permission to land outside the surface.
            for offset in [-800.0, 800.0] {
                assert!(at_takeoff(&course, jump, jump.takeoff.x + offset, radius, caps).is_none());
            }
        }
    }
}
