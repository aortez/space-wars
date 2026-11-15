//! The corner lamps' light on the plants in a running Clock: the grass it
//! keeps after the ceiling closes, the night that ends it, and the floor the
//! face shades.

use super::*;
use crate::{ceiling::Ceiling, ceiling::Phase};
use engine_common::{ClockEventKind, RenderFrame, RenderPrimitive};
use engine_core::plants::{Role, Tissue};
use engine_rapier::plant_light::DIM_ROOM;

const TICK: Duration = Duration::from_nanos(16_666_667);
/// The opening, hold and closing of a Grass event's ceiling, in seconds.
const CLOSED_AT: usize = 5 + 300 + 60 + 1;

fn clock(seed: u64, hour: u8, minute: u8) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        seed,
    );
    read(&mut state, hour, minute);
    state
}

fn read(state: &mut ClockState, hour: u8, minute: u8) {
    ClockScenario::step(
        state,
        &[ClockAction::set_reading(
            ClockReading::new(hour, minute, 0).unwrap(),
        )],
        Duration::ZERO,
    );
}

fn seconds(state: &mut ClockState, n: usize) {
    for _ in 0..n * 60 {
        ClockScenario::step(state, &[], TICK);
    }
}

fn living(state: &ClockState, role: Role) -> Vec<Tissue> {
    state
        .plants()
        .plants
        .observe()
        .tissues
        .into_iter()
        .filter(|t| t.role == role && t.health > 0)
        .collect()
}

fn light(state: &ClockState, tissue: &Tissue) -> u16 {
    state.plants().exposure[&tissue.id].light
}

/// The floor's light as drawn: each warm shape's span along x and its alpha.
fn floor_shapes(frame: &RenderFrame) -> Vec<(f32, f32, f32)> {
    frame
        .layers
        .iter()
        .filter(|layer| layer.z == render::ARENA_LAYER)
        .flat_map(|layer| &layer.primitives)
        .filter_map(|primitive| match primitive {
            RenderPrimitive::Polygon(polygon) => {
                let color = polygon.fill?.color;
                ((color.r, color.g, color.b) == (1.0, 0.76, 0.42)).then(|| {
                    let xs = polygon.points.iter().map(|p| p.x);
                    (
                        xs.clone().fold(f32::INFINITY, f32::min),
                        xs.fold(f32::NEG_INFINITY, f32::max),
                        color.a,
                    )
                })
            }
            _ => None,
        })
        .collect()
}

/// How opaque the floor's light is drawn on the floor's top at `x`.
fn floor_light_at(shapes: &[(f32, f32, f32)], x: f32) -> f32 {
    1.0 - shapes
        .iter()
        .filter(|&&(from, to, _)| from <= x && x <= to)
        .map(|&(_, _, alpha)| 1.0 - alpha)
        .product::<f32>()
}

#[test]
fn by_day_the_lamps_keep_the_grass_the_ceiling_planted_and_after_dusk_it_dies_while_the_moss_holds()
{
    let mut state = clock(4242, 12, 34);
    ClockScenario::step(
        &mut state,
        &[ClockAction::trigger_event(ClockEventKind::Grass)],
        Duration::ZERO,
    );
    seconds(&mut state, CLOSED_AT);
    assert_eq!(state.ceiling.phase, Phase::Closed);
    let ids = |shoots: &[Tissue]| {
        let mut ids: Vec<_> = shoots.iter().map(|t| t.id).collect();
        ids.sort();
        ids
    };
    let planted = living(&state, Role::Shoot);
    assert!(!planted.is_empty());

    // Without the lamps the grass has lost its shoots three minutes after
    // the ceiling closes (see `ceiling_light_contract`). By day they keep it
    // whole.
    seconds(&mut state, 240);
    let kept = living(&state, Role::Shoot);
    assert_eq!(ids(&kept), ids(&planted));
    for shoot in &kept {
        assert_eq!((shoot.health, shoot.shade_ticks), (100, 0), "{shoot:?}");
        assert!(light(&state, shoot) >= 500, "{shoot:?}");
    }
    assert_eq!(state.lamps_state().output_milli, 1000);

    // Half an hour after the lamps begin to dim, their glow leaves the grass
    // in shade. It dies within minutes and leaves the floor to the moss and
    // to the next day's Grass event.
    read(&mut state, 22, 30);
    assert_eq!(state.lamps_state().output_milli, 30);
    let (mut shoots_gone, mut retired) = (None, None);
    for second in 1..=900 {
        seconds(&mut state, 1);
        if shoots_gone.is_none() && living(&state, Role::Shoot).is_empty() {
            shoots_gone = Some(second);
        }
        if state.plants().grass_tissues() == 0 {
            retired = Some(second);
            break;
        }
    }
    let shoots_gone = shoots_gone.expect("the night ends the grass");
    let retired = retired.expect("nothing holds the capacity");
    println!(
        "after 22:30 the shoots are gone at {shoots_gone} s and the patch is retired at {retired} s"
    );
    assert!(shoots_gone <= 180, "{shoots_gone} s");
    let moss = living(&state, Role::Moss);
    assert!(!moss.is_empty());
    for mat in &moss {
        assert_eq!((mat.health, mat.shade_ticks), (100, 0), "{mat:?}");
        assert!(light(&state, mat) >= DIM_ROOM as u16, "{mat:?}");
    }
}

