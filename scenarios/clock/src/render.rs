use engine_common::{
    Camera2, Fill, RenderColor, RenderFrame, RenderPoint, RenderPolygon, RenderPrimitive,
    RenderText, Stroke, TextAnchor,
};

use crate::{
    ClockState, DigitPalette, REFORMING_TICKS, SegmentRepresentation, digits,
    layout::{CAMERA_HEIGHT, Layout},
};
use engine_core::Vec2;

mod aquarium;
pub(crate) mod ceiling;
mod cell_swap;
mod crow;
mod digit_slide;
mod duck;
mod explosion;
#[cfg(test)]
mod face_handover_contract;
mod floor;
mod frame;
mod glow;
pub(crate) mod grass_inspection;
mod marquee;
mod meltdown;
mod meridiem;
mod rain;

const BACKGROUND_LAYER: i32 = 0;
const ARENA_LAYER: i32 = 1;
pub(crate) const GLOW_LAYER: i32 = 2;
const INACTIVE_CELL_LAYER: i32 = 3;
pub(crate) const ACTIVE_CELL_LAYER: i32 = 4;
pub(crate) const LABEL_LAYER: i32 = 5;

const BACKGROUND_COLOR: RenderColor = RenderColor::rgb(0.018, 0.025, 0.055);
const FLOOR_COLOR: RenderColor = RenderColor::rgb(0.22, 0.14, 0.078);
const FLOOR_EDGE_COLOR: RenderColor = RenderColor::rgb(0.32, 0.21, 0.12);
/// Damp earth cut into the floor's own top rows. Darker and greyer than the
/// surrounding wood, which the old tone was too close to for the layer to read.
pub(crate) const SOIL_COLOR: RenderColor = RenderColor::rgb(0.155, 0.112, 0.082);
/// Cover tones from withered to sunlit, indexed by band. Distinct from the
/// soil and shoot palettes so a frame reader can tell the three apart.
pub(crate) const COVER_COLORS: [RenderColor; 3] = [
    RenderColor::rgb(0.38, 0.33, 0.14),
    RenderColor::rgb(0.33, 0.45, 0.14),
    RenderColor::rgb(0.20, 0.52, 0.15),
];
/// Decorative cover withers to bare soil over this long away from an opening.
const COVER_WITHER_SECONDS: f32 = 45.0;
/// Columns per cover strip, which bounds the primitives one floor can add.
const COVER_RUN_COLUMNS: usize = 24;
const INACTIVE_CELL_COLOR: RenderColor = RenderColor::rgb(0.032, 0.068, 0.083);
const LABEL_COLOR: RenderColor = RenderColor::rgb(0.52, 0.72, 0.77);

const COLON_X_UNITS: f32 = 14.5;
const COLON_Y_UNITS: [f32; 2] = [2.25, 5.75];

pub(crate) fn water_fixture(frame: &mut RenderFrame, water: &engine_water::WaterWorld) {
    meltdown::render_water(frame, water, ACTIVE_CELL_LAYER, 1.0);
}

pub(crate) fn water_fixture_cell(frame: &mut RenderFrame, center: Vec2, pitch: f32, lit: bool) {
    render_square(
        frame,
        center,
        pitch,
        0.0,
        f32::from(lit),
        DigitPalette::default(),
    );
}

