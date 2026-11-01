//! The spider on the actual digits: what it may hold, how a cell carries it
//! and lets it go, what the frame gives it, and what its stay in the arena
//! leaves behind. Every surface it holds is checked against the published
//! contract and the drawn face, not only against where the spider ends up.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
};

use super::*;
use crate::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState, MAX_ARENA_CLAIMS,
    arena_fixture::FixtureStart,
    digit_surface_tests::{ASPECTS, FONTS, inside, reading, sync, tick},
    events::digit_slide::DIGIT_SLIDE_TICKS,
    render::digit_cell_polygons,
};
use engine_common::{
    ClockArenaOwner, ClockEventKind, ClockEventProfile, ClockEvents, ClockFont, ClockFontSettings,
    ClockMarqueePreset, ClockTimeFormat, Scenario,
};
use engine_rapier::{
    spider::{MAX_NEARBY, MAX_RAYS, SpiderMode, SpiderReleaseReason},
    world::physics_activity,
};

mod capture;

const SEED: u64 = 7;
/// Clock's wide landscape view.
const ASPECT: f32 = 800.0 / 480.0;

/// What a drawing of a tick needs: the spider, the face and the room.
#[derive(Clone)]
pub(super) struct Frame {
    pub phase: &'static str,
    pub fixture: SpiderFixtureState,
    pub surfaces: Vec<DigitSurface>,
    /// The lit cells as drawn, with their ink.
    pub drawn: Vec<(Vec<Vec2>, f32)>,
    pub floor: Vec<(Vec2, Vec2)>,
}

thread_local! {
    /// The frames of a route, while the capture records them.
    static TRACE: RefCell<Option<Vec<Frame>>> = const { RefCell::new(None) };
    static PHASE: Cell<&'static str> = const { Cell::new("") };
}

/// Names the part of the route the next frames belong to.
fn phase(name: &'static str) {
    PHASE.with(|phase| phase.set(name));
}

/// Records a frame of `state`, while the capture is recording.
fn trace(state: &ClockState) {
    TRACE.with(|trace| {
        if let Some(frames) = trace.borrow_mut().as_mut() {
            let layout = Layout::new(state.aspect_ratio());
            frames.push(Frame {
                phase: PHASE.with(Cell::get),
                fixture: state.spider_state().expect("a spider"),
                surfaces: state.digit_surfaces(),
                drawn: digit_cell_polygons(state),
                floor: state.floor.geometry(layout).support().collect(),
            });
        }
    });
}

/// Runs `route` recording a frame of every tick the spider is driven, and
/// returns the frames.
pub(super) fn record(route: impl FnOnce()) -> Vec<Frame> {
    TRACE.with(|trace| *trace.borrow_mut() = Some(Vec::new()));
    route();
    TRACE.with(|trace| trace.borrow_mut().take().expect("the recording"))
}

/// A clock whose only event is the slide a minute change brings, in `font`.
fn config(aspect: f32, font: ClockFont, fixture: FixtureStart) -> ClockConfig {
    ClockConfig {
        aspect_ratio: aspect,
        event_profile: ClockEventProfile::Calm,
        events: ClockEvents {
            falling: false,
            color_cycle: false,
            meltdown: false,
            duck: false,
            marquee: false,
            digit_slide: true,
            rain: false,
            crow: false,
            explosion: false,
            aquarium: false,
            cell_swap: false,
            grass: false,
            slot_machine: false,
        },
        time_format: ClockTimeFormat::TwentyFourHour,
        fonts: ClockFontSettings {
            selected: font,
            rotate: false,
            ..ClockFontSettings::default()
        },
        arena_fixture: fixture,
        ..ClockConfig::default()
    }
}

fn clock_with(aspect: f32, font: ClockFont, fixture: FixtureStart, at: ClockReading) -> ClockState {
    let mut state = ClockScenario::init(config(aspect, font, fixture), SEED);
    sync(&mut state, &[ClockAction::set_reading(at)]);
    state
}

fn clock(aspect: f32, font: ClockFont) -> ClockState {
    clock_with(aspect, font, FixtureStart::None, reading(12, 39, 58))
}

fn idle() -> SpiderIntent {
    SpiderIntent::default()
}

fn grip() -> SpiderIntent {
    SpiderIntent {
        grip: true,
        ..idle()
    }
}

fn crawl(direction: f32) -> SpiderIntent {
    SpiderIntent {
        crawl: direction,
        ..grip()
    }
}

fn reel(amount: f32) -> SpiderIntent {
    SpiderIntent {
        reel: amount,
        ..grip()
    }
}

fn shoot_up() -> SpiderIntent {
    SpiderIntent {
        shoot: Some(Vec2::Y),
        ..grip()
    }
}

/// One moving tick with `intent`, checking the bounds every tick keeps.
fn drive(state: &mut ClockState, intent: SpiderIntent) -> SpiderFixtureState {
    assert!(state.set_spider_intent(intent));
    tick(state, &[]);
    trace(state);
    let fixture = state.spider_state().expect("the spider stays");
    assert!(
        fixture.spider.work.rays <= MAX_RAYS,
        "{:?}",
        fixture.spider.work
    );
    assert!(
        fixture.spider.work.nearby <= MAX_NEARBY,
        "{:?}",
        fixture.spider.work
    );
    assert!(fixture.proxies <= MAX_PROXIES);
    assert_eq!(
        fixture.unproxied, 0,
        "the shipped fonts never overflow the proxies"
    );
    assert_eq!(
        fixture.spider.joints,
        usize::from(fixture.spider.line.is_some())
    );
    assert!(state.arena_state().claims.len() <= MAX_ARENA_CLAIMS);
    fixture
}

/// Ticks with `intent` until `done`, at most `limit` ticks.
fn drive_until(
    state: &mut ClockState,
    limit: u32,
    intent: SpiderIntent,
    done: impl Fn(&SpiderFixtureState) -> bool,
    what: &str,
) -> SpiderFixtureState {
    for _ in 0..limit {
        let fixture = drive(state, intent);
        if done(&fixture) {
            return fixture;
        }
    }
    panic!(
        "{what}: not within {limit} ticks: {:?}",
        state.spider_state()
    );
}

/// A spot resting on the closed floor at `x`.
fn on_floor(layout: Layout, x: f32) -> Vec2 {
    Vec2::new(x, layout.floor_y + SpiderFixture::radius(layout) * 1.05)
}

/// A spot on the floor clear of the arena fixture's balls, which rest over
/// the drain and at a little over half the half width to each side.
fn on_floor_clear_of_balls(layout: Layout) -> Vec2 {
    on_floor(layout, 0.25 * layout.bounds_max.x)
}

