//! The standard for putting digit cells back where they belong: matter travels,
//! and a reading is never dissolved where it stands. The visible frame and the
//! plant lighting occluders are both measured, because they have to agree about
//! where the lit cells are.

use super::*;
use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};
use engine_common::{ClockEventKind, ClockEventProfile, ClockMarqueePreset, Scenario};
use std::time::Duration;

const ASPECT: f32 = 800.0 / 480.0;
const SEED: u64 = 7;

fn reading() -> ClockReading {
    ClockReading::new(8, 8, 0).expect("a valid reading")
}

fn plain() -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: ASPECT,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        SEED,
    );
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading())],
        Duration::ZERO,
    );
    state
}

fn marquee(preset: ClockMarqueePreset) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: ASPECT,
            event_profile: ClockEventProfile::Off,
            marquee_preset: preset,
            ..ClockConfig::default()
        },
        SEED,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(reading()),
            ClockAction::preview_event(ClockEventKind::Marquee),
        ],
        Duration::ZERO,
    );
    state
}

fn advance(state: &mut ClockState) {
    ClockScenario::step(state, &[], Duration::from_secs_f64(1.0 / 60.0));
}

/// Every filled square on one layer, as (centre, side, alpha).
fn squares(frame: &RenderFrame, z: i32) -> Vec<(Vec2, f32, f32)> {
    frame
        .layers
        .iter()
        .filter(|layer| layer.z == z)
        .flat_map(|layer| &layer.primitives)
        .filter_map(|primitive| {
            let RenderPrimitive::Polygon(polygon) = primitive else {
                return None;
            };
            if polygon.points.len() != 4 {
                return None;
            }
            let fill = polygon.fill.as_ref()?;
            let corners: Vec<Vec2> = polygon.points.iter().map(|p| Vec2::new(p.x, p.y)).collect();
            let centre = corners.iter().fold(Vec2::ZERO, |sum, p| sum + *p) * 0.25;
            Some((centre, (corners[1] - corners[0]).length(), fill.color.a))
        })
        .collect()
}

/// Cell-sized filled squares on one layer, as (centre, alpha). The glow pass
/// draws a wider halo on its own layer, so matching the side leaves just the
/// cells themselves and excludes a message's smaller type along with it.
fn cells(frame: &RenderFrame, z: i32, side: f32) -> Vec<(Vec2, f32)> {
    squares(frame, z)
        .into_iter()
        .filter(|(_, measured, _)| (measured - side).abs() <= side * 0.02)
        .map(|(centre, _, alpha)| (centre, alpha))
        .collect()
}

fn centroid(points: &[(Vec2, f32)]) -> Vec2 {
    assert!(!points.is_empty(), "a centroid needs at least one cell");
    points.iter().fold(Vec2::ZERO, |sum, (p, _)| sum + *p) / points.len() as f32
}

fn presets() -> impl Iterator<Item = ClockMarqueePreset> {
    ClockMarqueePreset::ALL
        .into_iter()
        .filter(|preset| *preset != ClockMarqueePreset::Random)
}

fn anchored() -> (Layout, f32, Vec<Vec2>) {
    let reference = plain();
    let layout = Layout::new(reference.aspect_ratio());
    let side = layout.pitch * 0.8;
    let drawn = cells(&render_frame(&reference), ACTIVE_CELL_LAYER, side);
    assert!(
        drawn.len() >= 80,
        "the plain face draws its whole reading, not {} cells",
        drawn.len()
    );
    assert!(
        drawn.iter().all(|(_, alpha)| *alpha > 0.999),
        "an ordinary reading is fully lit"
    );
    let places = drawn.into_iter().map(|(centre, _)| centre).collect();
    (layout, side, places)
}

/// A clock preset animates the face's own cells, so the takeover has to open on
/// the face's pose exactly: same places, same scale, same full brightness, and
/// one copy of each cell rather than a second copy crossfaded over the first.
#[test]
fn clock_face_contract_a_clock_preset_opens_on_the_anchored_face() {
    let (_, side, places) = anchored();
    for preset in presets() {
        let state = marquee(preset);
        let Some(crate::events::ActiveEvent::Marquee(event)) = &state.active_event else {
            panic!("{} leaves a marquee active", preset.as_str());
        };
        if !event.recipe.handover {
            continue;
        }
        assert_eq!(
            event.face_sweep(),
            None,
            "{} animates the face itself and never sweeps it",
            preset.as_str()
        );
        let drawn = cells(&render_frame(&state), ACTIVE_CELL_LAYER, side);
        for centre in &places {
            let found: Vec<f32> = drawn
                .iter()
                .filter(|(at, _)| (*at - *centre).length() < side * 0.1)
                .map(|(_, alpha)| *alpha)
                .collect();
            assert_eq!(
                found.len(),
                1,
                "{} draws the cell at {centre:?} exactly once, not {} times",
                preset.as_str(),
                found.len()
            );
            assert!(
                found[0] > 0.999,
                "{} opens that cell at full brightness, not {:.3}",
                preset.as_str(),
                found[0]
            );
        }
    }
}