pub fn render_frame(state: &ClockState) -> RenderFrame {
    let layout = Layout::new(state.aspect_ratio());
    let mut frame = RenderFrame::new(Camera2::new(RenderPoint::ZERO, CAMERA_HEIGHT));
    if let Some((message, _)) = state.event_notice {
        frame.push_primitive(
            100,
            RenderPrimitive::Text(RenderText {
                position: RenderPoint::new(0.0, layout.bounds_max.y - 24.0),
                text: message.into(),
                color: LABEL_COLOR,
                // RenderText sizes are logical pixels on both render paths.
                size: 18.0,
                anchor: TextAnchor::Center,
            }),
        );
    } else if state.config.show_date
        && state.player_duck_session().is_none()
        && let Some(label) = state.date_label()
    {
        frame.push_primitive(
            100,
            RenderPrimitive::Text(RenderText {
                position: RenderPoint::new(0.0, (layout.bounds_max.y + layout.canopy_y) * 0.5),
                text: label.into(),
                color: LABEL_COLOR,
                size: 18.0,
                anchor: TextAnchor::Center,
            }),
        );
    }
    frame.push_primitive(
        BACKGROUND_LAYER,
        rectangle(layout.bounds_min, layout.bounds_max, BACKGROUND_COLOR, None),
    );
    frame::render(&mut frame, state, layout);
    render_canopy(&mut frame, layout);
    ceiling::render(&mut frame, state, layout);
    if let Some(crate::events::ActiveEvent::Aquarium(event)) = &state.active_event {
        aquarium::water(&mut frame, event);
        floor::responsive(&mut frame, &event.floor, layout, 1.0);
    }
    if let Some(event) = shared_mechanics_arena(state) {
        let opacity = if state.duck_visit.is_some() {
            event.arena_opacity()
        } else {
            state.active_event.as_ref().unwrap().arena_opacity()
        };
        if let Some(panels) = event.responsive_floor() {
            floor::responsive(&mut frame, panels, layout, opacity);
        } else {
            render_floor(
                &mut frame,
                crate::floor::FloorGeometry::closed(layout),
                layout.pitch,
                1.0 - opacity,
            );
            duck::render_arena(&mut frame, event, opacity);
        }
    } else if let Some(crate::events::ActiveEvent::Rain(event)) = &state.active_event
        && let Some(course) = event.course()
    {
        // A shower owns a course claim, not a second responsive floor. Keep it
        // opaque while a visiting duck is present, including Rain's clearing phase.
        let opacity = if state.duck_visit.is_some() {
            1.0
        } else {
            event.opacity()
        };
        render_floor(
            &mut frame,
            crate::floor::FloorGeometry::closed(layout),
            layout.pitch,
            1.0 - opacity,
        );
        duck::shared_course(&mut frame, course, opacity);
    } else if let Some(crate::events::ActiveEvent::Rain(event)) = &state.active_event {
        floor::responsive(
            &mut frame,
            event.responsive_floor().expect("responsive rain"),
            layout,
            if state.duck_visit.is_some() {
                1.0
            } else {
                event.opacity()
            },
        );
    } else if let Some(duck) = &state.duck_visit
        && let Some(panels) = duck.responsive_floor()
    {
        floor::responsive(&mut frame, panels, layout, duck.arena_opacity());
    } else if let Some(event) = state.duck_scene() {
        // The custom course owns its floor, including the entrance/exit fade.
        // This backdrop never adds a collider across the course's physical pit.
        render_floor(
            &mut frame,
            crate::floor::FloorGeometry::closed(layout),
            layout.pitch,
            1.0 - event.arena_opacity(),
        );
    } else if let Some(crate::events::ActiveEvent::Meltdown(event)) = &state.active_event {
        if let Some(floor) = &event.floor {
            floor::responsive(&mut frame, floor, layout, event.floor_opacity());
        }
    } else {
        render_floor(&mut frame, state.floor.geometry(layout), layout.pitch, 1.0);
    }
    if let Some(soil) = &state.soil {
        for quad in soil.quads() {
            frame.push_primitive(
                ARENA_LAYER,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    quad.map(|p| RenderPoint::new(p.x, p.y)).to_vec(),
                    SOIL_COLOR,
                )),
            );
        }
        ground_cover(&mut frame, state, soil);
        let observation = state.plants.plants.observe();
        // Mats draw first: they lie on the floor line that the shoots rise from.
        for tissue in observation
            .tissues
            .iter()
            .filter(|t| t.role == engine_core::plants::Role::Moss)
        {
            if let Some(points) = crate::grass::residents::moss_polygon(tissue, soil) {
                let points = points
                    .into_iter()
                    .map(|p| RenderPoint::new(p.x, p.y))
                    .collect();
                frame.push_primitive(
                    ARENA_LAYER,
                    RenderPrimitive::Polygon(RenderPolygon::filled(
                        points,
                        if tissue.health == 0 {
                            RenderColor::rgb(0.28, 0.24, 0.16)
                        } else if tissue.shade_ticks > 0 || tissue.shortage_ticks > 0 {
                            let health = f32::from(tissue.health) / 100.0;
                            RenderColor::rgb(0.38 - 0.22 * health, 0.3 + 0.12 * health, 0.17)
                        } else {
                            // Deeper and duller than a blade, so a mat never
                            // reads as grass at a glance.
                            RenderColor::rgb(0.16, 0.42, 0.24)
                        },
                    )),
                );
            }
        }
        for tissue in observation
            .tissues
            .iter()
            .filter(|t| t.role == engine_core::plants::Role::Shoot)
        {
            if let Some(points) = crate::grass::residents::shoot_polygon(tissue, &observation, soil)
            {
                let points = points
                    .into_iter()
                    .map(|p| RenderPoint::new(p.x, p.y))
                    .collect();
                frame.push_primitive(
                    ARENA_LAYER,
                    RenderPrimitive::Polygon(RenderPolygon::filled(
                        points,
                        if tissue.health == 0 {
                            RenderColor::rgb(0.35, 0.28, 0.12)
                        } else if tissue.shade_ticks > 0 || tissue.shortage_ticks > 0 {
                            let health = f32::from(tissue.health) / 100.0;
                            RenderColor::rgb(0.45 - 0.23 * health, 0.32 + 0.33 * health, 0.12)
                        } else {
                            RenderColor::rgb(0.22, 0.65, 0.16)
                        },
                    )),
                );
            }
        }
    }
    if let Some(view) = crate::grass::inspection::snapshot(state) {
        grass_inspection::render(&mut frame, &view, layout);
    }
    if let Some(crate::events::ActiveEvent::Marquee(event)) = &state.active_event {
        render_marquee_face(&mut frame, state, layout, event);
        marquee::render(&mut frame, event, layout);
        render_player_and_course(&mut frame, state, layout);
        return frame;
    }
    render_segments(&mut frame, state, layout);
    if let Some(crate::events::ActiveEvent::Rain(event)) = &state.active_event {
        rain::render(&mut frame, event);
    }
    if let Some(crate::events::ActiveEvent::Meltdown(event)) = &state.active_event {
        meltdown::render(&mut frame, event, layout);
    }
    render_colon(&mut frame, state, layout);
    render_meridiem(&mut frame, state, layout);
    if let Some(crate::events::ActiveEvent::Aquarium(event)) = &state.active_event {
        aquarium::residents(&mut frame, event);
    }
    render_player_and_course(&mut frame, state, layout);
    frame
}

