use super::*;
use engine_common::ClockDuckFlapHold;
use engine_rapier::world::RayCastOptions;

impl DuckEvent {
    pub(crate) fn flap_body_count(&self) -> usize {
        (0..2)
            .filter(|side| {
                self.world
                    .as_ref()
                    .is_some_and(|w| w.contains_body(flaps::body(*side)))
            })
            .count()
    }
}

impl crate::ClockState {
    pub(crate) fn flap_body_count(&self) -> usize {
        self.duck_scene().map_or(0, DuckEvent::flap_body_count)
    }
}

fn open_player(aspect: f32, direction: f32) -> DuckEvent {
    let mut duck = DuckEvent::new_player(Layout::new(aspect), 42, None, 1, 1);
    duck.direction = direction;
    for _ in 0..OPENING_TICKS {
        duck.step();
    }
    assert_eq!(duck.door_openness().0, 1.0);
    duck
}

fn place(duck: &mut DuckEvent, position: Vec2) {
    let world = duck.world.as_mut().unwrap();
    assert!(world.set_pose(DUCK_BODY, position, 0.0, true));
    assert!(world.set_velocity(DUCK_BODY, Vec2::ZERO, 0.0, true));
}

fn land_on_flap(duck: &mut DuckEvent) {
    let panel = duck.door_panels()[0].unwrap();
    place(
        duck,
        Vec2::new(
            panel.center().x,
            panel.top + panel.width + duck.radius * 1.02,
        ),
    );
    for _ in 0..10 {
        duck.step();
    }
    assert!(duck.grounded());
    assert!(
        duck.world
            .as_ref()
            .unwrap()
            .surface_contacts(DUCK_COLLIDER)
            .any(|c| c.collider.entity == flaps::body(0).entity && c.normal.y > 0.7)
    );
}

#[test]
fn rigid_flaps_hold_real_standing_support_then_release_after_a_jump() {
    for aspect in [800.0 / 480.0, 1024.0 / 768.0, 480.0 / 800.0] {
        for direction in [-1.0, 1.0] {
            let mut duck = open_player(aspect, direction);
            land_on_flap(&mut duck);
            let body = flaps::body(0);
            let start = duck.world.as_ref().unwrap().motion(body).unwrap();
            for _ in 0..100 {
                duck.step();
                assert!(duck.grounded());
                assert_eq!(duck.door_openness().0, 1.0);
                assert_eq!(
                    duck.world.as_ref().unwrap().motion(body).unwrap().position,
                    start.position
                );
            }
            let stats = duck.diagnostics().flaps[0];
            assert_eq!(stats.requested_open_milli, 0);
            assert_eq!(stats.hold, Some(ClockDuckFlapHold::Standing));
            assert_eq!(
                stats.angular_velocity_milliradians, 0,
                "rigid support never springs"
            );
            duck.set_player_input(0, true, false, false);
            duck.step();
            assert_eq!(duck.jumps, 1);
            let v = duck
                .world
                .as_ref()
                .unwrap()
                .motion(DUCK_BODY)
                .unwrap()
                .linear_velocity
                .y;
            let expected = (2.0 * duck.movement.gravity * duck.movement.jump_height).sqrt()
                - duck.movement.gravity * DT;
            assert!(
                (v - expected).abs() < 0.01,
                "ordinary jump, without a bonus: {v}/{expected}"
            );
            let mut previous = 1.0;
            for _ in 0..35 {
                duck.step();
                let open = duck.door_openness().0;
                assert!(open <= previous && previous - open <= 1.0 / 24.0 + 1e-6);
                previous = open;
            }
            assert_eq!(duck.door_openness().0, 0.0);
            assert!(!duck.world.as_ref().unwrap().contains_body(body));
            assert!(duck.door_panels()[0].is_none());
            assert_eq!(duck.door_openness().1, 1.0, "an empty exit remains open");
        }
    }
}