/// A text takeover brings its own content, so the reading leaves by travelling
/// the way the message travels and comes back the same way. It never dims where
/// it stands, and the dim anchors it sat on stay put as the substrate.
#[test]
fn clock_face_contract_a_text_takeover_carries_the_reading_clear_and_back() {
    let (layout, side, places) = anchored();
    let home = places.iter().fold(Vec2::ZERO, |sum, p| sum + *p) / places.len() as f32;
    let anchors = cells(&render_frame(&plain()), INACTIVE_CELL_LAYER, side);
    assert!(!anchors.is_empty(), "the face draws dim anchors");

    for preset in presets() {
        let mut state = marquee(preset);
        let handover = matches!(&state.active_event,
            Some(crate::events::ActiveEvent::Marquee(event)) if event.recipe.handover);
        if handover {
            continue;
        }
        // Everything travels the way the message does, so both the exit and the
        // return are leftward: the offset only ever decreases.
        let mut leaving = f32::INFINITY;
        let mut cleared = false;
        let mut entered = None;
        let mut returning = f32::INFINITY;
        for tick in 0..crate::events::marquee::MARQUEE_TICKS {
            let frame = render_frame(&state);
            let drawn = cells(&frame, ACTIVE_CELL_LAYER, side);
            assert_eq!(
                cells(&frame, INACTIVE_CELL_LAYER, side).len(),
                anchors.len(),
                "{} keeps every dim anchor at tick {tick}",
                preset.as_str()
            );
            if drawn.is_empty() {
                cleared = true;
            } else {
                assert!(
                    drawn.iter().all(|(_, alpha)| *alpha > 0.999),
                    "{} never dims the reading to make it leave, but did at tick {tick}",
                    preset.as_str()
                );
                assert_eq!(
                    drawn.len(),
                    places.len(),
                    "{} carries the whole reading at tick {tick}",
                    preset.as_str()
                );
                let offset = centroid(&drawn).x - home.x;
                if !cleared {
                    assert!(
                        offset <= leaving + 1e-3,
                        "{} carries the reading out one way, but it turned back at tick \
                         {tick}: {offset:.2} after {leaving:.2}",
                        preset.as_str()
                    );
                    leaving = offset;
                } else {
                    assert!(
                        offset <= returning + 1e-3,
                        "{} brings the reading home the same way it left, but it reversed \
                         at tick {tick}: {offset:.2} after {returning:.2}",
                        preset.as_str()
                    );
                    entered = entered.or(Some(offset));
                    returning = offset;
                }
            }
            advance(&mut state);
        }
        assert!(
            cleared,
            "{} takes the face over, so the reading leaves the frame",
            preset.as_str()
        );
        assert!(
            leaving < -(layout.bounds_max.x - layout.bounds_min.x) * 0.5,
            "{} carries the reading well clear before it stops drawing it, not {leaving:.2}",
            preset.as_str()
        );
        let entered = entered.unwrap_or_else(|| {
            panic!("{} brings the reading back before it ends", preset.as_str())
        });
        assert!(
            entered > (layout.bounds_max.x - layout.bounds_min.x) * 0.5,
            "{} brings the reading back from the far side rather than the one it left \
             towards, entering at {entered:.2}",
            preset.as_str()
        );
        let home_again = cells(&render_frame(&state), ACTIVE_CELL_LAYER, side);
        assert_eq!(
            home_again.len(),
            places.len(),
            "{} ends with the whole reading back",
            preset.as_str()
        );
        assert!(
            (centroid(&home_again) - home).length() < side * 0.01,
            "{} lands the reading exactly where it started",
            preset.as_str()
        );
    }
}

