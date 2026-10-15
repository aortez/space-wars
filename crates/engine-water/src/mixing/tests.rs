use super::*;

fn parcel(x: f32, y: f32, vx: f32, volume: f64) -> Parcel {
    Parcel {
        position: Vec2::new(x, y),
        velocity: Vec2::new(vx, -10.0),
        volume,
        duration: 1.0 / 60.0,
        horizontal_bounds: None,
    }
}

fn metadata(p: Parcel, pool: usize) -> Option<Spill> {
    let section = Section {
        position: p.position,
        velocity: p.velocity,
        flow: p.volume / p.duration,
        material_offset: None,
    };
    Some(Spill {
        sheet: None,
        upstream_end: None,
        source: SpillSource::Outlet {
            pool,
            edge: usize::from(p.velocity.x > 0.0),
        },
        tick: 10,
        tail: section,
        head: section,
    })
}

fn mix(input: &[Parcel], sources: &[Option<Spill>]) -> (Vec<Parcel>, Vec<Option<Spill>>, Stats) {
    mix_with_solids(input, sources, &[])
}

fn mix_with_solids(
    input: &[Parcel],
    sources: &[Option<Spill>],
    solids: &[SolidBox],
) -> (Vec<Parcel>, Vec<Option<Spill>>, Stats) {
    mix_in_phase(input, sources, solids, Phase::InFlight)
}

fn mix_in_phase(
    input: &[Parcel],
    sources: &[Option<Spill>],
    solids: &[SolidBox],
    phase: Phase,
) -> (Vec<Parcel>, Vec<Option<Spill>>, Stats) {
    let mut parcels = Vec::with_capacity(8);
    parcels.extend_from_slice(input);
    let mut spills = Vec::with_capacity(8);
    spills.extend_from_slice(sources);
    let mut scratch = Scratch::new(8);
    let storage = (
        parcels.as_ptr(),
        spills.as_ptr(),
        scratch.candidates.as_ptr(),
        scratch.groups.as_ptr(),
        scratch.ribbons.as_ptr(),
    );
    let mut stats = Stats::default();
    step(
        &mut parcels,
        &mut spills,
        &mut scratch,
        &mut stats,
        &mut crate::splash::State::default(),
        solids,
        WaterConfig {
            retain_spill_shapes: true,
            ..Default::default()
        },
        1.0 / 60.0,
        10,
        phase,
    );
    assert_eq!(
        storage,
        (
            parcels.as_ptr(),
            spills.as_ptr(),
            scratch.candidates.as_ptr(),
            scratch.groups.as_ptr(),
            scratch.ribbons.as_ptr()
        )
    );
    (parcels, spills, stats)
}

#[test]
fn retained_contact_closes_the_gap_without_adding_area_in_either_mixing_phase() {
    for (phase, distance) in [(Phase::InFlight, 1.5), (Phase::Newborn { first: 0 }, 0.75)] {
        let input = [
            parcel(-distance, 100.0, 120.0, 400.0),
            parcel(distance, 100.0, -120.0, 400.0),
        ];
        let sources = std::array::from_fn::<_, 2, _>(|i| {
            let p = input[i];
            let mut s = metadata(p, i).unwrap();
            s.tail.position -= p.velocity * (p.duration * 0.5) as f32;
            s.head.position += p.velocity * (p.duration * 0.5) as f32;
            s.tail.material_offset = Some(Vec2::new(0.0, p.velocity.x.signum() * 100.0));
            s.head.material_offset = s.tail.material_offset;
            Some(s)
        });
        let first_count = sources[0].unwrap().ribbon(&input[0]).unwrap().quads().len();
        let (output, spills, stats) = mix_in_phase(&input, &sources, &[], phase);
        assert_eq!(stats.pairs, 1);
        assert_eq!(output[0].position, Vec2::new(0.0, 100.0));
        assert_eq!(output[0].velocity, Vec2::new(0.0, -10.0));
        let ribbon = spills[0].unwrap().ribbon(&output[0]).unwrap();
        let (left, right) = ribbon.quads().split_at(first_count);
        let left_edge = left
            .iter()
            .flatten()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let right_edge = right
            .iter()
            .flatten()
            .map(|p| p.x)
            .fold(f32::INFINITY, f32::min);
        assert!(
            (left_edge - right_edge).abs() < 1e-5,
            "contact must not freeze an air gap"
        );
        let area: f64 = ribbon
            .quads()
            .iter()
            .map(|q| {
                (0..4)
                    .map(|i| {
                        (q[i].x as f64 * q[(i + 1) % 4].y as f64
                            - q[i].y as f64 * q[(i + 1) % 4].x as f64)
                            * 0.5
                    })
                    .sum::<f64>()
                    .abs()
            })
            .sum();
        assert!(
            (area - 800.0).abs() < 0.001,
            "contact translates existing volume only"
        );
    }
}

