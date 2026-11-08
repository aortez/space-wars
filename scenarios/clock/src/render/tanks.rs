use super::*;
use crate::{
    events::{
        EventPhase,
        tanks::{BEAM_DOWN_TICKS, BEAM_UP_TICKS, TanksEvent},
    },
    presentation::Bounds,
};
use engine_rapier::world::PhysicsWorld;
use scenario_scorched_earth::RIGID_TANK_HULL;

const EARTH: RenderColor = RenderColor::rgb(0.36, 0.27, 0.16);
const GRAIN: RenderColor = RenderColor::rgb(0.69, 0.48, 0.25);
const TEAMS: [RenderColor; 2] = [
    RenderColor::rgb(0.35, 0.85, 0.68),
    RenderColor::rgb(0.98, 0.51, 0.27),
];

// Combat stays below the face; transport uses a clear lane through it.
fn polygon(frame: &mut RenderFrame, mut points: Vec<Vec2>, bounds: Bounds, color: RenderColor) {
    for (axis, edge, greater) in [
        (0, bounds.min.x, true),
        (0, bounds.max.x, false),
        (1, bounds.min.y, true),
        (1, bounds.max.y, false),
    ] {
        if points.is_empty() {
            return;
        }
        let coordinate = |p: Vec2| if axis == 0 { p.x } else { p.y };
        let inside = |p| {
            if greater {
                coordinate(p) >= edge
            } else {
                coordinate(p) <= edge
            }
        };
        let mut out = Vec::with_capacity(points.len() + 1);
        let mut previous = points[points.len() - 1];
        for point in points {
            if inside(previous) != inside(point) {
                let t = (edge - coordinate(previous)) / (coordinate(point) - coordinate(previous));
                out.push(previous + (point - previous) * t);
            }
            if inside(point) {
                out.push(point);
            }
            previous = point;
        }
        points = out;
    }
    if points.len() >= 3 {
        frame.push_primitive(
            ARENA_LAYER,
            RenderPrimitive::Polygon(RenderPolygon::filled(
                points
                    .into_iter()
                    .map(|p| RenderPoint::new(p.x, p.y))
                    .collect(),
                color,
            )),
        );
    }
}

fn disc(frame: &mut RenderFrame, center: Vec2, radius: f32, bounds: Bounds, color: RenderColor) {
    polygon(
        frame,
        (0..16)
            .map(|i| center + Vec2::from_radians(i as f32 * std::f32::consts::TAU / 16.0) * radius)
            .collect(),
        bounds,
        color,
    );
}

fn strip(
    frame: &mut RenderFrame,
    a: Vec2,
    b: Vec2,
    width: f32,
    bounds: Bounds,
    color: RenderColor,
) {
    let direction = (b - a).normalized();
    let side = Vec2::new(-direction.y, direction.x) * width * 0.5;
    polygon(
        frame,
        vec![a - side, b - side, b + side, a + side],
        bounds,
        color,
    );
}

