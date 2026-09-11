use super::*;

#[test]
fn dynamic_terrain_recomputes_mass_and_preserves_surviving_point_velocities() {
    for surface in [TerrainSurface::Blocks, TerrainSurface::Contour] {
        edited_mass_and_velocity(surface);
    }
}

fn edited_mass_and_velocity(surface: TerrainSurface) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let mut terrain = Terrain::generate(
        9,
        5,
        0.5,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |p| {
            if p.x < 3 || p.y == 2 {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let mut geometry = TerrainGeometry::with_surface(&terrain, surface);
    let velocity = Vec2::new(3.0, -2.0);
    let mut assembly = TerrainAssembly::insert(
        &mut world,
        PhysicsId::new(99),
        BodySpec {
            position: Vec2::new(7.0, -4.0),
            angle: 0.7,
            linear_velocity: velocity,
            angular_velocity: 1.3,
            ..BodySpec::default()
        },
        &terrain,
        &geometry,
        TerrainSpec {
            surface,
            ..TerrainSpec::default()
        },
    )
    .unwrap();
    let body = assembly.body();
    assert!((world.body_mass(body).unwrap() - 21.0 * 0.25).abs() < 0.0001);
    assert_eq!(world.motion(body).unwrap().linear_velocity, velocity);
    let pose = world.motion(body).unwrap();
    let point = pose.position
        + terrain
            .cell_center(CellCoord::new(0, 0))
            .rotate_radians(pose.angle);
    let point_velocity = world.velocity_at_point(body, point).unwrap();
    let old_center = world.center_of_mass(body).unwrap();
    terrain
        .apply(TerrainEdit {
            brush: Brush::Capsule {
                start: CellCoord::new(3, 2),
                end: CellCoord::new(8, 2),
                radius: 0,
            },
            mode: EditMode::Remove,
        })
        .unwrap();
    geometry.refresh(&terrain);
    assembly
        .synchronize(&mut world, &terrain, &geometry)
        .unwrap();
    assert!((world.body_mass(body).unwrap() - 15.0 * 0.25).abs() < 0.0001);
    assert!(world.center_of_mass(body).unwrap().distance_to(old_center) > 0.5);
    assert!(
        world
            .velocity_at_point(body, point)
            .unwrap()
            .distance_to(point_velocity)
            < 0.0001
    );
    assert_eq!(world.motion(body).unwrap().position, pose.position);
    assert_eq!(world.motion(body).unwrap().angle, pose.angle);
    assert_eq!(
        world.motion(body).unwrap().angular_velocity,
        pose.angular_velocity
    );
}
use crate::world::{BodyKind, PhysicsWorldConfig, RayCastOptions};
use engine_core::Vec2;
use engine_terrain::{Brush, CellCoord, EditMode, Material, MaterialId, TerrainEdit};

fn fixture() -> (PhysicsWorld, Terrain, TerrainGeometry, TerrainAssembly) {
    fixture_surface(TerrainSurface::Blocks)
}

fn fixture_surface(
    surface: TerrainSurface,
) -> (PhysicsWorld, Terrain, TerrainGeometry, TerrainAssembly) {
    let terrain = Terrain::generate(
        96,
        64,
        0.5,
        vec![Material {
            id: MaterialId(1),
            hardness: 10,
        }],
        |_| MaterialId(1),
    )
    .unwrap();
    let geometry = TerrainGeometry::with_surface(&terrain, surface);
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let assembly = TerrainAssembly::insert(
        &mut world,
        PhysicsId::new(1),
        BodySpec {
            kind: BodyKind::KinematicPosition,
            ..BodySpec::default()
        },
        &terrain,
        &geometry,
        TerrainSpec {
            surface,
            ..TerrainSpec::default()
        },
    )
    .unwrap();
    world.step(1.0 / 60.0);
    (world, terrain, geometry, assembly)
}

#[test]
fn tunnel_crosses_chunks_and_ray_queries_follow_rotating_translating_field() {
    let (mut world, mut terrain, mut geometry, mut assembly) = fixture();
    let options = RayCastOptions {
        max_distance: 80.0,
        ..RayCastOptions::default()
    };
    assert!(
        world
            .cast_ray(Vec2::new(-40.0, 0.0), Vec2::X, options)
            .is_some()
    );
    terrain
        .apply(TerrainEdit {
            brush: Brush::Capsule {
                start: CellCoord::new(-5, 32),
                end: CellCoord::new(100, 32),
                radius: 3,
            },
            mode: EditMode::Remove,
        })
        .unwrap();
    geometry.refresh(&terrain);
    assert_eq!(
        assembly.synchronize(&mut world, &terrain, &geometry),
        Some(6)
    );
    let center = Vec2::new(5.0, -8.0);
    let angle = 0.73;
    world.set_next_kinematic_pose(assembly.body(), center, angle);
    world.step(1.0 / 60.0);
    let direction = Vec2::X.rotate_radians(angle);
    assert!(
        world
            .cast_ray(
                center + Vec2::new(-40.0, 0.0).rotate_radians(angle),
                direction,
                options
            )
            .is_none()
    );
    let hit = world
        .cast_ray(
            center + Vec2::new(-40.0, 4.0).rotate_radians(angle),
            direction,
            options,
        )
        .unwrap();
    assert!((hit.distance - 16.0).abs() < 0.01);
}

#[test]
fn replacement_preserves_other_chunks_body_motion_sensors_and_mappings() {
    let (mut world, mut terrain, mut geometry, mut assembly) = fixture();
    let sensor_id = ColliderId::new(assembly.body().entity, ColliderRole::new(5), 0);
    let mut sensor = ColliderSpec::ball(sensor_id, 1.0);
    sensor.sensor = true;
    world.insert_collider(assembly.body(), &sensor);
    let untouched_id = ColliderId::new(assembly.body().entity, ColliderRole::new(1005), 0);
    let untouched = world.collider_handle(untouched_id).unwrap();
    let sensor_handle = world.collider_handle(sensor_id).unwrap();
    let body_handle = world.body_handle(assembly.body()).unwrap();
    let before = world.motion(assembly.body());
    terrain
        .apply(TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(8, 8),
                radius: 4,
            },
            mode: EditMode::Remove,
        })
        .unwrap();
    geometry.refresh(&terrain);
    assert_eq!(
        assembly.synchronize(&mut world, &terrain, &geometry),
        Some(1)
    );
    assert_eq!(
        assembly.synchronize(&mut world, &terrain, &geometry),
        Some(0)
    );
    assert_eq!(world.motion(assembly.body()), before);
    assert_eq!(world.collider_handle(untouched_id), Some(untouched));
    assert_eq!(world.collider_handle(sensor_id), Some(sensor_handle));
    assert_eq!(world.body_handle(assembly.body()), Some(body_handle));
    terrain
        .apply(TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(48, 32),
                radius: 200,
            },
            mode: EditMode::Remove,
        })
        .unwrap();
    geometry.refresh(&terrain);
    assembly
        .synchronize(&mut world, &terrain, &geometry)
        .unwrap();
    world.step(1.0 / 60.0);
    assert_eq!(world.collider_count(), 1);
    assert_eq!(world.collider_handle(sensor_id), Some(sensor_handle));
    let mut restored = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    assert_eq!(restored.collider_count(), 1);
    assert!(restored.remove_entity(assembly.body().entity));
    assert_eq!(restored.collider_count(), 0);
}

