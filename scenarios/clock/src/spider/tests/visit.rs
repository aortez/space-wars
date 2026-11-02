//! The spider's visit as an ordinary event: how it comes in, what it does on
//! the digits, how it leaves, and what waits for it.

use std::{collections::BTreeSet, time::Duration};

use super::*;
use crate::{
    events::COOLDOWN_TICKS,
    spider::visit::{EXIT_GRACE_TICKS, EXIT_RESERVE_TICKS, SPIDER_TICKS, compatible_event},
};
use engine_common::{ClockSpiderHold, ClockSpiderPhase, ClockSpiderState};

/// Clock's views: the canonical wide landscape, a squarer one, a very wide
/// one and a portrait one.
const VIEWS: [f32; 4] = [ASPECT, 4.0 / 3.0, 4.0, 0.6];
/// Ticks a visit may take at most, exit grace included, and a little over.
const LONGEST: u64 = SPIDER_TICKS + EXIT_GRACE_TICKS + 10;

/// A clock with the spider enabled and previewable, in `font`.
fn visited(aspect: f32, font: ClockFont, seed: u64) -> ClockState {
    let mut config = config(aspect, font, FixtureStart::None);
    config.events.spider = true;
    let mut state = ClockScenario::init(config, seed);
    sync(&mut state, &[ClockAction::set_reading(reading(12, 39, 58))]);
    state
}

/// The visit `SPIDER_PROBE="aspect,font,seed"` names, or the canonical view
/// in Classic, seed 7, with the spider previewed.
fn probed() -> ClockState {
    let probe = std::env::var("SPIDER_PROBE").unwrap_or_default();
    let mut parts = probe.split(',');
    let aspect: f32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(ASPECT);
    let font = match parts.next().unwrap_or("") {
        "sans" => ClockFont::Sans,
        "matrix" => ClockFont::Matrix,
        "serif" => ClockFont::Serif,
        _ => ClockFont::Classic,
    };
    let seed: u64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(7);
    let mut state = visited(aspect, font, seed);
    preview(&mut state, ClockEventKind::Spider);
    state
}

fn preview(state: &mut ClockState, kind: ClockEventKind) {
    sync(state, &[ClockAction::preview_event(kind)]);
}

fn spider(state: &ClockState) -> ClockSpiderState {
    state.spider_state().expect("a visiting spider")
}

/// What a visit did, read back tick by tick.
#[derive(Debug, Default)]
struct Visit {
    phases: BTreeSet<&'static str>,
    ticks: u64,
    /// The visit's age when it started leaving.
    left_at: Option<u64>,
    last: Option<ClockSpiderState>,
}

/// Runs the visit to its end, or for `limit` ticks, checking what every
/// tick publishes, and `each` tick on the way.
fn run(state: &mut ClockState, limit: u64, mut each: impl FnMut(&mut ClockState)) -> Visit {
    let mut visit = Visit::default();
    while visit.ticks < limit {
        let Some(spider) = state.spider_state() else {
            break;
        };
        readable(state, &spider);
        visit.phases.insert(spider.phase.as_str());
        if spider.phase == ClockSpiderPhase::Leaving && visit.left_at.is_none() {
            visit.left_at = Some(spider.age_ticks);
        }
        visit.last = Some(spider);
        each(state);
        tick(state, &[]);
        visit.ticks += 1;
    }
    visit
}

/// The published state agrees with the fixture it is read from.
fn readable(state: &ClockState, spider: &ClockSpiderState) {
    let fixture = state.spider_fixture_state().expect("the fixture");
    assert_eq!(spider.age_ticks, fixture.ticks);
    assert_eq!(
        spider.remaining_ticks,
        SPIDER_TICKS
            .saturating_sub(EXIT_RESERVE_TICKS)
            .saturating_sub(spider.age_ticks)
    );
    assert_eq!(spider.support.is_some(), fixture.support.is_some());
    assert_eq!(spider.anchor.is_some(), fixture.anchor.is_some());
    assert_eq!(
        spider.line_length_milli.is_some(),
        fixture.spider.line.is_some()
    );
    assert_eq!(spider.shooting, fixture.spider.shot.is_some());
    assert_eq!(spider.proxies, fixture.proxies);
    let position = fixture.spider.position;
    assert_eq!(
        spider.position_milli,
        [
            (position.x * 1000.0).round() as i32,
            (position.y * 1000.0).round() as i32
        ]
    );
    assert!(position.x.is_finite() && position.y.is_finite());
    if let Some(ClockSpiderHold::Surface { id, .. }) = spider.support {
        assert!(
            state.digit_surfaces().iter().any(|s| s.id.get() == id),
            "the held surface is published"
        );
    }
}