/// The surface of `slot` the spider aims for: the lowest lit cell, the one
/// nearest the slot's middle among those.
fn lowest_cell(state: &ClockState, slot: u8) -> DigitSurface {
    let layout = Layout::new(state.aspect_ratio());
    let middle = layout.digit_origin(usize::from(slot)).x + 3.0 * layout.pitch;
    let surfaces = state.digit_surfaces();
    let lowest = surfaces
        .iter()
        .filter(|s| s.slot == slot)
        .map(|s| s.origin.y)
        .fold(f32::MAX, f32::min);
    *surfaces
        .iter()
        .filter(|s| s.slot == slot && (s.origin.y - lowest).abs() < 1e-3)
        .min_by(|a, b| {
            (a.origin.x - middle)
                .abs()
                .total_cmp(&(b.origin.x - middle).abs())
        })
        .expect("a lit slot")
}

/// Up the tied line until the legs catch what it is tied to: the grip is
/// off on the way, so the legs do not hold the spider to the floor against
/// the reel, and on near the anchor.
fn climb_the_line(state: &mut ClockState, context: &str) -> SpiderFixtureState {
    let r = state.spider_state().expect("a spider").spider.radius;
    drive_until(
        state,
        600,
        SpiderIntent {
            reel: 1.0,
            ..idle()
        },
        |f| f.spider.line.is_some_and(|line| line.distance <= 1.6 * r),
        &format!("{context}: reels up the line"),
    );
    drive_until(
        state,
        120,
        reel(1.0),
        |f| held_surface(f).is_some(),
        &format!("{context}: catches what the line is tied to"),
    )
}

/// How many rigid bodies the spider may hold now.
fn fixture_rigid_peak(state: &ClockState) -> usize {
    state.spider_state().expect("a spider").rigid
}

/// Whether the spider holds a surface, and which.
fn held_surface(fixture: &SpiderFixtureState) -> Option<SurfaceId> {
    match fixture.support {
        Some(Hold::Surface { id, .. }) => Some(id),
        _ => None,
    }
}

fn on_floor_still(fixture: &SpiderFixtureState) -> bool {
    fixture.support == Some(Hold::Floor)
        && fixture.spider.velocity.length() < 1e-2 * fixture.spider.radius
}

/// The held point is on the surface the spider says it holds, which is
/// published as the fixture says and drawn where the point is.
fn assert_held_on_the_face(state: &ClockState, fixture: &SpiderFixtureState, context: &str) {
    let layout = Layout::new(state.aspect_ratio());
    let Some(Hold::Surface {
        id,
        source,
        departing,
    }) = fixture.support
    else {
        panic!("{context}: holds {:?}, not a surface", fixture.support);
    };
    let surfaces = state.digit_surfaces();
    let surface = surfaces
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("{context}: surface {} is published", id.get()));
    assert_eq!(
        (surface.source, surface.departing),
        (source, departing),
        "{context}"
    );
    let support = fixture.spider.support.expect("a held support");
    let local = surface.local(support.point);
    let visible = surface
        .visible()
        .unwrap_or_else(|| panic!("{context}: a held surface is drawn"));
    let slack = 0.05;
    assert!(
        (visible.min.x - slack..=visible.max.x + slack).contains(&local.x)
            && (visible.min.y - slack..=visible.max.y + slack).contains(&local.y),
        "{context}: held at {local:?} of {visible:?}"
    );
    let drawn = digit_cell_polygons(state);
    assert!(
        drawn.iter().any(|(polygon, ink)| {
            *ink >= 0.02 && inside(polygon, support.point, 0.02 * layout.pitch)
        }),
        "{context}: the held point {:?} is drawn",
        support.point
    );
}

/// Every proxy stands where its surface is drawn, and every drawn surface
/// without a rigid body has one.
fn assert_proxies_match_the_face(state: &ClockState, context: &str) {
    let layout = Layout::new(state.aspect_ratio());
    let world = state.arena.world().expect("the spider's world");
    let fixture = state.spider_state().expect("a spider");
    let surfaces = state.digit_surfaces();
    let (low, high) = SpiderFixture::room(layout);
    let within = |c: &Vec2| (low.x..=high.x).contains(&c.x) && (low.y..=high.y).contains(&c.y);
    let in_room = |s: &DigitSurface| {
        let Some(visible) = s.visible() else {
            return false;
        };
        let corners = [
            visible.min,
            Vec2::new(visible.max.x, visible.min.y),
            visible.max,
            Vec2::new(visible.min.x, visible.max.y),
        ]
        .map(|local| s.world(local));
        if s.angle.abs() < 1e-6 {
            // Clipped to the room: some of it lies inside.
            corners[0].x < high.x
                && corners[2].x > low.x
                && corners[0].y < high.y
                && corners[2].y > low.y
        } else {
            corners.iter().all(within)
        }
    };
    let wanted: Vec<&DigitSurface> = surfaces
        .iter()
        .filter(|s| s.opacity >= 0.02)
        .filter(|s| s.body.is_none_or(|body| !world.contains_body(body)))
        .filter(|s| in_room(s))
        .collect();
    assert_eq!(fixture.proxies, wanted.len(), "{context}");
    let mut proxied = 0;
    for index in 0..MAX_PROXIES {
        let Some((min, max)) = world.body_solid_bounds(proxy_body(index)) else {
            continue;
        };
        proxied += 1;
        let slot = state.spider.as_ref().unwrap().slots[index].expect("a proxy in the table");
        let surface = wanted
            .iter()
            .find(|s| s.id == slot.id)
            .unwrap_or_else(|| panic!("{context}: proxy {index} stands for a drawn surface"));
        let visible = surface.visible().unwrap();
        let corners = [
            visible.min,
            Vec2::new(visible.max.x, visible.min.y),
            visible.max,
            Vec2::new(visible.min.x, visible.max.y),
        ]
        .map(|local| surface.world(local));
        let mut expected_min = corners.iter().fold(Vec2::new(f32::MAX, f32::MAX), |m, c| {
            Vec2::new(m.x.min(c.x), m.y.min(c.y))
        });
        let mut expected_max = corners.iter().fold(Vec2::new(f32::MIN, f32::MIN), |m, c| {
            Vec2::new(m.x.max(c.x), m.y.max(c.y))
        });
        if surface.angle.abs() < 1e-6 {
            // An upright proxy stops at the room's inside.
            expected_min = Vec2::new(expected_min.x.max(low.x), expected_min.y.max(low.y));
            expected_max = Vec2::new(expected_max.x.min(high.x), expected_max.y.min(high.y));
        }
        let tolerance = 2e-3 * layout.pitch;
        assert!(
            min.distance_to(expected_min) < tolerance && max.distance_to(expected_max) < tolerance,
            "{context}: surface {} is drawn from {expected_min:?} to {expected_max:?}, its proxy stands from {min:?} to {max:?}",
            surface.id.get()
        );
    }
    assert_eq!(proxied, wanted.len(), "{context}");
}

