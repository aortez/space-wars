//! The circles a blast meets a body as, made from the body's own colliders.
use super::{Reachable, Recipient};
use crate::world::{BodyId, PhysicsWorld, from_rapier, to_rapier};
use engine_core::Vec2;
use rapier2d::parry::shape::Shape;
use rapier2d::prelude::Pose;

/// The most circles that stand for one collider. A longer, thinner collider
/// leaves gaps between them rather than costing a blast without bound.
const MAX_ROW: usize = 16;

/// The circles that fit within `body`'s solid colliders where the body is
/// now, each standing for `recipient` and as thick as `thickness`, for a host
/// to list for the body in a blast. A ball is itself, and a capsule is a row
/// of balls along its segment, close enough to touch. A cuboid or a convex
/// polygon is a row of the largest circles that fit along the axis its area
/// spreads farthest on, through its centroid: as many as that chord is long
/// in diameters of the circle at the centroid, from one to `MAX_ROW`, each at
/// the middle of its equal stretch of the chord. A compound is its parts.
/// Other shapes, sensors and disabled colliders stand for nothing, and a body
/// the world lacks stands for nothing at all.
///
/// The circles lie within the body, as Scorched Earth's tank circles do, so a
/// blast that bursts at the body's surface starts just outside them rather
/// than some way off. They leave out a polygon's corners, so they suit a
/// blast better than a beam, whose stop takes only what its circles cover.
pub fn body_circles(
    world: &PhysicsWorld,
    body: BodyId,
    recipient: Recipient,
    thickness: f32,
) -> Vec<Reachable> {
    let Some(rigid) = world
        .body_handle(body)
        .and_then(|handle| world.raw.bodies.get(handle))
    else {
        return Vec::new();
    };
    let mut circles = Vec::new();
    for collider in rigid
        .colliders()
        .iter()
        .filter_map(|handle| world.raw.colliders.get(*handle))
    {
        if collider.is_enabled() && !collider.is_sensor() {
            shape_circles(collider.shape(), collider.position(), &mut circles);
        }
    }
    circles
        .into_iter()
        .map(|(center, radius)| Reachable {
            recipient,
            center,
            radius,
            thickness,
        })
        .collect()
}

/// Adds the circles within `shape`, posed at `pose`, in the world's frame.
fn shape_circles(shape: &dyn Shape, pose: &Pose, circles: &mut Vec<(Vec2, f32)>) {
    let place = |(center, radius): (Vec2, f32)| (from_rapier(pose * to_rapier(center)), radius);
    if let Some(compound) = shape.as_compound() {
        for (local, part) in compound.shapes() {
            shape_circles(part.as_ref(), &(pose * local), circles);
        }
    } else if let Some(ball) = shape.as_ball() {
        circles.push(place((Vec2::ZERO, ball.radius)));
    } else if let Some(capsule) = shape.as_capsule() {
        let ends = [capsule.segment.a, capsule.segment.b].map(from_rapier);
        circles.extend(capsule_circles(ends, capsule.radius).into_iter().map(place));
    } else if let Some(cuboid) = shape.as_cuboid() {
        let half = from_rapier(cuboid.half_extents);
        let corners = [
            Vec2::new(-half.x, -half.y),
            Vec2::new(half.x, -half.y),
            half,
            Vec2::new(-half.x, half.y),
        ];
        circles.extend(polygon_circles(&corners).into_iter().map(place));
    } else if let Some(polygon) = shape.as_convex_polygon() {
        let vertices: Vec<Vec2> = polygon.points().iter().copied().map(from_rapier).collect();
        circles.extend(polygon_circles(&vertices).into_iter().map(place));
    }
}

/// Balls of `radius` along the segment between `ends`: one at each end, and
/// as few between as it takes for neighbors to touch, up to `MAX_ROW` in all.
/// A radius that is not positive and finite has none.
fn capsule_circles([a, b]: [Vec2; 2], radius: f32) -> Vec<(Vec2, f32)> {
    if !(radius > 0.0 && radius.is_finite()) {
        return Vec::new();
    }
    let gaps = ((a.distance_to(b) / (2.0 * radius)).ceil() as usize).min(MAX_ROW - 1);
    (0..=gaps)
        .map(|step| {
            let along = if gaps == 0 {
                0.0
            } else {
                step as f32 / gaps as f32
            };
            (a + (b - a) * along, radius)
        })
        .collect()
}

