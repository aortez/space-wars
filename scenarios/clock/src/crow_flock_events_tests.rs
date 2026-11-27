//! A flock beside the Clock's other events, on every face it may show and
//! under its controls (#372).
use super::*;
use crate::crow::{CROW_TICKS, MAX_CROWS, Phase};
use engine_core::Vec2;

/// By this age every crow of a visit has started for home.
const DEADLINE: u64 = 17 * 60;
/// A crow's silhouette, in pitches, as the flock's own tests take it.
const WIDTH: f32 = 1.47;
const HEIGHT: f32 = 1.155;

fn tick(state: &mut ClockState) {
    ClockScenario::step(state, &[], Duration::from_millis(16));
}

/// A Clock of `config`, with its own events off, showing `reading`.
fn clock(config: ClockConfig, seed: u64, reading: ClockReading) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            ..config
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading)],
        Duration::ZERO,
    );
    state
}

fn landscape() -> ClockConfig {
    ClockConfig {
        aspect_ratio: 16.0 / 9.0,
        ..Default::default()
    }
}

fn at(hour: u8, minute: u8) -> ClockReading {
    ClockReading::new(hour, minute, 0).unwrap()
}

fn step(state: &mut ClockState, action: Action) {
    ClockScenario::step(state, &[action], Duration::ZERO);
}

/// The events that may run beside a visit of crows: all but another visit,
/// the spider, which waits for crows as they wait for it, and Tanks, which
/// neither comes while crows visit nor lets them come.
fn joining() -> impl Iterator<Item = ClockEventKind> {
    ClockEventKind::ALL.into_iter().filter(|kind| {
        !matches!(
            kind,
            ClockEventKind::Crow | ClockEventKind::Spider | ClockEventKind::Tanks
        )
    })
}

/// The events that leave the crows no perch.
fn takes_the_perches(kind: ClockEventKind) -> bool {
    matches!(
        kind,
        ClockEventKind::Falling
            | ClockEventKind::Meltdown
            | ClockEventKind::DigitSlide
            | ClockEventKind::CellSwap
            | ClockEventKind::Marquee
            | ClockEventKind::Explosion
            | ClockEventKind::Coins
    )
}

fn overlap(a: Vec2, b: Vec2, layout: Layout) -> bool {
    (a.x - b.x).abs() < layout.pitch * WIDTH && (a.y - b.y).abs() < layout.pitch * HEIGHT
}

fn settled(phase: Phase) -> bool {
    matches!(phase, Phase::Perched | Phase::Pecking)
}

/// Whether feet at `feet` stand on the top of the cell `target` names, among
/// the lit cells of the face as it is now.
fn on_a_lit_cell(state: &ClockState, target: Option<[u8; 3]>, feet: Vec2) -> bool {
    let Some([slot, x, y]) = target else {
        return false;
    };
    let layout = Layout::new(state.aspect_ratio());
    state
        .segments()
        .iter()
        .filter(|segment| {
            segment.lit
                && segment.representation == SegmentRepresentation::Anchored
                && segment.id.digit_slot == slot
        })
        .flat_map(|segment| segment.cells().map(move |cell| (segment.id, cell)))
        .filter(|(_, cell)| cell.x as u8 == x && cell.y as u8 == y)
        .any(|(id, cell)| {
            let top = layout.cell_center(id, cell) + Vec2::new(0.0, layout.pitch * 0.4);
            (feet.x - top.x).abs() < layout.pitch * 0.5
                && (feet.y - top.y).abs() < layout.pitch * 0.01
        })
}