#[test]
fn a_spider_is_admitted_with_its_frame_and_a_proxy_for_every_drawn_cell() {
    for font in FONTS {
        for aspect in ASPECTS {
            let context = format!("{font:?} at {aspect}");
            let mut state = clock(aspect, font);
            let layout = Layout::new(aspect);
            assert_eq!(
                state.admit_spider(on_floor(layout, 0.0)),
                Ok(()),
                "{context}"
            );
            assert_eq!(state.spider_accounting().admitted, 1);
            assert_proxies_match_the_face(&state, &context);
            let world = state.arena.world().unwrap();
            for part in FramePart::ALL {
                let (min, max) = part.rect(layout);
                let body = BodyId::new(part.entity(), BodyRole::PRIMARY);
                let (at_min, at_max) = world.body_solid_bounds(body).expect("a frame part");
                assert!(
                    at_min.distance_to(min) < 1e-3 && at_max.distance_to(max) < 1e-3,
                    "{context}: {part:?}"
                );
            }
            // The hatch is open above the face's middle, and the beam spans it.
            let (left, left_end) = FramePart::CeilingLeft.rect(layout);
            let (right, right_end) = FramePart::CeilingRight.rect(layout);
            assert_eq!(left.x, layout.bounds_min.x);
            assert_eq!(right_end.x, layout.bounds_max.x);
            assert!((right.x - left_end.x - HATCH_WIDTH_PITCH * layout.pitch).abs() < 1e-3);
            let (beam, beam_end) = FramePart::Beam.rect(layout);
            assert!(beam.x < left_end.x && beam_end.x > right.x && beam.y > layout.bounds_max.y);
            let claims = state.arena_state().claims;
            assert_eq!(claims.len(), 1);
            assert_eq!(claims[0].owner, ClockArenaOwner::Spider);
            assert!(!claims[0].resident);
            let population = claims[0].population;
            assert_eq!(
                population.bodies,
                1 + FramePart::ALL.len() + state.digit_surfaces().len()
            );
            assert_eq!(population.colliders, population.bodies);
            assert_eq!(population.joints, 0);
            // A second spider is refused, changing nothing.
            assert_eq!(
                state.admit_spider(on_floor(layout, 0.0)),
                Err(SpiderRefusal::Present)
            );
            assert_eq!(state.spider_accounting().refused, 1);
            assert_eq!(state.arena_state().claims.len(), 1);
        }
    }
}

#[test]
fn a_spider_is_refused_a_start_inside_the_rooms_solids_or_outside_it() {
    let mut state = clock(ASPECT, ClockFont::Classic);
    let layout = Layout::new(ASPECT);
    let r = SpiderFixture::radius(layout);
    let cell = lowest_cell(&state, 2);
    let (rail, _) = FramePart::LeftRail.rect(layout);
    let starts = [
        ("in a digit cell", cell.origin),
        (
            "touching a cell",
            cell.origin - Vec2::new(0.0, 0.4 * layout.pitch + 0.9 * r),
        ),
        ("in the floor", Vec2::new(0.0, layout.floor_y - r)),
        (
            "in the left rail",
            Vec2::new(rail.x + layout.frame_width * 0.5, 0.0),
        ),
        (
            "in the ceiling",
            Vec2::new(layout.bounds_max.x * 0.5, layout.bounds_max.y),
        ),
        (
            "above the beam",
            Vec2::new(0.0, layout.bounds_max.y + 3.0 * layout.pitch),
        ),
        ("beyond the view", Vec2::new(layout.bounds_max.x + 1.0, 0.0)),
        ("nowhere", Vec2::new(f32::NAN, 0.0)),
    ];
    for (case, start) in starts {
        assert_eq!(
            state.admit_spider(start),
            Err(SpiderRefusal::Blocked),
            "{case}"
        );
        assert!(state.spider_state().is_none(), "{case}");
        assert!(state.arena.idle(), "{case}: nothing of the spider stays");
    }
    assert_eq!(state.spider_accounting().refused, starts.len() as u64);
    // Just clear of the same cell, and in the hatch, it is admitted.
    assert_eq!(
        state.admit_spider(cell.origin - Vec2::new(0.0, 0.4 * layout.pitch + 1.1 * r)),
        Ok(())
    );
    assert!(state.remove_spider());
    assert_eq!(
        state.admit_spider(SpiderFixture::under_beam(layout)),
        Ok(())
    );
    assert_eq!(state.spider_accounting().admitted, 2);
}