/// Filled visible cell geometry, excluding glow shells and UI labels. Shared
/// renderers supply effect transforms, clipping, reformation and fade state.
/// Any positive fill opacity is opaque in the first overhead-light model.
/// Decorative cover on the delivered layer: no tissue, no resources, no
/// physics and no persisted state. The whole floor greens when the material
/// arrives and recedes to the openings as unlit soil withers, so a hole reads
/// as the thing keeping its own patch alive. Simulated grass draws over it.
fn ground_cover(frame: &mut RenderFrame, state: &ClockState, soil: &crate::grass::SoilHabitat) {
    use crate::grass::{CLOCK_DIRT, SOIL_CELL_SIZE, SOIL_DEPTH_CELLS};
    let Some(age) = state.ceiling.cover_age() else {
        return;
    };
    let unlit = (1.0 - age.as_secs_f32() / COVER_WITHER_SECONDS).max(0.0);
    let sunlit = state.ceiling.fraction();
    let windows = state.plants.lit_columns.as_deref().unwrap_or_default();
    let surface_row = SOIL_DEPTH_CELLS as i32 - 1;
    // Three bands rather than a gradient, so a run of columns shares one strip.
    let band = |vitality: f32| (vitality * 3.0).ceil().clamp(0.0, 3.0) as u8;
    let mut run: Vec<(Vec2, f32)> = Vec::new();
    let mut run_band = 0;
    let mut flush = |run: &mut Vec<(Vec2, f32)>, band: u8| {
        if run.len() < 2 || band == 0 {
            run.clear();
            return;
        }
        let vitality = f32::from(band) / 3.0;
        let height = SOIL_CELL_SIZE * (0.45 + 0.95 * vitality);
        let mut points: Vec<RenderPoint> = Vec::with_capacity(run.len() * 2 + 2);
        for (index, &(base, angle)) in run.iter().enumerate() {
            // A blade tip and a notch per column give the strip a cut edge
            // instead of a painted line, without a primitive per blade.
            let tall = if index % 2 == 0 {
                height
            } else {
                height * 0.62
            };
            for offset in [
                Vec2::new(-SOIL_CELL_SIZE * 0.25, tall),
                Vec2::new(SOIL_CELL_SIZE * 0.25, tall * 0.35),
            ] {
                let p = base + offset.rotate_radians(angle);
                points.push(RenderPoint::new(p.x, p.y));
            }
        }
        for &(base, angle) in run.iter().rev() {
            let p = base + Vec2::new(0.0, -SOIL_CELL_SIZE * 0.1).rotate_radians(angle);
            points.push(RenderPoint::new(p.x, p.y));
        }
        frame.push_primitive(
            ARENA_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                points,
                COVER_COLORS[usize::from(band - 1)],
            )),
        );
        run.clear();
    };
    for column in 0..soil.terrain().width() as i32 {
        let dirt = soil
            .terrain()
            .cell(engine_terrain::CellCoord::new(column, surface_row))
            .is_some_and(|c| c.material == CLOCK_DIRT);
        let lit = windows
            .iter()
            .any(|&(low, high)| (low..=high).contains(&column));
        let current = band(if lit { sunlit } else { unlit });
        let seat = dirt
            .then(|| {
                Some((
                    soil.world_point([column, surface_row])?,
                    soil.surface_at(column)?.angle,
                ))
            })
            .flatten();
        match seat {
            Some(seat) if current == run_band && run.len() < COVER_RUN_COLUMNS => run.push(seat),
            Some(seat) => {
                flush(&mut run, run_band);
                run_band = current;
                run.push(seat);
            }
            // A gap in the material also ends the strip, so no cover bridges it.
            None => {
                flush(&mut run, run_band);
                run_band = 0;
            }
        }
    }
    flush(&mut run, run_band);
}

