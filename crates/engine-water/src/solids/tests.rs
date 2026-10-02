use super::*;
use crate::tests::{DT, assert_accounting, spec};
use crate::{Boundary, Parcel, WaterConfig};

fn world() -> WaterWorld {
    WaterWorld::new(
        WaterConfig {
            exit_y: -120.0,
            ..WaterConfig::default()
        },
        vec![spec(100.0, 10.0, vec![-100.0], [Boundary::Closed; 2])],
    )
    .unwrap()
}

fn drop(position: Vec2, velocity: Vec2) -> Parcel {
    Parcel {
        position,
        velocity,
        volume: 2.0,
        duration: DT,
        horizontal_bounds: None,
    }
}

#[test]
fn fast_parcels_cannot_cross_thin_walls_and_slide_to_the_exit() {
    for hz in [30, 60, 120] {
        for sign in [-1.0, 1.0] {
            let mut w = world();
            w.set_solid_boxes(&[SolidBox::new(
                Vec2::new(3.0 * sign, -30.0),
                Vec2::new(0.1, 40.0),
                0.0,
            )
            .unwrap()])
                .unwrap();
            w.add_falling(drop(Vec2::new(0.0, -10.0), Vec2::new(900.0 * sign, -40.0)))
                .unwrap();
            let parcels_ptr = w.parcels.as_ptr();
            let solids_ptr = w.solids.as_ptr();
            w.step(1.0 / f64::from(hz)).unwrap();
            let p = w.parcels()[0];
            assert!(p.position.x * sign < 2.9);
            assert_eq!(p.velocity.x, 0.0);
            assert!(p.velocity.y < -40.0);
            for _ in 0..hz * 2 {
                w.step(1.0 / f64::from(hz)).unwrap();
                assert_accounting(&w);
                assert_eq!(w.parcels.as_ptr(), parcels_ptr);
                assert_eq!(w.solids.as_ptr(), solids_ptr);
            }
            assert_eq!(w.stats().drained, 2.0);
        }
    }
}

#[test]
fn finite_walls_allow_flight_below_them_but_stop_rising_water_at_the_underside() {
    let mut w = world();
    w.set_solid_boxes(&[SolidBox::new(Vec2::new(0.0, 5.0), Vec2::new(2.0, 0.1), 0.0).unwrap()])
        .unwrap();
    w.add_falling(drop(Vec2::new(-10.0, -10.0), Vec2::new(900.0, 0.0)))
        .unwrap();
    w.add_falling(drop(Vec2::ZERO, Vec2::new(0.0, 900.0)))
        .unwrap();
    w.step(DT).unwrap();
    assert!(
        w.parcels()[0].position.x > 2.0,
        "free flight below a finite panel"
    );
    assert!(w.parcels()[1].position.y < 4.9);
    assert_eq!(w.parcels()[1].velocity.y, 0.0);
    assert_accounting(&w);
}

#[test]
fn floor_top_collects_water_instead_of_suspending_it_on_the_contact_skin() {
    for bed in [0.0, -195.25] {
        let mut w = WaterWorld::new(
            WaterConfig::default(),
            vec![spec(-20.0, 40.0, vec![bed], [Boundary::Closed; 2])],
        )
        .unwrap();
        w.set_solid_boxes(&[SolidBox::new(
            Vec2::new(0.0, bed as f32 - 10.0),
            Vec2::new(20.0, 10.0),
            0.0,
        )
        .unwrap()])
            .unwrap();
        w.add_falling(drop(
            Vec2::new(0.0, bed as f32 + 2.0),
            Vec2::new(0.0, -600.0),
        ))
        .unwrap();
        w.step(DT).unwrap();
        assert_eq!(w.stats().pooled, 2.0);
        assert_eq!(w.stats().in_flight, 0.0);
        assert_accounting(&w);
    }
}