#[test]
fn the_face_shades_the_floor_and_a_patch_planted_in_its_shadow_gets_none_of_the_lamps_light() {
    let mut state = clock(4242, 12, 34);
    state.admit_soil();
    let layout = Layout::new(state.aspect_ratio());
    let (left, right) = Ceiling::corridors(layout)[0];
    let soil = state.soil().unwrap();
    let (lit_x, shaded_x) = ((left + right) * 0.5, 0.0);
    let organism = |state: &ClockState, seed: u64| {
        let observation = state.plants().plants.observe();
        observation
            .tissues
            .iter()
            .find(|t| t.id == seed)
            .unwrap()
            .organism
    };
    let lit_column = soil.column_at(lit_x);
    let shaded_column = soil.column_at(shaded_x);
    let lit = state.plant_grass(lit_column).unwrap();
    let lit = organism(&state, lit);
    let shaded = state.plant_grass(shaded_column).unwrap();
    let shaded = organism(&state, shaded);
    seconds(&mut state, 90);

    // The lamps' light lands beside the face and the face shades the floor
    // under it from both.
    let sample = |x: f32| {
        *state
            .floor_light
            .iter()
            .min_by(|a, b| {
                (a.position.x - x)
                    .abs()
                    .total_cmp(&(b.position.x - x).abs())
            })
            .unwrap()
    };
    assert!(sample(lit_x).light >= 500.0, "{:?}", sample(lit_x));
    assert_eq!(sample(shaded_x).light, 0.0, "{:?}", sample(shaded_x));

    // The grass is given what the floor shows: the patch beside the face
    // grows in the lamp's light, and the one under it gets none of it, and
    // no more than the room's, which the face above it may shade too.
    let shoots = living(&state, Role::Shoot);
    let of = |organism: u64| -> Vec<&Tissue> {
        shoots.iter().filter(|t| t.organism == organism).collect()
    };
    assert!(!of(lit).is_empty());
    for shoot in of(lit) {
        assert!(light(&state, shoot) >= 500, "{shoot:?}");
        assert_eq!(shoot.shade_ticks, 0, "{shoot:?}");
    }
    for shoot in of(shaded) {
        assert!(light(&state, shoot) <= DIM_ROOM as u16, "{shoot:?}");
        assert!(shoot.shade_ticks > 0, "{shoot:?}");
    }
    assert!(of(shaded).len() < of(lit).len());

    // The drawn floor is lit over the one and dark over the other.
    let shapes = floor_shapes(&ClockScenario::render_frame(&state));
    let day = floor_light_at(&shapes, lit_x);
    println!(
        "{} floor polygons by day, {day} opaque at the lit patch",
        shapes.len()
    );
    assert!(day >= 0.3, "{day}: {shapes:?}");
    assert_eq!(floor_light_at(&shapes, shaded_x), 0.0, "{shapes:?}");

    // At night the lamps still light the floor beside the face, with 3% of
    // their day's light, which is too little to draw.
    read(&mut state, 23, 0);
    for _ in 0..lamps::light::FLOOR_TICKS {
        ClockScenario::step(&mut state, &[], TICK);
    }
    let night = state
        .floor_light
        .iter()
        .min_by(|a, b| {
            (a.position.x - lit_x)
                .abs()
                .total_cmp(&(b.position.x - lit_x).abs())
        })
        .unwrap()
        .light;
    assert!(0.0 < night && night < 30.0, "{night}");
    let shapes = floor_shapes(&ClockScenario::render_frame(&state));
    assert_eq!(floor_light_at(&shapes, lit_x), 0.0, "{shapes:?}");
}

#[test]
fn a_bare_floor_keeps_its_usual_look() {
    let mut state = clock(4242, 12, 34);
    seconds(&mut state, 1);
    assert!(state.soil().is_none());
    assert!(state.floor_light.is_empty());
    assert!(floor_shapes(&ClockScenario::render_frame(&state)).is_empty());
}
