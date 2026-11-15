//! A storm's plan and its strikes: how many strikes each amount of rain
//! plans and when, that the room never flashes more than three times in a
//! second, how a strike's light rises and fades, where its bolt comes down,
//! and that lightning leaves the rain as it would be without it.

use std::collections::BTreeSet;

use super::*;
use crate::rain::RainEvent;
use crate::{ClockReading, DisplaySnapshot};
use engine_common::{ClockRainState, ClockTimeFormat};

fn display() -> DisplaySnapshot {
    crate::digits::snapshot(
        ClockReading::new(8, 8, 0).unwrap(),
        ClockTimeFormat::TwentyFourHour,
    )
}

/// A heavy storm struck over a bare floor, tick by tick: its flash on each
/// tick, and the strikes it struck.
fn over_a_floor(seed: u64) -> (Vec<f32>, Vec<Strike>) {
    let layout = Layout::new(800.0 / 480.0);
    let mut storm = Storm::new(seed, ClockRainAmount::Heavy);
    let mut strikes = Vec::new();
    let flashes = (0..RAINING_TICKS + 60)
        .map(|tick| {
            let floor = |_| (layout.floor_y, ClockStrikeTarget::Floor);
            if storm.step(tick, &layout, floor, None) {
                strikes.push(storm.latest().unwrap().clone());
            }
            storm.flash(tick)
        })
        .collect();
    (flashes, strikes)
}

/// The ticks the light in `levels` flashes on, counted as WCAG 2.3.1 counts
/// them: a rise of at least a tenth of full light, then a fall of as much.
fn flashes(levels: &[f32]) -> Vec<usize> {
    let mut flashes = Vec::new();
    let mut low = 0.0_f32;
    let mut high: Option<(usize, f32)> = None;
    for (tick, &level) in levels.iter().enumerate() {
        match high {
            None if level - low >= 0.1 => high = Some((tick, level)),
            None => low = low.min(level),
            Some((_, peak)) if level > peak => high = Some((tick, level)),
            Some((peak_tick, peak)) if peak - level >= 0.1 => {
                flashes.push(peak_tick);
                high = None;
                low = level;
            }
            Some(_) => {}
        }
    }
    flashes
}

#[test]
fn heavy_showers_strike_three_to_five_times_medium_ones_once_or_twice_and_light_ones_never() {
    let counts = |amount| {
        let mut counts = [0; 6];
        for seed in 0..300 {
            counts[Storm::new(seed, amount).planned() as usize] += 1;
        }
        counts
    };
    let heavy = counts(ClockRainAmount::Heavy);
    assert_eq!(heavy[..3], [0, 0, 0]);
    // The draws place every strike asked for: as many showers strike five
    // times as three.
    assert!(
        heavy[3..].iter().all(|&n| (70..=130).contains(&n)),
        "{heavy:?}"
    );
    let medium = counts(ClockRainAmount::Medium);
    assert_eq!((medium[0], medium[3..].iter().sum::<u32>()), (0, 0));
    assert!(medium[1] > 100 && medium[2] > 100, "{medium:?}");
    assert_eq!(counts(ClockRainAmount::Light)[0], 300);
    assert_eq!(Storm::calm().planned(), 0);
}

#[test]
fn strikes_fall_while_it_rains_most_where_it_rains_hardest_and_never_closer_than_the_gap() {
    let first = (FIRST * RAINING_TICKS as f64).round() as u64;
    let last = (LAST * RAINING_TICKS as f64).round() as u64;
    let (mut middle, mut outer) = (0, 0);
    for seed in 0..300 {
        let ticks = Storm::new(seed, ClockRainAmount::Heavy).ticks();
        assert!(
            ticks.windows(2).all(|pair| pair[1] - pair[0] >= STRIKE_GAP),
            "{seed}: {ticks:?}"
        );
        for tick in ticks {
            assert!((first..=last).contains(&tick), "{seed}: {tick}");
            if (RAINING_TICKS / 4..RAINING_TICKS * 3 / 4).contains(&tick) {
                middle += 1;
            } else {
                outer += 1;
            }
        }
    }
    // The rain's rate puts most of it in the middle half of the shower.
    assert!(
        middle > outer,
        "{middle} in the middle half, {outer} outside it"
    );
    // The same seed plans the same storm.
    assert_eq!(
        Storm::new(42, ClockRainAmount::Heavy),
        Storm::new(42, ClockRainAmount::Heavy)
    );
}

#[test]
fn the_room_never_flashes_more_than_three_times_in_a_second() {
    let mut most_strokes = 0;
    for seed in 0..300 {
        let (levels, strikes) = over_a_floor(seed);
        let strokes: usize = strikes.iter().map(|strike| strike.strokes.len()).sum();
        most_strokes = strikes
            .iter()
            .map(|strike| strike.strokes.len())
            .fold(most_strokes, usize::max);
        let flashes = flashes(&levels);
        // Every stroke is a flash of its own: the light falls far enough
        // between them to count.
        assert_eq!(flashes.len(), strokes, "{seed}");
        for (index, &tick) in flashes.iter().enumerate() {
            let within = flashes[index..]
                .iter()
                .take_while(|&&later| later < tick + crate::FIXED_HZ as usize)
                .count();
            assert!(
                within <= 3,
                "{seed}: {within} flashes in a second from {tick}"
            );
        }
    }
    assert_eq!(most_strokes, 3);
}

