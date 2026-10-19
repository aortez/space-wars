use super::*;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario};
use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
use std::time::Duration;

fn contains(points: &[RenderPoint], p: Vec2) -> bool {
    let mut inside = false;
    for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}
fn sunlight(frame: &RenderFrame) -> Vec<&RenderPolygon> {
    frame
        .layers
        .iter()
        .flat_map(|l| &l.primitives)
        .filter_map(|p| match p {
            RenderPrimitive::Polygon(p)
                if p.fill
                    .is_some_and(|f| f.color == RenderColor::rgba(1.0, 0.87, 0.46, 0.12)) =>
            {
                Some(p)
            }
            _ => None,
        })
        .collect()
}

fn fresh_light(s: &ClockState, receivers: &[Receiver]) -> Vec<engine_core::lighting::Exposure> {
    let world = crate::grass::surfaces::world(&s.active_event, s.duck_visit.as_deref());
    let geometry = world.map(|w| w.query_snapshot());
    let transparent: Vec<_> = world
        .into_iter()
        .flat_map(|w| w.collider_ids())
        .filter(|id| id.entity.value() == crate::grass::SOIL_PHYSICS_ID)
        .collect();
    let mut quads = plant_face_polygons(s);
    quads.extend(plant_actor_polygons(s));
    if let Some(soil) = &s.soil {
        quads.extend(soil.quads().into_iter().map(|q| q.to_vec()));
    }
    let mut polygons = crate::grass::residents::optical_polygons(quads);
    if let Some(soil) = &s.soil {
        polygons.extend(crate::grass::residents::tissue_polygons(
            &s.plants.plants.observe(),
            soil,
        ));
    }
    crate::grass::residents::query_light(
        s.simulation_tick(),
        50,
        s.ceiling.source(Layout::new(s.aspect_ratio())),
        &[],
        polygons,
        geometry.as_ref(),
        &transparent,
        receivers,
    )
    .unwrap()
}
fn duck_fixture() -> ClockState {
    let mut s = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            aspect_ratio: 16.0 / 9.0,
            ..Default::default()
        },
        4242,
    );
    ClockScenario::step(
        &mut s,
        &[
            ClockAction::set_reading(ClockReading::new(12, 34, 0).unwrap()),
            ClockAction::trigger_event(ClockEventKind::Grass),
        ],
        Duration::ZERO,
    );
    for _ in 0..180 * 60 {
        ClockScenario::step(&mut s, &[], Duration::from_nanos(16_666_667));
    }
    ClockScenario::step(
        &mut s,
        &[ClockAction::preview_event(ClockEventKind::Duck)],
        Duration::ZERO,
    );
    s
}