/// The route the acceptance fixture takes: up to a digit on a line, along
/// its underside, off the end into a swing on the line, back up, and then
/// out with the old glyph when the minute changes.
fn climb_swing_and_ride_out(aspect: f32, font: ClockFont, residents: FixtureStart) {
    let context = format!("{font:?} at {aspect} with {residents:?}");
    let mut state = clock_with(aspect, font, residents, reading(12, 39, 58));
    let layout = Layout::new(aspect);
    let r = SpiderFixture::radius(layout);
    // Residents settle first.
    for _ in 0..240 {
        tick(&mut state, &[]);
    }
    let balls = state.arena_fixtures();
    let worlds = state.arena_accounting().worlds;
    let target = lowest_cell(&state, 2);
    let face_bottom = target.origin.y - 0.4 * layout.pitch;
    // From the floor when the shot reaches the face from it, and otherwise
    // from the air beneath the face, as in a portrait view.
    let silk_range = 40.0 * r;
    let from_floor = face_bottom - layout.floor_y < silk_range * 0.9;
    let start = if from_floor {
        on_floor_clear_of_balls(layout)
    } else {
        Vec2::new(target.origin.x, face_bottom - 8.0 * r)
    };
    assert_eq!(state.admit_spider(start), Ok(()), "{context}");
    // The room's solids reach the queries at the next step, so the first
    // tick sees none of them: a shot waits for it.
    phase("settles");
    drive(&mut state, idle());
    let ids_before: Vec<Vec<SurfaceId>> = (0..4u8)
        .map(|slot| {
            state
                .digit_surfaces()
                .iter()
                .filter(|s| s.slot == slot)
                .map(|s| s.id)
                .collect()
        })
        .collect();

    if from_floor {
        // Along the floor to beneath the cell.
        phase("walks under the cell");
        let landed = drive_until(
            &mut state,
            60,
            grip(),
            |f| f.support == Some(Hold::Floor),
            &format!("{context}: catches the floor"),
        );
        assert_eq!(landed.spider.mode, SpiderMode::Crawling);
        let direction = (target.origin.x - start.x).signum();
        drive_until(
            &mut state,
            900,
            crawl(direction),
            |f| (f.spider.position.x - target.origin.x).abs() < 0.2 * r,
            &format!("{context}: walks under the cell"),
        );
        assert_eq!(state.spider_state().unwrap().support, Some(Hold::Floor));
    }

    // Straight up at the cell, which the line ties to.
    phase("shoots the cell");
    drive(&mut state, shoot_up());
    let tied = drive_until(
        &mut state,
        60,
        grip(),
        |f| f.spider.line.is_some(),
        &format!("{context}: the line ties"),
    );
    assert_eq!(
        tied.anchor,
        Some(Hold::Surface {
            id: target.id,
            source: SurfaceSource::Anchored,
            departing: false,
        }),
        "{context}"
    );
    assert_eq!(tied.refusals, Refusals::default(), "{context}");

    // Up the line until the legs catch the cell's underside.
    phase("climbs the line");
    let caught = climb_the_line(&mut state, &context);
    assert_eq!(caught.spider.mode, SpiderMode::Crawling);
    assert_held_on_the_face(&state, &caught, &format!("{context}: caught"));
    assert!(
        caught.spider.up.y < -0.9,
        "{context}: hangs beneath it: {:?}",
        caught.spider.up
    );

    // Some line let out, then along the underside, holding a drawn cell the
    // whole way and crossing to the next.
    phase("pays out line");
    for _ in 0..24 {
        let fixture = drive(&mut state, reel(-1.0));
        assert_held_on_the_face(&state, &fixture, &format!("{context}: paying out"));
    }
    let mut held: BTreeSet<SurfaceId> = BTreeSet::new();
    phase("crawls the underside");
    for step in 0..45 {
        let fixture = drive(&mut state, crawl(1.0));
        assert_held_on_the_face(
            &state,
            &fixture,
            &format!("{context}: crawling, step {step}"),
        );
        held.insert(held_surface(&fixture).unwrap());
        assert_eq!(fixture.refusals, Refusals::default(), "{context}");
    }
    assert!(held.len() >= 2, "{context}: crossed a seam: {held:?}");
    let moved = state.spider_state().unwrap().spider.position.x - target.origin.x;
    assert!(
        moved.abs() > 1.5 * r,
        "{context}: moved {} radii",
        moved / r
    );

    // Letting go of the cell, it swings on the line, still tied to the cell
    // it came up to.
    phase("swings on the line");
    let mut swung = false;
    let mut lowest = f32::MAX;
    for _ in 0..90 {
        let fixture = drive(&mut state, idle());
        lowest = lowest.min(fixture.spider.position.y);
        if fixture.spider.mode == SpiderMode::Hanging
            && fixture.spider.velocity.x.abs() > 2.0 * r
            && fixture.spider.line.unwrap().taut
        {
            swung = true;
        }
        assert_eq!(
            fixture.anchor.map(|anchor| match anchor {
                Hold::Surface { id, .. } => id,
                other => panic!("{context}: tied to {other:?}"),
            }),
            Some(target.id),
            "{context}"
        );
    }
    assert!(swung, "{context}: swings");
    assert!(
        lowest < face_bottom - 2.0 * r,
        "{context}: hangs below the face"
    );

    // Back up to the cell, and the line is cut.
    phase("climbs back");
    let caught = climb_the_line(&mut state, &format!("{context}: again"));
    assert_held_on_the_face(&state, &caught, &format!("{context}: caught again"));
    let fixture = drive(
        &mut state,
        SpiderIntent {
            cut: true,
            ..grip()
        },
    );
    assert_eq!(fixture.spider.line, None);
    assert_eq!(
        fixture.spider.last_release.map(|release| release.reason),
        Some(SpiderReleaseReason::Requested)
    );
    phase("rests on the cell");
    for _ in 0..30 {
        let fixture = drive(&mut state, grip());
        assert_held_on_the_face(&state, &fixture, &format!("{context}: resting on the cell"));
    }
    let riding = held_surface(&state.spider_state().unwrap()).unwrap();

    // The minute changes: the old glyph slides out with the spider on it,
    // until the cell it holds is retired as it leaves the slot, or carries
    // the spider off its drawn part on the way, and the spider falls as from
    // any support that has gone. The new glyph is new surfaces, and the
    // spider never held one.
    assert!(state.set_spider_intent(grip()));
    tick(&mut state, &[ClockAction::set_reading(reading(12, 39, 59))]);
    trace(&state);
    assert_held_on_the_face(
        &state,
        &state.spider_state().unwrap(),
        &format!("{context}: seconds"),
    );
    phase("rides the old glyph out");
    assert!(state.set_spider_intent(grip()));
    tick(&mut state, &[ClockAction::set_reading(reading(12, 40, 0))]);
    trace(&state);
    assert_eq!(
        state.event_kind(),
        Some(ClockEventKind::DigitSlide),
        "{context}"
    );
    let incoming: Vec<SurfaceId> = state
        .digit_surfaces()
        .iter()
        .filter(|s| s.slot == 2 && !s.departing)
        .map(|s| s.id)
        .collect();
    assert!(
        !incoming.is_empty() && !incoming.contains(&riding),
        "{context}"
    );
    let mut rode = 0;
    let mut released = None;
    for step in 0..DIGIT_SLIDE_TICKS + 1 {
        let fixture = drive(&mut state, grip());
        let surfaces = state.digit_surfaces();
        match fixture.support {
            Some(Hold::Surface { id, departing, .. }) if released.is_none() => {
                assert_eq!(id, riding, "{context}: rides the cell it held");
                assert!(departing, "{context}");
                assert_held_on_the_face(
                    &state,
                    &fixture,
                    &format!("{context}: riding, step {step}"),
                );
                rode += 1;
            }
            Some(Hold::Surface { id, .. }) => {
                assert!(!incoming.contains(&id), "{context}: never the new glyph");
            }
            _ => {
                if released.is_none() {
                    let reason = fixture.spider.last_release.map(|release| release.reason);
                    let retired = surfaces.iter().all(|s| s.id != riding);
                    match reason {
                        Some(SpiderReleaseReason::Removed) => {
                            assert!(retired, "{context}: the cell went with its surface");
                        }
                        Some(SpiderReleaseReason::OutOfReach) => {
                            assert!(!retired, "{context}: carried off a cell still there");
                            assert!(
                                fixture.spider.position.y < face_bottom,
                                "{context}: carried out of the band"
                            );
                        }
                        other => panic!("{context}: let go as {other:?}"),
                    }
                    released = Some(step);
                }
            }
        }
    }
    let released = released.unwrap_or_else(|| panic!("{context}: the cell leaves"));
    assert!(rode >= 3, "{context}: rode {rode} ticks");
    assert!(released >= rode, "{context}");
    assert!(
        state.spider_state().unwrap().spider.position.y < face_bottom,
        "{context}"
    );
    // Unchanged slots keep their surfaces.
    let surfaces = state.digit_surfaces();
    for slot in [0u8, 1] {
        let ids: Vec<SurfaceId> = surfaces
            .iter()
            .filter(|s| s.slot == slot)
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, ids_before[usize::from(slot)], "{context}: slot {slot}");
    }
    // It falls to the floor and holds that.
    phase("falls to the floor");
    let landed = drive_until(
        &mut state,
        600,
        grip(),
        on_floor_still,
        &format!("{context}: lands"),
    );
    assert!(landed.spider.position.y < face_bottom, "{context}");

    // The residents were there all along, in the one world, and stay when
    // the spider goes.
    assert_eq!(state.arena_accounting().worlds, worlds.max(1), "{context}");
    assert_eq!(
        state.arena_fixtures(),
        balls,
        "{context}: the balls never moved"
    );
    assert!(state.remove_spider());
    assert_eq!(state.spider_state(), None);
    assert_eq!(state.spider_accounting().removed, 1);
    assert_eq!(state.arena_fixtures(), balls, "{context}");
    let claims = state.arena_state().claims;
    assert!(
        claims
            .iter()
            .all(|claim| claim.owner != ClockArenaOwner::Spider),
        "{context}"
    );
    if residents == FixtureStart::None {
        assert!(
            state.arena.idle(),
            "{context}: the last owner takes the world"
        );
    } else {
        let world = state.arena.world().expect("the fixture keeps the world");
        assert_eq!(world.joint_count(), 0);
        assert!(
            world.entity_ids().all(|entity| {
                let id = entity.value();
                !canonical::SPIDER.contains(&id)
                    && !canonical::SPIDER_FRAME.contains(&id)
                    && !canonical::SPIDER_SURFACES.contains(&id)
            }),
            "{context}: nothing of the spider stays"
        );
    }
    tick(&mut state, &[]);
    assert_eq!(state.arena_fixtures(), balls, "{context}");
}