/// Draws the anchored reading beneath a marquee, carried by whatever sweep is in
/// progress. A clock preset animates the face's own cells through the recipe, so
/// there is no second copy of the reading to lay underneath and crossfade
/// against. A text takeover brings its own content, so the lit cells travel
/// clear of the frame and return the same way instead of dissolving where they
/// stand. The visible frame and the plant lighting occluders have to agree about
/// where those cells are, so both go through here.
fn render_marquee_face(
    frame: &mut RenderFrame,
    state: &ClockState,
    layout: Layout,
    event: &crate::events::marquee::MarqueeEvent,
) {
    let before: Vec<usize> = frame.layers.iter().map(|l| l.primitives.len()).collect();
    render_segments(frame, state, layout);
    render_colon(frame, state, layout);
    render_meridiem(frame, state, layout);
    // Hand over only the lit cells these three calls just drew. The dim anchors
    // keep their places as the substrate, just as a fall or an explosion leaves
    // them, and the arena, the floor and a visiting duck are never carried
    // along. No offscreen image or extra frame allocation.
    let sweep = event.face_sweep();
    let clearance = layout.bounds_max.x - layout.bounds_min.x + layout.pitch;
    let opacity = 1.0 - event.playback().strength;
    let gone = sweep.map_or(opacity <= 0.0, |sweep| sweep.abs() >= 1.0);
    for (index, layer) in frame.layers.iter_mut().enumerate() {
        if layer.z == INACTIVE_CELL_LAYER {
            continue;
        }
        let start = before.get(index).copied().unwrap_or(0);
        if gone {
            // The reading is clear of the frame, or the recipe has taken it over
            // outright. Either way the face has nothing lit left to contribute.
            layer.primitives.truncate(start);
            continue;
        }
        for primitive in &mut layer.primitives[start..] {
            let RenderPrimitive::Polygon(polygon) = primitive else {
                continue;
            };
            match sweep {
                // A text takeover carries the reading out the way the message
                // travels, at full brightness for the whole journey.
                Some(sweep) => {
                    for point in &mut polygon.points {
                        point.x += sweep * clearance;
                    }
                }
                // A clock recipe animates these same cells and opens on the
                // face's own pose, so the two are one reading dissolving into
                // itself rather than into a differently placed copy of itself.
                None => {
                    if let Some(fill) = &mut polygon.fill {
                        fill.color.a *= opacity;
                    }
                    if let Some(stroke) = &mut polygon.stroke {
                        stroke.color.a *= opacity;
                    }
                }
            }
        }
    }
}

