use super::*;
use engine_material::{ARMOR, Beam, Connection, SOFT_TISSUE, SustainedLoad, sustained};

/// The sustained law's outcome for `force` on `surface`, across a patch
/// `width` wide.
fn held(surface: &ContactSurface, force: f32, width: f32, duration: f32) -> Outcome {
    let depth = match surface.depth {
        Depth::Fixed(depth) => depth,
        Depth::Patch(extent) => engine_material::deformation_depth(width, extent).unwrap(),
    };
    sustained(
        &surface.region,
        SustainedLoad {
            force,
            width,
            section: Section::new(surface.thickness, depth),
            duration,
            measured_plastic_displacement: None,
        },
    )
    .unwrap()
}

/// The normal impulse the last step's solve applied, over all contacts.
fn applied(world: &PhysicsWorld) -> f32 {
    let mut applied = 0.0;
    for pair in world.raw.narrow_phase.contact_pairs() {
        for manifold in &pair.manifolds {
            for contact in &manifold.data.solver_contacts {
                let point = &manifold.points[contact.contact_id[0] as usize];
                applied += (point.data.impulse - contact.warmstart_impulse).max(0.0);
            }
        }
    }
    applied
}

fn episode_of(charge: &SupportCharge) -> EpisodeId {
    match charge {
        SupportCharge::Charged { episode, .. } | SupportCharge::Unpriced { episode, .. } => {
            *episode
        }
    }
}

#[test]
fn a_resting_load_costs_nothing_and_a_crushing_one_costs_as_sustained_gives() {
    // A light box rests on rock. A heavy one presses on soft tissue, a
    // half-space, past its safe stress until it fails.
    for (material, density, depth, crushed) in [
        (ROCK, 1.0, Depth::Fixed(SECTION.depth), false),
        (SOFT_TISSUE, 25.0, Depth::Patch(None), true),
    ] {
        let mut world = world(-Vec2::Y * GRAVITY);
        let mut host = Host::default();
        let ground = floor(&mut world, &mut host, material, depth);
        let block = resting_box(&mut world, &mut host, 2, density, false);
        let sources = [ground, block];
        let weight = GRAVITY * world.body_mass(body(2)).unwrap();
        let mut absorbed = 0.0;
        for _ in 0..30 {
            world.step(DT);
            let before = host.surfaces.clone();
            host.absorbed.clear();
            let charges = world.charge_contact_supports(&mut host).unwrap();
            assert_eq!(charges.len(), 1, "{charges:?}");
            let (force, width, duration, outcomes) = supported(&charges[0]);
            // The force the solve applied. Rapier's report also counts the
            // warm start, a quarter more at four solver iterations.
            assert!(close(force, weight, 1.0e-4), "{force} vs {weight}");
            assert!(close(width, 2.0, 1.0e-5), "{width}");
            assert!(close(duration, DT, 1.0e-6), "{duration}");
            for side in 0..2 {
                assert_eq!(
                    outcomes[side],
                    held(&before[&sources[side]], force, width, duration)
                );
            }
            let charged: Vec<_> = (0..2)
                .filter(|&side| outcomes[side].consumed[0] > 0.0)
                .map(|side| (sources[side], outcomes[side].consumed[0]))
                .collect();
            assert_eq!(host.absorbed, charged);
            absorbed += charged.iter().map(|(_, work)| work).sum::<f32>();
        }
        if crushed {
            let tissue = host.surfaces[&ground].region;
            assert!(tissue.failed(), "{tissue:?}");
            assert!(close(absorbed, tissue.capacity(), 1.0e-4), "{absorbed}");
        } else {
            assert_eq!(absorbed, 0.0);
        }
    }
}

