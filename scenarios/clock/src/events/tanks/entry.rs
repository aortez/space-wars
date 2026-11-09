use super::*;
use crate::{digit_surfaces::CELL_HALF, digits::DisplaySnapshot, fonts, meridiem::Glyph};

pub const BEAM_HALF_WIDTH: f32 = 2.1;

/// Project the actual lit cells onto x: a lane must pass every row of the face.
/// Reserve the colon even while it blinks off.
fn ink(layout: Layout, display: DisplaySnapshot) -> Vec<[f32; 2]> {
    let mut spans = Vec::new();
    let mut add = |x: f32, half: f32| spans.push([x - half, x + half]);
    for (slot, digit) in display.digits.into_iter().enumerate() {
        for cell in fonts::glyph(display.font, digit).cells() {
            add(
                layout.slot_cell_center(slot, cell).x,
                CELL_HALF * layout.pitch,
            );
        }
    }
    add(
        layout.face_origin.x + 14.5 * layout.pitch,
        CELL_HALF * layout.pitch,
    );
    if let Some(label) = display.meridiem {
        for glyph in Glyph::for_label(label) {
            for cell in glyph.cells() {
                add(
                    glyph.cell_center(layout, cell).x,
                    crate::meridiem::PIXEL_SIZE * layout.pitch * 0.5,
                );
            }
        }
    }
    spans
}

pub fn clear(layout: Layout, display: DisplaySnapshot, scale: f32, x: f32) -> bool {
    let half = BEAM_HALF_WIDTH * scale + layout.pitch * 0.05;
    ink(layout, display)
        .iter()
        .all(|span| x + half <= span[0] || x - half >= span[1])
}

fn excluded(world: Option<&PhysicsWorld>) -> Vec<PhysicsId> {
    world
        .into_iter()
        .flat_map(PhysicsWorld::entity_ids)
        .filter(|id| {
            canonical::ARENA_FLOOR.contains(&id.value()) || canonical::TANKS.contains(&id.value())
        })
        .collect()
}

pub fn lanes(
    layout: Layout,
    display: DisplaySnapshot,
    world: Option<&PhysicsWorld>,
    scale: f32,
) -> Option<[f32; 2]> {
    // Leave rounding room when the chosen world x travels through native units.
    let half = BEAM_HALF_WIDTH * scale + layout.pitch * 0.05 + 0.01;
    let left = layout.bounds_min.x + layout.frame_width + half;
    let right = layout.bounds_max.x - layout.frame_width - half;
    let mut blocked = ink(layout, display);
    blocked.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut free = Vec::new();
    let mut start = left;
    for [low, high] in blocked {
        if low - half > start {
            free.push([start, (low - half).min(right)]);
        }
        start = start.max(high + half);
    }
    if start < right {
        free.push([start, right]);
    }
    let excluded = excluded(world);
    let candidates: [Option<f32>; 2] = std::array::from_fn(|slot| {
        let direction = if slot == 0 { -1.0 } else { 1.0 };
        free.iter()
            .filter_map(|&[a, b]| {
                let low = if slot == 0 { a } else { a.max(6.0 * scale) };
                let high = if slot == 0 { b.min(-6.0 * scale) } else { b };
                if high < low {
                    return None;
                }
                let x = (direction * 27.0 * scale).clamp(low, high);
                let bottom = layout.floor_y + 0.5;
                let top = layout.bounds_max.y;
                if world.is_some_and(|w| {
                    !w.current_cuboid_is_clear(
                        Vec2::new(x, (bottom + top) * 0.5),
                        0.0,
                        Vec2::new(half, (top - bottom) * 0.5),
                        CollisionGroups::ALL,
                        &excluded,
                    )
                }) {
                    return None;
                }
                let outer = x < layout.face_origin.x || x > -layout.face_origin.x;
                Some((outer, (x - direction * 27.0 * scale).abs(), x))
            })
            .min_by(|(outer_a, distance_a, _), (outer_b, distance_b, _)| {
                outer_a
                    .cmp(outer_b)
                    .then_with(|| distance_a.total_cmp(distance_b))
            })
            .map(|(_, _, x)| x)
    });
    Some([candidates[0]?, candidates[1]?])
}

/// Full-width terrain, with two supported, unobstructed transport lanes.
pub fn placement(
    layout: Layout,
    display: DisplaySnapshot,
    world: Option<&PhysicsWorld>,
) -> Option<Placement> {
    let width = layout.bounds_max.x - layout.bounds_min.x - 2.0 * layout.frame_width;
    let height = layout.face_origin.y - layout.floor_y - 8.0;
    let fit = (width / 108.0).min(height / 24.0).min(5.0);
    [fit, fit * 0.8]
        .into_iter()
        .find_map(|scale| fit_placement(layout, display, world, scale))
}

fn fit_placement(
    layout: Layout,
    display: DisplaySnapshot,
    world: Option<&PhysicsWorld>,
    scale: f32,
) -> Option<Placement> {
    let width = layout.bounds_max.x - layout.bounds_min.x - 2.0 * layout.frame_width;
    if scale < 0.75 {
        return None;
    }
    let terrain_width = (width / (0.5 * scale)).ceil() as u32 + 2;
    if terrain_width > scenario_scorched_earth::host::MAX_TERRAIN_WIDTH {
        return None;
    }
    let low = Vec2::new(-width * 0.5, layout.floor_y + 0.5);
    let high = Vec2::new(width * 0.5, layout.floor_y + 16.0 * scale);
    if world.is_some_and(|w| {
        !w.current_cuboid_is_clear(
            (low + high) * 0.5,
            0.0,
            (high - low) * 0.5,
            CollisionGroups::ALL,
            &excluded(world),
        )
    }) {
        return None;
    }
    let spawn_x = lanes(layout, display, world, scale)?.map(|x| x / scale);
    let mut place = Placement::new(canonical::TANKS, BUDGET);
    place.scale = scale;
    place.origin = Vec2::new(0.0, layout.floor_y + 12.0 * scale);
    place.terrain_width = terrain_width;
    place.spawn_x = spawn_x;
    place.reserved_grains = 2 * DIRT_PER_TANK;
    let edge = terrain_width as f32 * 0.25 + 5.0;
    place.bounds = Bounds {
        min: Vec2::new(-edge, -40.0),
        max: Vec2::new(edge, 65.0),
    };
    Some(place)
}