#[test]
fn deep_vertical_sheets_only_mix_when_their_material_reaches_contact() {
    for (separation, speed, duration, expected) in [
        (20.0, 120.0, 1.0 / 60.0, 0),
        (3.0, 120.0, 1.0 / 60.0, 1),
        (8.0, 600.0, 0.001, 1),
    ] {
        let input = [-1.0, 1.0].map(|sign| Parcel {
            position: Vec2::new(sign * separation * 0.5, 100.0),
            velocity: Vec2::new(-sign * speed, -60.0),
            volume: 400.0,
            duration,
            horizontal_bounds: None,
        });
        let sources = std::array::from_fn::<_, 2, _>(|i| {
            let p = input[i];
            let mut s = metadata(p, i).unwrap();
            s.tail.position -= p.velocity * (p.duration * 0.5) as f32;
            s.head.position += p.velocity * (p.duration * 0.5) as f32;
            let offset = Vec2::new(
                0.0,
                (p.volume / (2.0 * p.velocity.x as f64 * p.duration)) as f32,
            );
            s.tail.material_offset = Some(offset);
            s.head.material_offset = Some(offset);
            Some(s)
        });
        let (output, _, stats) = mix(&input, &sources);
        assert_eq!(
            stats.pairs, expected,
            "separation={separation}, speed={speed}"
        );
        assert_eq!(output.iter().map(|p| p.volume).sum::<f64>(), 800.0);
        if expected == 0 {
            assert_eq!(output, input);
            let rotated = sources.map(|s| {
                s.map(|mut s| {
                    s.tail.material_offset = None;
                    s.head.material_offset = None;
                    s
                })
            });
            assert_eq!(
                mix(&input, &rotated).2.pairs,
                1,
                "old tilted faces collide too soon"
            );
        } else {
            assert_eq!(output[0].velocity, Vec2::new(0.0, -60.0));
        }
    }
}

#[test]
fn finite_solid_blocks_cross_wall_mixing_but_allows_clear_pairs() {
    let wall = SolidBox::new(Vec2::ZERO, Vec2::new(0.1, 2.0), 0.0).unwrap();
    for y in [0.0, -5.0, 5.0] {
        for volumes in [[1.0, 1.0], [3.0, 1.0]] {
            let input = [
                parcel(-0.5, y, 60.0, volumes[0]),
                parcel(0.5, y, -60.0, volumes[1]),
            ];
            let sources = [metadata(input[0], 0), metadata(input[1], 1)];
            let (output, _, stats) = mix_with_solids(&input, &sources, &[wall]);
            assert_eq!(stats.pairs, u64::from(y != 0.0));
            assert_eq!(output.len(), if y == 0.0 { 2 } else { 1 });
            assert_eq!(
                output.iter().map(|p| p.volume).sum::<f64>(),
                volumes.iter().sum()
            );
            if y == 0.0 {
                assert_eq!(output, input);
            }
        }
    }
}

#[test]
fn clear_pairs_do_not_aggregate_across_a_solid() {
    let input = [
        parcel(-0.5, 2.0, 60.0, 10.0),
        parcel(0.5, 2.0, -60.0, 10.0),
        parcel(-0.5, -2.0, 60.0, 10.0),
        parcel(0.5, -2.0, -60.0, 10.0),
    ];
    let sources = [
        metadata(input[0], 0),
        metadata(input[1], 1),
        metadata(input[2], 0),
        metadata(input[3], 1),
    ];
    let wall = SolidBox::new(Vec2::ZERO, Vec2::new(2.0, 0.1), 0.0).unwrap();
    let (output, _, stats) = mix_with_solids(&input, &sources, &[wall]);
    assert_eq!(stats.pairs, 2);
    assert_eq!(
        output.len(),
        2,
        "two clear contact patches stay on their own sides"
    );
    assert!(
        output
            .iter()
            .all(|p| p.volume == 20.0 && p.position.y.abs() == 2.0)
    );
}

#[test]
fn equal_and_unequal_streams_conserve_volume_momentum_and_center_of_mass() {
    for weights in [[1.0, 1.0], [3.0, 1.0]] {
        let input = [
            parcel(-0.5, 0.0, 60.0, weights[0]),
            parcel(0.5, 0.0, -60.0, weights[1]),
        ];
        let (output, sources, stats) = mix(&input, &[metadata(input[0], 0), metadata(input[1], 1)]);
        assert_eq!(output.len(), 1);
        let total = weights[0] + weights[1];
        assert_eq!(output[0].volume, total);
        let expected_velocity = (input[0].velocity * weights[0] as f32
            + input[1].velocity * weights[1] as f32)
            / total as f32;
        let expected_position = (input[0].position * weights[0] as f32
            + input[1].position * weights[1] as f32)
            / total as f32;
        assert_eq!(output[0].velocity, expected_velocity);
        assert_eq!(output[0].position, expected_position);
        let energy_before = input
            .iter()
            .map(|p| p.volume * p.velocity.length_squared() as f64)
            .sum::<f64>();
        assert!(output[0].volume * output[0].velocity.length_squared() as f64 <= energy_before);
        assert_eq!(
            sources[0].unwrap().source,
            SpillSource::Junction { outlets: [1, 2] }
        );
        assert_eq!(stats.pairs, 1);
        assert_eq!(stats.volume, total);
    }
}