/// The reading's light-blocking geometry is the reading. A digit that has been
/// carried off no longer shades the plants it used to stand over, and one part
/// way across shades what it is actually over.
#[test]
fn clock_face_contract_the_lighting_occluders_follow_the_reading() {
    let (_, side, places) = anchored();
    let home = places.iter().fold(Vec2::ZERO, |sum, p| sum + *p) / places.len() as f32;
    for preset in presets() {
        let mut state = marquee(preset);
        for tick in 0..90u64 {
            let drawn = cells(&render_frame(&state), ACTIVE_CELL_LAYER, side);
            let occluders = plant_face_polygons(&state);
            if drawn.is_empty() {
                advance(&mut state);
                continue;
            }
            let reading = centroid(&drawn);
            let shading: Vec<(Vec2, f32)> = occluders
                .iter()
                .filter(|polygon| polygon.len() == 4)
                .filter(|polygon| ((polygon[1] - polygon[0]).length() - side).abs() < side * 0.02)
                .map(|polygon| {
                    (
                        polygon.iter().fold(Vec2::ZERO, |sum, p| sum + *p) * 0.25,
                        1.0,
                    )
                })
                .collect();
            assert_eq!(
                shading.len(),
                drawn.len(),
                "{} shades the plants with exactly the cells it draws, at tick {tick}",
                preset.as_str()
            );
            assert!(
                (centroid(&shading) - reading).length() < side * 0.01,
                "{} shades the plants where the reading actually is at tick {tick}: \
                 occluders at {:?}, cells at {reading:?}, home {home:?}",
                preset.as_str(),
                centroid(&shading)
            );
            advance(&mut state);
        }
    }
}

/// A clock preset keeps the reading lit for its whole visit. Its own lighting
/// pattern dims some cells and brightens others, and a scaling pose moves them,
/// but at no point is the reading put out. Counted at any scale, since a preset
/// that spins is entitled to shrink the reading to stay inside the frame.
#[test]
fn clock_face_contract_a_clock_preset_never_puts_the_reading_out() {
    let (_, _, places) = anchored();
    let full = places.len() as f32;
    for preset in presets() {
        let mut state = marquee(preset);
        let handover = matches!(&state.active_event,
            Some(crate::events::ActiveEvent::Marquee(event)) if event.recipe.handover);
        if !handover {
            continue;
        }
        let mut dimmest = f32::INFINITY;
        for tick in 0..crate::events::marquee::MARQUEE_TICKS {
            let ink: f32 = squares(&render_frame(&state), ACTIVE_CELL_LAYER)
                .iter()
                .map(|(_, _, alpha)| *alpha)
                .sum();
            assert!(
                ink > full * 0.45,
                "{} keeps the reading lit, but its ink fell to {ink:.1} of {full} at tick \
                 {tick}",
                preset.as_str()
            );
            dimmest = dimmest.min(ink);
            advance(&mut state);
        }
        assert!(
            dimmest.is_finite(),
            "{} draws the reading throughout its visit",
            preset.as_str()
        );
    }
}

/// A clock preset animates the face's own cells, so its opening pose has to be
/// the face's own pose. A rotation fit that shrinks the reading to keep a
/// spinning glyph inside the frame must ease in from the anchored scale rather
/// than snapping smaller on the first tick, which reads as the reading vanishing
/// and a differently sized one appearing in its place.
#[test]
fn clock_face_contract_a_clock_preset_opens_on_the_face_pose() {
    let (layout, side, places) = anchored();
    for preset in presets() {
        let state = marquee(preset);
        let Some(crate::events::ActiveEvent::Marquee(event)) = &state.active_event else {
            panic!("{} leaves a marquee active", preset.as_str());
        };
        if !event.recipe.handover {
            continue;
        }
        let playback = event.playback();
        assert_eq!(
            playback.strength,
            0.0,
            "{} opens before its own animation has any strength",
            preset.as_str()
        );
        let placement = super::marquee::placement(event, layout);
        for cell in &event.content.cells {
            let quad = event.recipe.quad(*cell, placement, playback);
            let centre = (quad[0] + quad[2]) * 0.5;
            let measured = (quad[1] - quad[0]).length();
            // The colon sits between the digits rather than on a segment cell,
            // so it is the one part of the reading with no anchored place.
            let known = places
                .iter()
                .any(|place| (*place - centre).length() < side * 0.01);
            assert!(
                known || centre.x.abs() < layout.pitch,
                "{} opens every cell on the face's own place, but {centre:?} is neither \
                 an anchored cell nor the colon",
                preset.as_str()
            );
            assert!(
                (measured - side).abs() < side * 0.01,
                "{} opens at the face's own scale, not {measured:.2} against {side:.2}",
                preset.as_str()
            );
        }
    }
}
