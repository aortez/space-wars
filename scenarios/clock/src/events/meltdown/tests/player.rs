use super::*;
use crate::events::ActiveEvent;
use engine_common::{ClockFloorMode, ClockRainAmount};
use engine_rapier::world::{BodyId, BodyRole, ColliderId, ColliderRole, PhysicsId, PhysicsWorld};

const DUCK: BodyId = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
const BLOCK: BodyId = BodyId::new(PhysicsId::new(4001), BodyRole::PRIMARY);
const BLOCK_HULL: ColliderId = ColliderId::new(PhysicsId::new(4001), ColliderRole::PRIMARY, 0);

fn ticks(state: &mut ClockState, count: usize) {
    for _ in 0..count {
        tick(state);
    }
}

fn toggle(state: &mut ClockState, seat: u8) {
    ClockScenario::step(
        state,
        &[ClockAction::toggle_player_duck(seat)],
        Duration::ZERO,
    );
}

fn player(aspect: f32, seed: u64, panels: bool) -> ClockState {
    let mut state = ready(aspect, seed);
    state.finish_event();
    let mut settings = state.settings();
    settings.time_format = ClockTimeFormat::TwelveHour;
    settings.rain_amount = ClockRainAmount::Heavy;
    state.configure(settings);
    if panels {
        state.preview_event(ClockEventKind::Rain);
        ticks(&mut state, 480);
    }
    toggle(&mut state, 1);
    ticks(&mut state, 90);
    state.finish_event();
    state
}

fn event(state: &ClockState) -> &MeltdownEvent {
    let Some(ActiveEvent::Meltdown(event)) = &state.active_event else {
        panic!("Meltdown")
    };
    event
}

fn event_mut(state: &mut ClockState) -> &mut MeltdownEvent {
    let Some(ActiveEvent::Meltdown(event)) = &mut state.active_event else {
        panic!("Meltdown")
    };
    event
}

fn world(state: &ClockState) -> &PhysicsWorld {
    state
        .duck_visit
        .as_deref()
        .or_else(|| event(state).vacant_arena())
        .unwrap()
        .arena_world()
}

fn isolate_block(state: &mut ClockState, position: Vec2, velocity: Vec2) {
    let event = event_mut(state);
    for cell in &mut event.cells {
        cell.release_tick = 10_000;
    }
    let cell = &mut event.cells[0];
    assert!(!cell.meridiem);
    cell.position = position;
    cell.velocity = velocity;
    cell.angle = 0.0;
    cell.spin = 0.0;
    cell.release_tick = 0;
}

fn conserved(state: &ClockState) {
    let d = state.meltdown_state().unwrap();
    let total = d.solid_microunits
        + d.pooled_microunits
        + d.spilling_microunits
        + d.drained_microunits
        + d.reclaimed_microunits;
    assert!(total.abs_diff(d.initial_microunits) <= 3, "{d:?}");
    assert!(state.body_count() <= 128 && state.collider_count() <= 128);
    assert!(d.spill_parcels <= MAX_SPILL_PARCELS);
    assert!(
        event(state)
            .water
            .pools()
            .iter()
            .map(|p| p.spec().bed.len())
            .sum::<usize>()
            <= 135
    );
}

#[test]
fn shared_meltdown_replays_conserves_material_and_cleans_up_in_both_arenas() {
    let mut floated = false;
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        for panels in [false, true] {
            let mut a = player(aspect, 42, panels);
            let mut b = player(aspect, 42, panels);
            let base = a.body_count();
            let session = a.player_duck_session();
            for state in [&mut a, &mut b] {
                state.preview_event(ClockEventKind::Meltdown);
            }
            assert_eq!(a.body_count(), base, "unreleased cells aren't rigid bodies");
            let mut saw_solid = false;
            let mut saw_liquid = false;
            for elapsed in 0..510 {
                conserved(&a);
                assert_eq!(a.meltdown_state(), b.meltdown_state());
                assert_eq!(a.player_duck_state(), b.player_duck_state());
                saw_solid |= world(&a).motions().any(|r| r.id.entity.value() >= 4001);
                saw_liquid |= event(&a).water.stats().injected > 0.0;
                floated |= a
                    .player_duck_state()
                    .is_some_and(|p| p.submerged_milli > 100 && !p.duck.grounded);
                if elapsed >= 420 {
                    assert!(
                        !world(&a).motions().any(|r| r.id.entity.value() >= 4001),
                        "no ghost cells"
                    );
                }
                tick(&mut a);
                tick(&mut b);
            }
            assert!(saw_solid && saw_liquid);
            assert_eq!(a.event_kind(), None);
            assert_eq!(a.player_duck_session(), session);
            assert_eq!(a.floor_mode(), ClockFloorMode::EventOwned);
            assert_eq!(a.body_count(), base);
            toggle(&mut a, 1);
            ticks(&mut a, 30);
            assert_eq!((a.body_count(), a.collider_count()), (0, 0));
            assert_eq!(a.floor_mode(), ClockFloorMode::Closed);
        }
    }
    assert!(
        floated,
        "the same player's buoyancy responds to melted water"
    );
}

