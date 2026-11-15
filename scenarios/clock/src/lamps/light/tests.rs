//! The lamps' day, and what they give the plants by day and by night.

use super::*;
use crate::ceiling::Ceiling;
use engine_core::lighting::{
    self, DirectionalSource, LightRay, LightingSnapshot, Receiver, Visibility,
};
use engine_rapier::plant_light::DIM_ROOM;

const ASPECTS: [f32; 6] = [0.25, 0.6, 0.75, 1024.0 / 768.0, 800.0 / 480.0, 4.0];

struct Clear;
impl Visibility for Clear {
    fn blocker(&self, _: LightRay) -> Option<LightId> {
        None
    }
}

fn at(seconds: u32) -> f32 {
    daylight(ClockReading::new(
        (seconds / 3600) as u8,
        (seconds / 60 % 60) as u8,
        (seconds % 60) as u8,
    ))
}

fn clock(hour: u32, minute: u32) -> u32 {
    hour * 3600 + minute * 60
}

/// What `sources` give `points` with nothing in the way and no other light.
fn given(sources: &[LocalSource], points: &[Vec2]) -> Vec<f32> {
    let dark = DirectionalSource {
        toward_source: Vec2::Y,
        strength: 0.0,
        color: Color::WHITE,
    };
    let snapshot = LightingSnapshot::new(0, 0, dark, sources).unwrap();
    points
        .chunks(lighting::MAX_RECEIVERS)
        .flat_map(|chunk| {
            let receivers: Vec<_> = chunk
                .iter()
                .map(|&position| Receiver {
                    position,
                    surface: None,
                })
                .collect();
            snapshot
                .query(0, 0, &receivers, &Clear)
                .unwrap()
                .into_iter()
                .map(|exposure| exposure.intensity)
        })
        .collect()
}

#[test]
fn the_lamps_keep_a_day_from_seven_to_ten_with_half_an_hour_of_dawn_and_dusk() {
    for (hour, minute) in [(0, 0), (3, 0), (6, 0), (6, 29), (22, 30), (23, 0), (23, 59)] {
        assert_eq!(at(clock(hour, minute)), 0.0, "{hour}:{minute:02}");
    }
    for (hour, minute) in [(7, 0), (9, 30), (12, 0), (21, 59), (22, 0)] {
        assert_eq!(at(clock(hour, minute)), 1.0, "{hour}:{minute:02}");
    }
    assert!((at(clock(6, 45)) - 0.5).abs() < 1e-6);
    assert!((at(clock(22, 15)) - 0.5).abs() < 1e-6);
    // Dawn rises and dusk falls a second at a time, without a step.
    let (mut dawn, mut dusk) = (0.0, 1.0);
    for second in 0..=TWILIGHT {
        let rising = at(DAY.0 - TWILIGHT + second);
        let falling = at(DAY.1 + second);
        assert!(
            rising >= dawn && rising - dawn < 0.001,
            "dawn at {second} s"
        );
        assert!(
            falling <= dusk && dusk - falling < 0.001,
            "dusk at {second} s"
        );
        (dawn, dusk) = (rising, falling);
    }
    assert_eq!((dawn, dusk), (1.0, 0.0));
    assert_eq!(daylight(None), 1.0);
    assert_eq!((output(1.0), output(0.0)), (1.0, NIGHT_OUTPUT));
}

#[test]
fn at_night_both_lamps_at_their_brightest_leave_every_grass_shoot_in_shade() {
    let threshold = f32::from(engine_core::plants::grass::physiology().local_light_threshold);
    let night = output(0.0);
    for aspect in ASPECTS {
        let layout = Layout::new(aspect);
        let mut brightest = 0.0_f32;
        for brightness in (0..=30).map(|i| i as f32 * 0.05) {
            for narrow in (0..=20).map(|i| i as f32 * 0.05) {
                for target in [Vec2::ZERO, Vec2::new(-5.0, -10.0), Vec2::new(40.0, 20.0)] {
                    let look = Look {
                        brightness,
                        narrow,
                        target,
                        subject: None,
                    };
                    let both: f32 = (0..2)
                        .flat_map(|lamp| sources(lamp, look, &layout, night))
                        .map(|source| source.strength)
                        .sum();
                    brightest = brightest.max(both);
                }
            }
        }
        // No receiver gets more than the sources' strengths, wherever it is,
        // and the room's light adds its own.
        assert!(brightest < 175.0, "aspect {aspect}: {brightest}");
        assert!(brightest + DIM_ROOM < threshold, "aspect {aspect}");
    }
}

#[test]
fn by_day_a_resting_lamp_with_nothing_in_its_way_pays_a_full_grass_patch_along_its_corridor() {
    // A full patch under even light holds at 500 and starves below 450.
    const PAYS: f32 = 550.0;
    for aspect in ASPECTS {
        let layout = Layout::new(aspect);
        for (lamp, (left, right)) in Ceiling::corridors(layout).into_iter().enumerate() {
            let points: Vec<Vec2> = (0..=16)
                .flat_map(|i| {
                    let x = left + (right - left) * i as f32 / 16.0;
                    [layout.floor_y, layout.floor_y + 30.0].map(|y| Vec2::new(x, y))
                })
                .collect();
            let light = given(&sources(lamp, Look::STEADY, &layout, 1.0), &points);
            let least = light.iter().copied().fold(f32::INFINITY, f32::min);
            assert!(
                least + DIM_ROOM >= PAYS,
                "aspect {aspect}, lamp {lamp}: {least}"
            );
        }
    }
}

#[test]
fn a_spotlight_sends_its_light_along_its_beam_and_a_lamp_that_is_out_sends_none() {
    for aspect in ASPECTS {
        let layout = Layout::new(aspect);
        let look = Look {
            brightness: SPOT_PEAK,
            narrow: 1.0,
            target: Vec2::new(5.0, 2.0),
            subject: None,
        };
        let spot = sources(0, look, &layout, 1.0);
        assert_eq!(spot.len(), 1, "a full spotlight is all beam");
        let beam = beam_from(0, entry(0, &layout), look, &layout);
        let aim = Vec2::new(beam.aim.cos(), beam.aim.sin());
        let beside = beam.apex + aim.rotate_radians(beam.spread * 1.5) * beam.distance;
        let light = given(&spot, &[beam.target, beside]);
        // The pool takes the beam's whole share, as if it lit every way.
        let every_way = LocalSource {
            cone: None,
            ..spot[0].clone()
        };
        let whole = given(&[every_way], &[beam.target])[0];
        assert!(
            whole > 0.0 && (light[0] - whole).abs() < 0.01,
            "aspect {aspect}: {light:?} {whole}"
        );
        assert_eq!(light[1], 0.0, "aspect {aspect}: beside the beam");
        let out = Look {
            brightness: 0.0,
            ..Look::STEADY
        };
        assert!(sources(0, out, &layout, 1.0).is_empty());
    }
}

#[test]
fn the_night_glow_still_shows() {
    assert_eq!(seen(output(1.0)), 1.0);
    assert!(seen(output(0.0)) > 0.3);
    assert!(seen(output(0.0)) < 0.35);
}
