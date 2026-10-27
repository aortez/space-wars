use super::*;
use engine_common::{ClockEventProfile, Scenario};
use std::time::Duration;

fn ready(profile: ClockEventProfile, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: profile,
            events: ClockEvents {
                falling: true,
                color_cycle: false,
                meltdown: false,
                duck: false,
                marquee: false,
                digit_slide: false,
                rain: false,
                crow: false,
                explosion: false,
                aquarium: false,
                cell_swap: false,
                grass: false,
            },
            ..ClockConfig::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(23, 58, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

fn ticks(state: &mut ClockState, count: u64) {
    for _ in 0..count {
        ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
    }
}

fn trigger(state: &mut ClockState) {
    ClockScenario::step(
        state,
        &[ClockAction::trigger_event(ClockEventKind::Falling)],
        Duration::ZERO,
    );
}

#[test]
fn fall_uses_compound_segments_then_releases_all_physics_resources() {
    let mut state = ready(ClockEventProfile::Off, 7);
    let lit_count = state.segments().iter().filter(|s| s.lit).count();
    trigger(&mut state);
    assert_eq!(state.event_phase(), Some(EventPhase::Falling));
    assert_eq!(state.body_count(), lit_count + 4);
    assert!(state.collider_count() > state.body_count() * 2);
    assert!(state.collider_count() <= 100);
    ticks(&mut state, FALLING_TICKS - 1);
    let layout = Layout::new(state.aspect_ratio());
    let on_floor = state
        .segments()
        .iter()
        .filter(|segment| {
            let SegmentRepresentation::Rigid { position, .. } = segment.representation else {
                return false;
            };
            position.y >= layout.floor_y && position.y < layout.floor_y + layout.pitch * 5.0
        })
        .count();
    assert!(
        on_floor >= lit_count / 2,
        "bars should collide with the floor, not fall through it: {on_floor}/{lit_count}"
    );
    ticks(&mut state, 1);
    assert_eq!(state.event_phase(), Some(EventPhase::Reforming));
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    ticks(&mut state, REFORMING_TICKS);
    assert_eq!(state.lifecycle(), EventLifecycle::Cooldown);
    assert!(
        state
            .segments()
            .iter()
            .all(|s| s.representation == SegmentRepresentation::Anchored)
    );
    ticks(&mut state, COOLDOWN_TICKS);
    assert!(state.can_trigger_event());
    assert_eq!(state.event_id(), 1);
}

#[test]
fn recovery_uses_latest_time_even_if_digits_change_during_the_fall_and_reform() {
    let mut state = ready(ClockEventProfile::Off, 1);
    trigger(&mut state);
    let released = state.segments().iter().map(|s| s.lit).collect::<Vec<_>>();
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(0, 0, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    assert_eq!(
        state.segments().iter().map(|s| s.lit).collect::<Vec<_>>(),
        released
    );
    ticks(&mut state, FALLING_TICKS);
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(0, 1, 30).unwrap(),
        )],
        Duration::ZERO,
    );
    ticks(&mut state, REFORMING_TICKS);
    assert_eq!(state.display().digits, [Some(0), Some(0), Some(0), Some(1)]);
    let mut expected = digits::create_segments();
    digits::apply_snapshot(&mut expected, state.display());
    assert_eq!(state.segments(), expected);
}

#[test]
fn seeded_demo_replays_schedule_and_motion_exactly() {
    let mut a = ready(ClockEventProfile::Demo, 42);
    let mut b = ready(ClockEventProfile::Demo, 42);
    let first = a.next_event_tick().unwrap();
    assert!((6 * 60..=10 * 60).contains(&first));
    for _ in 0..first + FALLING_TICKS + REFORMING_TICKS + COOLDOWN_TICKS {
        ticks(&mut a, 1);
        ticks(&mut b, 1);
        assert_eq!(
            (a.event_phase(), a.event_id(), a.next_event_tick()),
            (b.event_phase(), b.event_id(), b.next_event_tick())
        );
        assert_eq!(a.segments(), b.segments());
    }
    assert_eq!(a.event_id(), 1);
    assert!(a.next_event_tick().unwrap() > a.simulation_tick());
}