#[test]
fn ceiling_light_contract_aligned_duck_pose_draws_light_to_lit_tissue() {
    let mut s = duck_fixture();
    let previous_tick = s.plants.plants.observe().tick;
    let mut steps = 0;
    while s.plants.plants.observe().tick == previous_tick {
        ClockScenario::step(&mut s, &[], Duration::from_nanos(16_666_667));
        steps += 1;
        assert!(steps <= 60);
    }
    assert_eq!(steps, 59, "operator's aligned biology boundary");
    let o = s.plants.plants.observe();
    let t = o.tissues.iter().find(|t| t.id == 27).unwrap();
    let tip = crate::grass::residents::shoot_polygon(t, &o, s.soil.as_ref().unwrap()).unwrap()[2];
    let exposure = s.plants.exposure[&27].light;
    let frame = ClockScenario::render_frame(&s);
    let beam = sunlight(&frame);
    let lit = beam
        .iter()
        .any(|p| contains(&p.points, tip + Vec2::Y * 0.02));
    println!(
        "aligned-duck steps={steps} biology_tick={} tissue=27 tip={tip:?} cached_light={exposure} rendered_lit={lit}",
        o.tick
    );
    assert_eq!(exposure, 1000);
    let fresh = fresh_light(
        &s,
        &[Receiver {
            position: tip,
            surface: Some(engine_core::lighting::LightId(
                crate::grass::SOIL_PHYSICS_ID,
                27,
            )),
        }],
    );
    println!(
        "aligned-duck fresh_light={} blocked={:?}",
        fresh[0].intensity, fresh[0].first_blocker
    );
    assert_eq!(
        fresh[0].intensity, 1000.0,
        "same-pose query rules out cached biology"
    );
    let world = crate::grass::surfaces::world(&s.active_event, s.duck_visit.as_deref()).unwrap();
    let transparent: Vec<_> = world
        .collider_ids()
        .into_iter()
        .filter(|id| id.entity.value() == crate::grass::SOIL_PHYSICS_ID)
        .collect();
    let source = s.ceiling.source(Layout::new(s.aspect_ratio())).unwrap();
    let dx = (source.right - source.left) / (source.right - source.left).ceil();
    let left = source.left + ((tip.x - source.left) / dx).floor() * dx;
    let right = left + dx;
    let broad_top = world
        .query_snapshot()
        .lighting_occluder_bounds(&transparent)
        .unwrap()
        .into_iter()
        .filter(|(a, b)| a.x <= right && left <= b.x)
        .map(|(_, b)| b.y)
        .max_by(f32::total_cmp)
        .unwrap();
    println!(
        "aligned-duck old_aabb_top={broad_top} false_gap={}",
        broad_top - tip.y
    );
    assert!(
        broad_top - tip.y > 30.0,
        "reproduce the original AABB cause"
    );
    assert!(lit, "fresh biology receiver must lie in delivered sunlight");
}

#[test]
fn ceiling_light_contract_duck_motion_beam_matches_current_exposure() {
    let mut s = duck_fixture();
    let mut blocked = 0;
    let mut clear = 0;
    let mut previous = 0;
    for step in [0, 30, 59, 90, 120, 240, 480, 900] {
        for _ in previous..step {
            ClockScenario::step(&mut s, &[], Duration::from_nanos(16_666_667));
        }
        previous = step;
        let layout = Layout::new(s.aspect_ratio());
        let source = s.ceiling.source(layout).unwrap();
        let frame = ClockScenario::render_frame(&s);
        let beam = sunlight(&frame);
        assert!(!beam.is_empty(), "no silent refusal at moving pose {step}");
        let points: Vec<_> = (0..32)
            .flat_map(|x| {
                (0..16).map(move |y| {
                    Vec2::new(
                        source.left + (x as f32 + 0.173) / 32.0 * (source.right - source.left),
                        layout.floor_y + 3.137 + y as f32 * 5.31,
                    )
                })
            })
            .collect();
        for batch in points.chunks(64) {
            let receivers: Vec<_> = batch
                .iter()
                .map(|p| Receiver {
                    position: *p,
                    surface: None,
                })
                .collect();
            let samples = fresh_light(&s, &receivers);
            for (&p, sample) in batch.iter().zip(samples) {
                let drawn = beam.iter().any(|q| contains(&q.points, p));
                if sample.local == 0.0 {
                    blocked += 1;
                    assert!(
                        !drawn,
                        "beam crosses opaque geometry step={step} p={p:?} blocker={:?}",
                        sample.first_blocker
                    );
                } else {
                    clear += 1;
                    // Circumscribed curves may shade within 0.005 units of their
                    // boundary; allow a small spatial edge tolerance, never a
                    // whole AABB or the original 34-unit dark gap.
                    assert!(
                        drawn
                            || [Vec2::new(0.05, 0.05), Vec2::new(-0.05, 0.05)]
                                .iter()
                                .any(|d| beam.iter().any(|q| contains(&q.points, p + *d))),
                        "unblocked receiver missing beam step={step} p={p:?}"
                    );
                }
            }
        }
    }
    assert!(blocked > 100 && clear > 100);
    println!("duck-render-agreement blocked={blocked} clear={clear} poses=8 receivers=4096");
}