#[test]
fn block_block_contacts_do_not_trigger_the_material_change() {
    let mut state = player(4.0 / 3.0, 42, false);
    state.preview_event(ClockEventKind::Meltdown);
    let layout = state.duck_visit.as_ref().unwrap().layout;
    isolate_block(
        &mut state,
        Vec2::new(0.0, layout.floor_y + 6.0 * layout.pitch),
        Vec2::ZERO,
    );
    let second = MeltCell {
        position: event(&state).cells[0].position + Vec2::new(0.0, layout.pitch * 0.79),
        ..event(&state).cells[0]
    };
    event_mut(&mut state).cells[1] = second;
    let other = PhysicsId::new(4002);
    let mut contact = false;
    for _ in 0..12 {
        tick(&mut state);
        contact |= world(&state)
            .surface_contacts(BLOCK_HULL)
            .any(|c| c.collider.entity == other);
        assert_eq!(event(&state).water.stats().injected, 0.0);
        assert!(world(&state).motion(BLOCK).is_some());
        assert!(
            world(&state)
                .motion(BodyId::new(other, BodyRole::PRIMARY))
                .is_some()
        );
        conserved(&state);
    }
    assert!(contact);
}

#[test]
fn shared_meltdown_final_owner_cleans_up_from_every_material_phase() {
    for panels in [false, true] {
        for stop_at in [0, 70, 250, 440] {
            let mut state = player(4.0 / 3.0, 42, panels);
            state.preview_event(ClockEventKind::Meltdown);
            ticks(&mut state, stop_at);
            toggle(&mut state, 1);
            ticks(&mut state, 30);
            assert!(state.player_duck_session().is_none());
            assert!(event(&state).vacant_arena().is_some());
            for _ in stop_at + 30..510 {
                conserved(&state);
                tick(&mut state);
            }
            assert_eq!(state.event_kind(), None);
            assert_eq!(state.floor_mode(), ClockFloorMode::Closed);
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        }
    }
}

#[test]
fn scheduled_meltdown_starts_without_an_extra_player_tick() {
    let mut state = player(4.0 / 3.0, 42, false);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Demo;
    settings.events = engine_common::ClockEvents {
        falling: false,
        color_cycle: false,
        meltdown: true,
        duck: false,
        marquee: false,
        digit_slide: false,
        rain: false,
    };
    state.configure(settings);
    ticks(&mut state, crate::events::COOLDOWN_TICKS as usize);
    // This fixture previously previewed Meltdown; its per-kind reuse cooldown
    // can outlast the periodic deadline. Honor both public scheduling gates.
    let due = state
        .next_event_tick()
        .unwrap()
        .max(state.event_ready_at_tick(ClockEventKind::Meltdown));
    while state.simulation_tick() + 1 < due {
        tick(&mut state);
    }
    let duck_tick = state.duck_visit.as_ref().unwrap().tick;
    tick(&mut state);
    assert_eq!(event(&state).tick, 1);
    assert_eq!(state.duck_visit.as_ref().unwrap().tick, duck_tick + 1);
}

#[test]
fn impact_driven_course_outfalls_do_not_draw_compressed_water_as_long_spikes() {
    // The Picade production capture exposed this during rapid changes in the
    // water height as whole blocks liquefied near a course lip.
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: 4.0 / 3.0,
            event_profile: ClockEventProfile::Off,
            time_format: ClockTimeFormat::TwelveHour,
            ..Default::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 34, 56).unwrap(),
        )],
        Duration::ZERO,
    );
    toggle(&mut state, 1);
    ticks(&mut state, 90);
    state.preview_event(ClockEventKind::Meltdown);
    for elapsed in 0..180 {
        tick(&mut state);
        let water = &event(&state).water;
        for i in 0..water.parcels().len() {
            if let Some(ribbon) = water.spill_ribbon(i) {
                let [a, b] = ribbon.quads;
                let mid = (a[2] - a[1]).length();
                let ends = (a[3] - a[0]).length().max((b[2] - b[1]).length());
                assert!(
                    !matches!(
                        water.spill_source(i),
                        Some(engine_water::SpillSource::Junction { .. })
                    ) || mid.max(ends) <= 4.0 * (water.parcels()[i].volume as f32).sqrt() + 0.001,
                    "tick={elapsed} source={:?} middle={mid} ends={ends}",
                    water.spill_source(i)
                );
            }
        }
    }
}