#[test]
fn the_spider_climbs_a_digit_swings_from_it_and_rides_the_old_glyph_out_in_every_font_and_layout() {
    for (index, font) in FONTS.into_iter().enumerate() {
        for aspect in ASPECTS {
            let fixture = if index % 2 == 0 {
                FixtureStart::None
            } else {
                FixtureStart::Default
            };
            climb_swing_and_ride_out(aspect, font, fixture);
        }
    }
}

#[test]
fn the_spider_rides_with_resident_balls_in_place_through_the_whole_route() {
    climb_swing_and_ride_out(ASPECT, ClockFont::Classic, FixtureStart::Default);
    climb_swing_and_ride_out(4.0, ClockFont::Sans, FixtureStart::TwoOwners);
}

#[test]
fn proxies_follow_a_sliding_glyph_and_are_clipped_to_the_band_it_is_drawn_in() {
    for font in FONTS {
        for aspect in ASPECTS {
            let context = format!("{font:?} at {aspect}");
            let mut state = clock(aspect, font);
            let layout = Layout::new(aspect);
            assert_eq!(state.admit_spider(on_floor(layout, 0.0)), Ok(()));
            sync(
                &mut state,
                &[ClockAction::preview_event(ClockEventKind::DigitSlide)],
            );
            let mut clipped = 0;
            let mut ticks = 0;
            while state.event_kind() == Some(ClockEventKind::DigitSlide) {
                drive(&mut state, grip());
                ticks += 1;
                assert!(ticks <= DIGIT_SLIDE_TICKS, "{context}");
                let context = format!("{context}, tick {ticks}");
                assert_proxies_match_the_face(&state, &context);
                clipped += state
                    .digit_surfaces()
                    .iter()
                    .filter(|s| s.visible().is_some_and(|v| v != s.footprint))
                    .count();
            }
            assert!(clipped > 0, "{context}: cells were clipped on the way");
            assert_proxies_match_the_face(&state, &format!("{context}, settled"));
        }
    }
}

#[test]
fn proxies_follow_every_event_and_rigid_pieces_keep_their_own_bodies() {
    let kinds = [
        ClockEventKind::Falling,
        ClockEventKind::ColorCycle,
        ClockEventKind::Meltdown,
        ClockEventKind::Marquee,
        ClockEventKind::Rain,
        ClockEventKind::Explosion,
        ClockEventKind::Aquarium,
        ClockEventKind::CellSwap,
        ClockEventKind::SlotMachine,
        ClockEventKind::Duck,
        ClockEventKind::Crow,
    ];
    for (index, kind) in kinds.into_iter().enumerate() {
        let aspect = ASPECTS[index % ASPECTS.len()];
        let font = FONTS[index % FONTS.len()];
        let context = format!("{} in {font:?} at {aspect}", kind.as_str());
        let mut state = clock(aspect, font);
        let layout = Layout::new(aspect);
        // Beside the face, off the floor the events change.
        let start = Vec2::new(
            layout.bounds_min.x + layout.frame_width + 2.0 * SpiderFixture::radius(layout),
            0.0,
        );
        assert_eq!(state.admit_spider(start), Ok(()), "{context}");
        sync(&mut state, &[ClockAction::preview_event(kind)]);
        let running = |state: &ClockState| match kind {
            ClockEventKind::Crow => state.crow_state().is_some(),
            ClockEventKind::Duck => state.duck_state().is_some(),
            _ => state.event_kind() == Some(kind),
        };
        assert!(running(&state), "{context}: starts beside the spider");
        let mut ticks = 0;
        let mut rigid_seen = 0;
        while running(&state) && ticks < 4000 {
            let intent = if ticks % 120 < 60 { crawl(1.0) } else { grip() };
            let fixture = drive(&mut state, intent);
            ticks += 1;
            let context = format!("{context}, tick {ticks}");
            if ticks % 5 == 0 {
                assert_proxies_match_the_face(&state, &context);
            }
            let surfaces = state.digit_surfaces();
            let world = state.arena.world().expect("the spider's world");
            let mut bodies: Vec<_> = surfaces
                .iter()
                .filter(|s| s.eligible())
                .filter_map(|s| s.body)
                .filter(|body| world.contains_body(*body))
                .collect();
            bodies.sort_unstable();
            bodies.dedup();
            assert_eq!(fixture.rigid, bodies.len(), "{context}");
            rigid_seen += surfaces.iter().filter(|s| s.body.is_some()).count();
            if let Some(Hold::Rigid(body)) = fixture.support {
                assert!(
                    bodies.contains(&body),
                    "{context}: holds a body that carries a surface"
                );
            }
        }
        assert!(!running(&state), "{context}: the event ends");
        if matches!(kind, ClockEventKind::Falling | ClockEventKind::Explosion) {
            // Standalone, their pieces move in a world of their own, so the
            // proxies stand for them in the arena.
            assert!(rigid_seen > 0, "{context}: rigid pieces carried the face");
            assert_eq!(fixture_rigid_peak(&state), 0, "{context}");
        }
        assert_proxies_match_the_face(&state, &format!("{context}, after"));
        assert!(state.remove_spider());
    }
}