#[test]
fn ceiling_light_contract_slopes_subpixel_blockers_crossings_and_refusal() {
    use engine_core::lighting::{ApertureSource, LightId};
    let source = ApertureSource {
        id: LightId(99, 1),
        left: -4.0,
        right: 4.0,
        ceiling_y: 10.0,
        strength: 950.0,
    };
    for shift in [-5.0, -0.37, 0.0, 0.71, 5.0] {
        let shapes = vec![
            vec![
                Vec2::new(-2.0 + shift, 2.0),
                Vec2::new(2.0 + shift, 8.0),
                Vec2::new(2.0 + shift, 1.0),
            ],
            vec![
                Vec2::new(-1.0, 7.0),
                Vec2::new(1.0, 2.0),
                Vec2::new(1.0, 1.0),
            ],
            // Narrower than the former center-ray spacing.
            vec![
                Vec2::new(0.121, 8.9),
                Vec2::new(0.141, 8.7),
                Vec2::new(0.131, 8.0),
            ],
            // Above-ceiling and crossing-ceiling geometry have distinct outcomes.
            vec![
                Vec2::new(-3.5, 11.0),
                Vec2::new(-2.5, 12.0),
                Vec2::new(-3.0, 12.5),
            ],
            vec![
                Vec2::new(3.0, 9.0),
                Vec2::new(3.5, 12.0),
                Vec2::new(3.7, 8.0),
            ],
        ];
        let beam = beam::polygons(source, 0.0, &shapes).unwrap();
        let polygons = crate::grass::residents::optical_polygons(shapes);
        let points: Vec<_> = (0..99)
            .flat_map(|x| {
                (0..39).map(move |y| Vec2::new(-3.971 + x as f32 * 0.08, 0.113 + y as f32 * 0.25))
            })
            .collect();
        for chunk in points.chunks(64) {
            let receivers: Vec<_> = chunk
                .iter()
                .map(|p| Receiver {
                    position: *p,
                    surface: None,
                })
                .collect();
            let samples = crate::grass::residents::query_light(
                0,
                0,
                Some(source),
                &[],
                polygons.clone(),
                None,
                &[],
                &receivers,
            )
            .unwrap();
            for (&p, e) in chunk.iter().zip(samples) {
                let drawn = beam.iter().any(|q| contains(q, p));
                if e.local == 0.0 {
                    assert!(!drawn, "opaque silhouette shift={shift} point={p:?}");
                } else {
                    // The shared ray adapter trims 0.001 at the receiver.
                    assert!(
                        drawn || beam.iter().any(|q| contains(q, p + Vec2::Y * 0.002)),
                        "clear silhouette shift={shift} point={p:?}"
                    );
                }
            }
        }
        assert!(
            !beam.iter().any(|q| contains(q, Vec2::new(0.131, 8.5))),
            "subpixel blocker cannot be skipped"
        );
    }
    let huge = vec![vec![Vec2::new(-1.0, 1.0); 32_769]];
    assert!(beam::polygons(source, 0.0, &huge).is_none());
    let invalid = vec![vec![Vec2::new(f32::NAN, 1.0), Vec2::ZERO]];
    assert!(beam::polygons(source, 0.0, &invalid).is_none());
}