#[test]
fn calm_waits_for_its_seeded_deadline_before_releasing_any_bars() {
    let mut state = ready(ClockEventProfile::Calm, 42);
    let deadline = state.next_event_tick().unwrap();
    assert!((45 * 60..=75 * 60).contains(&deadline));
    ticks(&mut state, deadline - 1);
    assert_eq!(state.lifecycle(), EventLifecycle::Idle);
    assert_eq!((state.event_id(), state.body_count()), (0, 0));
    ticks(&mut state, 1);
    assert_eq!(state.event_phase(), Some(EventPhase::Falling));
    assert_eq!(state.phase_tick(), 0);
    assert_eq!(state.event_id(), 1);
}

#[test]
fn zero_duration_busy_triggers_and_resizes_do_not_leak_or_restart_events() {
    let mut state = ready(ClockEventProfile::Off, 5);
    trigger(&mut state);
    ticks(&mut state, 30);
    let poses = state.segments().to_vec();
    trigger(&mut state);
    assert_eq!(state.event_id(), 1);
    assert_eq!(state.phase_tick(), 30);
    assert_eq!(state.segments(), poses);
    state.set_aspect_ratio(state.aspect_ratio());
    assert_eq!(state.event_phase(), Some(EventPhase::Falling));
    state.set_aspect_ratio(0.75);
    assert_eq!(state.lifecycle(), EventLifecycle::Cooldown);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    trigger(&mut state);
    assert_eq!(state.event_id(), 1);
}

#[test]
fn off_stays_idle_and_repeated_manual_cycles_are_bounded() {
    let mut state = ready(ClockEventProfile::Off, 8);
    ticks(&mut state, 90 * 60);
    assert_eq!(state.event_id(), 0);
    assert_eq!(state.next_event_tick(), None);
    for event in 1..=8 {
        trigger(&mut state);
        assert_eq!(state.event_id(), event);
        assert!(state.body_count() <= 32);
        ticks(&mut state, FALLING_TICKS + REFORMING_TICKS + COOLDOWN_TICKS);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert_eq!(state.segments().len(), 28);
    }
}

#[test]
fn unsynchronized_clock_and_invalid_trigger_payloads_cannot_start_a_fall() {
    let mut state = ClockScenario::init(ClockConfig::default(), 1);
    trigger(&mut state);
    ticks(&mut state, 80 * 60);
    assert_eq!(state.event_id(), 0);
    assert_eq!(state.body_count(), 0);
    for payload in [
        vec![],
        vec![1],
        vec![2, 0],
        vec![1, 0, 255],
        vec![1, 0, 0, 0],
    ] {
        assert_eq!(
            ClockAction::decode(&Action::scenario(CLOCK_ACTION_TRIGGER_EVENT, payload)),
            None
        );
    }
}

#[test]
fn arena_side_walls_keep_falling_cell_geometry_inside_the_view() {
    for aspect in [5.0 / 3.0, 0.75] {
        let mut state = ready(ClockEventProfile::Off, 0);
        state.set_aspect_ratio(aspect);
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(11, 54, 19).unwrap(),
            )],
            Duration::ZERO,
        );
        trigger(&mut state);
        let layout = Layout::new(aspect);
        for tick in 0..FALLING_TICKS - 1 {
            ticks(&mut state, 1);
            for segment in state.segments() {
                let SegmentRepresentation::Rigid { position, angle } = segment.representation
                else {
                    continue;
                };
                let anchor = layout.segment_center(segment.id);
                for cell in digits::cells(segment.id.kind) {
                    let center = position
                        + (layout.cell_center(segment.id, *cell) - anchor).rotate_radians(angle);
                    let extent = layout.pitch * 0.4 * (angle.sin().abs() + angle.cos().abs());
                    // Contacts permit a small transient solver penetration;
                    // bars must not escape through the walls or tunnel outside.
                    assert!(
                        center.x - extent >= layout.bounds_min.x - 3.0
                            && center.x + extent <= layout.bounds_max.x + 3.0,
                        "aspect={aspect} tick={tick} segment={:?} center={center:?} extent={extent}",
                        segment.id
                    );
                }
            }
        }
    }
}