#[test]
fn block_pushes_duck_without_melting_then_liquefies_on_the_actual_floor() {
    let mut state = player(4.0 / 3.0, 42, false);
    state.preview_event(ClockEventKind::Meltdown);
    let duck = state.duck_visit.as_ref().unwrap();
    let layout = duck.layout;
    let radius = duck.radius;
    let origin = duck.arena_world().motion(DUCK).unwrap().position;
    // Land an ordinary square onto the actor, with a small sideways offset so
    // the same live solver contact pushes it. Other cells stay unreleased.
    isolate_block(
        &mut state,
        origin + Vec2::new(radius * 0.65, radius + layout.pitch * 0.4 + 1.0),
        Vec2::new(0.0, -50.0),
    );
    let mut contact = false;
    let mut moved = false;
    for _ in 0..120 {
        tick(&mut state);
        conserved(&state);
        if world(&state)
            .surface_contacts(BLOCK_HULL)
            .any(|c| c.collider.entity == DUCK.entity)
        {
            contact = true;
            assert!(world(&state).motion(BLOCK).is_some());
            assert_eq!(
                event(&state).water.stats().injected,
                0.0,
                "the duck is not a melting surface"
            );
        }
        moved |= (world(&state).motion(DUCK).unwrap().position.x - origin.x).abs() > radius * 0.05;
        if event(&state).water.stats().injected > 0.0 {
            break;
        }
    }
    assert!(contact && moved, "real block/duck contact and displacement");
    assert!(
        world(&state).motion(BLOCK).is_none(),
        "removed on conversion"
    );
    assert!((event(&state).water.stats().injected - event(&state).cell_area).abs() < 1e-6);
}

#[test]
fn solid_blocks_pass_through_real_gaps_without_becoming_water() {
    let mut state = player(4.0 / 3.0, 42, false);
    state.preview_event(ClockEventKind::Meltdown);
    let water = &event(&state).water;
    let gaps: Vec<_> = water
        .pools()
        .windows(2)
        .map(|pair| {
            let left = pair[0].spec();
            (
                left.left + left.column_width * left.bed.len() as f64,
                pair[1].spec().left,
            )
        })
        .collect();
    let &(left, right) = gaps
        .iter()
        .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
        .unwrap();
    let layout = state.duck_visit.as_ref().unwrap().layout;
    isolate_block(
        &mut state,
        Vec2::new(((left + right) * 0.5) as f32, layout.floor_y + layout.pitch),
        Vec2::ZERO,
    );
    // Use the actual small AM/PM geometry to fit completely through the pit.
    event_mut(&mut state).cells[0].meridiem = true;
    event_mut(&mut state).initial_area = event(&state)
        .cells
        .iter()
        .map(|c| c.area_scale() * event(&state).cell_area)
        .sum();
    assert!(right - left > f64::from(layout.pitch * crate::meridiem::PIXEL_SIZE));
    for _ in 0..100 {
        tick(&mut state);
        conserved(&state);
    }
    assert!(world(&state).motion(BLOCK).is_none());
    assert_eq!(event(&state).water.stats().injected, 0.0);
    assert!(event(&state).exited_solid_area > 0.0);
}

#[test]
fn conversion_backpressure_keeps_the_whole_solid_then_retries_without_loss() {
    let mut state = player(4.0 / 3.0, 42, false);
    state.preview_event(ClockEventKind::Meltdown);
    let duck = state.duck_visit.as_ref().unwrap();
    let layout = duck.layout;
    let x = duck.arena_world().motion(DUCK).unwrap().position.x * 0.85;
    isolate_block(
        &mut state,
        Vec2::new(x, layout.floor_y + layout.pitch * 0.4 + 1.0),
        Vec2::ZERO,
    );
    for _ in 0..MAX_SPILL_PARCELS {
        event_mut(&mut state)
            .water
            .add_falling(Parcel {
                position: Vec2::new(0.0, 10000.0),
                velocity: Vec2::ZERO,
                volume: 1.0,
                duration: 1.0 / 60.0,
                horizontal_bounds: None,
            })
            .unwrap();
    }
    ticks(&mut state, 40);
    assert!(world(&state).motion(BLOCK).is_some());
    assert_eq!(
        event(&state).water.stats().injected,
        MAX_SPILL_PARCELS as f64,
        "no partial conversion"
    );
    event_mut(&mut state).water.reclaim();
    ticks(&mut state, 20);
    assert!(world(&state).motion(BLOCK).is_none());
    assert!(
        (event(&state).water.stats().injected - MAX_SPILL_PARCELS as f64 - event(&state).cell_area)
            .abs()
            < 1e-6
    );
}