#[test]
fn a_step_that_ccd_splits_prices_no_supports() {
    // A fast CCD bullet strikes a wall elsewhere, which splits a step. The
    // last substep's impulse could carry a hit delivered in a small part of
    // the step, so the step's supports are left out. The steps around it price
    // the box at its weight.
    let mut world = world_with(-Vec2::Y * GRAVITY, 4);
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    resting_box(&mut world, &mut host, 2, 1.0, false);
    let weight = GRAVITY * world.body_mass(body(2)).unwrap();
    let wall = BodySpec {
        kind: BodyKind::Fixed,
        position: Vec2::new(26.0, 5.0),
        ..Default::default()
    };
    assert!(world.insert_body(body(3), wall, &[cuboid(3, 0, 0.5, 3.0)]));
    let shot = BodySpec {
        position: Vec2::new(20.0, 5.0),
        linear_velocity: Vec2::X * 400.0,
        gravity_scale: 0.0,
        ccd_enabled: true,
        can_sleep: false,
        ..Default::default()
    };
    assert!(world.insert_body(body(4), shot, &[ball(4, 0.1, 1.0)]));
    let (mut split, mut whole) = (0, 0);
    for _ in 0..8 {
        world.step(DT);
        let charges = world.charge_contact_supports(&mut host).unwrap();
        if world.loading_batches().len() > 1 {
            split += 1;
            assert_eq!(charges, Vec::new());
        } else {
            whole += 1;
            let (force, _, duration, _) = supported(&charges[0]);
            assert!(close(force, weight, 1.0e-4), "{force} vs {weight}");
            assert!(close(duration, DT, 1.0e-6), "{duration}");
        }
    }
    assert!(split > 0 && whole > 0, "{split} split, {whole} whole");
}

#[test]
fn a_chassis_welded_onto_a_foot_loads_the_foots_support_and_motion_is_unchanged() {
    let build = || {
        let mut world = world(-Vec2::Y * GRAVITY);
        let mut host = Host::default();
        floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
        let mut sole = cuboid(2, 0, 1.0, 0.25);
        sole.density = 1.0;
        let foot = BodySpec {
            position: Vec2::Y * 0.25,
            can_sleep: false,
            ..Default::default()
        };
        add(&mut world, &mut host, 2, foot, &[sole], HULL, None);
        let mut hull = cuboid(3, 0, 1.0, 0.5);
        hull.density = 4.0;
        let chassis = BodySpec {
            position: Vec2::Y,
            can_sleep: false,
            ..Default::default()
        };
        assert!(world.insert_body(body(3), chassis, &[hull]));
        world
            .insert_connection(
                JointId::new(PhysicsId::new(3), JointRole::PRIMARY),
                ConnectionSpec {
                    root_body: body(2),
                    body: body(3),
                    root: Vec2::Y * 0.5,
                    axis: Vec2::Y,
                    connection: Connection::new(ARMOR, Beam::new(1.0, 2.0, 0.5)).unwrap(),
                },
            )
            .unwrap();
        (world, host)
    };
    let (mut priced, mut host) = build();
    let (mut plain, _) = build();
    let weight =
        GRAVITY * (priced.body_mass(body(2)).unwrap() + priced.body_mass(body(3)).unwrap());
    let mut force = 0.0;
    for _ in 0..120 {
        priced.step(DT);
        plain.step(DT);
        let charges = priced.charge_contact_supports(&mut host).unwrap();
        force = supported(&charges[0]).0;
        for n in [2, 3] {
            assert_eq!(priced.motion(body(n)), plain.motion(body(n)));
        }
    }
    // The foot carries the chassis through the weld.
    assert!(close(force, weight, 1.0e-3), "{force} vs {weight}");
}