#[test]
fn timed_events_obey_the_same_lifecycle_and_cleanup_contract() {
    // Resident visitors have independent admission/cooldown contracts, covered
    // in autonomous_tests and crow_tests; neither occupies the timed-event slot.
    for definition in EVENT_CATALOG
        .into_iter()
        .filter(|d| !matches!(d.kind, ClockEventKind::Duck | ClockEventKind::Crow))
    {
        let kind = definition.kind;
        let mut state = ready(ClockEventProfile::Off, 9);
        let trigger = ClockAction::trigger_event(kind);
        assert_eq!(
            ClockAction::decode(&trigger),
            Some(ClockAction::TriggerEvent(kind))
        );
        ClockScenario::step(
            &mut state,
            &[trigger.clone(), trigger.clone()],
            Duration::ZERO,
        );
        assert_eq!(state.lifecycle(), EventLifecycle::Active);
        assert_eq!(state.event_kind(), Some(kind));
        assert_eq!((state.event_id(), state.phase_tick()), (1, 0));
        let partial = 60.min(definition.duration_ticks / 2);
        ticks(&mut state, partial);
        let frozen = ClockScenario::render_frame(&state);
        let tick = state.simulation_tick();
        ClockScenario::step(&mut state, std::slice::from_ref(&trigger), Duration::ZERO);
        assert_eq!(ClockScenario::render_frame(&state), frozen);
        assert_eq!(state.simulation_tick(), tick);
        assert_eq!(state.event_id(), 1);

        // Time correction during any event must not restore stale digits/color.
        let reading = ClockAction::set_reading(ClockReading::new(0, 1, 0).unwrap());
        ClockScenario::step(&mut state, std::slice::from_ref(&reading), Duration::ZERO);
        ticks(&mut state, definition.duration_ticks - partial);
        assert_eq!(state.lifecycle(), EventLifecycle::Cooldown);
        assert_eq!(state.event_kind(), None);
        assert_eq!(state.event_phase(), None);
        assert_eq!(state.palette(), DigitPalette::default());
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        let mut reference = ready(ClockEventProfile::Off, 9);
        ClockScenario::step(&mut reference, &[reading], Duration::ZERO);
        assert_eq!(state.segments(), reference.segments());
        assert_eq!(
            ClockScenario::render_frame(&state),
            ClockScenario::render_frame(&reference)
        );

        ClockScenario::step(&mut state, std::slice::from_ref(&trigger), Duration::ZERO);
        assert_eq!(state.event_id(), 1);
        ticks(&mut state, COOLDOWN_TICKS);
        assert!(state.can_trigger_event());
        // Manual previews bypass automatic enablement and per-event cooldowns.
        ClockScenario::step(&mut state, &[trigger], Duration::ZERO);
        assert_eq!(state.event_id(), 2);
        ticks(&mut state, partial);
        state.set_aspect_ratio(state.aspect_ratio());
        assert_eq!(state.event_kind(), Some(kind));
        state.set_aspect_ratio(0.75);
        assert_eq!(state.lifecycle(), EventLifecycle::Cooldown);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert_eq!(state.palette(), DigitPalette::default());
        assert!(
            state
                .segments()
                .iter()
                .all(|s| s.representation == SegmentRepresentation::Anchored)
        );
        assert_eq!(
            state.event_ready_at_tick(kind),
            state.simulation_tick() + definition.cooldown_ticks
        );
    }
}

