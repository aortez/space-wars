//! The standard for the run's physics arena: there is at most one, and every
//! tick that moves time steps it exactly once, whoever holds it. A visit, an
//! event sharing it, a lease left vacant and a resident such as soil all move
//! in that one step, and a tick that does not move time steps nothing.
//! Standalone events still step worlds of their own beside it, so the count
//! that proves this is the arena's, not the thread's.

use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState, arena::Owner};
use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
use engine_rapier::world::physics_activity;
use std::time::Duration;

const ASPECT: f32 = 800.0 / 480.0;
const SEED: u64 = 7;
const FRAME: Duration = Duration::from_nanos(16_666_667);

fn plain() -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: ASPECT,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        SEED,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(8, 8, 0).expect("a valid reading"),
        )],
        Duration::ZERO,
    );
    state
}

fn toggle(state: &mut ClockState, player: u8) {
    ClockScenario::step(
        state,
        &[ClockAction::toggle_player_duck(player)],
        Duration::ZERO,
    );
}

/// Advances `ticks` frames. Each must step the arena `arena` times and the
/// thread's worlds `worlds` times in all.
fn advance(state: &mut ClockState, ticks: usize, arena: u64, worlds: u64, what: &str) {
    for tick in 0..ticks {
        let before = (state.arena_accounting().steps, physics_activity().steps);
        ClockScenario::step(state, &[], FRAME);
        assert_eq!(
            state.arena_accounting().steps - before.0,
            arena,
            "{what}, tick {tick}: arena steps"
        );
        assert_eq!(
            physics_activity().steps - before.1,
            worlds,
            "{what}, tick {tick}: steps of every world"
        );
    }
}

#[test]
fn arena_contract_steps_once_per_tick_whoever_holds_it() {
    let mut state = plain();
    state.admit_soil();
    assert!(
        state.arena.holds(Owner::Soil),
        "delivered soil keeps the arena"
    );
    advance(&mut state, 30, 1, 1, "soil alone");

    // A standalone Falling steps its own world beside the arena until a
    // player joins, when the arena adopts that world.
    state.preview_event(ClockEventKind::Falling);
    advance(&mut state, 30, 1, 2, "soil beside a standalone Falling");
    toggle(&mut state, 1);
    assert!(state.arena.holds(Owner::Event(ClockEventKind::Falling)));
    advance(&mut state, 30, 1, 1, "a player joined to Falling");
    toggle(&mut state, 1);
    while state.duck_visit.is_some() {
        advance(&mut state, 1, 1, 1, "a dismissed player");
    }
    assert!(state.arena.vacant.is_some(), "the lease keeps the terrain");
    while state.event_kind().is_some() {
        advance(&mut state, 1, 1, 1, "a vacant lease");
    }
    assert_eq!(state.arena_accounting().claims, 1, "only the soil is left");
    advance(&mut state, 30, 1, 1, "soil alone again");

    // Events that share a visit's arena step in the visit's one step.
    for kind in [
        ClockEventKind::Aquarium,
        ClockEventKind::Explosion,
        ClockEventKind::Falling,
        ClockEventKind::Meltdown,
    ] {
        toggle(&mut state, 1);
        advance(&mut state, 90, 1, 1, "a player beside soil");
        state.preview_event(kind);
        assert_eq!(state.event_kind(), Some(kind));
        assert!(state.arena.holds(Owner::Event(kind)), "{kind:?} shares");
        advance(&mut state, 120, 1, 1, kind.as_str());
        state.finish_event();
        toggle(&mut state, 1);
        while state.duck_visit.is_some() {
            advance(&mut state, 1, 1, 1, "a departing player");
        }
    }

    // A tick that does not move time does not step anything.
    let before = (state.arena_accounting(), physics_activity().steps);
    for _ in 0..30 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(state.arena_accounting(), before.0);
    assert_eq!(physics_activity().steps, before.1);
}

#[test]
fn arena_contract_no_world_steps_while_nothing_holds_the_arena() {
    let mut state = plain();
    advance(&mut state, 30, 0, 0, "nothing holds the arena");
    toggle(&mut state, 1);
    advance(&mut state, 120, 1, 1, "a visit's own arena");
    toggle(&mut state, 1);
    while state.duck_visit.is_some() {
        let held = state.arena.world().is_some();
        advance(
            &mut state,
            1,
            u64::from(held),
            u64::from(held),
            "a departing visit",
        );
    }
    assert_eq!(state.arena_accounting().claims, 0);
    assert!(
        state.arena.world().is_none(),
        "the last claim takes the world"
    );
    advance(&mut state, 30, 0, 0, "nothing holds the arena again");
}