#[test]
fn a_landing_is_priced_as_an_impact_and_then_as_a_support() {
    // A box dropped from 1 lands at about 4.5, and one 0.0001 above the floor
    // at 0.05, slower than a support's points may move. Each landing's step is
    // the impact's.
    for (height, speed) in [(1.0, 0.0), (0.0001, 0.05)] {
        let mut world = world(-Vec2::Y * GRAVITY);
        let mut host = Host::default();
        floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
        let dropped = BodySpec {
            position: Vec2::Y * (0.5 + height),
            linear_velocity: -Vec2::Y * speed,
            can_sleep: false,
            ..Default::default()
        };
        add(
            &mut world,
            &mut host,
            2,
            dropped,
            &[cuboid(2, 0, 1.0, 0.5)],
            HULL,
            None,
        );
        let weight = GRAVITY * world.body_mass(body(2)).unwrap();
        let mut landed = None;
        let mut supports = Vec::new();
        for _ in 0..120 {
            world.step(DT);
            let impacts = world.charge_contact_impacts(&mut host).unwrap();
            let charges = world.charge_contact_supports(&mut host).unwrap();
            if let Some(ContactCharge::Charged { episode, .. }) = impacts.first() {
                assert!(landed.is_none(), "{impacts:?}");
                landed = Some(*episode);
                assert!(
                    charges.iter().all(|c| episode_of(c) != landed.unwrap()),
                    "{charges:?}"
                );
            } else {
                assert!(impacts.is_empty(), "{impacts:?}");
                if landed.is_some() {
                    supports.extend(charges);
                } else {
                    assert!(charges.is_empty(), "{charges:?}");
                }
            }
        }
        let landed = landed.unwrap();
        assert!(!supports.is_empty());
        assert!(
            supports.iter().all(|c| episode_of(c) == landed),
            "{supports:?}"
        );
        let (force, ..) = supported(supports.last().unwrap());
        assert!(close(force, weight, 1.0e-4), "{force} vs {weight}");
    }
}

#[test]
fn a_corner_folded_into_an_open_landing_is_not_priced_as_a_support() {
    // A tilted box lands on its lower corner and turns onto the other one
    // while the landing's episode is open. That corner approaches faster than
    // a support's points may move, so the step is a hit's, not a support's.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    let tilt = 0.1_f32;
    let dropped = BodySpec {
        position: Vec2::Y * (0.2 + 0.5 * tilt.cos() + tilt.sin()),
        angle: tilt,
        can_sleep: false,
        ..Default::default()
    };
    add(
        &mut world,
        &mut host,
        2,
        dropped,
        &[cuboid(2, 0, 1.0, 0.5)],
        HULL,
        None,
    );
    let weight = GRAVITY * world.body_mass(body(2)).unwrap();
    let mut folded = 0;
    let mut last = Vec::new();
    for _ in 0..240 {
        world.step(DT);
        let impacts = world.charge_contact_impacts(&mut host).unwrap();
        let supports = world.charge_contact_supports(&mut host).unwrap();
        let approaching = world
            .loading_batches()
            .iter()
            .flat_map(|b| &b.contacts)
            .any(|r| r.closing_speed > 0.1 && r.separation <= 0.01);
        if approaching {
            assert!(supports.is_empty(), "{supports:?}");
            folded += usize::from(impacts.is_empty());
        }
        if !supports.is_empty() {
            last = supports;
        }
    }
    assert!(folded > 0);
    let (force, ..) = supported(&last[0]);
    assert!(close(force, weight, 1.0e-4), "{force} vs {weight}");
}

#[test]
fn a_contact_that_carries_nothing_is_no_support() {
    // Without gravity, a box rests on the floor and presses on nothing.
    let mut world = world(Vec2::ZERO);
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    resting_box(&mut world, &mut host, 2, 1.0, false);
    for _ in 0..10 {
        world.step(DT);
        let touching = world.raw.narrow_phase.contact_pairs().any(|pair| {
            pair.manifolds
                .iter()
                .any(|m| !m.data.solver_contacts.is_empty())
        });
        assert!(touching);
        assert_eq!(
            world.charge_contact_supports(&mut host).unwrap(),
            Vec::new()
        );
    }
}

