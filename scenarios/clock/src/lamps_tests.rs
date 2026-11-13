//! The corner lamps in a running Clock.

use super::*;
use crate::lamps::PRESENT_NARROW;
use engine_common::{
    ClockLampActivity, ClockLampMode, ClockLampSide, ClockLampTemperament, ClockLampsState,
    RenderColor, RenderFrame, RenderPoint, RenderPrimitive,
};
use engine_core::Vec2;
use std::time::Duration;

const TICK: Duration = Duration::from_nanos(16_666_667);

fn clock(profile: ClockEventProfile, lamps: ClockLampMode, seed: u64) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: profile,
            lamps,
            ..ClockConfig::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(
            ClockReading::new(12, 39, 50).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

fn sides(lamps: &ClockLampsState) -> [engine_common::ClockLampBrainState; 2] {
    [lamps.left, lamps.right]
}

/// A ball rolled along the floor from `from` cells of the room's width, at
/// `speed` cells a second, and the entity of its body.
fn roll(state: &mut ClockState, from: f32, speed: f32) -> (usize, u64) {
    let index = state
        .admit_arena_fixture(&[state.rolling_arena_ball(from, speed)])
        .expect("the arena admits the ball");
    let entity = crate::arena::ids::canonical::FIXTURE_OWNER_BALLS[index][0].start;
    (index, entity)
}

/// The left lamp's temperament at each of these seeds: eager at 7, lazy at
/// 1234.
const SEEDS: [(u64, ClockLampTemperament); 2] = [
    (7, ClockLampTemperament::Eager),
    (1234, ClockLampTemperament::Lazy),
];

/// How soon a lamp follows a ball rolled on its side; how far its beam
/// strays from the ball, as seen from the lamp's head, once it has followed
/// it for half a second; and how soon both lamps are back at rest after the
/// ball stops or leaves the room.
const FOLLOWS_WITHIN: u32 = FIXED_HZ * 3 / 4;
const BALL_DEGREES: f32 = 8.0;
const RESTS_AFTER_STOPPING: u32 = 2 * FIXED_HZ;
const RESTS_AFTER_LEAVING: u32 = 3 * FIXED_HZ;

fn degrees_off(lamp: &engine_common::ClockLampBrainState, head: Vec2, point: Vec2) -> f32 {
    let aim = Vec2::new(lamp.aim_milli[0] as f32, lamp.aim_milli[1] as f32) * 0.001 - head;
    let to = point - head;
    (aim.dot(to) / (aim.length() * to.length()))
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

fn resting(lamp: &engine_common::ClockLampBrainState) -> bool {
    lamp.state == ClockLampActivity::Rest
        && lamp.narrow_milli == 0
        && lamp.brightness_milli == (lamps::REST_BRIGHTNESS * 1000.0).round() as u16
}

#[test]
fn a_ball_rolled_across_the_floor_is_followed_handed_over_at_the_middle_and_let_go() {
    use ClockLampActivity::{Notice, Rest, Search, Track};
    for (seed, left) in SEEDS {
        let mut state = clock(ClockEventProfile::Off, ClockLampMode::Lively, seed);
        assert_eq!(state.lamps_state().left.temperament, left, "seed {seed}");
        for _ in 0..FIXED_HZ {
            ClockScenario::step(&mut state, &[], TICK);
        }
        let layout = Layout::new(state.aspect_ratio());
        let middle = (layout.bounds_min.x + layout.bounds_max.x) * 0.5;
        let heads = [lamps::head(0, &layout), lamps::head(1, &layout)];
        let (index, entity) = roll(&mut state, 3.0, 15.0);
        let follows = |lamp: &engine_common::ClockLampBrainState| {
            lamp.state == Track && lamp.target.is_some_and(|target| target.entity == entity)
        };
        let mut tracked: [Option<u32>; 2] = [None; 2];
        let (mut worst, mut handed_at) = (0.0f32, None);
        let mut tick = 0;
        // Until the ball rolls out of the room.
        let gone = loop {
            ClockScenario::step(&mut state, &[], TICK);
            tick += 1;
            let lamps = state.lamps_state();
            let fixture = state.arena_fixture(index).expect("the fixture stays");
            let Some(ball) = fixture
                .balls
                .first()
                .filter(|ball| ball.position.x - fixture.radius < layout.bounds_max.x)
            else {
                break tick;
            };
            let sides = sides(&lamps);
            assert!(
                !(follows(&sides[0]) && follows(&sides[1])),
                "seed {seed} tick {tick}: both follow it"
            );
            for (lamp, side) in sides.iter().enumerate() {
                if !follows(side) {
                    continue;
                }
                let from = *tracked[lamp].get_or_insert(tick);
                if tick >= from + FIXED_HZ / 2 {
                    worst = worst.max(degrees_off(side, heads[lamp], ball.position));
                }
            }
            // The left lamp has it until it is a cell past the middle, and
            // the right lamp from there on.
            let past = ball.position.x - middle;
            if past < 0.0 {
                assert!(
                    matches!(sides[1].state, Rest) && sides[1].target.is_none(),
                    "seed {seed} tick {tick}: the right lamp {:?}",
                    sides[1].state
                );
            }
            if past > layout.pitch * 1.2 {
                assert!(
                    follows(&sides[1]),
                    "seed {seed} tick {tick}: the right lamp {:?}",
                    sides[1].state
                );
                assert_eq!(sides[0].state, Rest, "seed {seed} tick {tick}");
                handed_at.get_or_insert(tick);
            }
            if tick == FIXED_HZ / 2 {
                assert!(
                    matches!(sides[0].state, Notice | Track),
                    "seed {seed}: the left lamp {:?} half a second on",
                    sides[0].state
                );
            }
        };
        let left_from = tracked[0].expect("the left lamp followed the ball");
        let right_from = tracked[1].expect("the right lamp took the ball over");
        assert!(
            left_from <= FOLLOWS_WITHIN,
            "seed {seed}: followed from {left_from}"
        );
        assert!(right_from < gone && handed_at.is_some(), "seed {seed}");
        assert!(worst <= BALL_DEGREES, "seed {seed}: strayed {worst:.2}°");
        let lamps = state.lamps_state();
        assert_eq!(
            (lamps.notices, lamps.tracks, lamps.handoffs),
            (1, 2, 1),
            "seed {seed}"
        );
        // Gone from the room, it is searched for, then let go.
        let mut rested = None;
        for after in 0..5 * FIXED_HZ {
            ClockScenario::step(&mut state, &[], TICK);
            let lamps = state.lamps_state();
            if after < FIXED_HZ {
                assert!(
                    matches!(lamps.right.state, Search | Rest),
                    "seed {seed}: {:?}",
                    lamps.right.state
                );
            }
            if resting(&lamps.left) && resting(&lamps.right) {
                rested.get_or_insert(after);
            } else {
                assert!(rested.is_none(), "seed {seed}: stirred at {after}");
            }
        }
        let rested = rested.expect("the lamps came back to rest");
        assert!(
            rested <= RESTS_AFTER_LEAVING,
            "seed {seed}: rested {rested} ticks after it left"
        );
    }
}

#[test]
fn a_lamp_rests_soon_after_the_ball_it_follows_stops() {
    for (seed, _) in SEEDS {
        let mut state = clock(ClockEventProfile::Off, ClockLampMode::Lively, seed);
        for _ in 0..FIXED_HZ {
            ClockScenario::step(&mut state, &[], TICK);
        }
        let (index, entity) = roll(&mut state, 3.0, 15.0);
        for _ in 0..FIXED_HZ {
            ClockScenario::step(&mut state, &[], TICK);
        }
        let lamps = state.lamps_state();
        assert_eq!(lamps.left.state, ClockLampActivity::Track, "seed {seed}");
        assert_eq!(lamps.left.target.map(|target| target.entity), Some(entity));
        // It stops dead on the floor, where the lamp watches it settle, then
        // rests.
        let body = engine_rapier::world::BodyId::new(
            engine_rapier::world::PhysicsId::new(entity),
            engine_rapier::world::BodyRole::PRIMARY,
        );
        let world = state.arena.world_mut().expect("the arena's world");
        assert!(world.set_velocity(body, Vec2::ZERO, 0.0, true));
        let mut rested = None;
        for after in 0..4 * FIXED_HZ {
            ClockScenario::step(&mut state, &[], TICK);
            let lamps = state.lamps_state();
            let ball = state.arena_fixture(index).unwrap().balls[0];
            assert!(ball.velocity.length() < Layout::new(state.aspect_ratio()).pitch * 0.5);
            if resting(&lamps.left) && resting(&lamps.right) {
                rested.get_or_insert(after);
            } else {
                assert!(rested.is_none(), "seed {seed}: stirred at {after}");
            }
        }
        let rested = rested.expect("the lamp came back to rest");
        assert!(
            rested <= RESTS_AFTER_STOPPING,
            "seed {seed}: rested {rested} ticks after it stopped"
        );
    }
}

/// How long, at least, the lamps of the run below follow movers in the
/// minute before a bulb goes out of heat. It is this run's, not a rule:
/// heat carries over from longer ago, and over an hour some bulbs go out
/// after eight seconds.
const HOT_AFTER_SECONDS: u32 = 10;

#[test]
fn a_bulb_goes_out_after_long_bright_work_and_the_lamps_never_move_the_scene() {
    use ClockLampActivity::{Out, Track};
    // Demo at this seed overheats a bulb during an event in its second
    // minute.
    let seed = 12;
    let mut lively = clock(ClockEventProfile::Demo, ClockLampMode::Lively, seed);
    let mut again = clock(ClockEventProfile::Demo, ClockLampMode::Lively, seed);
    let mut steady = clock(ClockEventProfile::Demo, ClockLampMode::Steady, seed);
    let mut before = lively.lamps_state();
    let mut followed: [std::collections::VecDeque<bool>; 2] = Default::default();
    let mut hot_in_events = 0;
    for tick in 0..3 * 60 * FIXED_HZ {
        for state in [&mut lively, &mut again, &mut steady] {
            tick_with_reading(state, tick);
        }
        // The same seed gives the same lamps, and Steady lamps rest whatever
        // goes on.
        let lamps = lively.lamps_state();
        assert_eq!(lamps, again.lamps_state(), "tick {tick}");
        assert_eq!(steady.lamps.look(0), lamps::Look::STEADY, "tick {tick}");
        assert_eq!(steady.lamps.look(1), lamps::Look::STEADY, "tick {tick}");
        // The lamps act on the scene without changing it: the same events,
        // visits and residents, and every body where it would be.
        assert_eq!(lively.event_id(), steady.event_id(), "tick {tick}");
        assert_eq!(motions(&lively), motions(&steady), "tick {tick}");
        let visits = |state: &ClockState| {
            [
                state.duck_visit.is_some(),
                state.crow_visit.is_some(),
                state.spider_visit.is_some(),
            ]
        };
        assert_eq!(visits(&lively), visits(&steady), "tick {tick}");
        let (acting, resting) = (lively.arena_state(), steady.arena_state());
        assert_eq!(acting.accounting, resting.accounting, "tick {tick}");
        assert_eq!(acting.claims, resting.claims, "tick {tick}");
        assert!(
            !(lamps.left.state == Out && lamps.right.state == Out),
            "tick {tick}"
        );
        for (lamp, (now, then)) in sides(&lamps).iter().zip(sides(&before)).enumerate() {
            followed[lamp].push_back(now.state == Track);
            if followed[lamp].len() > 60 * FIXED_HZ as usize {
                followed[lamp].pop_front();
            }
            if now.state == Out && then.state != Out && then.drives.heat_milli >= 1000 {
                let ticks = followed[lamp].iter().filter(|&&on| on).count() as u32;
                assert!(
                    ticks >= HOT_AFTER_SECONDS * FIXED_HZ,
                    "tick {tick}: lamp {lamp} went out after following for {ticks} ticks"
                );
                hot_in_events += u32::from(lively.event_kind().is_some());
            }
        }
        before = lamps;
    }
    assert!(hot_in_events > 0, "no bulb overheated during an event");
    let lamps = lively.lamps_state();
    assert!(lamps.notices > 0 && lamps.tracks > 0, "{lamps:?}");
}

/// Every body of the scene, where it is and how it moves.
fn motions(state: &ClockState) -> Vec<Vec<engine_rapier::world::BodyMotionRecord>> {
    ClockState::scene_worlds(&state.arena, state.active_event.as_ref())
        .into_iter()
        .flatten()
        .map(|world| world.motions().collect())
        .collect()
}

fn layer(frame: &RenderFrame, z: i32) -> Vec<RenderPrimitive> {
    frame
        .layers
        .iter()
        .filter(|layer| layer.z == z)
        .flat_map(|layer| layer.primitives.iter().cloned())
        .collect()
}

const CREATURE_LAYER: i32 = 8;
const LAMP_LAYER: i32 = 11;

/// How much light one side's lamp spreads around its corner: the summed
/// alpha of its translucent bloom and halo, not its opaque head.
fn glow(frame: &RenderFrame, left: bool) -> f32 {
    layer(frame, LAMP_LAYER)
        .iter()
        .filter_map(|primitive| match primitive {
            RenderPrimitive::Circle(circle) if (circle.center.x < 0.0) == left => {
                circle.fill.as_ref().map(|fill| fill.color.a)
            }
            _ => None,
        })
        .filter(|alpha| *alpha < 1.0)
        .sum()
}

#[test]
fn resting_lamps_draw_the_same_frame_and_an_outage_darkens_only_its_lamp() {
    let mut state = clock(ClockEventProfile::Off, ClockLampMode::Lively, 7);
    let steady = ClockScenario::render_frame(&state);
    for _ in 0..10 * FIXED_HZ {
        ClockScenario::step(&mut state, &[], TICK);
    }
    assert_eq!(ClockScenario::render_frame(&state), steady);
    assert_eq!(
        crate::render::render_unlit(&state),
        steady,
        "rest lights nothing"
    );
    // Out: the right lamp's corner goes dark and the left lamp is untouched.
    state.preview_lamp(ClockLampActivity::Out, ClockLampSide::Right);
    let mut dark = None;
    while state.lamps_state().right.state == ClockLampActivity::Out {
        let right = state.lamps_state().right;
        if right.brightness_milli == 0 && right.state_ticks > 60 {
            dark = Some(ClockScenario::render_frame(&state));
            break;
        }
        ClockScenario::step(&mut state, &[], TICK);
    }
    let dark = dark.expect("an outage goes dark");
    assert_eq!(glow(&dark, true), glow(&steady, true));
    assert!(
        glow(&dark, false) < glow(&steady, false) * 0.2,
        "{} of {}",
        glow(&dark, false),
        glow(&steady, false)
    );
    // It comes back to the very same rest.
    for _ in 0..10 * FIXED_HZ {
        ClockScenario::step(&mut state, &[], TICK);
    }
    assert_eq!(ClockScenario::render_frame(&state), steady);
}

/// `primitive` with its colors made black, as opaque as they were, so two
/// primitives compare by shape.
fn shape(primitive: &RenderPrimitive) -> RenderPrimitive {
    let black = |color: &mut RenderColor| {
        *color = RenderColor::rgba(0.0, 0.0, 0.0, color.a);
    };
    let mut shape = primitive.clone();
    match &mut shape {
        RenderPrimitive::Circle(circle) => {
            circle
                .fill
                .iter_mut()
                .for_each(|fill| black(&mut fill.color));
            circle
                .stroke
                .iter_mut()
                .for_each(|stroke| black(&mut stroke.color));
        }
        RenderPrimitive::Polygon(polygon) => {
            polygon
                .fill
                .iter_mut()
                .for_each(|fill| black(&mut fill.color));
            polygon
                .stroke
                .iter_mut()
                .for_each(|stroke| black(&mut stroke.color));
        }
        RenderPrimitive::Line(line) => black(&mut line.stroke.color),
        RenderPrimitive::Text(text) => black(&mut text.color),
    }
    shape
}

/// Checks that every beam in the frame `state` draws stays in view; that the
/// light only warms what the scene drew in its pools, each thing in its place
/// and order and as opaque as it was; and that the lamps add a bounded number
/// of primitives to `steady`. Returns how many things the light warmed.
fn bounded_light(state: &ClockState, steady: &RenderFrame, aspect: f32) -> usize {
    let frame = ClockScenario::render_frame(state);
    let unlit = crate::render::render_unlit(state);
    let count = |frame: &RenderFrame| {
        frame
            .layers
            .iter()
            .map(|l| l.primitives.len())
            .sum::<usize>()
    };
    let bounds = frame.camera.world_bounds(aspect);
    for primitive in frame.layers.iter().flat_map(|layer| &layer.primitives) {
        if let RenderPrimitive::Polygon(polygon) = primitive {
            for point in &polygon.points {
                assert!(point.x >= bounds.min.x - 1e-3 && point.x <= bounds.max.x + 1e-3);
                assert!(point.y >= bounds.min.y - 1e-3 && point.y <= bounds.max.y + 1e-3);
            }
        }
    }
    // Where each spotlight's pool lies, and how far it reaches.
    let layout = Layout::new(aspect);
    let face = Vec2::new(layout.face_origin.x, layout.face_origin.y);
    let pools: Vec<Vec2> = (0..2)
        .map(|lamp| state.lamps.look(lamp))
        .filter(|look| look.narrow > 0.0)
        .map(|look| face + look.target * layout.pitch)
        .collect();
    let reach = 3.6 * layout.pitch;
    let depths = |frame: &RenderFrame| frame.layers.iter().map(|l| l.z).collect::<Vec<_>>();
    assert_eq!(depths(&frame), depths(&unlit), "{aspect}");
    let mut warmed = 0;
    for z in depths(&frame) {
        let (lit, drawn) = (layer(&frame, z), layer(&unlit, z));
        assert_eq!(lit.len(), drawn.len(), "{aspect}: layer {z}");
        for (lit, drawn) in lit.iter().zip(&drawn) {
            assert_eq!(shape(lit), shape(drawn), "{aspect}: layer {z}");
            if lit != drawn {
                warmed += 1;
                let middle = middle(drawn);
                assert!(
                    pools.iter().any(|pool| pool.distance_to(middle) < reach),
                    "{aspect}: layer {z} warmed outside the pools at {middle:?}"
                );
            }
        }
    }
    // Each lamp adds at most its beam's six fans, its pool's eight discs and
    // its hot core, and the ball a disc.
    assert!(
        count(&frame) <= count(steady) + 2 * (6 + 8 + 1) + 1,
        "{aspect}: {} against {}",
        count(&frame),
        count(steady)
    );
    warmed
}

#[test]
fn a_present_lights_the_cells_in_its_pool_and_every_beam_stays_in_view() {
    for aspect in [800.0 / 480.0, 4.0 / 3.0, 480.0 / 800.0] {
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: aspect,
                event_profile: ClockEventProfile::Off,
                ..ClockConfig::default()
            },
            5,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(23, 58, 0).unwrap(),
            )],
            Duration::ZERO,
        );
        let steady = ClockScenario::render_frame(&state);
        // The left lamp presents the hours, and its pool lights them while
        // it holds.
        state.preview_lamp(ClockLampActivity::Present, ClockLampSide::Left);
        let mut held = 0;
        while state.lamps_state().left.state == ClockLampActivity::Present {
            let lit = bounded_light(&state, &steady, aspect);
            if state.lamps_state().left.narrow_milli == (PRESENT_NARROW * 1000.0) as u16 {
                assert!(lit > 0, "{aspect}: the pool lights nothing");
                held += 1;
            }
            ClockScenario::step(&mut state, &[], TICK);
        }
        assert!(held > FIXED_HZ, "{aspect}: held for {held} ticks");
        // A ball rolled from the right is followed by one lamp, then the
        // other.
        let layout = Layout::new(aspect);
        let width = (layout.bounds_max.x - layout.bounds_min.x) / layout.pitch;
        let (index, _) = roll(&mut state, width - 3.0, -15.0);
        let mut followed = [0u32; 2];
        for _ in 0..5 * FIXED_HZ {
            bounded_light(&state, &steady, aspect);
            for (lamp, side) in sides(&state.lamps_state()).iter().enumerate() {
                followed[lamp] += u32::from(side.state == ClockLampActivity::Track);
            }
            ClockScenario::step(&mut state, &[], TICK);
        }
        assert!(
            followed.iter().all(|&ticks| ticks > FIXED_HZ / 2),
            "{aspect}: {followed:?}"
        );
        // Both come back to the very same rest once the ball is gone.
        state.remove_arena_fixture(index);
        for _ in 0..RESTS_AFTER_LEAVING {
            ClockScenario::step(&mut state, &[], TICK);
        }
        assert_eq!(ClockScenario::render_frame(&state), steady, "{aspect}");
    }
}