/// The spider comes in as a visit will: admitted under the beam above the
/// hatch, it ties a line to the beam and lets it out until it hangs below
/// `height`, in the middle of the room.
fn descend_from_the_beam(state: &mut ClockState, height: f32) -> SpiderFixtureState {
    let layout = Layout::new(state.aspect_ratio());
    let r = SpiderFixture::radius(layout);
    assert_eq!(
        state.admit_spider(SpiderFixture::under_beam(layout)),
        Ok(())
    );
    phase("descends through the hatch");
    drive(state, idle());
    drive(
        state,
        SpiderIntent {
            shoot: Some(Vec2::Y),
            ..idle()
        },
    );
    drive_until(
        state,
        30,
        idle(),
        |f| f.anchor == Some(Hold::Frame(FramePart::Beam)),
        "ties to the beam",
    );
    drive_until(
        state,
        900,
        reel(-1.0),
        |f| f.spider.position.y < height,
        "descends through the hatch",
    );
    let hanging = drive_until(
        state,
        240,
        grip(),
        |f| f.spider.velocity.length() < 0.2 * r,
        "hangs still",
    );
    assert_eq!(hanging.spider.mode, SpiderMode::Hanging);
    assert!(hanging.support.is_none());
    assert_eq!(hanging.anchor, Some(Hold::Frame(FramePart::Beam)));
    hanging
}

#[test]
fn a_departing_cell_passing_within_reach_is_never_caught() {
    let mut state = clock(ASPECT, ClockFont::Classic);
    let layout = Layout::new(ASPECT);
    let r = SpiderFixture::radius(layout);
    // Hanging from the ceiling just beside the last digit's right column,
    // within reach of the cells that slide down it, with nothing above.
    let surfaces = state.digit_surfaces();
    let column = surfaces
        .iter()
        .filter(|s| s.slot == 3)
        .max_by(|a, b| a.origin.x.total_cmp(&b.origin.x))
        .expect("a lit slot");
    let x = column.origin.x + 0.4 * layout.pitch + 1.1 * r;
    assert_eq!(state.admit_spider(Vec2::new(x, 0.0)), Ok(()));
    drive(&mut state, idle());
    drive(
        &mut state,
        SpiderIntent {
            shoot: Some(Vec2::Y),
            ..idle()
        },
    );
    let tied = drive_until(
        &mut state,
        60,
        idle(),
        |f| f.spider.line.is_some(),
        "ties to the ceiling",
    );
    assert_eq!(tied.anchor, Some(Hold::Frame(FramePart::CeilingRight)));
    let hanging = drive_until(
        &mut state,
        600,
        idle(),
        |f| f.spider.velocity.length() < 0.05 * r,
        "hangs still",
    );
    assert_eq!(hanging.spider.mode, SpiderMode::Hanging);
    // Every glyph slides out, departing, and back in as new surfaces. The
    // spider reaches for whatever comes within reach: the departing cells
    // beside it are refused, and a new cell it may catch and ride.
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::DigitSlide)],
    );
    let mut refused = 0;
    let mut ticks = 0;
    while state.event_kind() == Some(ClockEventKind::DigitSlide) {
        let fixture = drive(&mut state, grip());
        ticks += 1;
        assert!(ticks <= DIGIT_SLIDE_TICKS);
        refused += fixture.refusals.departing;
        if let Some(Hold::Surface { departing, .. }) = fixture.support {
            assert!(!departing, "caught a departing cell at tick {ticks}");
        }
    }
    assert!(refused > 0, "the departing cells came within reach");
}

#[test]
fn a_glyph_replaced_under_the_spider_lets_it_fall_and_a_glyph_that_comes_between_cuts_the_line() {
    // Replaced: the font changes while the spider holds a cell.
    let mut state = clock(ASPECT, ClockFont::Classic);
    let layout = Layout::new(ASPECT);
    let r = SpiderFixture::radius(layout);
    let target = lowest_cell(&state, 2);
    assert_eq!(
        state.admit_spider(on_floor(layout, target.origin.x)),
        Ok(())
    );
    drive_until(&mut state, 60, grip(), on_floor_still, "lands");
    drive(&mut state, shoot_up());
    let caught = climb_the_line(&mut state, "replaced");
    let held = held_surface(&caught).unwrap();
    drive(
        &mut state,
        SpiderIntent {
            cut: true,
            ..grip()
        },
    );
    let mut settings = state.settings();
    settings.fonts.selected = ClockFont::Sans;
    // Applied while paused, the change is published at once: the spider
    // still holds the proxy of a cell that is no longer there.
    sync(&mut state, &[ClockAction::configure(settings)]);
    let fixture = state.spider_state().unwrap();
    assert_eq!(fixture.support, Some(Hold::Retired(held)));
    assert!(state.digit_surfaces().iter().all(|s| s.id != held));
    let fixture = drive(&mut state, grip());
    assert_eq!(fixture.spider.mode, SpiderMode::Free);
    assert_eq!(
        fixture.spider.last_release.map(|release| release.reason),
        Some(SpiderReleaseReason::Removed)
    );
    assert_proxies_match_the_face(&state, "after the font change");
    // It falls, to the floor or onto a cell of the new glyph within reach,
    // which is another surface.
    let steady = std::cell::Cell::new(0);
    let recovered = drive_until(
        &mut state,
        600,
        grip(),
        |f| {
            steady.set(if held_surface(f).is_some() {
                steady.get() + 1
            } else {
                0
            });
            on_floor_still(f) || steady.get() >= 30
        },
        "falls or recovers",
    );
    if let Some(id) = held_surface(&recovered) {
        assert_ne!(id, held, "a cell of the new glyph");
        assert_held_on_the_face(&state, &recovered, "recovered");
    }
    assert!(state.remove_spider());

    // Between: a line to the ceiling up a column the next glyph fills.
    let mut state = clock_with(
        ASPECT,
        ClockFont::Classic,
        FixtureStart::None,
        reading(12, 14, 0),
    );
    let surfaces = state.digit_surfaces();
    let lit: BTreeSet<i8> = surfaces
        .iter()
        .filter(|s| s.slot == 2)
        .map(|s| s.cell.x)
        .collect();
    let column = (0..6i8)
        .find(|x| !lit.contains(x))
        .expect("a column the '1' leaves dark");
    let x = layout
        .slot_cell_center(2, crate::GridCell { x: column, y: 0 })
        .x;
    // From the middle of the room, up the dark column: the floor is farther
    // from the ceiling than a shot reaches.
    assert_eq!(state.admit_spider(Vec2::new(x, 0.0)), Ok(()));
    drive(&mut state, idle());
    drive(
        &mut state,
        SpiderIntent {
            shoot: Some(Vec2::Y),
            ..idle()
        },
    );
    let tied = drive_until(
        &mut state,
        60,
        idle(),
        |f| f.spider.line.is_some(),
        "ties to the ceiling",
    );
    assert!(
        matches!(
            tied.anchor,
            Some(Hold::Frame(
                FramePart::CeilingLeft | FramePart::CeilingRight
            ))
        ),
        "{:?}",
        tied.anchor
    );
    // The reading snaps to a '4', whose cells fill the column at once.
    tick(&mut state, &[ClockAction::set_reading(reading(12, 44, 0))]);
    assert_eq!(state.event_kind(), None, "a skipped half hour never slides");
    assert!(
        state
            .digit_surfaces()
            .iter()
            .any(|s| s.slot == 2 && s.cell.x == column),
        "the '4' lights the column"
    );
    let cut = drive_until(
        &mut state,
        3,
        idle(),
        |f| f.spider.line.is_none(),
        "the line is let go",
    );
    assert_eq!(
        cut.spider.last_release.map(|release| release.reason),
        Some(SpiderReleaseReason::Obstructed)
    );
    assert_eq!(cut.spider.mode, SpiderMode::Free);
    let fallen = drive_until(
        &mut state,
        240,
        grip(),
        on_floor_still,
        "falls to the floor",
    );
    assert!(fallen.spider.position.y < layout.floor_y + 2.0 * r);
}