#[test]
fn a_support_holds_steady_while_its_points_move_slowly() {
    // The resting box is set moving along the floor's normal. Rising, it
    // leaves the floor, and gravity brings it back within the step, so the
    // floor's impulse stops its return. Pressed in, the floor stops it. At
    // 0.05, under gravity 10, either is a resting support's jitter, priced at
    // what the step applied. At 0.5 it is a hit within the step, not priced,
    // and so is 0.05 at a length unit of 0.1.
    for (rising, gravity, unit, steady) in [
        (0.05, GRAVITY, 1.0, true),
        (-0.05, GRAVITY, 1.0, true),
        (0.05, GRAVITY, 0.1, false),
        (0.5, 100.0, 1.0, false),
        (-0.5, GRAVITY, 1.0, false),
    ] {
        let mut world = world(-Vec2::Y * gravity);
        assert!(world.set_length_unit(unit));
        let mut host = Host::default();
        floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
        resting_box(&mut world, &mut host, 2, 1.0, false);
        let weight = gravity * world.body_mass(body(2)).unwrap();
        for _ in 0..10 {
            world.step(DT);
            world.charge_contact_supports(&mut host).unwrap();
        }
        assert!(world.set_velocity(body(2), Vec2::Y * rising, 0.0, true));
        world.step(DT);
        let moving = world.loading_batches()[0]
            .contacts
            .iter()
            .all(|r| close(r.closing_speed, -rising, 1.0e-3));
        assert!(moving, "{:?}", world.loading_batches());
        let applied = applied(&world);
        assert!(
            applied > 0.0 && (applied < weight * DT) == (rising > 0.0),
            "{applied}"
        );
        let charges = world.charge_contact_supports(&mut host).unwrap();
        if steady {
            let (force, ..) = supported(&charges[0]);
            assert!(close(force * DT, applied, 1.0e-4), "{force} vs {applied}");
        } else {
            assert_eq!(charges, Vec::new());
        }
        world.step(DT);
        let charges = world.charge_contact_supports(&mut host).unwrap();
        let (force, ..) = supported(&charges[0]);
        assert!(close(force, weight, 1.0e-4), "{force} vs {weight}");
    }
}

#[test]
fn a_support_whose_body_lands_within_the_step_is_not_priced() {
    // The resting box is lifted 0.0019 off the floor and dropped at 0.2, its
    // episode still open. Capture sees it apart, closing too slowly to land
    // within a substep, but the step's gravity lands it, and the solve stops
    // it there.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    resting_box(&mut world, &mut host, 2, 1.0, false);
    for _ in 0..10 {
        world.step(DT);
        assert_eq!(world.charge_contact_supports(&mut host).unwrap().len(), 1);
    }
    assert!(world.set_pose(body(2), Vec2::Y * (0.5 + 0.0019), 0.0, true));
    assert!(world.set_velocity(body(2), -Vec2::Y * 0.2, 0.0, true));
    world.step(DT);
    let records = &world.loading_batches()[0].contacts;
    assert!(
        records
            .iter()
            .all(|r| r.separation > 1.0e-3 && r.closing_speed * DT < r.separation),
        "{records:?}"
    );
    assert!(applied(&world) > 0.0);
    assert_eq!(
        world.charge_contact_supports(&mut host).unwrap(),
        Vec::new()
    );
}

#[test]
fn a_box_tilted_onto_a_corner_is_priced_across_the_corner_it_rests_on() {
    // The resting box is turned about its left corner until its right one is
    // 0.0019 off the floor. Both are points of the pair, but only the left one
    // touches, so the support is as wide as the corner's patch.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    let at = BodySpec {
        position: Vec2::Y * 0.5,
        can_sleep: false,
        ..Default::default()
    };
    add(
        &mut world,
        &mut host,
        2,
        at,
        &[cuboid(2, 0, 1.0, 0.5)],
        HULL,
        Some(0.2),
    );
    for _ in 0..10 {
        world.step(DT);
        world.charge_contact_supports(&mut host).unwrap();
    }
    let angle = (0.0019_f32 / 2.0).asin();
    let (sin, cos) = angle.sin_cos();
    let center = Vec2::new(-1.0 + cos - 0.5 * sin, sin + 0.5 * cos);
    assert!(world.set_pose(body(2), center, angle, true));
    assert!(world.set_velocity(body(2), Vec2::ZERO, 0.0, true));
    world.step(DT);
    let records = &world.loading_batches()[0].contacts;
    let mut gaps: Vec<_> = records.iter().map(|r| r.separation).collect();
    gaps.sort_by(f32::total_cmp);
    assert!(
        gaps.len() == 2 && gaps[0] < 1.0e-4 && gaps[1] > 1.0e-3,
        "{records:?}"
    );
    let charges = world.charge_contact_supports(&mut host).unwrap();
    let (_, width, ..) = supported(&charges[0]);
    let corner = engine_material::patch_width_from_radii(0.2, None).unwrap();
    assert!(close(width, corner, 1.0e-5), "{width} vs {corner}");
}