#[test]
fn attachment_is_bounded_by_live_geometry_reach_and_visit_identity() {
    let mut duck = open_player(5.0 / 3.0, 1.0);
    let panel = duck.door_panels()[0].unwrap();
    let half = panel.half_extents();
    let local = Vec2::new(-half.x, -half.y);
    let hand = panel.center() + local.rotate_radians(panel.angle);
    let radius = duck.radius;
    place(&mut duck, hand + Vec2::new(0.0, -radius * 2.0));
    // A stationary attachment fixture stands in for the later ledge controller.
    duck.world
        .as_mut()
        .unwrap()
        .set_body_kind(DUCK_BODY, BodyKind::KinematicPosition, true);
    let handle = duck.flap_handle(0).unwrap();
    assert!(!duck.attach_to_flap(handle, Vec2::new(f32::NAN, 0.0)));
    assert!(!duck.attach_to_flap(handle, Vec2::new(1000.0, 1000.0)));
    assert!(duck.attach_to_flap(handle, local));
    for _ in 0..100 {
        duck.step();
    }
    assert_eq!(
        duck.diagnostics().flaps[0].hold,
        Some(ClockDuckFlapHold::Attachment)
    );
    assert_eq!(duck.door_openness().0, 1.0);
    duck.release_flap_attachment();
    place(&mut duck, Vec2::new(0.0, 0.0));
    assert!(
        !duck.attach_to_flap(handle, local),
        "a distant actor cannot hold a flap"
    );
    for _ in 0..30 {
        duck.step();
    }
    assert_eq!(duck.door_openness().0, 0.0);
    assert!(
        !duck.attach_to_flap(handle, local),
        "closed panel cannot be attached"
    );
    duck.claim_arena();
    duck.retire_from_arena();
    duck.rejoin_arena(2, 1);
    for _ in 0..OPENING_TICKS {
        duck.step();
    }
    let panel = duck.door_panels()[0].unwrap();
    let hand = panel.center() + local.rotate_radians(panel.angle);
    let radius = duck.radius;
    place(&mut duck, hand + Vec2::new(0.0, -radius * 2.0));
    assert!(
        !duck.attach_to_flap(handle, local),
        "retired visit claims stay stale"
    );
    assert!(duck.attach_to_flap(duck.flap_handle(0).unwrap(), local));
}

#[test]
fn a_duck_in_the_closing_sweep_holds_the_last_safe_pose_without_snapping() {
    for direction in [-1.0, 1.0] {
        let mut duck = open_player(5.0 / 3.0, direction);
        for _ in 0..30 {
            duck.step();
        }
        let panel = duck.door_panels()[0].unwrap();
        assert!(duck.door_openness().0 < 0.8);
        let half = panel.half_extents();
        let position = panel.center()
            + Vec2::new(-panel.inward * (half.x + duck.radius * 1.02), 0.0)
                .rotate_radians(panel.angle);
        place(&mut duck, position);
        duck.world
            .as_mut()
            .unwrap()
            .set_body_kind(DUCK_BODY, BodyKind::KinematicPosition, true);
        let open = duck.door_openness().0;
        for _ in 0..30 {
            duck.step();
            assert_eq!(duck.door_openness().0, open);
            assert_eq!(
                duck.diagnostics().flaps[0].hold,
                Some(ClockDuckFlapHold::Obstructed)
            );
            assert_eq!(
                duck.world
                    .as_ref()
                    .unwrap()
                    .motion(flaps::body(0))
                    .unwrap()
                    .position,
                panel.center()
            );
        }
        place(&mut duck, Vec2::new(0.0, 0.0));
        duck.step();
        assert!((duck.door_openness().0 - (open - 1.0 / 24.0)).abs() < 1e-6);
        for _ in 0..24 {
            duck.step();
        }
        assert!(duck.door_panels()[0].is_none());
    }
}

#[test]
fn flap_render_geometry_matches_rays_bounds_and_motion_in_every_layout() {
    for aspect in [5.0 / 3.0, 4.0 / 3.0, 0.6] {
        for direction in [-1.0, 1.0] {
            let mut duck = DuckEvent::new_player(Layout::new(aspect), 42, None, 1, 1);
            duck.direction = direction;
            for tick in 1..=110 {
                duck.step();
                for (side, panel) in duck.door_panels().into_iter().enumerate() {
                    let Some(panel) = panel else { continue };
                    let world = duck.world.as_ref().unwrap();
                    let motion = world.motion(flaps::body(side)).unwrap();
                    assert!((motion.position - panel.center()).length() < 0.0001);
                    assert!((motion.angle - panel.angle).abs() < 0.0001);
                    let half = panel.half_extents();
                    let corners =
                        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(x, y)| {
                            panel.center()
                                + Vec2::new(x * half.x, y * half.y).rotate_radians(panel.angle)
                        });
                    let (min, max) = world.body_solid_bounds(flaps::body(side)).unwrap();
                    for point in corners {
                        assert!(point.x >= min.x - 1e-4 && point.x <= max.x + 1e-4);
                        assert!(point.y >= min.y - 1e-4 && point.y <= max.y + 1e-4);
                    }
                    if tick % 6 == 0 && panel.angle.abs() > 0.2 {
                        let normal = Vec2::new(panel.inward, 0.0).rotate_radians(panel.angle);
                        let face = panel.center() + normal * half.x;
                        let hit = world
                            .cast_ray(
                                face + normal * duck.radius * 0.1,
                                -normal,
                                RayCastOptions {
                                    max_distance: duck.radius * 0.2,
                                    exclude_entity: Some(DUCK_ENTITY),
                                    ..Default::default()
                                },
                            )
                            .expect("visible wood has a physical surface");
                        assert_eq!(hit.collider.entity, flaps::body(side).entity);
                        assert!((hit.point - face).length() < 0.001);
                    }
                }
                assert!(
                    duck.diagnostics()
                        .flaps
                        .iter()
                        .filter(|f| f.body_id.is_some())
                        .count()
                        <= 2
                );
            }
        }
    }
}

