//! What the lamps do with what they perceive, fed percepts by hand: a lamp
//! notices a mover on its side, follows it closely and rests after losing
//! it; the eager lamp notices what the lazy one lets pass; a lamp takes up
//! only movers that catch its eye on their own; a lamp tires of a mover; a
//! mover crossing the middle passes between the lamps, unless the other is
//! tired of it; the lamps share only a mover that far outshines the rest; a
//! hot bulb goes out, and never both; sparks fly as a bulb goes dark; a big
//! knock dips the power; a knock near a lamp startles it; a lamp presents
//! the digits that changed on its side; a resting lamp is exactly steady but
//! for its fidgets; the beam moves under its limits and stays in the room;
//! Steady lamps rest; and the brain is seeded.

use super::*;
use crate::lamps::perception::{Impact, Signals};
use crate::layout::Layout;
use engine_rapier::world::{BodyId, BodyRole, PhysicsId};
use rand::SeedableRng;

fn stage() -> Stage {
    crate::ClockState::lamp_stage(&Layout::new(800.0 / 480.0))
}

fn seeded(seed: u64) -> (Brain, StdRng) {
    let mut rng = StdRng::seed_from_u64(seed);
    let brain = Brain::new(&mut rng);
    (brain, rng)
}

/// A brain whose left lamp is eager, or lazy.
fn brain_with(left_eager: bool) -> (Brain, StdRng) {
    (0..)
        .map(seeded)
        .find(|(brain, _)| brain.lamps[0].eager == left_eager)
        .unwrap()
}

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::new(0))
}

fn mover(n: u64, position: Vec2, velocity: Vec2, salience: f32) -> Mover {
    Mover {
        world: 0,
        body: body(n),
        position,
        velocity,
        size: 0.5 * stage().pitch,
        age: 0,
        salience,
    }
}

/// A velocity fast enough to count as moving.
fn drift(stage: &Stage) -> Vec2 {
    Vec2::new(3.0 * stage.pitch, 0.0)
}

fn seeing(movers: Vec<Mover>) -> Percepts {
    Percepts {
        movers,
        ..Percepts::default()
    }
}

fn knock(position: Vec2, strength: f32) -> Percepts {
    Percepts {
        impacts: vec![Impact {
            world: 0,
            body: body(99),
            position,
            strength,
        }],
        ..Percepts::default()
    }
}

/// Cells to world units, from the room's lower-left corner.
fn at(stage: &Stage, x: f32, y: f32) -> Vec2 {
    stage.min + Vec2::new(x, y) * stage.pitch
}

