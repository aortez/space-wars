use super::*;

fn ship() -> ShipState {
    ShipState::new(0, Vec2::ZERO, Color::RED, 100, 1.0 / 60.0)
}

fn landing(altitude: f32, angle: f32) -> LandingTelemetry {
    LandingTelemetry {
        planet: Some(0),
        altitude,
        angle_degrees: angle,
        ..Default::default()
    }
}

#[test]
fn gear_extends_smoothly_and_hysteresis_prevents_boundary_chatter() {
    let ship = ship();
    let mut gear = LandingGear::default();
    gear.advance(&landing(26.0, 0.0), &ship, RETRACT_SECONDS);
    assert_eq!(gear.extension(), 0.0);
    gear.advance(&landing(17.0, 0.0), &ship, EXTEND_SECONDS * 0.5);
    assert!((gear.extension() - 0.5).abs() < 1e-6);
    gear.advance(&landing(21.0, 60.0), &ship, EXTEND_SECONDS);
    assert_eq!(
        gear.extension(),
        1.0,
        "retain through height and angle deadbands"
    );
    gear.advance(&landing(25.0, 0.0), &ship, RETRACT_SECONDS * 0.5);
    assert!((gear.extension() - 0.5).abs() < 1e-6);
    gear.advance(&landing(21.0, 0.0), &ship, RETRACT_SECONDS);
    assert_eq!(
        gear.extension(),
        0.0,
        "do not redeploy inside the height deadband"
    );
    gear.advance(&landing(12.0, 60.0), &ship, EXTEND_SECONDS);
    assert_eq!(gear.extension(), 0.0, "approach must be aligned");
    gear.advance(&landing(12.0, 49.0), &ship, EXTEND_SECONDS);
    assert_eq!(gear.extension(), 1.0);
    gear.advance(&landing(12.0, 66.0), &ship, RETRACT_SECONDS);
    assert_eq!(gear.extension(), 0.0);
}

#[test]
fn cruise_retracts_but_actual_support_always_shows_fully_deployed_feet() {
    let mut ship = ship();
    ship.wings_closed = true;
    ship.wing_theta = MAX_WING_THETA;
    let mut gear = LandingGear::default();
    gear.advance(&landing(12.0, 0.0), &ship, RETRACT_SECONDS);
    assert_eq!(gear.extension(), 0.0);
    ship.thrust = 1.0;
    gear.advance(
        &LandingTelemetry {
            supported_feet: 1,
            ..landing(0.0, 0.0)
        },
        &ship,
        1.0 / 60.0,
    );
    assert_eq!(
        gear.extension(),
        1.0,
        "takeoff thrust must not hide loaded feet"
    );
}

#[test]
fn pause_invalid_dt_and_vehicle_changes_do_not_leave_stale_animation() {
    let mut ship = ship();
    let mut gear = LandingGear::default();
    gear.advance(&landing(26.0, 0.0), &ship, RETRACT_SECONDS * 0.5);
    let paused = gear;
    for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        gear.advance(&landing(12.0, 0.0), &ship, dt);
        assert_eq!(gear, paused);
    }
    ship.form = ShipForm::EscapePod;
    gear.advance(&landing(26.0, 0.0), &ship, 1.0 / 60.0);
    assert_eq!(gear, LandingGear::default());
    ship.form = ShipForm::Ship;
    gear.advance(&landing(26.0, 0.0), &ship, RETRACT_SECONDS);
    ship.dead = true;
    gear.advance(&landing(26.0, 0.0), &ship, 1.0 / 60.0);
    assert_eq!(gear, LandingGear::default());
}

#[test]
fn deployed_pads_match_physical_contacts_at_every_rotation_and_pods_keep_fixed_feet() {
    for form in [ShipForm::Ship, ShipForm::EscapePod] {
        for angle in [0.0, 0.7, 2.8] {
            let mut ship = ship();
            ship.form = form;
            ship.position = Vec2::new(10.0, -20.0);
            ship.rotation_radians = angle;
            let mut frame = RenderFrame::default();
            render::draw_landing_gear(
                &mut frame,
                &ship,
                if form == ShipForm::Ship { 1.0 } else { 0.0 },
                true,
            );
            let pads: Vec<_> = frame
                .layers
                .iter()
                .flat_map(|layer| &layer.primitives)
                .filter_map(|p| {
                    if let RenderPrimitive::Circle(pad) = p {
                        Some(pad)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(pads.len(), 2);
            let (feet, radius) = physics::surface_landing_geometry(form);
            for (pad, foot) in pads.into_iter().zip(feet) {
                let expected =
                    ship.position + physics::ship_pivot(form) + foot.rotate_radians(angle);
                assert!((Vec2::new(pad.center.x, pad.center.y) - expected).length() < 1e-5);
                assert_eq!(pad.radius, radius);
            }
        }
    }
    let mut frame = RenderFrame::default();
    render::draw_landing_gear(&mut frame, &ship(), 0.0, false);
    assert!(frame.layers.is_empty());
}
