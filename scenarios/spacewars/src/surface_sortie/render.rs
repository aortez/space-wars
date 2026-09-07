use super::*;
use engine_common::RenderLine;

const LIGHT: RenderColor = RenderColor::rgb(0.86, 0.93, 0.97);
const CYAN: RenderColor = RenderColor::rgb(0.25, 0.93, 0.8);
const ORANGE: RenderColor = RenderColor::rgb(1.0, 0.58, 0.23);

fn camera(state: &SurfaceSortieState) -> Camera2 {
    let snapshot = state.spaceling_snapshot();
    let ship = &state.world.ships[state.pilot.vehicle.0];
    let parked = state.vehicle_settled();
    let center = snapshot.map_or_else(
        || {
            if parked {
                (ship.position + state.access_position()) * 0.5 + Vec2::new(0.0, 2.0)
            } else {
                ship.position
            }
        },
        |s| s.motion.position + Vec2::new(0.0, 7.0),
    );
    let height = if snapshot.is_some() || parked {
        44.0
    } else {
        100.0
    };
    Camera2::new(render_point(center), height)
}

pub(super) fn frame(state: &SurfaceSortieState) -> RenderFrame {
    let observation = state.observation();
    let snapshot = state.spaceling_snapshot();
    let ship = &state.world.ships[state.pilot.vehicle.0];
    let parked = state.vehicle_settled();
    let camera = camera(state);
    let center = Vec2::new(camera.center.x, camera.center.y);
    let height = camera.height;
    let mut frame = RenderFrame::new(camera);
    let planet = &state.world.planets[0];
    let radius = planet.radius * BODY_BOUNDS_RADIUS_SCALE;
    circle(
        &mut frame,
        -20,
        planet.position,
        radius,
        RenderColor::rgb(0.09, 0.16, 0.23),
    );
    for index in 0..72 {
        let up =
            Vec2::from_radians(planet.wrapper_angle + index as f32 * std::f32::consts::TAU / 72.0);
        line(
            &mut frame,
            -18,
            planet.position + up * (radius - 1.0),
            planet.position + up * (radius - 0.08),
            RenderColor::rgb(0.28, 0.51, 0.6),
            1.0,
        );
    }
    let access = observation.access_position;
    if parked {
        circle(
            &mut frame,
            -1,
            access + state.access_up() * 0.12,
            0.38,
            CYAN,
        );
    }
    render_ship(&mut frame, ship);
    if ship.form == ShipForm::Ship {
        for foot in physics::LANDING_FEET {
            let position = ship.position + SHIP_PIVOT + foot.rotate_radians(ship.rotation_radians);
            line(
                &mut frame,
                1,
                position,
                position + Vec2::Y.rotate_radians(ship.rotation_radians) * 1.3,
                LIGHT,
                2.0,
            );
            circle(
                &mut frame,
                1,
                position,
                physics::LANDING_FOOT_RADIUS,
                if parked { CYAN } else { LIGHT },
            );
        }
    }
    render_exhaust(&mut frame, ship);
    if let Some(snapshot) = snapshot {
        draw_spaceling(
            &mut frame,
            snapshot,
            state.pilot.facing,
            state.pilot.gait_phase,
        );
        if let Some(support) = snapshot.support {
            line(
                &mut frame,
                6,
                support.position,
                support.position + support.normal,
                CYAN,
                2.0,
            );
        }
    }
    // Keep diagnostics legible when the parked ship crosses the HUD as the
    // planet rotates. The scene remains visible through the shallow strips.
    for (bottom, top) in [(0.27, 0.48), (-0.48, -0.28)] {
        frame.push_primitive(
            15,
            RenderPrimitive::Polygon(RenderPolygon {
                points: [(-4.0, bottom), (4.0, bottom), (4.0, top), (-4.0, top)]
                    .map(|(x, y)| render_point(center + Vec2::new(x, y) * height))
                    .to_vec(),
                fill: Some(Fill::new(RenderColor::rgba(0.025, 0.03, 0.055, 0.85))),
                stroke: None,
            }),
        );
    }
    let title_y = center.y + height * 0.43;
    text(
        &mut frame,
        Vec2::new(center.x, title_y),
        "SURFACE SORTIE  /  shared-world pilot experiment",
        LIGHT,
        18.0,
    );
    text(
        &mut frame,
        center + Vec2::new(0.0, height * 0.37),
        if snapshot.is_some() {
            "ON FOOT  |  Left/right: walk   A / Space: jump   B / X: board"
        } else {
            "ABOARD  |  Left/right: turn   A / Space: thrust   Down / S: brake   B / X: exit"
        },
        LIGHT,
        14.0,
    );
    text(
        &mut frame,
        center + Vec2::new(0.0, height * 0.31),
        "Start / Esc: pause   R: restart   No weapons or capture in this fixture",
        LIGHT,
        13.0,
    );
    let message = if !state.vehicle_available() {
        TransferResult::VehicleUnavailable.label()
    } else if !state.controls_armed {
        "Release all controls to activate the new control context"
    } else {
        state.last_transfer.label()
    };
    text(
        &mut frame,
        center - Vec2::new(0.0, height * 0.32),
        message,
        CYAN,
        16.0,
    );
    text(
        &mut frame,
        center - Vec2::new(0.0, height * 0.38),
        format!(
            "{}  |  angle {:.0}°  |  descent {:+.1}  |  sideways {:+.1}  |  feet {}/2",
            observation.landing.phase.label(),
            observation.landing.angle_degrees,
            observation.landing.descent_speed,
            observation.landing.lateral_speed,
            observation.landing.supported_feet
        ),
        if parked {
            CYAN
        } else if observation.landing.assist_strength > 0.0 {
            ORANGE
        } else {
            LIGHT
        },
        15.0,
    );
    text(
        &mut frame,
        center - Vec2::new(0.0, height * 0.44),
        format!(
            "{}  |  ship {:.0}%  |  transfers {}  |  bodies {}  |  access {:.1}u",
            observation.balance,
            ship.life / ship.life_max.max(1.0) * 100.0,
            observation.transfers,
            observation.physical_bodies,
            observation.position.distance_to(access),
        ),
        ORANGE,
        15.0,
    );
    frame
}