#[test]
fn color_cycle_changes_only_appearance_and_keeps_live_time_without_physics() {
    let mut state = ready(ClockEventProfile::Off, 0);
    let normal = ClockScenario::render_frame(&state);
    ClockScenario::step(
        &mut state,
        &[ClockAction::trigger_event(ClockEventKind::ColorCycle)],
        Duration::ZERO,
    );
    assert_eq!(ClockScenario::render_frame(&state), normal);
    for tick in 1..COLOR_CYCLE_TICKS {
        ticks(&mut state, 1);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert!(
            state
                .segments()
                .iter()
                .all(|s| s.representation == SegmentRepresentation::Anchored)
        );
        let fill = state.palette().fill;
        assert_eq!(fill.a, 1.0);
        assert!(
            [fill.r, fill.g, fill.b]
                .into_iter()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(&c))
        );
        if tick == 90 {
            let colored = ClockScenario::render_frame(&state);
            assert_ne!(colored, normal);
            for (a, b) in colored
                .layers
                .iter()
                .flat_map(|l| &l.primitives)
                .zip(normal.layers.iter().flat_map(|l| &l.primitives))
            {
                match (a, b) {
                    (
                        engine_common::RenderPrimitive::Polygon(a),
                        engine_common::RenderPrimitive::Polygon(b),
                    ) => assert_eq!(a.points, b.points),
                    _ => assert_eq!(a, b, "corner lights stay unchanged during color cycling"),
                }
            }
            let reading = ClockAction::set_reading(ClockReading::new(0, 0, 0).unwrap());
            ClockScenario::step(&mut state, &[reading], Duration::ZERO);
            let mut expected = digits::create_segments();
            digits::apply_snapshot(&mut expected, state.display());
            assert_eq!(state.segments(), expected);
        }
    }
    ticks(&mut state, 1);
    assert_eq!(state.palette(), DigitPalette::default());
}

#[test]
fn mixed_events_replay_schedule_color_and_physics_exactly() {
    let config = ClockConfig {
        event_profile: ClockEventProfile::Demo,
        ..ClockConfig::default()
    };
    let mut a = ClockScenario::init(config, 42);
    let mut b = ClockScenario::init(config, 42);
    let reading = ClockAction::set_reading(ClockReading::new(23, 58, 0).unwrap());
    ClockScenario::step(&mut a, std::slice::from_ref(&reading), Duration::ZERO);
    ClockScenario::step(&mut b, &[reading], Duration::ZERO);
    let mut seen = [false; ClockEventKind::ALL.len()];
    for _ in 0..600 * 60 {
        ticks(&mut a, 1);
        ticks(&mut b, 1);
        assert_eq!(
            (
                a.lifecycle(),
                a.event_kind(),
                a.phase_tick(),
                a.next_event_tick()
            ),
            (
                b.lifecycle(),
                b.event_kind(),
                b.phase_tick(),
                b.next_event_tick()
            )
        );
        assert_eq!(a.segments(), b.segments());
        assert_eq!(a.palette(), b.palette());
        assert_eq!(a.meltdown_state(), b.meltdown_state());
        assert_eq!(a.duck_state(), b.duck_state());
        assert_eq!(a.crow_state(), b.crow_state());
        assert_eq!(a.marquee_state(), b.marquee_state());
        assert_eq!(a.cell_swap_state(), b.cell_swap_state());
        if let Some(kind) = a.event_kind() {
            seen[kind as usize] = true;
        }
        seen[ClockEventKind::Duck as usize] |= a.duck_state().is_some();
        seen[ClockEventKind::Crow as usize] |= a.crow_state().is_some();
    }
    for definition in EVENT_CATALOG {
        assert_eq!(
            seen[definition.kind as usize],
            definition.trigger == engine_common::ClockEventTrigger::Periodic,
            "{:?}",
            definition.kind
        );
    }
}

