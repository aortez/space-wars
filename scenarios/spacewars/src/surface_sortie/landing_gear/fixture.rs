//! Prescribed gear transitions through the production animation and renderer.
//! These captures are not a second simulation or a new launcher scenario.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Flight,
    Extending,
    Approach,
    Landed,
    Retracting,
    Cruise,
}

impl Case {
    pub const ALL: [Self; 6] = [
        Self::Flight,
        Self::Extending,
        Self::Approach,
        Self::Landed,
        Self::Retracting,
        Self::Cruise,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Flight => "flight-stowed",
            Self::Extending => "extending",
            Self::Approach => "approach-deployed",
            Self::Landed => "landed",
            Self::Retracting => "retracting",
            Self::Cruise => "cruise-stowed",
        }
    }
}

pub struct Snapshot {
    pub frame: RenderFrame,
    pub bare: RenderFrame,
    pub extension: f32,
}

pub fn capture(case: Case, camera_height: f32) -> Snapshot {
    let mut ship = ShipState::new(0, Vec2::ZERO, Color::RED, 100, 1.0 / 60.0);
    ship.enable_weapon_supply();
    let mut gear = LandingGear::default();
    let far = LandingTelemetry {
        planet: Some(0),
        altitude: 26.0,
        ..Default::default()
    };
    let near = LandingTelemetry {
        altitude: 12.0,
        ..far
    };
    gear.advance(&far, &ship, RETRACT_SECONDS);
    match case {
        Case::Flight => {}
        Case::Cruise => {
            ship.wing_theta = MAX_WING_THETA;
            ship.wings_closed = true;
        }
        Case::Extending => gear.advance(&near, &ship, EXTEND_SECONDS * 0.5),
        Case::Approach => gear.advance(&near, &ship, EXTEND_SECONDS),
        Case::Landed => gear.advance(
            &LandingTelemetry {
                phase: LandingPhase::Landed,
                supported_feet: 2,
                altitude: 0.0,
                ..near
            },
            &ship,
            1.0 / 60.0,
        ),
        Case::Retracting => {
            gear.advance(&near, &ship, EXTEND_SECONDS);
            gear.advance(&far, &ship, RETRACT_SECONDS * 0.5);
        }
    }
    let mut bare = RenderFrame::new(Camera2::new(
        render_point(SHIP_PIVOT),
        camera_height.clamp(16.0, 440.0),
    ));
    if case == Case::Landed {
        let floor = SHIP_PIVOT.y + physics::LANDING_FEET[0].y - physics::LANDING_FOOT_RADIUS;
        bare.push_primitive(
            -10,
            RenderPrimitive::Polygon(RenderPolygon {
                points: [
                    Vec2::new(-100.0, floor),
                    Vec2::new(100.0, floor),
                    Vec2::new(100.0, -100.0),
                    Vec2::new(-100.0, -100.0),
                ]
                .map(render_point)
                .to_vec(),
                fill: Some(Fill::new(RenderColor::rgb(0.2, 0.28, 0.3))),
                stroke: None,
            }),
        );
    }
    render_ship(&mut bare, &ship);
    let mut frame = bare.clone();
    render::draw_landing_gear(&mut frame, &ship, gear.extension(), case == Case::Landed);
    Snapshot {
        frame,
        bare,
        extension: gear.extension(),
    }
}

/// Land and disembark using actual scenario actions, for full-scene screenshots
/// of deployed gear and an unoccupied ship with no ornamental thruster output.
pub fn gameplay_captures() -> [(&'static str, RenderFrame); 2] {
    let mut state = SurfaceSortieScenario::init(SurfaceMotionPreset::Stationary, 7);
    let dt = Duration::from_nanos(16_666_667);
    for _ in 0..120 {
        SurfaceSortieScenario::step(&mut state, &[], dt);
    }
    assert!(state.vehicle_settled(0));
    let aboard = SurfaceSortieScenario::render_frame(&state);
    SurfaceSortieScenario::step(
        &mut state,
        &[SurfaceSortieAction {
            interact_held: true,
            ..Default::default()
        }
        .encode(PlayerId::PLAYER_1)],
        dt,
    );
    for _ in 0..120 {
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
            dt,
        );
    }
    assert!(state.pilots[0].body.is_some());
    let effects = state.world.ships[0].thrusters.as_ref().unwrap();
    assert_eq!(effects.output, thrusters::ThrusterOutput::default());
    assert_eq!(effects.trail_count(), 0);
    [
        ("gameplay-landed", aboard),
        (
            "gameplay-on-foot",
            SurfaceSortieScenario::render_frame(&state),
        ),
    ]
}
