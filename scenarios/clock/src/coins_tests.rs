//! Coins is Explosion's batch with round pieces that drop, so these tests
//! cover what differs: the phases it publishes, its own identities, how the
//! pieces fall and come to rest, and how they fly home and square up. The
//! reassembly and the shared-arena lifecycle are Explosion's and keep their
//! tests there; one shared-arena pass and one duck contact here prove the
//! batch reaches the run's arena as Coins.
use super::*;
use engine_common::ClockFloorMode;
use engine_core::Vec2;
use engine_rapier::world::{BodyId, BodyRole, ColliderId, ColliderRole, PhysicsId};
use events::explosion::{CASCADE_TICKS, COINS_TICKS, MAX_EXPLOSION_CELLS, WARNING_TICKS};

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
fn event(state: &ClockState) -> &events::explosion::ExplosionEvent {
    let Some(ActiveEvent::Coins(event)) = &state.active_event else {
        panic!("Coins expected")
    };
    event
}
fn cells(state: &ClockState) -> &[events::explosion::Cell] {
    &event(state).cells
}
fn coin(index: usize) -> BodyId {
    BodyId::new(
        PhysicsId::new(arena::ids::canonical::COIN_CELLS.start + index as u64),
        BodyRole::PRIMARY,
    )
}
fn toggle(state: &mut ClockState, seat: u8) {
    ClockScenario::step(
        state,
        &[ClockAction::toggle_player_duck(seat)],
        Duration::ZERO,
    );
}
fn primitives(state: &ClockState) -> Vec<engine_common::RenderPrimitive> {
    ClockScenario::render_frame(state)
        .layers
        .iter()
        .filter(|layer| layer.z == render::ACTIVE_CELL_LAYER || layer.z == render::LABEL_LAYER)
        .flat_map(|layer| layer.primitives.iter().cloned())
        .collect()
}

#[test]
fn coins_keep_explosions_cap_and_take_their_own_identities() {
    let mut state = ready(4.0 / 3.0, 5, ClockTimeFormat::TwelveHour);
    // Synthetic all-segment face exercises the cap beyond valid clock readings.
    state.display.digits = [Some(8); 4];
    state.display.meridiem = Some("AM");
    state.preview_event(ClockEventKind::Coins);
    let count = cells(&state).len();
    assert_eq!(count, 96 + crate::meridiem::MAX_CELLS);
    assert!(count <= MAX_EXPLOSION_CELLS);
    assert_eq!(state.event_phase(), Some(EventPhase::Minting));
    ticks(&mut state, WARNING_TICKS);
    assert_eq!(state.event_phase(), Some(EventPhase::Cascading));
    assert_eq!(state.body_count(), count + 4);
    let world = event(&state).own_world().unwrap();
    for index in 0..count {
        assert!(
            world.motion(coin(index)).is_some(),
            "coin {index} is a body"
        );
    }
    assert!(
        world.motions().all(|motion| {
            let id = motion.id.entity.value();
            arena::ids::canonical::COIN_CELLS.contains(&id) || id < 5000
        }),
        "only coins and the floor's own bodies"
    );
    let diagnostics = state.coins_state().unwrap();
    assert_eq!(
        (
            diagnostics.cells,
            diagnostics.live_cells,
            diagnostics.max_cells
        ),
        (count as u32, count as u32, MAX_EXPLOSION_CELLS as u32)
    );
    assert!(!diagnostics.shared_arena);
    ticks(&mut state, COINS_TICKS - WARNING_TICKS);
    assert_eq!((state.body_count(), state.collider_count()), (0, 0));
    assert!(state.coins_state().is_none());
}

