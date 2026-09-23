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
    };
    Some(Spill {
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
    );
    let mut stats = Stats::default();
    step(
        &mut parcels,
        &mut spills,
        &mut scratch,
        &mut stats,
        WaterConfig::default(),
        1.0 / 60.0,
        10,
    );
    assert_eq!(
        storage,
        (
            parcels.as_ptr(),
            spills.as_ptr(),
            scratch.candidates.as_ptr(),
            scratch.groups.as_ptr()
        )
    );
    (parcels, spills, stats)
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
fn junction_links_material_continuity_not_collision_frame_numbers() {
    for emission_tick in [11, 12] {
        let old = parcel(0.0, -0.5, 0.0, 2.0);
        let previous = Spill {
            source: SpillSource::Junction { outlets: [1, 2] },
            tick: 10,
            upstream_end: Some([10; 2]),
            tail: Section {
                position: Vec2::new(0.0, -0.4),
                velocity: old.velocity,
                flow: 120.0,
            },
            head: Section {
                position: Vec2::new(0.0, -0.6),
                velocity: old.velocity,
                flow: 120.0,
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
            WaterConfig::default(),
            1.0 / 60.0,
            12,
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