/// Where a primitive sits: a disc's center, a line's middle, or the mean of
/// a polygon's points.
fn middle(primitive: &RenderPrimitive) -> Vec2 {
    match primitive {
        RenderPrimitive::Circle(circle) => Vec2::new(circle.center.x, circle.center.y),
        RenderPrimitive::Line(line) => {
            Vec2::new(line.start.x + line.end.x, line.start.y + line.end.y) * 0.5
        }
        RenderPrimitive::Polygon(polygon) => {
            polygon
                .points
                .iter()
                .fold(Vec2::ZERO, |sum, p| sum + Vec2::new(p.x, p.y))
                / polygon.points.len() as f32
        }
        RenderPrimitive::Text(text) => Vec2::new(text.position.x, text.position.y),
    }
}

#[test]
fn a_followed_ball_is_lit_and_nothing_outside_the_pool_changes() {
    let mut state = clock(ClockEventProfile::Off, ClockLampMode::Lively, 7);
    let aspect = state.aspect_ratio();
    let steady = ClockScenario::render_frame(&state);
    roll(&mut state, 3.0, 15.0);
    let mut followed = 0;
    for _ in 0..3 * FIXED_HZ {
        ClockScenario::step(&mut state, &[], TICK);
        // At this seed the left lamp is eager, and follows the ball first.
        let left = state.lamps_state().left;
        if left.state != ClockLampActivity::Track || left.narrow_milli < 1000 {
            continue;
        }
        followed += 1;
        bounded_light(&state, &steady, aspect);
        let frame = ClockScenario::render_frame(&state);
        let unlit = crate::render::render_unlit(&state);
        let ball = state.arena_fixtures()[0].balls[0].position;
        // The ball's disc, as the scene drew it and as the light leaves it.
        let disc = |frame: &RenderFrame| {
            layer(frame, CREATURE_LAYER)
                .into_iter()
                .find_map(|primitive| match primitive {
                    RenderPrimitive::Circle(circle)
                        if circle.center == RenderPoint::new(ball.x, ball.y) =>
                    {
                        circle.fill.map(|fill| fill.color)
                    }
                    _ => None,
                })
                .expect("the ball's disc")
        };
        let (lit, drawn) = (disc(&frame), disc(&unlit));
        let sum = |color: RenderColor| color.r + color.g + color.b;
        assert!(sum(lit) > sum(drawn) + 0.1, "{lit:?} from {drawn:?}");
    }
    assert!(followed > FIXED_HZ / 2, "followed for {followed} ticks");
}

