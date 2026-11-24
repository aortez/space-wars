//! Charges that don't change with how bodies are cut into colliders, the order
//! they are added in, pauses or replays, and the state charging keeps: retired
//! regions, a busy scene's episodes, and the step rate's effect on a drop.

use super::*;

/// The first ball's entity in the busy scene.
const FIRST_BALL: u64 = 10;
/// Rapier's prediction distance at a length unit of 1.
const PREDICTION: f32 = 0.002;

/// A world whose capture admits speculative arrivals and keeps at most
/// `max_episodes`, in which CCD splits a step into at most `substeps`.
fn capped(gravity: Vec2, max_episodes: usize, substeps: usize) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity,
        collect_events: false,
        max_ccd_substeps: substeps,
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig {
            max_episodes,
            ..CaptureConfig::default()
        })
        .unwrap();
    world.enable_speculative_arrivals().unwrap();
    world
}

/// Body `n`'s colliders: `parts` equal boxes of `density` side by side,
/// together `half_width` by `half_height` about its center, listed backward if
/// `reverse`.
fn pieces(
    n: u64,
    parts: u16,
    half_width: f32,
    half_height: f32,
    density: f32,
    reverse: bool,
) -> Vec<ColliderSpec> {
    let width = half_width / f32::from(parts);
    let mut pieces: Vec<_> = (0..parts)
        .map(|part| {
            let mut piece = cuboid(n, part, width, half_height);
            piece.local_position.x = (2.0 * f32::from(part) + 1.0) * width - half_width;
            piece.density = density;
            piece
        })
        .collect();
    if reverse {
        pieces.reverse();
    }
    pieces
}

/// A hull box 2 wide of `box_parts` pieces, weighing 500, crushing soft tissue
/// 8 wide of `floor_parts` pieces for a second. If `reverse`, the box is added
/// first and each body's pieces are listed backward. If `swap`, the bodies are
/// numbered so that the box's source sorts first. Returns the work the tissue
/// lost and the impulse its support carried.
fn crush_in_pieces(box_parts: u16, floor_parts: u16, reverse: bool, swap: bool) -> (f64, f64) {
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    let (tissue_n, box_n) = if swap { (7, 3) } else { (1, 2) };
    let ground = BodySpec {
        kind: BodyKind::Fixed,
        position: -Vec2::Y * 0.5,
        ..Default::default()
    };
    let at = BodySpec {
        position: Vec2::Y * 0.5,
        can_sleep: false,
        ..Default::default()
    };
    let order = if reverse {
        [box_n, tissue_n]
    } else {
        [tissue_n, box_n]
    };
    let mut tissue = None;
    for n in order {
        if n == box_n {
            let colliders = pieces(n, box_parts, 1.0, 0.5, 25.0, reverse);
            add(&mut world, &mut host, n, at, &colliders, HULL, None);
            continue;
        }
        let colliders = pieces(n, floor_parts, 4.0, 0.5, 1.0, reverse);
        let source = add(
            &mut world,
            &mut host,
            n,
            ground,
            &colliders,
            SOFT_TISSUE,
            None,
        );
        let surface = host.surfaces.get_mut(&source).unwrap();
        surface.region = Region::new(SOFT_TISSUE, 100.0).unwrap();
        surface.depth = Depth::Patch(None);
        tissue = Some(source);
    }
    let tissue = tissue.unwrap();
    let fresh = host.surfaces[&tissue].region;
    let mut impulse = 0.0;
    for _ in 0..120 {
        world.step(DT);
        let (impacts, supports) = charges(&mut world, &mut host);
        assert!(impacts.is_empty(), "{impacts:?}");
        let [support] = supports.as_slice() else {
            panic!("{supports:?}");
        };
        let (force, _, duration, _) = supported(support);
        impulse += f64::from(force) * f64::from(duration);
    }
    let region = host.surfaces[&tissue].region;
    assert!(!region.failed());
    (
        fresh.remaining_precise() - region.remaining_precise(),
        impulse,
    )
}

