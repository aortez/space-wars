//! Visits of several crows, through the Clock's own state and step (#370).
use super::*;
use crate::crow::{CROW_TICKS, MAX_CROWS};
use engine_rapier::world::physics_activity;

/// By this age every crow of a visit has started for home.
const DEADLINE: u64 = 17 * 60;

fn tick(state: &mut ClockState) {
    ClockScenario::step(state, &[], Duration::from_millis(16));
}

fn clock(aspect: f32, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
            ..Default::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 34, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

/// A Clock at `aspect` with a visit of `size` crows.
fn visit(aspect: f32, seed: u64, size: usize) -> ClockState {
    let mut state = clock(aspect, seed);
    assert!(state.preview_crows(size));
    state
}

/// The members of the crows that have not left.
fn members(state: &ClockState) -> Vec<u8> {
    state.crow_states().iter().map(|crow| crow.member).collect()
}

/// Whether crow `member`'s body is in the arena.
fn has_body(state: &ClockState, member: usize) -> bool {
    state.arena.world().is_some_and(|world| {
        world
            .body_ids()
            .any(|id| id == crow::body::crow(member).id())
    })
}

/// Sends crow `member` of the visit home, and steps once, by which time it
/// has left.
fn send_home(state: &mut ClockState, member: usize) {
    state.crow_visit.as_mut().unwrap().send_home(member);
    tick(state);
}

fn cooldown() -> u64 {
    EVENT_CATALOG[ClockEventKind::Crow as usize].cooldown_ticks
}

/// Each crow of a visit has its member, a body of its own in the arena and a
/// stream of its own, and no two set out for the same perch. The first sets
/// out as a crow alone does.
#[test]
fn each_crow_of_a_visit_has_its_own_member_body_and_perch() {
    let alone = visit(5.0 / 3.0, 3, 1).crow_state();
    for size in 1..=MAX_CROWS {
        let mut state = visit(5.0 / 3.0, 3, size);
        let crows = state.crow_states();
        assert_eq!(members(&state), (0..size as u8).collect::<Vec<_>>());
        assert!(crows.iter().all(|crow| crow.visit_id == state.event_id()));
        assert_eq!(state.crow_state(), alone);
        let mut targets: Vec<_> = crows.iter().map(|crow| crow.target.unwrap()).collect();
        targets.sort_unstable();
        targets.dedup();
        assert_eq!(targets.len(), size, "{crows:?}");
        for member in 0..MAX_CROWS {
            assert_eq!(has_body(&state, member), member < size);
        }
        tick(&mut state);
        let perches = state.crow_visit.as_ref().unwrap().perch_count();
        let floor = state.arena.state().floor.bodies;
        assert_eq!(state.body_count(), size + perches + floor);
    }
}

/// Crows leave one at a time, first, middle or last. Each takes its own body
/// and no other: the rest keep their members and their age, the singular
/// crow is the first of them, and the visit keeps its admission, so the
/// events it blocks stay blocked, another crow visit is refused and the
/// cooldown waits. The last crow to leave ends the visit, and the arena and
/// the cooldown see it end once.
#[test]
fn crows_leave_one_at_a_time_and_the_last_ends_the_visit_once() {
    for order in [[0, 1, 2, 3], [2, 0, 3, 1], [3, 2, 1, 0]] {
        let mut state = visit(5.0 / 3.0, 11, MAX_CROWS);
        let mut settings = state.settings();
        settings.events = ClockEvents {
            crow: true,
            explosion: false,
            aquarium: false,
            cell_swap: false,
            grass: false,
            falling: false,
            color_cycle: false,
            meltdown: false,
            duck: false,
            marquee: false,
            digit_slide: false,
            rain: false,
            slot_machine: false,
            spider: false,
            coins: false,
            tanks: false,
        };
        state.configure(settings);
        for _ in 0..60 {
            tick(&mut state);
        }
        let ready_at = state.event_ready_at_tick(ClockEventKind::Crow);
        let accounting = state.arena_accounting();
        let mut remaining: Vec<u8> = (0..MAX_CROWS as u8).collect();
        for (left, member) in order.into_iter().enumerate() {
            let age = state.crow_state().unwrap().age_ticks;
            send_home(&mut state, member);
            remaining.retain(|other| usize::from(*other) != member);
            assert_eq!(members(&state), remaining, "{order:?}");
            assert!(!has_body(&state, member));
            if left + 1 == MAX_CROWS {
                break;
            }
            for crow in state.crow_states() {
                assert!(has_body(&state, usize::from(crow.member)));
                assert_eq!(crow.age_ticks, age + 1);
            }
            assert_eq!(state.crow_state().unwrap().member, remaining[0]);
            assert_eq!(state.event_ready_at_tick(ClockEventKind::Crow), ready_at);
            assert_eq!(state.arena_accounting().released, accounting.released);
            for kind in [ClockEventKind::Crow, ClockEventKind::Tanks] {
                assert!(state.event_blocked_by_crow(kind));
            }
            assert!(state.event_blocked_by_spider(ClockEventKind::Spider));
            assert!(state.automatic_events_suspended());
            let id = state.event_id();
            ClockScenario::step(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::Crow)],
                Duration::ZERO,
            );
            assert_eq!(state.event_id(), id);
            assert_eq!(state.event_notice.unwrap().0, "Crow is already visiting");
            assert!(!state.preview_crows(2));
            assert_eq!(members(&state), remaining);
        }
        assert!(state.crow_state().is_none() && state.crow_states().is_empty());
        assert!(!state.arena.holds(arena::Owner::Crow));
        assert_eq!(state.arena_accounting().released, accounting.released + 1);
        assert_eq!(
            state.event_ready_at_tick(ClockEventKind::Crow),
            state.simulation_tick() + cooldown()
        );
        assert!(!state.event_blocked_by_crow(ClockEventKind::Crow));
        assert!(!state.automatic_events_suspended());
    }
}