#[test]
fn the_colon_and_the_seconds_change_nothing_the_spider_stands_on() {
    let mut state = clock(ASPECT, ClockFont::Matrix);
    let layout = Layout::new(ASPECT);
    let target = lowest_cell(&state, 1);
    assert_eq!(
        state.admit_spider(on_floor(layout, target.origin.x)),
        Ok(())
    );
    drive_until(&mut state, 60, grip(), on_floor_still, "lands");
    drive(&mut state, shoot_up());
    let caught = climb_the_line(&mut state, "seconds");
    let held = held_surface(&caught).unwrap();
    let ids: Vec<SurfaceId> = state.digit_surfaces().iter().map(|s| s.id).collect();
    for second in 1..=10u8 {
        assert!(state.set_spider_intent(grip()));
        tick(
            &mut state,
            &[ClockAction::set_reading(reading(12, 39, 58 + second % 2))],
        );
        let fixture = state.spider_state().unwrap();
        assert_eq!(held_surface(&fixture), Some(held), "second {second}");
        assert_eq!(
            state
                .digit_surfaces()
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(fixture.proxies, ids.len());
    }
}

#[test]
fn a_resize_cancels_the_spider_and_keeps_the_residents_as_they_were() {
    let mut state = clock_with(
        ASPECT,
        ClockFont::Classic,
        FixtureStart::Default,
        reading(12, 39, 58),
    );
    let layout = Layout::new(ASPECT);
    for _ in 0..240 {
        tick(&mut state, &[]);
    }
    assert_eq!(state.admit_spider(on_floor_clear_of_balls(layout)), Ok(()));
    drive_until(&mut state, 60, grip(), on_floor_still, "lands");
    let balls = state.arena_fixtures();
    let worlds = physics_activity().worlds;
    state.set_aspect_ratio(4.0 / 3.0);
    assert_eq!(state.spider_state(), None);
    assert_eq!(state.spider_accounting().cancelled, 1);
    assert_eq!(physics_activity().worlds, worlds, "the same world");
    let world = state.arena.world().expect("the fixture keeps the world");
    assert_eq!(world.joint_count(), 0);
    assert!(
        world
            .entity_ids()
            .all(|entity| !canonical::SPIDER_FRAME.contains(&entity.value()))
    );
    let claims = state.arena_state().claims;
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].owner, ClockArenaOwner::Fixture { index: 0 });
    // The balls came in as a resize brings residents in, and no more.
    let after = state.arena_fixtures();
    assert_eq!(after[0].balls.len(), balls[0].balls.len());
    for (before, after) in balls[0].balls.iter().zip(&after[0].balls) {
        assert!((before.position.y - after.position.y).abs() < 1e-3);
    }
    // And the spider is welcome again in the new view.
    let layout = Layout::new(4.0 / 3.0);
    assert_eq!(state.admit_spider(on_floor_clear_of_balls(layout)), Ok(()));
    assert_proxies_match_the_face(&state, "after the resize");
}

#[test]
fn the_spider_is_refused_while_the_residents_do_not_fit_the_view_and_beside_a_full_arena() {
    // Residents that decline a narrower view keep the arena from taking the
    // spider, as from taking a visit, until they fit again.
    let mut state = clock_with(
        4.0,
        ClockFont::Classic,
        FixtureStart::TwoOwners,
        reading(12, 39, 58),
    );
    for _ in 0..240 {
        tick(&mut state, &[]);
    }
    state.set_aspect_ratio(0.6);
    assert!(
        state.arena_accounting().layout_incompatible,
        "the outer balls do not fit a portrait view"
    );
    let layout = Layout::new(0.6);
    assert_eq!(
        state.admit_spider(on_floor_clear_of_balls(layout)),
        Err(SpiderRefusal::Arena(Unavailable::LayoutIncompatible))
    );
    state.set_aspect_ratio(4.0);
    assert!(!state.arena_accounting().layout_incompatible);
    let layout = Layout::new(4.0);
    assert_eq!(state.admit_spider(on_floor_clear_of_balls(layout)), Ok(()));
    assert!(state.remove_spider());

    // Beside a visit, two fixtures leave the spider no room, as they leave
    // Meltdown none: it takes an event's room.
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Duck)],
    );
    for _ in 0..3 {
        tick(&mut state, &[]);
    }
    assert!(state.arena.holds(Owner::Visit));
    assert_eq!(
        state.admit_spider(on_floor_clear_of_balls(layout)),
        Err(SpiderRefusal::Arena(Unavailable::Capacity))
    );
    assert_eq!(state.spider_accounting().refused, 2);
    assert!(state.arena_accounting().refused >= 2);
}

#[test]
fn a_text_marquee_sweeps_the_face_past_the_rails_and_the_spider_stays_in_the_room() {
    for font in [ClockFont::Matrix, ClockFont::Classic] {
        let context = format!("{font:?}");
        let mut state = clock(ASPECT, font);
        let layout = Layout::new(ASPECT);
        let r = SpiderFixture::radius(layout);
        let (low, high) = SpiderFixture::room(layout);
        // In the air beside the left rail, where the face sweeps out past it.
        assert_eq!(state.admit_spider(Vec2::new(low.x + 2.0 * r, 0.0)), Ok(()));
        let mut settings = state.settings();
        settings.marquee_preset = ClockMarqueePreset::TextScroll;
        sync(&mut state, &[ClockAction::configure(settings)]);
        sync(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Marquee)],
        );
        assert_eq!(
            state.event_kind(),
            Some(ClockEventKind::Marquee),
            "{context}"
        );
        let mut ticks = 0;
        let mut swept = 0;
        let mut held = 0;
        while state.event_kind() == Some(ClockEventKind::Marquee) {
            let intent = if ticks % 120 < 60 { crawl(1.0) } else { grip() };
            let fixture = drive(&mut state, intent);
            ticks += 1;
            assert!(ticks <= 4000, "{context}");
            let context = format!("{context}, tick {ticks}");
            assert_proxies_match_the_face(&state, &context);
            let surfaces = state.digit_surfaces();
            // Cells carried beyond the rails or drawn without ink stand for
            // nothing, so no proxy lies outside the room.
            swept += surfaces
                .iter()
                .filter(|s| s.opacity < 0.02 || s.origin.x < low.x)
                .count();
            let world = state.arena.world().unwrap();
            for index in 0..MAX_PROXIES {
                if let Some((min, max)) = world.body_solid_bounds(proxy_body(index)) {
                    assert!(
                        min.x >= low.x - 1e-3 && max.x <= high.x + 1e-3,
                        "{context}: proxy {index} from {min:?} to {max:?} is in the room"
                    );
                }
            }
            // The spider holds only what is drawn, and stays in the view.
            if held_surface(&fixture).is_some() {
                assert_held_on_the_face(&state, &fixture, &context);
                held += 1;
            }
            assert!(
                fixture.spider.position.x > layout.bounds_min.x
                    && fixture.spider.position.x < layout.bounds_max.x,
                "{context}: the spider is at {:?}",
                fixture.spider.position
            );
        }
        assert!(swept > 0, "{context}: the face was swept out");
        assert!(held > 0, "{context}: the spider held a cell on the way");
        assert_proxies_match_the_face(&state, &format!("{context}, after"));
    }
}