/// Nothing of the visit is left: no spider, no line, no proxies, no frame.
fn gone(state: &ClockState, bodies_before: usize) {
    assert!(state.spider_state().is_none());
    assert!(state.spider_fixture_state().is_none());
    assert!(state.spider_visit.is_none());
    assert_eq!(state.body_count(), bodies_before);
    assert!(
        !state
            .arena_state()
            .claims
            .iter()
            .any(|claim| claim.owner == ClockArenaOwner::Spider)
    );
}

#[test]
fn visits_come_down_the_line_crawl_swing_and_leave_through_the_hatch() {
    for aspect in VIEWS {
        let mut swung = 0;
        let mut visits = 0;
        for font in FONTS {
            for seed in 0..2u64 {
                let mut state = visited(aspect, font, seed);
                let mut reference = visited(aspect, font, seed);
                preview(&mut state, ClockEventKind::Spider);
                let visit = run(&mut state, LONGEST, |state| {
                    assert!(matches!(
                        state.event_kind(),
                        None | Some(ClockEventKind::DigitSlide)
                    ));
                    tick(&mut reference, &[]);
                });
                let last = visit.last.expect("a visit");
                let why = format!("aspect={aspect} {font:?} seed={seed}: {visit:?}");
                assert_eq!(state.spider_accounting().departed, 1, "{why}");
                assert_eq!(state.spider_accounting().failed_exits, 0, "{why}");
                assert!(!last.failed_exit, "{why}");
                assert!(visit.ticks <= SPIDER_TICKS + EXIT_GRACE_TICKS, "{why}");
                for phase in ["descending", "crossing", "climbing", "crawling", "leaving"] {
                    assert!(visit.phases.contains(phase), "{phase}: {why}");
                }
                assert!(last.climbs >= 1, "{why}");
                // It leaves while there is still time to, by its own reckoning.
                assert!(
                    visit.left_at.unwrap() <= SPIDER_TICKS - EXIT_RESERVE_TICKS,
                    "{why}"
                );
                gone(&state, 0);
                // The time went on as it would have without the spider.
                tick(&mut reference, &[]);
                assert_eq!(state.segments(), reference.segments(), "{why}");
                assert_eq!(state.reading(), reference.reading(), "{why}");
                swung += usize::from(last.swings > 0);
                visits += 1;
            }
        }
        assert!(
            swung * 2 >= visits,
            "aspect={aspect}: {swung} of {visits} visits swung"
        );
    }
}

#[test]
fn visits_replay_exactly_and_draw_within_bounds() {
    let mut a = visited(ASPECT, ClockFont::Classic, 7);
    let mut b = visited(ASPECT, ClockFont::Classic, 7);
    preview(&mut a, ClockEventKind::Spider);
    preview(&mut b, ClockEventKind::Spider);
    let mut ticks = 0;
    let visit = run(&mut a, LONGEST, |a| {
        assert_eq!(a.spider_state(), b.spider_state());
        assert_eq!(a.spider_fixture_state(), b.spider_fixture_state());
        if ticks % 60 == 0 {
            let frame = ClockScenario::render_frame(a);
            assert_eq!(frame, ClockScenario::render_frame(&b));
            assert!(
                frame
                    .layers
                    .iter()
                    .map(|l| l.primitives.len())
                    .sum::<usize>()
                    < 1500
            );
        }
        tick(&mut b, &[]);
        ticks += 1;
    });
    assert!(visit.phases.contains("swinging"), "{visit:?}");
    assert!(b.spider_state().is_none());
}

