//! Initial Spacewars scenario port.
//!
//! M9 intentionally keeps gameplay shallow: deterministic initial state, fixed
//! timestep ownership, and render primitive emission for two ships. Flight,
//! input, planets, weapons, collisions, sounds, and asteroids land in later
//! slices.

use std::time::Duration;

use engine_common::{
    Action, Camera2, Fill, Observation, RenderCircle, RenderColor, RenderFrame, RenderPoint,
    RenderPolygon, RenderPrimitive, RenderText, Scenario, StepResult, Stroke, TextAnchor,
    TickModel,
};
use engine_core::{Color, PlayerConfig, SpacewarsConfig, Transform2, Vec2};

const WORLD_LAYER: i32 = -20;
const SHIP_LAYER: i32 = 0;
const LABEL_LAYER: i32 = 10;

const SHIP_BODY: [Vec2; 3] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(5.0, 0.0),
    Vec2::new(2.5, 7.0),
];
const SHIP_WING_MOUNT: [Vec2; 3] = [
    Vec2::new(2.5, 5.5),
    Vec2::new(0.77, 2.5),
    Vec2::new(4.23, 2.5),
];
const SHIP_THRUSTER: [Vec2; 3] = [
    Vec2::new(0.0, -1.0),
    Vec2::new(2.5, 0.0),
    Vec2::new(5.0, -1.0),
];
const SHIP_LASER: [Vec2; 3] = [
    Vec2::new(2.0, 6.0),
    Vec2::new(2.5, 7.0),
    Vec2::new(3.0, 6.0),
];
const SHIP_LEFT_WING: [Vec2; 3] = [
    Vec2::new(2.5, 2.0),
    Vec2::new(1.0, -0.5),
    Vec2::new(-3.0, 2.0),
];
const SHIP_RIGHT_WING: [Vec2; 3] = [
    Vec2::new(2.5, 2.0),
    Vec2::new(4.0, -0.5),
    Vec2::new(8.0, 2.0),
];
const SHIP_PIVOT: Vec2 = Vec2::new(2.5, 3.5);

pub struct SpacewarsScenario;

#[derive(Debug, Clone)]
pub struct SpacewarsState {
    pub config: SpacewarsConfig,
    pub seed: u64,
    pub tick: u64,
    pub players: [PlayerState; 2],
    pub ships: [ShipState; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    pub id: usize,
    pub name: String,
    pub health_percent: u32,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShipState {
    pub owner_id: usize,
    pub position: Vec2,
    pub rotation_radians: f32,
    pub color: Color,
}

impl Scenario for SpacewarsScenario {
    type State = SpacewarsState;
    type Config = SpacewarsConfig;

    fn init(config: Self::Config, seed: u64) -> Self::State {
        let players = [
            player_state(0, &config.players[0]),
            player_state(1, &config.players[1]),
        ];
        let ships = [
            ShipState {
                owner_id: 0,
                position: Vec2::new(375.0, 450.0),
                rotation_radians: 0.0,
                color: config.players[0].color,
            },
            ShipState {
                owner_id: 1,
                position: Vec2::new(375.0, 500.0),
                rotation_radians: 0.0,
                color: config.players[1].color,
            },
        ];

        SpacewarsState {
            config,
            seed,
            tick: 0,
            players,
            ships,
        }
    }

    fn step(state: &mut Self::State, _actions: &[Action], _dt: Duration) -> StepResult {
        state.tick += 1;
        StepResult::default()
    }

    fn observe(_state: &Self::State) -> Observation {
        Observation {
            payload: Vec::new(),
        }
    }

    fn render_frame(state: &Self::State) -> RenderFrame {
        render_state(state)
    }

    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: 60 }
    }
}

fn player_state(id: usize, config: &PlayerConfig) -> PlayerState {
    PlayerState {
        id,
        name: config.name.clone(),
        health_percent: config.health_percent,
        color: config.color,
    }
}

fn render_state(state: &SpacewarsState) -> RenderFrame {
    let radius = state.config.universe_radius as f32;
    let center = Vec2::new(radius, radius);
    let mut frame = RenderFrame::new(Camera2::new(render_point(center), radius * 2.2));

    frame.push_primitive(
        WORLD_LAYER,
        RenderPrimitive::Circle(RenderCircle {
            center: render_point(center),
            radius,
            fill: None,
            stroke: Some(Stroke::new(RenderColor::rgba(0.45, 0.5, 0.56, 0.75), 2.0)),
        }),
    );

    for ship in &state.ships {
        render_ship(&mut frame, ship);
    }

    for ship in &state.ships {
        render_ship_label(&mut frame, state, ship);
    }

    frame
}