pub(super) fn minimap(state: &SurfaceSortieState, viewport_aspect: f32) -> RenderFrame {
    let radius = state.world.config.universe_radius as f32;
    let mut map = RenderFrame::new(Camera2::new(
        render_point(Vec2::splat(radius)),
        radius * 2.08,
    ));
    map.push_primitive(
        -20,
        RenderPrimitive::Circle(RenderCircle {
            center: render_point(Vec2::splat(radius)),
            radius,
            fill: None,
            stroke: Some(Stroke::new(RenderColor::rgb(0.36, 0.43, 0.52), 1.0)),
        }),
    );
    for planet in &state.world.planets {
        circle(
            &mut map,
            -10,
            planet.position,
            planet.radius * BODY_BOUNDS_RADIUS_SCALE,
            CYAN,
        );
    }
    // The footprint follows the actual full-window camera, including resizes.
    let camera = camera(state);
    let aspect = if viewport_aspect.is_finite() && viewport_aspect > 0.0 {
        viewport_aspect
    } else {
        1.0
    };
    let half = Vec2::new(camera.height * aspect, camera.height) * 0.5;
    let center = Vec2::new(camera.center.x, camera.center.y);
    map.push_primitive(
        0,
        RenderPrimitive::Polygon(RenderPolygon {
            points: [
                Vec2::new(-half.x, -half.y),
                Vec2::new(half.x, -half.y),
                half,
                Vec2::new(-half.x, half.y),
            ]
            .map(|offset| render_point(center + offset))
            .to_vec(),
            fill: None,
            stroke: Some(Stroke::new(LIGHT, 1.0)),
        }),
    );
    let ship = &state.world.ships[state.pilot.vehicle.0];
    map.push_primitive(
        2,
        RenderPrimitive::Polygon(RenderPolygon {
            points: [
                Vec2::new(0.0, 20.0),
                Vec2::new(-12.0, -12.0),
                Vec2::new(0.0, -5.0),
                Vec2::new(12.0, -12.0),
            ]
            .map(|offset| {
                render_point(
                    ship.position + SHIP_PIVOT + offset.rotate_radians(ship.rotation_radians),
                )
            })
            .to_vec(),
            fill: Some(Fill::new(RenderColor::rgb(1.0, 0.22, 0.22))),
            stroke: Some(Stroke::new(LIGHT, 1.0)),
        }),
    );
    if let Some(snapshot) = state.spaceling_snapshot() {
        map.push_primitive(
            3,
            RenderPrimitive::Polygon(RenderPolygon {
                points: [
                    Vec2::new(0.0, 12.0),
                    Vec2::new(-10.0, 0.0),
                    Vec2::new(0.0, -12.0),
                    Vec2::new(10.0, 0.0),
                ]
                .map(|offset| render_point(snapshot.motion.position + offset))
                .to_vec(),
                fill: Some(Fill::new(ORANGE)),
                stroke: Some(Stroke::new(LIGHT, 1.0)),
            }),
        );
    }
    map
}