#[test]
fn automatic_visits_follow_the_schedule_and_the_reuse_delay() {
    let mut state = visited(ASPECT, ClockFont::Classic, 3);
    let mut settings = state.settings();
    settings.event_profile = ClockEventProfile::Demo;
    state.configure(settings);
    assert!(!state.automatic_events_suspended());
    for _ in 0..15 * 60 {
        if state.spider_state().is_some() {
            break;
        }
        tick(&mut state, &[]);
    }
    let id = state.event_id();
    assert_eq!(spider(&state).visit_id, id);
    assert!(state.event_blocked_by_spider(ClockEventKind::Spider));
    // Next Event cycles what stays live beside it, and another admission
    // waits for this visit: no second spider comes.
    state.next_event();
    assert_eq!(state.event_notice.unwrap().0, "Digit Slide");
    assert_eq!(state.event_kind(), Some(ClockEventKind::DigitSlide));
    preview(&mut state, ClockEventKind::Spider);
    assert_eq!(state.event_notice.unwrap().0, "Spider is already visiting");
    assert_eq!(spider(&state).visit_id, id);
    assert_eq!(state.spider_accounting().admitted, 1);
    let visit = run(&mut state, LONGEST, |_| {});
    assert!(visit.phases.contains("leaving"));
    gone(&state, 0);
    assert!(!state.automatic_events_suspended());
    let departed = state.simulation_tick();
    let ready_at = state.event_ready_at_tick(ClockEventKind::Spider);
    assert_eq!(
        ready_at,
        departed + 30 * 60,
        "one reuse delay, from departure"
    );
    // The slide Next Event started counted as an admission of its own.
    let id = state.event_id();
    while state.simulation_tick() + 1 < ready_at {
        tick(&mut state, &[]);
        assert_eq!(state.event_id(), id);
        assert!(state.spider_state().is_none());
    }
    for _ in 0..15 * 60 {
        tick(&mut state, &[]);
        if state.spider_state().is_some() {
            break;
        }
    }
    assert_eq!(state.event_id(), id + 1);
    assert_eq!(spider(&state).visit_id, id + 1);
    assert_eq!(state.spider_accounting().admitted, 2);
}

#[test]
fn manual_requests_add_one_spider_and_busy_ones_report_why() {
    let mut state = visited(ASPECT, ClockFont::Classic, 5);
    state.config.events = ClockEvents::default();
    preview(&mut state, ClockEventKind::Spider);
    let before = state.spider_state();
    preview(&mut state, ClockEventKind::Spider);
    assert_eq!(state.spider_state(), before, "no second spider");
    assert_eq!(state.spider_accounting().admitted, 1);
    // Next Event cycles the events that stay live beside it, and the rest
    // report the visit rather than evicting it.
    state.next_event();
    assert_eq!(state.event_kind(), Some(ClockEventKind::ColorCycle));
    assert_eq!(state.spider_state(), before);
    preview(&mut state, ClockEventKind::Falling);
    assert_eq!(state.event_notice.unwrap().0, "Spider is visiting");
    assert_eq!(state.event_kind(), Some(ClockEventKind::ColorCycle));
    assert_eq!(state.spider_state(), before);
    assert!(state.event_blocked_by_spider(ClockEventKind::Duck));
    assert!(state.event_blocked_by_spider(ClockEventKind::Spider));
    assert!(!state.event_blocked_by_spider(ClockEventKind::DigitSlide));

    // The spider waits for a duck, a crow, a player's duck and an event it
    // would not keep live, and takes nothing over.
    for (kind, action) in [
        (
            ClockEventKind::Duck,
            ClockAction::preview_event(ClockEventKind::Duck),
        ),
        (
            ClockEventKind::Crow,
            ClockAction::preview_event(ClockEventKind::Crow),
        ),
        (ClockEventKind::Duck, ClockAction::toggle_player_duck(1)),
        (
            ClockEventKind::Rain,
            ClockAction::preview_event(ClockEventKind::Rain),
        ),
        (
            ClockEventKind::Meltdown,
            ClockAction::preview_event(ClockEventKind::Meltdown),
        ),
    ] {
        let mut state = visited(ASPECT, ClockFont::Classic, 5);
        state.config.events = ClockEvents::default();
        sync(&mut state, &[action]);
        for _ in 0..30 {
            tick(&mut state, &[]);
        }
        let snapshot = (
            state.duck_state(),
            state.player_duck_state(),
            state.crow_state(),
            state.rain_state(),
            state.meltdown_state(),
            state.event_kind(),
            state.phase_tick(),
            state.body_count(),
        );
        assert!(
            state.event_blocked_by_spider(ClockEventKind::Spider),
            "{kind:?}"
        );
        preview(&mut state, ClockEventKind::Spider);
        assert_eq!(
            state.event_notice.unwrap().0,
            "The spider waits for the visitor or event to finish",
            "{kind:?}"
        );
        assert!(state.spider_state().is_none(), "{kind:?}");
        assert_eq!(state.spider_accounting().admitted, 0, "{kind:?}");
        assert_eq!(
            (
                state.duck_state(),
                state.player_duck_state(),
                state.crow_state(),
                state.rain_state(),
                state.meltdown_state(),
                state.event_kind(),
                state.phase_tick(),
                state.body_count(),
            ),
            snapshot,
            "{kind:?}: nothing evicted"
        );
    }
}