/// The angle at a lamp's head between where its beam points and `point`,
/// in degrees.
fn off_by(brain: &Brain, lamp: usize, stage: &Stage, point: Vec2) -> f32 {
    let head = stage.heads[lamp];
    let aim = brain.lamps[lamp].aim - head;
    let to = point - head;
    (aim.dot(to) / (aim.length() * to.length()))
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

fn target(brain: &Brain, lamp: usize) -> Option<u64> {
    brain.lamps[lamp]
        .target
        .map(|(_, body)| body.entity.value())
}

/// The largest angle a tracking beam strays from a mover at eight cells a
/// second, once it has followed it for half a second.
const TRACKING_DEGREES: f32 = 4.0;

#[test]
fn a_lamp_notices_a_mover_on_its_side_follows_it_closely_and_rests_after_losing_it() {
    let stage = stage();
    let speed = 8.0 * stage.pitch;
    for left_eager in [true, false] {
        let (mut brain, mut rng) = brain_with(left_eager);
        let temper = usize::from(left_eager);
        let mut position = at(&stage, 2.0, 3.0);
        let (mut noticed, mut tracked, mut worst) = (None, None, 0.0f32);
        let mut tick = 0;
        while stage.side(position.x) == Some(0) {
            let percepts = seeing(vec![mover(1, position, Vec2::new(speed, 0.0), 10.0)]);
            brain.step(&mut rng, &percepts, stage, true);
            match brain.activity(0) {
                Activity::Notice => {
                    noticed.get_or_insert(tick);
                }
                Activity::Track => {
                    tracked.get_or_insert(tick);
                }
                other => assert!(noticed.is_none(), "tick {tick}: {other:?}"),
            }
            if tracked.is_some_and(|from| tick >= from + FIXED_HZ / 2) {
                worst = worst.max(off_by(&brain, 0, &stage, position));
            }
            // The right lamp leaves the left lamp's mover to it.
            assert_eq!(brain.activity(1), Activity::Rest, "tick {tick}");
            position += Vec2::new(speed, 0.0) * DT;
            tick += 1;
        }
        let noticed = noticed.expect("the lamp noticed the mover");
        let tracked = tracked.expect("the lamp followed the mover");
        assert!(noticed <= FIXED_HZ / 2, "noticed at {noticed}");
        assert_eq!(
            tracked,
            noticed + NOTICE_TICKS[temper],
            "{noticed} {tracked}"
        );
        assert!(worst <= TRACKING_DEGREES, "strayed {worst:.2}°");
        assert_eq!(brain.lamps[0].look.narrow, 1.0);
        // It stops, and drops out of what the lamps see: the lamp searches
        // where it was, gives up and widens back to rest.
        let mut rested = None;
        for tick in 0..5 * FIXED_HZ {
            brain.step(&mut rng, &Percepts::default(), stage, true);
            if tick == 0 {
                assert_eq!(brain.activity(0), Activity::Search);
            }
            if brain.lamps[0].look == Look::STEADY {
                rested.get_or_insert(tick);
            } else {
                assert!(rested.is_none(), "tick {tick}");
            }
        }
        let rested = rested.expect("the lamp came back to rest");
        assert!(rested <= SEARCH_TICKS + FIXED_HZ, "rested after {rested}");
        assert_eq!(brain.counts().notices, 1);
        assert_eq!(brain.counts().tracks, 1);
    }
}

#[test]
fn the_eager_lamp_notices_movers_the_lazy_one_lets_pass() {
    let stage = stage();
    let (mut brain, mut rng) = brain_with(true);
    // Movers between the two lamps' thresholds, one on each side, moving
    // back and forth on the spot.
    for tick in 0..3 * FIXED_HZ {
        let sway = Vec2::new((tick as f32 * 0.1).sin() * stage.pitch, 0.0);
        let percepts = seeing(vec![
            mover(1, at(&stage, 6.0, 4.0) + sway, drift(&stage), 4.5),
            mover(
                2,
                stage.max - Vec2::splat(6.0 * stage.pitch) + sway,
                drift(&stage),
                4.5,
            ),
        ]);
        brain.step(&mut rng, &percepts, stage, true);
    }
    assert_eq!(brain.activity(0), Activity::Track);
    assert_eq!(target(&brain, 0), Some(1));
    // The eager lamp stays on its side, though the lazy one lets its mover
    // pass: that lamp is free to notice it.
    assert_eq!(brain.activity(1), Activity::Rest);
    assert_eq!(brain.counts().notices, 1);
}

#[test]
fn a_lamp_tires_of_a_mover_and_leaves_it_alone_for_a_while() {
    let stage = stage();
    for left_eager in [true, false] {
        let (mut brain, mut rng) = brain_with(left_eager);
        let temper = usize::from(left_eager);
        let circling = |tick: u32| {
            let angle = tick as f32 * DT;
            at(&stage, 8.0, 6.0) + Vec2::new(angle.cos(), angle.sin()) * (2.0 * stage.pitch)
        };
        let (mut tracked, mut tired, mut again) = (None, None, None);
        for tick in 0..30 * FIXED_HZ {
            let percepts = seeing(vec![mover(1, circling(tick), drift(&stage), 10.0)]);
            // A cool bulb, so its heat plays no part.
            brain.lamps[0].heat = 0.0;
            brain.step(&mut rng, &percepts, stage, true);
            match brain.activity(0) {
                Activity::Track => {
                    tracked.get_or_insert(tick);
                }
                Activity::Rest if tracked.is_some() => {
                    tired.get_or_insert(tick);
                }
                Activity::Notice if tired.is_some() => {
                    again.get_or_insert(tick);
                }
                _ => {}
            }
        }
        let (tracked, tired) = (tracked.unwrap(), tired.unwrap());
        let patience = (PATIENCE_SECONDS[temper] * FIXED_HZ as f32) as u32;
        assert!(tired.abs_diff(tracked + patience) <= 1, "{tracked} {tired}");
        // It noticed the mover once more ten seconds after it tired of it, as
        // soon as its interest came back.
        assert_eq!(brain.counts().notices, 2);
        let again = again.expect("the lamp noticed the mover again");
        let alone = TIRED_TICKS as u32;
        assert!(
            again >= tired + alone && again <= tired + alone + FIXED_HZ / 2,
            "tired at {tired}, noticed again at {again}"
        );
    }
}

#[test]
fn a_mover_crossing_the_middle_passes_to_the_lamp_on_its_new_side() {
    let stage = stage();
    for left_eager in [true, false] {
        let (mut brain, mut rng) = brain_with(left_eager);
        let speed = 8.0 * stage.pitch;
        let mut position = at(&stage, 3.0, 3.0);
        let mut handed = None;
        for tick in 0..10 * FIXED_HZ {
            let percepts = seeing(vec![mover(1, position, Vec2::new(speed, 0.0), 10.0)]);
            brain.step(&mut rng, &percepts, stage, true);
            let side = stage.side(position.x);
            if side == Some(1) && handed.is_none() {
                handed = Some(tick);
                assert_eq!(brain.counts().handoffs, 1, "tick {tick}");
                assert_eq!(brain.activity(1), Activity::Track);
                assert_eq!(target(&brain, 1), Some(1));
                assert_eq!(brain.activity(0), Activity::Rest);
                assert_eq!(target(&brain, 0), None);
            }
            // Never both on it: it is not salient enough to share.
            assert!(
                target(&brain, 0).is_none() || target(&brain, 1).is_none(),
                "tick {tick}"
            );
            if position.x > stage.max.x - 4.0 * stage.pitch {
                break;
            }
            position += Vec2::new(speed, 0.0) * DT;
        }
        assert!(handed.is_some());
        assert_eq!(brain.counts().handoffs, 1);
        assert_eq!(brain.activity(1), Activity::Track);
        assert!(off_by(&brain, 1, &stage, position) <= TRACKING_DEGREES);
    }
}

#[test]
fn a_lamp_tired_of_a_mover_is_not_handed_it() {
    let stage = stage();
    for left_eager in [true, false] {
        let (mut brain, mut rng) = brain_with(left_eager);
        brain.lamps[1].tired_of = Some(((0, body(1)), u64::MAX));
        let speed = 8.0 * stage.pitch;
        let mut position = at(&stage, 3.0, 3.0);
        while position.x < stage.max.x - 4.0 * stage.pitch {
            let percepts = seeing(vec![mover(1, position, Vec2::new(speed, 0.0), 10.0)]);
            brain.step(&mut rng, &percepts, stage, true);
            assert_eq!(target(&brain, 1), None);
            position += Vec2::new(speed, 0.0) * DT;
        }
        // The left lamp follows it on across the room.
        assert_eq!(brain.counts().handoffs, 0);
        assert_eq!(brain.activity(0), Activity::Track);
        assert_eq!(target(&brain, 0), Some(1));
    }
}

#[test]
fn a_lamp_takes_up_only_movers_that_catch_its_eye_on_their_own() {
    let stage = stage();
    // A bright mover crosses the room and goes, then circles on the left
    // until the lamp there tires of it. Two dull movers sway the whole
    // time, one on each side, below either lamp's notice: neither lamp
    // takes them up, though each has interest left from the bright one
    // after the handoff, the search and the tiring.
    let speed = 8.0 * stage.pitch;
    let across = ((stage.max.x - stage.min.x) / speed * FIXED_HZ as f32) as u32;
    let circling = across + 3 * FIXED_HZ;
    for left_eager in [true, false] {
        let (mut brain, mut rng) = brain_with(left_eager);
        let (mut searched, mut tired) = (false, false);
        for tick in 0..circling + 17 * FIXED_HZ {
            let sway = Vec2::new((tick as f32 * 0.07).sin() * 2.0 * stage.pitch, 0.0);
            let mut movers = vec![
                mover(2, at(&stage, 8.0, 7.0) + sway, drift(&stage), 0.5),
                mover(
                    3,
                    stage.max - Vec2::new(8.0, 7.0) * stage.pitch + sway,
                    drift(&stage),
                    0.5,
                ),
            ];
            if tick < across {
                let x = 1.0 + tick as f32 * DT * 8.0;
                movers.push(mover(1, at(&stage, x, 3.0), Vec2::new(speed, 0.0), 10.0));
            } else if (circling..circling + 14 * FIXED_HZ).contains(&tick) {
                let angle = tick as f32 * DT;
                let position = at(&stage, 8.0, 4.0)
                    + Vec2::new(angle.cos(), angle.sin()) * (2.0 * stage.pitch);
                movers.push(mover(4, position, drift(&stage), 10.0));
            }
            brain.lamps[0].heat = 0.0;
            brain.step(&mut rng, &seeing(movers), stage, true);
            for lamp in 0..2 {
                assert!(
                    !matches!(target(&brain, lamp), Some(2 | 3)),
                    "tick {tick}: lamp {lamp} took up a dull mover"
                );
            }
            searched |= brain.activity(1) == Activity::Search;
            tired |= brain.lamps[0].tired_of.is_some();
        }
        assert_eq!(brain.counts().handoffs, 1);
        assert!(searched && tired, "searched {searched}, tired {tired}");
    }
}

#[test]
fn the_lamps_share_only_a_mover_that_far_outshines_the_rest() {
    let stage = stage();
    let place = |n: u64, x: f32, tick: u32| {
        at(&stage, x, 4.0) + Vec2::new(0.0, (tick as f32 * 0.05 + n as f32).sin() * stage.pitch)
    };
    for seed in 0..8 {
        // Two movers on the left: the lamps never hold the same one.
        let (mut brain, mut rng) = seeded(seed);
        for tick in 0..6 * FIXED_HZ {
            let percepts = seeing(vec![
                mover(1, place(1, 5.0, tick), drift(&stage), 10.0),
                mover(2, place(2, 9.0, tick), drift(&stage), 8.0),
            ]);
            brain.step(&mut rng, &percepts, stage, true);
            let [left, right] = [0, 1].map(|lamp| brain.lamps[lamp].claim());
            assert!(left.is_none() || left != right, "seed {seed} tick {tick}");
        }
        // One that far outshines the next: both lamps follow it.
        let (mut brain, mut rng) = seeded(seed);
        for tick in 0..2 * FIXED_HZ {
            let percepts = seeing(vec![
                mover(1, place(1, 5.0, tick), drift(&stage), 40.0),
                mover(2, place(2, 9.0, tick), drift(&stage), 5.0),
            ]);
            brain.step(&mut rng, &percepts, stage, true);
        }
        for lamp in 0..2 {
            assert_eq!(brain.activity(lamp), Activity::Track, "seed {seed}");
            assert_eq!(target(&brain, lamp), Some(1), "seed {seed}");
        }
    }
}

#[test]
fn a_bulb_kept_bright_goes_out_and_the_lamps_are_never_out_together() {
    let stage = stage();
    for left_eager in [true, false] {
        let (mut brain, mut rng) = brain_with(left_eager);
        // A new mover on each side every few seconds keeps both lamps
        // following something, and their bulbs bright.
        let mut first_out = [None; 2];
        let mut bright = [0u32; 2];
        for tick in 0..4 * 60 * FIXED_HZ {
            let wave = u64::from(tick / (4 * FIXED_HZ));
            let sway = Vec2::new((tick as f32 * 0.05).sin() * 2.0 * stage.pitch, 0.0);
            let percepts = seeing(vec![
                mover(
                    10 + 2 * wave,
                    at(&stage, 6.0, 4.0) + sway,
                    drift(&stage),
                    10.0,
                ),
                mover(
                    11 + 2 * wave,
                    stage.max - Vec2::new(6.0, 12.0) * stage.pitch + sway,
                    drift(&stage),
                    10.0,
                ),
            ]);
            brain.step(&mut rng, &percepts, stage, true);
            let out = [0, 1].map(|lamp| brain.activity(lamp) == Activity::Out);
            assert!(!(out[0] && out[1]), "both out at {tick}");
            for (lamp, first) in first_out.iter_mut().enumerate() {
                if out[lamp] {
                    first.get_or_insert(tick);
                }
                if brain.lamps[lamp].look.narrow == 1.0 {
                    bright[lamp] += 1;
                }
            }
        }
        // Each went out, and only after a long while at full spotlight.
        for (lamp, out) in first_out.iter().enumerate() {
            let out = out.unwrap_or_else(|| panic!("lamp {lamp} never went out"));
            assert!(
                out >= (HEAT_SECONDS * FIXED_HZ as f32) as u32,
                "lamp {lamp} out at {out}"
            );
        }
        assert!(brain.counts().outages >= 2);
        assert!(
            bright.iter().all(|&ticks| ticks > 60 * FIXED_HZ),
            "{bright:?}"
        );
    }
}

#[test]
fn sparks_fly_as_a_bulb_goes_dark_briefly_and_stop_when_it_is_calmed() {
    let stage = stage();
    let mut bursts = 0;
    for seed in 0..24 {
        let (mut brain, mut rng) = seeded(seed);
        brain.preview(&mut rng, Activity::Out, 1);
        let (mut calmed, mut calmed_rng) = (brain.clone(), rng.clone());
        let mut sparks = 0;
        while brain.activity(1) == Activity::Out {
            brain.step(&mut rng, &Percepts::default(), stage, true);
            assert_eq!(brain.spark_tick(0), None);
            if let Some(tick) = brain.spark_tick(1) {
                // One burst, from the tick the bulb goes dark.
                assert_eq!(tick, sparks);
                assert_eq!(brain.look(1).brightness, 0.0, "seed {seed}");
                sparks += 1;
            }
            // A copy calmed as its sparks begin throws no more.
            let lively = brain.spark_tick(1) != Some(0);
            calmed.step(&mut calmed_rng, &Percepts::default(), stage, lively);
            if !lively {
                for _ in 0..SPARK_TICKS {
                    calmed.step(&mut calmed_rng, &Percepts::default(), stage, true);
                    assert_eq!(calmed.spark_tick(1), None, "seed {seed}");
                }
            }
        }
        assert!(
            sparks == 0 || sparks == SPARK_TICKS,
            "seed {seed}: {sparks}"
        );
        bursts += u32::from(sparks > 0);
    }
    assert!(bursts > 0 && bursts < 24, "{bursts} of 24 outages sparked");
}

#[test]
fn an_overheated_bulb_waits_for_the_other_to_come_back_before_it_goes_out() {
    let stage = stage();
    let (mut brain, mut rng) = seeded(3);
    brain.preview(&mut rng, Activity::Out, 0);
    brain.lamps[1].heat = 1.0;
    let percepts = seeing(vec![mover(
        1,
        stage.max - Vec2::new(6.0, 12.0) * stage.pitch,
        drift(&stage),
        10.0,
    )]);
    let (mut left_back, mut right_out) = (None, None);
    for tick in 0..20 * FIXED_HZ {
        brain.step(&mut rng, &percepts, stage, true);
        let [left, right] = [0, 1].map(|lamp| brain.activity(lamp));
        assert!(
            !(left == Activity::Out && right == Activity::Out),
            "tick {tick}"
        );
        if left == Activity::Out {
            // The hot lamp rests instead of following what it sees.
            assert_eq!(right, Activity::Rest, "tick {tick}");
        } else {
            left_back.get_or_insert(tick);
        }
        if right == Activity::Out {
            right_out = Some(tick);
            break;
        }
    }
    // Once the left lamp is back, the right one, still hot, follows the
    // mover again and goes out in its turn.
    let left_back = left_back.expect("the left lamp came back");
    let right_out = right_out.expect("the right lamp went out");
    assert!(right_out > left_back, "{left_back} {right_out}");
    assert_eq!(brain.counts().outages, 2);
}

#[test]
fn a_big_knock_dips_both_lamps_and_takes_out_a_hot_bulb() {
    let stage = stage();
    for (heat, out) in [
        ([0.8, 0.2], Some(0)),
        ([0.75, 0.9], Some(1)),
        ([0.3, 0.6], None),
    ] {
        let (mut brain, mut rng) = seeded(5);
        for (lamp, heat) in heat.into_iter().enumerate() {
            brain.lamps[lamp].heat = heat;
            // Having just startled, neither startles at the knock.
            brain.lamps[lamp].startled_at = Some(0);
        }
        brain.step(
            &mut rng,
            &knock(at(&stage, 10.0, 1.0), BIG_KNOCK + 5.0),
            stage,
            true,
        );
        assert_eq!(brain.counts().dips, 1);
        for lamp in 0..2 {
            let dark = brain.lamps[lamp].look.brightness;
            if Some(lamp) == out {
                assert_eq!(brain.activity(lamp), Activity::Out, "{heat:?}");
            } else {
                assert_ne!(brain.activity(lamp), Activity::Out, "{heat:?}");
                assert!(dark < 0.5 * REST_BRIGHTNESS, "{heat:?} {dark}");
            }
        }
        // The dip passes.
        for _ in 0..DIP_TICKS {
            brain.step(&mut rng, &Percepts::default(), stage, true);
        }
        for lamp in (0..2).filter(|&lamp| Some(lamp) != out) {
            assert_eq!(brain.lamps[lamp].look, Look::STEADY);
        }
    }
    // A knock short of big dips nothing, and the power dips at most once
    // in a while.
    let (mut brain, mut rng) = seeded(5);
    brain.step(
        &mut rng,
        &knock(at(&stage, 10.0, 1.0), BIG_KNOCK - 0.5),
        stage,
        true,
    );
    assert_eq!(brain.counts().dips, 0);
    for _ in 0..DIP_REST {
        brain.step(
            &mut rng,
            &knock(at(&stage, 10.0, 1.0), BIG_KNOCK + 5.0),
            stage,
            true,
        );
    }
    assert_eq!(brain.counts().dips, 1);
    brain.step(
        &mut rng,
        &knock(at(&stage, 10.0, 1.0), BIG_KNOCK + 5.0),
        stage,
        true,
    );
    assert_eq!(brain.counts().dips, 2);
}

#[test]
fn a_knock_near_a_lamp_startles_it_at_most_once_a_second() {
    let stage = stage();
    let near = STARTLE_NEAR * (stage.max.y - stage.min.y);
    let below = |lamp: usize, distance: f32| stage.heads[lamp] - Vec2::new(0.0, distance);
    let (mut brain, mut rng) = seeded(9);
    // Not loud enough, near the lamp or twice as far off.
    brain.step(
        &mut rng,
        &knock(below(0, 0.5 * near), STARTLE_KNOCK - 0.5),
        stage,
        true,
    );
    brain.step(
        &mut rng,
        &knock(below(0, 2.0 * near), 2.0 * STARTLE_KNOCK - 1.0),
        stage,
        true,
    );
    assert_eq!(brain.counts().startles, 0);
    // Twice as far off and twice as strong is loud enough, for the left
    // lamp only: the right one is farther still.
    brain.step(
        &mut rng,
        &knock(below(0, 2.0 * near), 2.0 * STARTLE_KNOCK + 1.0),
        stage,
        true,
    );
    assert_eq!(brain.counts().startles, 1);
    assert_eq!(brain.state(0).state, Activity::Startle);
    assert_eq!(brain.state(1).state, Activity::Rest);
    assert_ne!(brain.lamps[0].look.brightness, REST_BRIGHTNESS);
    // Another knock within the second leaves it be; one near the right lamp
    // startles that lamp.
    for tick in 1..STARTLE_REST {
        brain.step(
            &mut rng,
            &knock(below(0, 0.5 * near), STARTLE_KNOCK + 3.0),
            stage,
            true,
        );
        if tick == u64::from(STARTLE_TICKS) {
            assert_eq!(brain.state(0).state, Activity::Rest);
            assert_eq!(brain.lamps[0].look, Look::STEADY);
        }
    }
    assert_eq!(brain.counts().startles, 1);
    brain.step(
        &mut rng,
        &knock(below(1, 0.5 * near), STARTLE_KNOCK + 3.0),
        stage,
        true,
    );
    assert_eq!(brain.counts().startles, 2);
    assert_eq!(brain.state(1).state, Activity::Startle);
    brain.step(
        &mut rng,
        &knock(below(0, 0.5 * near), STARTLE_KNOCK + 3.0),
        stage,
        true,
    );
    assert_eq!(brain.counts().startles, 3);
}

#[test]
fn a_lamp_at_rest_presents_the_digits_that_changed_on_its_side() {
    let stage = stage();
    let minute = Percepts {
        signals: Signals {
            changed_digits: [false, false, false, true],
            ..Signals::default()
        },
        ..Percepts::default()
    };
    for left_eager in [true, false] {
        let (mut brain, mut rng) = brain_with(left_eager);
        let mut presents = 0;
        for _ in 0..40 {
            brain.step(&mut rng, &minute, stage, true);
            let presenting = brain.activity(1) == Activity::Present;
            assert_eq!(brain.activity(0), Activity::Rest);
            for tick in 0..6 * FIXED_HZ {
                brain.step(&mut rng, &Percepts::default(), stage, true);
                if presenting && tick == PRESENT_TICKS / 2 {
                    // Its light settled on the minute's last digit.
                    let look = brain.lamps[1].look;
                    assert_eq!(look.narrow, PRESENT_NARROW);
                    let digit = Vec2::new(DIGIT_ORIGINS[3] + 3.0, 4.5);
                    assert!((look.target - digit).length() < 0.05, "{look:?}");
                }
            }
            assert_eq!(brain.activity(1), Activity::Rest);
            presents += usize::from(presenting);
        }
        assert_eq!(brain.counts().presents as usize, presents);
        // The right lamp is lazy when the left is eager.
        let chance = PRESENT_CHANCE[usize::from(!left_eager)];
        assert!(
            (presents as f64 - 40.0 * chance).abs() <= 8.0,
            "{presents} of 40 at {chance}"
        );
    }
}

#[test]
fn a_resting_lamp_is_exactly_steady_but_for_its_fidgets() {
    let stage = stage();
    let (mut brain, mut rng) = seeded(11);
    let mut fidgeting = 0;
    for _ in 0..10 * 60 * FIXED_HZ {
        brain.step(&mut rng, &Percepts::default(), stage, true);
        for lamp in &brain.lamps {
            assert_eq!(lamp.activity, Activity::Rest);
            if lamp.flicker.is_some() {
                fidgeting += 1;
                assert_eq!(lamp.look.narrow, 0.0);
            } else {
                assert_eq!(lamp.look, Look::STEADY);
            }
        }
    }
    // Ten minutes bring a few fidgets, and they are brief.
    let fidgets = brain.counts().fidgets;
    assert!((4..=20).contains(&fidgets), "{fidgets}");
    assert!(fidgeting < 2 * 60 * fidgets, "{fidgeting} ticks");
}

#[test]
fn the_beam_moves_under_its_limits() {
    let stage = stage();
    for left_eager in [true, false] {
        let (mut brain, mut rng) = brain_with(left_eager);
        let temper = usize::from(left_eager);
        let (speed, acceleration) = (
            AIM_SPEED[temper] * stage.pitch,
            AIM_ACCELERATION[temper] * stage.pitch,
        );
        let mut before = brain.lamps[0].clone();
        for tick in 0..20 * FIXED_HZ {
            // A mover darting about the left side, faster than the beam.
            let t = tick as f32 * DT;
            let position = at(
                &stage,
                8.0 + 5.0 * (3.0 * t).sin(),
                6.0 + 3.0 * (5.0 * t).cos(),
            );
            let percepts = seeing(vec![mover(1, position, Vec2::new(60.0, 0.0), 10.0)]);
            brain.step(&mut rng, &percepts, stage, true);
            let lamp = &brain.lamps[0];
            // Its aim means nothing while the beam is not narrowed.
            if before.narrow > 0.0 && lamp.narrow > 0.0 {
                let moved = (lamp.aim - before.aim).length() / DT;
                let turned = (lamp.aim_velocity - before.aim_velocity).length() / DT;
                assert!(moved <= speed * 1.001, "tick {tick}: {moved} > {speed}");
                assert!(turned <= acceleration * 1.001, "tick {tick}: {turned}");
            }
            let widened = (lamp.narrow_velocity - before.narrow_velocity).abs() / DT;
            assert!(
                lamp.narrow_velocity.abs() <= NARROW_SPEED * 1.001,
                "tick {tick}"
            );
            assert!(
                widened <= NARROW_ACCELERATION * 1.001,
                "tick {tick}: {widened}"
            );
            before = lamp.clone();
        }
    }
}

#[test]
fn a_beam_stops_at_the_rooms_edge_for_a_mover_that_runs_out() {
    let stage = stage();
    let inside = |aim: Vec2| {
        aim.x >= stage.min.x && aim.x <= stage.max.x && aim.y >= stage.min.y && aim.y <= stage.max.y
    };
    for left_eager in [true, false] {
        for cells in [15.0, 40.0] {
            let (mut brain, mut rng) = brain_with(left_eager);
            let speed = cells * stage.pitch;
            // It circles on the right until the lamp there follows it, then
            // runs out through the right side.
            let start = stage.max - Vec2::new(8.0, 9.0) * stage.pitch;
            let mut position = start;
            let mut dashing = false;
            for tick in 0..6 * FIXED_HZ {
                let velocity = if dashing {
                    position += Vec2::new(speed, 0.0) * DT;
                    Vec2::new(speed, 0.0)
                } else {
                    let angle = tick as f32 * DT * 2.0;
                    position = start + Vec2::new(angle.cos(), angle.sin()) * (2.0 * stage.pitch);
                    drift(&stage)
                };
                // In view until three cells past the edge.
                let movers = if position.x < stage.max.x + 3.0 * stage.pitch {
                    vec![mover(1, position, velocity, 10.0)]
                } else {
                    Vec::new()
                };
                brain.step(&mut rng, &seeing(movers), stage, true);
                dashing |= brain.activity(1) == Activity::Track && tick >= 2 * FIXED_HZ;
                for lamp in 0..2 {
                    assert!(
                        inside(brain.lamps[lamp].aim),
                        "{cells} cells/s, tick {tick}: lamp {lamp} aims at {:?}",
                        brain.lamps[lamp].aim
                    );
                }
            }
            assert!(dashing, "the right lamp never followed the mover");
        }
    }
}

#[test]
fn steady_lamps_rest_and_one_that_is_out_warms_back_up() {
    let stage = stage();
    let (mut brain, mut rng) = brain_with(true);
    let percepts = seeing(vec![mover(1, at(&stage, 6.0, 4.0), drift(&stage), 10.0)]);
    for _ in 0..2 * FIXED_HZ {
        brain.step(&mut rng, &percepts, stage, true);
    }
    assert_eq!(brain.activity(0), Activity::Track);
    brain.preview(&mut rng, Activity::Out, 1);
    assert_eq!(brain.activity(1), Activity::Out);
    let mut steady = None;
    for tick in 0..3 * FIXED_HZ {
        brain.step(&mut rng, &percepts, stage, false);
        if (0..2).all(|lamp| brain.lamps[lamp].look == Look::STEADY) {
            steady.get_or_insert(tick);
        } else {
            assert!(steady.is_none(), "tick {tick}");
        }
    }
    let steady = steady.expect("both lamps came back to rest");
    assert!(steady <= FIXED_HZ + WARM_UP_TICKS, "steady after {steady}");
    for lamp in 0..2 {
        assert_eq!(brain.activity(lamp), Activity::Rest);
        assert_eq!(brain.state(lamp).drives.interest_milli, 0);
    }
}

#[test]
fn the_same_seed_gives_the_same_lamps() {
    let stage = stage();
    let run = |seed: u64| {
        let (mut brain, mut rng) = seeded(seed);
        let mut states = Vec::new();
        for tick in 0..30 * FIXED_HZ {
            let t = tick as f32 * DT;
            let percepts = Percepts {
                movers: vec![mover(
                    1,
                    at(&stage, 16.0 + 14.0 * (0.4 * t).sin(), 4.0),
                    drift(&stage),
                    12.0,
                )],
                impacts: (tick % 97 == 0)
                    .then(|| Impact {
                        world: 0,
                        body: body(2),
                        position: at(&stage, 4.0, 1.0),
                        strength: 15.0,
                    })
                    .into_iter()
                    .collect(),
                ..Percepts::default()
            };
            brain.step(&mut rng, &percepts, stage, true);
            states.push([brain.state(0), brain.state(1)]);
        }
        states
    };
    assert_eq!(run(21), run(21));
    let eager = |seed| seeded(seed).0.eager();
    assert!((0..8).any(|seed| eager(seed) != eager(0)));
}