fn draw_spaceling(frame: &mut RenderFrame, snapshot: SpacelingSnapshot, facing: f32, phase: f32) {
    let center = snapshot.motion.position;
    let local = |x, y| center + Vec2::new(x, y).rotate_radians(snapshot.motion.angle);
    let suit = match snapshot.balance {
        SpacelingBalance::Balanced => ORANGE,
        SpacelingBalance::KnockedDown => RenderColor::rgb(1.0, 0.25, 0.25),
        SpacelingBalance::Recovering => RenderColor::rgb(1.0, 0.85, 0.3),
    };
    let stride = if snapshot.grounded() && snapshot.balance == SpacelingBalance::Balanced {
        phase.sin()
            * (snapshot.relative_speed.abs() / SurfaceSortieState::spec().walk_speed).min(1.0)
    } else {
        0.2
    };
    for side in [-1.0, 1.0] {
        let swing = stride * side;
        limb(
            frame,
            local(side * 0.1, -0.15),
            local(side * 0.12 + swing * 0.25, -0.8),
            LIGHT,
            0.13,
        );
        limb(
            frame,
            local(side * 0.2, 0.3),
            local(side * 0.36 - swing * 0.12, -0.12),
            suit,
            0.12,
        );
    }
    frame.push_primitive(
        3,
        RenderPrimitive::Polygon(RenderPolygon {
            points: [(-0.23, -0.2), (0.23, -0.2), (0.26, 0.4), (-0.26, 0.4)]
                .map(|(x, y)| render_point(local(x, y)))
                .to_vec(),
            fill: Some(Fill::new(suit)),
            stroke: Some(Stroke::new(LIGHT, 1.0)),
        }),
    );
    circle(frame, 4, local(0.0, 0.63), 0.25, LIGHT);
    circle(
        frame,
        5,
        local(facing * 0.1, 0.65),
        0.14,
        RenderColor::rgb(0.09, 0.16, 0.23),
    );
}

fn limb(frame: &mut RenderFrame, a: Vec2, b: Vec2, color: RenderColor, width: f32) {
    let side = (b - a)
        .normalized()
        .rotate_radians(std::f32::consts::FRAC_PI_2)
        * width
        * 0.5;
    frame.push_primitive(
        2,
        RenderPrimitive::Polygon(RenderPolygon {
            points: [a - side, a + side, b + side, b - side]
                .map(render_point)
                .to_vec(),
            fill: Some(Fill::new(color)),
            stroke: None,
        }),
    );
}

fn circle(frame: &mut RenderFrame, layer: i32, center: Vec2, radius: f32, color: RenderColor) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Circle(RenderCircle {
            center: render_point(center),
            radius,
            fill: Some(Fill::new(color)),
            stroke: None,
        }),
    );
}

fn line(frame: &mut RenderFrame, layer: i32, a: Vec2, b: Vec2, color: RenderColor, width: f32) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Line(RenderLine::new(
            render_point(a),
            render_point(b),
            Stroke::new(color, width),
        )),
    );
}

fn text(
    frame: &mut RenderFrame,
    position: Vec2,
    label: impl Into<String>,
    color: RenderColor,
    size: f32,
) {
    let mut text = RenderText::new(render_point(position), label);
    text.anchor = TextAnchor::Center;
    text.color = color;
    text.size = size;
    frame.push_primitive(20, RenderPrimitive::Text(text));
}