#[test]
fn collection_respects_the_order_of_pool_and_wall_hits() {
    for pool_before_wall in [false, true] {
        let mut w = WaterWorld::new(
            WaterConfig::default(),
            vec![if pool_before_wall {
                spec(4.0, 2.0, vec![0.0], [Boundary::Closed; 2])
            } else {
                spec(8.0, 4.0, vec![-10.0], [Boundary::Closed; 2])
            }],
        )
        .unwrap();
        w.set_solid_boxes(&[
            SolidBox::new(Vec2::new(6.0, -10.0), Vec2::new(0.1, 20.0), 0.0).unwrap(),
        ])
        .unwrap();
        w.add_falling(drop(Vec2::new(0.0, 10.0), Vec2::new(600.0, -600.0)))
            .unwrap();
        w.step(DT).unwrap();
        assert_eq!(w.stats().pooled, if pool_before_wall { 2.0 } else { 0.0 });
        if !pool_before_wall {
            assert!(w.parcels()[0].position.x < 5.9);
        }
        assert_accounting(&w);
    }
}

#[test]
fn newborn_outfall_half_step_and_material_faces_also_hit_the_wall() {
    let mut w = WaterWorld::new(
        WaterConfig::default(),
        vec![spec(
            -5.0,
            5.0,
            vec![0.0],
            [Boundary::Closed, Boundary::Spill { lip: 0.0 }],
        )],
    )
    .unwrap();
    let wall = SolidBox::new(Vec2::new(0.1, -5.0), Vec2::new(0.01, 10.0), 0.0).unwrap();
    w.set_solid_boxes(&[wall]).unwrap();
    w.add_to_pool(0, -1.0, 50.0).unwrap();
    w.step(DT).unwrap();
    assert!(!w.parcels().is_empty());
    for (p, spill) in w.parcels.iter().zip(&w.spills) {
        assert!(p.position.x < 0.09, "{p:?}");
        assert_eq!(p.velocity.x, 0.0);
        let spill = spill.unwrap();
        assert!(spill.head.position.x < 0.09);
        assert!(spill.tail.position.x < 0.09);
    }
    assert_accounting(&w);
}

#[test]
fn moving_rotated_panels_eject_existing_water_and_replay_conservatively() {
    let run = || {
        let mut w = world();
        w.add_falling(drop(Vec2::new(0.0, -5.0), Vec2::new(5.0, -10.0)))
            .unwrap();
        let mut states = Vec::new();
        for tick in 0..60 {
            let angle = tick as f32 * 0.001 - 0.2;
            let center = Vec2::new(0.4 + tick as f32 * 0.01, -20.0);
            let half = Vec2::new(1.0, 30.0);
            w.set_solid_boxes(&[SolidBox::new(center, half, angle).unwrap()])
                .unwrap();
            w.step(DT).unwrap();
            for p in w.parcels() {
                let local = (p.position - center).rotate_radians(-angle);
                assert!(local.x.abs() >= half.x - 0.001 || local.y.abs() >= half.y - 0.001);
            }
            states.push((w.stats(), w.parcels().to_vec()));
            assert_accounting(&w);
        }
        states
    };
    assert_eq!(run(), run());
}

#[test]
fn invalid_solid_geometry_and_excess_capacity_leave_the_world_unchanged() {
    assert!(SolidBox::new(Vec2::ZERO, Vec2::new(-1.0, 1.0), 0.0).is_err());
    assert!(SolidBox::new(Vec2::ZERO, Vec2::new(1.0, 1.0), f32::NAN).is_err());
    assert!(SolidBox::new(Vec2::new(f32::INFINITY, 0.0), Vec2::new(1.0, 1.0), 0.0).is_err());
    let b = SolidBox::new(Vec2::ZERO, Vec2::new(1.0, 1.0), 0.0).unwrap();
    let mut w = world();
    w.set_solid_boxes(&[b]).unwrap();
    assert_eq!(
        w.set_solid_boxes(&[b; MAX_SOLID_BOXES + 1]),
        Err(WaterError::Capacity)
    );
    assert_eq!(w.solid_boxes(), &[b]);
    w.set_solid_boxes(&[]).unwrap();
    assert!(w.solid_boxes().is_empty());
}
