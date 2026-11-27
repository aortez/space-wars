//! Flocks: visits of two to four crows that come in together and then go each
//! its own way, out of each other's room (#371).
use super::*;
use crate::crow::{CROW_TICKS, MAX_CROWS, Phase};
use engine_core::Vec2;

/// By this age every crow of a visit has started for home.
const DEADLINE: u64 = 17 * 60;
/// The longest a crow's flight to its first perch may take, in ticks.
const FLIGHT: u64 = 8 * 60;
/// A crow's silhouette, in pitches: 14 of its pixels across and 11 up, each
/// 0.105 of a pitch.
const WIDTH: f32 = 1.47;
const HEIGHT: f32 = 1.155;
/// The longest two crows' silhouettes may overlap while one is in the air,
/// in ticks: time for one to cross the other, or to pass one coming down.
const CROSSING: u32 = 90;

fn tick(state: &mut ClockState) {
    ClockScenario::step(state, &[], Duration::from_millis(16));
}

fn clock(
    aspect: f32,
    seed: u64,
    font: ClockFont,
    tolerance: ClockCrowWaterTolerance,
    (hour, minute): (u8, u8),
) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            event_profile: ClockEventProfile::Off,
            crow_water_tolerance: tolerance,
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
            ClockReading::new(hour, minute, 0).unwrap(),
        )],
        Duration::ZERO,
    );
    state
}

/// A Clock at `aspect` at 12:34, with a visit of `size` crows.
fn flock(aspect: f32, seed: u64, size: usize) -> ClockState {
    let mut state = clock(
        aspect,
        seed,
        ClockFont::Classic,
        ClockCrowWaterTolerance::Shy,
        (12, 34),
    );
    assert!(state.preview_crows(size));
    state
}

/// Whether the silhouettes of crows with their feet at `a` and `b` overlap.
fn overlap(a: Vec2, b: Vec2, layout: Layout) -> bool {
    (a.x - b.x).abs() < layout.pitch * WIDTH && (a.y - b.y).abs() < layout.pitch * HEIGHT
}

fn settled(phase: Phase) -> bool {
    matches!(phase, Phase::Perched | Phase::Pecking)
}

/// One admission in four brings a flock, of two, three or four crows each as
/// likely, by a draw of the visit's seed alone. The Clock's own admissions
/// bring what their seeds draw: the first visits of these runs bring one crow
/// and flocks of each size.
#[test]
fn one_admission_in_four_brings_a_flock_of_two_three_or_four() {
    let mut sizes = [0; MAX_CROWS + 1];
    for seed in 0..20_000 {
        let size = crow::visit_size(seed);
        assert_eq!(crow::visit_size(seed), size);
        sizes[size] += 1;
    }
    eprintln!("visits of one to four crows in 20000: {:?}", &sizes[1..]);
    assert!((14_600..=15_400).contains(&sizes[1]), "{sizes:?}");
    for size in 2..=MAX_CROWS {
        assert!((1_500..=1_830).contains(&sizes[size]), "{sizes:?}");
    }
    for (seed, size) in [(0, 1), (20, 2), (5, 3), (6, 4)] {
        let mut state = clock(
            16.0 / 9.0,
            seed,
            ClockFont::Classic,
            ClockCrowWaterTolerance::Shy,
            (12, 34),
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::preview_event(ClockEventKind::Crow)],
            Duration::ZERO,
        );
        assert_eq!(state.crow_states().len(), size, "seed={seed}");
    }
}