#[test]
fn seeded_cascades_replay_and_finish_with_bounded_physics_at_every_aspect() {
    for aspect in [0.25, 0.6, 0.75, 4.0 / 3.0, 5.0 / 3.0, 4.0] {
        for seed in 0..2 {
            for format in [ClockTimeFormat::TwelveHour, ClockTimeFormat::TwentyFourHour] {
                let mut a = ready(aspect, seed, format);
                let mut b = ready(aspect, seed, format);
                let baseline = ClockScenario::render_frame(&a);
                a.preview_event(ClockEventKind::Coins);
                b.preview_event(ClockEventKind::Coins);
                let count = cells(&a).len();
                assert!(count > 20 && count <= MAX_EXPLOSION_CELLS);
                let pitch = Layout::new(aspect).pitch;
                let mut saw_drop = false;
                let mut saw_roll = false;
                for t in 0..COINS_TICKS {
                    assert_eq!(cells(&a), cells(&b), "aspect={aspect} seed={seed} tick={t}");
                    assert_eq!(a.coins_state(), b.coins_state());
                    let expected = if t < WARNING_TICKS {
                        (EventPhase::Minting, 4)
                    } else if t < WARNING_TICKS + CASCADE_TICKS {
                        (EventPhase::Cascading, count + 4)
                    } else {
                        (EventPhase::Reforming, 0)
                    };
                    assert_eq!(a.event_phase(), Some(expected.0), "tick {t}");
                    assert_eq!(a.body_count(), expected.1);
                    assert_eq!(a.collider_count(), expected.1);
                    for cell in cells(&a) {
                        assert!(
                            cell.position.x.is_finite()
                                && cell.position.y.is_finite()
                                && cell.angle.is_finite()
                        );
                        saw_roll |= cell.angle.abs() > 0.1;
                        saw_drop |= cell.origin.y - cell.position.y > pitch;
                    }
                    if t % 30 == 0 {
                        let frame = ClockScenario::render_frame(&a);
                        assert!(
                            frame
                                .layers
                                .iter()
                                .map(|l| l.primitives.len())
                                .sum::<usize>()
                                < 1000
                        );
                        assert_eq!(frame, ClockScenario::render_frame(&b));
                    }
                    ticks(&mut a, 1);
                    ticks(&mut b, 1);
                }
                assert!(saw_drop && saw_roll, "aspect={aspect} seed={seed}");
                assert!(a.coins_state().is_none());
                assert_eq!((a.body_count(), a.collider_count()), (0, 0));
                assert_eq!(a.floor_mode(), ClockFloorMode::Closed);
                assert_eq!(ClockScenario::render_frame(&a), baseline);
            }
        }
    }
}

/// The cascade is long enough for the coins to come to rest before they fly
/// home on every layout a device has: landscape, the Picade's 4:3 and the
/// 480x800 portrait, where the face is 28 pitches up and the fall alone takes
/// two seconds. "At rest" is under half a pitch a second, which is under half a
/// pixel a frame at the largest pitch. Coins that found the drain keep falling
/// out of view and are not counted; the return brings them home from wherever
/// they are. The 0.25 strip is left out on purpose: its face is 73 pitches up,
/// and its coins are still falling when the return begins, as Explosion's
/// debris is there.
#[test]
fn coins_come_to_rest_before_they_fly_home() {
    for (aspect, seed) in [(4.0 / 3.0, 1), (800.0 / 480.0, 2), (0.6, 3), (4.0, 5)] {
        let mut state = ready(aspect, seed, ClockTimeFormat::TwentyFourHour);
        let layout = Layout::new(aspect);
        state.preview_event(ClockEventKind::Coins);
        ticks(&mut state, WARNING_TICKS);
        let count = cells(&state).len();
        let mut settled_at = None;
        let mut drained = 0;
        for tick in 0..CASCADE_TICKS {
            let world = event(&state).own_world().unwrap();
            let (mut resting, mut in_view) = (0, 0);
            drained = 0;
            for index in 0..count {
                let motion = world.motion(coin(index)).unwrap();
                if motion.position.y < layout.floor_y {
                    drained += 1;
                    continue;
                }
                in_view += 1;
                if motion.linear_velocity.length() < layout.pitch * 0.5 {
                    resting += 1;
                }
            }
            if settled_at.is_none() && resting * 10 >= in_view * 9 {
                settled_at = Some(tick);
            }
            ticks(&mut state, 1);
        }
        let settled_at = settled_at
            .unwrap_or_else(|| panic!("aspect={aspect} seed={seed}: the coins never settled"));
        println!(
            "coins settle: aspect={aspect} seed={seed} count={count} drained={drained} \
             settled_at_tick={settled_at} of {CASCADE_TICKS}"
        );
        assert!(
            settled_at + 30 <= CASCADE_TICKS,
            "aspect={aspect} seed={seed}: settled at tick {settled_at}, leaving less than \
             half a second of rest before the return"
        );
        assert!(
            drained * 2 < count,
            "aspect={aspect} seed={seed}: {drained} of {count} coins went down the drain"
        );
    }
}