/// What holds of a flock at every step, whatever else the Clock is doing:
/// every crow starts for home by the deadline and is gone within the cap, a
/// perched crow stands on a lit cell of the face as it is now, and no two
/// crows are bound for each other's room or settled in it.
fn check_flock(state: &ClockState, context: &str) {
    let Some(visit) = state.crow_visit.as_ref() else {
        return;
    };
    let layout = Layout::new(state.aspect_ratio());
    let crows: Vec<_> = visit
        .crows()
        .map(|crow| (crow.diagnostics(), crow.position, crow.destination()))
        .collect();
    for (crow, at, _) in &crows {
        assert!(crow.age_ticks < CROW_TICKS, "{context}: {crow:?}");
        assert!(
            crow.age_ticks < DEADLINE || crow.phase == Phase::Leaving,
            "{context}: {crow:?}"
        );
        if crow.phase == Phase::Perched {
            assert!(
                on_a_lit_cell(state, crow.target, *at),
                "{context}: {crow:?}"
            );
        }
    }
    for (index, (a, at, to)) in crows.iter().enumerate() {
        for (b, other_at, other_to) in &crows[index + 1..] {
            if let (Some(to), Some(other_to)) = (to, other_to) {
                assert!(!overlap(*to, *other_to, layout), "{context}: {a:?} {b:?}");
            }
            if settled(a.phase) && settled(b.phase) {
                assert!(!overlap(*at, *other_at, layout), "{context}: {a:?} {b:?}");
            }
        }
    }
}

/// Whether the water the crows read, Rain's, Aquarium's or Meltdown's, is
/// the same in both runs.
fn same_water(state: &ClockState, control: &ClockState) -> bool {
    fn water(state: &ClockState) -> Option<&engine_water::WaterWorld> {
        match &state.active_event {
            Some(ActiveEvent::Rain(event)) => Some(&event.water),
            Some(ActiveEvent::Aquarium(event)) => Some(&event.water),
            Some(ActiveEvent::Meltdown(event)) => Some(&event.water),
            _ => None,
        }
    }
    match (water(state), water(control)) {
        (Some(a), Some(b)) => a.stats() == b.stats() && a.parcels() == b.parcels(),
        (a, b) => a.is_none() && b.is_none(),
    }
}

/// Whether crow `member`'s body is in the arena.
fn has_body(state: &ClockState, member: usize) -> bool {
    state.arena.world().is_some_and(|world| {
        world
            .body_ids()
            .any(|id| id == crow::body::crow(member).id())
    })
}

/// Each event that may run beside crows, whether it comes first or the flock
/// does, runs as it does beside a flock that left at once: the same phases on
/// the same steps, the same water, and the same duck, until two seconds after
/// the last crow has gone. The flock keeps its room and stands only on lit
/// cells, an event that takes the perches sends every crow home at once, and
/// every crow is gone within the cap. Nothing of the crows is left, and the
/// visit's cooldown runs once, from when its last crow left.
#[test]
fn a_flock_and_each_event_beside_it_keep_their_own_lifecycles() {
    let (mut sent_home, mut admitted) = (0, 0);
    for aspect in [16.0 / 9.0, 0.6] {
        for kind in joining() {
            for event_first in [true, false] {
                let context = format!("aspect={aspect} {kind:?} event_first={event_first}");
                // Both runs admit the same crows, so their schedules number the
                // events alike and draw the same seeds. The control's leave at
                // once.
                let start = || {
                    let mut state = clock(
                        ClockConfig {
                            aspect_ratio: aspect,
                            ..Default::default()
                        },
                        3,
                        at(12, 34),
                    );
                    if event_first {
                        step(&mut state, ClockAction::preview_event(kind));
                        for _ in 0..30 {
                            tick(&mut state);
                        }
                    }
                    let came = state.preview_crows(MAX_CROWS);
                    (state, came)
                };
                let ((mut state, came), (mut control, _)) = (start(), start());
                assert!(came, "{context}: the arena refused the crows");
                admitted += 1;
                control.finish_crow_visit();
                let lifecycle = |state: &ClockState| {
                    (
                        state.event_kind(),
                        state.event_id(),
                        state.lifecycle(),
                        state.event_phase(),
                        state.phase_tick(),
                        state.duck_visit.is_some(),
                    )
                };
                let running = |state: &ClockState| {
                    state.event_kind() == Some(kind)
                        || kind == ClockEventKind::Duck && state.duck_visit.is_some()
                };
                let (mut age, mut gone, mut left_at, mut beside) = (0, 0, None, false);
                while gone < 120 {
                    beside |= state.crow_visit.is_some() && running(&state);
                    let context = format!("{context} age={age}");
                    if !event_first && age == 240 {
                        step(&mut state, ClockAction::preview_event(kind));
                        step(&mut control, ClockAction::preview_event(kind));
                        if takes_the_perches(kind) {
                            for crow in state.crow_states() {
                                assert_eq!(crow.phase, Phase::Leaving, "{context}");
                                sent_home += 1;
                            }
                        }
                    }
                    assert_eq!(lifecycle(&state), lifecycle(&control), "{context}");
                    assert!(same_water(&state, &control), "{context}");
                    check_flock(&state, &context);
                    if state.crow_visit.is_none() {
                        left_at.get_or_insert(state.simulation_tick());
                        gone += 1;
                    }
                    tick(&mut state);
                    tick(&mut control);
                    age += 1;
                }
                assert!(beside, "{context}: the event never ran beside the crows");
                assert!(!state.arena.holds(arena::Owner::Crow), "{context}");
                assert!(
                    (0..MAX_CROWS).all(|member| !has_body(&state, member)),
                    "{context}"
                );
                assert_eq!(
                    state.arena_accounting().released,
                    control.arena_accounting().released,
                    "{context}"
                );
                assert_eq!(
                    state.event_ready_at_tick(ClockEventKind::Crow),
                    left_at.unwrap() + EVENT_CATALOG[ClockEventKind::Crow as usize].cooldown_ticks,
                    "{context}"
                );
            }
        }
    }
    eprintln!("{admitted} flocks beside events; {sent_home} crows sent home by an event");
    assert!(sent_home > 0);
}