/// However many crows leave early, the rest start for home by the visit's
/// 17-second deadline and none outlasts its 22 seconds, and every crow is as
/// old as the visit. No two ever claim the same perch or spot.
#[test]
fn a_visit_of_crows_starts_home_by_its_deadline_and_ends_within_its_cap() {
    let mut early = 0;
    for aspect in [0.6, 800.0 / 480.0, 4.0] {
        for seed in 0..6 {
            let mut state = visit(aspect, seed, MAX_CROWS);
            let mut age = 0;
            let mut fewer = None;
            while !state.crow_states().is_empty() {
                let crows = state.crow_states();
                assert!(age < CROW_TICKS, "aspect={aspect} seed={seed}");
                assert!(crows.iter().all(|crow| crow.age_ticks == age));
                if age >= DEADLINE {
                    assert!(
                        crows.iter().all(|crow| crow.phase == crow::Phase::Leaving),
                        "aspect={aspect} seed={seed} age={age}: {crows:?}"
                    );
                }
                let mut targets: Vec<_> = crows.iter().filter_map(|crow| crow.target).collect();
                let mut spots: Vec<_> = crows
                    .iter()
                    .filter_map(|crow| crow.ground_target_milli)
                    .collect();
                let claimed = targets.len() + spots.len();
                targets.sort_unstable();
                targets.dedup();
                spots.sort_unstable();
                spots.dedup();
                assert_eq!(targets.len() + spots.len(), claimed, "{crows:?}");
                if crows.len() < MAX_CROWS {
                    fewer.get_or_insert(age);
                }
                tick(&mut state);
                age += 1;
            }
            assert!(!state.arena.holds(arena::Owner::Crow));
            early += usize::from(fewer.is_some_and(|age| age < DEADLINE));
        }
    }
    assert!(early > 0, "no crow left before the others");
}