#[test]
fn a_support_across_a_gap_carries_its_whole_weight_across_what_it_rests_on() {
    // The floor is two colliders on one source, with a gap 1 wide under the
    // box's middle. Both carry the box, so its support takes their loads
    // together, across the 1 it rests on.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    let ground = BodySpec {
        kind: BodyKind::Fixed,
        position: -Vec2::Y * 0.5,
        ..Default::default()
    };
    let side = |part, x| {
        let mut spec = cuboid(1, part, 4.75, 0.5);
        spec.local_position.x = x;
        spec
    };
    let sides = [side(0, -5.25), side(1, 5.25)];
    add(&mut world, &mut host, 1, ground, &sides, ROCK, None);
    resting_box(&mut world, &mut host, 2, 1.0, false);
    let weight = GRAVITY * world.body_mass(body(2)).unwrap();
    for _ in 0..10 {
        world.step(DT);
        let charges = world.charge_contact_supports(&mut host).unwrap();
        let (force, width, ..) = supported(&charges[0]);
        assert!(close(force, weight, 1.0e-4), "{force} vs {weight}");
        assert!(close(width, 1.0, 1.0e-5), "{width}");
    }
}

#[test]
fn a_support_whose_body_is_removed_after_the_step_is_not_priced() {
    // Rapier keeps the removed body's contacts until the next step.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    let kept = resting_box(&mut world, &mut host, 2, 1.0, false);
    let beside = BodySpec {
        position: Vec2::new(5.0, 0.5),
        can_sleep: false,
        ..Default::default()
    };
    add(
        &mut world,
        &mut host,
        3,
        beside,
        &[cuboid(3, 0, 1.0, 0.5)],
        HULL,
        None,
    );
    for _ in 0..10 {
        world.step(DT);
        assert_eq!(world.charge_contact_supports(&mut host).unwrap().len(), 2);
    }
    world.step(DT);
    assert!(world.remove_entity(PhysicsId::new(3)));
    let charges = world.charge_contact_supports(&mut host).unwrap();
    assert_eq!(charges.len(), 1, "{charges:?}");
    let SupportCharge::Charged { sources, .. } = charges[0] else {
        panic!("{charges:?}");
    };
    assert!(sources.contains(&kept));
}

#[test]
fn a_step_with_a_refused_batch_prices_no_supports() {
    // Capture admits 3 points a batch. A second box falling beside the first
    // brings its count to 4, and capture refuses the batches from then on. The
    // first box's episode stays open through them, but its support isn't
    // priced.
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: -Vec2::Y * GRAVITY,
        collect_events: false,
        ..Default::default()
    });
    let config = CaptureConfig {
        max_contacts_per_batch: 3,
        ..Default::default()
    };
    world.enable_loading_capture(config).unwrap();
    world.enable_speculative_arrivals().unwrap();
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    resting_box(&mut world, &mut host, 2, 1.0, false);
    let falling = BodySpec {
        position: Vec2::new(5.0, 1.0),
        can_sleep: false,
        ..Default::default()
    };
    add(
        &mut world,
        &mut host,
        3,
        falling,
        &[cuboid(3, 0, 1.0, 0.5)],
        HULL,
        None,
    );
    let (mut refused, mut priced) = (0, 0);
    for _ in 0..60 {
        world.step(DT);
        let charges = world.charge_contact_supports(&mut host).unwrap();
        if world.loading_diagnostics().rejected_batches > 0 {
            refused += 1;
            assert_eq!(charges, Vec::new());
        } else {
            priced += 1;
            assert_eq!(charges.len(), 1, "{charges:?}");
        }
    }
    assert!(
        refused > 0 && priced > 0,
        "{refused} refused, {priced} priced"
    );
}