/// A box crushing soft tissue for a second is charged the same work, and its
/// support carries the box's weight for the second, whether the box and the
/// tissue are one collider or several, whichever is added first and whichever
/// source sorts first. Each partition's seams lie apart from the other side's.
/// Where seams meet and the box drifts across them, the support also counts
/// the solve's sideways pushes at the corners there, as Limits says.
#[test]
fn support_work_does_not_depend_on_partitions_or_insertion_order() {
    let (work, impulse) = crush_in_pieces(1, 1, false, false);
    assert!(work > 0.0);
    assert!((impulse - 500.0).abs() <= 1.0e-6 * 500.0, "{impulse}");
    for box_parts in [1, 2, 4] {
        for floor_parts in [1, 5] {
            for (reverse, swap) in [(false, false), (true, false), (false, true), (true, true)] {
                let label = format!(
                    "box {box_parts}, tissue {floor_parts}, reverse {reverse}, swap {swap}"
                );
                let (w, i) = crush_in_pieces(box_parts, floor_parts, reverse, swap);
                assert!((w - work).abs() <= 1.0e-4 * work, "{label}: {w} vs {work}");
                assert!(
                    (i - impulse).abs() <= 1.0e-6 * impulse,
                    "{label}: {i} vs {impulse}"
                );
            }
        }
    }
}

/// A ball dropped onto rock from several heights, at several step rates, is
/// priced within a step of gravity of what its free fall gives. Without CCD,
/// capture first sees the ball the step after a step carries it into the
/// rock, or to within Rapier's prediction distance of it, at the speed it
/// ended that step with. The heights 0.5 to 3.7 carry it into the rock. At
/// each rate one more, `near`, ends a quarter second's steps a thousandth
/// above it.
#[test]
fn a_drop_is_priced_within_a_step_of_gravity_of_its_free_fall() {
    for rate in [30.0, 60.0, 120.0, 240.0] {
        let dt = 1.0 / rate;
        let mut free = world(-Vec2::Y * GRAVITY);
        let up = BodySpec {
            position: Vec2::Y * 1.5,
            can_sleep: false,
            ..Default::default()
        };
        let mut host = Host::default();
        add(
            &mut free,
            &mut host,
            2,
            up,
            &[ball(2, 0.5, 1.0)],
            HULL,
            None,
        );
        for _ in 0..(rate / 4.0) as usize {
            free.step(dt);
        }
        let near = 1.5 - free.motion(body(2)).unwrap().position.y + 0.001;
        for height in [0.5, 1.3, 2.0, 3.7, near] {
            let mut world = world(-Vec2::Y * GRAVITY);
            let mut host = Host::default();
            floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
            let above = BodySpec {
                position: Vec2::Y * (0.5 + height),
                can_sleep: false,
                ..Default::default()
            };
            add(
                &mut world,
                &mut host,
                2,
                above,
                &[ball(2, 0.5, 1.0)],
                HULL,
                Some(0.5),
            );
            let mut falling = 0.0;
            let charges = (0..240)
                .find_map(|_| {
                    falling = -world.motion(body(2)).unwrap().linear_velocity.y;
                    world.step(dt);
                    let charges = world.charge_contact_impacts(&mut host).unwrap();
                    (!charges.is_empty()).then_some(charges)
                })
                .unwrap();
            let label = format!("{height} at {rate} Hz");
            let [charge] = charges.as_slice() else {
                panic!("{label}: {charges:?}");
            };
            let (speed, _) = charged(charge);
            assert!(
                close(speed, falling, 1.0e-6),
                "{label}: {speed} vs {falling}"
            );
            let free = (2.0 * GRAVITY * height).sqrt();
            assert!(
                (speed - free).abs() <= GRAVITY * dt,
                "{label}: {speed} vs {free}"
            );
            let separation = world.loading_batches()[0]
                .contacts
                .iter()
                .map(|record| record.separation)
                .fold(f32::INFINITY, f32::min);
            assert_eq!(separation > 0.0, height == near, "{label}: {separation}");
            assert!(separation < PREDICTION, "{label}: {separation}");
        }
    }
}

