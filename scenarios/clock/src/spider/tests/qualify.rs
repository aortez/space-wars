//! The visit's proof for #384: what the renderer shows is what the physics
//! has and nothing of it remains after a departure, the spider stays on the
//! drawn face while the digits move under it, visits repeat within their
//! budgets beside residents they leave alone, a run's world and a refusal
//! are accounted for, and the profiles bring the spider in or keep it out.

use super::visit::{Visit, gone, preview, run, spider, visited};
use super::*;
use crate::spider::visit::{EXIT_GRACE_TICKS, SPIDER_TICKS};
use engine_common::{ClockSpiderHold, ClockSpiderPhase, RenderPrimitive};

/// Ticks a visit may take at most, exit grace included, and a little over.
const LONGEST: u64 = SPIDER_TICKS + EXIT_GRACE_TICKS + 10;
/// The renderer's layer for the spider, its line and the hatch.
const SPIDER_LAYER: i32 = 8;

/// The spider layer's line primitives as (start, end).
fn drawn_lines(state: &ClockState) -> Vec<(Vec2, Vec2)> {
    ClockScenario::render_frame(state)
        .layers
        .iter()
        .filter(|layer| layer.z == SPIDER_LAYER)
        .flat_map(|layer| layer.primitives.iter())
        .filter_map(|primitive| match primitive {
            RenderPrimitive::Line(line) => Some((
                Vec2::new(line.start.x, line.start.y),
                Vec2::new(line.end.x, line.end.y),
            )),
            _ => None,
        })
        .collect()
}

fn close(a: Vec2, b: Vec2) -> bool {
    a.distance_to(b) < 1e-3
}

#[test]
fn the_drawn_line_is_the_physics_line_and_nothing_remains_after_departure() {
    let mut state = visited(ASPECT, ClockFont::Classic, 7);
    preview(&mut state, ClockEventKind::Spider);
    let mut lines_seen = 0;
    let mut shots_seen = 0;
    let visit = run(&mut state, LONGEST, |state| {
        let fixture = state.spider_fixture_state().expect("the fixture");
        let drawn = drawn_lines(state);
        let mut expected = Vec::new();
        if let Some(line) = fixture.spider.line {
            expected.push((fixture.spider.position, line.anchor));
            lines_seen += 1;
        }
        if let Some(shot) = fixture.spider.shot {
            expected.push((fixture.spider.position, shot.tip));
            shots_seen += 1;
        }
        // One drawn line per physical one, from the body to where it goes:
        // no second tether, no line without an anchor, no anchor without a
        // line.
        assert_eq!(drawn.len(), expected.len(), "{:?}", state.spider_state());
        for (start, end) in &expected {
            assert!(
                drawn
                    .iter()
                    .any(|(s, e)| close(*s, *start) && close(*e, *end)),
                "drawn {drawn:?} has the physical line {start:?} -> {end:?}"
            );
        }
        let published = state.spider_state().unwrap();
        assert_eq!(published.anchor.is_some(), fixture.spider.line.is_some());
        assert_eq!(published.shooting, fixture.spider.shot.is_some());
    });
    assert!(lines_seen > 0 && shots_seen > 0, "{visit:?}");
    assert_eq!(state.spider_accounting().departed, 1, "{visit:?}");
    gone(&state, 0);
    // Nothing of the spider is drawn once it has gone.
    assert!(
        ClockScenario::render_frame(&state)
            .layers
            .iter()
            .all(|layer| layer.z != SPIDER_LAYER || layer.primitives.is_empty())
    );
    assert!(drawn_lines(&state).is_empty());
}

fn fixture_position(state: &ClockState) -> Vec2 {
    state
        .spider_fixture_state()
        .expect("the fixture")
        .spider
        .position
}

/// The slot of the cell the spider holds, where it holds one.
fn held_slot(state: &ClockState) -> Option<u8> {
    let ClockSpiderHold::Surface { id, .. } = spider(state).support? else {
        return None;
    };
    state
        .digit_surfaces()
        .iter()
        .find(|s| s.id.get() == id)
        .map(|s| s.slot)
}

