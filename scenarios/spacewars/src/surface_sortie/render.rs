use super::*;
use engine_common::RenderLine;

const LIGHT: RenderColor = RenderColor::rgb(0.86, 0.93, 0.97);
const CYAN: RenderColor = RenderColor::rgb(0.25, 0.93, 0.8);
const ORANGE: RenderColor = RenderColor::rgb(1.0, 0.58, 0.23);
const AMBER: RenderColor = RenderColor::rgb(1.0, 0.82, 0.25);

fn camera(state: &SurfaceSortieState, player: usize) -> Camera2 {
    let snapshot = state.spaceling_snapshot(player);
    let ship = &state.world.ships[state.pilots[player].vehicle.0];
    let parked = state.vehicle_settled(player);
    let (center, height) = if let Some(snapshot) = snapshot {
        let pilot = snapshot.motion.position;
        let ship_center = ship.position + physics::ship_pivot(ship.form);
        if parked && pilot.distance_to(ship_center) < 32.0 {
            // North-up framing must work on the sides/underside too. Keep the
            // nearby ship and pilot in the central band between the HUD strips;
            // walk farther away and the camera follows only the active pilot.
            let separation = ship_center - pilot;
            let height = 44.0_f32.max((separation.y.abs() + 12.0) / 0.48);
            ((pilot + ship_center) * 0.5, height)
        } else {
            (pilot + snapshot.up * 7.0, 44.0)
        }
    } else if parked {
        (
            (ship.position + state.access_position(player)) * 0.5 + state.access_up(player) * 2.0,
            44.0,
        )
    } else {
        let target = state
            .combat_enabled()
            .then(|| {
                state.pilots.iter().enumerate().find_map(|(seat, pilot)| {
                    let target = &state.world.ships[pilot.vehicle.0];
                    (seat != player
                        && pilot.body.is_none()
                        && !target.dead
                        && target.form == ShipForm::Ship
                        && target.position.distance_to(ship.position) < 260.0)
                        .then_some(target.position)
                })
            })
            .flatten();
        if let Some(target) = target {
            let separation = target - ship.position;
            (
                (ship.position + target) * 0.5,
                (180.0_f32
                    .max(separation.x.abs() / 0.6)
                    .max(separation.y.abs() / 0.35))
                .min(440.0),
            )
        } else {
            (
                ship.position,
                if state.combat_enabled() && ship.form == ShipForm::Ship {
                    260.0
                } else {
                    100.0
                },
            )
        }
    };
    Camera2::new(render_point(center), height)
}