/// Four hull balls dropped onto rock, and a hull box resting on it.
fn bouncing() -> (PhysicsWorld, Host) {
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    for n in 2..6_u16 {
        let mut shape = ball(n.into(), 0.5, 1.0);
        shape.restitution = 0.4;
        let at = f32::from(n);
        let thrown = BodySpec {
            position: Vec2::new(1.3 * at - 5.0, 1.0 + 0.7 * at),
            linear_velocity: Vec2::X * (0.3 * at - 1.0),
            can_sleep: false,
            ..Default::default()
        };
        add(
            &mut world,
            &mut host,
            n.into(),
            thrown,
            &[shape],
            HULL,
            Some(0.5),
        );
    }
    let resting = BodySpec {
        position: Vec2::new(8.0, 0.5),
        can_sleep: false,
        ..Default::default()
    };
    add(
        &mut world,
        &mut host,
        6,
        resting,
        &[cuboid(6, 0, 1.0, 0.5)],
        HULL,
        None,
    );
    (world, host)
}

/// The bouncing scene, stepped as it is and again with a pause after every
/// step: steps of zero, of a negative length, of NaN and of infinity. The
/// paused steps price nothing, and the run prices the same charges and leaves
/// the same surfaces.
#[test]
fn pauses_and_steps_of_no_length_price_nothing_and_change_nothing() {
    let run = |pause: bool| {
        let (mut world, mut host) = bouncing();
        let mut log = Vec::new();
        for _ in 0..240 {
            world.step(DT);
            log.push(charges(&mut world, &mut host));
            if !pause {
                continue;
            }
            for dt in [0.0, -1.0, f32::NAN, f32::INFINITY, 0.0] {
                world.step(dt);
                assert_eq!(charges(&mut world, &mut host), (vec![], vec![]), "{dt}");
            }
        }
        (log, host)
    };
    let (steady, steady_host) = run(false);
    let (paused, paused_host) = run(true);
    assert!(steady.iter().any(|(impacts, _)| !impacts.is_empty()));
    assert!(steady.iter().any(|(_, supports)| !supports.is_empty()));
    assert_eq!(paused, steady);
    assert_eq!(paused_host.absorbed, steady_host.absorbed);
    assert_eq!(paused_host.surfaces, steady_host.surfaces);
}

/// Ball 1 strikes ball 2. Before the step is priced, the host removes ball 2
/// and puts a new actor half a unit beyond it under the same IDs, keeping the
/// old surface. The strike is reported as retired with its energy, and
/// nothing is charged: the old token is dead, and the new actor has a new
/// one. Ball 1 goes on at the balls' common velocity and strikes the new
/// actor, a new impact on the new token.
#[test]
fn an_impact_on_a_retired_region_never_reaches_an_actor_reusing_its_ids() {
    let mut host = Host::default();
    let (mut world, [striker, struck]) = balls(&mut host, [Some(0.5); 2]);
    let [a, b] = [1, 2].map(|n| world.body_mass(body(n)).unwrap());
    world.step(DT);
    assert!(world.remove_entity(PhysicsId::new(2)));
    let beyond = BodySpec {
        position: Vec2::X * 0.5,
        can_sleep: false,
        ..Default::default()
    };
    let actor = add(
        &mut world,
        &mut host,
        2,
        beyond,
        &[ball(2, 0.5, 3.0)],
        ROCK,
        Some(0.5),
    );
    assert_eq!(actor.region.id, struck.region.id);
    assert!(!world.loading_region_is_live(struck.region));
    assert!(world.loading_region_is_live(actor.region));
    let (impacts, supports) = charges(&mut world, &mut host);
    let [
        ContactCharge::Unpriced {
            episode: stale,
            sources,
            energy: Some(energy),
            reason: Unpriced::Retired,
            ..
        },
    ] = impacts.as_slice()
    else {
        panic!("{impacts:?}");
    };
    assert_eq!(*sources, [striker, struck]);
    let brought = 0.5 * a * b / (a + b) * 20.0 * 20.0;
    assert!(close(*energy, brought, 1.0e-6), "{energy} vs {brought}");
    assert!(supports.is_empty(), "{supports:?}");
    assert!(host.absorbed.is_empty());
    let next = first_charges(&mut world, &mut host);
    let [
        ContactCharge::Charged {
            episode,
            sources,
            closing_speed,
            ..
        },
    ] = next.as_slice()
    else {
        panic!("{next:?}");
    };
    assert_ne!(episode, stale);
    assert_eq!(*sources, [striker, actor]);
    let common = 20.0 * a / (a + b);
    assert!(close(*closing_speed, common, 1.0e-5), "{closing_speed}");
    assert!(host.absorbed.iter().all(|(source, _)| *source != struck));
}