pub(super) fn render(
    frame: &mut RenderFrame,
    event: &TanksEvent,
    world: &PhysicsWorld,
    layout: Layout,
) {
    let bounds = Bounds {
        min: Vec2::new(layout.bounds_min.x + layout.frame_width, layout.floor_y),
        max: Vec2::new(
            layout.bounds_max.x - layout.frame_width,
            layout.face_origin.y - 4.0,
        ),
    };
    let scale = event.duel.placement().scale;
    for field in event.duel.terrain() {
        let motion = world.motion(field.assembly.body()).expect("owned terrain");
        let transform = |p: Vec2| motion.position + p.rotate_radians(motion.angle);
        for chunk in field.geometry.chunks() {
            for rect in &chunk.rectangles {
                let center = rect.local_center(&field.terrain);
                let half = rect.half_extents(&field.terrain);
                polygon(
                    frame,
                    [
                        Vec2::new(-half.x, -half.y),
                        Vec2::new(half.x, -half.y),
                        half,
                        Vec2::new(-half.x, half.y),
                    ]
                    .map(|p| transform(center + p))
                    .to_vec(),
                    bounds,
                    EARTH,
                );
            }
            for patch in &chunk.polygons {
                polygon(
                    frame,
                    patch.vertices.iter().map(|p| transform(*p)).collect(),
                    bounds,
                    EARTH,
                );
            }
        }
    }
    for grain in event.duel.grains().chain(event.converted.iter()) {
        let motion = world.motion(grain.body()).expect("owned grain");
        if let Some(vertices) = grain.shape().vertices(grain.radius()) {
            polygon(
                frame,
                vertices
                    .into_iter()
                    .map(|p| motion.position + p.rotate_radians(motion.angle))
                    .collect(),
                bounds,
                GRAIN,
            );
        } else {
            disc(frame, motion.position, grain.radius(), bounds, GRAIN);
        }
    }
    if event.phase != EventPhase::Raising {
        let tank_bounds = if event.phase == EventPhase::BeamDown {
            Bounds {
                max: Vec2::new(bounds.max.x, layout.bounds_max.y - layout.frame_width),
                ..bounds
            }
        } else {
            bounds
        };
        for (slot, tank) in event
            .duel
            .tanks(world)
            .into_iter()
            .enumerate()
            .filter(|(_, tank)| tank.present)
        {
            let mut color = if tank.health > 0.0 {
                TEAMS[slot]
            } else {
                RenderColor::rgb(0.28, 0.28, 0.25)
            };
            color.a = match event.phase {
                EventPhase::BeamDown => (event.phase_tick as f32 / BEAM_DOWN_TICKS as f32).max(0.1),
                EventPhase::BeamUp => 1.0 - event.phase_tick as f32 / BEAM_UP_TICKS as f32,
                _ => 1.0,
            };
            if matches!(event.phase, EventPhase::BeamDown | EventPhase::BeamUp) {
                let mut beam = TEAMS[slot];
                beam.a = 0.14 + 0.06 * (event.phase_tick as f32 * 0.4).sin().abs();
                let width = 2.1 * scale;
                let x = tank.motion.position.x;
                let bottom = if event.phase == EventPhase::BeamDown {
                    event.landing(slot).y
                } else {
                    tank.motion.position.y
                };
                polygon(
                    frame,
                    vec![
                        Vec2::new(x - width, bottom),
                        Vec2::new(x + width, bottom),
                        Vec2::new(x + width, tank_bounds.max.y),
                        Vec2::new(x - width, tank_bounds.max.y),
                    ],
                    tank_bounds,
                    beam,
                );
            }
            let local =
                |p: Vec2| tank.motion.position + (p * scale).rotate_radians(tank.motion.angle);
            polygon(
                frame,
                RIGID_TANK_HULL.map(local).to_vec(),
                tank_bounds,
                color,
            );
            for x in [-1.0, 0.0, 1.0] {
                let mut track = RenderColor::rgb(0.08, 0.11, 0.12);
                track.a = color.a;
                disc(
                    frame,
                    local(Vec2::new(x, -0.45)),
                    0.28 * scale,
                    tank_bounds,
                    track,
                );
            }
            let barrel = if event.phase == EventPhase::BeamDown {
                [local(Vec2::Y), local(Vec2::Y * 3.5)]
            } else {
                tank.barrel
            };
            disc(frame, barrel[0], 0.65 * scale, tank_bounds, color);
            strip(
                frame,
                barrel[0],
                barrel[1],
                0.45 * scale,
                tank_bounds,
                color,
            );
        }
    }
    for shell in event.duel.shells() {
        disc(
            frame,
            shell.position,
            0.24 * scale,
            bounds,
            RenderColor::rgb(1.0, 0.91, 0.62),
        );
    }
    for (point, age) in event.duel.flashes().filter(|_| event.result.is_none()) {
        let t = age as f32 / 22.0;
        disc(
            frame,
            point,
            (0.5 + 2.0 * t) * scale,
            bounds,
            RenderColor::rgba(1.0, 0.65, 0.18, 0.6 * (1.0 - t)),
        );
    }
}
