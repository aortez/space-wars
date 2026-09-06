use engine_common::{
    Camera2, Fill, RenderCircle, RenderColor, RenderFrame, RenderLine, RenderPoint, RenderPolygon,
    RenderPrimitive, RenderText, Stroke, TextAnchor,
};
use engine_core::Vec2;
use engine_rapier::spaceling::SpacelingBalance;

use crate::{BUMPS, FIXED_HZ, SpacelingLabState};

const TERRAIN: RenderColor = RenderColor::rgb(0.09, 0.16, 0.23);
const OUTLINE: RenderColor = RenderColor::rgb(0.28, 0.51, 0.6);
const SUIT: RenderColor = RenderColor::rgb(1.0, 0.58, 0.23);
const LIGHT: RenderColor = RenderColor::rgb(0.86, 0.93, 0.97);
const CYAN: RenderColor = RenderColor::rgb(0.25, 0.93, 0.8);
const RED: RenderColor = RenderColor::rgb(1.0, 0.3, 0.3);
const YELLOW: RenderColor = RenderColor::rgb(1.0, 0.85, 0.3);

pub(super) fn frame(state: &SpacelingLabState) -> RenderFrame {
    let spaceling = state.spaceling_snapshot();
    let center = spaceling.motion.position;
    let mut frame = RenderFrame::new(Camera2::new(point(center), 24.0));
    frame.push_primitive(
        -20,
        RenderPrimitive::Circle(RenderCircle {
            center: point(state.planet.center),
            radius: state.planet.radius,
            fill: Some(Fill::new(TERRAIN)),
            stroke: Some(Stroke::new(OUTLINE, 1.5)),
        }),
    );
    for bump in BUMPS {
        let angle = state.planet.angle + bump.surface_angle;
        let origin = state.planet.center
            + Vec2::from_radians(angle) * bump.radial_center_distance(state.planet.radius);
        let points = bump
            .local_vertices()
            .map(|p| point(origin + p.rotate_radians(angle - std::f32::consts::FRAC_PI_2)))
            .to_vec();
        frame.push_primitive(
            -19,
            RenderPrimitive::Polygon(RenderPolygon {
                points,
                fill: Some(Fill::new(RenderColor::rgb(0.37, 0.28, 0.19))),
                stroke: Some(Stroke::new(SUIT, 1.2)),
            }),
        );
    }
    // Planet-local marks make support rotation visible without terrain textures.
    for index in 0..40 {
        let angle = state.planet.angle + index as f32 * std::f32::consts::TAU / 40.0;
        let normal = Vec2::from_radians(angle);
        line(
            &mut frame,
            -18,
            state.planet.center + normal * (state.planet.radius - 0.7),
            state.planet.center + normal * (state.planet.radius - 0.15),
            OUTLINE,
            1.0,
        );
    }

    let local = |x, y| center + Vec2::new(x, y).rotate_radians(spaceling.motion.angle);
    let suit = match spaceling.balance {
        SpacelingBalance::Balanced => SUIT,
        SpacelingBalance::KnockedDown => RED,
        SpacelingBalance::Recovering => YELLOW,
    };
    let stride = if spaceling.balance != SpacelingBalance::Balanced {
        0.2
    } else if spaceling.grounded() {
        state.gait_phase.sin()
            * (spaceling.relative_speed.abs() / state.spaceling_spec.walk_speed).min(1.0)
    } else {
        0.45
    };
    for side in [-1.0, 1.0] {
        let swing = stride * side;
        let hip = local(side * 0.1, -0.15);
        let knee = local(side * 0.12 + swing * 0.18, -0.48);
        let foot = local(side * 0.12 + swing * 0.3, -0.82 + swing.abs() * 0.1);
        limb(&mut frame, hip, knee, suit, 0.13);
        limb(&mut frame, knee, foot, LIGHT, 0.12);
        limb(
            &mut frame,
            foot,
            foot + local(state.facing * 0.12, 0.0) - center,
            LIGHT,
            0.13,
        );
        limb(
            &mut frame,
            local(side * 0.22, 0.32),
            local(side * 0.36 - swing * 0.12, -0.1),
            suit,
            0.12,
        );
    }
    frame.push_primitive(
        2,
        RenderPrimitive::Polygon(RenderPolygon {
            points: [(-0.23, -0.2), (0.23, -0.2), (0.26, 0.4), (-0.26, 0.4)]
                .map(|(x, y)| point(local(x, y)))
                .to_vec(),
            fill: Some(Fill::new(suit)),
            stroke: Some(Stroke::new(LIGHT, 1.2)),
        }),
    );
    circle(&mut frame, 3, local(0.0, 0.63), 0.25, LIGHT);
    circle(
        &mut frame,
        4,
        local(state.facing * 0.1, 0.65),
        0.14,
        TERRAIN,
    );

    if let Some(support) = spaceling.support {
        circle(&mut frame, 5, support.position, 0.09, CYAN);
        line(
            &mut frame,
            5,
            support.position,
            support.position + support.normal * 0.8,
            CYAN,
            1.5,
        );
    }
    if state.gravity.length_squared() > 0.001 {
        let start = center + Vec2::new(1.3, 0.6);
        let end = start + state.gravity.normalized() * 1.1;
        line(&mut frame, 5, start, end, SUIT, 1.5);
        circle(&mut frame, 5, end, 0.07, SUIT);
    }
    if let Some(shove) = state
        .last_shove
        .filter(|shove| state.tick.saturating_sub(shove.tick) < u64::from(FIXED_HZ) / 2)
    {
        circle(&mut frame, 5, shove.point, 0.12, RED);
        line(
            &mut frame,
            5,
            shove.point,
            shove.point + shove.velocity_delta * 0.25,
            RED,
            3.0,
        );
    }

    text(
        &mut frame,
        center + Vec2::new(0.0, 10.5),
        "SPACELING LAB",
        LIGHT,
        20.0,
    );
    text(
        &mut frame,
        center + Vec2::new(0.0, 9.6),
        "D-pad / A,D / arrows: walk    A / Space: jump",
        LIGHT,
        14.0,
    );
    text(
        &mut frame,
        center + Vec2::new(0.0, 8.8),
        "B / X: test shove    Start / Esc: pause    R: restart",
        LIGHT,
        14.0,
    );
    let balance = match spaceling.balance {
        SpacelingBalance::Balanced => "BALANCED",
        SpacelingBalance::KnockedDown => "KNOCKED DOWN",
        SpacelingBalance::Recovering => "RECOVERING",
    };
    text(
        &mut frame,
        center + Vec2::new(0.0, -8.3),
        format!(
            "{balance}    recovery {:.0}%    knockdowns {}    recoveries {}    shoves {}",
            spaceling.recovery_progress * 100.0,
            spaceling.knockdowns,
            spaceling.recoveries,
            state.shoves,
        ),
        suit,
        16.0,
    );
    let status = if spaceling.grounded() {
        "GROUNDED"
    } else {
        "AIRBORNE"
    };
    let support = spaceling.support.map_or_else(
        || "none".into(),
        |contact| {
            format!(
                "{}:{}",
                contact.collider.entity.value(),
                contact.collider.part
            )
        },
    );
    text(
        &mut frame,
        center + Vec2::new(0.0, -9.3),
        format!(
            "{status}    support {support}    contacts {}    relative speed {:+.2}",
            spaceling.contacts, spaceling.relative_speed,
        ),
        CYAN,
        16.0,
    );
    let air = if spaceling.grounded() {
        state.last_airtime_ticks
    } else {
        state.airborne_ticks
    };
    text(
        &mut frame,
        center + Vec2::new(0.0, -10.3),
        format!(
            "gravity {:.1}    jumps {}    landings {}    air {:.2}s    planet {:.3} rad/s",
            state.gravity.length(),
            spaceling.jumps,
            state.landings,
            air as f32 / FIXED_HZ as f32,
            state.config.planet_angular_velocity,
        ),
        LIGHT,
        14.0,
    );
    frame
}