#[test]
fn retiring_flaps_removes_only_the_visits_bodies_and_claims() {
    let mut duck = open_player(5.0 / 3.0, 1.0);
    land_on_flap(&mut duck);
    let mut arena = VisitArena::claim(&mut duck);
    let foreign = PhysicsId::new(3001);
    let id = BodyId::new(foreign, BodyRole::PRIMARY);
    assert!(duck.arena_world_mut().insert_body(
        id,
        BodySpec {
            position: Vec2::new(0.0, 100.0),
            ..Default::default()
        },
        &[ColliderSpec::ball(
            ColliderId::new(foreign, ColliderRole::PRIMARY, 0),
            2.0
        )]
    ));
    let count = duck.physics_counts().0;
    let foreign_motion = duck.arena_world().motion(id);
    arena.retain(Box::new(duck));
    let duck = arena.vacant().unwrap();
    assert_eq!(
        duck.physics_counts().0,
        count - 2,
        "character and one open flap retire"
    );
    assert_eq!(duck.arena_world().motion(id), foreign_motion);
    assert!(duck.door_panels().iter().all(Option::is_none));
    let duck = arena.step(None, None, false);
    let after = duck.arena_world().motion(id).unwrap();
    assert!(
        (after.linear_velocity.y + duck.movement.gravity * DT).abs() < 0.001,
        "one shared step"
    );
    duck.release_arena();
    assert_eq!(duck.physics_counts(), (0, 0));
}

#[test]
fn a_jump_uses_the_support_points_incoming_motion() {
    let mut duck = open_player(5.0 / 3.0, 1.0);
    land_on_flap(&mut duck);
    let world = duck.world.as_mut().unwrap();
    let motion = world.motion(flaps::body(0)).unwrap();
    world.set_next_kinematic_pose(
        flaps::body(0),
        motion.position + Vec2::new(0.0, 2.0 * DT),
        motion.angle,
    );
    duck.set_player_input(0, true, false, false);
    duck.run(None);
    let v = duck
        .world
        .as_ref()
        .unwrap()
        .motion(DUCK_BODY)
        .unwrap()
        .linear_velocity
        .y;
    let expected = 2.0 + (2.0 * duck.movement.gravity * duck.movement.jump_height).sqrt()
        - duck.movement.gravity * DT;
    assert!(
        (v - expected).abs() < 0.005,
        "inherit physical support velocity: {v}/{expected}"
    );
}

#[test]
fn a_player_can_jump_onto_a_rigid_flap_from_the_real_course_floor() {
    // These are real Jump/steering sequences from the ordinary spawn, with no
    // repositioning, altered jump strength or attachment claim. The player can
    // catch the rigid surface as it closes. It stops while supporting them;
    // unlike the horizontal fixture above, a sloping flap can be slid off.
    for (aspect, jump_at, approach_at) in [(5.0 / 3.0, 8, 20), (4.0 / 3.0, 4, 20), (0.6, 8, 24)] {
        for direction in [-1.0, 1.0] {
            let mut duck = open_player(aspect, direction);
            let mut landed = false;
            for tick in 0..90 {
                let approach =
                    tick >= approach_at && duck.position().is_some_and(|p| p.x > duck.radius * 2.5);
                duck.set_player_input(
                    if approach {
                        (-direction * 1000.0) as i16
                    } else {
                        0
                    },
                    tick == jump_at,
                    false,
                    false,
                );
                duck.step();
                if duck.jumps == 1
                    && duck
                        .world
                        .as_ref()
                        .unwrap()
                        .surface_contacts(DUCK_COLLIDER)
                        .any(|c| {
                            c.collider.entity == flaps::body(0).entity
                                && c.normal.y > 0.7
                                && c.separation <= duck.radius * 0.05
                        })
                {
                    landed = true;
                    let open = duck.door_openness().0;
                    duck.set_player_input(0, false, false, false);
                    duck.step();
                    assert_eq!(duck.door_openness().0, open);
                    assert_eq!(
                        duck.diagnostics().flaps[0].hold,
                        Some(ClockDuckFlapHold::Standing)
                    );
                    break;
                }
            }
            assert!(landed, "aspect={aspect} direction={direction}");
        }
    }
}