#[test]
fn a_strike_flashes_full_on_its_first_tick_and_is_dark_a_quarter_second_after_its_last_stroke() {
    for seed in 0..40 {
        let (_, strikes) = over_a_floor(seed);
        for strike in strikes {
            assert_eq!(strike.flash(strike.tick), 1.0);
            assert_eq!(strike.channel(strike.tick), 1.0);
            assert_eq!(strike.flash(strike.tick - 1), 0.0);
            let last = strike.tick + u64::from(strike.strokes.last().unwrap().at);
            for stroke in &strike.strokes {
                let at = strike.tick + u64::from(stroke.at);
                assert!(strike.flash(at) >= stroke.peak, "{seed}");
            }
            assert_eq!(strike.flash(last + 15), 0.0);
            assert_eq!(strike.channel(last + 20), 0.0);
            assert!(strike.sparks.iter().all(|spark| spark.life <= SPARK_TICKS));
        }
    }
}

#[test]
fn a_bolt_drops_every_step_from_the_canopy_to_the_highest_surface_below_and_its_forks_die_out_above_it()
 {
    let mut targets = BTreeSet::new();
    for aspect in [480.0 / 800.0, 1024.0 / 768.0, 800.0 / 480.0] {
        let layout = Layout::new(aspect);
        let limit = open_room(&layout);
        for seed in 0..12 {
            let mut event = RainEvent::new(layout, seed, ClockRainAmount::Heavy, display());
            while event.tick < RAINING_TICKS {
                event.step();
                let Some(strike) = event.storm().began(event.tick) else {
                    continue;
                };
                let channel = &strike.bolt.channel;
                assert_eq!(channel[0].y, layout.canopy_y);
                assert!(channel.windows(2).all(|pair| pair[1].y < pair[0].y));
                assert!(channel.iter().all(|point| point.x.abs() <= limit));
                let ground = strike.ground();
                let (surface, target) =
                    surface(&event.water, event.surfaces.floor_pools, &layout, ground.x);
                assert_eq!(ground.y, surface.min(layout.canopy_y - layout.pitch));
                assert_eq!(strike.target, target);
                let lowest = ground.y.max(layout.floor_y) + layout.pitch * 0.3;
                for fork in &strike.bolt.forks {
                    assert!(channel.contains(&fork[0]));
                    assert!(fork.len() >= 3);
                    assert!(fork.windows(2).all(|pair| pair[1].y < pair[0].y));
                    assert!(fork[1..].iter().all(|point| point.y >= lowest));
                    assert!(fork.iter().all(|point| point.x.abs() <= limit));
                }
                targets.insert(format!("{:?}", strike.target));
            }
        }
    }
    // Bolts struck the face's lit cells and the water pooled on the floor.
    assert!(
        targets.contains("Digit") && targets.contains("Water"),
        "{targets:?}"
    );
}

#[test]
fn a_bolt_keeps_clear_of_the_duck() {
    let layout = Layout::new(800.0 / 480.0);
    let limit = open_room(&layout);
    let floor = |_| (layout.floor_y, ClockStrikeTarget::Floor);
    for seed in 0..60 {
        let planned = Storm::new(seed, ClockRainAmount::Heavy);
        for (tick, duck) in planned
            .ticks()
            .into_iter()
            .zip([-300.0, -40.0, 0.0, 60.0, 340.0])
        {
            let mut storm = planned.clone();
            storm.next = storm.planned.iter().position(|p| p.tick == tick).unwrap();
            assert!(storm.step(tick, &layout, floor, Some((duck, 40.0))));
            let x = storm.latest().unwrap().ground().x;
            assert!(
                (x - duck).abs() >= 40.0 - 1e-3,
                "{seed}: {x} by the duck at {duck}"
            );
            assert!(x.abs() <= limit);
        }
    }
}

#[test]
fn lightning_leaves_the_rain_and_its_duck_as_they_would_be_without_it() {
    let layout = Layout::new(800.0 / 480.0);
    for seed in [3, 7, 11] {
        let mut stormy = RainEvent::new(layout, seed, ClockRainAmount::Heavy, display());
        let mut calm =
            RainEvent::new(layout, seed, ClockRainAmount::Heavy, display()).without_lightning();
        loop {
            let done = stormy.step();
            assert_eq!(done, calm.step());
            assert_eq!(
                ClockRainState {
                    strikes_planned: 0,
                    strikes: 0,
                    flash_milli: 0,
                    last_strike: None,
                    ..stormy.diagnostics()
                },
                calm.diagnostics()
            );
            if done {
                break;
            }
        }
        let rain = stormy.diagnostics();
        assert!(rain.strikes >= 3 && rain.strikes == rain.strikes_planned);
        assert!(rain.last_strike.is_some());
        assert_eq!(calm.diagnostics().strikes_planned, 0);
    }
}