/// An even layer is a ground height, not an obstacle. A raised patch forced the
/// runner to jump beside a doorway, where the opened flap - an awning reaching
/// `radius*4.25` inward from its top hinge - struck it back down onto the dirt
/// until the visit timed out. These aspects and seeds all failed that way.
///
/// The layer is inset into the support that already holds actors up, so it must
/// never become what a runner stands on, and the runner must leave through its
/// own doorway with the layer delivered.
#[test]
fn clock_grass_contract_floor_layer_is_inset_and_never_supports_a_runner() {
    for (aspect, seeds) in [
        (1.0f32, [0u64, 3, 14]),
        (4.0 / 3.0, [3, 13, 25]),
        (2.0, [0, 8, 25]),
    ] {
        let layout = Layout::new(aspect);
        for seed in seeds {
            let mut s = ClockScenario::init(
                ClockConfig {
                    aspect_ratio: aspect,
                    event_profile: ClockEventProfile::Off,
                    ..Default::default()
                },
                seed,
            );
            ClockScenario::step(
                &mut s,
                &[
                    ClockAction::set_reading(ClockReading::new(23, 58, 0).unwrap()),
                    ClockAction::trigger_event(ClockEventKind::Grass),
                ],
                Duration::ZERO,
            );
            // The opening takes five seconds, then delivers its material.
            ticks(&mut s, 6 * 60);
            let soil = s.soil().expect("delivered layer");
            assert_eq!(
                s.grass_state().soil_retained,
                soil.terrain().width() as usize * crate::grass::SOIL_DEPTH_CELLS,
                "aspect={aspect}: the layer covers every supported column"
            );
            for quad in soil.quads() {
                assert!(
                    quad[2].y <= layout.floor_y + 0.01,
                    "aspect={aspect}: material stays inside the floor"
                );
            }
            let mut waited = 0;
            while !s.can_trigger_event() && waited < 2000 {
                ticks(&mut s, 1);
                waited += 1;
            }
            ClockScenario::step(
                &mut s,
                &[ClockAction::trigger_event(ClockEventKind::Duck)],
                Duration::ZERO,
            );
            let mut outcome = None;
            let mut soil_only_ticks = 0;
            for _ in 0..crate::events::duck::DUCK_TICKS + 200 {
                ticks(&mut s, 1);
                if let Some(duck) = s.duck_scene()
                    && let Some(world) = duck.existing_world()
                {
                    let mut ground = world
                        .surface_contacts(crate::events::duck::DUCK_COLLIDER)
                        .filter(|c| c.normal.y > 0.7 && c.separation <= duck.radius * 0.05)
                        .map(|c| c.collider.entity.value())
                        .peekable();
                    if ground.peek().is_some()
                        && ground.all(|id| id == crate::grass::SOIL_PHYSICS_ID)
                    {
                        soil_only_ticks += 1;
                    }
                }
                if let Some(o) = s.duck_state().and_then(|d| d.outcome) {
                    outcome = Some(o);
                    break;
                }
                if s.duck_visit.is_none() {
                    break;
                }
            }
            assert_eq!(
                soil_only_ticks, 0,
                "aspect={aspect} seed={seed}: the inset layer became the runner's support"
            );
            assert_eq!(
                outcome,
                Some(engine_common::ClockDuckOutcome::Exited),
                "aspect={aspect} seed={seed}: the runner left through its doorway"
            );
        }
    }
}