fn render_ship(frame: &mut RenderFrame, ship: &ShipState) {
    let transform = Transform2 {
        translation: ship.position,
        scale: Vec2::splat(1.0),
        rotation_radians: ship.rotation_radians,
        pivot: SHIP_PIVOT,
    };
    let base = render_color(ship.color);
    let outline = RenderColor::rgba(0.02, 0.02, 0.03, 0.9);

    push_filled_polygon(frame, transform, &SHIP_LEFT_WING, dim(base, 0.72), outline);
    push_filled_polygon(frame, transform, &SHIP_RIGHT_WING, dim(base, 0.72), outline);
    push_filled_polygon(
        frame,
        transform,
        &SHIP_WING_MOUNT,
        RenderColor::rgba(10.0 / 255.0, 180.0 / 255.0, 50.0 / 255.0, 1.0),
        outline,
    );
    push_filled_polygon(frame, transform, &SHIP_THRUSTER, dim(base, 0.58), outline);
    push_filled_polygon(frame, transform, &SHIP_BODY, base, outline);
    push_filled_polygon(frame, transform, &SHIP_LASER, dim(base, 1.15), outline);
}

fn render_ship_label(frame: &mut RenderFrame, state: &SpacewarsState, ship: &ShipState) {
    let player = &state.players[ship.owner_id];
    let mut text = RenderText::new(
        render_point(ship.position + Vec2::new(2.5, 18.0)),
        player.name.as_str(),
    );
    text.color = render_color(player.color);
    text.size = 14.0;
    text.anchor = TextAnchor::Center;
    frame.push_primitive(LABEL_LAYER, RenderPrimitive::Text(text));
}

fn push_filled_polygon(
    frame: &mut RenderFrame,
    transform: Transform2,
    points: &[Vec2],
    fill: RenderColor,
    outline: RenderColor,
) {
    frame.push_primitive(
        SHIP_LAYER,
        RenderPrimitive::Polygon(RenderPolygon {
            points: points
                .iter()
                .map(|point| render_point(transform.transform_point(*point)))
                .collect(),
            fill: Some(Fill::new(fill)),
            stroke: Some(Stroke::new(outline, 0.75)),
        }),
    );
}

fn render_point(point: Vec2) -> RenderPoint {
    RenderPoint::new(point.x, point.y)
}

fn render_color(color: Color) -> RenderColor {
    RenderColor::rgba(color.r, color.g, color.b, color.a)
}

fn dim(color: RenderColor, scale: f32) -> RenderColor {
    RenderColor::rgba(
        (color.r * scale).clamp(0.0, 1.0),
        (color.g * scale).clamp(0.0, 1.0),
        (color.b * scale).clamp(0.0, 1.0),
        color.a,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_deathmatch() -> SpacewarsState {
        SpacewarsScenario::init(SpacewarsConfig::deathmatch(), 123)
    }

    #[test]
    fn init_builds_original_two_ship_starting_positions() {
        let state = init_deathmatch();

        assert_eq!(state.seed, 123);
        assert_eq!(state.tick, 0);
        assert_eq!(state.config.universe_radius, 300);
        assert_eq!(state.ships[0].owner_id, 0);
        assert_eq!(state.ships[0].position, Vec2::new(375.0, 450.0));
        assert_eq!(state.ships[1].owner_id, 1);
        assert_eq!(state.ships[1].position, Vec2::new(375.0, 500.0));
        assert_eq!(state.players[0].name, "Player 1");
        assert_eq!(state.players[1].name, "Player 2");
    }

    #[test]
    fn step_advances_tick_without_moving_ships_in_m9() {
        let mut state = init_deathmatch();
        let start = state.ships;

        let result = SpacewarsScenario::step(&mut state, &[], Duration::from_secs_f32(1.0 / 60.0));

        assert!(!result.terminated);
        assert_eq!(state.tick, 1);
        assert_eq!(state.ships, start);
    }

    #[test]
    fn render_frame_contains_world_two_ships_and_labels() {
        let state = init_deathmatch();
        let frame = SpacewarsScenario::render_frame(&state);

        let circles = frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Circle(_)))
            .count();
        let polygons = frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Polygon(_)))
            .count();
        let text = frame
            .layers
            .iter()
            .flat_map(|layer| &layer.primitives)
            .filter(|primitive| matches!(primitive, RenderPrimitive::Text(_)))
            .count();

        assert_eq!(frame.camera.center, RenderPoint::new(300.0, 300.0));
        assert_eq!(circles, 1);
        assert_eq!(polygons, 12);
        assert_eq!(text, 2);
    }
}