#[test]
fn a_support_whose_bodies_sleep_is_not_priced() {
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    resting_box(&mut world, &mut host, 2, 1.0, true);
    let handle = world.body_handle(body(2)).unwrap();
    let [mut awake, mut asleep] = [0, 0];
    for _ in 0..600 {
        // The solve acts on the box if it is awake when the step begins.
        let sleeping = world.raw.bodies[handle].is_sleeping();
        world.step(DT);
        let charges = world.charge_contact_supports(&mut host).unwrap();
        if sleeping {
            assert!(charges.is_empty(), "{charges:?}");
            asleep += 1;
        } else {
            assert_eq!(charges.len(), 1, "{charges:?}");
            awake += 1;
        }
    }
    assert!(awake > 0 && asleep > 0, "{awake} awake, {asleep} asleep");
}

#[test]
fn a_support_on_a_body_frozen_asleep_is_priced() {
    // A plank falls asleep on the floor, and the host freezes it as fixed
    // without waking it, so it still sleeps. Rapier solves a pair whose
    // dynamic bodies are awake, so a box set on the plank's top is priced at
    // its weight.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    let plank = BodySpec {
        position: Vec2::Y * 0.25,
        can_sleep: true,
        ..Default::default()
    };
    add(
        &mut world,
        &mut host,
        3,
        plank,
        &[cuboid(3, 0, 4.0, 0.25)],
        ROCK,
        None,
    );
    let handle = world.body_handle(body(3)).unwrap();
    for _ in 0..600 {
        if world.raw.bodies[handle].is_sleeping() {
            break;
        }
        world.step(DT);
    }
    assert!(world.raw.bodies[handle].is_sleeping());
    assert!(world.set_body_kind(body(3), BodyKind::Fixed, false));
    let top = world.body_point_to_world(body(3), Vec2::Y * 0.25).unwrap();
    let on_top = BodySpec {
        position: top + Vec2::Y * 0.5,
        can_sleep: false,
        ..Default::default()
    };
    add(
        &mut world,
        &mut host,
        2,
        on_top,
        &[cuboid(2, 0, 1.0, 0.5)],
        HULL,
        None,
    );
    let weight = GRAVITY * world.body_mass(body(2)).unwrap();
    for _ in 0..5 {
        world.step(DT);
        let frozen = world.loading_batches()[0].contacts.iter().all(|r| {
            r.bodies
                .iter()
                .any(|b| b.kind == BodyKind::Fixed && b.sleeping)
        });
        assert!(frozen, "{:?}", world.loading_batches());
        let charges = world.charge_contact_supports(&mut host).unwrap();
        assert_eq!(charges.len(), 1, "{charges:?}");
        let (force, ..) = supported(&charges[0]);
        assert!(close(force, weight, 1.0e-4), "{force} vs {weight}");
    }
}

#[test]
fn a_support_is_priced_once_a_step_and_charged_only_where_the_host_allows() {
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    let ground = floor(&mut world, &mut host, SOFT_TISSUE, Depth::Patch(None));
    resting_box(&mut world, &mut host, 2, 25.0, false);
    host.protected.insert(ground);
    for _ in 0..20 {
        world.step(DT);
        let charges = world.charge_contact_supports(&mut host).unwrap();
        let SupportCharge::Charged {
            outcomes,
            protected,
            ..
        } = &charges[0]
        else {
            panic!("{charges:?}");
        };
        // The shielded tissue is priced as crushed, and keeps its capacity.
        assert_eq!(*protected, [true, false]);
        assert!(outcomes[0].consumed[0] > 0.0, "{outcomes:?}");
        assert_eq!(
            world.charge_contact_supports(&mut host).unwrap(),
            Vec::new()
        );
    }
    assert!(host.absorbed.is_empty(), "{:?}", host.absorbed);
}