/// On every font, at narrow, portrait, landscape and wide layouts, at
/// readings that leave few perches, with the date and the meridiem shown or
/// not, the time goes on second by second and turns a minute in the middle
/// of the visit. Every perched crow stands on a lit cell of the face as it
/// is at that moment, none is bound for or settled in another's room, and
/// every crow is gone within the cap.
#[test]
fn a_flock_stands_only_on_the_face_it_sees_as_the_time_goes_on() {
    let date = ClockDate::new(2026, 11, 27).unwrap();
    let (mut visits, mut retargeted) = (0, 0);
    for font in ClockFont::ALL {
        for aspect in [0.25, 0.6, 16.0 / 9.0, 4.0] {
            for (time_format, show_date) in [
                (ClockTimeFormat::TwentyFourHour, false),
                (ClockTimeFormat::TwelveHour, true),
            ] {
                for (hour, minute) in [(11, 11), (21, 11)] {
                    let context = format!(
                        "{font:?} aspect={aspect} {time_format:?} date={show_date} \
                         {hour}:{minute}"
                    );
                    // Fifty seconds past the minute, so that it turns ten
                    // seconds into the visit, as the crows hop.
                    let reading = |seconds: u64| {
                        let past = 50 + seconds;
                        ClockReading::new(hour, minute + (past / 60) as u8, (past % 60) as u8)
                            .unwrap()
                            .with_date(date)
                    };
                    let mut state = clock(
                        ClockConfig {
                            aspect_ratio: aspect,
                            fonts: ClockFontSettings {
                                selected: font,
                                ..Default::default()
                            },
                            time_format,
                            show_date,
                            ..Default::default()
                        },
                        5,
                        reading(0),
                    );
                    assert!(state.preview_crows(MAX_CROWS), "{context}");
                    visits += 1;
                    let mut age = 0;
                    while state.crow_visit.is_some() {
                        let context = format!("{context} age={age}");
                        if age > 0 && age % 60 == 0 {
                            let before = state.crow_states();
                            step(&mut state, ClockAction::set_reading(reading(age / 60)));
                            retargeted += before
                                .iter()
                                .zip(state.crow_states())
                                .filter(|(before, after)| before.target != after.target)
                                .count();
                        }
                        check_flock(&state, &context);
                        tick(&mut state);
                        age += 1;
                    }
                    assert!(age <= CROW_TICKS, "{context}");
                }
            }
        }
    }
    eprintln!("{visits} flocks; {retargeted} crows retargeted as the time went on");
    assert!(retargeted > 0);
}