#[test]
fn shared_meltdown_steps_once_and_preserves_visit_through_final_tick() {
    for opening in [true, false] {
        let mut composed = player(4.0 / 3.0, 42, false);
        let mut baseline = player(4.0 / 3.0, 42, false);
        if opening {
            for state in [&mut composed, &mut baseline] {
                toggle(state, 1);
                ticks(state, 30);
                toggle(state, 1);
            }
        }
        composed.preview_event(ClockEventKind::Meltdown);
        for cell in &mut event_mut(&mut composed).cells {
            cell.release_tick = 10_000;
        }
        for elapsed in 0..530 {
            for state in [&mut composed, &mut baseline] {
                let (session_id, player) = state.player_duck_session().unwrap();
                ClockScenario::step(
                    state,
                    &[ClockAction::player_duck_input(crate::ClockDuckInput {
                        session_id,
                        player,
                        move_milli: if elapsed % 60 < 10 { 400 } else { 0 },
                        jump: elapsed == 120,
                    })],
                    Duration::from_nanos(16_666_667),
                );
            }
            assert_eq!(
                composed.player_duck_state(),
                baseline.player_duck_state(),
                "opening={opening} tick={elapsed}"
            );
        }
    }
}

#[test]
fn shared_meltdown_survives_pause_dismiss_rejoin_reading_changes_and_replacement() {
    for panels in [false, true] {
        let mut state = player(4.0 / 3.0, 42, panels);
        state.preview_event(ClockEventKind::Meltdown);
        ticks(&mut state, 70);
        let id = state.event_id();
        let frame = ClockScenario::render_frame(&state);
        for _ in 0..120 {
            ClockScenario::step(&mut state, &[], Duration::ZERO);
        }
        assert_eq!(ClockScenario::render_frame(&state), frame);
        toggle(&mut state, 1);
        ticks(&mut state, 30);
        assert!(state.player_duck_session().is_none());
        assert!(event(&state).vacant_arena().is_some());
        let cells = event(&state).cells.clone();
        let water = event(&state).diagnostics();
        toggle(&mut state, 2);
        assert_eq!(state.event_id(), id);
        assert_eq!(event(&state).cells, cells);
        assert_eq!(event(&state).diagnostics(), water);
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(13, 0, 0).unwrap(),
            )],
            Duration::ZERO,
        );
        let mut settings = state.settings();
        settings.time_format = ClockTimeFormat::TwentyFourHour;
        state.configure(settings);
        ticks(&mut state, 330);
        assert_eq!(state.event_kind(), Some(ClockEventKind::Meltdown));
        assert!(event(&state).cells.is_empty());
        assert!(!world(&state).motions().any(|r| r.id.entity.value() >= 4001));
        assert_eq!(
            state.display(),
            crate::digits::snapshot(ClockReading::new(13, 0, 0).unwrap(), settings.time_format)
        );
        let player = state.player_duck_state();
        state.preview_event(ClockEventKind::ColorCycle);
        assert_eq!(state.player_duck_state(), player);
        state.set_aspect_ratio(0.6);
        assert!(state.player_duck_session().is_none());
        assert_eq!(state.event_kind(), None);
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    }
}

#[test]
fn water_lab_previews_remain_explicitly_incompatible_with_a_player() {
    let mut state = player(4.0 / 3.0, 42, false);
    state.config.water_lab = ClockWaterLab::Floating;
    state.sync_event_schedule();
    assert!(state.event_blocked_by_player(ClockEventKind::Meltdown));
    let before = state.player_duck_state();
    state.preview_event(ClockEventKind::Meltdown);
    assert_eq!(state.event_kind(), None);
    assert_eq!(state.player_duck_state(), before);
    toggle(&mut state, 1);
    ticks(&mut state, 30);
    state.preview_event(ClockEventKind::Meltdown);
    assert!(event(&state).lab);
    assert!(!event(&state).shares_visit_arena());
}