fn point(value: Vec2) -> RenderPoint {
    RenderPoint::new(value.x, value.y)
}

// Stroke widths are screen pixels. Use filled geometry for limb thickness in
// world units so the silhouette survives both zoom and either renderer.
fn limb(frame: &mut RenderFrame, start: Vec2, end: Vec2, color: RenderColor, width: f32) {
    let direction = (end - start).normalized();
    let offset = Vec2::new(-direction.y, direction.x) * (width * 0.5);
    frame.push_primitive(
        1,
        RenderPrimitive::Polygon(RenderPolygon::filled(
            [start + offset, end + offset, end - offset, start - offset]
                .map(point)
                .to_vec(),
            color,
        )),
    );
    circle(frame, 1, start, width * 0.5, color);
    circle(frame, 1, end, width * 0.5, color);
}

fn circle(frame: &mut RenderFrame, layer: i32, center: Vec2, radius: f32, color: RenderColor) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Circle(RenderCircle::filled(point(center), radius, color)),
    );
}

fn line(
    frame: &mut RenderFrame,
    layer: i32,
    start: Vec2,
    end: Vec2,
    color: RenderColor,
    width: f32,
) {
    frame.push_primitive(
        layer,
        RenderPrimitive::Line(RenderLine::new(
            point(start),
            point(end),
            Stroke::new(color, width),
        )),
    );
}

fn text(
    frame: &mut RenderFrame,
    position: Vec2,
    value: impl Into<String>,
    color: RenderColor,
    size: f32,
) {
    let mut text = RenderText::new(point(position), value);
    text.anchor = TextAnchor::Center;
    text.color = color;
    text.size = size;
    frame.push_primitive(10, RenderPrimitive::Text(text));
}
