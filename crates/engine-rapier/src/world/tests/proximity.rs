//! What is near a point, nearest first and within a bounded look; where a
//! moved body is before the next step; and how a position-driven body will
//! move through it.
use super::*;

const DT: f32 = 1.0 / 60.0;

fn collider(entity: u64) -> ColliderId {
    ColliderId::new(PhysicsId::new(entity), ColliderRole::PRIMARY, 0)
}

fn body(entity: u64) -> BodyId {
    BodyId::new(PhysicsId::new(entity), BodyRole::PRIMARY)
}

/// A row of two-unit boxes along the x axis, centered every three units
/// from x = 3, indexed by a step.
fn boxes(count: u64) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    for entity in 1..=count {
        assert!(world.insert_body(
            body(entity),
            BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::new(3.0 * entity as f32, 0.0),
                ..BodySpec::default()
            },
            &[ColliderSpec::cuboid(collider(entity), 1.0, 1.0)],
        ));
    }
    world.step(DT);
    world
}

#[test]
fn nearby_colliders_come_nearest_first_within_a_bounded_look() {
    let mut world = boxes(6);
    // A sensor where the search starts, which is never a solid to find.
    let mut sensor = ColliderSpec::ball(collider(100), 1.0);
    sensor.sensor = true;
    assert!(world.insert_body(
        body(100),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(9.5, 0.0),
            ..BodySpec::default()
        },
        &[sensor],
    ));
    world.step(DT);
    let options = NearbyOptions {
        collision_groups: CollisionGroups::ALL,
        exclude_entity: None,
        max_visited: 64,
        max_found: 3,
    };
    // From inside box 3, boxes 4, 2 and 5 are 1.5, 2.5 and 4.5 away.
    let center = Vec2::new(9.5, 0.0);
    let near = world.colliders_within(center, 5.0, options, |_| true);
    assert_eq!(near.colliders, vec![collider(3), collider(4), collider(2)]);
    assert_eq!(near.visited, 5);
    // What the caller refuses, or excludes, makes room for the next nearest.
    let near = world.colliders_within(center, 5.0, options, |c| c != collider(3));
    assert_eq!(near.colliders, vec![collider(4), collider(2), collider(5)]);
    let excluded = NearbyOptions {
        exclude_entity: Some(PhysicsId::new(3)),
        ..options
    };
    let near = world.colliders_within(center, 5.0, excluded, |_| true);
    assert_eq!(near.colliders, vec![collider(4), collider(2), collider(5)]);
    // A crowd costs the bound and no more.
    let world = boxes(40);
    let bounded = NearbyOptions {
        max_visited: 8,
        max_found: 64,
        ..options
    };
    let near = world.colliders_within(Vec2::new(60.0, 0.0), 100.0, bounded, |_| true);
    assert_eq!(near.visited, 8);
    assert_eq!(near.colliders.len(), 8);
}

#[test]
fn a_moved_body_is_measured_where_it_is_and_a_point_found_on_it_stays_on_it() {
    let mut world = boxes(1);
    // Moved up ten; rays see it where the last step left it.
    assert!(world.set_pose(body(1), Vec2::new(3.0, 10.0), 0.0, true));
    let hit = world
        .cast_ray(
            Vec2::ZERO,
            Vec2::X,
            RayCastOptions {
                max_distance: 10.0,
                solid: true,
                include_sensors: false,
                collision_groups: CollisionGroups::ALL,
                exclude_entity: None,
            },
        )
        .unwrap();
    assert!((hit.point - Vec2::new(2.0, 0.0)).length() < 1e-5);
    // The point the ray found is the box's own left face, wherever the box
    // now is; the body's present pose would put it in empty space.
    let local = world.queried_point_to_body(collider(1), hit.point).unwrap();
    assert!((local - Vec2::new(-1.0, 0.0)).length() < 1e-5, "{local:?}");
    let moved = world.body_point_to_world(body(1), local).unwrap();
    assert!((moved - Vec2::new(2.0, 10.0)).length() < 1e-5);
    let normal = world
        .queried_vector_to_body(collider(1), hit.normal)
        .unwrap();
    assert!((normal - Vec2::new(-1.0, 0.0)).length() < 1e-5);
    assert!((world.world_point_to_body(body(1), hit.point).unwrap() - local).length() > 9.0);
    // Measured directly, the box is where its body now is.
    let (nearest, inside) = world
        .nearest_boundary_point(collider(1), Vec2::new(0.0, 10.0))
        .unwrap();
    assert!(!inside);
    assert!((nearest - Vec2::new(2.0, 10.0)).length() < 1e-5);
}

#[test]
fn a_position_driven_body_moves_as_its_next_pose_says_before_it_gets_there() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    assert!(world.insert_body(
        body(1),
        BodySpec {
            kind: BodyKind::KinematicPosition,
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(collider(1), 1.0, 1.0)],
    ));
    world.step(DT);
    let point = Vec2::new(1.0, 0.0);
    assert_eq!(
        world.incoming_velocity_at_point(body(1), point, DT),
        Some(Vec2::ZERO)
    );
    // Told to slide and turn through the coming step: its solved velocity,
    // a step old, still says it is still.
    let angle = 0.3_f32;
    assert!(world.set_next_kinematic_pose(body(1), Vec2::new(0.5, 0.0), angle));
    let expected = (Vec2::new(0.5 + angle.cos(), angle.sin()) - point) / DT;
    let incoming = world
        .incoming_velocity_at_point(body(1), point, DT)
        .unwrap();
    assert!((incoming - expected).length() < 1e-3, "{incoming:?}");
    assert_eq!(world.velocity_at_point(body(1), point), Some(Vec2::ZERO));
    assert_eq!(world.incoming_velocity_at_point(body(1), point, 0.0), None);
}
