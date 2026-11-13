//! Coins is Explosion's batch with round pieces that drop, so these tests
//! cover what differs: the phases it publishes, its own identities, how the
//! pieces fall and come to rest, the magpie that takes some of them, and how
//! the rest fly home and square up with replacements for the shortfall. The
//! reassembly and the shared-arena lifecycle are Explosion's and keep their
//! tests there; one shared-arena pass and one duck contact here prove the
//! batch reaches the run's arena as Coins.
use super::*;
use engine_common::{ClockFloorMode, ClockFont, ClockFontSettings, ClockMagpiePhase};
use engine_core::Vec2;
use engine_rapier::world::{BodyId, BodyRole, ColliderId, ColliderRole, PhysicsId};
use events::explosion::{
    CASCADE_TICKS, COINS_TICKS, GATHERING_TICKS, MAX_EXPLOSION_CELLS, WARNING_TICKS,
    magpie::Support,
};

/// The gathering, from the first tick the coins lie for the magpie.
const GATHERING_AT: u64 = WARNING_TICKS + CASCADE_TICKS;
/// The return, once the gathering is up.
const REFORMING_AT: u64 = GATHERING_AT + GATHERING_TICKS;

fn ready(aspect: f32, seed: u64, format: ClockTimeFormat) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            // Steady lamps, which never change a frame on their own.
            lamps: engine_common::ClockLampMode::Steady,
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
/// `ready`, in `font`.
fn ready_in(aspect: f32, seed: u64, font: ClockFont) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            // Steady lamps, which never change a frame on their own.
            lamps: engine_common::ClockLampMode::Steady,
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
            fonts: ClockFontSettings {
                selected: font,
                ..Default::default()
            },
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
                    let taken = a.coins_state().unwrap().taken as usize;
                    let expected = if t < WARNING_TICKS {
                        (EventPhase::Minting, 4)
                    } else if t < GATHERING_AT {
                        (EventPhase::Cascading, count + 4)
                    } else if t < REFORMING_AT {
                        // The magpie takes its coins out of the world as it
                        // takes them out of the pile.
                        (EventPhase::Gathering, count + 4 - taken)
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
                assert!(
                    a.surface_accounting().retired(Retirement::Taken) >= 1,
                    "aspect={aspect} seed={seed}: the magpie took nothing"
                );
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
/// The coins the magpie kept are grown back from the pile as the shortfall
/// rule says, so the face is whole again.
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
        ticks(&mut state, REFORMING_AT);
        assert_eq!(state.event_phase(), Some(EventPhase::Reforming));
        let taken = state.coins_state().unwrap().taken as usize;
        assert!(taken > 0, "{font:?}: the magpie took nothing");
        assert_eq!(
            cells(&state).len(),
            count,
            "{font:?}: a replacement is split off the pile for each coin the magpie kept"
        );
        let growing = cells(&state).iter().filter(|cell| cell.side == 0.0).count();
        assert_eq!(
            growing, taken,
            "{font:?}: the replacements start from nothing"
        );
        let away = cells(&state)
            .iter()
            .filter(|cell| (cell.position - cell.origin).length() > pitch)
            .count();
        assert!(
            away * 10 >= count * 9,
            "{font:?}: only {away} of {count} pieces start the return away from home"
        );
        // The first return frame is continuous with the pile: every coin that
        // is left is still a disc, and the replacements have no size yet.
        // Half way home each is part way to a square.
        let circles = primitives(&state)
            .iter()
            .filter(|p| matches!(p, engine_common::RenderPrimitive::Circle(_)))
            .count();
        assert_eq!(
            circles,
            count - taken,
            "{font:?}: the return starts from the discs"
        );
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

fn magpie(state: &ClockState) -> engine_common::ClockMagpieState {
    state.coins_state().unwrap().magpie.unwrap()
}

/// Layer 8 is the bird's, and the coin at its beak is the one circle there.
/// The bird's feet, as the diagnostics give them.
fn feet(state: &ClockState) -> Vec2 {
    let [x, y] = magpie(state).position_milli;
    Vec2::new(x as f32 / 1000.0, y as f32 / 1000.0)
}
fn bird_primitives(state: &ClockState) -> (usize, usize) {
    let frame = ClockScenario::render_frame(state);
    let bird: Vec<_> = frame
        .layers
        .iter()
        .filter(|layer| layer.z == 8)
        .flat_map(|layer| layer.primitives.iter())
        .collect();
    let circles = bird
        .iter()
        .filter(|p| matches!(p, engine_common::RenderPrimitive::Circle(_)))
        .count();
    (bird.len() - circles, circles)
}

/// The magpie makes three or four trips in the gathering budget on every
/// device layout, taking a coin off the top of the pile each time and
/// carrying it out of view, and is away before the return begins. Each coin
/// it keeps leaves the world when it leaves the pile, its surface retires as
/// taken, and the return grows a replacement for it from the pile.
#[test]
fn the_magpie_takes_three_or_four_coins_and_is_gone_before_the_return() {
    for (aspect, seed) in [
        (4.0 / 3.0, 1),
        (800.0 / 480.0, 2),
        (0.6, 3),
        (4.0, 5),
        (800.0 / 480.0, 7),
        (4.0 / 3.0, 11),
        (0.75, 12),
    ] {
        let context = format!("aspect={aspect} seed={seed}");
        let layout = Layout::new(aspect);
        let mut state = ready(aspect, seed, ClockTimeFormat::TwentyFourHour);
        let baseline = ClockScenario::render_frame(&state);
        state.preview_event(ClockEventKind::Coins);
        let count = cells(&state).len();
        // The bird waits out of view until the coins have landed.
        for _ in 0..GATHERING_AT {
            let bird = magpie(&state);
            assert_eq!(bird.phase, ClockMagpiePhase::Away, "{context}");
            assert_eq!(bird_primitives(&state), (0, 0), "{context}");
            ticks(&mut state, 1);
        }
        assert_eq!(state.event_phase(), Some(EventPhase::Gathering));
        let mut phases = std::collections::BTreeSet::new();
        let mut last_seen = 0;
        let mut carried_frames = 0;
        let mut trip_lengths = Vec::new();
        let mut trip_started = None;
        for tick in 0..GATHERING_TICKS {
            let coins = state.coins_state().unwrap();
            let bird = coins.magpie.unwrap();
            phases.insert(bird.phase);
            assert_eq!(
                state.body_count(),
                count + 4 - coins.taken as usize,
                "{context} tick {tick}: a taken coin has left the world, and the bird has no body"
            );
            assert_eq!(
                coins.live_cells as usize,
                count - coins.taken as usize,
                "{context} tick {tick}"
            );
            assert_eq!(
                state.surface_accounting().retired(Retirement::Taken),
                u64::from(coins.taken),
                "{context} tick {tick}: a taken coin's surface goes with it"
            );
            let (pixels, circles) = bird_primitives(&state);
            if bird.phase == ClockMagpiePhase::Away {
                assert_eq!((pixels, circles), (0, 0), "{context} tick {tick}");
            } else {
                last_seen = tick;
                assert!(pixels > 40, "{context} tick {tick}: the bird is drawn");
                assert_eq!(
                    circles,
                    usize::from(bird.carrying),
                    "{context} tick {tick}: the coin in the beak is drawn there"
                );
                let x = bird.position_milli[0] as f32 / 1000.0;
                let y = bird.position_milli[1] as f32 / 1000.0;
                assert!(
                    y >= layout.floor_y - 0.01 && y < layout.canopy_y,
                    "{context} tick {tick}: the bird is below the floor or in the canopy at {y}"
                );
                assert!(
                    x > layout.bounds_min.x - layout.pitch * 2.5
                        && x < layout.bounds_max.x + layout.pitch * 2.5,
                    "{context} tick {tick}: the bird is far outside the view at {x}"
                );
            }
            if bird.phase == ClockMagpiePhase::Entering && trip_started.is_none() {
                trip_started = Some(tick);
            }
            if bird.phase == ClockMagpiePhase::Away
                && let Some(started) = trip_started.take()
            {
                trip_lengths.push(tick - started);
            }
            carried_frames += usize::from(bird.carrying);
            ticks(&mut state, 1);
        }
        let coins = state.coins_state().unwrap();
        let bird = coins.magpie.unwrap();
        assert_eq!(
            state.event_phase(),
            Some(EventPhase::Reforming),
            "{context}"
        );
        assert!(
            (3..=4).contains(&coins.taken),
            "{context}: the magpie took {} coins",
            coins.taken
        );
        assert_eq!(
            bird.trips, coins.taken,
            "{context}: every coin taken was carried off"
        );
        assert_eq!(bird.abandoned, 0, "{context}: no trip was given up");
        assert_eq!(
            bird.phase,
            ClockMagpiePhase::Away,
            "{context}: the bird is out of view when the coins fly home"
        );
        assert!(carried_frames > 60, "{context}");
        for phase in [
            ClockMagpiePhase::Entering,
            ClockMagpiePhase::Landed,
            ClockMagpiePhase::Hopping,
            ClockMagpiePhase::Taking,
            ClockMagpiePhase::Leaving,
        ] {
            assert!(phases.contains(&phase), "{context}: no {phase:?}");
        }
        println!(
            "magpie: {context} count={count} taken={} trips of {:?} ticks, gone after tick {last_seen} of {GATHERING_TICKS}",
            coins.taken, trip_lengths
        );
        // The shortfall is grown back from the pile, and the face is whole.
        assert_eq!(cells(&state).len(), count, "{context}");
        assert_eq!(
            cells(&state).iter().filter(|cell| cell.side == 0.0).count(),
            coins.taken as usize,
            "{context}"
        );
        ticks(&mut state, REFORMING_TICKS);
        assert!(state.coins_state().is_none(), "{context}");
        assert_eq!((state.body_count(), state.collider_count()), (0, 0));
        assert_eq!(ClockScenario::render_frame(&state), baseline, "{context}");
        assert_eq!(
            state.surface_accounting().retired(Retirement::Taken),
            u64::from(coins.taken),
            "{context}"
        );
    }
}

/// A coin that is kicked down the drain between being chosen and being
/// reached is given up for another, and the trip still ends with a coin.
#[test]
fn a_coin_that_leaves_the_pile_is_chosen_again() {
    let mut state = ready(800.0 / 480.0, 2, ClockTimeFormat::TwentyFourHour);
    let layout = Layout::new(800.0 / 480.0);
    state.preview_event(ClockEventKind::Coins);
    ticks(&mut state, GATHERING_AT);
    while magpie(&state).phase != ClockMagpiePhase::Landed {
        ticks(&mut state, 1);
    }
    let chosen = magpie(&state).target.expect("a coin to go for") as usize;
    let before = cells(&state)[chosen].position;
    {
        let Some(ActiveEvent::Coins(event)) = &mut state.active_event else {
            panic!("Coins expected")
        };
        let world = event.own_world_mut().expect("a standalone pile");
        assert!(world.set_pose(
            coin(chosen),
            Vec2::new(0.0, layout.floor_y - layout.pitch * 12.0),
            0.0,
            true
        ));
    }
    // The bird reads the pile as the last step left it, so it sees the move
    // on the tick after the world has stepped it.
    ticks(&mut state, 2);
    let bird = magpie(&state);
    let target = bird.target.expect("another coin") as usize;
    assert_ne!(target, chosen, "the drained coin is not gone for");
    assert!(cells(&state)[target].position.y > layout.floor_y - layout.pitch);
    assert!(
        (cells(&state)[chosen].position - before).length() > layout.pitch * 5.0,
        "the coin really left"
    );
    assert!(matches!(
        bird.phase,
        ClockMagpiePhase::Landed | ClockMagpiePhase::Hopping | ClockMagpiePhase::Flying
    ));
    while state.event_phase() == Some(EventPhase::Gathering) {
        ticks(&mut state, 1);
    }
    let coins = state.coins_state().unwrap();
    assert!(coins.taken >= 2, "{coins:?}");
    assert!(
        !cells(&state).iter().any(|cell| cell.taken),
        "the return holds only the coins that remain"
    );
}

/// A resize or a replacing preview in the middle of the gathering, with a
/// coin in the bird's beak, releases the pile, the bird and the surfaces
/// once, and the face is whole on the next frame.
#[test]
fn a_gathering_cut_short_releases_the_pile_the_bird_and_the_surfaces_once() {
    for (replace, elapsed) in [
        (false, 120),
        (true, 120),
        (false, GATHERING_TICKS / 2),
        (true, GATHERING_TICKS / 2),
    ] {
        let context = format!("replace={replace} elapsed={elapsed}");
        let mut state = ready(4.0 / 3.0, 11, ClockTimeFormat::TwelveHour);
        let lit = state
            .segments
            .iter()
            .filter(|s| s.lit)
            .map(|s| s.cells().count())
            .sum::<usize>();
        state.preview_event(ClockEventKind::Coins);
        ticks(&mut state, GATHERING_AT + elapsed);
        // Run on to a frame with the coin in the beak, so the cut comes with
        // a surface already retired as taken.
        while !magpie(&state).carrying && state.event_phase() == Some(EventPhase::Gathering) {
            ticks(&mut state, 1);
        }
        assert!(magpie(&state).carrying, "{context}");
        let taken = state.coins_state().unwrap().taken;
        assert!(taken >= 1, "{context}");
        if replace {
            state.preview_event(ClockEventKind::ColorCycle);
            assert_eq!(
                state.event_kind(),
                Some(ClockEventKind::ColorCycle),
                "{context}"
            );
        } else {
            state.set_aspect_ratio(5.0 / 3.0);
            assert_eq!(state.event_kind(), None, "{context}");
        }
        assert!(state.coins_state().is_none(), "{context}");
        assert_eq!(
            (state.body_count(), state.collider_count()),
            (0, 0),
            "{context}"
        );
        assert_eq!(state.floor_mode(), ClockFloorMode::Closed, "{context}");
        assert_eq!(
            bird_primitives(&state),
            (0, 0),
            "{context}: nothing of the bird stays"
        );
        // The face takes its surfaces back at the next step.
        ticks(&mut state, 1);
        let accounting = state.surface_accounting();
        assert_eq!(
            accounting.retired(Retirement::Taken),
            u64::from(taken),
            "{context}: the coin in the beak was retired once"
        );
        assert_eq!(
            accounting.live, lit,
            "{context}: one surface per lit cell again"
        );
    }
}

/// Beside a duck, the magpie takes coins out of the run's arena, and the
/// return leaves the arena holding only what is not the batch's.
#[test]
fn the_magpie_takes_coins_out_of_the_shared_arena() {
    let mut state = ready(4.0 / 3.0, 42, ClockTimeFormat::TwelveHour);
    toggle(&mut state, 1);
    ticks(&mut state, 90);
    state.finish_event();
    let base = (state.body_count(), state.collider_count());
    state.preview_event(ClockEventKind::Coins);
    assert!(state.coins_state().unwrap().magpie.is_some());
    ticks(&mut state, GATHERING_AT);
    let count = cells(&state).len();
    assert_eq!(state.body_count(), base.0 + count);
    while state.event_phase() == Some(EventPhase::Gathering) {
        let coins = state.coins_state().unwrap();
        assert!(coins.shared_arena);
        assert_eq!(state.body_count(), base.0 + count - coins.taken as usize);
        assert_eq!(
            state.collider_count(),
            base.1 + count - coins.taken as usize
        );
        assert!(
            state
                .arena
                .world()
                .unwrap()
                .motions()
                .filter(|m| arena::ids::canonical::COIN_CELLS.contains(&m.id.entity.value()))
                .count()
                == count - coins.taken as usize,
            "the arena holds exactly the coins still in the pile"
        );
        ticks(&mut state, 1);
    }
    let taken = state.coins_state().unwrap().taken;
    assert!((3..=4).contains(&taken), "{taken}");
    assert_eq!((state.body_count(), state.collider_count()), base);
    ticks(&mut state, REFORMING_TICKS);
    assert!(state.coins_state().is_none());
    assert_eq!((state.body_count(), state.collider_count()), base);
    assert!(
        !state
            .arena
            .holds(arena::Owner::Event(ClockEventKind::Coins))
    );
}

#[test]
fn the_magpie_never_sinks_below_the_floor() {
    // A descent cut short by a retarget kept its downward momentum, and the
    // floor caught only feet that came down near the new destination: at
    // 800x480 in Matrix with seed 9 the bird went 4.6 units into the floor on
    // its way to the next coin.
    for (aspect, font, seed) in [
        (800.0 / 480.0, ClockFont::Matrix, 9),
        (4.0 / 3.0, ClockFont::Matrix, 9),
        (800.0 / 480.0, ClockFont::Classic, 3),
    ] {
        let layout = Layout::new(aspect);
        let mut state = ready_in(aspect, seed, font);
        state.preview_event(ClockEventKind::Coins);
        ticks(&mut state, GATHERING_AT);
        let mut lowest = f32::INFINITY;
        while state.event_phase() == Some(EventPhase::Gathering) {
            let bird = magpie(&state);
            let feet = feet(&state);
            if bird.phase != ClockMagpiePhase::Away
                && feet.x > layout.bounds_min.x
                && feet.x < layout.bounds_max.x
            {
                lowest = lowest.min(feet.y);
                assert!(
                    feet.y >= layout.floor_y - 0.01,
                    "{font:?} at {aspect} seed {seed}: feet at {feet:?} are below the floor at {} while {:?}",
                    layout.floor_y,
                    bird.phase
                );
            }
            ticks(&mut state, 1);
        }
        assert!(
            lowest.is_finite(),
            "{font:?} at {aspect} seed {seed}: the bird came"
        );
    }
}

#[test]
fn the_magpie_lands_on_the_course_it_shares_with_a_duck() {
    // Beside a player's duck the coins lie on its course, with platforms and
    // a pit where the batch's own floor has its drain. The bird sets down on
    // what is there: taking the floor for granted, at 4:3 seed 11 landed over
    // the pit and seed 22 put its feet 16 units inside a platform.
    let aspect = 4.0 / 3.0;
    let layout = Layout::new(aspect);
    for seed in [11, 22, 3, 7] {
        let mut state = ready(aspect, seed, ClockTimeFormat::TwelveHour);
        toggle(&mut state, 1);
        ticks(&mut state, 90);
        state.finish_event();
        state.preview_event(ClockEventKind::Coins);
        assert!(state.coins_state().unwrap().shared_arena);
        ticks(&mut state, GATHERING_AT);
        let mut landings = 0;
        while state.event_phase() == Some(EventPhase::Gathering) {
            let bird = magpie(&state);
            let feet = feet(&state);
            let terrain = state
                .duck_visit
                .as_ref()
                .map(|duck| &duck.terrain)
                .or(state.arena.vacant.as_ref())
                .expect("the duck's course");
            let support = Support::Terrain(terrain);
            if bird.phase != ClockMagpiePhase::Away
                && feet.x > layout.bounds_min.x
                && feet.x < layout.bounds_max.x
            {
                if let Some(top) = support.at(feet.x) {
                    assert!(
                        feet.y >= top - 0.01,
                        "seed {seed}: feet at {feet:?} are inside the course, whose top is {top} there, while {:?}",
                        bird.phase
                    );
                }
                if bird.phase == ClockMagpiePhase::Landed && bird.phase_tick == 0 {
                    landings += 1;
                    let top = support.at(feet.x);
                    assert!(
                        top.is_some_and(|top| (feet.y - top).abs() < 0.01),
                        "seed {seed}: landed at {feet:?} with {top:?} under the feet"
                    );
                }
            }
            ticks(&mut state, 1);
        }
        assert!(landings >= 1, "seed {seed}: the bird landed");
        assert!(
            state.coins_state().unwrap().taken >= 1,
            "seed {seed}: the bird took a coin off the course"
        );
    }
}
