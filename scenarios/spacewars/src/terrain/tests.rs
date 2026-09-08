use super::*;
use engine_rapier::world::{CollisionGroups, RayCastOptions};

fn step(state: &mut SpacewarsState) {
    SpacewarsScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
}

fn fixture() -> SpacewarsState {
    let mut state = SpacewarsScenario::init_terrain_fixture(42);
    state.planets[0].mass = 0.0;
    state.planets[0].wrapper_omega = 0.0;
    step(&mut state);
    state
}

fn remaining(state: &SpacewarsState) -> usize {
    state
        .terrain
        .planets
        .values()
        .map(|p| &p.field)
        .chain(state.terrain.fragments.values().map(|f| &f.terrain))
        .map(|t| {
            t.cells()
                .iter()
                .filter(|c| c.material != MaterialId::VOID)
                .count()
        })
        .sum()
}

fn cut(state: &mut SpacewarsState, start: Vec2, end: Vec2, radius: u32) {
    let terrain = state.planet_terrain(0).unwrap();
    let edit = TerrainEdit {
        brush: Brush::Capsule {
            start: terrain.local_to_cell(start).unwrap(),
            end: terrain.local_to_cell(end).unwrap(),
            radius,
        },
        mode: EditMode::Remove,
    };
    state.queue_planet_edit(0, edit).unwrap();
}

#[test]
fn real_cannon_commits_one_bounded_crater_on_the_following_tick() {
    let mut state = fixture();
    let initial = remaining(&state);
    SpacewarsScenario::step(
        &mut state,
        &[SpacewarsAction::set_cannon(0, true)],
        Duration::from_secs_f64(1.0 / 60.0),
    );
    state.ships[0].set_cannon(false);
    for _ in 0..60 {
        if state.terrain_cannon_hits() > 0 {
            break;
        }
        step(&mut state);
    }
    assert_eq!(state.terrain_cannon_hits(), 1);
    assert_eq!(state.terrain_removed_cells(), 0, "damage is still queued");
    assert_eq!(state.terrain.pending.len(), 1);
    let mut checkpoint = state.clone();
    step(&mut state);
    let removed = state.terrain_removed_cells();
    assert!(
        removed > 0 && removed <= 113,
        "bounded radius-six footprint: {removed}"
    );
    assert_eq!(initial, remaining(&state) + removed as usize);
    assert!(
        state
            .planet_terrain(0)
            .unwrap()
            .cells()
            .iter()
            .any(|c| c.material == ORE && c.durability == 20)
    );
    for _ in 0..60 {
        step(&mut state);
    }
    for _ in 0..61 {
        step(&mut checkpoint);
    }
    assert_eq!(
        SpacewarsScenario::observe(&state).payload,
        SpacewarsScenario::observe(&checkpoint).payload
    );
    assert_eq!(
        state.physics.snapshot_bytes(),
        checkpoint.physics.snapshot_bytes()
    );
    assert_eq!(
        state.terrain_cannon_hits(),
        1,
        "a consumed shell cannot hit again"
    );
    assert_eq!(state.terrain_removed_cells(), removed);
}

#[test]
fn complete_tunnel_passes_lasers_and_a_real_ship_between_both_terrain_bodies() {
    let mut state = fixture();
    let center = state.planets[0].position;
    let start = center + Vec2::new(0.0, 100.0);
    let down = Vec2::new(0.0, -1.0);
    let initial_hit = state.physics.cast_laser(0, start, down, 200.0).unwrap();
    assert_eq!(
        initial_hit.target,
        Some(MechanicalEntity::Body(BodyId::Planet(0)))
    );
    assert!(initial_hit.point.y > center.y + 50.0);
    let initial = remaining(&state);
    cut(&mut state, Vec2::new(0.0, -90.0), Vec2::new(0.0, 90.0), 14);
    step(&mut state);
    assert_eq!(state.terrain.fragments.len(), 1);
    assert_eq!(
        initial,
        remaining(&state) + state.terrain_removed_cells() as usize
    );
    assert!(state.physics.cast_laser(0, start, down, 200.0).is_none());
    state.ships[0].position = start;
    state.ships[0].velocity = down * 100.0;
    let life = state.ships[0].life;
    for _ in 0..130 {
        step(&mut state);

        assert!(state.body_collisions.iter().all(|c| c.ship != 0));
        if state.ships[0].position.y < center.y - 90.0 {
            break;
        }
    }
    assert!(
        state.ships[0].position.y < center.y - 90.0,
        "ship should exit tunnel: {:?}",
        state.ships[0].position
    );
    assert_eq!(state.ships[0].life, life);
}

