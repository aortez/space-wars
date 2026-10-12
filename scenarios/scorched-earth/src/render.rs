use super::*;
use engine_common::{
    Fill, RenderCircle, RenderColor, RenderLine, RenderPolygon, RenderPrimitive, RenderText,
    Stroke, TextAnchor,
};

const INK: RenderColor = RenderColor::rgb(0.91, 0.94, 0.89);
const TEAMS: [RenderColor; 2] = [
    RenderColor::rgb(0.38, 0.81, 0.67),
    RenderColor::rgb(0.96, 0.53, 0.31),
];
fn point(p: Vec2) -> RenderPoint {
    RenderPoint::new(p.x, p.y)
}
fn polygon(
    frame: &mut RenderFrame,
    layer: i32,
    vertices: impl IntoIterator<Item = Vec2>,
    color: RenderColor,
) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Polygon(RenderPolygon {
            points: vertices.into_iter().map(point).collect(),
            fill: Some(Fill::new(color)),
            stroke: None,
        }),
    );
}
fn rect(frame: &mut RenderFrame, layer: i32, low: Vec2, high: Vec2, color: RenderColor) {
    polygon(
        frame,
        layer,
        [
            low,
            Vec2::new(high.x, low.y),
            high,
            Vec2::new(low.x, high.y),
        ],
        color,
    );
}
fn text(
    frame: &mut RenderFrame,
    at: Vec2,
    value: impl Into<String>,
    size: f32,
    color: RenderColor,
) {
    frame.push_primitive(
        20,
        RenderPrimitive::Text(RenderText {
            position: point(at),
            text: value.into(),
            size,
            color,
            anchor: TextAnchor::Center,
        }),
    );
}
fn line(frame: &mut RenderFrame, a: Vec2, b: Vec2, color: RenderColor, width: f32) {
    line_at(frame, 5, a, b, color, width);
}
fn line_at(frame: &mut RenderFrame, layer: i32, a: Vec2, b: Vec2, color: RenderColor, width: f32) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Line(RenderLine::new(
            point(a),
            point(b),
            Stroke::new(color, width),
        )),
    );
}
fn circle(frame: &mut RenderFrame, at: Vec2, radius: f32, color: RenderColor) {
    frame.push_primitive(
        6,
        RenderPrimitive::Circle(RenderCircle {
            center: point(at),
            radius,
            fill: Some(Fill::new(color)),
            stroke: None,
        }),
    );
}
fn button_center(index: usize) -> Vec2 {
    if index == 20 {
        Vec2::new(41.25, 19.0)
    } else if index >= 16 {
        Vec2::new((index as f32 - 17.5) * 22.0, -30.0)
    } else if index >= 11 {
        Vec2::new((index as f32 - 13.5) * 16.5, 19.0)
    } else {
        Vec2::new((index as f32 - 5.0) * 8.5, -26.0)
    }
}
fn button_width(index: usize) -> f32 {
    if index >= 11 { 8.0 } else { 4.0 }
}
pub(super) fn button_at(at: Vec2) -> Option<Command> {
    Command::ALL.into_iter().enumerate().find_map(|(i, c)| {
        let d = at - button_center(i);
        (d.x.abs() <= button_width(i) && d.y.abs() <= 1.8).then_some(c)
    })
}