#[test]
fn the_arenas_own_settings_return_when_a_visit_leaves_beside_the_spider() {
    // At this width the duck's gravity differs from Clock's, so both the
    // gravity and the solver's length scale show the visit.
    let aspect = 4.0;
    let mut state = clock_with(
        aspect,
        ClockFont::Classic,
        FixtureStart::Default,
        reading(12, 39, 58),
    );
    let layout = Layout::new(aspect);
    for _ in 0..240 {
        tick(&mut state, &[]);
    }
    assert_eq!(state.admit_spider(on_floor_clear_of_balls(layout)), Ok(()));
    drive_until(&mut state, 60, grip(), on_floor_still, "lands");
    let own = (-crate::arena::MECHANICS_GRAVITY, layout.pitch);
    let settings = |state: &ClockState| {
        let world = state.arena.world().expect("the fixture keeps the world");
        (world.gravity().y, world.length_unit())
    };
    assert_eq!(settings(&state), own);
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::Duck)],
    );
    let mut ticks = 0;
    let mut visited = false;
    while state.duck_state().is_some() {
        drive(&mut state, grip());
        ticks += 1;
        assert!(ticks < 6000, "the visit ends");
        if state.arena.holds(Owner::Visit) {
            assert_ne!(settings(&state), own, "the visit brings its own");
            visited = true;
        }
    }
    assert!(visited);
    for _ in 0..5 {
        drive(&mut state, grip());
    }
    assert!(!state.arena.holds(Owner::Visit));
    assert!(state.arena.holds(Owner::Spider));
    assert_eq!(settings(&state), own, "Clock's own settings are back");
}

#[test]
fn a_start_blocked_by_a_resident_claims_nothing() {
    let mut state = clock_with(
        ASPECT,
        ClockFont::Classic,
        FixtureStart::Default,
        reading(12, 39, 58),
    );
    for _ in 0..240 {
        tick(&mut state, &[]);
    }
    let before = state.arena_accounting();
    let ball = state.arena_fixtures()[0].balls[1].position;
    assert_eq!(state.admit_spider(ball), Err(SpiderRefusal::Blocked));
    let after = state.arena_accounting();
    assert_eq!(
        (after.granted, after.released, after.worlds),
        (before.granted, before.released, before.worlds)
    );
    assert_eq!(after.claims, 1);
    assert_eq!(state.spider_accounting().refused, 1);
}

#[test]
fn a_proxy_born_as_a_cell_comes_into_view_moves_with_it_from_its_first_step() {
    let mut state = clock(ASPECT, ClockFont::Sans);
    let layout = Layout::new(ASPECT);
    assert_eq!(state.admit_spider(on_floor(layout, 0.0)), Ok(()));
    drive(&mut state, grip());
    sync(
        &mut state,
        &[ClockAction::preview_event(ClockEventKind::DigitSlide)],
    );
    let dt = 1.0 / 60.0;
    let mut previous: Vec<DigitSurface> = state.digit_surfaces();
    let mut born_moving = 0;
    let mut proxied_before: BTreeSet<SurfaceId> = BTreeSet::new();
    while state.event_kind() == Some(ClockEventKind::DigitSlide) {
        drive(&mut state, grip());
        let surfaces = state.digit_surfaces();
        let world = state.arena.world().unwrap();
        let fixture = state.spider.as_ref().unwrap();
        let mut proxied_now = BTreeSet::new();
        for (index, slot) in fixture.slots.iter().enumerate() {
            let Some(proxy) = slot else {
                continue;
            };
            proxied_now.insert(proxy.id);
            let now = surfaces.iter().find(|s| s.id == proxy.id).unwrap();
            let Some(was) = previous.iter().find(|s| s.id == proxy.id) else {
                continue;
            };
            let moved = (now.origin - was.origin) / dt;
            let velocity = world.motion(proxy_body(index)).unwrap().linear_velocity;
            assert!(
                (velocity - moved).length() < 1e-2 * layout.pitch,
                "surface {} moved at {moved:?}, its proxy at {velocity:?}",
                proxy.id.get()
            );
            if !proxied_before.contains(&proxy.id) && moved.length() > 0.0 {
                born_moving += 1;
            }
        }
        proxied_before = proxied_now;
        previous = surfaces;
    }
    assert!(born_moving > 0, "cells came into the band on the move");
}

#[test]
fn a_paused_clock_freezes_the_spider_and_the_run_ends_once_with_it() {
    let mut state = clock(ASPECT, ClockFont::Serif);
    let layout = Layout::new(ASPECT);
    assert_eq!(state.admit_spider(on_floor(layout, 0.0)), Ok(()));
    let landed = drive_until(&mut state, 60, grip(), on_floor_still, "lands");
    let steps = state.arena_accounting().steps;
    assert!(state.set_spider_intent(crawl(1.0)));
    for _ in 0..10 {
        sync(&mut state, &[]);
        let fixture = state.spider_state().unwrap();
        assert_eq!(fixture.spider.position, landed.spider.position);
        assert_eq!(fixture.ticks, landed.ticks);
    }
    assert_eq!(
        state.arena_accounting().steps,
        steps,
        "a tick that does not move time steps nothing"
    );
    // The intent set while paused acts on the next moving tick.
    tick(&mut state, &[]);
    let fixture = state.spider_state().unwrap();
    assert!(fixture.spider.position.x > landed.spider.position.x);
    assert_eq!(fixture.spider.intent, crawl(1.0));
    let before = physics_activity();
    drop(state);
    let after = physics_activity();
    assert_eq!(
        after.dropped - before.dropped,
        1,
        "the arena's world goes with the run, once"
    );
    assert_eq!(after.worlds, before.worlds);
}