/// A ceiling breaks in one to several places, randomised per opening sequence.
/// Openings stay in the corridors beside the clock face: material covers the
/// whole floor, but a vertical beam through whichever segments form the current
/// hour does not sustain a patch, so an opening under the face grows nothing.
/// Each opening carries its own seeded patch, bounded to its own lit columns.
#[test]
fn clock_grass_contract_randomised_openings_each_grow_their_own_patch() {
    use crate::ceiling::{CORRIDORS, Ceiling, MAX_OPENINGS};
    let mut counts = std::collections::BTreeSet::new();
    for aspect in [0.25f32, 0.6, 4.0 / 3.0, 800.0 / 480.0, 4.0] {
        let layout = Layout::new(aspect);
        let corridors = Ceiling::corridors(layout);
        for &(start, end) in &corridors {
            assert!(start < end, "aspect={aspect}: every corridor is real floor");
        }
        for seed in 0..12u64 {
            let mut ceiling = Ceiling::default();
            assert!(ceiling.start(seed));
            ceiling.advance(crate::ceiling::OPENING_DURATION);
            let openings = ceiling.sources(layout);
            assert!(!openings.is_empty() && openings.len() <= MAX_OPENINGS);
            counts.insert(openings.len());
            for pair in openings.windows(2) {
                assert!(
                    pair[0].right < pair[1].left,
                    "aspect={aspect} seed={seed}: openings stay disjoint and ordered"
                );
            }
            for opening in &openings {
                assert!(opening.left < opening.right);
                assert!(
                    corridors.iter().any(|&(start, end)| {
                        opening.left >= start - 0.01 && opening.right <= end + 0.01
                    }),
                    "aspect={aspect} seed={seed}: an opening left its corridor"
                );
                // Beside the 6x9 font, never under it.
                assert!(
                    opening.right <= layout.face_origin.x + 0.01
                        || opening.left >= -layout.face_origin.x - 0.01
                );
            }
            // Identities are unique, so one publication accepts them all.
            let ids: std::collections::BTreeSet<_> =
                openings.iter().map(|a| (a.id.0, a.id.1)).collect();
            assert_eq!(ids.len(), openings.len());
            // The same sequence redraws the same holes after a resize.
            let resized = ceiling.sources(Layout::new(aspect * 1.1));
            assert_eq!(resized.len(), openings.len());
        }
    }
    assert!(
        counts.len() > 1 && counts.contains(&1),
        "the count varies and a single opening remains possible: {counts:?}"
    );

    // Each opening's patch establishes, and no tissue grows outside a lit
    // column. Several openings mean several organisms, within the resident cap.
    for seed in [1u64, 2, 4242] {
        let mut s = ClockScenario::init(
            ClockConfig {
                aspect_ratio: 800.0 / 480.0,
                event_profile: ClockEventProfile::Off,
                ..Default::default()
            },
            seed,
        );
        ClockScenario::step(
            &mut s,
            &[
                ClockAction::set_reading(ClockReading::new(0, 34, 0).unwrap()),
                ClockAction::trigger_event(ClockEventKind::Grass),
            ],
            Duration::ZERO,
        );
        ticks(&mut s, 180 * 60);
        let layout = Layout::new(s.aspect_ratio());
        let openings = s.ceiling.sources(layout);
        let g = s.grass_state();
        assert_eq!(g.seeded, crate::grass::SEED_BATCH, "seed={seed}");
        // Several openings introduce a refusal path the single aperture had
        // not: an overlap, a reused identity or a capacity overrun all cost
        // the whole publication, and the patches would go dark together.
        assert_eq!(
            g.optical_refusals, 0,
            "seed={seed}: every opening published"
        );
        assert!(
            g.development.iter().all(|v| *v > 0),
            "seed={seed}: every opening's patch is paid for: {g:?}"
        );
        // Each seed is its own patch, so the resident budget bounds the count
        // rather than the opening count.
        assert!(
            !g.organisms.is_empty()
                && g.organisms.len() <= crate::grass::residents::MAX_PLANT_ORGANISMS,
            "seed={seed}: patches stay inside the resident budget: {:?}",
            g.organisms
        );
        let soil = s.soil().unwrap();
        let lit: Vec<_> = openings
            .iter()
            .filter_map(|a| soil.columns_within(a.left, a.right))
            .collect();
        for tissue in &s.plants().plants.observe().tissues {
            assert!(
                lit.iter()
                    .any(|&(low, high)| (low..=high).contains(&tissue.cell[0])),
                "seed={seed}: tissue at column {} grew outside every lit opening",
                tissue.cell[0]
            );
        }
        assert!(openings.len() <= CORRIDORS * MAX_OPENINGS);
    }
}