/// A flock comes in from the side its first crow drew, in a line led by the
/// crow going furthest, each crow a gap behind the one before it and, where
/// the canopy leaves room, in a lane of its own. No crow overtakes another on
/// the way in, and none has longer for its place in the line: each comes down
/// within a flight's time of the visit's start.
#[test]
fn a_flock_comes_in_from_one_side_in_a_loose_line_without_overtaking() {
    for aspect in [0.6, 16.0 / 9.0, 2.4] {
        let layout = Layout::new(aspect);
        for seed in 0..6 {
            for size in 2..=MAX_CROWS {
                let context = format!("aspect={aspect} seed={seed} size={size}");
                let mut state = flock(aspect, seed, size);
                let visit = state.crow_visit.as_ref().unwrap();
                let inward = if visit.first().position.x < 0.0 {
                    1.0
                } else {
                    -1.0
                };
                // The line from its head, and how far in each crow is going.
                let mut line: Vec<_> = visit
                    .crows()
                    .map(|crow| {
                        let reach = crow.destination().map_or(f32::INFINITY, |to| to.x * inward);
                        (crow.position, reach)
                    })
                    .collect();
                line.sort_by(|a, b| (b.0.x * inward).total_cmp(&(a.0.x * inward)));
                let edge = if inward > 0.0 {
                    layout.bounds_min.x
                } else {
                    layout.bounds_max.x
                };
                assert!(
                    line.iter().all(|(at, _)| (at.x - edge) * inward < 0.0),
                    "{context}: {line:?}"
                );
                for pair in line.windows(2) {
                    let ((head, reach), (next, next_reach)) = (pair[0], pair[1]);
                    assert!((head.x - next.x).abs() >= layout.pitch * 2.0, "{context}");
                    assert!(reach >= next_reach, "{context}: {line:?}");
                }
                for (index, (at, _)) in line.iter().enumerate() {
                    for (other, _) in &line[index + 1..] {
                        let apart = (at.y - other.y).abs();
                        if aspect < 2.0 {
                            assert!(apart > layout.pitch * 0.2, "{context}: {line:?}");
                        } else {
                            assert!(apart < 1e-3, "{context}: {line:?}");
                        }
                    }
                }
                let entering = |state: &ClockState| {
                    let mut entering: Vec<_> = state
                        .crow_visit
                        .iter()
                        .flat_map(crow::CrowVisit::crows)
                        .filter(|crow| crow.phase == Phase::Entering)
                        .map(|crow| (crow.position.x * inward, crow.diagnostics().member))
                        .collect();
                    entering.sort_by(|a, b| a.0.total_cmp(&b.0));
                    entering
                        .into_iter()
                        .map(|(_, member)| member)
                        .collect::<Vec<_>>()
                };
                let mut order = entering(&state);
                let mut landed = [false; MAX_CROWS];
                for _ in 0..FLIGHT {
                    tick(&mut state);
                    for crow in state.crow_states() {
                        landed[usize::from(crow.member)] |= crow.phase == Phase::Perched;
                    }
                    let now = entering(&state);
                    order.retain(|member| now.contains(member));
                    assert_eq!(order, now, "{context}");
                }
                assert!(landed[..size].iter().all(|landed| *landed), "{context}");
            }
        }
    }
}