pub(crate) fn plant_face_polygons(state: &ClockState) -> Vec<Vec<Vec2>> {
    let layout = Layout::new(state.aspect_ratio());
    let mut frame = RenderFrame::default();
    if let Some(crate::events::ActiveEvent::Marquee(event)) = &state.active_event {
        render_marquee_face(&mut frame, state, layout, event);
        marquee::render(&mut frame, event, layout);
    } else {
        render_segments(&mut frame, state, layout);
        render_colon(&mut frame, state, layout);
        render_meridiem(&mut frame, state, layout);
    }
    filled_polygons(frame, ACTIVE_CELL_LAYER)
}

pub(crate) fn plant_actor_polygons(state: &ClockState) -> Vec<Vec<Vec2>> {
    let mut frame = RenderFrame::default();
    if let Some(visit) = &state.crow_visit {
        crow::render(&mut frame, visit, Layout::new(state.aspect_ratio()));
    }
    let mut polygons = filled_polygons(frame, 8);
    if let Some(crate::events::ActiveEvent::Aquarium(event)) = &state.active_event {
        let mut fish = RenderFrame::default();
        aquarium::residents(&mut fish, event);
        polygons.extend(filled_polygons(fish, 14));
    }
    polygons
}

fn filled_polygons(frame: RenderFrame, z: i32) -> Vec<Vec<Vec2>> {
    frame
        .layers
        .into_iter()
        .filter(|layer| layer.z == z)
        .flat_map(|layer| layer.primitives)
        .filter_map(|primitive| {
            let RenderPrimitive::Polygon(p) = primitive else {
                return None;
            };
            p.fill.filter(|f| f.color.a > 0.0)?;
            Some(p.points.into_iter().map(|p| Vec2::new(p.x, p.y)).collect())
        })
        .collect()
}

fn render_player_and_course(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    if let Some(visit) = &state.crow_visit {
        crow::render(frame, visit, layout);
    }
    // Kept outside the marquee's face fade and early return. The duck/course
    // retain their own opacity and geometry while clock content transforms.
    if let Some(event) = state.duck_scene() {
        let shared = matches!(&state.active_event, Some(crate::events::ActiveEvent::Rain(rain)) if rain.course().is_some());
        if shared || shared_mechanics_arena(state).is_some() || event.responsive_floor().is_some() {
            duck::render_with_course(frame, event, state.config.duck_debug_overlay, false);
        } else {
            duck::render(frame, event, state.config.duck_debug_overlay);
        }
    }
    if let Some((_, player)) = state.player_duck_session() {
        frame.push_primitive(
            100,
            RenderPrimitive::Text(RenderText {
                position: RenderPoint::new(layout.bounds_max.x - 60.0, layout.bounds_max.y - 24.0),
                text: format!("P{player} DUCK"),
                color: LABEL_COLOR,
                size: 14.0,
                anchor: TextAnchor::Center,
            }),
        );
    }
}

fn shared_mechanics_arena(state: &ClockState) -> Option<&crate::events::duck::DuckEvent> {
    let event = state.active_event.as_ref()?;
    event
        .shares_visit_arena()
        .then(|| state.duck_visit.as_deref().or(event.vacant_arena()))
        .flatten()
}

fn render_floor(
    frame: &mut RenderFrame,
    floor: crate::floor::FloorGeometry,
    pitch: f32,
    opacity: f32,
) {
    if opacity <= 0.0 {
        return;
    }
    for (min, max) in floor.slabs() {
        floor::slab(frame, min, max, pitch, opacity);
    }
}