#[test]
fn fast_shell_damage_stays_on_a_rotated_moving_planets_contact_cells() {
    let mut state = fixture();
    state.planets[0].wrapper_angle = 0.73;
    state.planets[0].wrapper_omega = 0.04;
    step(&mut state);
    let center = state.planets[0].position;
    let outward = Vec2::new(0.0, 1.0).rotate_radians(0.73);
    state.ships[0].position = center + Vec2::new(150.0, 100.0);
    state.debris.push(DebrisState::new_shell(
        0,
        0,
        center + outward * 85.0,
        -outward * 1800.0,
        0.73,
    ));
    for _ in 0..10 {
        step(&mut state);
        if state.terrain_cannon_hits() > 0 {
            break;
        }
    }
    assert_eq!(state.terrain_cannon_hits(), 1);
    let PendingEdit { body, edit } = state.terrain.pending[0];
    assert_eq!(body, physics::planet_entity(0));
    let Brush::Circle { center: cell, .. } = edit.brush else {
        panic!("cannon circle");
    };
    let point = state.planet_terrain(0).unwrap().cell_center(cell);
    assert!(
        point.x.abs() < 5.0 && point.y > 50.0,
        "body-local surface point: {point:?}"
    );
    step(&mut state);
    assert!(state.terrain_removed_cells() > 0);
}

#[test]
fn removing_base_footing_releases_hold_and_disables_services_and_rebuilding() {
    let mut state = fixture();
    let anchor = spaceport_docking_anchor(&state.planets[0]);
    state.ships[0].position = anchor;
    state.ships[0].set_brake(1.0);
    state.ships[0].life = 10.0;
    for _ in 0..5 {
        step(&mut state);
    }
    assert!(state.physics.ship_is_constrained(0));
    assert!(!state.spaceport_contacts.is_empty());
    let life = state.ships[0].life;
    cut(&mut state, Vec2::new(58.0, -5.0), Vec2::new(58.0, 5.0), 4);
    step(&mut state);
    assert!(!state.planet_base_supported(0));
    assert!(!state.physics.ship_is_constrained(0));
    assert!(state.spaceport_contacts.is_empty());
    for _ in 0..20 {
        step(&mut state);
    }
    assert_eq!(state.ships[0].life, life, "unsupported pad must not heal");
    assert_eq!(state.planets[0].owner_id, Some(0));
    assert_eq!(state.planets[0].building_new_ship_time, 0.0);
    assert!(!spaceport_accepts_ship(&state, 0, 0));
    state.rovers.clear();
    state.rover_builds[0].progress = 1.0;
    for _ in 0..10 {
        step(&mut state);
    }
    assert!(
        state.rovers.is_empty(),
        "offline base must not deploy a new rover"
    );
    state.ships[0].form = ShipForm::EscapePod;
    state.ships[0].position = anchor;
    state.ships[0].life = 0.0;
    for _ in 0..120 {
        step(&mut state);
    }
    assert_eq!(
        state.ships[0].life, 0.0,
        "unsupported owned berth must not rebuild a pod"
    );
    assert_eq!(state.planets[0].building_new_ship_time, 0.0);
    assert!(state.spaceport_contacts.is_empty());
}