/// Sixty hull balls thrown about between rock walls, with capture keeping at
/// most `max_episodes`.
fn busy(max_episodes: usize) -> (PhysicsWorld, Host) {
    let mut world = capped(-Vec2::Y * GRAVITY, max_episodes, 1);
    let mut host = Host::default();
    let walls = [
        (Vec2::new(0.0, -0.5), Vec2::new(10.0, 0.5)),
        (Vec2::new(-10.5, 10.0), Vec2::new(0.5, 10.0)),
        (Vec2::new(10.5, 10.0), Vec2::new(0.5, 10.0)),
    ];
    for (n, (position, half)) in (1..).zip(walls) {
        let wall = BodySpec {
            kind: BodyKind::Fixed,
            position,
            ..Default::default()
        };
        let shape = cuboid(n, 0, half.x, half.y);
        add(&mut world, &mut host, n, wall, &[shape], ROCK, None);
    }
    for k in 0..60 {
        let n = FIRST_BALL + k;
        let mut shape = ball(n, 0.4, 1.0);
        shape.restitution = 0.5;
        let (column, row) = ((k % 10) as f32, (k / 10) as f32);
        let thrown = BodySpec {
            position: Vec2::new(1.7 * column - 8.0, 1.5 * row + 2.0),
            linear_velocity: Vec2::new(((k * 7) % 11) as f32 - 5.0, ((k * 3) % 5) as f32),
            can_sleep: false,
            ..Default::default()
        };
        add(&mut world, &mut host, n, thrown, &[shape], HULL, Some(0.4));
    }
    (world, host)
}

/// The busy scene for ten seconds. Capture keeps an episode for each pair in
/// contact, at most as many as it has room for, and each step reports each
/// impact of the batches capture admitted, once. With the default room it
/// refuses nothing. With room for 48 episodes it fills that room and refuses
/// the batches that would take more, and a step with a refused batch prices
/// no supports. Removing the balls retires every episode they kept.
#[test]
fn a_busy_scene_keeps_bounded_state_and_reports_each_admitted_impact() {
    let room = CaptureConfig::default().max_episodes;
    for max_episodes in [room, 48] {
        let (mut world, mut host) = busy(max_episodes);
        let mut most = 0;
        let mut refused = 0;
        let mut priced = 0;
        for _ in 0..1200 {
            world.step(DT);
            let rejected = world.loading_diagnostics().rejected_batches;
            let (impacts, supports) = charges(&mut world, &mut host);
            let episodes = world.loading.as_ref().unwrap().episodes.len();
            assert!(episodes <= max_episodes);
            most = most.max(episodes);
            let admitted: Vec<_> = world
                .loading_batches()
                .iter()
                .flat_map(|batch| &batch.events)
                .filter(|event| event.transition == EpisodeTransition::Impact)
                .map(|event| (event.id, event.sources))
                .collect();
            let reported: Vec<_> = impacts
                .iter()
                .map(|charge| match charge {
                    ContactCharge::Charged {
                        episode, sources, ..
                    }
                    | ContactCharge::Unpriced {
                        episode, sources, ..
                    } => (*episode, *sources),
                })
                .collect();
            assert_eq!(reported, admitted);
            if rejected > 0 {
                refused += 1;
                assert!(supports.is_empty(), "{supports:?}");
            }
            priced += impacts
                .iter()
                .filter(|charge| matches!(charge, ContactCharge::Charged { .. }))
                .count();
        }
        assert!(priced > 0);
        if max_episodes == room {
            assert_eq!(refused, 0);
        } else {
            assert_eq!(most, max_episodes);
            assert!(refused > 0);
        }
        for k in 0..60 {
            assert!(world.remove_entity(PhysicsId::new(FIRST_BALL + k)));
        }
        assert_eq!(world.loading.as_ref().unwrap().episodes.len(), 0);
    }
}