#[test]
fn a_support_by_a_wall_is_priced_across_what_carries_it() {
    // The floor is one collider with a wall, so the wall's points share the
    // pair of what rests on the floor. They are speculative. A box sliding
    // toward the wall at 0.005 closes on it too slowly to reach it within a
    // substep. A ball rolling away from it at 0.15 holds its floor point
    // still, while its point by the wall moves at 0.21, faster than a
    // support's may move, but the solve doesn't act on it. The wall's points
    // carry nothing, so they neither widen the patch nor unsettle the support.
    let ball_patch = engine_material::patch_width_from_radii(0.5, None).unwrap();
    for (rolling, gap, speed, width) in [
        (false, 0.001, 0.005, 2.0),
        (true, 0.0002, -0.15, ball_patch),
    ] {
        let mut world = world(-Vec2::Y * GRAVITY);
        let mut host = Host::default();
        let ground = BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        };
        let child = |half_width, half_height, position| CompoundChild {
            shape: ColliderShape::Cuboid {
                half_width,
                half_height,
            },
            position,
            angle: 0.0,
        };
        let half_width = if rolling { 0.5 } else { 1.0 };
        let mut corner = cuboid(1, 0, 10.0, 0.5);
        corner.shape = ColliderShape::Compound {
            children: vec![
                child(10.0, 0.5, -Vec2::Y * 0.5),
                child(0.5, 1.0, Vec2::new(half_width + 0.5 + gap, 1.0)),
            ],
        };
        add(&mut world, &mut host, 1, ground, &[corner], ROCK, None);
        let moving = BodySpec {
            position: Vec2::Y * 0.5,
            linear_velocity: Vec2::X * speed,
            angular_velocity: if rolling { -speed / 0.5 } else { 0.0 },
            can_sleep: false,
            ..Default::default()
        };
        let (shape, radius) = if rolling {
            (ball(2, 0.5, 1.0), Some(0.5))
        } else {
            (cuboid(2, 0, 1.0, 0.5), None)
        };
        add(&mut world, &mut host, 2, moving, &[shape], HULL, radius);
        let weight = GRAVITY * world.body_mass(body(2)).unwrap();
        let mut beside = 0;
        for _ in 0..5 {
            world.step(DT);
            let batch = &world.loading_batches()[0];
            let wall: Vec<_> = batch
                .contacts
                .iter()
                .filter(|r| r.normal.x.abs() > 0.9)
                .collect();
            beside += usize::from(!wall.is_empty());
            for record in wall {
                let moving = record.closing_speed.hypot(record.tangent_speed);
                assert!(
                    close(
                        moving,
                        speed.abs() * if rolling { 2.0_f32.sqrt() } else { 1.0 },
                        1.0e-2
                    ),
                    "{record:?}"
                );
                assert!(record.separation > record.closing_speed * DT, "{record:?}");
            }
            let charges = world.charge_contact_supports(&mut host).unwrap();
            let (force, priced, ..) = supported(&charges[0]);
            assert!(close(force, weight, 1.0e-3), "{force} vs {weight}");
            assert!(close(priced, width, 1.0e-5), "{priced} vs {width}");
        }
        assert!(beside > 0, "rolling {rolling}");
    }
}

#[test]
fn a_support_sliding_faster_than_its_points_may_move_is_not_priced() {
    // A box sliding along the floor at 0.05 rests on it, priced at its
    // weight. At 0.5 it grinds along the floor, its load moving across both
    // surfaces, and is not priced.
    for (sliding, steady) in [(0.05, true), (0.5, false)] {
        let mut world = world(-Vec2::Y * GRAVITY);
        let mut host = Host::default();
        floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
        resting_box(&mut world, &mut host, 2, 1.0, false);
        let weight = GRAVITY * world.body_mass(body(2)).unwrap();
        for _ in 0..10 {
            world.step(DT);
            world.charge_contact_supports(&mut host).unwrap();
        }
        assert!(world.set_velocity(body(2), Vec2::X * sliding, 0.0, true));
        for _ in 0..10 {
            world.step(DT);
            let slides = world.loading_batches()[0]
                .contacts
                .iter()
                .all(|r| close(r.tangent_speed.abs(), sliding, 1.0e-3));
            assert!(slides, "{:?}", world.loading_batches());
            let charges = world.charge_contact_supports(&mut host).unwrap();
            if steady {
                let (force, ..) = supported(&charges[0]);
                assert!(close(force, weight, 1.0e-4), "{force} vs {weight}");
            } else {
                assert_eq!(charges, Vec::new());
            }
        }
    }
}