#[test]
fn the_spider_stays_on_the_drawn_face_while_the_digits_slide_under_it() {
    let mut state = visited(ASPECT, ClockFont::Classic, 7);
    preview(&mut state, ClockEventKind::Spider);
    let mut changed_at = None;
    let mut checked = 0;
    let mut rode_a_departing_cell = false;
    let visit = run(&mut state, LONGEST, |state| {
        let published = spider(state);
        // A cell above the bottom row, so the old glyph carries the spider
        // down a while before the band's edge takes it off: a bottom cell
        // is clipped away under it at once.
        let layout = Layout::new(state.aspect_ratio());
        let [low, _] = layout.digit_band();
        let high_enough = fixture_position(state).y > low + 1.2 * layout.pitch;
        if changed_at.is_none()
            && published.phase == ClockSpiderPhase::Crawling
            && published.phase_tick > 10
            && high_enough
            && let Some(slot) = held_slot(state)
        {
            // The digit it holds changes by the next minute, as the host's
            // readings come: only a contiguous forward minute slides, so
            // the clock is first set to the minute before, which leaves the
            // held slot's digit as it is, and then to the one after.
            let (before, after) = match slot {
                0 => (reading(9, 59, 59), reading(10, 0, 0)),
                1 => (reading(12, 59, 59), reading(13, 0, 0)),
                _ => (reading(12, 39, 59), reading(12, 40, 0)),
            };
            sync(state, &[ClockAction::set_reading(before)]);
            tick(state, &[ClockAction::set_reading(after)]);
            assert_eq!(state.event_kind(), Some(ClockEventKind::DigitSlide));
            changed_at = Some(published.age_ticks);
        }
        // Wherever its legs hold, the point is on a cell the face draws
        // right now, departing or not: the visible and the physical support
        // match in motion. A proxy retired by the reading applied without
        // a tick goes at the next tick.
        let fixture = state.spider_fixture_state().unwrap();
        let Some(Hold::Surface { id, departing, .. }) = fixture.support else {
            return;
        };
        let id = id.get();
        let point = fixture.spider.support.expect("a hold").point;
        let surface = state
            .digit_surfaces()
            .into_iter()
            .find(|s| s.id.get() == id)
            .expect("the held surface is published");
        let local = surface.local(point);
        let visible = surface.visible().expect("a held surface is drawn");
        let slack = 0.05;
        assert!(
            (visible.min.x - slack..=visible.max.x + slack).contains(&local.x)
                && (visible.min.y - slack..=visible.max.y + slack).contains(&local.y),
            "held at {local:?} of {visible:?} at age {}",
            published.age_ticks
        );
        let layout = Layout::new(state.aspect_ratio());
        let drawn = digit_cell_polygons(state);
        assert!(
            drawn
                .iter()
                .any(|(polygon, ink)| *ink >= 0.02 && inside(polygon, point, 0.02 * layout.pitch)),
            "the held point {point:?} is drawn at age {}",
            published.age_ticks
        );
        checked += 1;
        rode_a_departing_cell |= departing;
    });
    assert!(changed_at.is_some(), "{visit:?}");
    assert!(checked > 100, "{visit:?}");
    assert!(rode_a_departing_cell, "{visit:?}");
    assert_eq!(state.spider_accounting().departed, 1, "{visit:?}");
    gone(&state, 0);
}

#[test]
fn visits_repeat_within_their_budgets_and_leave_the_residents_alone() {
    let mut config = config(ASPECT, ClockFont::Classic, FixtureStart::Default);
    config.events.spider = true;
    config.event_profile = ClockEventProfile::Demo;
    let mut state = ClockScenario::init(config, 11);
    sync(&mut state, &[ClockAction::set_reading(reading(12, 39, 58))]);
    for _ in 0..180 {
        tick(&mut state, &[]);
    }
    let balls = state.body_count();
    assert_eq!(balls, 4, "three balls and the floor under them");
    let limits = state.arena_state().limits;
    let mut departed = 0;
    let mut retired_at = Vec::new();
    while departed < 3 {
        // The schedule brings the next visit in.
        for _ in 0..40 * 60 {
            if state.spider_state().is_some() {
                break;
            }
            tick(&mut state, &[]);
        }
        let id = spider(&state).visit_id;
        let steps_before = state.arena_accounting().steps;
        let mut ticks = 0;
        let visit: Visit = run(&mut state, LONGEST, |state| {
            ticks += 1;
            let arena = state.arena_state();
            let claim = arena
                .claims
                .iter()
                .find(|claim| claim.owner == ClockArenaOwner::Spider)
                .expect("the spider's claim");
            assert!(claim.population.bodies <= claim.budget.bodies);
            assert!(claim.population.colliders <= claim.budget.colliders);
            assert!(claim.population.joints <= claim.budget.joints);
            assert!(arena.accounting.bodies <= limits.capacity.bodies);
            assert!(arena.accounting.joints <= limits.capacity.joints);
            assert!(spider(state).proxies <= MAX_PROXIES);
            // One physical step a tick, the residents' and the spider's
            // alike; the check runs before the tick it counts.
            assert_eq!(state.arena_accounting().steps, steps_before + ticks - 1);
            assert_eq!(
                state
                    .arena_fixtures()
                    .iter()
                    .map(|owner| owner.balls.len())
                    .sum::<usize>(),
                3
            );
        });
        assert_eq!(visit.last.unwrap().visit_id, id);
        departed += 1;
        assert_eq!(state.spider_accounting().departed, departed, "{visit:?}");
        gone(&state, balls);
        let ready_at = state.event_ready_at_tick(ClockEventKind::Spider);
        assert_eq!(ready_at, state.simulation_tick() + 30 * 60);
        retired_at.push(ready_at);
    }
    assert_eq!(state.spider_accounting().admitted, 3);
    assert_eq!(state.spider_accounting().failed_exits, 0);
    // Each departure set the reuse delay once, from its own tick.
    assert!(
        retired_at
            .windows(2)
            .all(|pair| pair[1] > pair[0] + 30 * 60)
    );
    assert_eq!(
        state.arena_state().accounting.worlds,
        1,
        "one world all along"
    );
}