pub(super) fn frame(state: &SurfaceSortieState, player: usize) -> RenderFrame {
    let observation = state.observation(player);
    let snapshot = state.spaceling_snapshot(player);
    let ship = &state.world.ships[state.pilots[player].vehicle.0];
    let parked = state.vehicle_settled(player);
    let camera = camera(state, player);
    let center = Vec2::new(camera.center.x, camera.center.y);
    let height = camera.height;
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
            terrain::render_body(
                &mut frame,
                &material.field,
                &material.geometry,
                motion.position,
                motion.angle,
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
            terrain::render_body(
                &mut frame,
                &fragment.terrain,
                &fragment.geometry,
                body.position,
                body.angle,
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
    if state.player_count() > 1 || observation.planet_claim.is_some() {
        draw_player_hud(&mut frame, state, player, center, height);
        return frame;
    }
    let access = observation.access_position;
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
        state.generated_case.map_or_else(
            || format!("SURFACE SORTIE  /  {}", state.motion_preset.label()),
            |case| {
                if state.travel_enabled() {
                    return format!(
                        "SURFACE EXPEDITION / V1  |  seed {}  |  approach planet {}",
                        case.seed, state.pilots[player].planet
                    );
                }
                format!(
                    "{}  |  seed {} planet {} bearing {}",
                    case.profile.label(),
                    case.seed,
                    case.planet,
                    case.bearing
                )
            },
        ),
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
    let message = if !state.vehicle_available(player) {
        TransferResult::VehicleUnavailable.label()
    } else if !state.pilots[player].controls_armed {
        "Release all controls to activate the new control context"
    } else {
        state.pilots[player].last_transfer.label()
    };
    text(
        &mut frame,
        center - Vec2::new(0.0, height * 0.278),
        message,
        CYAN,
        16.0,
    );
    let post = observation
        .outpost
        .as_ref()
        .expect("pinned outpost fixture");
    let ownership = post.owner.map_or_else(
        || "NEUTRAL".to_owned(),
        |owner| format!("P{}", owner.index() + 1),
    );
    text(
        &mut frame,
        center - Vec2::new(0.0, height * 0.323),
        format!(
            "OUTPOST {ownership}  |  {:.0}%  |  {}  |  {:.1}u{}",
            post.capture_progress * 100.0,
            post.capture_status.label(),
            observation.position.distance_to(post.position),
            if state.travel_enabled() {
                format!("  |  site {} / planet {}", post.id.0, post.planet)
            } else {
                String::new()
            }
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

pub(super) fn minimap(
    state: &SurfaceSortieState,
    player: usize,
    viewport_aspect: f32,
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
    // The footprint follows the actual full-window camera, including resizes.
    let camera = camera(state, player);
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
        let (feet, radius) = physics::surface_landing_geometry(ship.form);
        for foot in feet {
            let position = ship.position
                + physics::ship_pivot(ship.form)
                + foot.rotate_radians(ship.rotation_radians);
            line(
                frame,
                1,
                position,
                position
                    + Vec2::Y.rotate_radians(ship.rotation_radians)
                        * radius
                        * (1.3 / physics::LANDING_FOOT_RADIUS),
                LIGHT,
                2.0,
            );
            circle(
                frame,
                1,
                position,
                radius,
                if parked { CYAN } else { LIGHT },
            );
        }
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

fn draw_player_hud(
    frame: &mut RenderFrame,
    state: &SurfaceSortieState,
    player: usize,
    center: Vec2,
    height: f32,
) {
    let observation = state.observation(player);
    let ship = &state.world.ships[state.pilots[player].vehicle.0];
    let color = pilot_color(state, player);
    for (bottom, top) in [(0.29, 0.49), (-0.49, -0.25)] {
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
    let mode = if observation.pilot_vitals.is_some_and(|v| !v.alive()) {
        "DEAD"
    } else if observation.location == PilotLocation::OnFoot {
        "ON FOOT"
    } else if ship.form == ShipForm::EscapePod {
        "POD"
    } else {
        "ABOARD"
    };
    let (objective, progress, detail) = if let Some(claim) = &observation.planet_claim {
        let owner = claim.owner.map_or("Neutral".to_owned(), |owner| {
            format!("P{}", owner.index() + 1)
        });
        let phase = match claim.phase {
            PlanetClaimPhase::Idle => String::new(),
            PlanetClaimPhase::Lowering => format!(" / lowering {:.0}%", claim.progress * 100.0),
            PlanetClaimPhase::Raising => format!(" / raising {:.0}%", claim.progress * 100.0),
        };
        (
            format!("Planet {}: {owner}{phase}", claim.planet),
            claim.status.label().to_owned(),
            claim.flag.map_or_else(
                || {
                    format!(
                        "Stand still 3s / feet {}/2",
                        observation.landing.supported_feet
                    )
                },
                |flag| {
                    format!(
                        "Flag {:.1}u / feet {}/2",
                        observation.position.distance_to(flag.position),
                        observation.landing.supported_feet
                    )
                },
            ),
        )
    } else {
        let post = observation.outpost.as_ref().expect("outpost fixture");
        (
            format!(
                "Site {}: {} / {:.0}%",
                post.id.0,
                post.owner.map_or("Neutral".to_owned(), |owner| format!(
                    "P{}",
                    owner.index() + 1
                )),
                post.capture_progress * 100.0
            ),
            post.capture_status.label().to_owned(),
            format!(
                "{} / feet {}/2",
                post.repair_status.label(),
                observation.landing.supported_feet
            ),
        )
    };
    let recovery = observation.recovery.as_ref();
    let vehicle_status = if !observation.ship_available && recovery.is_some() {
        if ship.dead {
            "Ship lost / pilot alive".to_owned()
        } else {
            format!("Escape pod / {}", observation.landing.phase.label())
        }
    } else {
        format!(
            "Ship {:.0}%  /  {}",
            ship.life / ship.life_max * 100.0,
            observation.landing.phase.label()
        )
    };
    let vehicle_status = if state.has_material_ground() && ship.form == ShipForm::Ship && !ship.dead
    {
        let flight = state.flight_observation(player);
        format!(
            "{:.0}% {} {:.0}u/s / {}",
            ship.life / ship.life_max * 100.0,
            flight.label(),
            flight.relative_speed,
            observation.landing.phase.label()
        )
    } else {
        vehicle_status
    };
    let righting = state.pilots[player].pod_righting_observation(
        &state.world.physics,
        ship,
        &observation.landing,
    );
    let recovery_message = recovery
        .filter(|r| r.scuttle_progress > 0.0 || !observation.ship_available)
        .map(|r| {
            if let Some(lift) = righting {
                if lift.remaining_seconds > 0.0 {
                    return "Pod recovery lift / turn upright".to_owned();
                }
                if lift.eligible {
                    return "Tipped pod / brake + thrust to lift".to_owned();
                }
            }
            if r.scuttle_progress > 0.0 {
                format!(
                    "Scuttle {:.0}% / release to cancel",
                    r.scuttle_progress * 100.0
                )
            } else if r.rebuild_progress > 0.0 {
                format!(
                    "Rebuild {:.0}% / {}",
                    r.rebuild_progress * 100.0,
                    match r.status {
                        SurfaceRecoveryStatus::ClearanceBlocked =>
                            "space blocked; move along surface",
                        SurfaceRecoveryStatus::HatchBlocked =>
                            "hatch access blocked; move along surface",
                        _ => "stand still",
                    }
                )
            } else if r.status == SurfaceRecoveryStatus::LandPod
                && observation.landing.phase == LandingPhase::Landed
            {
                if observation.last_transfer == TransferResult::ExitBlocked {
                    "No clear space beside the hatch".to_owned()
                } else {
                    "B: exit your landed pod".to_owned()
                }
            } else {
                r.status.label().to_owned()
            }
        });
    let damage = state.damage_observation(player);
    let supply = (mode == "ABOARD").then(|| ship.weapon_supply()).flatten();
    let progress = supply.map_or(progress, |s| {
        let status = if s.laser_recharging {
            "laser charging".to_owned()
        } else if let Some(progress) = s.reload_progress {
            format!(
                "reload {:.1}s",
                (1.0 - progress) * weapons::ROUND_RELOAD_SECONDS
            )
        } else if s.rounds_loaded < weapons::ROUND_CAPACITY as u8 {
            "reload needs 25%".to_owned()
        } else {
            "rounds ready".to_owned()
        };
        format!("Energy {:.0}% / {status}", s.energy_percent)
    });
    let progress = state.pilots[player]
        .body
        .as_ref()
        .and_then(|body| body.jetpack())
        .map_or(progress, |pack| {
            format!("Jetpack {:.0}% / hold A for lift", pack.charge * 100.0)
        });
    let vehicle_status = if damage
        .last_damage_tick
        .is_some_and(|tick| observation.tick.saturating_sub(tick) < 180)
    {
        if damage.last_ship_lost {
            format!("{} / ship lost", damage.last_source.unwrap_or("Impact"))
        } else {
            format!(
                "{} -{:.0}% / ship {:.0}%",
                damage.last_source.unwrap_or("Impact"),
                damage.last_damage_percent,
                ship.life / ship.life_max * 100.0
            )
        }
    } else {
        vehicle_status
    };
    let vehicle_status = if let Some(heat) = observation.solar.filter(|h| h.intensity > 0.0) {
        format!(
            "SOLAR HEAT / hull {:.0}% / -{:.0}%/s",
            ship.life / ship.life_max * 100.0,
            heat.damage_percent_per_second
        )
    } else {
        vehicle_status
    };
    let pilot_status = observation.pilot_vitals.map_or_else(
        || {
            format!(
                "P{}  {mode}  /  planet {}",
                player + 1,
                observation.motion.planet
            )
        },
        |v| format!("P{}  {mode}  /  pilot {:.0}%", player + 1, v.health),
    );
    let pilot_status = state.surface_comparison.map_or_else(
        || pilot_status.clone(),
        |surface| {
            format!(
                "{} / {pilot_status}",
                if surface == engine_terrain::TerrainSurface::Contour {
                    "SLOPES"
                } else {
                    "STEPS"
                }
            )
        },
    );
    let vehicle_status = if let Some(v) = observation
        .pilot_vitals
        .filter(|v| v.protected_until_tick > observation.tick)
    {
        format!(
            "Ejection protection {:.1}s",
            (v.protected_until_tick - observation.tick) as f32 / 60.0
        )
    } else {
        vehicle_status
    };
    let lines = [
        (0.49, state.match_clock_label().unwrap_or_default(), LIGHT),
        (0.445, pilot_status, color),
        (0.385, vehicle_status, LIGHT),
        (
            0.325,
            if state.has_material_ground() && mode == "ABOARD" {
                "A: thrust  RB: cruise  B: exit"
            } else {
                "A: thrust/jump  B: board/exit"
            }
            .to_owned(),
            LIGHT,
        ),
        (
            -0.29,
            if let Some(message) = state.match_result_message() {
                message
            } else if !observation.controls_armed {
                "Release controls to continue".to_owned()
            } else if let Some(message) = recovery_message {
                message
            } else if !observation.ship_available {
                "Vehicle lost; restart".to_owned()
            } else {
                match observation.last_transfer {
                    TransferResult::ExitBlocked => "No clear space beside the hatch",
                    TransferResult::TooFar => "Return to your own ship's hatch",
                    TransferResult::MustBeSupported => "Stand still on the ship's planet",
                    TransferResult::ShipNotSettled => "Land and settle before transfer",
                    _ if observation.location == PilotLocation::OnFoot => {
                        if observation.planet_claim.is_some() {
                            "Claim on foot; B at your hatch"
                        } else {
                            "Walk to terminal; B at hatch"
                        }
                    }
                    _ if observation.landing.phase == LandingPhase::Landed => {
                        "B: exit your landed ship"
                    }
                    _ => "Land rear-first; settle to exit",
                }
                .to_owned()
            },
            CYAN,
        ),
        (-0.345, objective, LIGHT),
        (-0.405, progress, AMBER),
        (
            -0.46,
            observation.mining.as_ref().map_or(detail, |mining| {
                if matches!(observation.location, PilotLocation::Aboard(_)) {
                    if state.combat_enabled() {
                        return "RT: laser  X: missiles  RB: cruise".to_owned();
                    }
                    return "X: asteroid / RB+X: heavy".to_owned();
                }
                format!(
                    "Cut {} / removed {} / RT: mine Y: size",
                    if mining.radius == 0 {
                        "1 cell".to_owned()
                    } else {
                        format!("r{}", mining.radius)
                    },
                    mining.removed_cells
                )
            }),
            color,
        ),
    ];
    for (y, label, color) in lines {
        text(
            frame,
            center + Vec2::new(0.0, y * height),
            label,
            color,
            13.0,
        );
    }
    if let Some(supply) = supply {
        let start = center + Vec2::new(-0.24, -0.434) * height;
        let end = center + Vec2::new(0.24, -0.434) * height;
        line(
            frame,
            20,
            start,
            end,
            RenderColor::rgb(0.16, 0.22, 0.28),
            height * 0.006,
        );
        line(
            frame,
            20,
            start,
            start + (end - start) * supply.energy_percent / 100.0,
            if supply.energy_percent < weapons::ROUND_ENERGY_COST {
                AMBER
            } else {
                CYAN
            },
            height * 0.006,
        );
        for quarter in 1..4 {
            let at = start + (end - start) * quarter as f32 / 4.0;
            line(
                frame,
                20,
                at - Vec2::Y * height * 0.005,
                at + Vec2::Y * height * 0.005,
                LIGHT,
                height * 0.0015,
            );
        }
    }
}