#[test]
fn a_wheel_is_priced_across_the_patch_its_radius_gives() {
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    let at = BodySpec {
        position: Vec2::Y * 0.5,
        can_sleep: false,
        ..Default::default()
    };
    add(
        &mut world,
        &mut host,
        2,
        at,
        &[ball(2, 0.5, 1.0)],
        HULL,
        Some(0.5),
    );
    let weight = GRAVITY * world.body_mass(body(2)).unwrap();
    let mut charges = Vec::new();
    for _ in 0..10 {
        world.step(DT);
        charges = world.charge_contact_supports(&mut host).unwrap();
    }
    let (force, width, ..) = supported(&charges[0]);
    assert!(close(force, weight, 1.0e-3), "{force} vs {weight}");
    assert_eq!(
        width,
        engine_material::patch_width_from_radii(0.5, None).unwrap()
    );
}

#[test]
fn supports_the_host_cannot_price_are_reported_with_their_force() {
    // An undeclared box, and a ball declared flat on the flat floor, which
    // meets it at one point.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    let block = resting_box(&mut world, &mut host, 2, 1.0, false);
    host.surfaces.remove(&block);
    let at = BodySpec {
        position: Vec2::new(5.0, 0.5),
        can_sleep: false,
        ..Default::default()
    };
    add(
        &mut world,
        &mut host,
        3,
        at,
        &[ball(3, 0.5, 1.0)],
        HULL,
        None,
    );
    let weights = [2, 3].map(|n| GRAVITY * world.body_mass(body(n)).unwrap());
    let mut charges = Vec::new();
    for _ in 0..10 {
        world.step(DT);
        charges = world.charge_contact_supports(&mut host).unwrap();
    }
    let unpriced: Vec<_> = charges
        .iter()
        .map(|charge| match charge {
            SupportCharge::Unpriced { force, reason, .. } => (*force, *reason),
            other => panic!("{other:?}"),
        })
        .collect();
    let reasons = [
        Unpriced::Undeclared,
        Unpriced::Material(MaterialError::InvalidGeometry),
    ];
    assert_eq!(unpriced.len(), 2, "{charges:?}");
    for ((force, reason), (weight, expected)) in
        unpriced.into_iter().zip(weights.into_iter().zip(reasons))
    {
        assert!(close(force, weight, 1.0e-3), "{force} vs {weight}");
        assert_eq!(reason, expected);
    }
    assert!(host.absorbed.is_empty(), "{:?}", host.absorbed);
}

#[test]
fn supports_need_arrivals_and_a_step_since_the_world_was_restored() {
    let mut host = Host::default();
    let mut bare = PhysicsWorld::new(PhysicsWorldConfig::default());
    assert_eq!(
        bare.charge_contact_supports(&mut host),
        Err(LoadingError::Disabled)
    );
    let mut touching = PhysicsWorld::new(PhysicsWorldConfig::default());
    touching
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    assert_eq!(
        touching.charge_contact_supports(&mut host),
        Err(LoadingError::ArrivalsDisabled)
    );
    // A restored world keeps the solver's impulses, but no step has run.
    let mut world = world(-Vec2::Y * GRAVITY);
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    resting_box(&mut world, &mut host, 2, 1.0, false);
    world.step(DT);
    let mut restored = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    assert_eq!(restored.charge_contact_supports(&mut host), Ok(Vec::new()));
    assert_eq!(world.charge_contact_supports(&mut host).unwrap().len(), 1);
}