#[test]
fn exclusion_runs_both_ways_for_every_kind() {
    for kind in ClockEventKind::ALL {
        if matches!(
            kind,
            ClockEventKind::Duck | ClockEventKind::Crow | ClockEventKind::Spider
        ) {
            continue;
        }
        // The event first: the spider comes in beside it or waits for it.
        let mut state = visited(ASPECT, ClockFont::Classic, 1);
        state.config.events = ClockEvents::default();
        preview(&mut state, kind);
        assert_eq!(state.event_kind(), Some(kind));
        assert_eq!(
            state.event_blocked_by_spider(ClockEventKind::Spider),
            !compatible_event(kind),
            "{kind:?}"
        );
        preview(&mut state, ClockEventKind::Spider);
        assert_eq!(
            state.spider_state().is_some(),
            compatible_event(kind),
            "{kind:?}"
        );
        assert_eq!(state.event_kind(), Some(kind), "{kind:?}: left running");

        // The spider first: the event comes in beside it or waits for it.
        let mut state = visited(ASPECT, ClockFont::Classic, 1);
        state.config.events = ClockEvents::default();
        preview(&mut state, ClockEventKind::Spider);
        let before = state.spider_state();
        assert_eq!(
            state.event_blocked_by_spider(kind),
            !compatible_event(kind),
            "{kind:?}"
        );
        preview(&mut state, kind);
        assert_eq!(
            state.event_kind() == Some(kind),
            compatible_event(kind),
            "{kind:?}"
        );
        assert_eq!(state.spider_state(), before, "{kind:?}: the spider stays");
    }
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

/// A reading that changes the digit in `slot`, from 12:39.
fn reading_changing(slot: u8) -> ClockReading {
    match slot {
        0 => reading(2, 39, 0),
        1 => reading(13, 39, 0),
        2 => reading(12, 49, 0),
        _ => reading(12, 38, 0),
    }
}

#[test]
fn the_time_changes_under_a_crawl_and_a_swing_and_the_spider_carries_on() {
    for phase in [ClockSpiderPhase::Crawling, ClockSpiderPhase::Swinging] {
        let mut state = visited(ASPECT, ClockFont::Classic, 7);
        preview(&mut state, ClockEventKind::Spider);
        let mut changed = None;
        let visit = run(&mut state, LONGEST, |state| {
            let spider = spider(state);
            if changed.is_none()
                && spider.phase == phase
                && spider.phase_tick > 10
                && let Some(slot) = held_slot(state)
            {
                let at = reading_changing(slot);
                sync(state, &[ClockAction::set_reading(at)]);
                changed = Some((slot, at, spider));
            }
        });
        let (slot, at, before) = changed.expect("the phase was reached");
        assert!(
            visit.phases.contains("recovering") || visit.last.unwrap().climbs > before.climbs,
            "{phase:?} on slot {slot}: {visit:?}"
        );
        assert_eq!(
            state.spider_accounting().departed,
            1,
            "{phase:?}: {visit:?}"
        );
        gone(&state, 0);
        let mut reference = visited(ASPECT, ClockFont::Classic, 7);
        sync(&mut reference, &[ClockAction::set_reading(at)]);
        for _ in 0..visit.ticks {
            tick(&mut reference, &[]);
        }
        assert_eq!(
            state.reading().unwrap().minute(),
            reference.reading().unwrap().minute()
        );
    }
}

#[test]
fn a_way_out_that_keeps_going_from_under_it_ends_in_a_reported_failed_exit() {
    let mut state = visited(ASPECT, ClockFont::Classic, 7);
    preview(&mut state, ClockEventKind::Spider);
    let mut change = 0u8;
    let visit = run(&mut state, LONGEST, |state| {
        let spider = spider(state);
        if spider.phase == ClockSpiderPhase::Leaving && spider.age_ticks % 60 == 0 {
            // Both digits by the hatch change every second.
            change = (change + 1) % 6;
            let at = reading(10 + change, 10 * change + 9, 30);
            sync(state, &[ClockAction::set_reading(at)]);
        }
    });
    // Reported where it stands for one tick, past the budget and the grace.
    let last = visit.last.unwrap();
    assert!(last.failed_exit, "{visit:?}");
    assert_eq!(last.age_ticks, SPIDER_TICKS + EXIT_GRACE_TICKS + 1);
    assert_eq!(last.remaining_ticks, 0);
    assert_eq!(visit.ticks, SPIDER_TICKS + EXIT_GRACE_TICKS + 2);
    assert_eq!(state.spider_accounting().failed_exits, 1);
    assert_eq!(state.spider_accounting().departed, 0);
    gone(&state, 0);
    let retired = state.simulation_tick();
    assert_eq!(
        state.event_ready_at_tick(ClockEventKind::Spider),
        retired + 30 * 60
    );
    assert!(!state.automatic_events_suspended());
}

#[test]
fn pause_freezes_the_visit_and_resize_cancels_it() {
    let mut state = visited(ASPECT, ClockFont::Classic, 7);
    preview(&mut state, ClockEventKind::Spider);
    for _ in 0..300 {
        tick(&mut state, &[]);
    }
    let before = state.spider_state();
    let fixture = state.spider_fixture_state();
    let frame = ClockScenario::render_frame(&state);
    for _ in 0..100 {
        ClockScenario::step(&mut state, &[], Duration::ZERO);
    }
    assert_eq!(state.spider_state(), before);
    assert_eq!(state.spider_fixture_state(), fixture);
    assert_eq!(ClockScenario::render_frame(&state), frame);
    // Disabling future visits leaves this one to finish.
    let mut settings = state.settings();
    settings.events.spider = false;
    settings.event_profile = ClockEventProfile::Demo;
    sync(&mut state, &[ClockAction::configure(settings)]);
    assert_eq!(state.spider_state(), before);
    let id = state.event_id();
    let visit = run(&mut state, LONGEST, |_| {});
    assert!(visit.phases.contains("leaving"));
    gone(&state, 0);
    for _ in 0..60 * 60 {
        tick(&mut state, &[]);
        assert!(state.spider_state().is_none());
    }
    assert_eq!(state.event_id(), id, "no visit while disabled");
    settings.events.spider = true;
    settings.event_profile = ClockEventProfile::Calm;
    sync(&mut state, &[ClockAction::configure(settings)]);

    // A resize cancels a visit, once, and the next admission is a new one.
    preview(&mut state, ClockEventKind::Spider);
    for _ in 0..300 {
        tick(&mut state, &[]);
    }
    let visit_id = spider(&state).visit_id;
    state.set_aspect_ratio(0.6);
    gone(&state, 0);
    assert_eq!(state.spider_accounting().cancelled, 1);
    assert_eq!(state.spider_accounting().departed, 1);
    let cancelled = state.simulation_tick();
    assert_eq!(
        state.event_ready_at_tick(ClockEventKind::Spider),
        cancelled + 30 * 60
    );
    preview(&mut state, ClockEventKind::Spider);
    assert!(spider(&state).visit_id > visit_id);
    let visit = run(&mut state, LONGEST, |_| {});
    assert!(visit.phases.contains("leaving"));
    gone(&state, 0);
    assert_eq!(state.spider_accounting().departed, 2);
}

#[test]
fn visits_leave_the_residents_where_they_were() {
    let mut config = config(ASPECT, ClockFont::Classic, FixtureStart::Default);
    config.events.spider = true;
    let mut state = ClockScenario::init(config, 7);
    sync(&mut state, &[ClockAction::set_reading(reading(12, 39, 58))]);
    for _ in 0..120 {
        tick(&mut state, &[]);
    }
    // The three balls and the floor under them.
    let balls = state.body_count();
    assert_eq!(balls, 4);
    let claims = state.arena_state().claims.len();
    preview(&mut state, ClockEventKind::Spider);
    assert_eq!(state.arena_state().claims.len(), claims + 1);
    let visit = run(&mut state, LONGEST, |state| {
        assert!(state.body_count() > balls);
    });
    assert_eq!(state.spider_accounting().departed, 1, "{visit:?}");
    gone(&state, balls);
    assert_eq!(state.arena_state().claims.len(), claims);
    let _ = COOLDOWN_TICKS;
}

/// Prints every change of the visit's phase, hold, anchor or shot, and a
/// line every two seconds besides.
#[test]
#[ignore = "probe"]
fn probe_visit_log() {
    let mut state = probed();
    let mut last = String::new();
    let mut ticks = 0u64;
    while state.spider_state().is_some() && ticks < LONGEST {
        tick(&mut state, &[]);
        ticks += 1;
        let Some(s) = state.spider_state() else { break };
        let f = state.spider_fixture_state().unwrap();
        let key = format!(
            "{} {:?} up=({:.1},{:.1}) hold={:?} anchor={:?} shot={} fail={:?} rel={:?}",
            s.phase.as_str(),
            s.mode,
            f.spider.up.x,
            f.spider.up.y,
            s.support,
            s.anchor,
            s.shooting,
            f.spider.last_failure,
            f.spider.last_release.map(|r| r.reason)
        );
        if key != last || ticks % 120 == 0 {
            println!(
                "t{ticks:4} age={} pos=({:.0},{:.0}) v=({:.0},{:.0}) line={:?} {key} climbs={} swings={} falls={} rec={}",
                s.age_ticks,
                f.spider.position.x,
                f.spider.position.y,
                f.spider.velocity.x,
                f.spider.velocity.y,
                s.line_length_milli.map(|l| l / 1000),
                s.climbs,
                s.swings,
                s.falls,
                s.recoveries
            );
            last = key;
        }
    }
    println!(
        "ended at t{ticks}: state={:?} accounting={:?}",
        state.spider_state().map(|s| s.phase),
        state.spider_accounting()
    );
}

/// One line per visit over every view, font and six seeds.
#[test]
#[ignore = "probe"]
fn probe_visit_sweep() {
    for aspect in VIEWS {
        for font in FONTS {
            for seed in 0..6u64 {
                let mut state = visited(aspect, font, seed);
                preview(&mut state, ClockEventKind::Spider);
                let visit = run(&mut state, LONGEST, |_| {});
                let s = visit.last.unwrap();
                let a = state.spider_accounting();
                println!(
                    "aspect {aspect:.2} {font:?} seed {seed}: ended t{} departed={} failed={} leaving_at={:?} climbs={} swings={} falls={} rec={} last={} {:?}",
                    visit.ticks,
                    a.departed,
                    a.failed_exits,
                    visit.left_at,
                    s.climbs,
                    s.swings,
                    s.falls,
                    s.recoveries,
                    s.phase.as_str(),
                    s.support
                );
            }
        }
    }
}