#[test]
fn invalid_replacement_is_atomic_and_ccd_stops_at_thin_terrain() {
    let (mut world, _, _, assembly) = fixture();
    let role = ColliderRole::new(1000);
    let id = ColliderId::new(assembly.body().entity, role, 0);
    let before = world.snapshot_bytes().unwrap();
    assert!(!world.replace_colliders(assembly.body(), role, &[ColliderSpec::ball(id, f32::NAN)]));
    let valid = ColliderSpec::ball(id, 1.0);
    assert!(!world.replace_colliders(assembly.body(), role, &[valid.clone(), valid]));
    assert_eq!(before, world.snapshot_bytes().unwrap());

    let terrain = Terrain::generate(
        65,
        65,
        0.5,
        vec![Material {
            id: MaterialId(1),
            hardness: 1,
        }],
        |c| {
            if c.x == 32 {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let geometry = TerrainGeometry::new(&terrain);
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        max_ccd_substeps: 4,
        ..PhysicsWorldConfig::default()
    });
    TerrainAssembly::insert(
        &mut world,
        PhysicsId::new(1),
        BodySpec {
            kind: BodyKind::Fixed,
            ..BodySpec::default()
        },
        &terrain,
        &geometry,
        TerrainSpec::default(),
    )
    .unwrap();
    let shot = BodyId::new(PhysicsId::new(2), BodyRole::PRIMARY);
    world.insert_body(
        shot,
        BodySpec {
            position: Vec2::new(-3.0, 0.0),
            linear_velocity: Vec2::new(1000.0, 0.0),
            ccd_enabled: true,
            ..BodySpec::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(shot.entity, ColliderRole::PRIMARY, 0),
            0.15,
        )],
    );
    world.step(1.0 / 60.0);
    assert!(
        world.motion(shot).unwrap().position.x < 0.0,
        "CCD projectile crossed one-cell wall"
    );
}

#[test]
fn dynamic_body_crosses_a_completed_tunnel_without_hidden_contacts() {
    for surface in [TerrainSurface::Blocks, TerrainSurface::Contour] {
        cross_completed_tunnel(surface);
    }
}

fn cross_completed_tunnel(surface: TerrainSurface) {
    let (mut world, mut terrain, mut geometry, mut assembly) = fixture_surface(surface);
    terrain
        .apply(TerrainEdit {
            brush: Brush::Capsule {
                start: CellCoord::new(-5, 32),
                end: CellCoord::new(100, 32),
                radius: 4,
            },
            mode: EditMode::Remove,
        })
        .unwrap();
    geometry.refresh(&terrain);
    assembly
        .synchronize(&mut world, &terrain, &geometry)
        .unwrap();
    world.step(1.0 / 60.0);
    let traveler = BodyId::new(PhysicsId::new(2), BodyRole::PRIMARY);
    assert!(world.insert_body(
        traveler,
        BodySpec {
            position: Vec2::new(-27.0, 0.25),
            linear_velocity: Vec2::new(20.0, 0.0),
            ccd_enabled: true,
            ..BodySpec::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(traveler.entity, ColliderRole::PRIMARY, 0),
            0.3
        )]
    ));
    for _ in 0..180 {
        world.step(1.0 / 60.0);
    }
    let motion = world.motion(traveler).unwrap();
    assert!(
        motion.position.x > 30.0,
        "traveler hit stale terrain: {motion:?}"
    );
    assert!((motion.position.y - 0.25).abs() < 0.01);
}

mod contour;