#[test]
fn rigid_flap_scene_fixtures_cover_pause_floor_motion_and_target_viewports() {
    use crate::{ClockAction, ClockConfig, ClockDate, ClockReading, ClockScenario};
    use engine_common::{ClockEventProfile, Scenario};
    use std::time::Duration;
    let mut captures = Vec::new();
    for (width, height) in [(800, 480), (1024, 768), (480, 800)] {
        for direction in [-1.0, 1.0] {
            for moving_floor in [false, true] {
                let aspect = width as f32 / height as f32;
                let layout = Layout::new(aspect);
                let mut state = ClockScenario::init(
                    ClockConfig {
                        aspect_ratio: aspect,
                        event_profile: ClockEventProfile::Off,
                        ..Default::default()
                    },
                    42,
                );
                ClockScenario::step(
                    &mut state,
                    &[ClockAction::set_reading(
                        ClockReading::new(10, 8, 0)
                            .unwrap()
                            .with_date(ClockDate::new(2026, 10, 17).unwrap()),
                    )],
                    Duration::ZERO,
                );
                let mut duck = if moving_floor {
                    let mut floor = ResponsiveFloor::new(
                        crate::floor::responsive::FloorShape::clock(layout),
                        0.0,
                    );
                    floor.opening = 0.9;
                    DuckEvent::new_responsive_player(layout, 42, 1, 1, direction, floor, None)
                } else {
                    let mut duck = DuckEvent::new_player(layout, 42, None, 1, 1);
                    duck.direction = direction;
                    duck
                };
                for _ in 0..OPENING_TICKS {
                    duck.step();
                }
                land_on_flap(&mut duck);
                state.duck_visit = Some(Box::new(duck));
                let step = |state: &mut crate::ClockState, count| {
                    for _ in 0..count {
                        ClockScenario::step(state, &[], Duration::from_nanos(16_666_667));
                    }
                };
                for (stage, count) in [
                    ("standing", 60),
                    ("jump", 6),
                    ("closing", 10),
                    ("closed", 30),
                ] {
                    if stage == "jump" {
                        state
                            .duck_visit
                            .as_mut()
                            .unwrap()
                            .set_player_input(0, true, false, false);
                    }
                    step(&mut state, count);
                    let before = state.duck_scene().unwrap().diagnostics();
                    let frame = ClockScenario::render_frame(&state);
                    for _ in 0..5 {
                        ClockScenario::step(&mut state, &[], Duration::ZERO);
                    }
                    assert_eq!(state.duck_scene().unwrap().diagnostics(), before);
                    assert_eq!(ClockScenario::render_frame(&state), frame);
                    if stage == "standing" {
                        assert!(before.grounded);
                        assert_eq!(before.flaps[0].hold, Some(ClockDuckFlapHold::Standing));
                    }
                    if stage == "closed" {
                        assert_eq!(before.flaps[0].body_id, None);
                    }
                    captures.push(serde_json::json!({
                        "name": format!("flap-{width}x{height}-{}-{}-{stage}", if direction > 0.0 {"left"} else {"right"}, if moving_floor {"moving"} else {"course"}),
                        "width": width, "height": height, "seed":42, "stage":stage,
                        "moving_floor": moving_floor, "duck":before,
                        "bodies":state.body_count(), "colliders":state.collider_count(), "frame":frame
                    }));
                }
                state.set_aspect_ratio(0.75);
                assert_eq!((state.body_count(), state.collider_count()), (0, 0));
                assert!(state.duck_state().is_none());
            }
        }
    }
    if let Some(path) = std::env::var_os("SPACEWARS_DUCK_FLAP_FRAMES") {
        std::fs::write(path, serde_json::to_vec(&captures).unwrap()).unwrap();
    }
}