#[test]
fn rover_has_no_hidden_circular_surface_after_ground_is_removed() {
    let mut state = SpacewarsScenario::init_terrain_fixture(42);
    for _ in 0..120 {
        step(&mut state);
    }
    let rover = state.rovers[0];
    let before = state.physics.rover_snapshot(&rover).unwrap();
    let planet = state.planets[0];
    let local = (before.chassis.position - planet.position).rotate_radians(-planet.wrapper_angle);
    cut(&mut state, local, local * 0.3, 12);
    step(&mut state);
    let inward = (planet.position - before.chassis.position).normalized();
    let ray = state.physics.world.cast_ray(
        before.chassis.position,
        inward,
        RayCastOptions {
            max_distance: 15.0,
            include_sensors: false,
            collision_groups: CollisionGroups::new(1 << 9, 1 << 5 | 1 << 10),
            ..RayCastOptions::default()
        },
    );
    assert!(ray.is_none(), "the old rover-only circle must be gone");
    let old_radius = before.chassis.position.distance_to(planet.position);
    let mut minimum = old_radius;
    for _ in 0..30 {
        step(&mut state);
        minimum = minimum.min(
            state
                .physics
                .rover_snapshot(&rover)
                .unwrap()
                .chassis
                .position
                .distance_to(state.planets[0].position),
        );
    }
    assert!(
        minimum < old_radius - 4.0,
        "rover should fall into the excavation: {old_radius} -> {minimum}"
    );
}

#[test]
fn pause_keeps_pending_edits_and_fixture_controls_require_release() {
    let mut state = fixture();
    let action = fixture_controls(true, false);
    SpacewarsScenario::step(
        &mut state,
        std::slice::from_ref(&action),
        Duration::from_secs_f64(1.0 / 60.0),
    );
    let removed = state.terrain_removed_cells();
    assert!(removed > 0);
    SpacewarsScenario::step(&mut state, &[action], Duration::from_secs_f64(1.0 / 60.0));
    assert_eq!(state.terrain_removed_cells(), removed);
    cut(
        &mut state,
        Vec2::new(-30.0, 30.0),
        Vec2::new(-30.0, 30.0),
        3,
    );
    let observation = SpacewarsScenario::observe(&state).payload;
    SpacewarsScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(SpacewarsScenario::observe(&state).payload, observation);
    step(&mut state);
    assert!(state.terrain_removed_cells() > removed);
    let reset = SpacewarsScenario::init_terrain_fixture(42);
    assert_eq!(reset.terrain_removed_cells(), 0);
    assert_eq!(reset.terrain_fragments().count(), 0);
}

#[test]
fn fully_removed_planet_leaves_no_solid_or_service_colliders() {
    let mut state = fixture();
    let initial = remaining(&state);
    cut(&mut state, Vec2::ZERO, Vec2::ZERO, 100);
    step(&mut state);
    assert_eq!(remaining(&state), 0);
    assert_eq!(state.terrain_removed_cells() as usize, initial);
    assert!(!state.planet_base_supported(0));
    assert!(
        state
            .physics
            .cast_laser(
                0,
                state.planets[0].position + Vec2::new(0.0, 90.0),
                Vec2::new(0.0, -1.0),
                180.0
            )
            .is_none()
    );
}

#[test]
fn cannon_and_laser_hit_a_detached_piece_and_its_last_cell_cleans_up() {
    let mut state = fixture();
    let id = FRAGMENT_ID_BASE;
    let position = state.planets[0].position + Vec2::new(120.0, 0.0);
    let field = Terrain::generate(
        1,
        1,
        1.0,
        vec![Material {
            id: ROCK,
            hardness: 100,
        }],
        |_| ROCK,
    )
    .unwrap();
    let fragment = TerrainFragment::insert(
        &mut state.physics.world,
        PhysicsId::new(id),
        engine_terrain::DetachedTerrain {
            terrain: field,
            parent_offset: Vec2::ZERO,
        },
        engine_rapier::world::BodyMotion {
            position,
            angle: 0.0,
            linear_velocity: Vec2::ZERO,
            angular_velocity: 0.0,
        },
        position,
        physics::terrain_spec(),
    )
    .unwrap();
    state.terrain.fragments.insert(id, fragment);
    state.terrain.next_fragment = id + 1;
    state.physics.terrain_fragments.insert(id);
    step(&mut state);
    let hit = state
        .physics
        .cast_laser(
            0,
            position + Vec2::new(0.0, 10.0),
            Vec2::new(0.0, -1.0),
            20.0,
        )
        .unwrap();
    assert_eq!(hit.target, Some(MechanicalEntity::TerrainFragment(id)));
    let planet_hash = state.planet_terrain(0).unwrap().hash();
    state.debris.push(DebrisState::new_shell(
        0,
        0,
        position + Vec2::new(0.0, 10.0),
        Vec2::new(0.0, -300.0),
        0.0,
    ));
    for _ in 0..8 {
        step(&mut state);
        if state.terrain.fragments.is_empty() {
            break;
        }
    }
    assert!(state.terrain.fragments.is_empty());
    assert!(!state.physics.world.contains_entity(PhysicsId::new(id)));
    assert_eq!(state.terrain_removed_cells(), 1);
    assert_eq!(state.planet_terrain(0).unwrap().hash(), planet_hash);
}