/// Beside a player's duck the flock keeps off the ground, which the duck has
/// taken, and neither ends the other's visit: the duck is still the
/// player's when the last crow has gone.
#[test]
fn a_flock_beside_a_players_duck_keeps_off_the_ground() {
    for seed in 0..4 {
        let context = format!("seed={seed}");
        let mut state = clock(landscape(), seed, at(12, 34));
        step(&mut state, ClockAction::toggle_player_duck(1));
        for _ in 0..30 {
            tick(&mut state);
        }
        assert!(state.player_duck_state().is_some(), "{context}");
        assert!(state.preview_crows(MAX_CROWS), "{context}");
        let mut age = 0;
        while state.crow_visit.is_some() {
            let context = format!("{context} age={age}");
            for crow in state.crow_states() {
                assert_eq!(crow.ground_target_milli, None, "{context}");
                assert_ne!(crow.phase, Phase::Pecking, "{context}");
            }
            check_flock(&state, &context);
            tick(&mut state);
            age += 1;
        }
        assert!(state.player_duck_state().is_some(), "{context}");
    }
}

/// While a flock visits, another Crow, asked for by preview, trigger or Next,
/// is refused and changes nothing of it, and Next with another event on
/// starts that one beside the flock. A change to the crows' settings is for
/// the next visit: each crow keeps its temperament, and switching Crow off
/// sends none home. The next visit takes the new temperament.
#[test]
fn a_flock_carries_on_through_busy_requests_event_cycling_and_setting_changes() {
    let mut state = clock(
        ClockConfig {
            crow_water_tolerance: ClockCrowWaterTolerance::Varied,
            ..landscape()
        },
        11,
        at(12, 34),
    );
    assert!(state.preview_crows(MAX_CROWS));
    for _ in 0..240 {
        tick(&mut state);
    }
    let only = |crow, rain| ClockEvents {
        crow,
        rain,
        explosion: false,
        aquarium: false,
        cell_swap: false,
        grass: false,
        falling: false,
        color_cycle: false,
        meltdown: false,
        duck: false,
        marquee: false,
        digit_slide: false,
        slot_machine: false,
        spider: false,
        coins: false,
        tanks: false,
    };
    let mut settings = state.settings();
    settings.events = only(true, false);
    step(&mut state, ClockAction::configure(settings));
    let flock = state.crow_states();
    let (id, ready_at) = (
        state.event_id(),
        state.event_ready_at_tick(ClockEventKind::Crow),
    );
    for (request, notice) in [
        (
            ClockAction::preview_event(ClockEventKind::Crow),
            Some("Crow is already visiting"),
        ),
        (ClockAction::trigger_event(ClockEventKind::Crow), None),
        (ClockAction::next_event(), Some("Crow is already visiting")),
    ] {
        state.event_notice = None;
        step(&mut state, request);
        assert_eq!(state.crow_states(), flock);
        assert_eq!(state.event_id(), id);
        assert_eq!(state.event_ready_at_tick(ClockEventKind::Crow), ready_at);
        assert_eq!(state.event_notice.map(|(notice, _)| notice), notice);
    }

    settings.events = only(true, true);
    step(&mut state, ClockAction::configure(settings));
    step(&mut state, ClockAction::next_event());
    assert_eq!(state.event_kind(), Some(ClockEventKind::Rain));
    assert_eq!(state.crow_states(), flock);

    let temperaments = |state: &ClockState| {
        state
            .crow_states()
            .iter()
            .map(|crow| (crow.member, crow.water_tolerance))
            .collect::<Vec<_>>()
    };
    let drawn = temperaments(&state);
    settings.crow_water_tolerance = ClockCrowWaterTolerance::Hardy;
    settings.events = only(false, true);
    step(&mut state, ClockAction::configure(settings));
    assert_eq!(state.settings(), settings);
    assert_eq!(state.crow_states(), flock);
    while state.crow_visit.is_some() {
        let now = temperaments(&state);
        assert!(now.iter().all(|crow| drawn.contains(crow)), "{now:?}");
        check_flock(&state, "after the changes");
        tick(&mut state);
    }
    assert!(state.preview_crows(MAX_CROWS));
    assert!(
        state
            .crow_states()
            .iter()
            .all(|crow| crow.water_tolerance == ClockCrowWaterTolerance::Hardy)
    );
}
