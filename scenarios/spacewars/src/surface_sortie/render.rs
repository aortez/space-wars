use super::*;
use engine_common::RenderLine;

const LIGHT: RenderColor = RenderColor::rgb(0.86, 0.93, 0.97);
const CYAN: RenderColor = RenderColor::rgb(0.25, 0.93, 0.8);
const ORANGE: RenderColor = RenderColor::rgb(1.0, 0.58, 0.23);
const AMBER: RenderColor = RenderColor::rgb(1.0, 0.82, 0.25);

fn camera(state: &SurfaceSortieState, player: usize) -> Camera2 {
    super::camera::target(state, player, false).camera
}

pub(super) fn frame(state: &SurfaceSortieState, player: usize) -> RenderFrame {
    frame_in_view(state, player, None)
}

pub(super) fn frame_in_view(
    state: &SurfaceSortieState,
    player: usize,
    viewport: Option<RenderPoint>,
) -> RenderFrame {
    frame_with_camera(state, player, camera(state, player), viewport)
}

pub(super) fn frame_with_camera(
    state: &SurfaceSortieState,
    player: usize,
    camera: Camera2,
    viewport: Option<RenderPoint>,
) -> RenderFrame {
    let observation = state.observation(player);
    let center = Vec2::new(camera.center.x, camera.center.y);
    let height = camera.height;
    let view = viewport
        .filter(|v| v.x.is_finite() && v.y.is_finite() && v.x > 0.0 && v.y > 0.0)
        .map(|v| {
            let mut bounds = camera.world_bounds(v.x / v.y);
            // Keep border pixels/strokes even at very small raster resolutions.
            // Extra world-coordinate slack absorbs inverse-transform rounding.
            let padding = height * 6.0 / v.y.max(1.0)
                + (center.x.abs() + center.y.abs() + height) * 32.0 * f32::EPSILON;
            bounds.min.x -= padding;
            bounds.min.y -= padding;
            bounds.max.x += padding;
            bounds.max.y += padding;
            bounds
        });
    let mut frame = RenderFrame::new(camera);
    if let Some(sun) = state.world.sun {
        draw_corona(&mut frame, state, sun, -22);
        circle(
            &mut frame,
            -21,
            sun.position,
            sun.radius,
            RenderColor::rgb(1.0, 0.85, 0.25),
        );
    }
    for (planet_index, planet) in state.world.planets.iter().enumerate() {
        let radius = planet.radius * BODY_BOUNDS_RADIUS_SCALE;
        if let Some(material) = state.world.terrain.planets.get(&planet_index) {
            let motion = motion::SurfaceFrame::read(&state.world.physics, planet_index);
            terrain::render_body_in_view(
                &mut frame,
                &material.field,
                &material.geometry,
                motion.position,
                motion.angle,
                view,
            );
        } else {
            circle(
                &mut frame,
                -20,
                planet.position,
                radius,
                RenderColor::rgb(0.09, 0.16, 0.23),
            );
        }
        if let Some(owner) = planet.owner_id {
            frame.push_primitive(
                -17,
                RenderPrimitive::Circle(RenderCircle {
                    center: render_point(planet.position),
                    radius,
                    fill: None,
                    stroke: Some(Stroke::new(
                        render_color(state.world.players[owner].color),
                        1.5,
                    )),
                }),
            );
        }
        for index in 0..if state.has_material_ground() { 0 } else { 72 } {
            let up = Vec2::from_radians(
                planet.wrapper_angle + index as f32 * std::f32::consts::TAU / 72.0,
            );
            line(
                &mut frame,
                -18,
                planet.position + up * (radius - 1.0),
                planet.position + up * (radius - 0.08),
                RenderColor::rgb(0.28, 0.51, 0.6),
                1.0,
            );
        }
    }
    for fragment in state.world.terrain.fragments.values() {
        if let Some(body) = state.world.physics.world.motion(fragment.assembly.body()) {
            terrain::render_body_in_view(
                &mut frame,
                &fragment.terrain,
                &fragment.geometry,
                body.position,
                body.angle,
                view,
            );
        }
    }
    if let Some(mining) = &observation.mining {
        if let Some((start, end)) = mining.beam {
            line(
                &mut frame,
                8,
                start,
                end,
                if mining.held { AMBER } else { CYAN },
                if mining.held { 2.0 } else { 0.7 },
            );
        }
    }
    for post in &observation.outposts {
        draw_outpost(&mut frame, state, post);
    }
    for flag in observation
        .planet_claims
        .iter()
        .filter_map(|claim| claim.flag.as_ref())
    {
        draw_planet_flag(&mut frame, state, flag);
    }
    if observation.recovery.is_some() {
        for debris in state.world.debris.iter().filter(|debris| !debris.dead) {
            render_debris(&mut frame, debris);
        }
        render_particles(&mut frame, &state.world);
    }
    for player in 0..state.player_count() {
        draw_actor(&mut frame, state, player);
    }
    if state.surface_comparison.is_some() {
        for (&index, terrain) in &state.world.terrain.planets {
            let motion = motion::SurfaceFrame::read(&state.world.physics, index);
            terrain::render_wireframe(
                &mut frame,
                &terrain.field,
                &terrain.geometry,
                motion.position,
                motion.angle,
            );
        }
        for fragment in state.world.terrain.fragments.values() {
            if let Some(motion) = state.world.physics.world.motion(fragment.assembly.body()) {
                terrain::render_wireframe(
                    &mut frame,
                    &fragment.terrain,
                    &fragment.geometry,
                    motion.position,
                    motion.angle,
                );
            }
        }
    }
    frame
}