#[test]
fn ceiling_light_contract_canonical_silhouettes_block_without_aabb_shadows() {
    use engine_core::lighting::{ApertureSource, LightId};
    use engine_rapier::world::*;
    let source = ApertureSource {
        id: LightId(99, 1),
        left: -4.0,
        right: 4.0,
        ceiling_y: 10.0,
        strength: 950.0,
    };
    let shapes = [
        ColliderShape::Cuboid {
            half_width: 1.7,
            half_height: 0.23,
        },
        ColliderShape::Ball { radius: 0.81 },
        ColliderShape::Capsule {
            half_segment: 1.5,
            radius: 0.3,
        },
        ColliderShape::ConvexPolygon {
            vertices: vec![
                Vec2::new(-1.3, -0.6),
                Vec2::new(1.7, -0.4),
                Vec2::new(0.2, 1.4),
            ],
        },
        ColliderShape::Polyline {
            vertices: vec![
                Vec2::new(-1.4, -0.7),
                Vec2::new(0.3, 1.2),
                Vec2::new(1.3, -0.9),
            ],
        },
        ColliderShape::Compound {
            children: vec![
                CompoundChild {
                    shape: ColliderShape::Cuboid {
                        half_width: 1.4,
                        half_height: 0.2,
                    },
                    position: Vec2::new(0.3, 0.1),
                    angle: 0.7,
                },
                CompoundChild {
                    shape: ColliderShape::Ball { radius: 0.41 },
                    position: Vec2::new(-1.0, -1.0),
                    angle: 0.0,
                },
            ],
        },
    ];
    let mut blocked = 0;
    let mut clear = 0;
    for (kind, shape) in shapes.into_iter().enumerate() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let entity = PhysicsId::new(51);
        let body = BodyId::new(entity, BodyRole::PRIMARY);
        let id = ColliderId::new(entity, ColliderRole::PRIMARY, 0);
        let mut collider = ColliderSpec::ball(id, 1.0);
        collider.shape = shape;
        collider.local_position = Vec2::new(0.19, 0.31);
        collider.local_angle = 0.23;
        assert!(world.insert_body(
            body,
            BodySpec {
                kind: BodyKind::Fixed,
                ..Default::default()
            },
            &[collider]
        ));
        for (pose, (x, y, angle)) in [
            (-0.41, 4.7, 0.0),
            (0.31, 3.1, 0.6),
            (-0.83, 5.9, 1.3),
            (0.14, 9.8, 2.1),
            (0.0, 13.0, 0.0),
            (9.0, 4.0, 1.7),
        ]
        .into_iter()
        .enumerate()
        {
            world.set_pose(body, Vec2::new(x, y), angle, true);
            world.step(1.0 / 60.0);
            let snapshot = world.query_snapshot();
            let silhouettes = snapshot.lighting_occluder_silhouettes(&[]).unwrap();
            let beam = beam::polygons(source, 0.0, &silhouettes).unwrap();
            let points: Vec<_> = (0..61)
                .flat_map(|x| {
                    (0..39).map(move |y| {
                        Vec2::new(-3.977 + x as f32 * 0.131, 0.117 + y as f32 * 0.251)
                    })
                })
                .collect();
            for chunk in points.chunks(64) {
                let receivers: Vec<_> = chunk
                    .iter()
                    .map(|p| Receiver {
                        position: *p,
                        surface: None,
                    })
                    .collect();
                let samples = crate::grass::residents::query_light(
                    0,
                    0,
                    Some(source),
                    &[],
                    vec![],
                    Some(&snapshot),
                    &[],
                    &receivers,
                )
                .unwrap();
                for (&p, e) in chunk.iter().zip(samples) {
                    let lit = contains(&beam::outline(&beam), p);
                    if e.local == 0.0 {
                        blocked += 1;
                        assert!(
                            !lit,
                            "light crosses canonical shape kind={kind} pose={pose} point={p:?}"
                        );
                    } else {
                        clear += 1;
                        assert!(
                            lit || [Vec2::new(0.05, 0.05), Vec2::new(-0.05, 0.05)]
                                .iter()
                                .any(|d| beam.iter().any(|q| contains(q, p + *d))),
                            "false shadow kind={kind} pose={pose} point={p:?}"
                        );
                    }
                }
            }
            assert!(
                snapshot
                    .lighting_occluder_silhouettes(&[id])
                    .unwrap()
                    .is_empty()
            );
        }
        let old = world.query_snapshot();
        world.remove_entity(entity);
        world.step(1.0 / 60.0);
        assert!(
            world
                .query_snapshot()
                .lighting_occluder_silhouettes(&[])
                .unwrap()
                .is_empty()
        );
        assert!(!old.lighting_occluder_silhouettes(&[]).unwrap().is_empty());
    }
    assert!(blocked > 1000 && clear > 1000);
    println!(
        "canonical-render-agreement shapes=6 poses=36 receivers=85644 blocked={blocked} clear={clear}"
    );
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let id = ColliderId::new(PhysicsId::new(61), ColliderRole::PRIMARY, 0);
    assert!(world.insert_body(
        BodyId::new(id.entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[ColliderSpec::ball(id, 100.0)]
    ));
    assert!(matches!(
        world.query_snapshot().lighting_occluder_silhouettes(&[]),
        Err(engine_core::lighting::LightingError::Capacity)
    ));
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let mut sensor = ColliderSpec::ball(id, 1.0);
    sensor.sensor = true;
    assert!(world.insert_body(
        BodyId::new(id.entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[sensor]
    ));
    assert!(
        world
            .query_snapshot()
            .lighting_occluder_silhouettes(&[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn ceiling_light_contract_source_plane_containment_keeps_local_endpoint_rules() {
    use engine_core::lighting::{ApertureSource, LightId};
    let source = ApertureSource {
        id: LightId(99, 1),
        left: -4.0,
        right: 4.0,
        ceiling_y: 10.0,
        strength: 950.0,
    };
    let id = LightId(100, 1);
    let polygon = vec![
        Vec2::new(-1.0, 5.0),
        Vec2::new(1.0, 5.0),
        Vec2::new(1.0, 11.0),
        Vec2::new(-1.0, 11.0),
    ];
    let receivers = [
        Receiver {
            position: Vec2::new(0.0, 6.0),
            surface: None,
        },
        Receiver {
            position: Vec2::new(0.0, 6.0),
            surface: Some(id),
        },
        Receiver {
            position: Vec2::new(2.0, 6.0),
            surface: None,
        },
    ];
    let samples = crate::grass::residents::query_light(
        0,
        0,
        Some(source),
        &[],
        vec![(id, polygon.clone())],
        None,
        &[],
        &receivers,
    )
    .unwrap();
    assert_eq!(
        samples[0].local, 0.0,
        "the whole ray lies inside another opaque polygon"
    );
    assert_eq!(samples[0].first_blocker, Some(id));
    assert_eq!(samples[1].local, 950.0, "existing local surface exemption");
    assert_eq!(
        samples[2].local, 950.0,
        "adjacent clear space is not the polygon AABB"
    );
    let beam = beam::polygons(source, 0.0, &[polygon]).unwrap();
    assert!(!beam.iter().any(|q| contains(q, receivers[0].position)));
    assert!(beam.iter().any(|q| contains(q, receivers[2].position)));
}

fn controlled_fixture() -> ClockState {
    let mut s = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            aspect_ratio: 16.0 / 9.0,
            ..Default::default()
        },
        4242,
    );
    ClockScenario::step(
        &mut s,
        &[
            ClockAction::set_reading(ClockReading::new(12, 34, 0).unwrap()),
            ClockAction::trigger_event(ClockEventKind::Grass),
        ],
        Duration::ZERO,
    );
    while !s.can_trigger_event() {
        ClockScenario::step(&mut s, &[], Duration::from_nanos(16_666_667));
        assert!(s.simulation_tick() < 30 * 60);
    }
    ClockScenario::step(
        &mut s,
        &[ClockAction::trigger_event(ClockEventKind::Duck)],
        Duration::ZERO,
    );
    while s.duck_state().and_then(|d| d.position_milli).is_none() {
        ClockScenario::step(&mut s, &[], Duration::from_nanos(16_666_667));
        assert!(s.simulation_tick() < 40 * 60);
    }
    ClockScenario::step(
        &mut s,
        &[ClockAction::toggle_player_duck(1)],
        Duration::ZERO,
    );
    s
}

// Model the actual public lease: a held button, a released setpoint, then
// neutral simulation frames for state polling/IPC. Continuous intent hides
// the loss of momentum at the physical six-unit soil lip.
fn control_lease(s: &mut ClockState, direction: i16, jump: bool, held: u32, gap: u32) {
    let (session_id, player) = s.player_duck_session().unwrap();
    for tick in 0..held + gap {
        ClockScenario::step(
            s,
            &[ClockAction::player_duck_input(crate::ClockDuckInput {
                session_id,
                player,
                move_milli: if tick < held { direction } else { 0 },
                jump: jump && tick < held,
                run: false,
                dive: false,
            })],
            Duration::from_nanos(16_666_667),
        );
    }
    ClockScenario::step(
        s,
        &[ClockAction::player_duck_input(crate::ClockDuckInput {
            session_id,
            player,
            move_milli: 0,
            jump: false,
            run: false,
            dive: false,
        })],
        Duration::ZERO,
    );
}

fn controlled_sequence(gap: u32, hop: bool) -> bool {
    let mut s = controlled_fixture();
    let mut stage = 0;
    let mut shadow_indices = Vec::new();
    let mut hops = 0;
    for _ in 0..240 {
        let optics = optical_evidence(&s).unwrap();
        let [a, b, _] = optics.aperture_milli;
        let duck = s.player_duck_state().unwrap().duck;
        let x = duck.position_milli.unwrap()[0];
        let reached = match stage {
            0 => x > b + 16000,
            1 => x < b - 4000,
            2 => x < (a + b) / 2 + 4000,
            3 => x > b + 16000,
            _ => unreachable!(),
        };
        if reached {
            println!(
                "leased-moving gap={gap} stage={stage} tick={} x={x} hops={hops}",
                s.simulation_tick()
            );
            // Public capture clears held input and leaves the body at rest.
            // Paused rendering must not advance source time, physics or light.
            let frozen = s.grass_state();
            for _ in 0..60 {
                ClockScenario::step(&mut s, &[], Duration::ZERO);
            }
            assert_eq!(s.grass_state(), frozen);
            if stage == 2 {
                shadow_indices = optics
                    .samples
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| {
                        (s.sunlight == 0
                            && s.first_blocker.is_some()
                            && s.first_blocker == optics.moving_blocker)
                            .then_some(i)
                    })
                    .collect();
                assert!(
                    !shadow_indices.is_empty(),
                    "crossing must contain Duck-caused shade"
                );
            }
            if stage == 3 {
                assert!(
                    shadow_indices
                        .iter()
                        .any(|i| optics.samples[*i].sunlight > 0),
                    "departure restores light"
                );
                assert!(hops > 0, "exercise the real soil lip jump");
                return true;
            }
            stage += 1;
        }
        if hop && stage == 1 && duck.grounded && x < b + 10000 {
            hops += 1;
            assert!(hops <= 6, "bounded soil lip jump attempts");
            control_lease(&mut s, 0, true, 6, gap);
        }
        control_lease(
            &mut s,
            if stage == 1 || stage == 2 {
                -1000
            } else {
                1000
            },
            false,
            if hop { 6 } else { 3 },
            gap,
        );
    }
    let duck = s.player_duck_state().unwrap().duck;
    println!(
        "leased-moving stalled gap={gap} stage={stage} pose={:?}",
        duck.position_milli
    );
    assert_eq!(stage, 1, "the uncorrected script stalls at the soil lip");
    false
}

#[test]
fn ceiling_light_contract_short_leases_reproduce_soil_lip_stall() {
    assert!(!controlled_sequence(12, false));
}

#[test]
fn ceiling_light_contract_moving_evidence_sequence() {
    for gap in [3, 12, 30] {
        assert!(controlled_sequence(gap, true));
    }
}