/// Stepped without time, as while paused, a visit of crows holds still: no
/// crow ages, moves or leaves, though one of them has left already.
#[test]
fn a_visit_of_crows_holds_still_without_time() {
    let mut state = visit(5.0 / 3.0, 5, MAX_CROWS);
    for _ in 0..90 {
        tick(&mut state);
    }
    send_home(&mut state, 1);
    let before = state.crow_states();
    let flights: Vec<_> = state
        .crow_visit
        .as_ref()
        .unwrap()
        .crows()
        .map(|crow| crow.flight)
        .collect();
    for _ in 0..100 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(state.crow_states(), before);
    let after: Vec<_> = state
        .crow_visit
        .as_ref()
        .unwrap()
        .crows()
        .map(|crow| crow.flight)
        .collect();
    assert_eq!(after, flights);
}

/// A resize ends a visit of crows at once, with every crow's body, and
/// retires it once. Restarting or leaving Clock drops its world once, and
/// keeps no crow.
#[test]
fn a_resize_restart_or_exit_ends_a_visit_of_crows_once() {
    let mut state = visit(5.0 / 3.0, 9, MAX_CROWS);
    for _ in 0..60 {
        tick(&mut state);
    }
    send_home(&mut state, 1);
    let accounting = state.arena_accounting();
    state.set_aspect_ratio(0.6);
    assert!(state.crow_states().is_empty());
    assert!(!state.arena.holds(arena::Owner::Crow));
    assert_eq!(state.arena_accounting().released, accounting.released + 1);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    assert_eq!(
        state.event_ready_at_tick(ClockEventKind::Crow),
        state.simulation_tick() + cooldown()
    );

    // Leaving Clock drops the crows' world, once.
    assert!(state.preview_crows(MAX_CROWS));
    let before = physics_activity();
    drop(state);
    assert_eq!(physics_activity().dropped, before.dropped + 1);

    // A restart builds the new run before the old one goes, and the new run
    // has no crows.
    let old = visit(5.0 / 3.0, 9, MAX_CROWS);
    let before = physics_activity();
    let restarted = clock(5.0 / 3.0, 9);
    drop(old);
    assert_eq!(physics_activity().dropped, before.dropped + 1);
    assert!(restarted.crow_states().is_empty() && restarted.arena.idle());
}

/// Rain and a visit of crows keep their own lifecycles: the crows come into
/// Rain's arena with their bodies and nothing else, and a crow leaving early
/// changes nothing of Rain's, or of the schedule's event. Only the visit's
/// own cooldown may differ, from when its last crow leaves.
#[test]
fn a_visit_of_crows_and_rain_keep_their_own_lifecycles() {
    let start = || {
        let mut state = clock(1.6, 4);
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Rain)],
            Duration::ZERO,
        );
        for _ in 0..30 {
            tick(&mut state);
        }
        let rain = state.rain_state();
        let counts = (state.body_count(), state.collider_count());
        assert!(state.preview_crows(MAX_CROWS));
        assert_eq!(state.event_kind(), Some(ClockEventKind::Rain));
        assert_eq!(state.rain_state(), rain);
        assert_eq!(
            (state.body_count(), state.collider_count()),
            (counts.0 + MAX_CROWS, counts.1 + MAX_CROWS)
        );
        state
    };
    let (mut state, mut control) = (start(), start());
    send_home(&mut state, 2);
    tick(&mut control);
    let schedule = |state: &ClockState| {
        (
            state.event_kind(),
            state.event_id(),
            state.lifecycle(),
            state.event_phase(),
            state.phase_tick(),
            state.next_event_tick(),
        )
    };
    while state.event_kind() == Some(ClockEventKind::Rain) {
        assert_eq!(schedule(&state), schedule(&control));
        assert_eq!(state.rain_state(), control.rain_state());
        tick(&mut state);
        tick(&mut control);
    }
    assert_eq!(schedule(&state), schedule(&control));
}