#[test]
fn simultaneous_shells_share_one_budget_and_do_not_leave_delayed_work() {
    let mut state = fixture();
    cut(
        &mut state,
        Vec2::new(-70.0, 60.0),
        Vec2::new(70.0, 60.0),
        10,
    );
    step(&mut state);
    let center = state.planets[0].position;
    state.ships[0].position = center + Vec2::new(150.0, 150.0);
    for x in [-24.0, -12.0, 0.0, 12.0, 24.0] {
        let field = state.planet_terrain(0).unwrap();
        let column = field.local_to_cell(Vec2::new(x, 0.0)).unwrap().x;
        let y = (0..field.height() as i32)
            .rev()
            .find(|y| field.cell(CellCoord::new(column, *y)).unwrap().material != MaterialId::VOID)
            .unwrap();
        let surface = field.cell_center(CellCoord::new(column, y));
        state.debris.push(DebrisState::new_shell(
            0,
            0,
            center + surface + Vec2::new(0.0, 2.0),
            Vec2::new(0.0, -300.0),
            0.0,
        ));
    }
    step(&mut state);
    assert_eq!(state.terrain.pending.len(), MAX_CANNON_HITS);
    assert_eq!(state.terrain.cannon_hits, 4);
    assert_eq!(state.terrain.budget_skips, 1);
    step(&mut state);
    let removed = state.terrain.removed_cells;
    for _ in 0..10 {
        step(&mut state);
    }
    assert_eq!(state.terrain.cannon_hits, 4);
    assert_eq!(state.terrain.removed_cells, removed);
    assert!(state.terrain.pending.is_empty());
}

#[test]
fn detached_terrain_collides_with_the_sun_ordinary_planets_and_world_boundary() {
    for case in 0..3 {
        let mut state = fixture();
        let target = Vec2::new(250.0, 500.0);
        let (start, velocity, target_id) = match case {
            0 => {
                state.sun = Some(SunState {
                    position: target,
                    radius: 20.0,
                    mass: 0.0,
                    color: Color::YELLOW,
                });
                (
                    target + Vec2::new(40.0, 0.0),
                    Vec2::new(-300.0, 0.0),
                    PhysicsId::new(2),
                )
            }
            1 => {
                state.planets.push(PlanetState {
                    position: target,
                    radius: 20.0,
                    mass: 0.0,
                    owner_id: None,
                    ..state.planets[0]
                });
                (
                    target + Vec2::new(40.0, 0.0),
                    Vec2::new(-300.0, 0.0),
                    physics::planet_entity(1),
                )
            }
            _ => (
                Vec2::new(980.0, 500.0),
                Vec2::new(300.0, 0.0),
                PhysicsId::new(1),
            ),
        };
        reconcile_physics(&mut state, 1.0 / 60.0);
        let field = Terrain::generate(
            1,
            1,
            1.0,
            vec![Material {
                id: ROCK,
                hardness: 100,
            }],
            |_| ROCK,
        )
        .unwrap();
        let id = PhysicsId::new(FRAGMENT_ID_BASE);
        TerrainFragment::insert(
            &mut state.physics.world,
            id,
            engine_terrain::DetachedTerrain {
                terrain: field,
                parent_offset: Vec2::ZERO,
            },
            engine_rapier::world::BodyMotion {
                position: start,
                angle: 0.0,
                linear_velocity: velocity,
                angular_velocity: 0.0,
            },
            start,
            physics::terrain_spec(),
        )
        .unwrap();
        let mut contacted = false;
        for _ in 0..15 {
            state.physics.world.step(1.0 / 60.0);
            contacted |= state.physics.world.contact_events().iter().any(|e| {
                let pair = [e.collider_a.entity, e.collider_b.entity];
                pair.contains(&id) && pair.contains(&target_id) && e.impulse_magnitude > 0.0
            });
        }
        assert!(contacted, "detached terrain should collide in case {case}");
    }
}