#[test]
fn deep_collision_retains_both_material_footprints_without_inventing_a_bridge() {
    for right_volume in [100.0, 200.0] {
        let input = [
            parcel(-1.0, 100.0, 120.0, 200.0),
            parcel(1.0, 100.0, -120.0, right_volume),
        ];
        let sources = std::array::from_fn::<_, 2, _>(|i| {
            let p = input[i];
            let mut s = metadata(p, i).unwrap();
            s.tail.position -= p.velocity * (p.duration * 0.5) as f32;
            s.head.position += p.velocity * (p.duration * 0.5) as f32;
            Some(s)
        });
        let before: Vec<_> = input
            .iter()
            .zip(sources)
            .flat_map(|(p, s)| s.unwrap().ribbon(p).unwrap().quads().to_vec())
            .collect();
        let (output, sources, stats) = mix(&input, &sources);
        assert_eq!(stats.pairs, 1);
        assert_eq!(output.len(), 1);
        let p = output[0];
        assert_eq!(p.volume, 200.0 + right_volume);
        let spill = sources[0].unwrap();
        assert!(spill.sheet.is_some(), "deep slices must retain their shape");
        let ribbon = spill.ribbon(&p).unwrap();
        assert_eq!(before.len(), ribbon.quads().len());
        for (original, retained) in before.iter().zip(ribbon.quads()) {
            for (a, b) in original.iter().zip(retained) {
                assert!((*a - *b).length() < 1e-5);
            }
        }
        assert!((0..ribbon.quads().len()).all(|i| !ribbon.has_surface(i)));
        // Cancellation/acceleration changes velocity, not the material's
        // existing extent. Only uniform translation occurs in free fall.
        let displacement = Vec2::new(0.0, -30.0);
        let falling = Parcel {
            position: p.position + displacement,
            velocity: Vec2::new(0.0, -180.0),
            ..p
        };
        for (original, moved) in ribbon
            .quads()
            .iter()
            .zip(spill.ribbon(&falling).unwrap().quads())
        {
            for (a, b) in original.iter().zip(moved) {
                assert!((*a + displacement - *b).length() < 2e-5);
            }
        }
    }
}

#[test]
fn swept_contacts_catch_fast_streams_before_they_pass_through() {
    let mut input = [parcel(-4.0, 0.0, 600.0, 0.1), parcel(4.0, 0.0, -600.0, 0.1)];
    // Thin slices are disjoint at BOTH ends of the step, but cross within it.
    for p in &mut input {
        p.duration = 0.001;
    }
    let (output, _, stats) = mix(&input, &[metadata(input[0], 0), metadata(input[1], 1)]);
    assert_eq!(output.len(), 1);
    assert_eq!(stats.pairs, 1);
}

#[test]
fn newborn_sweep_catches_crossed_centers_and_preserves_older_parcels() {
    let config = WaterConfig::default();
    let dt = 1.0 / 60.0;
    for weights in [[1.0, 1.0], [3.0, 1.0]] {
        for wall in [false, true] {
            let older = parcel(-0.1, 0.0, 120.0, 0.5);
            let mut born = [
                parcel(-0.5, 0.0, 120.0, weights[0]),
                parcel(0.5, 0.0, -120.0, weights[1]),
            ];
            for p in &mut born {
                let mut center = Section {
                    position: p.position,
                    velocity: p.velocity,
                    flow: 0.0,
                    material_offset: None,
                };
                center.advance(dt * 0.5, config, None);
                p.position = center.position;
                p.velocity = center.velocity;
            }
            assert!(born[0].position.x > born[1].position.x);
            let mut parcels = vec![older, born[0], born[1]];
            let mut sources = vec![
                metadata(older, 2),
                metadata(born[0], 0),
                metadata(born[1], 1),
            ];
            let mut stats = Stats::default();
            let solids = if wall {
                vec![SolidBox::new(Vec2::ZERO, Vec2::new(0.1, 10.0), 0.0).unwrap()]
            } else {
                vec![]
            };
            step(
                &mut parcels,
                &mut sources,
                &mut Scratch::new(3),
                &mut stats,
                &mut crate::splash::State::default(),
                &solids,
                config,
                dt,
                10,
                Phase::Newborn { first: 1 },
            );
            assert_eq!(
                parcels[0], older,
                "already advanced water must not mix a second time"
            );
            assert_eq!(stats.pairs, u64::from(!wall));
            if wall {
                assert_eq!(&parcels[1..], &born);
            } else {
                assert_eq!(parcels.len(), 2);
                let total = weights[0] + weights[1];
                assert_eq!(parcels[1].volume, total);
                let expected_position = (born[0].position * weights[0] as f32
                    + born[1].position * weights[1] as f32)
                    / total as f32;
                let expected_velocity = (born[0].velocity * weights[0] as f32
                    + born[1].velocity * weights[1] as f32)
                    / total as f32;
                assert!((parcels[1].position - expected_position).length() < 1e-6);
                assert!((parcels[1].velocity - expected_velocity).length() < 1e-6);
            }
        }
    }
}