pub(super) fn minimap(
    state: &SurfaceSortieState,
    player: usize,
    viewport_aspect: f32,
) -> RenderFrame {
    minimap_with_camera(state, player, viewport_aspect, camera(state, player))
}

pub(super) fn minimap_with_camera(
    state: &SurfaceSortieState,
    player: usize,
    viewport_aspect: f32,
    camera: Camera2,
) -> RenderFrame {
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
        draw_corona(&mut map, state, sun, -20);
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
                radius: state.world.planets[state.pilots[player].planet].orbit_radius,
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
            planet
                .owner_id
                .map_or(CYAN, |owner| render_color(state.world.players[owner].color)),
        );
    }
    for claim in &state.claims {
        let observation = state
            .claim_observation(claim.planet, player)
            .expect("indexed claim");
        let Some(flag) = observation.flag else {
            continue;
        };
        let half = (radius * 0.025).max(12.0);
        let marker = flag.position + flag.normal * (half * 2.0).max(35.0);
        let color = render_color(state.world.players[flag.player.index()].color);
        line(&mut map, 1, flag.position, marker, color, 1.0);
        map.push_primitive(
            2,
            RenderPrimitive::Polygon(RenderPolygon {
                points: [
                    Vec2::new(-half, -half),
                    Vec2::new(-half, half),
                    Vec2::new(half, 0.0),
                ]
                .map(|point| render_point(marker + point))
                .to_vec(),
                fill: Some(Fill::new(color)),
                stroke: Some(Stroke::new(LIGHT, 1.0)),
            }),
        );
    }
    for outpost in &state.outposts {
        let planet = &state.world.planets[outpost.planet];
        let site = outpost.position(planet);
        // Lift the symbol off the planet rim, with a leader to its exact location;
        // the tiny ship/creature icons must remain distinguishable nearby.
        // Generated worlds are much wider than the small lab. Keep ownership
        // squares large enough to show their fill inside the one-pixel outline.
        let half_size = if state.travel_enabled() {
            (radius * 0.025).max(12.0)
        } else {
            12.0
        };
        let marker = site + outpost.up(planet) * (half_size * 2.0).max(35.0);
        let owner_color = outpost_color(state, outpost.owner);
        line(&mut map, 1, site, marker, owner_color, 1.0);
        map.push_primitive(
            1,
            RenderPrimitive::Polygon(RenderPolygon {
                points: [
                    Vec2::new(-half_size, -half_size),
                    Vec2::new(half_size, -half_size),
                    Vec2::new(half_size, half_size),
                    Vec2::new(-half_size, half_size),
                ]
                .map(|point| render_point(marker + point))
                .to_vec(),
                fill: Some(Fill::new(owner_color)),
                stroke: Some(Stroke::new(LIGHT, 1.0)),
            }),
        );
    }
    // Use exactly the displayed camera, including smoothing and resizes.
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
    for player in 0..state.player_count() {
        let ship = &state.world.ships[state.pilots[player].vehicle.0];
        if !ship.dead {
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
                            ship.position
                                + physics::ship_pivot(ship.form)
                                + (offset
                                    * if ship.form == ShipForm::EscapePod {
                                        0.7
                                    } else {
                                        1.0
                                    })
                                .rotate_radians(ship.rotation_radians),
                        )
                    })
                    .to_vec(),
                    fill: Some(Fill::new(render_color(state.world.players[player].color))),
                    stroke: Some(Stroke::new(LIGHT, 1.0)),
                }),
            );
        }
        if let Some(snapshot) = state.spaceling_snapshot(player) {
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
                    fill: Some(Fill::new(pilot_color(state, player))),
                    stroke: Some(Stroke::new(LIGHT, 1.0)),
                }),
            );
        }
    }
    map
}