/// Every piece travels home from where it lies, and nothing fades: at the
/// half-way frame the return is a polygon per piece at full opacity, and at
/// the end every piece is at its own cell, square, with the face restored.
#[test]
fn pieces_fly_home_from_where_they_lie_and_square_up() {
    for font in ClockFont::ALL {
        let mut state = ready(4.0 / 3.0, 42, ClockTimeFormat::TwelveHour);
        let mut settings = state.settings();
        settings.fonts.selected = font;
        state.configure(settings);
        let baseline = ClockScenario::render_frame(&state);
        state.preview_event(ClockEventKind::Coins);
        let count = cells(&state).len();
        let pitch = Layout::new(4.0 / 3.0).pitch;
        ticks(&mut state, WARNING_TICKS + CASCADE_TICKS);
        assert_eq!(state.event_phase(), Some(EventPhase::Reforming));
        let away = cells(&state)
            .iter()
            .filter(|cell| (cell.position - cell.origin).length() > pitch)
            .count();
        assert!(
            away * 10 >= count * 9,
            "{font:?}: only {away} of {count} pieces start the return away from home"
        );
        // The first return frame is continuous with the cascade: every piece is
        // still a disc. Half way home each is part way to a square.
        let circles = primitives(&state)
            .iter()
            .filter(|p| matches!(p, engine_common::RenderPrimitive::Circle(_)))
            .count();
        assert_eq!(circles, count, "{font:?}: the return starts from the discs");
        ticks(&mut state, REFORMING_TICKS / 2);
        let halfway = primitives(&state);
        assert!(
            !halfway
                .iter()
                .any(|p| matches!(p, engine_common::RenderPrimitive::Circle(_))),
            "{font:?}: half way home the pieces are no longer discs"
        );
        let polygons: Vec<_> = halfway
            .into_iter()
            .filter_map(|p| match p {
                engine_common::RenderPrimitive::Polygon(polygon) => Some(polygon),
                _ => None,
            })
            .collect();
        assert_eq!(
            polygons.len(),
            count + 2,
            "{font:?}: one piece per cell, plus the colon"
        );
        assert!(
            polygons.iter().all(|p| p.fill.unwrap().color.a == 1.0),
            "{font:?}: returning pieces stay fully visible"
        );
        // The last tick the pieces exist on is one short of the snap home, so
        // they sit a fraction of a pixel out; the hand back lands them exactly.
        ticks(&mut state, REFORMING_TICKS - REFORMING_TICKS / 2 - 1);
        for cell in cells(&state) {
            assert!(
                (cell.position - cell.origin).length() < pitch * 0.05,
                "{font:?}: a piece is {} from home on the last tick",
                (cell.position - cell.origin).length()
            );
            assert!(cell.angle.abs() < 0.01, "{font:?}: a piece arrives square");
        }
        ticks(&mut state, 1);
        assert!(state.coins_state().is_none(), "{font:?}");
        assert_eq!(ClockScenario::render_frame(&state), baseline, "{font:?}");
    }
}

#[test]
fn shared_cascade_lives_in_the_arena_and_removes_only_its_own_bodies() {
    let mut state = ready(4.0 / 3.0, 42, ClockTimeFormat::TwelveHour);
    toggle(&mut state, 1);
    ticks(&mut state, 90);
    state.finish_event();
    let base = (state.body_count(), state.collider_count());
    let base_flaps = state.flap_body_count();
    state.preview_event(ClockEventKind::Coins);
    assert_eq!((state.body_count(), state.collider_count()), base);
    ticks(&mut state, WARNING_TICKS + 5);
    let count = cells(&state).len();
    assert_eq!(state.body_count(), base.0 + count);
    assert_eq!(state.collider_count(), base.1 + count);
    assert!(state.coins_state().unwrap().shared_arena);
    assert!(
        state
            .arena
            .holds(arena::Owner::Event(ClockEventKind::Coins))
    );
    let before = cells(&state).to_vec();
    toggle(&mut state, 1);
    ticks(&mut state, 30);
    assert!(state.duck_visit.is_none());
    let departed = (state.body_count(), state.collider_count());
    assert_eq!(departed.0, base.0 + count - 1 - base_flaps);
    assert_ne!(cells(&state), before, "the retained arena keeps simulating");
    while state.event_phase() != Some(EventPhase::Reforming) {
        ticks(&mut state, 1);
    }
    // Nobody rejoined, so the return leaves the departed world less its coins.
    assert_eq!(
        (state.body_count(), state.collider_count()),
        (departed.0 - count, departed.1 - count)
    );
    assert!(
        state
            .arena
            .world()
            .unwrap()
            .motions()
            .all(|m| m.id.entity.value() < 5000),
        "the return took every coin out of the arena"
    );
    ticks(&mut state, REFORMING_TICKS);
    assert!(state.coins_state().is_none());
    assert!(
        !state
            .arena
            .holds(arena::Owner::Event(ClockEventKind::Coins))
    );
}

#[test]
fn coins_fall_in_the_visit_world_and_can_hit_the_duck() {
    let mut state = ready(4.0 / 3.0, 42, ClockTimeFormat::TwentyFourHour);
    toggle(&mut state, 1);
    ticks(&mut state, 90);
    state.preview_event(ClockEventKind::Coins);
    ticks(&mut state, WARNING_TICKS);
    let character = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
    let target = coin(0);
    let physics = state.arena.world_mut().unwrap();
    let position = physics.motion(character).unwrap().position;
    physics.set_pose(target, position + Vec2::new(0.0, 40.0), 0.0, true);
    physics.set_velocity(target, Vec2::new(0.0, -150.0), 0.0, true);
    let mut contact = false;
    for _ in 0..60 {
        ticks(&mut state, 1);
        contact |= state
            .arena
            .world()
            .unwrap()
            .surface_contacts(ColliderId::new(character.entity, ColliderRole::PRIMARY, 0))
            .any(|contact| contact.collider.entity == target.entity);
    }
    assert!(contact, "coins must collide in the actual duck world");
}
