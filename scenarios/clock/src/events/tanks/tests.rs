use super::*;
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState, DIGIT_SLOT_COUNT,
    SegmentRepresentation,
    digit_surface_tests::{Loss, Markers, assert_face_agreement, lit_cells},
    digit_surfaces::{Retirement, SurfaceSource},
    events::{ActiveEvent, REFORMING_TICKS},
    fonts::CellMask,
};
use engine_common::{ClockEventProfile, ClockTimeFormat, Scenario};
use engine_rapier::world::{
    BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, RayCastOptions, physics_activity,
};
use scenario_scorched_earth::host::SCENERY;
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

fn tanks(state: &ClockState) -> &TanksEvent {
    match state.active_event.as_ref().unwrap() {
        ActiveEvent::Tanks(event) => event,
        _ => panic!("Tanks"),
    }
}

/// The middles of the polygons `frame` draws on `layer`.
fn middles(frame: &engine_common::RenderFrame, layer: i32) -> Vec<Vec2> {
    frame
        .layers
        .iter()
        .filter(|drawn| drawn.z == layer)
        .flat_map(|drawn| &drawn.primitives)
        .filter_map(|primitive| match primitive {
            engine_common::RenderPrimitive::Polygon(polygon) => Some(
                polygon
                    .points
                    .iter()
                    .fold(Vec2::ZERO, |sum, p| sum + Vec2::new(p.x, p.y))
                    / polygon.points.len() as f32,
            ),
            _ => None,
        })
        .collect()
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

#[test]
fn fights_pair_plotting_with_plotting_or_with_learning_and_open_from_either_side() {
    // Every 24 seeds, each pairing opens from either side with each mix of
    // dodgers: neither, either one, or both.
    let mut fights = Vec::new();
    for seed in 0..24 {
        let config = config(GrainShape::Round, seed);
        let pair = config.cpu_policy;
        assert!(pair.contains(&CpuPolicy::Plotted), "{seed}: {pair:?}");
        let fight = (pair, config.first_to_fire, config.cpu_dodging);
        assert!(!fights.contains(&fight), "{seed}: {fight:?}");
        fights.push(fight);
    }
}

#[test]
fn the_tanks_land_wide_and_hold_fire_until_they_stand_in_range() {
    // On the HyperPixel's 5:3, the gaps beside the colon would land the tanks
    // about 22 units apart. They land beside the outer digits instead, and
    // close in before either fires.
    let mut state = ready(5.0 / 3.0, 2, ClockTimeFormat::TwentyFourHour);
    start_fighting(&mut state);
    let apart = |state: &ClockState| {
        let event = tanks(state);
        let place = event.duel.placement();
        let [left, right] = event
            .duel
            .tanks(state.arena.world().unwrap())
            .map(|tank| place.point_from_world(tank.motion.position).x);
        right - left
    };
    assert!(apart(&state) > RANGE.far, "{}", apart(&state));
    while tanks(&state).fire_opened.is_none() {
        assert_eq!(state.tanks_state().unwrap().shots, 0);
        tick(&mut state);
    }
    assert!(RANGE.holds(apart(&state)), "{}", apart(&state));
    assert!(tanks(&state).fire_opened.unwrap() < OPENING_TICKS);
}

#[test]
fn the_face_stops_shells_in_front_of_it_and_plotted_shots_clear_it() {
    // On the Picade's 4:3 and the HyperPixel's 5:3 alike, the learning tank
    // on the left sends shells into the face and the plotting one on the
    // right shoots past it.
    for aspect in [4.0 / 3.0, 5.0 / 3.0] {
        face_stops_shells(aspect);
    }
}

fn face_stops_shells(aspect: f32) {
    let mut state = ready(aspect, 2, ClockTimeFormat::TwentyFourHour);
    start_fighting(&mut state);
    assert_eq!(
        state.tanks_state().unwrap().cpu,
        ["Learning + move", "Plotting + move"]
    );
    let layout = tanks(&state).layout;
    let scale = tanks(&state).duel.placement().scale;
    let mut face_hits = [0; 2];
    while state.tanks_state().unwrap().result.is_none() {
        assert!(tanks(&state).tick < MAX_TICKS);
        let before = ClockScenario::render_frame(&state);
        let shells: Vec<_> = tanks(&state).duel.shells().collect();
        tick(&mut state);
        let event = tanks(&state);
        let bodies = event
            .duel
            .tanks(state.arena.world().unwrap())
            .map(|tank| tank.body.entity);
        for impact in event.duel.impacts() {
            if impact.collider.entity != digits::BODY {
                continue;
            }
            let owner = bodies
                .iter()
                .position(|body| *body == impact.source.owner)
                .unwrap();
            face_hits[owner] += 1;
            // The shell burst on the cell its ray met.
            let cell = event.digits.cells()[usize::from(impact.collider.part)];
            let offset = impact.point - cell.center;
            assert!(
                offset.x.abs().max(offset.y.abs()) <= cell.half + 1e-3,
                "{offset:?} {cell:?}"
            );
            if face_hits[0] + face_hits[1] > 1 {
                continue;
            }
            // Before it did, it flew in front of the face, where combat
            // drawing used to stop, and its burst is drawn there too.
            let shell = shells
                .iter()
                .find(|shell| shell.source == impact.source)
                .unwrap();
            assert!(shell.position.y > layout.face_origin.y - 4.0, "{shell:?}");
            assert!(
                middles(&before, crate::render::SHOT_LAYER)
                    .iter()
                    .any(|middle| middle.distance_to(shell.position) < 1e-3),
                "{shell:?}"
            );
            assert!(
                middles(
                    &ClockScenario::render_frame(&state),
                    crate::render::SHOT_LAYER
                )
                .iter()
                .any(|middle| middle.distance_to(impact.point) < scale),
                "{impact:?}"
            );
        }
    }
    assert!(
        face_hits[0] > 0,
        "at {aspect}, the learning tank's shells reach the face"
    );
    assert_eq!(
        face_hits[1], 0,
        "at {aspect}, the plotting tank's shots clear it"
    );
    assert_eq!(state.tanks_state().unwrap().face_hits, face_hits[0]);
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
fn portrait_tanks_drive_inward_from_outer_entry_lanes() {
    let mut state = ready(0.6, 42, ClockTimeFormat::TwentyFourHour);
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(8, 8, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    start_fighting(&mut state);
    // Isolate driving from displacement caused by shell impacts.
    event(&mut state).duel.set_firing(false);
    let initial = state.tanks_state().unwrap().positions_milli;
    for _ in 0..10 * u64::from(FIXED_HZ) {
        tick(&mut state);
    }
    let positions = state.tanks_state().unwrap().positions_milli;
    for (before, after) in initial.into_iter().zip(positions) {
        assert!(
            after[0].abs() + 1000 < before[0].abs(),
            "tank must approach from its outer lane: {before:?} -> {after:?}"
        );
    }
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
            assert!(state.tanks_state().unwrap().combat_ticks < OPENING_TICKS + 300);
        };
        event(&mut state).duel.set_firing(false);
        event(&mut state).combat_ticks = COMBAT_TICKS - 1;
        tick(&mut state);
        assert_eq!(state.event_phase(), Some(EventPhase::FinishingShots));
        assert!(state.tanks_state().unwrap().result.is_none());
        // The target's hull and wheels move together, so the springs between
        // them hold.
        let (body, parts) = {
            let event = match state.active_event.as_ref().unwrap() {
                ActiveEvent::Tanks(event) => event,
                _ => unreachable!(),
            };
            let world = state.arena.world().unwrap();
            let slot = event
                .duel
                .tanks(world)
                .iter()
                .position(|tank| tank.body.entity != shell.source.owner)
                .unwrap();
            let parts: Vec<_> = event
                .duel
                .tank_bodies(slot)
                .map(|part| (part, world.motion(part).unwrap()))
                .collect();
            (parts[0].0, parts)
        };
        let shell = event(&mut state)
            .duel
            .shells()
            .find(|s| s.source == shell.source)
            .unwrap();
        let world = state.arena.held_mut(OWNER).unwrap();
        let hull = parts[0].1.position;
        for &(part, motion) in &parts {
            world.set_pose(
                part,
                shell.position + (motion.position - hull),
                motion.angle,
                true,
            );
            world.set_velocity(part, shell.velocity, 0.0, true);
        }
        tick(&mut state);
        if state.arena.world().unwrap().motion(body).is_some() {
            let world = state.arena.held_mut(OWNER).unwrap();
            for &(part, motion) in &parts {
                world.set_pose(part, motion.position, motion.angle, true);
            }
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
        // The living loser blows up as a dead one does.
        assert_eq!(state.event_phase(), Some(EventPhase::BlowingUp));
        assert!(
            (5..=wreck::PIECES_PER_TANK as u32).contains(&snapshot.pieces),
            "{snapshot:?}"
        );
        assert!(
            state.arena.world().unwrap().motion(body).is_none(),
            "living loser actually retired"
        );
        while state.event_phase() == Some(EventPhase::BlowingUp) {
            tick(&mut state);
        }
        let after = state.tanks_state().unwrap();
        assert_eq!(after.converted_cells, DIRT_PER_TANK as u32);
        assert!(after.loose_cells + after.converted_cells <= MAX_GRAINS as u32);
        let grain = event(&mut state).converted[0].body();
        assert_eq!(
            state.arena.world().unwrap().body_kind(grain),
            Some(BodyKind::Dynamic),
            "converted dirt participates in physics"
        );
        assert_eq!(after.health_milli, snapshot.health_milli, "latched outcome");
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
    assert_eq!(MAX_TICKS, 131 * 60);
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

/// Steps the fight to the tick its first shell bursts on the face, and
/// returns where it burst.
fn to_the_first_face_hit(state: &mut ClockState) -> Vec2 {
    loop {
        assert!(tanks(state).result.is_none(), "a shell reaches the face");
        tick(state);
        if let Some(impact) = tanks(state)
            .duel
            .impacts()
            .iter()
            .find(|impact| impact.collider.entity == digits::BODY)
        {
            return impact.point;
        }
    }
}

/// Whether every segment of `slot` keeps at least half its cells.
fn readable(state: &ClockState, slot: usize) -> bool {
    let loose = tanks(state).damage.loose_cells(slot);
    state
        .segments()
        .iter()
        .filter(|segment| usize::from(segment.id.digit_slot) == slot && segment.lit)
        .all(|segment| {
            let cells = segment.shape.cells().count();
            let lost = segment
                .shape
                .cells()
                .filter(|cell| loose.contains(*cell))
                .count();
            2 * lost <= cells
        })
}

#[test]
fn a_burst_knocks_loose_the_cells_beside_it_and_they_land_on_the_hills() {
    for aspect in [4.0 / 3.0, 5.0 / 3.0] {
        let mut state = ready(aspect, 2, ClockTimeFormat::TwentyFourHour);
        start_fighting(&mut state);
        let layout = tanks(&state).layout;
        let burst = to_the_first_face_hit(&mut state);
        let loose = tanks(&state).damage.blocks();
        assert!((1..=3).contains(&loose.len()), "at {aspect}: {loose:?}");
        for ((slot, cell), _) in &loose {
            let home = layout.slot_cell_center(usize::from(*slot), *cell);
            assert!(
                home.distance_to(burst) <= 1.2 * layout.pitch,
                "at {aspect}, {home:?} stood beside the burst at {burst:?}"
            );
            assert!(readable(&state, usize::from(*slot)), "at {aspect}");
        }
        // Each leaves a hole in the face that later shells fly through.
        tick(&mut state);
        for ((slot, cell), _) in &loose {
            let home = layout.slot_cell_center(usize::from(*slot), *cell);
            assert!(
                !tanks(&state)
                    .digits
                    .cells()
                    .iter()
                    .any(|box_| box_.center.distance_to(home) < 1e-3),
                "at {aspect}, a box stands where {home:?} came loose"
            );
        }
        let carried = state
            .digit_surfaces()
            .iter()
            .filter(|surface| surface.source == SurfaceSource::Rigid)
            .count();
        assert_eq!(carried, loose.len(), "each block carries its surface");
        // The blocks come to rest on the hills, where the duel's rays look
        // straight through them to the ground beneath. A later burst may
        // knock one about again, so each is judged the first time it rests.
        let mut still = vec![0; loose.len()];
        let mut rested = vec![false; loose.len()];
        for _ in 0..600 {
            tick(&mut state);
            assert_face_agreement(&state, &format!("at {aspect}"));
            let world = state.arena.world().unwrap();
            for (index, (_, body)) in loose.iter().enumerate() {
                if rested[index] {
                    continue;
                }
                let motion = world.motion(*body).expect("the block still lies loose");
                still[index] = if motion.linear_velocity.length() < 0.02 * layout.pitch {
                    still[index] + 1
                } else {
                    0
                };
                if still[index] < 10 {
                    continue;
                }
                rested[index] = true;
                let ground = world
                    .cast_ray(
                        motion.position + Vec2::Y * layout.pitch,
                        -Vec2::Y,
                        RayCastOptions {
                            max_distance: 4.0 * layout.pitch,
                            collision_groups: CollisionGroups {
                                memberships: u32::MAX,
                                filter: !SCENERY,
                            },
                            ..Default::default()
                        },
                    )
                    .expect("ground under the block");
                let height = (motion.position.y - ground.point.y) / layout.pitch;
                assert!(
                    (0.3..1.0).contains(&height),
                    "at {aspect}, a block rests {height} pitches over the ground"
                );
                let reach = motion.position.distance_to(
                    layout.slot_cell_center(usize::from(loose[index].0.0), loose[index].0.1),
                );
                assert!(
                    reach < 10.0 * layout.pitch,
                    "at {aspect}, a block came to rest {reach} from home"
                );
            }
        }
        assert!(
            rested.iter().all(|rested| *rested),
            "at {aspect}: {rested:?}"
        );
    }
}

#[test]
fn a_damaged_digit_re_forms_into_a_new_reading_from_where_its_cells_are() {
    let mut state = ready(5.0 / 3.0, 2, ClockTimeFormat::TwentyFourHour);
    start_fighting(&mut state);
    to_the_first_face_hit(&mut state);
    for _ in 0..120 {
        tick(&mut state);
    }
    let slot = (0..DIGIT_SLOT_COUNT)
        .find(|slot| tanks(&state).damage.standing()[*slot].is_some())
        .expect("a damaged digit");
    let loose = tanks(&state).damage.blocks();
    let before = state.digit_surfaces();
    // The reading changes the damaged digit of 23:59.
    let mut digits = [2, 3, 5, 9];
    digits[slot] -= 1;
    let reading =
        ClockReading::new(digits[0] * 10 + digits[1], digits[2] * 10 + digits[3], 0).unwrap();
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading)],
        Duration::ZERO,
    );
    assert_eq!(
        tanks(&state).damage.standing()[slot],
        Some(CellMask(0)),
        "the digit's cells are on their way home"
    );
    let world = state.arena.world().unwrap();
    for (_, body) in &loose {
        assert!(!world.contains_body(*body), "the blocks left the arena");
    }
    // They start from where they lay and stood.
    let now = state.digit_surfaces();
    for surface in before
        .iter()
        .filter(|surface| usize::from(surface.slot) == slot)
    {
        let moving = now
            .iter()
            .find(|moving| moving.id == surface.id)
            .expect("the cell is on its way home");
        assert_eq!(moving.source, SurfaceSource::Return);
        assert!(
            moving.origin.distance_to(surface.origin) < 1e-3,
            "{surface:?}"
        );
    }
    assert_face_agreement(&state, "the return's start");
    for step in 0..REFORMING_TICKS {
        tick(&mut state);
        assert_face_agreement(&state, &format!("{step} ticks into the return"));
    }
    assert!(tanks(&state).damage.whole());
    assert!(
        state
            .segments()
            .iter()
            .all(|segment| segment.representation == SegmentRepresentation::Anchored)
    );
    let fresh = ready(5.0 / 3.0, 2, ClockTimeFormat::TwentyFourHour);
    let mut fresh = fresh;
    ClockScenario::step(
        &mut fresh,
        &[ClockAction::set_reading(reading)],
        Duration::ZERO,
    );
    assert_eq!(
        lit_cells(&state),
        lit_cells(&fresh),
        "the face reads the new time"
    );
    // A cell that stood, and is lit in the new reading too, kept its surface
    // and its place.
    let after = state.digit_surfaces();
    let mut kept = 0;
    for surface in before.iter().filter(|surface| {
        usize::from(surface.slot) == slot && surface.source == SurfaceSource::Anchored
    }) {
        if let Some(home) = after
            .iter()
            .find(|home| home.origin.distance_to(surface.origin) < 1e-3)
        {
            assert_eq!(home.id, surface.id, "{surface:?}");
            kept += 1;
        }
    }
    assert!(kept > 0);
}

#[test]
fn every_loose_cell_rides_its_block_and_comes_home_before_the_tanks_leave() {
    let mut state = ready(5.0 / 3.0, 2, ClockTimeFormat::TwentyFourHour);
    let whole = lit_cells(&state);
    start_fighting(&mut state);
    let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
    let marked = markers.attached().count();
    let mut rode = 0;
    while state.active_event.is_some() {
        tick(&mut state);
        assert_face_agreement(&state, "the fight");
        let surfaces = markers.follow(&state);
        rode = rode.max(
            surfaces
                .iter()
                .filter(|surface| surface.source == SurfaceSource::Rigid && markers.on(surface.id))
                .count(),
        );
        if state.active_event.is_some() {
            assert!(tanks(&state).damage.blocks().len() <= MAX_LOOSE);
            for slot in 0..DIGIT_SLOT_COUNT {
                assert!(
                    readable(&state, slot),
                    "slot {slot} keeps half of each segment"
                );
            }
        }
    }
    assert!(rode > 0, "a marked cell was knocked loose");
    assert_eq!(
        markers.attached().count(),
        marked,
        "every marker came home on its cell"
    );
    assert_eq!(lit_cells(&state), whole);
    assert!(!state.arena.world().is_some_and(|world| {
        world
            .entity_ids()
            .any(|id| canonical::TANKS_CELLS.contains(&id.value()))
    }));
}

#[test]
fn a_fight_cut_short_keeps_what_stands_and_retires_what_lies_loose() {
    let mut state = ready(5.0 / 3.0, 2, ClockTimeFormat::TwentyFourHour);
    let whole = lit_cells(&state);
    start_fighting(&mut state);
    to_the_first_face_hit(&mut state);
    tick(&mut state);
    let surfaces = state.digit_surfaces();
    let loose: Vec<_> = surfaces
        .iter()
        .filter(|surface| surface.source == SurfaceSource::Rigid)
        .copied()
        .collect();
    let slot = loose[0].slot;
    let standing: Vec<_> = surfaces
        .iter()
        .filter(|surface| surface.slot == slot && surface.source == SurfaceSource::Anchored)
        .map(|surface| surface.id)
        .collect();
    assert!(!standing.is_empty());
    let mut markers = Markers::attach(&state, |_| Some(Vec2::ZERO));
    state.finish_event();
    ClockScenario::step(&mut state, &[], Duration::ZERO);
    markers.follow(&state);
    for surface in &loose {
        let marker = markers
            .markers
            .iter()
            .find(|marker| marker.surface == surface.id)
            .unwrap();
        assert_eq!(marker.lost, Some(Loss::Retired(Retirement::Interrupted)));
    }
    for id in standing {
        assert!(markers.on(id), "what stood stays");
    }
    assert_eq!(lit_cells(&state), whole, "the face draws the digit whole");
    assert_face_agreement(&state, "after the fight");
    assert!(!state.arena.world().is_some_and(|world| {
        world
            .entity_ids()
            .any(|id| canonical::TANKS_CELLS.contains(&id.value()))
    }));
}

/// Steps a fight on to its latched result.
fn to_the_result(state: &mut ClockState) {
    while tanks(state).result.is_none() {
        tick(state);
    }
}

/// Whether any of the losers' pieces is still in the arena.
fn pieces_remain(state: &ClockState) -> bool {
    state.arena.world().is_some_and(|world| {
        world
            .entity_ids()
            .any(|id| canonical::TANKS_PIECES.contains(&id.value()))
    })
}

#[test]
fn a_loser_blows_apart_and_each_piece_turns_to_dirt_where_it_comes_to_rest() {
    for (aspect, seed) in [(4.0 / 3.0, 43), (5.0 / 3.0, 42), (16.0 / 9.0, 7), (4.0, 43)] {
        let mut state = ready(aspect, seed, ClockTimeFormat::TwentyFourHour);
        start_fighting(&mut state);
        to_the_result(&mut state);
        assert_eq!(state.event_phase(), Some(EventPhase::BlowingUp));
        let event = tanks(&state);
        let scale = event.duel.placement().scale;
        let losers: Vec<usize> = (0..2)
            .filter(|slot| !event.result.unwrap().winners()[*slot])
            .collect();
        assert!(!losers.is_empty(), "{aspect}: a fight with a winner");
        // Each loser is gone, in its turret, both wheels and two or three
        // pieces of hull, all thrown up by the blast.
        let world = state.arena.world().unwrap();
        for &slot in &losers {
            assert!(event.duel.tank_bodies(slot).next().is_none());
            let pieces: Vec<_> = event
                .wreck
                .pieces
                .iter()
                .filter(|p| p.slot == slot)
                .collect();
            let count = |kind: fn(&wreck::Shape) -> bool| {
                pieces.iter().filter(|piece| kind(&piece.shape)).count()
            };
            assert_eq!(
                count(|shape| matches!(shape, wreck::Shape::Turret { .. })),
                1
            );
            assert_eq!(
                count(|shape| matches!(shape, wreck::Shape::Wheel { .. })),
                2
            );
            assert!((2..=3).contains(&count(|shape| matches!(shape, wreck::Shape::Hull { .. }))));
            assert_eq!(
                pieces.iter().map(|piece| piece.cells()).sum::<usize>(),
                DIRT_PER_TANK
            );
            for piece in &pieces {
                let motion = world.motion(piece.body).unwrap();
                assert!(
                    motion.linear_velocity.y > 2.0 * scale,
                    "{aspect}: {piece:?}"
                );
            }
        }
        assert_eq!(state.tanks_state().unwrap().converted_cells, 0);
        // Each piece turns into its share of the dirt where it lies, and
        // most come to rest first.
        let mut last = std::collections::HashMap::new();
        let (mut ticks, mut rested) = (0, 0);
        while state.event_phase() == Some(EventPhase::BlowingUp) {
            let before = tanks(&state).converted.len();
            {
                let world = state.arena.world().unwrap();
                for piece in &tanks(&state).wreck.pieces {
                    last.insert(piece.body, world.motion(piece.body).unwrap());
                }
            }
            tick(&mut state);
            ticks += 1;
            let event = tanks(&state);
            let world = state.arena.world().unwrap();
            let gone: Vec<_> = last
                .keys()
                .filter(|body| !event.wreck.pieces.iter().any(|piece| piece.body == **body))
                .copied()
                .collect();
            for grain in &event.converted[before..] {
                let at = world.motion(grain.body()).unwrap().position;
                assert!(
                    gone.iter()
                        .any(|body| last[body].position.distance_to(at) < 2.0 * scale),
                    "{aspect}: dirt at {at:?} where no piece lay"
                );
            }
            for body in gone {
                let motion = last.remove(&body).unwrap();
                rested += usize::from(motion.linear_velocity.length() < 0.5 * scale);
                assert!(world.motion(body).is_none());
            }
        }
        assert!(ticks <= BLOW_UP_TICKS, "{aspect}: {ticks}");
        let snapshot = state.tanks_state().unwrap();
        assert_eq!(snapshot.pieces, 0);
        assert_eq!(
            snapshot.converted_cells as usize,
            DIRT_PER_TANK * losers.len(),
            "{aspect}: no piece left the view"
        );
        assert!(
            rested >= 4 * losers.len(),
            "{aspect}: {rested} came to rest"
        );
        assert!(!pieces_remain(&state));
        assert_eq!(state.event_phase(), Some(EventPhase::BeamUp));
        while state.event_kind().is_some() {
            tick(&mut state);
        }
        assert!(state.arena.idle());
    }
}

#[test]
fn a_fight_with_no_survivors_blows_up_both_tanks_then_withdraws() {
    let mut state = ready(4.0 / 3.0, 69, ClockTimeFormat::TwentyFourHour);
    start_fighting(&mut state);
    to_the_result(&mut state);
    assert_eq!(tanks(&state).result, Some(ResultKind::Neither));
    assert_eq!(state.event_phase(), Some(EventPhase::BlowingUp));
    // Both tanks blow apart, each into six pieces here, which fill the room
    // the arena keeps for them.
    let event = tanks(&state);
    assert_eq!(event.wreck.blasts.len(), 2);
    assert_eq!(event.wreck.pieces.len(), wreck::MAX_PIECES);
    for slot in 0..2 {
        assert!(event.duel.tank_bodies(slot).next().is_none());
        assert_eq!(
            event
                .wreck
                .pieces
                .iter()
                .filter(|piece| piece.slot == slot)
                .map(|piece| piece.cells())
                .sum::<usize>(),
            DIRT_PER_TANK
        );
    }
    let mut ticks = 0;
    while state.event_phase() == Some(EventPhase::BlowingUp) {
        tick(&mut state);
        ticks += 1;
    }
    assert!(ticks <= BLOW_UP_TICKS, "{ticks}");
    let snapshot = state.tanks_state().unwrap();
    assert_eq!(snapshot.pieces, 0);
    assert_eq!(snapshot.converted_cells as usize, 2 * DIRT_PER_TANK);
    assert!(!pieces_remain(&state));
    // No winner beams up, so the hills withdraw at once.
    assert_eq!(state.event_phase(), Some(EventPhase::Withdrawing));
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn the_fire_over_a_blast_stays_below_the_face() {
    let mut state = ready(5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    start_fighting(&mut state);
    to_the_result(&mut state);
    let layout = tanks(&state).layout;
    let mut fire = 0;
    for _ in 0..BLOW_UP_TICKS {
        let frame = ClockScenario::render_frame(&state);
        for drawn in frame
            .layers
            .iter()
            .filter(|drawn| drawn.z == crate::render::SHOT_LAYER)
        {
            for primitive in &drawn.primitives {
                if let engine_common::RenderPrimitive::Polygon(polygon) = primitive {
                    fire += 1;
                    assert!(
                        polygon
                            .points
                            .iter()
                            .all(|p| p.y < layout.face_origin.y - 4.0)
                    );
                }
            }
        }
        tick(&mut state);
    }
    assert!(fire > 0, "the blast drew its fire");
}

#[test]
fn a_piece_that_leaves_the_view_retires_without_its_dirt() {
    let mut state = ready(5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    start_fighting(&mut state);
    to_the_result(&mut state);
    let layout = tanks(&state).layout;
    let turret = tanks(&state)
        .wreck
        .pieces
        .iter()
        .find(|piece| matches!(piece.shape, wreck::Shape::Turret { .. }))
        .unwrap();
    let (body, cells) = (turret.body, turret.cells());
    state.arena.held_mut(OWNER).unwrap().set_pose(
        body,
        Vec2::new(0.0, layout.bounds_min.y - 1.0),
        0.0,
        true,
    );
    tick(&mut state);
    assert!(state.arena.world().unwrap().motion(body).is_none());
    assert!(
        tanks(&state)
            .wreck
            .pieces
            .iter()
            .all(|piece| piece.body != body)
    );
    while state.event_phase() == Some(EventPhase::BlowingUp) {
        tick(&mut state);
    }
    assert_eq!(
        state.tanks_state().unwrap().converted_cells as usize,
        DIRT_PER_TANK - cells
    );
}

#[test]
fn a_loser_out_of_view_retires_without_blowing_up() {
    let mut state = ready(5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    start_fighting(&mut state);
    // Once a hit has parted the tanks' health and no shell is in flight, the
    // fight times out, so the weaker tank loses.
    let health = loop {
        let snapshot = state.tanks_state().unwrap();
        assert!(snapshot.result.is_none());
        let health = snapshot.health_milli;
        if health[0] != health[1] && tanks(&state).duel.shells().next().is_none() {
            break health;
        }
        tick(&mut state);
    };
    let slot = usize::from(health[1] < health[0]);
    event(&mut state).duel.set_firing(false);
    event(&mut state).combat_ticks = COMBAT_TICKS - 1;
    // The loser falls out of view before the result is latched.
    let layout = tanks(&state).layout;
    let parts: Vec<_> = {
        let world = state.arena.world().unwrap();
        tanks(&state)
            .duel
            .tank_bodies(slot)
            .map(|part| (part, world.motion(part).unwrap()))
            .collect()
    };
    let drop = Vec2::new(0.0, layout.bounds_min.y - 10.0 - parts[0].1.position.y);
    let world = state.arena.held_mut(OWNER).unwrap();
    for (part, motion) in &parts {
        world.set_pose(*part, motion.position + drop, motion.angle, true);
    }
    to_the_result(&mut state);
    let event = tanks(&state);
    assert!(!event.result.unwrap().winners()[slot]);
    assert!(
        event.duel.tank_bodies(slot).next().is_none(),
        "the loser retired"
    );
    assert!(event.wreck.blasts.is_empty() && event.wreck.pieces.is_empty());
    assert_eq!(state.tanks_state().unwrap().converted_cells, 0);
    assert!(!pieces_remain(&state));
    assert_eq!(state.event_phase(), Some(EventPhase::BeamUp));
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_fight_cut_short_while_a_loser_blows_up_leaves_nothing_behind() {
    for resize in [false, true] {
        let mut state = ready(5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
        start_fighting(&mut state);
        to_the_result(&mut state);
        for _ in 0..20 {
            tick(&mut state);
        }
        assert!(!tanks(&state).wreck.pieces.is_empty());
        if resize {
            state.set_aspect_ratio(4.0 / 3.0);
        } else {
            state.finish_event();
        }
        ClockScenario::step(&mut state, &[], Duration::ZERO);
        assert!(state.event_kind().is_none());
        assert!(!pieces_remain(&state));
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert!(state.arena.idle());
    }
}

#[test]
fn one_spaceling_in_five_gets_out_by_a_roll_of_its_own() {
    // Each of a fight's seeds pairs its CPUs, orders their fire and picks
    // its dodgers by the seed's remainder by 24. Each remainder's fights roll
    // alike, one in five for each side, half of those in a pod.
    let mut rolled = [[0u32; 3]; 24];
    for seed in 0..240_000u64 {
        let fight = seed.wrapping_add((ClockEventKind::Tanks as u64) << 32);
        let rolls = survivor::rolls(fight);
        assert_eq!(
            rolls,
            survivor::rolls(fight),
            "a seed's roll stays the same"
        );
        let counts = &mut rolled[(fight % 24) as usize];
        for roll in rolls {
            counts[0] += 1;
            counts[1] += u32::from(roll.is_some());
            counts[2] += u32::from(roll == Some(survivor::Ejection::Pod));
        }
    }
    for (remainder, [rolls, out, pods]) in rolled.into_iter().enumerate() {
        let rate = out as f32 / rolls as f32;
        assert!((rate - 0.2).abs() < 0.015, "{remainder}: {rate}");
        let pods = pods as f32 / out as f32;
        assert!((pods - 0.5).abs() < 0.04, "{remainder}: {pods}");
    }
    // The event rolls once, from its own seed, as it starts.
    let mut state = ready(5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    state.preview_event(ClockEventKind::Tanks);
    let event = tanks(&state);
    assert_eq!(event.survivors.rolls, survivor::rolls(event.seed));
    assert_eq!(
        state.tanks_state().unwrap().ejections,
        event.survivors.rolls
    );
}

/// A roll that draws `ending` from the endings that fit a fight with a
/// winner in view.
fn drawing(ending: survivor::Ending) -> f64 {
    match ending {
        survivor::Ending::RescueBeam => 0.1,
        survivor::Ending::Stowaway => 0.3,
        survivor::Ending::Surrender => 0.5,
        survivor::Ending::PartingShot => 0.7,
        survivor::Ending::Rebuild => 0.9,
    }
}

/// The fight at `seed` on an `aspect` screen, with every loser's spaceling
/// getting out by `ejection` and drawing `ending` if it fits, stepped on to
/// its result.
fn survival(
    aspect: f32,
    seed: u64,
    ejection: survivor::Ejection,
    ending: survivor::Ending,
) -> (ClockState, usize) {
    let mut state = ready(aspect, seed, ClockTimeFormat::TwentyFourHour);
    start_fighting(&mut state);
    event(&mut state).survivors.rolls = [Some(ejection); 2];
    event(&mut state).survivors.ending_rolls = [drawing(ending); 2];
    to_the_result(&mut state);
    let result = tanks(&state).result.unwrap();
    assert!(matches!(result, ResultKind::Left | ResultKind::Right));
    let loser = usize::from(result == ResultKind::Left);
    (state, loser)
}

fn survivor(state: &ClockState, slot: usize) -> &survivor::Survivor {
    tanks(state).survivors.get(slot).expect("a survivor")
}

/// Steps on until the survivor from `slot` enters `phase`, at most `limit`
/// ticks, and answers the ticks it took.
fn until_phase(state: &mut ClockState, slot: usize, phase: survivor::Phase, limit: u64) -> u64 {
    let mut ticks = 0;
    while survivor(state, slot).phase != phase {
        assert!(
            ticks < limit,
            "{:?} in place of {phase:?}",
            survivor(state, slot).phase
        );
        tick(state);
        ticks += 1;
    }
    ticks
}

fn survivors_remain(state: &ClockState) -> bool {
    state.arena.world().is_some_and(|world| {
        world
            .entity_ids()
            .any(|id| canonical::TANKS_SURVIVORS.contains(&id.value()))
    })
}

#[test]
fn a_thrown_spaceling_tumbles_clear_gets_up_waves_and_is_beamed_up() {
    use survivor::Phase;
    let (mut state, loser) = survival(
        4.0 / 3.0,
        43,
        survivor::Ejection::Thrown,
        survivor::Ending::RescueBeam,
    );
    let scale = tanks(&state).duel.placement().scale;
    let spec = survivor::spec(scale);
    // A third of the tank's height, thrown up and toward the middle,
    // tumbling faster than it can keep its feet.
    assert!((2.0 * spec.half_height() / scale - 1.0).abs() < 0.05);
    let motion = {
        let body = survivor(&state, loser).spaceling.as_ref().unwrap().body();
        state.arena.world().unwrap().motion(body).unwrap()
    };
    assert!(motion.linear_velocity.y > 5.0 * scale);
    assert!(motion.linear_velocity.x * motion.position.x < 0.0);
    assert!(motion.angular_velocity.abs() > spec.balance.knockdown_angular_speed);
    let block = state.tanks_state().unwrap().survivors[loser].unwrap();
    assert_eq!(
        (block.ejection, block.ending, block.phase),
        (
            survivor::Ejection::Thrown,
            engine_common::ClockTanksEnding::RescueBeam,
            Phase::Flying
        )
    );
    assert!(state.tanks_state().unwrap().survivors[1 - loser].is_none());
    tick(&mut state);
    {
        let world = state.arena.world().unwrap();
        let snapshot = survivor(&state, loser)
            .spaceling
            .as_ref()
            .unwrap()
            .snapshot(world)
            .unwrap();
        assert_eq!(
            snapshot.balance,
            engine_rapier::spaceling::SpacelingBalance::KnockedDown
        );
    }
    // It lands knocked down and gets up within 4 s of the blast.
    let flying = until_phase(&mut state, loser, Phase::GettingUp, 2 * 60);
    let down = until_phase(&mut state, loser, Phase::LookingAround, 4 * 60);
    assert!(flying + down < 4 * 60, "{flying} + {down}");
    let survivor_state = state.tanks_state().unwrap().survivors[loser].unwrap();
    assert!(survivor_state.stood_up && survivor_state.knockdowns >= 1);
    // It looks the other way, then back.
    let facing = survivor(&state, loser).facing;
    let (mut looked, mut away) = (0, 0);
    while survivor(&state, loser).phase == Phase::LookingAround {
        tick(&mut state);
        looked += 1;
        away += u64::from(survivor(&state, loser).facing != facing);
    }
    assert_eq!(looked, survivor::LOOK_TICKS);
    assert_eq!(away, survivor::LOOK_TICKS / 3);
    assert_eq!(survivor(&state, loser).facing, facing);
    assert_eq!(survivor(&state, loser).phase, Phase::Waving);
    // Once it has waved and the blow-up is over, a beam comes down on it as
    // the winner beams up.
    let waved = until_phase(
        &mut state,
        loser,
        Phase::BeamedUp,
        survivor::WAVE_TICKS + 3 * 60,
    );
    assert!(waved >= survivor::WAVE_TICKS);
    assert_eq!(state.event_phase(), Some(EventPhase::BeamUp));
    let body = survivor(&state, loser).spaceling.as_ref().unwrap().body();
    let from = state.arena.world().unwrap().motion(body).unwrap().position;
    assert_eq!(
        state.arena.world().unwrap().body_kind(body),
        Some(BodyKind::KinematicPosition)
    );
    while state.event_phase() == Some(EventPhase::BeamUp) {
        tick(&mut state);
    }
    // The beam has lifted it out, and it leaves with the winner.
    assert_eq!(state.event_phase(), Some(EventPhase::Withdrawing));
    assert!(!survivors_remain(&state));
    let block = state.tanks_state().unwrap().survivors[loser].unwrap();
    assert_eq!(block.phase, Phase::BeamedUp);
    assert!(block.position_milli[1] as f32 / 1000.0 > from.y + 9.0 * scale);
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_pod_arcs_out_lands_and_opens_and_its_spaceling_climbs_out() {
    use survivor::Phase;
    let (mut state, loser) = survival(
        5.0 / 3.0,
        42,
        survivor::Ejection::Pod,
        survivor::Ending::RescueBeam,
    );
    let scale = tanks(&state).duel.placement().scale;
    let pod = survivor(&state, loser).pod.unwrap();
    assert!(survivor(&state, loser).spaceling.is_none());
    let motion = state.arena.world().unwrap().motion(pod).unwrap();
    assert!(motion.linear_velocity.y > 8.0 * scale);
    assert!(motion.linear_velocity.x * motion.position.x < 0.0);
    assert_eq!(
        state.tanks_state().unwrap().survivors[loser].unwrap().phase,
        Phase::Sealed
    );
    // It comes down and opens once it has lain still a moment, by 2.5 s
    // after the blast at the latest.
    let sealed = until_phase(&mut state, loser, Phase::Opening, 150);
    let landed = state.arena.world().unwrap().motion(pod).unwrap();
    assert!(landed.linear_velocity.length() < 0.5 * scale || sealed == 150);
    assert!(landed.position.y < motion.position.y + scale);
    assert!(!tanks(&state).survivors.puffs.is_empty(), "smoke behind it");
    let mut hatch = 0.0;
    while survivor(&state, loser).phase == Phase::Opening {
        let open = survivor(&state, loser).hatch(tanks(&state).tick);
        assert!(open >= hatch);
        hatch = open;
        tick(&mut state);
    }
    assert!(hatch > 0.9);
    // Its spaceling climbs out over the side it faces, stands, looks around
    // and waves.
    assert_eq!(survivor(&state, loser).phase, Phase::ClimbingOut);
    let out = {
        let body = survivor(&state, loser).spaceling.as_ref().unwrap().body();
        state.arena.world().unwrap().motion(body).unwrap()
    };
    assert!(out.position.distance_to(landed.position) < 2.0 * scale);
    until_phase(&mut state, loser, Phase::LookingAround, 2 * 60);
    until_phase(&mut state, loser, Phase::BeamedUp, 5 * 60);
    assert!(survivor(&state, loser).stood_up);
    // The empty pod stays where it lies while the beam takes the spaceling,
    // then goes with the hills.
    let lying = state.arena.world().unwrap().motion(pod).unwrap().position;
    while state.event_phase() == Some(EventPhase::BeamUp) {
        tick(&mut state);
    }
    assert_eq!(state.event_phase(), Some(EventPhase::Withdrawing));
    assert!(survivor(&state, loser).spaceling.is_none());
    let world = state.arena.world().unwrap();
    assert!(world.motion(pod).unwrap().position.distance_to(lying) < 0.5 * scale);
    assert_eq!(world.body_kind(pod), Some(BodyKind::KinematicPosition));
    assert!(tanks(&state).departure.iter().any(|(body, _)| *body == pod));
    // It goes open, as it was left.
    assert_eq!(survivor(&state, loser).hatch(tanks(&state).tick), 1.0);
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_spaceling_or_pod_that_leaves_the_view_retires_there() {
    use survivor::Phase;
    for ejection in [survivor::Ejection::Thrown, survivor::Ejection::Pod] {
        let (mut state, loser) = survival(5.0 / 3.0, 42, ejection, survivor::Ending::RescueBeam);
        let layout = tanks(&state).layout;
        let body = match ejection {
            survivor::Ejection::Thrown => {
                survivor(&state, loser).spaceling.as_ref().unwrap().body()
            }
            survivor::Ejection::Pod => survivor(&state, loser).pod.unwrap(),
        };
        state.arena.held_mut(OWNER).unwrap().set_pose(
            body,
            Vec2::new(0.0, layout.bounds_min.y - 1.0),
            0.0,
            true,
        );
        tick(&mut state);
        assert_eq!(survivor(&state, loser).phase, Phase::Lost);
        assert!(!survivors_remain(&state));
        assert_eq!(
            state.tanks_state().unwrap().survivors[loser].unwrap().phase,
            Phase::Lost
        );
        // Nothing waits for it: the winner beams up once the pieces are dirt.
        while state.event_phase() == Some(EventPhase::BlowingUp) {
            tick(&mut state);
        }
        assert_eq!(state.event_phase(), Some(EventPhase::BeamUp));
        assert!(survivor(&state, loser).lifted.is_none());
        while state.event_kind().is_some() {
            tick(&mut state);
        }
        assert!(state.arena.idle());
    }
}

#[test]
fn a_spaceling_still_down_when_its_time_runs_out_is_lifted_out_as_it_lies() {
    use survivor::{Ending, Fallback, Phase};
    // A stowaway has longer, for its run, a parting shot longer still, and a
    // rebuild longest.
    for (ending, longest) in [
        (Ending::RescueBeam, survivor::RESCUE_BY_TICKS),
        (Ending::Stowaway, survivor::AFTERMATH_TICKS),
        (Ending::PartingShot, survivor::PARTING_TICKS),
        (Ending::Rebuild, survivor::REBUILD_BY_TICKS),
    ] {
        let (mut state, loser) = survival(4.0 / 3.0, 43, survivor::Ejection::Thrown, ending);
        assert_eq!(survivor(&state, loser).drawn, ending);
        let body = survivor(&state, loser).spaceling.as_ref().unwrap().body();
        // It lands, and something keeps it on its back.
        until_phase(&mut state, loser, Phase::GettingUp, 2 * 60);
        let mut aftermath = 0;
        while matches!(
            state.event_phase(),
            Some(EventPhase::BlowingUp | EventPhase::Aftermath)
        ) {
            aftermath += u64::from(state.event_phase() == Some(EventPhase::Aftermath));
            let world = state.arena.held_mut(OWNER).unwrap();
            let motion = world.motion(body).unwrap();
            world.set_pose(body, motion.position, std::f32::consts::FRAC_PI_2, true);
            world.set_velocity(body, Vec2::ZERO, 0.0, true);
            tick(&mut state);
        }
        // The aftermath waits for it as long as it may, then the beam takes
        // it as it lies, whatever it drew.
        assert_eq!(aftermath, longest, "{ending:?}");
        assert_eq!(state.event_phase(), Some(EventPhase::BeamUp));
        let survivor = survivor(&state, loser);
        assert_eq!(survivor.phase, Phase::BeamedUp);
        assert_eq!(survivor.ending, Ending::RescueBeam);
        let fallback = (ending != Ending::RescueBeam).then_some(Fallback::OutOfTime);
        assert_eq!(survivor.fallback, fallback);
        assert!(!survivor.stood_up);
        assert!(survivor.lifted.unwrap().1.angle.cos().abs() < 0.2);
        while state.event_kind().is_some() {
            tick(&mut state);
        }
        assert!(state.arena.idle());
    }
}

#[test]
fn a_fight_cut_short_at_any_stage_of_a_survivor_leaves_nothing_behind() {
    use survivor::{Ejection, Ending, Phase};
    let stages: [(Ejection, Ending, &[Phase]); 5] = [
        (
            Ejection::Thrown,
            Ending::RescueBeam,
            &[
                Phase::Flying,
                Phase::GettingUp,
                Phase::LookingAround,
                Phase::Waving,
                Phase::BeamedUp,
            ],
        ),
        (
            Ejection::Pod,
            Ending::RescueBeam,
            &[
                Phase::Sealed,
                Phase::Opening,
                Phase::ClimbingOut,
                Phase::LookingAround,
                Phase::Waving,
                Phase::BeamedUp,
            ],
        ),
        (
            Ejection::Thrown,
            Ending::Stowaway,
            &[Phase::Running, Phase::Climbing, Phase::Riding],
        ),
        (
            Ejection::Thrown,
            Ending::Surrender,
            &[Phase::Surrendering, Phase::Taken],
        ),
        (
            Ejection::Thrown,
            Ending::PartingShot,
            &[Phase::Covered, Phase::Dodging, Phase::Hopping],
        ),
    ];
    for (ejection, ending, phases) in stages {
        // Each stage, then the empty pod going with the hills.
        for stage in 0..=phases.len() {
            let (mut state, loser) = survival(5.0 / 3.0, 42, ejection, ending);
            if let Some(&phase) = phases.get(stage) {
                until_phase(&mut state, loser, phase, 10 * 60);
                for _ in 0..5 {
                    if survivor(&state, loser).phase == phase {
                        tick(&mut state);
                    }
                }
            } else {
                while state.event_phase() != Some(EventPhase::Withdrawing) {
                    tick(&mut state);
                }
                tick(&mut state);
            }
            assert!(
                survivors_remain(&state) || stage == phases.len() && ejection == Ejection::Thrown
            );
            if stage % 2 == 0 {
                state.set_aspect_ratio(4.0 / 3.0);
            } else {
                state.finish_event();
            }
            ClockScenario::step(&mut state, &[], Duration::ZERO);
            assert!(
                state.event_kind().is_none(),
                "{ejection:?} {ending:?} {stage}"
            );
            assert!(!survivors_remain(&state));
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            assert!(state.arena.idle());
        }
    }
}

/// The winner as it stands, in view.
fn winner(state: &ClockState) -> survivor::Winner {
    tanks(state)
        .winner(state.arena.world().unwrap())
        .expect("a winner in view")
}

/// A thin wall `height` of the duel's units high, from the ground halfway
/// between `at` and the winner, which meets only the spacelings and the
/// pods. Answers the wall and the top of it.
fn wall_between(state: &mut ClockState, at: Vec2, height: f32) -> (BodyId, Vec2) {
    let x = (at.x + winner(state).hull.position.x) / 2.0;
    wall(state, 0, Vec2::new(x, at.y), height)
}

/// A thin wall `height` of the duel's units high, from the ground under
/// `over`, which meets only the spacelings and the pods: the test's
/// `index`th obstacle. Answers the wall and the top of it.
fn wall(state: &mut ClockState, index: u64, over: Vec2, height: f32) -> (BodyId, Vec2) {
    let scale = tanks(state).duel.placement().scale;
    let ground = state
        .arena
        .world()
        .unwrap()
        .cast_ray(
            over + Vec2::Y * (5.0 * scale),
            Vec2::new(0.0, -1.0),
            RayCastOptions {
                collision_groups: wreck::GROUPS,
                ..Default::default()
            },
        )
        .unwrap()
        .point
        .y;
    // Sunk a unit into the ground.
    let half = Vec2::new(0.25, (height + 1.0) / 2.0) * scale;
    let middle = Vec2::new(over.x, ground + height * scale - half.y);
    (
        obstacle(state, index, middle, half),
        middle + Vec2::Y * half.y,
    )
}

/// A fixed box with its middle at `middle` and half its width and height
/// `half`, which meets only the spacelings and the pods: the test's
/// `index`th obstacle.
fn obstacle(state: &mut ClockState, index: u64, middle: Vec2, half: Vec2) -> BodyId {
    let body = BodyId::new(
        PhysicsId::new(crate::arena::ids::canonical::TEST_OBSTACLES.start + index),
        BodyRole::PRIMARY,
    );
    let mut stone = ColliderSpec::cuboid(
        ColliderId::new(body.entity, ColliderRole::PRIMARY, 0),
        half.x,
        half.y,
    );
    stone.collision_groups = CollisionGroups {
        memberships: 1 << 20,
        filter: wreck::GROUPS.memberships,
    };
    let world = state.arena.held_mut(OWNER).unwrap();
    assert!(world.insert_body(
        body,
        BodySpec {
            kind: BodyKind::Fixed,
            position: middle,
            ..Default::default()
        },
        &[stone],
    ));
    body
}

#[test]
fn a_survivor_draws_its_ending_evenly_from_those_that_fit_the_fight() {
    use survivor::{Ejection, Ending};
    // Without a winner in view only the rescue beam fits. With one, each of
    // the five takes a fifth of the rolls.
    for roll in [0.0, 0.2, 0.4, 0.6, 0.8, 0.999] {
        assert_eq!(survivor::ending(roll, false), Ending::RescueBeam);
    }
    let mut drawn = [0u32; 5];
    for seed in 0..60_000u64 {
        let fight = seed.wrapping_add((ClockEventKind::Tanks as u64) << 32);
        let rolls = survivor::ending_rolls(fight);
        assert_eq!(
            rolls,
            survivor::ending_rolls(fight),
            "a seed's roll stays the same"
        );
        for roll in rolls {
            assert!((0.0..1.0).contains(&roll));
            drawn[match survivor::ending(roll, true) {
                Ending::RescueBeam => 0,
                Ending::Stowaway => 1,
                Ending::Surrender => 2,
                Ending::PartingShot => 3,
                Ending::Rebuild => 4,
            }] += 1;
        }
    }
    for count in drawn {
        let share = count as f32 / 120_000.0;
        assert!((share - 1.0 / 5.0).abs() < 0.01, "{drawn:?}");
    }
    // The event rolls once, from its own seed, as it starts.
    let mut state = ready(5.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    state.preview_event(ClockEventKind::Tanks);
    let fight = tanks(&state);
    assert_eq!(
        fight.survivors.ending_rolls,
        survivor::ending_rolls(fight.seed)
    );
    // A fight that leaves no winner gives each spaceling the rescue beam,
    // whatever its roll, and waits no longer than a rescue takes.
    let mut state = ready(4.0 / 3.0, 69, ClockTimeFormat::TwentyFourHour);
    start_fighting(&mut state);
    event(&mut state).survivors.rolls = [Some(Ejection::Thrown); 2];
    event(&mut state).survivors.ending_rolls =
        [drawing(Ending::Stowaway), drawing(Ending::Surrender)];
    to_the_result(&mut state);
    assert_eq!(tanks(&state).result, Some(ResultKind::Neither));
    for slot in 0..2 {
        let survivor = survivor(&state, slot);
        assert_eq!(
            (survivor.drawn, survivor.ending),
            (Ending::RescueBeam, Ending::RescueBeam)
        );
    }
    assert_eq!(
        tanks(&state).survivors.aftermath_ticks(),
        survivor::RESCUE_BY_TICKS
    );
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_turret_turns_over_the_top_onto_its_target_at_a_turrets_pace() {
    let pivot = Vec2::new(3.0, 1.0);
    let target = pivot + Vec2::new(-5.0, -1.0);
    let mut muzzle = pivot + Vec2::new(2.0, 0.6);
    let most = survivor::TURRET_SPEED / FIXED_HZ as f32;
    let mut over = false;
    for _ in 0..120 {
        let from = (muzzle - pivot).normalized();
        let to = survivor::turn_barrel([pivot, muzzle], target);
        assert!((to.length() - 1.0).abs() < 1e-5);
        assert!(from.dot(to) >= most.cos() - 1e-5);
        // Up and over, never down through the hull.
        assert!(to.y > -0.25, "{to:?}");
        over |= to.y > 0.999;
        muzzle = pivot + to * 2.8;
    }
    assert!(over);
    let on = (muzzle - pivot).normalized();
    assert!(on.distance_to((target - pivot).normalized()) < 1e-4);
}

#[test]
fn a_stowaway_runs_to_the_winner_climbs_onto_its_roof_and_rides_it_out() {
    use survivor::{Ending, Phase};
    let (mut state, loser) = survival(5.0 / 3.0, 42, survivor::Ejection::Thrown, Ending::Stowaway);
    let scale = tanks(&state).duel.placement().scale;
    assert_eq!(survivor(&state, loser).drawn, Ending::Stowaway);
    // A stowaway's fight waits longer for it.
    assert_eq!(
        tanks(&state).survivors.aftermath_ticks(),
        survivor::AFTERMATH_TICKS
    );
    until_phase(&mut state, loser, Phase::LookingAround, 5 * 60);
    // It eyes the winner a moment, then runs, three times as fast as it
    // walks.
    let toward = (winner(&state).hull.position.x - survivor(&state, loser).position.x).signum();
    let mut spotting = 0;
    while survivor(&state, loser).phase == Phase::LookingAround {
        assert_eq!(survivor(&state, loser).facing, toward);
        tick(&mut state);
        spotting += 1;
    }
    assert_eq!(spotting, survivor::SPOT_TICKS);
    assert_eq!(survivor(&state, loser).phase, Phase::Running);
    let spaceling = survivor(&state, loser).spaceling.as_ref().unwrap();
    let body = spaceling.body();
    let spec = spaceling.spec();
    assert_eq!(spec.walk_speed, 3.0 * survivor::spec(scale).walk_speed);
    let mut fastest = 0.0f32;
    while survivor(&state, loser).phase == Phase::Running {
        let velocity = state
            .arena
            .world()
            .unwrap()
            .motion(body)
            .unwrap()
            .linear_velocity;
        fastest = fastest.max(velocity.x * toward);
        tick(&mut state);
    }
    assert!(fastest > 0.9 * spec.walk_speed, "{}", fastest / scale);
    // Beside the winner, it climbs on. The hull carries it from here, and
    // it pushes nothing.
    assert_eq!(survivor(&state, loser).phase, Phase::Climbing);
    {
        let [low, high] = winner(&state).bounds;
        let x = survivor(&state, loser).position.x;
        assert!((low.x - x).max(x - high.x) < 0.6 * scale);
        let world = state.arena.world().unwrap();
        assert_eq!(world.body_kind(body), Some(BodyKind::KinematicPosition));
    }
    // It came up on the barrel's side, so it climbs over the turret.
    let on_roof = |state: &ClockState| {
        let hull = winner(state).hull;
        let at = state.arena.world().unwrap().motion(body).unwrap().position;
        (at - hull.position).rotate_radians(-hull.angle) / scale
    };
    let [pivot, muzzle] = winner(&state).barrel;
    let aim = (muzzle - pivot)
        .rotate_radians(-winner(&state).hull.angle)
        .x
        .signum();
    assert_eq!(aim, -toward);
    let mut highest = f32::MIN;
    while survivor(&state, loser).phase == Phase::Climbing {
        tick(&mut state);
        highest = highest.max(on_roof(&state).y);
    }
    let top = (pivot - winner(&state).hull.position).y / scale + wreck::TURRET_RADIUS;
    assert!(highest > top, "{highest} {top}");
    // It rides on the roof behind the turret, between it and the back edge,
    // facing the turret.
    assert_eq!(survivor(&state, loser).phase, Phase::Riding);
    let riding = on_roof(&state);
    let half = winner(&state).half / scale;
    let height = survivor::spec(scale).half_height() / scale;
    assert!((riding.y - half.y - height).abs() < 0.05, "{riding:?}");
    assert!(
        riding.x.abs() > wreck::TURRET_RADIUS + 0.2 && riding.x.abs() < half.x,
        "{riding:?}"
    );
    assert_eq!(riding.x.signum(), -aim);
    assert_eq!(survivor(&state, loser).facing, aim);
    {
        let world = state.arena.world().unwrap();
        let at = world.motion(body).unwrap().position;
        let hit = world
            .cast_ray(
                at + Vec2::Y * (2.0 * scale),
                Vec2::new(0.0, -1.0),
                RayCastOptions::default(),
            )
            .expect("the hull under it");
        assert_ne!(hit.collider.entity, body.entity);
    }
    // It sits a moment, then the winner's beam carries it up with the hull.
    let mut sitting = 0;
    while state.event_phase() != Some(EventPhase::BeamUp) {
        tick(&mut state);
        sitting += 1;
        assert_eq!(survivor(&state, loser).phase, Phase::Riding);
        assert!(on_roof(&state).distance_to(riding) < 0.01);
    }
    assert_eq!(sitting, survivor::RIDE_TICKS);
    assert!(tanks(&state).departure.iter().any(|(id, _)| *id == body));
    while state.event_phase() == Some(EventPhase::BeamUp) {
        assert!(on_roof(&state).distance_to(riding) < 0.01);
        tick(&mut state);
    }
    // It leaves with the winner and leaves nothing behind.
    assert_eq!(state.event_phase(), Some(EventPhase::Withdrawing));
    assert!(!survivors_remain(&state));
    let block = state.tanks_state().unwrap().survivors[loser].unwrap();
    assert_eq!(
        (block.drawn, block.ending, block.fallback, block.phase),
        (Ending::Stowaway, Ending::Stowaway, None, Phase::Riding)
    );
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_stowaway_with_no_way_to_the_winner_waves_for_the_rescue_beam() {
    use survivor::{Ending, Fallback, Phase};
    let (mut state, loser) = survival(5.0 / 3.0, 42, survivor::Ejection::Thrown, Ending::Stowaway);
    let scale = tanks(&state).duel.placement().scale;
    until_phase(&mut state, loser, Phase::LookingAround, 5 * 60);
    // A wall higher than it jumps stands between it and the winner.
    let at = survivor(&state, loser).position;
    let (wall, _) = wall_between(&mut state, at, 3.5);
    // It sees it has no way, and waves for the rescue beam where it stands.
    until_phase(&mut state, loser, Phase::Waving, survivor::SPOT_TICKS + 1);
    let waving = survivor(&state, loser);
    assert_eq!(
        (waving.drawn, waving.ending, waving.fallback),
        (Ending::Stowaway, Ending::RescueBeam, Some(Fallback::NoWay))
    );
    assert!(waving.position.distance_to(at) < 0.5 * scale);
    state
        .arena
        .held_mut(OWNER)
        .unwrap()
        .remove_entity(wall.entity);
    until_phase(
        &mut state,
        loser,
        Phase::BeamedUp,
        survivor::WAVE_TICKS + survivor::AFTERMATH_TICKS,
    );
    // Its own beam takes it, not the winner's.
    let rescued = survivor(&state, loser);
    assert!(rescued.taken.is_none() && rescued.lifted.is_some());
    let block = state.tanks_state().unwrap().survivors[loser].unwrap();
    assert_eq!(
        (block.drawn, block.ending, block.fallback),
        (Ending::Stowaway, Ending::RescueBeam, Some(Fallback::NoWay))
    );
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_spaceling_that_surrenders_raises_a_white_flag_and_goes_up_beside_the_winner() {
    use survivor::{Ending, Phase};
    let (mut state, loser) = survival(5.0 / 3.0, 42, survivor::Ejection::Thrown, Ending::Surrender);
    let scale = tanks(&state).duel.placement().scale;
    assert_eq!(
        tanks(&state).survivors.aftermath_ticks(),
        survivor::AFTERMATH_TICKS
    );
    // It looks around, as one waiting for its beam does, then turns to the
    // winner and raises a white flag.
    until_phase(&mut state, loser, Phase::LookingAround, 5 * 60);
    until_phase(
        &mut state,
        loser,
        Phase::Surrendering,
        survivor::LOOK_TICKS + 1,
    );
    let toward = (winner(&state).hull.position.x - survivor(&state, loser).position.x).signum();
    assert_eq!(survivor(&state, loser).facing, toward);
    let barrel = |state: &ClockState| {
        let [pivot, muzzle] = winner(state).barrel;
        (muzzle - pivot).normalized()
    };
    let most = survivor::TURRET_SPEED / FIXED_HZ as f32;
    let (mut flag, mut aim, mut held) = (0.0, barrel(&state), 0);
    while survivor(&state, loser).phase == Phase::Surrendering {
        let raised = survivor(&state, loser).flag(tanks(&state).tick);
        assert!(raised >= flag);
        flag = raised;
        tick(&mut state);
        held += 1;
        // The winner's turret turns onto it at a turret's pace.
        let turned = barrel(&state);
        assert!(aim.dot(turned) >= most.cos() - 1e-4);
        aim = turned;
    }
    assert_eq!(flag, 1.0);
    // Its flag up a moment and the turret on it, the winner's beam takes
    // it, and it pushes nothing from here.
    assert!(held >= survivor::FLAG_TICKS + survivor::WAVE_TICKS);
    assert_eq!(state.event_phase(), Some(EventPhase::Aftermath));
    let taken = survivor(&state, loser);
    assert_eq!(taken.phase, Phase::Taken);
    let [pivot, _] = winner(&state).barrel;
    assert!(aim.dot((taken.position - pivot).normalized()) > 0.05_f32.cos());
    let (from, beside, _) = taken.taken.expect("taken by the winner's beam");
    assert_eq!(beside.x.signum(), -toward);
    assert!((beside.x.abs() - winner(&state).half.x - scale).abs() < 1e-3 * scale);
    let body = taken.spaceling.as_ref().unwrap().body();
    {
        let world = state.arena.world().unwrap();
        assert_eq!(world.body_kind(body), Some(BodyKind::KinematicPosition));
        let hit = world.cast_ray(
            from.position + Vec2::Y * (2.0 * scale),
            Vec2::new(0.0, -1.0),
            RayCastOptions::default(),
        );
        assert!(hit.is_none_or(|hit| hit.collider.entity != body.entity));
    }
    // The beam holds it where it stood while it comes down, then draws it
    // over to the winner's side, its flag still up and the turret on it.
    let mut drawn = 0;
    while state.event_phase() == Some(EventPhase::Aftermath) {
        tick(&mut state);
        drawn += 1;
        let taken = survivor(&state, loser);
        assert_eq!(taken.flag(tanks(&state).tick), 1.0);
        if drawn < survivor::TAKE_DESCENT_TICKS {
            assert!(taken.position.distance_to(from.position) < 1e-3 * scale);
        }
        // Its feet clear the ground on the way.
        if drawn > survivor::TAKE_DESCENT_TICKS + 10 && drawn < survivor::TAKE_TICKS - 10 {
            let below = state.arena.world().unwrap().cast_ray(
                taken.position,
                Vec2::new(0.0, -1.0),
                RayCastOptions {
                    collision_groups: wreck::GROUPS,
                    ..Default::default()
                },
            );
            let feet = survivor::spec(scale).half_height();
            assert!(below.is_none_or(|hit| hit.distance > feet), "{drawn}");
        }
        let turned = barrel(&state);
        assert!(aim.dot(turned) >= most.cos() - 1e-4);
        aim = turned;
    }
    assert_eq!(drawn, survivor::TAKE_TICKS);
    let hull = winner(&state).hull.position;
    let at = |state: &ClockState| state.arena.world().unwrap().motion(body).unwrap().position;
    assert!(at(&state).distance_to(hull + beside) < 1e-3 * scale);
    // It goes up beside the winner as the winner rises, and goes with it.
    assert_eq!(state.event_phase(), Some(EventPhase::BeamUp));
    assert!(tanks(&state).departure.iter().any(|(id, _)| *id == body));
    while state.event_phase() == Some(EventPhase::BeamUp) {
        let hull = winner(&state).hull.position;
        assert!(at(&state).distance_to(hull + beside) < 1e-3 * scale);
        tick(&mut state);
    }
    assert!(!survivors_remain(&state));
    let block = state.tanks_state().unwrap().survivors[loser].unwrap();
    assert_eq!(
        (block.drawn, block.ending, block.fallback, block.phase),
        (Ending::Surrender, Ending::Surrender, None, Phase::Taken)
    );
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn the_winners_beam_lifts_a_spaceling_it_takes_over_what_stands_between() {
    use survivor::{Ending, Phase};
    let (mut state, loser) = survival(5.0 / 3.0, 42, survivor::Ejection::Thrown, Ending::Surrender);
    let scale = tanks(&state).duel.placement().scale;
    until_phase(&mut state, loser, Phase::LookingAround, 5 * 60);
    until_phase(
        &mut state,
        loser,
        Phase::Surrendering,
        survivor::LOOK_TICKS + 1,
    );
    // A wall five units high stands between it and the winner, higher than
    // the least arc lifts it.
    let at = survivor(&state, loser).position;
    let (wall, top) = wall_between(&mut state, at, 5.0);
    until_phase(
        &mut state,
        loser,
        Phase::Taken,
        survivor::FLAG_TICKS + 2 * survivor::WAVE_TICKS,
    );
    // Where it crosses the wall's middle, between one tick and the next.
    let feet = survivor::spec(scale).half_height() + 0.2 * scale;
    let mut was = survivor(&state, loser).position;
    let mut crossings = Vec::new();
    while state.event_phase() == Some(EventPhase::Aftermath) {
        tick(&mut state);
        let at = survivor(&state, loser).position;
        if (was.x - top.x) * (at.x - top.x) <= 0.0 && was.x != at.x {
            let t = (top.x - was.x) / (at.x - was.x);
            crossings.push(was.y + (at.y - was.y) * t - feet);
        }
        was = at;
    }
    assert_eq!(crossings.len(), 1);
    assert!(
        crossings[0] >= top.y - 0.05 * scale,
        "{crossings:?} {top:?}"
    );
    assert_eq!(state.event_phase(), Some(EventPhase::BeamUp));
    state
        .arena
        .held_mut(OWNER)
        .unwrap()
        .remove_entity(wall.entity);
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

/// The parting shot at the survivor from `slot`.
fn parting_shot(state: &ClockState, slot: usize) -> &survivor::Shot {
    survivor(state, slot).shot.as_ref().expect("a parting shot")
}

/// How often the survivor from `slot` has been knocked down.
fn knockdowns(state: &ClockState, slot: usize) -> u32 {
    state.tanks_state().unwrap().survivors[slot]
        .unwrap()
        .knockdowns
}

#[test]
fn the_winner_takes_a_parting_shot_and_the_spaceling_dodges_it_and_runs_off_the_side() {
    use survivor::{Ending, Phase};
    let (mut state, loser) = survival(
        4.0 / 3.0,
        42,
        survivor::Ejection::Thrown,
        Ending::PartingShot,
    );
    let scale = tanks(&state).duel.placement().scale;
    let walker = survivor::spec(scale);
    assert_eq!(survivor(&state, loser).drawn, Ending::PartingShot);
    // A parting shot's fight waits longer still.
    assert_eq!(
        tanks(&state).survivors.aftermath_ticks(),
        survivor::PARTING_TICKS
    );
    // Up again, it looks around a moment, then faces the winner as the
    // winner covers it.
    until_phase(&mut state, loser, Phase::LookingAround, 5 * 60);
    assert_eq!(
        until_phase(&mut state, loser, Phase::Covered, survivor::SPOT_TICKS + 1),
        survivor::SPOT_TICKS
    );
    let toward = (winner(&state).hull.position.x - survivor(&state, loser).position.x).signum();
    assert_eq!(survivor(&state, loser).facing, toward);
    let stood = survivor(&state, loser).position;
    // The winner plots a shot at where it stands, its turret turning at a
    // turret's pace, and fires once the barrel lies along the arc it chose.
    // The duel stands still until then.
    let barrel = |state: &ClockState| {
        let [pivot, muzzle] = winner(state).barrel;
        (muzzle - pivot).normalized()
    };
    let most = survivor::TURRET_SPEED / FIXED_HZ as f32;
    let held = tanks(&state).duel.tick();
    let mut aim = barrel(&state);
    while parting_shot(&state, loser).fired.is_none() {
        assert_eq!(survivor(&state, loser).phase, Phase::Covered);
        assert_eq!(tanks(&state).duel.active_shells(), 0);
        assert_eq!(tanks(&state).duel.tick(), held);
        tick(&mut state);
        let turned = barrel(&state);
        // Firing sets the barrel on the arc.
        if parting_shot(&state, loser).fired.is_none() {
            assert!(aim.dot(turned) >= most.cos() - 1e-4);
        }
        aim = turned;
    }
    let fired = tanks(&state).tick;
    let direction = parting_shot(&state, loser).aim.direction().unwrap();
    assert!(barrel(&state).dot(direction) > 0.05_f32.cos());
    assert_eq!(tanks(&state).duel.active_shells(), 1);
    let shots = tanks(&state).duel.shots();
    // A moment later it sees the shell coming and traces where it bursts,
    // passing through it, beside where it stands. It runs clear at a
    // runner's pace, away from the winner, the way it will flee.
    assert_eq!(
        until_phase(
            &mut state,
            loser,
            Phase::Dodging,
            survivor::REACTION_TICKS + 1
        ),
        survivor::REACTION_TICKS
    );
    let burst = parting_shot(&state, loser).burst.expect("a traced burst");
    assert!(
        burst.distance_to(stood) < 3.0 * scale,
        "{burst:?} {stood:?}"
    );
    let dodge = parting_shot(&state, loser).dodge.expect("a dodge");
    assert_eq!(dodge.direction, -toward);
    let spec = |state: &ClockState| survivor(state, loser).spaceling.as_ref().unwrap().spec();
    assert_eq!(spec(&state).walk_speed, 3.0 * walker.walk_speed);
    // The duel steps while the shell flies, and its CPUs fire nothing.
    while parting_shot(&state, loser).landed.is_none() {
        assert_eq!(tanks(&state).duel.tick() - held, tanks(&state).tick - fired);
        assert_eq!(tanks(&state).duel.shots(), shots);
        tick(&mut state);
    }
    let (point, landed) = parting_shot(&state, loser).landed.unwrap();
    assert_eq!(tanks(&state).duel.tick() - held, landed - fired);
    // The shell bursts where it traced, and it ran clear, out of the
    // burst's reach. It takes the blast at a walk.
    let at = survivor(&state, loser).position;
    assert!((at.x - dodge.from) * dodge.direction >= dodge.needed);
    let point = point.expect("a burst");
    assert!(
        point.distance_to(burst) < 0.05 * scale,
        "{point:?} {burst:?}"
    );
    assert!(
        at.distance_to(point) > survivor::BLAST_REACH * scale,
        "{}",
        at.distance_to(point) / scale
    );
    assert_eq!(spec(&state).walk_speed, walker.walk_speed);
    // It watches the shell burst a moment, then runs for the side of the
    // battlefield away from the winner, which lies near enough, and is gone
    // once it is out of view. The duel stands still again.
    assert_eq!(
        until_phase(
            &mut state,
            loser,
            Phase::Fleeing,
            survivor::REACTION_TICKS + 1
        ),
        survivor::REACTION_TICKS
    );
    assert_eq!(survivor(&state, loser).facing, -toward);
    assert_eq!(spec(&state).walk_speed, 3.0 * walker.walk_speed);
    let held = tanks(&state).duel.tick();
    while survivor(&state, loser).phase == Phase::Fleeing {
        assert_eq!(tanks(&state).duel.tick(), held);
        tick(&mut state);
    }
    assert_eq!(survivor(&state, loser).phase, Phase::Fled);
    let layout = tanks(&state).layout;
    let edge = layout.bounds_max.x - layout.frame_width;
    assert!(survivor(&state, loser).position.x * -toward > edge);
    assert!(!survivors_remain(&state));
    // The winner goes up alone, at once.
    assert_eq!(state.event_phase(), Some(EventPhase::BeamUp));
    let block = state.tanks_state().unwrap().survivors[loser].unwrap();
    assert_eq!(
        (block.drawn, block.ending, block.fallback, block.phase),
        (Ending::PartingShot, Ending::PartingShot, None, Phase::Fled)
    );
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_spaceling_the_parting_shell_knocks_over_gets_up_and_rises_out_of_view_on_its_jetpack() {
    use survivor::{Ending, Phase};
    let (mut state, loser) = survival(
        16.0 / 9.0,
        42,
        survivor::Ejection::Thrown,
        Ending::PartingShot,
    );
    let scale = tanks(&state).duel.placement().scale;
    until_phase(&mut state, loser, Phase::Dodging, 6 * 60);
    while parting_shot(&state, loser).landed.is_none() {
        tick(&mut state);
    }
    // Out of the burst's reach, its blast still knocks the spaceling over,
    // and it gets up.
    let (point, _) = parting_shot(&state, loser).landed.unwrap();
    let at = survivor(&state, loser).position;
    assert!(at.distance_to(point.expect("a burst")) > survivor::BLAST_REACH * scale);
    let down = knockdowns(&state, loser);
    until_phase(&mut state, loser, Phase::GettingUp, 10);
    assert_eq!(knockdowns(&state, loser), down + 1);
    // Back on its feet, with the side too far to run for, it rises out of
    // view on its jetpack, faster and faster, drifting away from the winner
    // and leaving smoke behind it. It meets nothing on the way.
    until_phase(&mut state, loser, Phase::Hopping, 4 * 60);
    let away = parting_shot(&state, loser).away;
    let toward = (winner(&state).hull.position.x - survivor(&state, loser).position.x).signum();
    assert_eq!(away, -toward);
    assert_eq!(survivor(&state, loser).facing, away);
    let body = survivor(&state, loser).spaceling.as_ref().unwrap().body();
    assert_eq!(
        state.arena.world().unwrap().body_kind(body),
        Some(BodyKind::KinematicPosition)
    );
    let puffs = tanks(&state).survivors.puffs.len();
    let mut was = survivor(&state, loser)
        .hop
        .expect("where it took off")
        .position;
    let mut rise = 0.0;
    while survivor(&state, loser).phase == Phase::Hopping {
        tick(&mut state);
        let at = survivor(&state, loser).position;
        assert!(at.y - was.y > rise && (at.x - was.x) * away > 0.0);
        (was, rise) = (at, at.y - was.y);
    }
    assert!(tanks(&state).survivors.puffs.len() > puffs);
    // It is gone once it has risen out of view.
    assert_eq!(survivor(&state, loser).phase, Phase::Fled);
    let layout = tanks(&state).layout;
    assert!(was.y - survivor::spec(scale).half_height() > layout.canopy_y);
    assert!(!survivors_remain(&state));
    let block = state.tanks_state().unwrap().survivors[loser].unwrap();
    assert_eq!(
        (block.drawn, block.ending, block.fallback, block.phase),
        (Ending::PartingShot, Ending::PartingShot, None, Phase::Fled)
    );
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_spaceling_too_near_the_winner_for_a_parting_shot_waves_for_the_rescue_beam() {
    use survivor::{Ending, Fallback, Phase};
    let (mut state, loser) = survival(
        4.0 / 3.0,
        42,
        survivor::Ejection::Thrown,
        Ending::PartingShot,
    );
    let scale = tanks(&state).duel.placement().scale;
    let shots = tanks(&state).duel.shots();
    until_phase(&mut state, loser, Phase::LookingAround, 5 * 60);
    // Set down ten units from the winner's middle, its shell's burst would
    // reach the winner.
    let hull = winner(&state).hull.position;
    let side = (survivor(&state, loser).position.x - hull.x).signum();
    let x = hull.x + side * 10.0 * scale;
    let body = survivor(&state, loser).spaceling.as_ref().unwrap().body();
    let world = state.arena.held_mut(OWNER).unwrap();
    let ground = world
        .cast_ray(
            Vec2::new(x, hull.y + 8.0 * scale),
            Vec2::new(0.0, -1.0),
            RayCastOptions {
                collision_groups: wreck::GROUPS,
                exclude_entity: Some(body.entity),
                ..Default::default()
            },
        )
        .unwrap()
        .point
        .y;
    let feet = survivor::spec(scale).half_height() + 0.05 * scale;
    world.set_pose(body, Vec2::new(x, ground + feet), 0.0, true);
    world.set_velocity(body, Vec2::ZERO, 0.0, true);
    // The winner holds its fire, and the spaceling waves for the rescue
    // beam.
    until_phase(&mut state, loser, Phase::Waving, 4 * 60);
    let waving = survivor(&state, loser);
    assert_eq!(
        (waving.drawn, waving.ending, waving.fallback),
        (
            Ending::PartingShot,
            Ending::RescueBeam,
            Some(Fallback::NoShot)
        )
    );
    assert!(waving.shot.is_none());
    until_phase(
        &mut state,
        loser,
        Phase::BeamedUp,
        survivor::WAVE_TICKS + survivor::PARTING_TICKS,
    );
    assert_eq!(tanks(&state).duel.shots(), shots);
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_spaceling_up_too_late_for_a_parting_shot_waves_for_the_rescue_beam() {
    use survivor::{Ending, Fallback, Phase};
    let (mut state, loser) = survival(
        4.0 / 3.0,
        42,
        survivor::Ejection::Thrown,
        Ending::PartingShot,
    );
    let body = survivor(&state, loser).spaceling.as_ref().unwrap().body();
    // It lands, and something keeps it on its back until four seconds are
    // left.
    until_phase(&mut state, loser, Phase::GettingUp, 2 * 60);
    let mut aftermath = 0;
    while aftermath < survivor::PARTING_TICKS - 4 * 60 {
        aftermath += u64::from(state.event_phase() == Some(EventPhase::Aftermath));
        let world = state.arena.held_mut(OWNER).unwrap();
        let motion = world.motion(body).unwrap();
        world.set_pose(body, motion.position, std::f32::consts::FRAC_PI_2, true);
        world.set_velocity(body, Vec2::ZERO, 0.0, true);
        tick(&mut state);
    }
    // Up at last, it is covered, but the winner finds no time for the
    // shell's flight and the spaceling's escape, and the spaceling waves for
    // the rescue beam.
    until_phase(&mut state, loser, Phase::Covered, 3 * 60);
    until_phase(&mut state, loser, Phase::Waving, 60);
    let waving = survivor(&state, loser);
    assert_eq!(
        (waving.drawn, waving.ending, waving.fallback),
        (
            Ending::PartingShot,
            Ending::RescueBeam,
            Some(Fallback::OutOfTime)
        )
    );
    assert!(waving.shot.as_ref().unwrap().fired.is_none());
    until_phase(&mut state, loser, Phase::BeamedUp, 4 * 60);
    assert_eq!(tanks(&state).duel.active_shells(), 0);
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_spaceling_with_no_way_out_after_a_parting_shot_waves_for_the_rescue_beam() {
    use survivor::{Ending, Fallback, Phase};
    let (mut state, loser) = survival(
        4.0 / 3.0,
        42,
        survivor::Ejection::Thrown,
        Ending::PartingShot,
    );
    let scale = tanks(&state).duel.placement().scale;
    until_phase(&mut state, loser, Phase::Dodging, 6 * 60);
    while parting_shot(&state, loser).landed.is_none() {
        tick(&mut state);
    }
    // As the shell bursts, a wall higher than it jumps comes up between it
    // and the side, and a roof over it.
    let at = survivor(&state, loser).position;
    let away = parting_shot(&state, loser).away;
    let (wall, _) = wall(&mut state, 0, at + Vec2::X * (away * 4.0 * scale), 3.5);
    let roof = obstacle(
        &mut state,
        1,
        at + Vec2::Y * (3.0 * scale),
        Vec2::new(8.0, 0.25) * scale,
    );
    // It finds it has no way out, and waves for the rescue beam.
    until_phase(
        &mut state,
        loser,
        Phase::Waving,
        survivor::REACTION_TICKS + 1,
    );
    let waving = survivor(&state, loser);
    assert_eq!(
        (waving.drawn, waving.ending, waving.fallback),
        (
            Ending::PartingShot,
            Ending::RescueBeam,
            Some(Fallback::NoWay)
        )
    );
    let world = state.arena.held_mut(OWNER).unwrap();
    world.remove_entity(wall.entity);
    world.remove_entity(roof.entity);
    until_phase(
        &mut state,
        loser,
        Phase::BeamedUp,
        survivor::WAVE_TICKS + survivor::PARTING_TICKS,
    );
    let rescued = survivor(&state, loser);
    assert!(rescued.taken.is_none() && rescued.lifted.is_some());
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_parting_shell_still_in_the_air_holds_the_aftermath_until_it_lands() {
    use survivor::{Ending, Phase};
    let (mut state, loser) = survival(
        4.0 / 3.0,
        42,
        survivor::Ejection::Thrown,
        Ending::PartingShot,
    );
    let scale = tanks(&state).duel.placement().scale;
    until_phase(&mut state, loser, Phase::Dodging, 6 * 60);
    // Carried out of view as it dodges, the spaceling has fled before its
    // shell lands.
    let layout = tanks(&state).layout;
    let away = parting_shot(&state, loser).away;
    let at = survivor(&state, loser).position;
    let body = survivor(&state, loser).spaceling.as_ref().unwrap().body();
    let world = state.arena.held_mut(OWNER).unwrap();
    let side = away * (layout.bounds_max.x - layout.frame_width + 2.0 * scale);
    world.set_pose(body, Vec2::new(side, at.y), 0.0, true);
    tick(&mut state);
    assert_eq!(survivor(&state, loser).phase, Phase::Fled);
    assert!(!survivors_remain(&state));
    // The aftermath waits for the shell, and the duel steps it on, until it
    // lands. Then the winner goes up.
    while parting_shot(&state, loser).landed.is_none() {
        assert_eq!(state.event_phase(), Some(EventPhase::Aftermath));
        assert_eq!(tanks(&state).duel.active_shells(), 1);
        tick(&mut state);
    }
    assert_eq!(tanks(&state).duel.active_shells(), 0);
    assert_eq!(state.event_phase(), Some(EventPhase::BeamUp));
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

/// The fight at `seed` on an `aspect` screen, as `survival` sets it up for a
/// thrown spaceling that draws the rebuild, the winner seeing it go back to
/// its dirt if it `watched`.
fn rebuilding(aspect: f32, seed: u64, watched: bool) -> (ClockState, usize) {
    let (mut state, loser) = survival(
        aspect,
        seed,
        survivor::Ejection::Thrown,
        survivor::Ending::Rebuild,
    );
    event(&mut state).survivors.watched = watched;
    (state, loser)
}

/// The cells of the dirt the tank in `slot` blew apart into, as they lie.
fn dirt_of(state: &ClockState, slot: usize) -> Vec<rebuild::Cell> {
    let event = tanks(state);
    rebuild::pile(
        state.arena.world().unwrap(),
        &event.wreck.shares,
        &event.converted,
        slot,
    )
    .expect("all of its dirt")
}

/// The entities of `cells`, in order.
fn entities<'a>(cells: impl IntoIterator<Item = &'a rebuild::Cell>) -> Vec<u64> {
    let mut entities: Vec<u64> = cells
        .into_iter()
        .map(|cell| cell.body.entity.value())
        .collect();
    entities.sort_unstable();
    entities
}

/// The arena's world.
fn arena(state: &ClockState) -> &PhysicsWorld {
    state.arena.world().unwrap()
}

/// The winner's shot at the dirt of the tank in `slot`.
fn shot_at_dirt(state: &ClockState, slot: usize) -> &survivor::Shot {
    survivor(state, slot)
        .shot
        .as_ref()
        .expect("a shot at the dirt")
}

/// The side whose tank in `slot` wins.
fn win_for(slot: usize) -> ResultKind {
    if slot == 0 {
        ResultKind::Left
    } else {
        ResultKind::Right
    }
}

#[test]
fn a_spaceling_rebuilds_its_tank_from_its_dirt_and_the_fight_reopens_for_a_sudden_death() {
    use survivor::{Ending, Phase};
    let (mut state, loser) = rebuilding(5.0 / 3.0, 43, false);
    let scale = tanks(&state).duel.placement().scale;
    let winner = 1 - loser;
    assert_eq!(survivor(&state, loser).drawn, Ending::Rebuild);
    // A rebuild's fight waits longer for it.
    assert_eq!(
        tanks(&state).survivors.aftermath_ticks(),
        survivor::REBUILD_BY_TICKS
    );
    // Up again, it looks about until every piece of its tank has turned to
    // dirt. Then it runs to stand beside the dirt, on its own side of where
    // the tank will stand and clear of its wheels.
    until_phase(&mut state, loser, Phase::LookingAround, 5 * 60);
    until_phase(&mut state, loser, Phase::Returning, 5 * 60);
    assert!(tanks(&state).wreck.all_dirt(loser));
    let pile = survivor(&state, loser).pile.expect("dirt to rebuild from");
    let side = (pile.stand - pile.hull.x).signum();
    assert_eq!(
        side,
        (survivor(&state, loser).position.x - pile.hull.x).signum()
    );
    assert!(side * (pile.stand - pile.hull.x) > (rebuild::TANK_HALF_WIDTH + 0.5) * scale);
    // The tank will stand in the middle of its dirt, all 24 cells of it.
    let cells = dirt_of(&state, loser);
    assert_eq!(cells.len(), DIRT_PER_TANK);
    let across = cells.iter().map(|cell| cell.position.x).sum::<f32>() / cells.len() as f32;
    assert!((pile.hull.x - across).abs() < 1e-3 * scale);
    // It stops beside the dirt, and the dirt sets off home.
    until_phase(&mut state, loser, Phase::Rebuilding, 5 * 60);
    assert!((survivor(&state, loser).position.x - pile.stand).abs() < scale);
    let rebuilding = tanks(&state)
        .rebuild
        .as_ref()
        .expect("the dirt crawling home");
    assert_eq!(
        (rebuilding.slot, rebuilding.hull, rebuilding.stage),
        (loser, pile.hull, Stage::Crawling)
    );
    let since = rebuilding.since;
    let done = since + rebuild::REBUILD_TICKS;
    let order: Vec<rebuild::Cell> = rebuilding.cells().copied().collect();
    assert_eq!(entities(&order), entities(&cells));
    // Cell by cell, the wheels' first and the turret's last, the dirt crawls
    // home over the next four seconds. A cell on its way passes everything.
    // The spaceling waits, and the winner lets the dirt be.
    let mut set_off: Vec<Option<u64>> = vec![None; order.len()];
    while tanks(&state).tick < done - 1 {
        assert_eq!(survivor(&state, loser).phase, Phase::Rebuilding);
        assert_eq!(tanks(&state).duel.active_shells(), 0);
        for (index, cell) in order.iter().enumerate() {
            let kind = arena(&state).body_kind(cell.body);
            match set_off[index] {
                None if kind == Some(BodyKind::KinematicPosition) => {
                    set_off[index] = Some(tanks(&state).tick);
                }
                None => assert_eq!(kind, Some(BodyKind::Dynamic)),
                Some(_) => {
                    assert_eq!(kind, Some(BodyKind::KinematicPosition));
                    assert!(arena(&state).contact_points(cell.collider).is_empty());
                }
            }
        }
        tick(&mut state);
    }
    let set_off: Vec<u64> = set_off
        .into_iter()
        .map(|tick| tick.expect("every cell on its way"))
        .collect();
    assert!(set_off[..8].iter().max() < set_off[DIRT_PER_TANK - 4..].iter().min());
    // By the last tick of the crawl the cells stand together as the tank.
    for cell in &order {
        let at = arena(&state).motion(cell.body).unwrap().position - pile.hull;
        assert!(
            at.x.abs() < 2.0 * scale && at.y.abs() < 1.6 * scale,
            "{:?}",
            at / scale
        );
    }
    // Then the cells are the tank again, where the dirt said it would stand,
    // battered, with a quarter of its armor, in puffs of dust. The winner
    // keeps its own armor.
    let health = tanks(&state).duel.tanks(arena(&state))[winner].health;
    tick(&mut state);
    assert_eq!(
        tanks(&state).rebuild.as_ref().unwrap().stage,
        Stage::Rebuilt
    );
    for cell in &cells {
        assert!(arena(&state).motion(cell.body).is_none());
    }
    assert!(
        tanks(&state)
            .converted
            .iter()
            .all(|grain| cells.iter().all(|cell| cell.body != grain.body()))
    );
    assert!(
        !tanks(&state)
            .wreck
            .shares
            .iter()
            .any(|share| share.slot == loser)
    );
    let tank = tanks(&state).duel.tanks(arena(&state))[loser];
    assert!(tank.present);
    assert_eq!(tank.health, rebuild::REBUILT_HEALTH);
    assert!(tank.motion.position.distance_to(pile.hull) < 0.05 * scale);
    assert!(tank.motion.angle.cos() > 0.999);
    let tanks_now = tanks(&state).duel.tanks(arena(&state));
    assert_eq!(tanks_now[winner].health, health);
    let puffs = tanks(&state)
        .wreck
        .puffs
        .iter()
        .filter(|puff| puff.tick == done)
        .count();
    assert_eq!(puffs, 5);
    // The spaceling climbs up the tank's side, onto the turret and down
    // through the hatch, passing everything, and is gone inside.
    assert_eq!(survivor(&state, loser).phase, Phase::Boarding);
    let body = survivor(&state, loser).spaceling.as_ref().unwrap().body();
    assert_eq!(
        arena(&state).body_kind(body),
        Some(BodyKind::KinematicPosition)
    );
    assert_eq!(
        until_phase(&mut state, loser, Phase::Aboard, survivor::BOARD_TICKS + 1),
        survivor::BOARD_TICKS
    );
    assert!(survivor(&state, loser).spaceling.is_none());
    assert!(!survivors_remain(&state));
    // Once it is aboard, the fight reopens: the first result stands no
    // longer, and the tanks drive and fire again.
    assert_eq!(state.event_phase(), Some(EventPhase::Fighting));
    assert!(tanks(&state).result.is_none());
    let block = state.tanks_state().unwrap().rebuild.unwrap();
    assert_eq!(
        (
            block.slot as usize,
            block.stage,
            block.first_result.as_str(),
            block.fired
        ),
        (loser, Stage::Rebuilt, win_for(winner).label(), false)
    );
    // The sudden death runs on its own clock, and the duel steps again,
    // until the winner destroys the rebuilt tank.
    let held = tanks(&state).duel.tick();
    let mut ticks = 0;
    while tanks(&state).result.is_none() {
        tick(&mut state);
        ticks += 1;
        assert!(ticks <= rebuild::SUDDEN_DEATH_TICKS + SHELL_LIFETIME_TICKS as u64);
    }
    assert!(tanks(&state).duel.tick() > held);
    assert_eq!(tanks(&state).result, Some(win_for(winner)));
    assert!(
        tanks(&state).rebuild.as_ref().unwrap().sudden_death_ticks < rebuild::SUDDEN_DEATH_TICKS
    );
    // It blows up as any loser does, into the same 24 cells of dirt, and no
    // spaceling gets out this time.
    assert_eq!(state.event_phase(), Some(EventPhase::BlowingUp));
    while state.event_phase() == Some(EventPhase::BlowingUp) {
        tick(&mut state);
    }
    assert!(tanks(&state).wreck.all_dirt(loser));
    assert_eq!(entities(&dirt_of(&state, loser)), entities(&cells));
    assert_eq!(tanks(&state).survivors.out.len(), 1);
    assert!(!survivors_remain(&state));
    // The winner goes up alone, and the fight never reopens again.
    while state.event_kind().is_some() {
        assert!(tanks(&state).result.is_some());
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_rebuilt_tank_that_destroys_the_winner_wins_and_goes_up_with_its_spaceling() {
    use survivor::Phase;
    let (mut state, loser) = rebuilding(5.0 / 3.0, 48, false);
    let winner = 1 - loser;
    until_phase(&mut state, loser, Phase::Aboard, 15 * 60);
    while tanks(&state).result.is_none() {
        tick(&mut state);
    }
    // The first tank destroyed loses: here the winner, which blows up as any
    // loser does. Its spaceling has had no roll of its own, whatever its
    // roll said, and the one in the rebuilt tank stays aboard.
    let health = state.tanks_state().unwrap().health_milli;
    assert!(health[winner] == 0 && health[loser] > 0, "{health:?}");
    assert_eq!(tanks(&state).result, Some(win_for(loser)));
    assert_eq!(state.event_phase(), Some(EventPhase::BlowingUp));
    assert!(
        tanks(&state)
            .wreck
            .pieces
            .iter()
            .any(|piece| piece.slot == winner)
    );
    assert_eq!(
        tanks(&state).survivors.rolls[winner],
        Some(survivor::Ejection::Thrown)
    );
    assert_eq!(tanks(&state).survivors.out.len(), 1);
    assert_eq!(survivor(&state, loser).phase, Phase::Aboard);
    // The rebuilt tank goes up in the winner's place.
    while state.event_phase() != Some(EventPhase::BeamUp) {
        tick(&mut state);
    }
    assert!(tanks(&state).wreck.all_dirt(winner));
    assert_eq!(dirt_of(&state, winner).len(), DIRT_PER_TANK);
    assert!(!survivors_remain(&state));
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_sudden_death_that_runs_out_goes_to_the_tank_with_more_armor_left() {
    use survivor::Phase;
    // One winner kept all its armor, the other less than the rebuilt tank's
    // quarter.
    for (seed, rebuilt_wins) in [(43, false), (48, true)] {
        let (mut state, loser) = rebuilding(5.0 / 3.0, seed, false);
        let winner = 1 - loser;
        until_phase(&mut state, loser, Phase::Aboard, 15 * 60);
        assert_eq!(state.event_phase(), Some(EventPhase::Fighting));
        // Neither tank fires.
        event(&mut state).duel.set_firing(false);
        let mut ticks = 0;
        while state.event_phase() == Some(EventPhase::Fighting) {
            tick(&mut state);
            ticks += 1;
        }
        // Twenty seconds on, the sudden death is over, and the tank with more
        // of its armor left wins. The living loser blows up as a destroyed
        // one does.
        assert_eq!(ticks, rebuild::SUDDEN_DEATH_TICKS, "{seed}");
        let health = state.tanks_state().unwrap().health_milli;
        assert!(health.iter().all(|&hp| hp > 0), "{health:?}");
        let (won, lost) = if rebuilt_wins {
            (loser, winner)
        } else {
            (winner, loser)
        };
        assert!(health[won] > health[lost], "{seed}: {health:?}");
        assert_eq!(tanks(&state).result, Some(win_for(won)));
        assert_eq!(state.event_phase(), Some(EventPhase::BlowingUp));
        while state.event_phase() == Some(EventPhase::BlowingUp) {
            tick(&mut state);
        }
        assert!(tanks(&state).wreck.all_dirt(lost));
        assert_eq!(tanks(&state).survivors.out.len(), 1);
        while state.event_kind().is_some() {
            tick(&mut state);
        }
        assert!(state.arena.idle());
    }
}

#[test]
fn the_winner_shoots_at_the_dirt_crawling_home_and_scatters_it() {
    use survivor::{Ending, Fallback, Phase};
    let (mut state, loser) = rebuilding(5.0 / 3.0, 43, true);
    let scale = tanks(&state).duel.placement().scale;
    // The winner sees the spaceling set off back to its dirt, and turns on
    // where the tank will stand, plotting a shot there.
    until_phase(&mut state, loser, Phase::Returning, 8 * 60);
    tick(&mut state);
    let pile = survivor(&state, loser).pile.unwrap();
    assert!(!pile.watched);
    assert!(shot_at_dirt(&state, loser).fired.is_none());
    let pivot = winner(&state).barrel[0];
    assert!(tanks(&state).survivors.target(pivot).is_some());
    // It holds its fire until a moment after the dirt sets off, then fires
    // once, if its shell will land before the last cell is home.
    until_phase(&mut state, loser, Phase::Rebuilding, 5 * 60);
    let since = tanks(&state).rebuild.as_ref().unwrap().since;
    let done = since + rebuild::REBUILD_TICKS;
    while shot_at_dirt(&state, loser).fired.is_none() {
        assert_eq!(tanks(&state).duel.active_shells(), 0);
        tick(&mut state);
    }
    let (_, fired) = shot_at_dirt(&state, loser).fired.unwrap();
    let flight = u64::from(shot_at_dirt(&state, loser).aim.flight_ticks().unwrap());
    assert!(fired >= since + survivor::REACTION_TICKS);
    assert!(fired + flight + survivor::REACTION_TICKS <= done);
    assert_eq!(tanks(&state).duel.active_shells(), 1);
    let shots = tanks(&state).duel.shots();
    // The shell bursts by where the tank would stand before the last cell
    // is home. The cells fall where they are, dirt again, for the burst to
    // throw, and no tank stands.
    while shot_at_dirt(&state, loser).landed.is_none() {
        assert_eq!(tanks(&state).duel.shots(), shots);
        tick(&mut state);
    }
    let (point, landed) = shot_at_dirt(&state, loser).landed.unwrap();
    assert!(landed < done);
    let point = point.expect("a burst");
    assert!(point.distance_to(pile.hull) <= rebuild::SCATTER_REACH * scale);
    let rebuild = tanks(&state).rebuild.as_ref().unwrap();
    assert_eq!((rebuild.stage, rebuild.fired), (Stage::Scattered, true));
    for cell in rebuild.cells() {
        assert_eq!(arena(&state).body_kind(cell.body), Some(BodyKind::Dynamic));
    }
    assert_eq!(dirt_of(&state, loser).len(), DIRT_PER_TANK);
    assert!(!tanks(&state).duel.tanks(arena(&state))[loser].present);
    // The spaceling waves for the rescue beam, and the beam takes it.
    let waving = survivor(&state, loser);
    assert_eq!(
        (waving.drawn, waving.ending, waving.fallback),
        (
            Ending::Rebuild,
            Ending::RescueBeam,
            Some(Fallback::Scattered)
        )
    );
    until_phase(
        &mut state,
        loser,
        Phase::BeamedUp,
        survivor::REBUILD_BY_TICKS,
    );
    // The dirt lies where the burst threw it.
    while state.event_kind().is_some() {
        assert_eq!(
            tanks(&state).rebuild.as_ref().unwrap().stage,
            Stage::Scattered
        );
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_spaceling_whose_dirt_is_lost_out_of_view_waves_for_the_rescue_beam() {
    use survivor::{Ending, Fallback, Phase};
    let (mut state, loser) = rebuilding(5.0 / 3.0, 43, false);
    // A piece of its tank leaves the view before it turns to dirt.
    let layout = tanks(&state).layout;
    let piece = tanks(&state)
        .wreck
        .pieces
        .iter()
        .find(|piece| piece.slot == loser)
        .unwrap()
        .body;
    state.arena.held_mut(OWNER).unwrap().set_pose(
        piece,
        Vec2::new(0.0, layout.bounds_min.y - 1.0),
        0.0,
        true,
    );
    // It waits until the rest is dirt, then waves for the rescue beam: its
    // tank cannot be rebuilt.
    until_phase(&mut state, loser, Phase::Waving, 8 * 60);
    let waving = survivor(&state, loser);
    assert_eq!(
        (waving.drawn, waving.ending, waving.fallback),
        (
            Ending::Rebuild,
            Ending::RescueBeam,
            Some(Fallback::DirtLost)
        )
    );
    assert!(waving.pile.is_none() && tanks(&state).rebuild.is_none());
    assert!(!tanks(&state).wreck.all_dirt(loser));
    until_phase(
        &mut state,
        loser,
        Phase::BeamedUp,
        survivor::REBUILD_BY_TICKS,
    );
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_spaceling_with_no_way_back_to_its_dirt_waves_for_the_rescue_beam() {
    use survivor::{Ending, Fallback, Phase};
    let (mut state, loser) = rebuilding(5.0 / 3.0, 43, false);
    let scale = tanks(&state).duel.placement().scale;
    until_phase(&mut state, loser, Phase::Returning, 8 * 60);
    // A wall higher than it jumps comes up just ahead of it.
    let at = survivor(&state, loser).position;
    let stand = survivor(&state, loser).pile.unwrap().stand;
    let ahead = at + Vec2::X * ((stand - at.x).signum() * 1.5 * scale);
    let (wall, _) = wall(&mut state, 0, ahead, 3.5);
    // It runs into the wall, makes no headway, and waves for the rescue
    // beam. The dirt stays where it lies.
    until_phase(&mut state, loser, Phase::Waving, 3 * 60);
    let waving = survivor(&state, loser);
    assert_eq!(
        (waving.drawn, waving.ending, waving.fallback),
        (Ending::Rebuild, Ending::RescueBeam, Some(Fallback::NoWay))
    );
    assert!(tanks(&state).rebuild.is_none());
    state
        .arena
        .held_mut(OWNER)
        .unwrap()
        .remove_entity(wall.entity);
    until_phase(
        &mut state,
        loser,
        Phase::BeamedUp,
        survivor::REBUILD_BY_TICKS,
    );
    assert_eq!(dirt_of(&state, loser).len(), DIRT_PER_TANK);
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_spaceling_up_too_late_to_rebuild_its_tank_waves_for_the_rescue_beam() {
    use survivor::{Ending, Fallback, Phase};
    let (mut state, loser) = rebuilding(5.0 / 3.0, 43, false);
    let body = survivor(&state, loser).spaceling.as_ref().unwrap().body();
    // It lands, and something keeps it on its back until five seconds are
    // left: too little to see its tank rebuilt and climb in.
    until_phase(&mut state, loser, Phase::GettingUp, 2 * 60);
    let mut aftermath = 0;
    while aftermath < survivor::REBUILD_BY_TICKS - 5 * 60 {
        aftermath += u64::from(state.event_phase() == Some(EventPhase::Aftermath));
        let world = state.arena.held_mut(OWNER).unwrap();
        let motion = world.motion(body).unwrap();
        world.set_pose(body, motion.position, std::f32::consts::FRAC_PI_2, true);
        world.set_velocity(body, Vec2::ZERO, 0.0, true);
        tick(&mut state);
    }
    until_phase(&mut state, loser, Phase::Waving, 3 * 60);
    let waving = survivor(&state, loser);
    assert_eq!(
        (waving.drawn, waving.ending, waving.fallback),
        (
            Ending::Rebuild,
            Ending::RescueBeam,
            Some(Fallback::OutOfTime)
        )
    );
    assert!(tanks(&state).rebuild.is_none());
    until_phase(&mut state, loser, Phase::BeamedUp, 5 * 60);
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_rebuild_cut_short_at_any_stage_leaves_nothing_behind() {
    use survivor::Phase;
    type Reached = fn(&ClockState, usize) -> bool;
    let stages: [(bool, &str, Reached); 7] = [
        (false, "running back", |state, loser| {
            survivor(state, loser).phase == Phase::Returning
        }),
        (false, "crawling home", |state, _| {
            tanks(state).rebuild.is_some()
        }),
        (false, "climbing in", |state, loser| {
            survivor(state, loser).phase == Phase::Boarding
        }),
        (false, "fighting again", |state, _| {
            tanks(state)
                .rebuild
                .as_ref()
                .is_some_and(|rebuild| rebuild.reopened)
        }),
        (false, "blowing up again", |state, _| {
            tanks(state)
                .rebuild
                .as_ref()
                .is_some_and(|rebuild| rebuild.reopened)
                && state.event_phase() == Some(EventPhase::BlowingUp)
        }),
        (true, "shot at", |state, loser| {
            survivor(state, loser)
                .shot
                .as_ref()
                .is_some_and(|shot| shot.fired.is_some())
        }),
        (true, "scattered", |state, _| {
            tanks(state)
                .rebuild
                .as_ref()
                .is_some_and(|rebuild| rebuild.stage == Stage::Scattered)
        }),
    ];
    for (index, (watched, stage, reached)) in stages.into_iter().enumerate() {
        let (mut state, loser) = rebuilding(5.0 / 3.0, 43, watched);
        let mut ticks = 0;
        while !reached(&state, loser) {
            assert!(ticks < 40 * 60, "{stage}");
            tick(&mut state);
            ticks += 1;
        }
        for _ in 0..5 {
            tick(&mut state);
        }
        if index % 2 == 0 {
            state.set_aspect_ratio(4.0 / 3.0);
        } else {
            state.finish_event();
        }
        ClockScenario::step(&mut state, &[], Duration::ZERO);
        assert!(state.event_kind().is_none(), "{stage}");
        assert!(
            !survivors_remain(&state) && !pieces_remain(&state),
            "{stage}"
        );
        assert_eq!(
            (state.body_count(), state.collider_count()),
            (0, 0),
            "{stage}"
        );
        assert!(state.arena.idle(), "{stage}");
    }
}