fn outpost_color(state: &SurfaceSortieState, owner: Option<PlayerId>) -> RenderColor {
    owner.map_or(AMBER, |owner| {
        render_color(state.world.players[owner.index()].color)
    })
}

fn draw_planet_flag(
    frame: &mut RenderFrame,
    state: &SurfaceSortieState,
    flag: &PlanetFlagObservation,
) {
    let up = flag.normal;
    let right = Vec2::new(up.y, -up.x);
    let local = |x, y| flag.position + right * x + up * y;
    line(frame, 0, local(0.0, 0.0), local(0.0, 3.6), LIGHT, 1.5);
    let top = 0.9 + flag.raised_fraction * 2.7;
    frame.push_primitive(
        1,
        RenderPrimitive::Polygon(RenderPolygon {
            points: [(0.0, top), (1.8, top - 0.45), (0.0, top - 0.9)]
                .map(|(x, y)| render_point(local(x, y)))
                .to_vec(),
            fill: Some(Fill::new(render_color(
                state.world.players[flag.player.index()].color,
            ))),
            stroke: Some(Stroke::new(LIGHT, 1.0)),
        }),
    );
}

fn draw_outpost(
    frame: &mut RenderFrame,
    state: &SurfaceSortieState,
    observation: &OutpostObservation,
) {
    let planet = &state.world.planets[observation.planet];
    let up = observation.surface_normal;
    let right = Vec2::new(up.y, -up.x);
    let base = observation.position;
    let local = |x, y| base + right * x + up * y;
    let owner_color = outpost_color(state, observation.owner);
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

fn draw_spaceling(
    frame: &mut RenderFrame,
    snapshot: SpacelingSnapshot,
    facing: f32,
    phase: f32,
    color: RenderColor,
) {
    let center = snapshot.motion.position;
    let local = |x, y| center + Vec2::new(x, y).rotate_radians(snapshot.motion.angle);
    let suit = match snapshot.balance {
        SpacelingBalance::Balanced => color,
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

fn draw_corona(frame: &mut RenderFrame, state: &SurfaceSortieState, sun: SunState, layer: i32) {
    if !state.combat_enabled() {
        return;
    }
    circle(
        frame,
        layer,
        sun.position,
        sun.radius + solar::CORONA_WIDTH,
        RenderColor::rgba(1.0, 0.28, 0.04, 0.24),
    );
    frame.push_primitive(
        layer,
        RenderPrimitive::Circle(RenderCircle {
            center: render_point(sun.position),
            radius: sun.radius + solar::CORONA_WIDTH,
            fill: None,
            stroke: Some(Stroke::new(RenderColor::rgba(1.0, 0.38, 0.08, 0.7), 1.0)),
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

fn pilot_color(state: &SurfaceSortieState, player: usize) -> RenderColor {
    if state.player_count() == 1 {
        ORANGE
    } else {
        render_color(state.world.players[player].color)
    }
}

fn draw_actor(frame: &mut RenderFrame, state: &SurfaceSortieState, player: usize) {
    let access = state.access_position(player);
    let ship = &state.world.ships[state.pilots[player].vehicle.0];
    let snapshot = state.spaceling_snapshot(player);
    let parked = state.vehicle_settled(player);
    if parked && !ship.dead {
        circle(
            frame,
            -1,
            access + state.access_up(player) * 0.12,
            0.38,
            CYAN,
        );
    }
    if !ship.dead {
        render_ship(frame, ship);
        render_laser(frame, ship);
    }
    if !ship.dead && (ship.form == ShipForm::Ship || state.pilots[player].recovery.is_some()) {
        draw_landing_gear(
            frame,
            ship,
            state.pilots[player].landing_gear.extension(),
            parked,
        );
    }
    if !ship.dead {
        render_exhaust(frame, ship);
    }
    if let Some(snapshot) = snapshot {
        draw_spaceling(
            frame,
            snapshot,
            state.pilots[player].facing,
            state.pilots[player].gait_phase,
            pilot_color(state, player),
        );
        if let Some(pack) = state.pilots[player]
            .body
            .as_ref()
            .and_then(|body| body.jetpack())
        {
            let position = snapshot.motion.position;
            let right = Vec2::new(snapshot.up.y, -snapshot.up.x);
            for side in [-1.0, 1.0] {
                let nozzle = position + right * side * 0.28 - snapshot.up * 0.2;
                circle(frame, 5, nozzle, 0.15, CYAN);
                if pack.active {
                    line(frame, 4, nozzle, nozzle - snapshot.up * 1.3, ORANGE, 3.0);
                    line(frame, 5, nozzle, nozzle - snapshot.up * 0.65, LIGHT, 1.5);
                }
            }
        }
        if let Some(support) = snapshot.support {
            line(
                frame,
                6,
                support.position,
                support.position + support.normal,
                CYAN,
                2.0,
            );
        }
    }
}

/// Full ships fold their visual feet into the fuselage. At full extension the
/// pads match the existing physical contacts exactly; pods retain fixed feet.
pub(super) fn draw_landing_gear(
    frame: &mut RenderFrame,
    ship: &ShipState,
    extension: f32,
    parked: bool,
) {
    let full_ship = ship.form == ShipForm::Ship;
    let extension = if full_ship {
        extension.clamp(0.0, 1.0)
    } else {
        1.0
    };
    if ship.dead || extension <= 0.0 {
        return;
    }
    let origin = ship.position + physics::ship_pivot(ship.form);
    let transform = |point: Vec2| origin + point.rotate_radians(ship.rotation_radians);
    let (feet, radius) = physics::surface_landing_geometry(ship.form);
    for foot in feet {
        let hinge = if full_ship {
            Vec2::new(foot.x * 0.5, -2.5)
        } else {
            foot + Vec2::Y * radius * (1.3 / physics::LANDING_FOOT_RADIUS)
        };
        let position = transform(hinge + (foot - hinge) * extension);
        // Draw ship gear behind the hull, so retraction disappears into its
        // bays instead of sliding circles over the top of the ship artwork.
        let layer = if full_ship { SHIP_LAYER - 1 } else { 1 };
        line(
            frame,
            layer,
            transform(hinge),
            position,
            LIGHT,
            if full_ship { 0.35 } else { 2.0 },
        );
        circle(
            frame,
            layer,
            position,
            radius,
            if parked { CYAN } else { LIGHT },
        );
    }
}