fn render_canopy(frame: &mut RenderFrame, layout: Layout) {
    // Background framing only: no ceiling body or extra water support. Rain
    // begins below this band, while event notices remain readable over it.
    frame.push_primitive(
        ARENA_LAYER,
        rectangle(
            RenderPoint::new(layout.bounds_min.x, layout.canopy_y),
            layout.bounds_max,
            RenderColor::rgb(0.063, 0.053, 0.041),
            None,
        ),
    );
    frame.push_primitive(
        ARENA_LAYER,
        rectangle(
            RenderPoint::new(layout.bounds_min.x, layout.canopy_y),
            RenderPoint::new(
                layout.bounds_max.x,
                layout.canopy_y + (layout.pitch * 0.10).clamp(1.5, 3.0),
            ),
            RenderColor::rgb(0.17, 0.13, 0.085),
            None,
        ),
    );
}

fn render_segments(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    if let Some(crate::events::ActiveEvent::CellSwap(event)) = &state.active_event {
        cell_swap::render(frame, event, layout, state.palette());
        return;
    }
    if let Some(crate::events::ActiveEvent::Explosion(event)) = &state.active_event {
        explosion::render(frame, state, event, layout);
        return;
    }
    if let Some(crate::events::ActiveEvent::DigitSlide(event)) = &state.active_event {
        digit_slide::render(frame, event, layout, state.palette());
        return;
    }
    if matches!(&state.active_event, Some(crate::events::ActiveEvent::Meltdown(event))
        if !event.lab && event.phase() == crate::events::EventPhase::Reforming)
    {
        meltdown::render_reforming_face(frame, state, layout);
        return;
    }
    let palette = state.palette();
    let t = (state.phase_tick() as f32 / REFORMING_TICKS as f32).clamp(0.0, 1.0);
    let progress = t * t * (3.0 - 2.0 * t);
    for segment in state.segments() {
        let anchor = layout.segment_center(segment.id);
        for cell in segment.guides() {
            let brightness = if segment.representation == SegmentRepresentation::Anchored
                && segment.lit
                && segment.shape.contains(cell)
            {
                1.0
            } else {
                0.0
            };
            render_square(
                frame,
                layout.cell_center(segment.id, cell),
                layout.pitch,
                0.0,
                brightness,
                palette,
            );
        }
        let (position, angle, was_lit, mix) = match segment.representation {
            SegmentRepresentation::Anchored | SegmentRepresentation::Disintegrated => continue,
            SegmentRepresentation::Rigid { position, angle } => (position, angle, true, 0.0),
            SegmentRepresentation::Reforming(reformation) => (
                reformation.position() + (anchor - reformation.position()) * progress,
                reformation.angle() * (1.0 - progress),
                reformation.was_lit(),
                progress,
            ),
        };
        let previous = if matches!(segment.representation, SegmentRepresentation::Reforming(_)) {
            segment.previous_shape
        } else {
            segment.shape
        };
        for cell in crate::fonts::CellMask(previous.0 | segment.shape.0).cells() {
            let brightness = f32::from(was_lit && previous.contains(cell)) * (1.0 - mix)
                + f32::from(segment.lit && segment.shape.contains(cell)) * mix;
            if brightness > 0.0 {
                let center = layout.cell_center(segment.id, cell);
                render_square(
                    frame,
                    position + (center - anchor).rotate_radians(angle),
                    layout.pitch,
                    angle,
                    brightness,
                    palette,
                );
            }
        }
    }
}

fn render_colon(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    for y in COLON_Y_UNITS {
        render_cell(
            frame,
            RenderPoint::new(
                layout.face_origin.x + COLON_X_UNITS * layout.pitch,
                layout.face_origin.y + y * layout.pitch,
            ),
            layout.pitch,
            state.display().colon_lit,
            state.palette(),
        );
    }
}

fn render_meridiem(frame: &mut RenderFrame, state: &ClockState, layout: Layout) {
    meridiem::render(frame, state, layout);
}

fn render_cell(
    frame: &mut RenderFrame,
    lower_left: RenderPoint,
    pitch: f32,
    lit: bool,
    palette: DigitPalette,
) {
    render_square(
        frame,
        Vec2::new(lower_left.x + pitch * 0.5, lower_left.y + pitch * 0.5),
        pitch,
        0.0,
        f32::from(lit),
        palette,
    );
}