#[test]
fn junction_links_material_continuity_not_collision_frame_numbers() {
    for emission_tick in [11, 12] {
        let old = parcel(0.0, -0.5, 0.0, 2.0);
        let previous = Spill {
            sheet: None,
            source: SpillSource::Junction { outlets: [1, 2] },
            tick: 10,
            upstream_end: Some([10; 2]),
            tail: Section {
                position: Vec2::new(0.0, -0.4),
                velocity: old.velocity,
                flow: 120.0,
                material_offset: None,
            },
            head: Section {
                position: Vec2::new(0.0, -0.6),
                velocity: old.velocity,
                flow: 120.0,
                material_offset: None,
            },
        };
        let a = parcel(-0.5, 0.0, 60.0, 1.0);
        let b = parcel(0.5, 0.0, -60.0, 1.0);
        let mut parcels = vec![old, a, b];
        let mut spills = vec![
            Some(previous),
            Some(Spill {
                tick: emission_tick,
                ..metadata(a, 0).unwrap()
            }),
            Some(Spill {
                tick: emission_tick,
                ..metadata(b, 1).unwrap()
            }),
        ];
        step(
            &mut parcels,
            &mut spills,
            &mut Scratch::new(3),
            &mut Stats::default(),
            &mut crate::splash::State::default(),
            &[],
            WaterConfig::default(),
            1.0 / 60.0,
            12,
            Phase::InFlight,
        );
        assert_eq!(parcels.len(), 2);
        assert_eq!(parcels.iter().map(|p| p.volume).sum::<f64>(), 4.0);
        let next = spills[1].unwrap();
        assert_eq!(next.upstream_end, Some([emission_tick; 2]));
        assert_eq!(
            next.head.position == previous.tail.position,
            emission_tick == 11,
            "delayed collision of adjacent source slices connects; a real source pause stays separate"
        );
    }
}

#[test]
fn separate_contact_patches_of_the_same_outlets_do_not_teleport_together() {
    let input = [
        parcel(-0.5, 0.0, 60.0, 1.0),
        parcel(0.5, 0.0, -60.0, 1.0),
        parcel(-0.5, -100.0, 60.0, 1.0),
        parcel(0.5, -100.0, -60.0, 1.0),
    ];
    let (output, _, stats) = mix(
        &input,
        &[
            metadata(input[0], 0),
            metadata(input[1], 1),
            metadata(input[2], 0),
            metadata(input[3], 1),
        ],
    );
    assert_eq!(output.len(), 2);
    assert!(
        output
            .iter()
            .any(|p| p.position.y == 0.0 && p.volume == 2.0)
    );
    assert!(
        output
            .iter()
            .any(|p| p.position.y == -100.0 && p.volume == 2.0)
    );
    assert_eq!(stats.pairs, 2);
}

#[test]
fn misses_parallel_separating_and_independent_parcels_are_unchanged() {
    let left = parcel(-0.5, 0.0, 60.0, 0.1);
    for right in [
        parcel(0.5, 10.0, -60.0, 0.1),
        parcel(0.5, 0.0, 60.0, 0.1),
        parcel(-2.0, 0.0, -60.0, 0.1),
    ] {
        let (output, _, stats) = mix(&[left, right], &[metadata(left, 0), metadata(right, 1)]);
        assert_eq!(output, [left, right]);
        assert_eq!(stats.pairs, 0);
    }
    let right = parcel(0.5, 0.0, -60.0, 0.1);
    for source in [
        None,
        Some(Spill {
            sheet: None,
            source: SpillSource::Junction { outlets: [1, 2] },
            ..metadata(right, 1).unwrap()
        }),
    ] {
        let (output, _, _) = mix(&[left, right], &[metadata(left, 0), source]);
        assert_eq!(output, [left, right]);
    }
    let bounded = Parcel {
        horizontal_bounds: Some([-3.0, 3.0]),
        ..right
    };
    let (output, _, _) = mix(&[left, bounded], &[metadata(left, 0), metadata(bounded, 1)]);
    assert_eq!(output, [left, bounded]);
}