/// Over layouts from tall to wide, fonts and readings, some with narrow
/// digits whose perches stand side by side, no crow of a flock sets out for a
/// perch where it would stand in another's silhouette, nor for a spot on the
/// ground when another is bound for the ground too. No two crows that have
/// come down stand so, and none overlaps another in the air for longer than
/// it takes to cross. A crow that has pecked leaves while others stay, and
/// the rest have its room. All start for home by the deadline, and are gone
/// by the visit's cap.
#[test]
fn no_two_crows_of_a_flock_come_down_in_each_others_room_or_fly_together() {
    let (mut grounded_together, mut pecked_alone, mut longest) = (0, 0, 0);
    for aspect in [0.25, 0.6, 16.0 / 9.0, 2.4] {
        let layout = Layout::new(aspect);
        for font in [ClockFont::Classic, ClockFont::Matrix] {
            for reading in [(12, 34), (11, 11)] {
                for seed in 0..4 {
                    for size in 2..=MAX_CROWS {
                        let context =
                            format!("aspect={aspect} {font:?} {reading:?} seed={seed} size={size}");
                        let mut state =
                            clock(aspect, seed, font, ClockCrowWaterTolerance::Shy, reading);
                        assert!(state.preview_crows(size));
                        let mut together = [[0; MAX_CROWS]; MAX_CROWS];
                        let mut phases = [None; MAX_CROWS];
                        let mut age = 0;
                        while let Some(visit) = state.crow_visit.as_ref() {
                            assert!(age < CROW_TICKS, "{context}");
                            let crows: Vec<_> = visit
                                .crows()
                                .map(|crow| (crow.diagnostics(), crow.position, crow.destination()))
                                .collect();
                            let staying = crows
                                .iter()
                                .filter(|(crow, ..)| crow.phase != Phase::Leaving)
                                .count();
                            for (crow, ..) in &crows {
                                assert!(
                                    age < DEADLINE || crow.phase == Phase::Leaving,
                                    "{context}"
                                );
                                let member = usize::from(crow.member);
                                if crow.phase == Phase::Leaving
                                    && phases[member] == Some(Phase::Pecking)
                                    && staying > 0
                                {
                                    pecked_alone += 1;
                                }
                                phases[member] = Some(crow.phase);
                            }
                            for (index, (a, at, to)) in crows.iter().enumerate() {
                                for (b, other_at, other_to) in &crows[index + 1..] {
                                    let pair = format!("{context} age={age}: {a:?} {b:?}");
                                    if let (Some(to), Some(other_to)) = (to, other_to) {
                                        assert!(!overlap(*to, *other_to, layout), "{pair}");
                                    }
                                    grounded_together += usize::from(
                                        a.ground_target_milli.is_some()
                                            && b.ground_target_milli.is_some(),
                                    );
                                    let run =
                                        &mut together[usize::from(a.member)][usize::from(b.member)];
                                    if settled(a.phase) && settled(b.phase) {
                                        assert!(!overlap(*at, *other_at, layout), "{pair}");
                                    } else if overlap(*at, *other_at, layout) {
                                        *run += 1;
                                        longest = longest.max(*run);
                                        assert!(*run <= CROSSING, "{pair}");
                                    } else {
                                        *run = 0;
                                    }
                                }
                            }
                            tick(&mut state);
                            age += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!(
        "longest overlap in the air {longest} ticks; {grounded_together} ticks with two crows \
         bound for the ground; {pecked_alone} crows left after pecking while others stayed"
    );
    assert!(grounded_together > 0 && pecked_alone > 0);
}

/// A new reading or font moves the perches under a flock. Synchronized
/// without time, as while paused, each crow whose perch went sets out for
/// another out of the others' room, or for home, and none moves, ages, pecks
/// or dries. An event that takes every perch sends every crow home.
#[test]
fn new_perches_retarget_a_flock_out_of_each_others_room_without_time() {
    let aspect = 16.0 / 9.0;
    let layout = Layout::new(aspect);
    let mut retargeted = 0;
    for seed in 0..4 {
        let mut state = flock(aspect, seed, MAX_CROWS);
        for _ in 0..300 {
            tick(&mut state);
        }
        let mut settings = state.settings();
        settings.fonts.selected = ClockFont::Matrix;
        let changes = [
            ClockAction::set_reading(ClockReading::new(18, 7, 0).unwrap()),
            ClockAction::configure(settings),
            ClockAction::preview_event(ClockEventKind::Falling),
        ];
        for change in changes {
            let before = state.crow_states();
            ClockScenario::step(&mut state, &[change], Duration::ZERO);
            let after = state.crow_states();
            assert_eq!(after.len(), before.len());
            for (before, after) in before.iter().zip(&after) {
                let held = |crow: &engine_common::ClockCrowState| {
                    (
                        crow.member,
                        crow.position_milli,
                        crow.age_ticks,
                        crow.pecks,
                        crow.wetness_milli,
                    )
                };
                assert_eq!(held(after), held(before), "seed={seed}");
                retargeted += usize::from(after.target != before.target);
            }
            let destinations: Vec<_> = state
                .crow_visit
                .iter()
                .flat_map(crow::CrowVisit::crows)
                .filter_map(crow::Crow::destination)
                .collect();
            for (index, to) in destinations.iter().enumerate() {
                for other in &destinations[index + 1..] {
                    assert!(!overlap(*to, *other, layout), "seed={seed}: {after:?}");
                }
            }
        }
        assert!(
            state
                .crow_states()
                .iter()
                .all(|crow| crow.phase == Phase::Leaving)
        );
        for _ in 0..CROW_TICKS {
            tick(&mut state);
        }
        assert!(state.crow_states().is_empty());
    }
    assert!(retargeted > 0);
}

/// Shy and Hardy hold for every crow of a flock. Varied draws each crow's
/// own, from a stream of its own: a crow's temperament is the same however
/// many crows its visit brings, and some flocks mix the two.
#[test]
fn each_crow_of_a_flock_takes_the_clocks_temperament_or_draws_its_own() {
    let temperaments = |seed: u64, size: usize, tolerance| {
        let mut state = clock(16.0 / 9.0, seed, ClockFont::Classic, tolerance, (12, 34));
        assert!(state.preview_crows(size));
        state
            .crow_states()
            .iter()
            .map(|crow| crow.water_tolerance)
            .collect::<Vec<_>>()
    };
    let mut mixed = 0;
    for seed in 0..40 {
        for kind in [ClockCrowWaterTolerance::Shy, ClockCrowWaterTolerance::Hardy] {
            assert_eq!(temperaments(seed, MAX_CROWS, kind), [kind; MAX_CROWS]);
        }
        let flock = temperaments(seed, MAX_CROWS, ClockCrowWaterTolerance::Varied);
        for size in 1..MAX_CROWS {
            assert_eq!(
                temperaments(seed, size, ClockCrowWaterTolerance::Varied),
                flock[..size]
            );
        }
        mixed += usize::from(
            flock.contains(&ClockCrowWaterTolerance::Shy)
                && flock.contains(&ClockCrowWaterTolerance::Hardy),
        );
    }
    eprintln!("{mixed} of 40 Varied flocks of four mix Shy and Hardy crows");
    assert!(mixed > 0);
}

/// In the same heavy rain, each crow of a flock reads the water for itself,
/// by its own temperament, and none changes it: the water is as it is beside
/// crows that never move. None comes down with wet feet, a crow that gets wet
/// leaves while others stay, and every crow is gone within the visit.
#[test]
fn each_crow_of_a_flock_reads_the_rain_for_itself_and_leaves_the_water_as_it_was() {
    let aspect = 5.0 / 3.0;
    let layout = Layout::new(aspect);
    let mut wet_alone = [0; ClockCrowWaterTolerance::ALL.len()];
    for (kind, tolerance) in ClockCrowWaterTolerance::ALL.into_iter().enumerate() {
        for seed in 0..3 {
            let start = || {
                let mut state = clock(aspect, seed, ClockFont::Classic, tolerance, (12, 34));
                state.config.rain_amount = ClockRainAmount::Heavy;
                assert!(state.preview_crows(MAX_CROWS));
                state
            };
            let (mut state, mut control) = (start(), start());
            control.crow_visit = None;
            let mut phases = [None; MAX_CROWS];
            for t in 0..CROW_TICKS {
                if t == 180 {
                    for state in [&mut state, &mut control] {
                        state.preview_event(ClockEventKind::Rain);
                    }
                }
                tick(&mut state);
                tick(&mut control);
                let (Some(ActiveEvent::Rain(rain)), Some(ActiveEvent::Rain(other))) =
                    (&state.active_event, &control.active_event)
                else {
                    continue;
                };
                assert_eq!(rain.water.stats(), other.water.stats());
                assert_eq!(rain.water.parcels(), other.water.parcels());
                let environment = crow::Environment {
                    water: Some(&rain.water),
                    ..Default::default()
                };
                let crows: Vec<_> = state
                    .crow_visit
                    .iter()
                    .flat_map(crow::CrowVisit::crows)
                    .map(|crow| (crow.diagnostics(), crow.position))
                    .collect();
                let staying = crows
                    .iter()
                    .filter(|(crow, _)| crow.phase != Phase::Leaving)
                    .count();
                for (crow, at) in &crows {
                    let member = usize::from(crow.member);
                    if settled(crow.phase) {
                        let own = crow::WaterTolerance::new(crow.water_tolerance, 0);
                        assert!(
                            !environment.wet_feet(*at, layout, own),
                            "{tolerance:?} seed={seed} t={t}: {crow:?}"
                        );
                    }
                    if crow.phase == Phase::Leaving
                        && phases[member].is_some_and(|phase| phase != Phase::Leaving)
                        && crow.wet_departures > 0
                        && staying > 0
                    {
                        wet_alone[kind] += 1;
                    }
                    phases[member] = Some(crow.phase);
                }
            }
            assert!(state.crow_states().is_empty(), "{tolerance:?} seed={seed}");
        }
    }
    eprintln!("crows that left wet while others stayed, by temperament: {wet_alone:?}");
    assert!(wet_alone.iter().sum::<usize>() > 0);
}