fn render_square(
    frame: &mut RenderFrame,
    center: Vec2,
    pitch: f32,
    angle: f32,
    brightness: f32,
    palette: DigitPalette,
) {
    let layer = if brightness > 0.0 {
        ACTIVE_CELL_LAYER
    } else {
        INACTIVE_CELL_LAYER
    };
    let color = if brightness > 0.0 {
        RenderColor {
            a: brightness,
            ..palette.fill
        }
    } else {
        INACTIVE_CELL_COLOR
    };
    let stroke = (brightness > 0.0).then(|| {
        Stroke::new(
            RenderColor {
                a: brightness,
                ..palette.edge
            },
            (pitch * 0.045).max(0.8),
        )
    });
    let half = pitch * 0.4;
    let points = [
        Vec2::new(-half, -half),
        Vec2::new(half, -half),
        Vec2::new(half, half),
        Vec2::new(-half, half),
    ]
    .map(|offset| {
        let point = center + offset.rotate_radians(angle);
        RenderPoint::new(point.x, point.y)
    });
    if brightness > 0.0 {
        glow::quad(frame, points, color, None);
    }
    frame.push_primitive(
        layer,
        RenderPrimitive::Polygon(RenderPolygon {
            points: points.to_vec(),
            fill: Some(Fill::new(color)),
            stroke,
        }),
    );
}

fn rectangle(
    min: RenderPoint,
    max: RenderPoint,
    color: RenderColor,
    stroke: Option<Stroke>,
) -> RenderPrimitive {
    RenderPrimitive::Polygon(RenderPolygon {
        points: vec![
            min,
            RenderPoint::new(max.x, min.y),
            max,
            RenderPoint::new(min.x, max.y),
        ],
        fill: Some(Fill::new(color)),
        stroke,
    })
}

#[cfg(test)]
mod tests {
    use engine_common::{ClockTimeFormat, RenderPrimitive, Scenario};

    use super::*;
    use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario};

    fn rendered_at(aspect_ratio: f32) -> RenderFrame {
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio,
                time_format: ClockTimeFormat::TwentyFourHour,
                ..ClockConfig::default()
            },
            7,
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(
                ClockReading::new(23, 58, 2).unwrap(),
            )],
            std::time::Duration::ZERO,
        );
        ClockScenario::render_frame(&state)
    }

    #[test]
    fn cells_remain_inside_landscape_and_portrait_cameras() {
        for aspect_ratio in [5.0 / 3.0, 16.0 / 9.0, 3.0 / 4.0] {
            let frame = rendered_at(aspect_ratio);
            let bounds = frame.camera.world_bounds(aspect_ratio);
            for polygon in frame.layers.iter().flat_map(|layer| &layer.primitives) {
                let RenderPrimitive::Polygon(polygon) = polygon else {
                    continue;
                };
                for point in &polygon.points {
                    assert!(point.x >= bounds.min.x - f32::EPSILON);
                    assert!(point.x <= bounds.max.x + f32::EPSILON);
                    assert!(point.y >= bounds.min.y - f32::EPSILON);
                    assert!(point.y <= bounds.max.y + f32::EPSILON);
                }
            }
        }
    }

    #[test]
    fn normal_face_has_a_small_bounded_primitive_count() {
        let frame = rendered_at(5.0 / 3.0);
        let primitive_count = frame
            .layers
            .iter()
            .map(|layer| layer.primitives.len())
            .sum::<usize>();
        // The face, wooden surround and lighting remain a bounded draw list.
        assert!(primitive_count < 600, "{primitive_count} primitives");
    }

    #[test]
    fn twelve_hour_meridiem_is_visible_to_both_render_backends() {
        for hour in [0, 12] {
            let mut state = ClockScenario::init(
                ClockConfig {
                    time_format: ClockTimeFormat::TwelveHour,
                    ..ClockConfig::default()
                },
                0,
            );
            ClockScenario::step(
                &mut state,
                &[ClockAction::set_reading(
                    ClockReading::new(hour, 0, 0).unwrap(),
                )],
                std::time::Duration::ZERO,
            );
            let frame = ClockScenario::render_frame(&state);
            let label = frame
                .layers
                .iter()
                .find(|layer| layer.z == LABEL_LAYER)
                .unwrap();
            assert!(!label.primitives.is_empty());
            assert!(
                label
                    .primitives
                    .iter()
                    .all(|primitive| matches!(primitive, RenderPrimitive::Polygon(_)))
            );
        }
    }
}