#[test]
fn the_lamps_state_shows_each_lamps_activity_target_and_drives() {
    let mut state = clock(ClockEventProfile::Off, ClockLampMode::Lively, 7);
    let (_, entity) = roll(&mut state, 3.0, 15.0);
    for _ in 0..FIXED_HZ {
        ClockScenario::step(&mut state, &[], TICK);
    }
    let json = serde_json::to_value(state.lamps_state()).unwrap();
    let left = &json["left"];
    assert_eq!(left["temperament"], "eager", "{json}");
    assert_eq!(left["state"], "track", "{json}");
    assert_eq!(left["target"]["world"], 0, "{json}");
    assert_eq!(left["target"]["entity"], entity, "{json}");
    assert_eq!(left["narrow_milli"], 1000, "{json}");
    assert!(
        left["drives"]["interest_milli"].as_u64().unwrap() > 0,
        "{json}"
    );
    assert!(
        left["drives"]["boredom_milli"].as_u64().unwrap() > 0,
        "{json}"
    );
    assert!(left["drives"]["heat_milli"].as_u64().unwrap() > 0, "{json}");
    assert_eq!(json["right"]["state"], "rest", "{json}");
    assert_eq!(json["right"]["target"], serde_json::Value::Null, "{json}");
    assert_eq!(json["notices"], 1, "{json}");
    assert_eq!(json["tracks"], 1, "{json}");
    // Old states without the lamps' minds still read, as resting lamps.
    let old: ClockLampsState = serde_json::from_str(r#"{"outages": 2}"#).unwrap();
    assert_eq!(old.left.state, ClockLampActivity::Rest);
    assert_eq!(old.outages, 2);
}

/// Steps the clock a tick, giving it the reading a real clock would show
/// `tick` ticks after 12:39:50.
fn tick_with_reading(state: &mut ClockState, tick: u32) {
    let actions = if tick.is_multiple_of(FIXED_HZ) {
        let seconds = 12 * 3600 + 39 * 60 + 50 + tick / FIXED_HZ;
        vec![ClockAction::set_reading(
            ClockReading::new(
                (seconds / 3600 % 24) as u8,
                (seconds / 60 % 60) as u8,
                (seconds % 60) as u8,
            )
            .unwrap(),
        )]
    } else {
        Vec::new()
    };
    ClockScenario::step(state, &actions, TICK);
}

/// What the lamps do through an hour of Demo at a few seeds, for tuning the
/// brain: how often each thing happens, the share of the hour each lamp
/// spends at each activity, how long its tracks last, what they follow
/// between events, and each outage with the seconds its lamp tracked in the
/// minute before.
#[test]
#[ignore = "reports an hour of Demo's lamps; run with --ignored --nocapture"]
fn what_an_hour_of_demo_looks_like() {
    use ClockLampActivity::{Out, Track};
    let hour = 60 * 60 * FIXED_HZ;
    for seed in [42, 7, 1234] {
        let mut state = clock(ClockEventProfile::Demo, ClockLampMode::Lively, seed);
        let mut time = [[0u32; ClockLampActivity::ALL.len()]; 2];
        let (mut event_ticks, mut tracking_in_events) = (0, 0);
        let mut recent: [std::collections::VecDeque<bool>; 2] = Default::default();
        let mut outages = Vec::new();
        // Ticks each body was tracked between events, by world and entity.
        let mut between = std::collections::BTreeMap::<(u8, u64), u32>::new();
        let mut before = state.lamps_state();
        for tick in 0..hour {
            tick_with_reading(&mut state, tick);
            let lamps = state.lamps_state();
            let event = state.event_kind();
            let tracking = sides(&lamps).iter().any(|lamp| lamp.state == Track);
            event_ticks += u32::from(event.is_some());
            tracking_in_events += u32::from(event.is_some() && tracking);
            for (lamp, (now, then)) in sides(&lamps).iter().zip(sides(&before)).enumerate() {
                time[lamp][now.state as usize] += 1;
                if let (Track, Some(target), None) = (now.state, now.target, event) {
                    *between.entry((target.world, target.entity)).or_default() += 1;
                }
                recent[lamp].push_back(now.state == Track);
                if recent[lamp].len() > 60 * FIXED_HZ as usize {
                    recent[lamp].pop_front();
                }
                if now.state == Out && then.state != Out {
                    let tracked = recent[lamp].iter().filter(|&&on| on).count();
                    outages.push(format!(
                        "lamp {lamp} at {:.0} s in {} at heat {}, {:.1} s tracked",
                        tick as f32 / FIXED_HZ as f32,
                        event.map_or("no event", |kind| kind.as_str()),
                        then.drives.heat_milli,
                        tracked as f32 / FIXED_HZ as f32
                    ));
                }
            }
            assert!(!(lamps.left.state == Out && lamps.right.state == Out));
            before = lamps;
        }
        let lamps = state.lamps_state();
        println!(
            "seed {seed}: {}/{}; events {:.0}% of the hour, a lamp tracking in {:.0}% of their time",
            lamps.left.temperament.as_str(),
            lamps.right.temperament.as_str(),
            100.0 * event_ticks as f32 / hour as f32,
            100.0 * tracking_in_events as f32 / event_ticks.max(1) as f32
        );
        println!(
            "  notices {} tracks {} handoffs {} startles {} outages {} presents {} fidgets {} dips {}",
            lamps.notices,
            lamps.tracks,
            lamps.handoffs,
            lamps.startles,
            lamps.outages,
            lamps.presents,
            lamps.fidgets,
            lamps.dips
        );
        let tracked: u32 = time.iter().map(|time| time[Track as usize]).sum();
        println!(
            "  tracks last {:.2} s on average",
            tracked as f32 / FIXED_HZ as f32 / lamps.tracks.max(1) as f32
        );
        for (lamp, time) in time.iter().enumerate() {
            let shares: Vec<String> = ClockLampActivity::ALL
                .iter()
                .map(|activity| {
                    let share = 100.0 * time[*activity as usize] as f32 / hour as f32;
                    format!("{} {share:.1}%", activity.as_str())
                })
                .collect();
            println!("  lamp {lamp}: {}", shares.join(", "));
        }
        let mut between: Vec<_> = between.into_iter().collect();
        between.sort_by_key(|(_, ticks)| std::cmp::Reverse(*ticks));
        let between: Vec<String> = between
            .iter()
            .take(4)
            .map(|((world, entity), ticks)| {
                format!("{world}:{entity} {:.0} s", *ticks as f32 / FIXED_HZ as f32)
            })
            .collect();
        println!("  tracked between events: {}", between.join(", "));
        for outage in outages {
            println!("  out: {outage}");
        }
    }
}
