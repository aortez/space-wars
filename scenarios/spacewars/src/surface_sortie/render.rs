use super::*;
use engine_common::RenderLine;

const LIGHT: RenderColor = RenderColor::rgb(0.86, 0.93, 0.97);
const CYAN: RenderColor = RenderColor::rgb(0.25, 0.93, 0.8);
const ORANGE: RenderColor = RenderColor::rgb(1.0, 0.58, 0.23);
const AMBER: RenderColor = RenderColor::rgb(1.0, 0.82, 0.25);

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
    if let Some(sun) = state.world.sun {
        circle(
            &mut frame,
            -21,
            sun.position,
            sun.radius,
            RenderColor::rgb(1.0, 0.85, 0.25),
        );
    }
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
    draw_outpost(&mut frame, state, &observation.outpost);
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
    for (bottom, top) in [(0.27, 0.48), (-0.49, -0.255)] {
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
        format!("SURFACE SORTIE  /  {}", state.motion_preset.label()),
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
        format!(
            "Planet {:.1}u/s / spin {:+.3}  |  Stand at terminal: capture  |  Start / Esc: pause",
            observation.motion.planet_velocity.length(),
            observation.motion.planet_spin
        ),
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
        center - Vec2::new(0.0, height * 0.278),
        message,
        CYAN,
        16.0,
    );
    let post = &observation.outpost;
    let ownership = post.owner.map_or_else(
        || "NEUTRAL".to_owned(),
        |owner| format!("P{}", owner.index() + 1),
    );
    text(
        &mut frame,
        center - Vec2::new(0.0, height * 0.323),
        format!(
            "OUTPOST {ownership}  |  {:.0}%  |  {}  |  {:.1}u",
            post.capture_progress * 100.0,
            post.capture_status.label(),
            observation.position.distance_to(post.position)
        ),
        if post.owner == Some(observation.owner) {
            CYAN
        } else {
            AMBER
        },
        14.0,
    );
    text(
        &mut frame,
        center - Vec2::new(0.0, height * 0.369),
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
        center - Vec2::new(0.0, height * 0.415),
        format!(
            "Ship {:.0}%  |  {}  |  hatch {:.1}u  |  bodies {}",
            ship.life / ship.life_max.max(1.0) * 100.0,
            post.repair_status.label(),
            observation.position.distance_to(access),
            observation.physical_bodies,
        ),
        ORANGE,
        15.0,
    );
    let metrics = observation.motion_metrics;
    text(
        &mut frame,
        center - Vec2::new(0.0, height * 0.465),
        format!(
            "Rel {:.1}u/s  |  lost support: pilot {} ship {}  |  idle drift {:.2}u  |  damage {:.1}",
            observation.motion.relative_velocity.length(),
            metrics.pilot_support_losses,
            metrics.ship_support_losses,
            metrics.idle_drift,
            metrics.ship_damage
        ),
        LIGHT,
        12.0,
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
    if let Some(sun) = state.world.sun {
        circle(
            &mut map,
            -19,
            sun.position,
            sun.radius,
            RenderColor::rgb(1.0, 0.85, 0.25),
        );
        map.push_primitive(
            -18,
            RenderPrimitive::Circle(RenderCircle {
                center: render_point(sun.position),
                radius: state.world.planets[0].orbit_radius,
                fill: None,
                stroke: Some(Stroke::new(RenderColor::rgba(0.55, 0.6, 0.7, 0.45), 1.0)),
            }),
        );
    }
    for planet in &state.world.planets {
        circle(
            &mut map,
            -10,
            planet.position,
            planet.radius * BODY_BOUNDS_RADIUS_SCALE,
            CYAN,
        );
    }
    let outpost = &state.outpost;
    let planet = &state.world.planets[outpost.planet];
    let site = outpost.position(planet);
    // Lift the symbol off the planet rim, with a leader to its exact location;
    // the tiny ship/creature icons must remain distinguishable nearby.
    let marker = site + outpost.up(planet) * 35.0;
    let owner_color = outpost_color(state);
    line(&mut map, 1, site, marker, owner_color, 1.0);
    map.push_primitive(
        1,
        RenderPrimitive::Polygon(RenderPolygon {
            points: [
                Vec2::new(-12.0, -12.0),
                Vec2::new(12.0, -12.0),
                Vec2::new(12.0, 12.0),
                Vec2::new(-12.0, 12.0),
            ]
            .map(|point| render_point(marker + point))
            .to_vec(),
            fill: Some(Fill::new(owner_color)),
            stroke: Some(Stroke::new(LIGHT, 1.0)),
        }),
    );
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

fn outpost_color(state: &SurfaceSortieState) -> RenderColor {
    state.outpost.owner.map_or(AMBER, |owner| {
        render_color(state.world.players[owner.index()].color)
    })
}

fn draw_outpost(
    frame: &mut RenderFrame,
    state: &SurfaceSortieState,
    observation: &OutpostObservation,
) {
    let planet = &state.world.planets[state.outpost.planet];
    let up = state.outpost.up(planet);
    let right = Vec2::new(up.y, -up.x);
    let base = observation.position;
    let local = |x, y| base + right * x + up * y;
    let owner_color = outpost_color(state);
    // Service radius is an eligibility hint, not a docking region or force.
    // Its underground half is occluded by the planet fill.
    let range_color = if observation.owner.is_some() {
        RenderColor::rgba(0.3, 0.9, 0.7, 0.35)
    } else {
        RenderColor::rgba(1.0, 0.82, 0.25, 0.2)
    };
    for segment in (0..64).step_by(2) {
        let a = segment as f32 * std::f32::consts::TAU / 64.0;
        let b = (segment + 1) as f32 * std::f32::consts::TAU / 64.0;
        line(
            frame,
            -21,
            base + Vec2::from_radians(a) * observation.repair_range,
            base + Vec2::from_radians(b) * observation.repair_range,
            range_color,
            1.0,
        );
    }
    for segment in 0..12 {
        let a = (segment as f32 / 12.0 * 2.0 - 1.0) * observation.capture_range
            / (planet.radius * BODY_BOUNDS_RADIUS_SCALE);
        let b = ((segment + 1) as f32 / 12.0 * 2.0 - 1.0) * observation.capture_range
            / (planet.radius * BODY_BOUNDS_RADIUS_SCALE);
        let radius = planet.radius * BODY_BOUNDS_RADIUS_SCALE + 0.08;
        line(
            frame,
            -5,
            planet.position + up.rotate_radians(a) * radius,
            planet.position + up.rotate_radians(b) * radius,
            owner_color,
            2.0,
        );
    }
    let quad = |left, bottom, right, top, fill| {
        RenderPrimitive::Polygon(RenderPolygon {
            points: [(left, bottom), (right, bottom), (right, top), (left, top)]
                .map(|(x, y)| render_point(local(x, y)))
                .to_vec(),
            fill: Some(Fill::new(fill)),
            stroke: Some(Stroke::new(LIGHT, 1.0)),
        })
    };
    let half = physics::OUTPOST_TERMINAL_HALF_SIZE;
    frame.push_primitive(
        -4,
        quad(
            -half.x,
            -0.02,
            half.x,
            half.y * 2.0 - 0.02,
            RenderColor::rgb(0.2, 0.27, 0.35),
        ),
    );
    frame.push_primitive(
        -3,
        quad(
            -0.48,
            0.9,
            0.48,
            1.8,
            if observation.owner.is_some() {
                CYAN
            } else {
                AMBER
            },
        ),
    );
    line(frame, -3, local(0.6, 1.8), local(0.6, 3.6), LIGHT, 1.5);
    if observation.owner.is_some() {
        frame.push_primitive(-2, quad(0.6, 2.75, 2.1, 3.6, owner_color));
    }
    frame.push_primitive(
        -2,
        quad(-1.2, 2.6, 0.25, 2.8, RenderColor::rgb(0.08, 0.1, 0.16)),
    );
    if observation.capture_progress > 0.0 {
        frame.push_primitive(
            -1,
            quad(
                -1.2,
                2.6,
                -1.2 + 1.45 * observation.capture_progress,
                2.8,
                owner_color,
            ),
        );
    }
    text(
        frame,
        local(0.0, 4.4),
        observation.owner.map_or_else(
            || "NEUTRAL OUTPOST".to_owned(),
            |owner| format!("P{} OUTPOST", owner.index() + 1),
        ),
        owner_color,
        12.0,
    );
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