/// A row of the largest circles that fit within the convex polygon
/// `vertices`, wound either way, along the axis through its centroid on which
/// its area spreads farthest. A polygon without area has none.
fn polygon_circles(vertices: &[Vec2]) -> Vec<(Vec2, f32)> {
    let Some(&origin) = vertices.first() else {
        return Vec::new();
    };
    let edges = || {
        let count = vertices.len();
        (0..count).map(move |at| (vertices[at], vertices[(at + 1) % count]))
    };
    // Twice the area, signed by the winding, and the centroid, measured from
    // the first vertex so that a polygon far out keeps its precision.
    let mut doubled = 0.0;
    let mut moment = Vec2::ZERO;
    for (a, b) in edges() {
        let (a, b) = (a - origin, b - origin);
        let cross = a.x * b.y - b.x * a.y;
        doubled += cross;
        moment += (a + b) * cross;
    }
    if !(doubled != 0.0 && doubled.is_finite()) {
        return Vec::new();
    }
    let winding = doubled.signum();
    let centroid = origin + moment / (3.0 * doubled);
    // The area's second moments about the centroid, as sums whose ratios
    // give its principal axis.
    let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
    for (a, b) in edges() {
        let (a, b) = (a - centroid, b - centroid);
        let cross = a.x * b.y - b.x * a.y;
        xx += (a.x * a.x + a.x * b.x + b.x * b.x) * cross;
        yy += (a.y * a.y + a.y * b.y + b.y * b.y) * cross;
        xy += (a.x * b.y + 2.0 * a.x * a.y + 2.0 * b.x * b.y + b.x * a.y) * cross;
    }
    let axis = Vec2::from_radians(0.5 * (winding * xy).atan2(winding * (xx - yy)));
    // Each edge's inward normal, and how deep the centroid lies behind it.
    let sides: Vec<(Vec2, f32)> = edges()
        .filter_map(|(a, b)| {
            let along = b - a;
            let length = along.length();
            (length > 0.0).then(|| {
                let normal = Vec2::new(-along.y, along.x) * (winding / length);
                (normal, normal.dot(centroid - a))
            })
        })
        .collect();
    // How far the point `t` along the axis from the centroid lies inside every
    // edge, which is the radius of the largest circle that fits there.
    let depth = |t: f32| {
        sides
            .iter()
            .map(|(normal, deep)| deep + t * normal.dot(axis))
            .fold(f32::INFINITY, f32::min)
    };
    let middle = depth(0.0);
    if !(middle > 0.0 && middle.is_finite()) {
        return Vec::new();
    }
    // The chord along the axis, between the edges it crosses.
    let (mut start, mut end) = (f32::NEG_INFINITY, f32::INFINITY);
    for (normal, deep) in &sides {
        let rate = normal.dot(axis);
        if rate > 0.0 {
            start = start.max(-deep / rate);
        } else if rate < 0.0 {
            end = end.min(deep / -rate);
        }
    }
    let count = ((end - start) / (2.0 * middle))
        .round()
        .clamp(1.0, MAX_ROW as f32) as usize;
    let stretch = (end - start) / count as f32;
    (0..count)
        .filter_map(|at| {
            let t = start + (at as f32 + 0.5) * stretch;
            let radius = depth(t);
            (radius > 0.0 && radius.is_finite()).then(|| (centroid + axis * t, radius))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{
        BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
        PhysicsWorldConfig,
    };

    const RECIPIENT: Recipient = Recipient::Body {
        key: 7,
        entity: PhysicsId::new(7),
    };

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0e-4
    }

    /// A world with one fixed body of entity 7 at `position`, turned by
    /// `angle`, made of `colliders`.
    fn body(position: Vec2, angle: f32, colliders: &[ColliderSpec]) -> (PhysicsWorld, BodyId) {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let body = BodyId::new(PhysicsId::new(7), BodyRole::PRIMARY);
        assert!(world.insert_body(
            body,
            BodySpec {
                kind: BodyKind::Fixed,
                position,
                angle,
                ..Default::default()
            },
            colliders,
        ));
        (world, body)
    }

    fn collider(part: u16) -> ColliderId {
        ColliderId::new(PhysicsId::new(7), ColliderRole::PRIMARY, part)
    }

    /// Whether the circle lies within the convex polygon `vertices`, wound
    /// counterclockwise.
    fn within(vertices: &[Vec2], (center, radius): (Vec2, f32)) -> bool {
        (0..vertices.len()).all(|at| {
            let (a, b) = (vertices[at], vertices[(at + 1) % vertices.len()]);
            let along = b - a;
            let inward = Vec2::new(-along.y, along.x) / along.length();
            inward.dot(center - a) >= radius - 1.0e-4
        })
    }

    #[test]
    fn a_ball_is_itself() {
        let (world, body) = body(
            Vec2::new(3.0, 4.0),
            0.0,
            &[ColliderSpec::ball(collider(0), 1.5)],
        );
        let circles = body_circles(&world, body, RECIPIENT, 2.0);
        assert_eq!(
            circles,
            [Reachable {
                recipient: RECIPIENT,
                center: Vec2::new(3.0, 4.0),
                radius: 1.5,
                thickness: 2.0,
            }]
        );
    }

    #[test]
    fn a_cuboid_is_a_row_of_circles_along_its_length_within_it() {
        // A tank's chassis, 4.4 long and 1.2 tall, stood on its end.
        let (world, body) = body(
            Vec2::new(10.0, 0.0),
            std::f32::consts::FRAC_PI_2,
            &[ColliderSpec::cuboid(collider(0), 2.2, 0.6)],
        );
        let circles = body_circles(&world, body, RECIPIENT, 2.0);
        let mut row: Vec<(f32, f32, f32)> = circles
            .iter()
            .map(|circle| (circle.center.y, circle.center.x, circle.radius))
            .collect();
        row.sort_by(|a, b| a.0.total_cmp(&b.0));
        let expected = [(-1.65, 0.55), (-0.55, 0.6), (0.55, 0.6), (1.65, 0.55)];
        assert_eq!(row.len(), expected.len(), "{row:?}");
        for ((y, x, radius), (want_y, want_radius)) in row.into_iter().zip(expected) {
            assert!(close(x, 10.0) && close(y, want_y), "{y} {x}");
            assert!(close(radius, want_radius), "{radius}");
        }
    }

    #[test]
    fn a_square_is_its_inscribed_circle() {
        let square = [
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(0.0, 2.0),
        ];
        let circles = polygon_circles(&square);
        assert_eq!(circles.len(), 1);
        let (center, radius) = circles[0];
        assert!(close(center.x, 1.0) && close(center.y, 1.0) && close(radius, 1.0));
    }

    #[test]
    fn a_polygons_circles_lie_within_it_whichever_way_it_winds() {
        // A sliver of rock, and the hull of a ship, each counterclockwise.
        let shapes = [
            vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(4.0, 0.0),
                Vec2::new(0.0, 1.0),
            ],
            vec![
                Vec2::new(-2.5, 0.0),
                Vec2::new(2.5, 0.0),
                Vec2::new(1.5, 3.5),
                Vec2::new(0.0, 7.0),
                Vec2::new(-1.5, 3.5),
            ],
        ];
        for shape in shapes {
            let circles = polygon_circles(&shape);
            assert!(circles.len() > 1, "{circles:?}");
            for circle in &circles {
                assert!(within(&shape, *circle), "{circle:?}");
            }
            // Wound the other way, its row may run the other way along the
            // axis, but it is the same row.
            let clockwise: Vec<Vec2> = shape.iter().rev().copied().collect();
            let reversed = polygon_circles(&clockwise);
            assert_eq!(reversed.len(), circles.len());
            for (a, ra) in &circles {
                assert!(
                    reversed
                        .iter()
                        .any(|(b, rb)| a.distance_to(*b) < 1.0e-4 && close(*ra, *rb)),
                    "{a:?} in {reversed:?}"
                );
            }
        }
    }

    #[test]
    fn a_capsules_balls_touch_from_end_to_end() {
        let circles = capsule_circles([Vec2::new(0.0, -1.0), Vec2::new(0.0, 1.0)], 0.5);
        assert_eq!(
            circles,
            [
                (Vec2::new(0.0, -1.0), 0.5),
                (Vec2::new(0.0, 0.0), 0.5),
                (Vec2::new(0.0, 1.0), 0.5),
            ]
        );
        assert_eq!(capsule_circles([Vec2::ZERO; 2], 0.5), [(Vec2::ZERO, 0.5)]);
        assert!(capsule_circles([Vec2::ZERO, Vec2::X], 0.0).is_empty());
    }

    #[test]
    fn a_long_thin_polygon_has_at_most_a_full_row() {
        let rod = [
            Vec2::new(-50.0, -0.1),
            Vec2::new(50.0, -0.1),
            Vec2::new(50.0, 0.1),
            Vec2::new(-50.0, 0.1),
        ];
        assert_eq!(polygon_circles(&rod).len(), MAX_ROW);
    }

    #[test]
    fn a_polygon_without_area_has_none() {
        let line = [Vec2::ZERO, Vec2::X, Vec2::X * 2.0];
        assert!(polygon_circles(&line).is_empty());
        assert!(polygon_circles(&[]).is_empty());
    }

    #[test]
    fn sensors_and_missing_bodies_stand_for_nothing() {
        let sensor = ColliderSpec {
            sensor: true,
            ..ColliderSpec::ball(collider(0), 1.0)
        };
        let (world, body) = body(Vec2::ZERO, 0.0, &[sensor]);
        assert!(body_circles(&world, body, RECIPIENT, 2.0).is_empty());
        let missing = BodyId::new(PhysicsId::new(8), BodyRole::PRIMARY);
        assert!(body_circles(&world, missing, RECIPIENT, 2.0).is_empty());
    }
}
