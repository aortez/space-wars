use super::*;
struct Clear;
impl Visibility for Clear {
    fn blocker(&self, _: LightRay) -> Option<LightId> {
        None
    }
}
fn source(id: u64) -> LocalSource {
    LocalSource {
        id: LightId(id, 1),
        active: true,
        color: Color::RED,
        strength: 100.0,
        range: 10.0,
        footprint: vec![EmissionPoint {
            position: Vec2::ZERO,
            surface: None,
        }],
    }
}
fn snapshot(sources: &[LocalSource]) -> LightingSnapshot {
    LightingSnapshot::new(
        10,
        7,
        DirectionalSource {
            toward_source: Vec2::Y,
            strength: 25.0,
            color: Color::WHITE,
        },
        sources,
    )
    .unwrap()
}
fn receiver(x: f32) -> Receiver {
    Receiver {
        position: Vec2::new(x, 0.0),
        surface: None,
    }
}
fn query(s: &LightingSnapshot, x: f32) -> Exposure {
    s.query(10, 7, &[receiver(x)], &Clear).unwrap().remove(0)
}
#[test]
fn finite_contact_distance_cutoff_combination_and_baseline() {
    let s = snapshot(&[source(1)]);
    assert_eq!(query(&s, 0.0).intensity, 125.0);
    assert_eq!(query(&s, 5.0).intensity, 50.0);
    assert_eq!(query(&s, 10.0).local, 0.0);
    assert_eq!(query(&s, 20.0).local, 0.0);
    assert_eq!(query(&snapshot(&[]), 0.0).intensity, 25.0);
    assert_eq!(
        query(&snapshot(&[source(1), source(2)]), 0.0).intensity,
        225.0
    );
    let mut bright = source(1);
    bright.strength = FULL_SUN;
    assert_eq!(query(&snapshot(&[bright]), 0.0).intensity, FULL_SUN);
}
#[test]
fn identity_copies_hue_active_and_footprint_preserve_energy() {
    let a = source(1);
    let copies = snapshot(&[a.clone(), a.clone()]);
    assert_eq!(copies.duplicate_copies(), 1);
    assert_eq!(query(&copies, 2.0), query(&snapshot(&[a.clone()]), 2.0));
    let mut b = a.clone();
    b.color = Color::BLUE;
    assert_eq!(
        query(&snapshot(&[a.clone()]), 0.0),
        query(&snapshot(&[b.clone()]), 0.0)
    );
    assert!(matches!(
        LightingSnapshot::new(10, 7, copies.directional, &[a, b.clone()]),
        Err(LightingError::ConflictingIdentity)
    ));
    b.footprint = vec![b.footprint[0]; MAX_FOOTPRINT];
    assert_eq!(query(&snapshot(&[b.clone()]), 0.0).local, 100.0);
    b.active = false;
    assert_eq!(query(&snapshot(&[b]), 0.0).local, 0.0);
}
#[test]
fn immutable_age_generation_removal_and_invalid_inputs() {
    let s = snapshot(&[source(1)]);
    assert!(s.query(11, 7, &[receiver(0.0)], &Clear).is_ok());
    for tick in [9, 12, u64::MAX] {
        assert_eq!(s.query(tick, 7, &[], &Clear), Err(LightingError::Stale));
    }
    assert_eq!(
        s.query(10, 8, &[], &Clear),
        Err(LightingError::GeometryGeneration)
    );
    assert_eq!(
        s.query(10, 7, &[receiver(f32::NAN)], &Clear),
        Err(LightingError::InvalidInput)
    );
    let mut invalid = source(1);
    invalid.range = f32::INFINITY;
    assert!(matches!(
        LightingSnapshot::new(10, 7, s.directional, &[invalid]),
        Err(LightingError::InvalidInput)
    ));
    assert_eq!(query(&snapshot(&[]), 0.0).local, 0.0);
    assert_eq!(query(&s, 0.0).local, 100.0); // Older publication stays immutable.
}
#[test]
fn bounds_reject_overflow_and_count_maximum_work() {
    let sources: Vec<_> = (0..MAX_SOURCES)
        .map(|id| {
            let mut s = source(id as u64);
            s.footprint = vec![s.footprint[0]; MAX_FOOTPRINT];
            s
        })
        .collect();
    let s = snapshot(&sources);
    let results = s
        .query(10, 7, &vec![receiver(0.0); MAX_RECEIVERS], &Clear)
        .unwrap();
    assert!(
        results
            .iter()
            .all(|r| r.rays == 1 + MAX_SOURCES * MAX_FOOTPRINT)
    );
    assert_eq!(
        s.query(10, 7, &vec![receiver(0.0); MAX_RECEIVERS + 1], &Clear),
        Err(LightingError::Capacity)
    );
    let mut over = sources;
    over.push(source(MAX_SOURCES as u64));
    assert!(matches!(
        LightingSnapshot::new(10, 7, s.directional, &over),
        Err(LightingError::Capacity)
    ));
    let mut reversed = s.sources.clone();
    reversed.reverse();
    assert_eq!(snapshot(&reversed).sources, s.sources);
}
#[test]
fn blockers_remove_only_their_direct_contribution() {
    struct Block;
    impl Visibility for Block {
        fn blocker(&self, ray: LightRay) -> Option<LightId> {
            (ray.endpoint.position == Vec2::ZERO).then_some(LightId(99, 1))
        }
    }
    let s = snapshot(&[source(1)]);
    let r = s.query(10, 7, &[receiver(1.0)], &Block).unwrap().remove(0);
    assert_eq!(r.directional, 25.0);
    assert_eq!(r.local, 0.0);
    assert_eq!(r.blocked_rays, 1);
    assert_eq!(r.first_blocker, Some(LightId(99, 1)));
}
#[test]
fn publisher_is_atomic_monotonic_and_observable() {
    let mut publisher = LightingPublisher::default();
    let d = snapshot(&[]).directional;
    publisher.publish(1, 1, d, &[source(1)]).unwrap();
    let old = publisher.snapshot().unwrap();
    assert_eq!(publisher.publish(1, 1, d, &[]), Err(LightingError::Stale));
    let mut oversized = source(1);
    oversized
        .footprint
        .resize(MAX_FOOTPRINT + 1, oversized.footprint[0]);
    assert_eq!(
        publisher.publish(2, 2, d, &[oversized]),
        Err(LightingError::Capacity)
    );
    assert_eq!(publisher.snapshot().unwrap().tick(), 1);
    publisher.publish(2, 2, d, &[]).unwrap();
    assert_eq!(old.sources().len(), 1);
    assert!(publisher.snapshot().unwrap().sources().is_empty());
    assert_eq!(
        publisher.counters(),
        LightingCounters {
            published: 2,
            rejected: 2,
            capacity_rejections: 1
        }
    );
}
#[test]
fn input_copy_and_footprint_caps_fail_without_partial_publication() {
    let d = snapshot(&[]).directional;
    let copies = vec![source(1); MAX_SOURCES * MAX_FOOTPRINT];
    let s = LightingSnapshot::new(0, 0, d, &copies).unwrap();
    assert_eq!(s.sources().len(), 1);
    assert_eq!(s.duplicate_copies(), copies.len() - 1);
    let mut over = copies;
    over.push(source(1));
    assert!(matches!(
        LightingSnapshot::new(0, 0, d, &over),
        Err(LightingError::Capacity)
    ));
    let mut empty = source(1);
    empty.footprint.clear();
    assert!(matches!(
        LightingSnapshot::new(0, 0, d, &[empty]),
        Err(LightingError::Capacity)
    ));
    let invalid_d = DirectionalSource {
        toward_source: Vec2::ZERO,
        ..d
    };
    assert!(matches!(
        LightingSnapshot::new(0, 0, invalid_d, &[]),
        Err(LightingError::InvalidInput)
    ));
}