pub(super) fn frame(state: &ScorchedState) -> RenderFrame {
    let mut frame = RenderFrame::new(state.camera());
    rect(
        &mut frame,
        -30,
        Vec2::new(-1000.0, -1000.0),
        Vec2::new(1000.0, 1000.0),
        RenderColor::rgb(0.055, 0.085, 0.12),
    );
    // Distant hills are presentation only. Foreground material below is drawn
    // from the same interpolated geometry that supplies collision shapes.
    // The raster backend requires convex polygons: emit one trapezoid per ridge segment.
    let ridge = |x: f32| Vec2::new(x, 5.0 + 3.0 * (x * 0.16).sin() + 2.0 * (x * 0.37).cos());
    for i in 0..26 {
        let x = -65.0 + i as f32 * 5.0;
        polygon(
            &mut frame,
            -25,
            [
                Vec2::new(x, -32.0),
                Vec2::new(x + 5.0, -32.0),
                ridge(x + 5.0),
                ridge(x),
            ],
            RenderColor::rgb(0.10, 0.17, 0.21),
        );
    }
    for body in &state.terrain {
        let motion = state.physics.motion(body.assembly.body()).unwrap();
        for chunk in body.geometry.chunks() {
            for r in &chunk.rectangles {
                let center = r.local_center(&body.terrain);
                let half = r.half_extents(&body.terrain);
                polygon(
                    &mut frame,
                    -5,
                    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(x, y)| {
                        motion.position
                            + (center + Vec2::new(x * half.x, y * half.y))
                                .rotate_radians(motion.angle)
                    }),
                    RenderColor::rgb(0.38, 0.30, 0.20),
                );
            }
            for poly in &chunk.polygons {
                polygon(
                    &mut frame,
                    -5,
                    poly.vertices
                        .iter()
                        .map(|p| motion.position + p.rotate_radians(motion.angle)),
                    RenderColor::rgb(0.38, 0.30, 0.20),
                );
            }
        }
    }
    for grain in state.loose.iter() {
        let m = state.physics.motion(grain.body()).unwrap();
        let color = RenderColor::rgb(0.77, 0.59, 0.34);
        if let Some(vertices) = grain.shape().vertices(grain.radius()) {
            polygon(
                &mut frame,
                2,
                vertices
                    .into_iter()
                    .map(|p| m.position + p.rotate_radians(m.angle)),
                color,
            );
        } else {
            circle(&mut frame, m.position, grain.radius(), color);
        }
    }
    if let Some(course) = state.config.course {
        let finish = Vec2::new(course.finish_x(), course.floor(course.finish_x()) + 0.3);
        line(&mut frame, finish, finish + Vec2::Y * 5.0, TEAMS[0], 1.5);
        text(
            &mut frame,
            finish + Vec2::new(0.0, 6.0),
            "FINISH",
            10.0,
            TEAMS[0],
        );
    }
    for (i, tank) in state.tanks.iter().enumerate() {
        let m = state.physics.motion(tank.body).unwrap();
        let local = |x, y| m.position + Vec2::new(x, y).rotate_radians(m.angle);
        let color = if tank.health > 0.0 {
            TEAMS[i]
        } else {
            RenderColor::rgb(0.24, 0.25, 0.26)
        };
        if let Some(gear) = &state.running_gear[i] {
            let spec = suspension::spec(state.config.wheel_tuning);
            let snapshot = gear.snapshot(&state.physics).unwrap();
            polygon(
                &mut frame,
                4,
                [
                    local(-spec.chassis_half_width, -spec.chassis_half_height),
                    local(spec.chassis_half_width, -spec.chassis_half_height),
                    local(spec.chassis_half_width, spec.chassis_half_height),
                    local(-spec.chassis_half_width, spec.chassis_half_height),
                ],
                color,
            );
            for (wheel, anchor) in snapshot.wheels.iter().zip(snapshot.suspension_anchors) {
                line(&mut frame, anchor, wheel.position, INK, 1.5);
                frame.push_primitive(
                    6,
                    RenderPrimitive::Circle(RenderCircle {
                        center: point(wheel.position),
                        radius: spec.wheel_radius,
                        fill: Some(Fill::new(RenderColor::rgb(0.08, 0.11, 0.12))),
                        stroke: Some(Stroke::new(RenderColor::rgb(0.35, 0.43, 0.43), 1.0)),
                    }),
                );
                for spoke in 0..3 {
                    let direction = Vec2::from_radians(
                        wheel.angle + spoke as f32 * std::f32::consts::TAU / 3.0,
                    );
                    line_at(
                        &mut frame,
                        7,
                        wheel.position,
                        wheel.position + direction * spec.wheel_radius * 0.75,
                        color,
                        1.0,
                    );
                }
                circle(&mut frame, wheel.position, 0.14, color);
            }
        } else {
            polygon(
                &mut frame,
                4,
                [
                    local(-1.7, -0.65),
                    local(1.7, -0.65),
                    local(1.9, 0.0),
                    local(1.4, 0.6),
                    local(-1.4, 0.6),
                    local(-1.9, 0.0),
                ],
                color,
            );
            for x in [-1.2, -0.4, 0.4, 1.2] {
                circle(
                    &mut frame,
                    local(x, -0.5),
                    0.30,
                    RenderColor::rgb(0.08, 0.11, 0.12),
                );
            }
        }
        let (pivot, muzzle) = state.muzzle(i);
        circle(&mut frame, pivot, 0.7, color);
        line(&mut frame, pivot, muzzle, color, 5.0);
        text(
            &mut frame,
            m.position
                + if state.config.roof && m.position.y > 10.0 {
                    Vec2::new(-5.0, 1.0)
                } else {
                    Vec2::Y * 4.5
                },
            format!(
                "{} · {:.0}",
                if state.config.course.is_some() && i == 1 {
                    "PARKED"
                } else if i == state.selected && !state.config.demo {
                    "YOU"
                } else {
                    "CPU"
                },
                tank.health
            ),
            12.0,
            color,
        );
    }
    for shell in &state.shells {
        line(
            &mut frame,
            shell.position - shell.velocity.normalized() * 1.4,
            shell.position,
            TEAMS[shell.owner],
            1.6,
        );
        circle(&mut frame, shell.position, 0.19, INK);
    }
    for (position, tick) in &state.flashes {
        let t = (state.tick - tick) as f32 / 22.0;
        frame.push_primitive(
            8,
            RenderPrimitive::Circle(RenderCircle {
                center: point(*position),
                radius: 0.5 + t * BLAST_RADIUS,
                fill: None,
                stroke: Some(Stroke::new(
                    RenderColor::rgba(1.0, 0.78, 0.35, 1.0 - t),
                    2.0,
                )),
            }),
        );
    }
    text(
        &mut frame,
        Vec2::new(0.0, 28.0),
        "SCORCHED EARTH",
        24.0,
        INK,
    );
    let tank = state.tanks[state.selected];
    let shape = if state.config.shape == GrainShape::Round {
        "Round"
    } else {
        "Angular"
    };
    text(
        &mut frame,
        Vec2::new(0.0, 24.0),
        if state.config.course.is_some() {
            format!(
                "Driving course · {} dirt · physics {}",
                shape,
                state.config.cohesion.label()
            )
        } else {
            format!(
                "Tank {} · {:.0}° · power {:.0} · {} · collapse {}",
                state.selected + 1,
                tank.elevation.to_degrees(),
                tank.speed,
                shape,
                if state.config.slumping { "ON" } else { "off" }
            )
        },
        15.0,
        TEAMS[state.selected],
    );
    let course_snapshot = state
        .config
        .course
        .map(|_| driving_lab::snapshot(state, 0, 1.0));
    let course_goal = state.course_decision.as_ref().filter(|_| {
        state.config.course.is_some()
            && state.config.course_goal_driving
            && state.config.cpu_driving
            && state.config.demo
    });
    let result = if let (Some(course), Some(drive)) = (state.config.course, &course_snapshot) {
        format!(
            "{} · {:.1} / {:.1} units · {}{}",
            course.description(),
            drive.position[0] - course::START_X,
            course.finish_x() - course::START_X,
            if drive.position[0] >= course.finish_x() {
                "FINISHED"
            } else {
                drive.driving.mode
            },
            course_goal.map_or(String::new(), |g| format!(" · retries {}/2", g.retries)),
        )
    } else if state.config.roof {
        let d = state.cohesion_diagnostics();
        format!(
            "Roof · {} shots · {} roof cracks · {} ledge breaks · {} clump breaks",
            state.shots,
            d.roof_failures,
            d.failures - d.roof_failures,
            state.impact_diagnostics().failures
        )
    } else if state.config.hillside {
        format!(
            "Hillside · {} shots · {} bank breaks · {} clump breaks",
            state.shots,
            state.cohesion_diagnostics().failures,
            state.impact_diagnostics().failures
        )
    } else if let Some(height) = state.config.drop_height {
        let d = state.impact_diagnostics();
        if state.tick <= 120 {
            format!(
                "{} drop in {:.1}s · same clump, no added kick",
                height.label(),
                (120 - state.tick) as f32 / 60.0
            )
        } else if d.failures > 0 {
            format!(
                "Impact chipped {} cells · Reset to replay",
                d.released_cells
            )
        } else if d.capacity_denials > 0 {
            "Impact retained: not enough loose-dirt capacity".into()
        } else {
            "Clump intact · compare low / high drop and weak / strong dirt".into()
        }
    } else if state.config.undercut {
        let d = state.cohesion_diagnostics();
        if state.tick <= 120 {
            format!(
                "Bank test: support cut in {:.1}s · identical cut at every strength",
                (120 - state.tick) as f32 / 60.0
            )
        } else if state.rejected_blasts > 0 {
            "Support cut refused: not enough loose-dirt capacity".into()
        } else if d.failures > 0 {
            format!(
                "Root broke · {} cells released · {} chipped on impact",
                d.released_cells,
                state.impact_diagnostics().released_cells
            )
        } else if d.held_checks > 0 {
            "Bank holds after the cut · switch strength to compare".into()
        } else if state.config.cohesion == BankStrength::Off {
            "Support removed · bank strength is off".into()
        } else {
            "Support removed · watching the ledge".into()
        }
    } else if state.config.bombardment_seconds > 0 {
        let elapsed = state.tick / u64::from(FIXED_HZ);
        let duration = u64::from(state.config.bombardment_seconds);
        if elapsed < duration {
            format!(
                "Barrage: {}s remaining · {} bank cells released",
                duration - elapsed,
                state.slumping_diagnostics().released_cells
            )
        } else {
            format!(
                "Rest: {}s · {} bank cells released · {} pending checks",
                elapsed - duration,
                state.slumping_diagnostics().released_cells,
                state.slumping_diagnostics().pending
            )
        }
    } else if let Some(winner) = state.winner() {
        format!("Tank {} wins — Reset to replay the same hills", winner + 1)
    } else if !state.fighting() {
        "Both tanks destroyed — Reset to replay".into()
    } else {
        state.status.into()
    };
    rect(
        &mut frame,
        15,
        Vec2::new(-1000.0, -1000.0),
        Vec2::new(100.0, -18.0),
        RenderColor::rgb(0.055, 0.085, 0.12),
    );
    text(&mut frame, Vec2::new(0.0, -19.6), result, 13.0, INK);
    let (events, event_label, rejections) = if state.config.drop_height.is_some() {
        let d = state.impact_diagnostics();
        (
            d.failures,
            "clump breaks",
            d.capacity_denials + d.fragment_denials,
        )
    } else {
        (state.impacts, "impacts", state.rejected_blasts)
    };
    text(
        &mut frame,
        Vec2::new(0.0, -22.3),
        if let Some(drive) = &course_snapshot {
            format!(
                "Support {} · route {}{}{}",
                if drive.supported { "yes" } else { "no" },
                drive.ground.reason.unwrap_or("clear"),
                drive.suspension_lengths.map_or(String::new(), |v| format!(
                    " · springs {:.2} / {:.2}",
                    v[0], v[1]
                )),
                course_goal.map_or(String::new(), |g| format!(
                    " · speed {:.1}/{:.1}",
                    g.actual_speed, g.desired_speed
                )),
            )
        } else {
            format!(
                "{} / {} loose  ·  {} returned  ·  {} {}  ·  {} capacity rejections",
                state.loose_cells(),
                state.config.max_grains,
                state.deposited_cells(),
                events,
                event_label,
                rejections
            )
        },
        12.0,
        INK,
    );
    for (i, label) in [
        "Angle −",
        "Angle +",
        "Power −",
        "Power +",
        "FIRE",
        "Tank",
        if state.config.demo { "Demo ON" } else { "Demo" },
        "Dirt",
        "Reset",
        if state.config.hillside || state.config.roof {
            "Physics"
        } else if state.config.undercut || state.config.drop_height.is_some() {
            "Strength"
        } else {
            "Collapse"
        },
        if let Some(course) = state.config.course {
            course.next().map_or("Duel", DrivingCourse::label)
        } else if state.config.roof {
            "Course"
        } else if state.config.hillside {
            "Roof"
        } else if state.config.drop_height == Some(DropHeight::High) {
            "Hillside"
        } else if state.config.drop_height == Some(DropHeight::Low) {
            "High drop"
        } else if state.config.undercut {
            "Low drop"
        } else if state.config.bombardment_seconds > 0 {
            "Bank test"
        } else {
            "Barrage"
        },
        if state.config.undercut
            || state.config.drop_height.is_some()
            || state.config.hillside
            || state.config.roof
        {
            "Bank test"
        } else {
            "Hills"
        },
        if !state.config.undercut
            && state.config.drop_height.is_none()
            && (state.config.slumping != (state.config.cohesion != BankStrength::Off)
                || (state.config.cohesion != BankStrength::Off && !state.config.breakup))
        {
            "Physics: custom"
        } else {
            match (
                state.config.undercut || state.config.drop_height.is_some(),
                state.config.cohesion,
            ) {
                (true, BankStrength::Off) => "Strength: off",
                (true, BankStrength::Weak) => "Strength: weak",
                (true, BankStrength::Strong) => "Strength: strong",
                (false, BankStrength::Off) => "Physics: off",
                (false, BankStrength::Weak) => "Physics: weak",
                (false, BankStrength::Strong) => "Physics: strong",
            }
        },
        match state.config.drop_height {
            None => "Drop test",
            Some(DropHeight::Low) => "Drop: low",
            Some(DropHeight::High) => "Drop: high",
        },
        "Hillside",
        "Roof",
        "Drive left",
        "Drive right",
        if state.config.cpu_driving {
            if state.config.course.is_some() {
                if state.config.course_goal_driving {
                    "Goal driver"
                } else {
                    "Range driver"
                }
            } else {
                "CPU drive ON"
            }
        } else {
            "CPU stationary"
        },
        if !suspension::enabled(state.config) && state.config.suspension {
            "Wheels: lab off"
        } else if state.config.suspension {
            "Sprung wheels"
        } else {
            "Rigid tracks"
        },
        match state.config.course {
            None => "Drive course",
            Some(DrivingCourse::Flat) => "Course: flat",
            Some(DrivingCourse::Step) => "Course: step",
            Some(DrivingCourse::Slope) => "Course: slope",
            Some(DrivingCourse::Trench) => "Course: trench",
            Some(DrivingCourse::Rubble) => "Course: rubble",
        },
    ]
    .into_iter()
    .enumerate()
    {
        let center = button_center(i);
        rect(
            &mut frame,
            18,
            center - Vec2::new(button_width(i), 1.8),
            center + Vec2::new(button_width(i), 1.8),
            if i == 4
                || (i == 9 && state.config.slumping)
                || (i == 10 && state.config.bombardment_seconds > 0)
                || (i == 11 && state.config.undercut)
                || (i == 12 && state.config.cohesion != BankStrength::Off)
                || (i == 13 && state.config.drop_height.is_some())
                || (i == 14 && state.config.hillside)
                || (i == 15 && state.config.roof)
                || (i == 18 && state.config.cpu_driving)
                || (i == 19 && suspension::enabled(state.config))
                || (i == 20 && state.config.course.is_some())
            {
                RenderColor::rgb(0.28, 0.36, 0.27)
            } else {
                RenderColor::rgb(0.14, 0.21, 0.25)
            },
        );
        text(&mut frame, center - Vec2::Y * 0.4, label, 11.0, INK);
    }
    text(
        &mut frame,
        Vec2::new(0.0, if state.config.roof { 21.8 } else { 14.5 }),
        if state.config.course.is_some() {
            "Drive to the flag · V: CPU / manual · J: next course · R: reset · Firing disabled"
        } else if state.config.roof {
            "Demo: one shot under the roof · East: physics · North: dirt · Reset: replay"
        } else if state.config.hillside {
            "Fire at the bank · Demo: three shots · East: physics · North: dirt"
        } else if state.config.undercut || state.config.drop_height.is_some() {
            "Picade: East strength · North dirt · Left shoulder next test"
        } else {
            "A/D or left stick drive · Arrows aim/power · Picade: hold Tank + joystick to drive"
        },
        11.0,
        INK,
    );
    if state.config.undercut && state.tick <= 120 {
        let color = RenderColor::rgb(0.96, 0.65, 0.28);
        for (a, b) in [
            (Vec2::new(3.5, -7.5), Vec2::new(6.5, -7.5)),
            (Vec2::new(6.5, -7.5), Vec2::new(6.5, 3.5)),
            (Vec2::new(6.5, 3.5), Vec2::new(3.5, 3.5)),
            (Vec2::new(3.5, 3.5), Vec2::new(3.5, -7.5)),
        ] {
            line(&mut frame, a, b, color, 1.5);
        }
    }
    frame
}
