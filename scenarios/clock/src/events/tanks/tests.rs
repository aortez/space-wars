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
use engine_rapier::world::{RayCastOptions, physics_activity};
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
    assert_eq!(MAX_TICKS, 92 * 60);
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

/// The fight at `seed` on an `aspect` screen, with every loser's spaceling
/// getting out by `ejection`, stepped on to its result.
fn survival(aspect: f32, seed: u64, ejection: survivor::Ejection) -> (ClockState, usize) {
    let mut state = ready(aspect, seed, ClockTimeFormat::TwentyFourHour);
    start_fighting(&mut state);
    event(&mut state).survivors.rolls = [Some(ejection); 2];
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
    let (mut state, loser) = survival(4.0 / 3.0, 43, survivor::Ejection::Thrown);
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
    let (mut state, loser) = survival(5.0 / 3.0, 42, survivor::Ejection::Pod);
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
        let (mut state, loser) = survival(5.0 / 3.0, 42, ejection);
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
    use survivor::Phase;
    let (mut state, loser) = survival(4.0 / 3.0, 43, survivor::Ejection::Thrown);
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
    // The aftermath waits for it as long as it may, then the beam takes it
    // as it lies.
    assert_eq!(aftermath, AFTERMATH_TICKS);
    assert_eq!(state.event_phase(), Some(EventPhase::BeamUp));
    let survivor = survivor(&state, loser);
    assert_eq!(survivor.phase, Phase::BeamedUp);
    assert!(!survivor.stood_up);
    assert!(survivor.lifted.unwrap().1.angle.cos().abs() < 0.2);
    while state.event_kind().is_some() {
        tick(&mut state);
    }
    assert!(state.arena.idle());
}

#[test]
fn a_fight_cut_short_at_any_stage_of_a_survivor_leaves_nothing_behind() {
    use survivor::{Ejection, Phase};
    let stages: [(Ejection, &[Phase]); 2] = [
        (
            Ejection::Thrown,
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
            &[
                Phase::Sealed,
                Phase::Opening,
                Phase::ClimbingOut,
                Phase::LookingAround,
                Phase::Waving,
                Phase::BeamedUp,
            ],
        ),
    ];
    for (ejection, phases) in stages {
        // Each stage, then the empty pod going with the hills.
        for stage in 0..=phases.len() {
            let (mut state, loser) = survival(5.0 / 3.0, 42, ejection);
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
            assert!(state.event_kind().is_none(), "{ejection:?} {stage}");
            assert!(!survivors_remain(&state));
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            assert!(state.arena.idle());
        }
    }
}