#[test]
fn a_visit_makes_the_run_its_world_and_a_refused_one_changes_nothing() {
    // No world before the first visit; one after it, kept for the run.
    let mut state = visited(ASPECT, ClockFont::Classic, 7);
    assert_eq!(state.arena_accounting().worlds, 0);
    assert_eq!(state.body_count(), 0);
    preview(&mut state, ClockEventKind::Spider);
    assert_eq!(state.arena_accounting().worlds, 1);
    let steps = state.arena_accounting().steps;
    let visit = run(&mut state, LONGEST, |_| {});
    assert_eq!(state.spider_accounting().departed, 1, "{visit:?}");
    assert_eq!(state.arena_accounting().steps, steps + visit.ticks);
    assert_eq!(state.arena_accounting().worlds, 1);
    gone(&state, 0);

    // Two fixtures and a visit leave the spider no room: the refusal is
    // reported before anything is claimed, and the visit goes on.
    let mut config = config(ASPECT, ClockFont::Classic, FixtureStart::TwoOwners);
    config.events = ClockEvents::default();
    let mut state = ClockScenario::init(config, 7);
    sync(&mut state, &[ClockAction::set_reading(reading(12, 39, 58))]);
    for _ in 0..180 {
        tick(&mut state, &[]);
    }
    preview(&mut state, ClockEventKind::Duck);
    tick(&mut state, &[]);
    assert!(state.duck_state().is_some());
    let before = (
        state.arena_state(),
        state.duck_state(),
        state.body_count(),
        state.event_id(),
    );
    assert_eq!(
        state.event_blocked_by_arena(ClockEventKind::Spider),
        Some(crate::arena::Unavailable::Capacity)
    );
    preview(&mut state, ClockEventKind::Spider);
    assert!(state.spider_state().is_none());
    assert_eq!(state.spider_accounting().admitted, 0);
    assert_eq!(
        (
            state.arena_state(),
            state.duck_state(),
            state.body_count(),
            state.event_id(),
        ),
        before,
        "nothing changed"
    );
    assert!(state.event_notice.is_some());
}

#[test]
fn off_keeps_the_spider_out_and_calm_brings_it_in() {
    let mut state = visited(ASPECT, ClockFont::Classic, 3);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Off;
    state.configure(settings);
    for _ in 0..120 * 60 {
        tick(&mut state, &[]);
        assert!(state.spider_state().is_none());
    }
    assert_eq!(state.event_id(), 0);
    assert_eq!(state.spider_accounting().admitted, 0);
    preview(&mut state, ClockEventKind::Spider);
    assert!(state.spider_state().is_some(), "manual admission while Off");
    let visit = run(&mut state, LONGEST, |_| {});
    assert_eq!(state.spider_accounting().departed, 1, "{visit:?}");
    gone(&state, 0);

    let mut state = visited(ASPECT, ClockFont::Classic, 3);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Calm;
    state.configure(settings);
    let mut arrived = None;
    for t in 0..120 * 60 {
        tick(&mut state, &[]);
        if state.spider_state().is_some() {
            arrived = Some(t);
            break;
        }
    }
    let arrived = arrived.expect("a Calm visit within two minutes");
    assert!(
        arrived >= 45 * 60,
        "after Calm's idle wait, not before: {arrived}"
    );
    assert_eq!(spider(&state).visit_id, state.event_id());
}

/// Prints the route envelope as the docs' table: every view and font over
/// six seeds, with how many departed, swung and fell, and the visits'
/// lengths.
#[test]
#[ignore = "probe"]
fn probe_route_envelope() {
    println!(
        "| View | Font | Departed | Swung | Fell | Visit (s) median / longest | Left at (s) |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    for (name, aspect) in [
        ("800×480", ASPECT),
        ("1024×768", 4.0 / 3.0),
        ("4:1", 4.0),
        ("480×800", 0.6),
    ] {
        for font in FONTS {
            let mut ends = Vec::new();
            let mut left = Vec::new();
            let (mut departed, mut swung, mut fell) = (0, 0, 0);
            for seed in 0..6u64 {
                let mut state = visited(aspect, font, seed);
                preview(&mut state, ClockEventKind::Spider);
                let visit = run(&mut state, LONGEST, |_| {});
                let last = visit.last.unwrap();
                departed += state.spider_accounting().departed;
                swung += u32::from(last.swings > 0);
                fell += u32::from(last.falls > 0);
                ends.push(visit.ticks);
                left.push(visit.left_at.unwrap_or(0));
            }
            ends.sort_unstable();
            left.sort_unstable();
            println!(
                "| {name} | {font:?} | {departed}/6 | {swung}/6 | {fell}/6 | {:.1} / {:.1} | {:.1} |",
                ends[3] as f32 / 60.0,
                ends[5] as f32 / 60.0,
                left[3] as f32 / 60.0
            );
        }
    }
}
