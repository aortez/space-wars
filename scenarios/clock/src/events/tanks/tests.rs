use super::*;
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState, events::ActiveEvent,
};
use engine_common::{ClockEventProfile, ClockTimeFormat, Scenario};
use engine_rapier::world::physics_activity;
use std::time::Duration;

fn ready(aspect: f32, seed: u64, format: ClockTimeFormat) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
            time_format: format,
            ..Default::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(23, 59, 58).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

fn tick(state: &mut ClockState) {
    let before = physics_activity().steps;
    ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
    assert_eq!(physics_activity().steps - before, 1, "one canonical solve");
}

fn event(state: &mut ClockState) -> &mut TanksEvent {
    match state.active_event.as_mut().unwrap() {
        ActiveEvent::Tanks(event) => event,
        _ => panic!("Tanks"),
    }
}

#[test]
fn terrain_fills_the_width_and_transport_uses_current_glyph_gaps() {
    use engine_common::ClockFont;
    for aspect in [0.6, 4.0 / 3.0, 5.0 / 3.0, 4.0] {
        let layout = Layout::new(aspect);
        for font in ClockFont::ALL {
            for format in [ClockTimeFormat::TwelveHour, ClockTimeFormat::TwentyFourHour] {
                for (hour, minute) in [(1, 11), (8, 8), (12, 39), (23, 59)] {
                    let mut display = crate::digits::snapshot(
                        ClockReading::new(hour, minute, 0).unwrap(),
                        format,
                    );
                    display.font = font;
                    let place = placement(layout, display, None)
                        .unwrap_or_else(|| panic!("{aspect}, {font:?}, {display:?}"));
                    let width = place.terrain_width as f32 * 0.5 * place.scale;
                    assert!(
                        width
                            >= layout.bounds_max.x - layout.bounds_min.x - 2.0 * layout.frame_width
                    );
                    for x in place.spawn_x.map(|x| x * place.scale) {
                        assert!(entry::clear(layout, display, place.scale, x));
                    }
                }
            }
        }
    }
    let layout = Layout::new(4.0 / 3.0);
    let display = |hour, minute| {
        crate::digits::snapshot(
            ClockReading::new(hour, minute, 0).unwrap(),
            ClockTimeFormat::TwentyFourHour,
        )
    };
    let narrow = placement(layout, display(11, 11), None).unwrap();
    let wide = placement(layout, display(8, 8), None).unwrap();
    assert_ne!(
        narrow.spawn_x, wide.spawn_x,
        "the reading determines the landings"
    );
    let fallback = entry::lanes(layout, display(8, 8), None, 6.0).unwrap();
    assert!(
        fallback[0] < layout.face_origin.x,
        "the left outer margin is the fallback when the glyph gaps are too narrow"
    );

    let mut state = ready(4.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(11, 11, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    state.preview_event(ClockEventKind::Tanks);
    let ground = event(&mut state).ground;
    let (min, max) = state
        .arena
        .world()
        .unwrap()
        .body_solid_bounds(ground)
        .unwrap();
    assert!(min.x <= layout.bounds_min.x + layout.frame_width);
    assert!(max.x >= layout.bounds_max.x - layout.frame_width);
    for _ in 0..RAISE_TICKS + BEAM_DOWN_TICKS / 2 {
        tick(&mut state);
    }
    let tanks = state.tanks_state().unwrap();
    assert!(
        tanks
            .positions_milli
            .iter()
            .all(|p| p[1] as f32 / 1000.0 > layout.face_origin.y),
        "the tanks travel through the face from above"
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(8, 8, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    assert_eq!(state.event_phase(), Some(EventPhase::BeamDown));
    assert_eq!(
        state.phase_tick(),
        0,
        "a closed lane restarts transport from the top"
    );
    for _ in 0..BEAM_DOWN_TICKS + SETTLE_TICKS {
        if state.event_phase() == Some(EventPhase::Fighting) {
            break;
        }
        tick(&mut state);
    }
    assert_eq!(state.event_phase(), Some(EventPhase::Fighting));
    assert!(
        state
            .tanks_state()
            .unwrap()
            .supported
            .iter()
            .all(|supported| *supported)
    );
}

#[test]
fn entry_combat_and_departure_share_the_arena_and_keep_the_current_reading() {
    for (aspect, seed, format) in [
        (0.6, 42, ClockTimeFormat::TwelveHour),
        (4.0 / 3.0, 43, ClockTimeFormat::TwentyFourHour),
        (5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour),
        (4.0, 43, ClockTimeFormat::TwelveHour),
    ] {
        let mut state = ready(aspect, seed, format);
        state.preview_event(ClockEventKind::Tanks);
        assert_eq!(
            state.event_kind(),
            Some(ClockEventKind::Tanks),
            "aspect={aspect}, refusal={:?}",
            state.event_blocked_by_arena(ClockEventKind::Tanks)
        );
        let mut saw_fight = false;
        let mut shots = 0;
        let mut saw_ending = false;
        for t in 0..MAX_TICKS {
            if state.event_kind().is_none() {
                break;
            }
            let snapshot = state.tanks_state().unwrap();
            assert!(snapshot.entry_failure.is_none(), "{aspect}: {snapshot:?}");
            if matches!(
                state.event_phase(),
                Some(EventPhase::Raising | EventPhase::BeamDown | EventPhase::Settling)
            ) {
                assert_eq!(snapshot.shots, 0);
                assert_eq!(snapshot.combat_ticks, 0);
            }
            saw_fight |= state.event_phase() == Some(EventPhase::Fighting);
            saw_ending |= snapshot.result.is_some();
            shots = shots.max(snapshot.shots);
            assert!(snapshot.combat_ticks <= COMBAT_TICKS);
            if t == 200 {
                ClockScenario::step(
                    &mut state,
                    &[ClockAction::set_reading(
                        ClockReading::new(0, 0, 1).unwrap(),
                    )],
                    Duration::ZERO,
                );
            }
            if t % 120 == 0 {
                let frame = ClockScenario::render_frame(&state);
                assert_eq!(
                    frame,
                    ClockScenario::render_frame(&state),
                    "drawing never advances"
                );
                let before = state.tanks_state();
                ClockScenario::step(&mut state, &[], Duration::ZERO);
                assert_eq!(
                    state.tanks_state(),
                    before,
                    "pause freezes the entire phase"
                );
            }
            tick(&mut state);
        }
        assert!(
            saw_fight && saw_ending && shots > 0,
            "aspect={aspect}, fight={saw_fight}, ending={saw_ending}, shots={shots}"
        );
        assert!(state.event_kind().is_none(), "bounded exit");
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert!(state.arena.idle());
        assert_eq!(state.reading.unwrap().hour, 0);
    }
}

fn start_fighting(state: &mut ClockState) {
    state.preview_event(ClockEventKind::Tanks);
    for _ in 0..RAISE_TICKS + BEAM_DOWN_TICKS + SETTLE_TICKS {
        if state.event_phase() == Some(EventPhase::Fighting) {
            return;
        }
        tick(state);
    }
    panic!("entry did not settle: {:?}", state.tanks_state());
}

#[test]
fn visitors_and_player_ducks_are_excluded_in_both_directions() {
    for visitor in [
        ClockEventKind::Duck,
        ClockEventKind::Crow,
        ClockEventKind::Spider,
    ] {
        let mut state = ready(5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
        state.preview_event(visitor);
        let before = (
            state.event_id(),
            state.duck_state(),
            state.crow_state(),
            state.spider_state(),
        );
        state.preview_event(ClockEventKind::Tanks);
        assert!(state.tanks_state().is_none());
        assert_eq!(
            before,
            (
                state.event_id(),
                state.duck_state(),
                state.crow_state(),
                state.spider_state()
            )
        );
        assert!(state.event_notice.is_some());
    }
    let mut state = ready(5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    state.toggle_player_duck(1);
    let session = state.player_duck_session();
    state.preview_event(ClockEventKind::Tanks);
    assert_eq!(state.player_duck_session(), session);
    assert!(state.tanks_state().is_none());

    let mut state = ready(5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    state.preview_event(ClockEventKind::Tanks);
    let before = state.tanks_state();
    for visitor in [
        ClockEventKind::Duck,
        ClockEventKind::Crow,
        ClockEventKind::Spider,
    ] {
        state.preview_event(visitor);
        assert_eq!(state.tanks_state(), before);
    }
    state.toggle_player_duck(1);
    assert!(state.player_duck_session().is_none());
    assert_eq!(state.tanks_state(), before);
}

#[test]
fn placement_refusal_and_repeated_phase_cancellation_preserve_residents() {
    use crate::arena_fixture::FixtureBall;
    let mut blocked = ready(0.6, 42, ClockTimeFormat::TwentyFourHour);
    let floor = Layout::new(0.6).floor_y;
    let resident = blocked
        .admit_arena_fixture(&[FixtureBall {
            position: Vec2::new(0.0, floor + 10.0),
            velocity: Vec2::ZERO,
        }])
        .unwrap();
    let before = blocked.arena_fixture(resident);
    blocked.preview_event(ClockEventKind::Tanks);
    assert!(blocked.tanks_state().is_none());
    assert_eq!(
        blocked.event_blocked_by_arena(ClockEventKind::Tanks),
        Some(Unavailable::Placement)
    );
    assert_eq!(blocked.arena_fixture(resident), before);

    let mut state = ready(5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    let resident = state
        .admit_arena_fixture(&[FixtureBall {
            position: Vec2::new(355.0, 180.0),
            velocity: Vec2::ZERO,
        }])
        .unwrap();
    let world = state
        .arena
        .held_mut(Owner::Fixture(resident as u8))
        .unwrap();
    let entity = PhysicsId::new(canonical::FIXTURE_OWNER_BALLS[resident][0].start);
    world.set_body_kind(
        BodyId::new(entity, engine_rapier::world::BodyRole::PRIMARY),
        BodyKind::Fixed,
        false,
    );
    for phase in [
        EventPhase::Raising,
        EventPhase::BeamDown,
        EventPhase::Settling,
        EventPhase::Fighting,
        EventPhase::BeamUp,
        EventPhase::Withdrawing,
    ] {
        state.preview_event(ClockEventKind::Tanks);
        for _ in 0..MAX_TICKS {
            if state.event_phase() == Some(phase) {
                break;
            }
            if state.event_phase() == Some(EventPhase::Fighting) {
                event(&mut state).duel.set_firing(false);
                event(&mut state).combat_ticks = COMBAT_TICKS - 1;
            }
            tick(&mut state);
        }
        assert_eq!(state.event_phase(), Some(phase));
        let before = state.arena_fixture(resident);
        state.finish_event();
        assert_eq!(state.arena_fixture(resident), before);
        assert_eq!(state.arena.owners(), &[Owner::Fixture(resident as u8)]);
        assert!(
            !state
                .arena
                .world()
                .unwrap()
                .entity_ids()
                .any(|id| canonical::TANKS.contains(&id.value()))
        );
        assert_eq!(
            state.event_ready_at_tick(ClockEventKind::Tanks),
            state.schedule.tick + REUSE_TICKS
        );
    }
}

#[test]
fn timeout_waits_for_a_real_final_hit_then_converts_the_living_loser() {
    for seed in [42, 43] {
        let mut state = ready(5.0 / 3.0, seed, ClockTimeFormat::TwentyFourHour);
        start_fighting(&mut state);
        // A real in-flight shot crosses the cutoff. Move its opponent into
        // that sweep to make the final hit deterministic, with no HP injection.
        let shell = loop {
            if let Some(shell) = event(&mut state).duel.shells().find(|s| s.age >= 12) {
                break shell;
            }
            tick(&mut state);
            assert!(state.tanks_state().unwrap().combat_ticks < 300);
        };
        event(&mut state).duel.set_firing(false);
        event(&mut state).combat_ticks = COMBAT_TICKS - 1;
        tick(&mut state);
        assert_eq!(state.event_phase(), Some(EventPhase::FinishingShots));
        assert!(state.tanks_state().unwrap().result.is_none());
        let (body, original) = {
            let event = match state.active_event.as_ref().unwrap() {
                ActiveEvent::Tanks(event) => event,
                _ => unreachable!(),
            };
            let target = event
                .duel
                .tanks(state.arena.world().unwrap())
                .into_iter()
                .find(|tank| tank.body.entity != shell.source.owner)
                .unwrap();
            (target.body, target.motion)
        };
        let shell = event(&mut state)
            .duel
            .shells()
            .find(|s| s.source == shell.source)
            .unwrap();
        let world = state.arena.held_mut(OWNER).unwrap();
        world.set_pose(body, shell.position, 0.0, true);
        world.set_velocity(body, shell.velocity, 0.0, true);
        tick(&mut state);
        if state.arena.world().unwrap().motion(body).is_some() {
            state.arena.held_mut(OWNER).unwrap().set_pose(
                body,
                original.position,
                original.angle,
                true,
            );
        }
        for _ in 0..SHELL_LIFETIME_TICKS {
            if state.tanks_state().unwrap().result.is_some() {
                break;
            }
            tick(&mut state);
        }
        let snapshot = state.tanks_state().unwrap();
        assert!(
            snapshot.health_milli.iter().all(|hp| *hp > 0),
            "{snapshot:?}"
        );
        assert_ne!(
            snapshot.health_milli[0], snapshot.health_milli[1],
            "final hit changes an otherwise drawn cutoff"
        );
        assert_eq!(snapshot.converted_cells, DIRT_PER_TANK as u32);
        assert!(snapshot.loose_cells + snapshot.converted_cells <= MAX_GRAINS as u32);
        assert!(
            state.arena.world().unwrap().motion(body).is_none(),
            "living loser actually retired"
        );
        let grain = event(&mut state).converted[0].body();
        let pose = state.arena.world().unwrap().motion(grain).unwrap();
        tick(&mut state);
        assert_ne!(
            state.arena.world().unwrap().motion(grain).unwrap(),
            pose,
            "converted dirt participates in physics"
        );
        assert_eq!(
            state.tanks_state().unwrap().health_milli,
            snapshot.health_milli,
            "latched outcome"
        );
        state.finish_event();
        assert!(state.arena.idle());
    }
}

#[test]
fn health_outcomes_and_periodic_registration_cover_every_result() {
    for (health, expected) in [
        ([0.0, 0.0], ResultKind::Neither),
        ([1.0, 0.0], ResultKind::Left),
        ([0.0, 1.0], ResultKind::Right),
        ([53.0, 53.0], ResultKind::Draw),
        ([53.0, 54.0], ResultKind::Right),
    ] {
        assert_eq!(ResultKind::from_health(health), expected);
    }
    let definition = &crate::EVENT_CATALOG[ClockEventKind::Tanks as usize];
    assert_eq!(
        definition.trigger,
        engine_common::ClockEventTrigger::Periodic
    );
    assert_eq!(definition.cooldown_ticks, REUSE_TICKS);
    assert_eq!(MAX_TICKS, 84 * 60);
    for profile in [
        ClockEventProfile::Off,
        ClockEventProfile::Calm,
        ClockEventProfile::Demo,
    ] {
        let mut state = ready(5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
        state.config.event_profile = profile;
        let mut payload = match ClockAction::configure(state.settings()) {
            engine_common::Action::Scenario { payload, .. } => payload,
            _ => unreachable!(),
        };
        payload[4] = 0;
        payload[5] = 128;
        let settings = match ClockAction::decode(&engine_common::Action::scenario(
            crate::CLOCK_ACTION_CONFIGURE,
            payload,
        ))
        .unwrap()
        {
            ClockAction::Configure(settings) => settings,
            _ => unreachable!(),
        };
        state.configure(settings);
        if profile == ClockEventProfile::Off {
            assert!(state.next_event_tick().is_none());
        } else {
            let deadline = state.next_event_tick().unwrap();
            for _ in 0..=deadline {
                ClockScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            }
            assert_eq!(state.event_kind(), Some(ClockEventKind::Tanks));
        }
    }
}