/// Ball 1 strikes ball 2 in a step that a CCD bullet splits where it strikes a
/// wall, with capture's room for one episode or for two. With room for one,
/// the balls' strike takes it, and capture refuses the batch in which the
/// bullet strikes. That batch is missing from the charges, and the rest of the
/// step is priced: the balls' strike, on its own energy. With room for two,
/// both strikes are priced.
#[test]
fn a_refused_batch_is_missing_from_its_step_and_the_rest_is_priced() {
    for room in [1, 2] {
        let mut world = capped(Vec2::ZERO, room, 4);
        let mut host = Host::default();
        // Ball 1 closes on ball 2 in the second step, as the bullet does on
        // its wall.
        let moving = BodySpec {
            position: -Vec2::X * (1.001 + 20.0 * DT),
            linear_velocity: Vec2::X * 20.0,
            can_sleep: false,
            ..Default::default()
        };
        let resting = BodySpec {
            can_sleep: false,
            ..Default::default()
        };
        let balls = [
            add(
                &mut world,
                &mut host,
                1,
                moving,
                &[ball(1, 0.5, 1.0)],
                HULL,
                Some(0.5),
            ),
            add(
                &mut world,
                &mut host,
                2,
                resting,
                &[ball(2, 0.5, 3.0)],
                ROCK,
                Some(0.5),
            ),
        ];
        let fixed = BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(26.0, 5.0),
            ..Default::default()
        };
        let wall = add(
            &mut world,
            &mut host,
            3,
            fixed,
            &[cuboid(3, 0, 0.5, 3.0)],
            ROCK,
            None,
        );
        let shot = BodySpec {
            position: Vec2::new(20.0, 5.0),
            linear_velocity: Vec2::X * 400.0,
            ccd_enabled: true,
            can_sleep: false,
            ..Default::default()
        };
        let bullet = add(
            &mut world,
            &mut host,
            4,
            shot,
            &[ball(4, 0.1, 1.0)],
            HULL,
            Some(0.1),
        );
        let [a, b] = [1, 2].map(|n| world.body_mass(body(n)).unwrap());
        world.step(DT);
        assert_eq!(charges(&mut world, &mut host), (vec![], vec![]));
        world.step(DT);
        let diagnostics = world.loading_diagnostics();
        let label = format!("room {room}: {diagnostics:?}");
        assert!(diagnostics.batches_seen > 1, "{label}");
        assert_eq!(diagnostics.rejected_batches > 0, room == 1, "{label}");
        if room == 1 {
            let limit = Some(CaptureRejection::EpisodeLimit);
            assert_eq!(diagnostics.last_rejection, limit, "{label}");
        }
        let (impacts, supports) = charges(&mut world, &mut host);
        assert!(supports.is_empty(), "{label}: {supports:?}");
        let struck: Vec<_> = impacts
            .iter()
            .map(|charge| match charge {
                ContactCharge::Charged { sources, .. } => *sources,
                other => panic!("{label}: {other:?}"),
            })
            .collect();
        let expected = if room == 1 {
            vec![balls]
        } else {
            vec![balls, [wall, bullet]]
        };
        assert_eq!(struck, expected, "{label}");
        let (speed, outcome) = charged(&impacts[0]);
        assert!(close(speed, 20.0, 1.0e-6), "{label}: {speed}");
        let brought = 0.5 * a * b / (a + b) * 20.0 * 20.0;
        let available = outcome.ledger.available;
        assert!(close(available, brought, 1.0e-6), "{label}: {available}");
    }
}

/// The bouncing scene is saved 100 steps in, with balls still falling and the
/// box resting, and the host's surfaces with it. Restored in the same build,
/// it prices every later step as the run did, and leaves the same surfaces.
#[test]
fn a_restored_snapshot_prices_the_rest_of_its_run_alike_in_the_same_build() {
    let (mut world, mut host) = bouncing();
    let mut log = Vec::new();
    let mut saved = None;
    for step in 0..240 {
        if step == 100 {
            saved = Some((world.snapshot_bytes().unwrap(), host.surfaces.clone()));
        }
        world.step(DT);
        log.push(charges(&mut world, &mut host));
    }
    let rest = &log[100..];
    assert!(rest.iter().any(|(impacts, _)| !impacts.is_empty()));
    assert!(rest.iter().any(|(_, supports)| !supports.is_empty()));
    let (bytes, surfaces) = saved.unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    let mut again = Host {
        surfaces,
        ..Default::default()
    };
    for (step, original) in (100..).zip(rest) {
        restored.step(DT);
        assert_eq!(&charges(&mut restored, &mut again), original, "step {step}");
    }
    assert_eq!(again.surfaces, host.surfaces);
}