#[test]
fn capture_frames_for_review() {
    let Some(dir) = std::env::var_os("SPACEWARS_CLOCK_FRAMES") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (label, seed, grass_seconds, duck) in [
        ("01-bare-floor", 4242u64, 0u64, false),
        ("02-just-delivered", 4242, 7, false),
        ("03-receding-30s", 4242, 36, false),
        ("04-settled-two-openings", 4242, 240, false),
        ("05-three-openings", 2, 240, false),
        ("06-duck-on-the-layer", 4242, 240, true),
        ("07-after-closing", 4242, 380, false),
        ("08-moss-succession", 4242, 560, false),
    ] {
        let mut s = ClockScenario::init(
            ClockConfig {
                aspect_ratio: 800.0 / 480.0,
                event_profile: ClockEventProfile::Off,
                ..Default::default()
            },
            seed,
        );
        ClockScenario::step(
            &mut s,
            &[ClockAction::set_reading(
                ClockReading::new(10, 42, 0).unwrap(),
            )],
            Duration::ZERO,
        );
        if grass_seconds > 0 {
            ClockScenario::step(
                &mut s,
                &[ClockAction::trigger_event(ClockEventKind::Grass)],
                Duration::ZERO,
            );
            ticks(&mut s, grass_seconds * 60);
        }
        if duck {
            let mut waited = 0;
            while !s.can_trigger_event() && waited < 2000 {
                ticks(&mut s, 1);
                waited += 1;
            }
            ClockScenario::step(
                &mut s,
                &[ClockAction::trigger_event(ClockEventKind::Duck)],
                Duration::ZERO,
            );
            ticks(&mut s, 600);
        }
        let frame = ClockScenario::render_frame(&s);
        let primitives: usize = frame.layers.iter().map(|l| l.primitives.len()).sum();
        println!(
            "{label}: phase={:?} openings={} living={} moss={} primitives={primitives}",
            s.ceiling.phase,
            s.ceiling.sources(Layout::new(s.aspect_ratio())).len(),
            s.grass_state().living_tissues,
            s.grass_state().moss_tissues,
        );
        std::fs::write(
            dir.join(format!("{label}.json")),
            serde_json::to_vec(&serde_json::json!({
                "label": label, "aspect_ratio": s.aspect_ratio(), "frame": frame,
                "grass": s.grass_state(),
                "openings": s.ceiling.sources(Layout::new(s.aspect_ratio()))
                    .iter().map(|a| [a.left, a.right]).collect::<Vec<_>>(),
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

/// Decorative cover is render-only: the delivered layer greens the whole floor
/// and recedes to the openings as unlit soil withers, so a hole reads as the
/// thing keeping its own patch alive. It leaves with the light.
#[test]
fn clock_grass_contract_ground_cover_greens_the_floor_then_recedes_to_the_openings() {
    use engine_common::RenderPrimitive;
    /// Horizontal extent of each cover strip. Strips are disjoint runs of
    /// columns, so their widths sum to the covered width of the floor.
    fn cover_strips(state: &ClockState) -> Vec<(f32, f32)> {
        ClockScenario::render_frame(state)
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter_map(|primitive| match primitive {
                RenderPrimitive::Polygon(polygon) => polygon
                    .fill
                    .filter(|fill| crate::render::COVER_COLORS.contains(&fill.color))
                    .map(|_| {
                        (
                            polygon.points.iter().map(|p| p.x).fold(f32::MAX, f32::min),
                            polygon.points.iter().map(|p| p.x).fold(f32::MIN, f32::max),
                        )
                    }),
                _ => None,
            })
            .collect()
    }
    let covered = |strips: &[(f32, f32)]| strips.iter().map(|(a, b)| b - a).sum::<f32>();

    let mut s = ClockScenario::init(
        ClockConfig {
            aspect_ratio: 800.0 / 480.0,
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        4242,
    );
    ClockScenario::step(
        &mut s,
        &[
            ClockAction::set_reading(ClockReading::new(10, 42, 0).unwrap()),
            ClockAction::trigger_event(ClockEventKind::Grass),
        ],
        Duration::ZERO,
    );
    // The opening takes five seconds; nothing is covered before it delivers.
    ticks(&mut s, 3 * 60);
    assert!(
        cover_strips(&s).is_empty(),
        "no cover before the material arrives"
    );

    ticks(&mut s, 4 * 60);
    let layout = Layout::new(s.aspect_ratio());
    let (floor_left, floor_right) = crate::ceiling::Ceiling::span(layout);
    let floor = floor_right - floor_left;
    let fresh = covered(&cover_strips(&s));
    assert!(
        fresh > floor * 0.9,
        "the delivered layer greens the whole floor: {fresh} of {floor}"
    );

    // Unlit cover withers; what remains sits over an opening.
    ticks(&mut s, 90 * 60);
    let strips = cover_strips(&s);
    let settled = covered(&strips);
    assert!(
        !strips.is_empty() && settled < fresh * 0.25,
        "cover receded to the openings: {settled} of {fresh}"
    );
    let openings = s.ceiling.sources(layout);
    for &(low, high) in &strips {
        assert!(
            openings
                .iter()
                .any(|a| a.left - crate::grass::SOIL_CELL_SIZE <= low
                    && high <= a.right + crate::grass::SOIL_CELL_SIZE),
            "a surviving strip ({low}, {high}) lies over no opening"
        );
    }

    // The cover leaves with the light rather than outliving the opening.
    ticks(&mut s, 400 * 60);
    assert_eq!(s.ceiling.phase, crate::ceiling::Phase::Closed);
    assert!(
        cover_strips(&s).is_empty(),
        "no cover once the ceiling has closed"
    );
}
