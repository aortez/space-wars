use super::*;
use crate::{
    Boundary, WaterError,
    tests::{DT, assert_accounting, spec},
};
use engine_core::Vec2;

fn tank(response: f64, columns: usize, depth: f64) -> WaterWorld {
    let mut water = WaterWorld::new(
        WaterConfig {
            impact_response: response,
            ..WaterConfig::default()
        },
        vec![spec(
            -52.5,
            105.0,
            vec![0.0; columns],
            [Boundary::Closed; 2],
        )],
    )
    .unwrap();
    if depth > 0.0 {
        let dx = 105.0 / columns as f64;
        for i in 0..columns {
            water
                .add_to_pool(0, -52.5 + (i as f64 + 0.5) * dx, depth * dx)
                .unwrap();
        }
    }
    water
}

fn drop(volume: f64, speed: f32, height: f32) -> Parcel {
    Parcel {
        position: Vec2::new(0.0, height),
        velocity: Vec2::new(0.0, -speed),
        volume,
        duration: DT,
        horizontal_bounds: None,
    }
}

#[test]
fn invalid_response_is_rejected_and_default_stays_disabled() {
    assert_eq!(WaterConfig::default().impact_response, 0.0);
    for value in [-0.001, 1.001, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            WaterWorld::new(
                WaterConfig {
                    impact_response: value,
                    ..WaterConfig::default()
                },
                vec![spec(0.0, 10.0, vec![0.0; 2], [Boundary::Closed; 2])]
            ),
            Err(WaterError::InvalidGeometry)
        ));
    }
}

#[test]
fn wet_impact_is_symmetric_conservative_and_changes_motion_not_injection() {
    let mut control = tank(0.0, 21, 6.0);
    let mut response = tank(0.25, 21, 6.0);
    let parcels_ptr = response.parcels.as_ptr();
    let columns_ptr = response.pools[0].volume.as_ptr();
    for water in [&mut control, &mut response] {
        water.add_falling(drop(4.0, 180.0, 7.0)).unwrap();
        water.step(DT).unwrap();
        assert_accounting(water);
    }
    assert_eq!(control.stats().impact_transfers, 0);
    assert_eq!(response.stats().impact_transfers, 1);
    assert_eq!(response.stats().injected, control.stats().injected);
    assert!(response.pools[0].velocity[10] < control.pools[0].velocity[10]);
    assert!(response.pools[0].velocity[11] > control.pools[0].velocity[11]);
    for _ in 0..600 {
        response.step(DT).unwrap();
        assert_accounting(&response);
        assert_eq!(response.parcels.as_ptr(), parcels_ptr);
        assert_eq!(response.pools[0].volume.as_ptr(), columns_ptr);
        let columns = &response.pools[0].volume;
        for (a, b) in columns.iter().zip(columns.iter().rev()) {
            assert!((a - b).abs() < 1e-8);
        }
    }
    assert_eq!(
        response.stats().impact_transfers,
        1,
        "never re-apply an old impact"
    );
}

#[test]
fn strength_and_arrival_speed_change_kick_without_changing_volume() {
    let kick = |strength, speed| {
        let mut water = tank(strength, 21, 6.0);
        water.deposit(0, 10, drop(4.0, speed, 6.0));
        (water.pools[0].velocity[11], water.pools[0].volume.clone())
    };
    let (slow, volume) = kick(0.25, 90.0);
    let (fast, other_volume) = kick(0.25, 180.0);
    let (weak, _) = kick(0.125, 180.0);
    assert_eq!(volume, other_volume);
    assert_eq!(fast, slow * 2.0);
    assert_eq!(weak, slow);
}

#[test]
fn dry_single_column_and_upward_collections_do_not_get_fake_surface_kicks() {
    for (columns, depth, speed) in [(21, 0.0, 180.0), (1, 6.0, 180.0), (21, 6.0, -180.0)] {
        let mut water = tank(1.0, columns, depth);
        water.deposit(0, columns / 2, drop(4.0, speed, depth as f32));
        assert_eq!(water.stats().impact_transfers, 0);
        assert!(water.pools[0].velocity.iter().all(|v| *v == 0.0));
    }
}

#[test]
fn impulses_do_not_cross_dry_barriers_or_modify_boundary_flux_laws() {
    let mut water = tank(1.0, 3, 6.0);
    water.pools[0].spec.bed[0] = 20.0;
    water.pools[0].volume[0] = 0.0;
    water.deposit(0, 1, drop(10.0, 500.0, 6.0));
    assert_eq!(water.pools[0].velocity[0], 0.0);
    assert_eq!(water.pools[0].velocity[1], 0.0);
    assert!(water.pools[0].velocity[2] > 0.0);
    assert_eq!(water.pools[0].velocity[3], 0.0);
}

#[test]
fn thin_films_bound_a_large_fast_drop_by_local_depth_not_free_fall_speed() {
    for depth in [0.0001, 0.01, 0.1, 1.0] {
        let mut water = tank(0.25, 21, depth);
        water.deposit(0, 10, drop(1000.0, 950.0, depth as f32));
        let limit = 0.25 * (water.config.gravity * depth).sqrt();
        assert!(water.pools[0].velocity[10].abs() <= limit + 1e-12);
        assert!(water.pools[0].velocity[11].abs() <= limit + 1e-12);
        assert!(water.pools[0].velocity[10] < 0.0);
        assert!(water.pools[0].velocity[11] > 0.0);
    }
}

#[test]
fn repeated_large_impacts_remain_bounded_and_replay_identically() {
    let mut a = tank(1.0, 21, 0.1);
    let mut b = tank(1.0, 21, 0.1);
    for tick in 0..1200 {
        for water in [&mut a, &mut b] {
            if tick < 300 {
                let height = water.pools[0].surface(10) as f32 + 1.0;
                water.add_falling(drop(100.0, 950.0, height)).unwrap();
            }
            water.step(DT).unwrap();
            assert_accounting(water);
            assert!(
                water.pools[0]
                    .velocity
                    .iter()
                    .all(|v| v.abs() <= water.config.max_speed)
            );
        }
        assert_eq!(a.stats(), b.stats());
        assert_eq!(a.pools, b.pools);
        assert_eq!(a.parcels, b.parcels);
    }
    assert_eq!(a.stats().impact_transfers, 300);
}

#[test]
fn birth_step_outfall_collection_gets_one_impact_and_keeps_closed_mass_accounting() {
    let mut water = WaterWorld::new(
        WaterConfig {
            impact_response: 0.25,
            ..WaterConfig::default()
        },
        vec![
            spec(
                -40.0,
                40.0,
                vec![0.0],
                [Boundary::Closed, Boundary::Spill { lip: 0.0 }],
            ),
            spec(0.0, 30.0, vec![-0.1; 3], [Boundary::Closed; 2]),
        ],
    )
    .unwrap();
    water.add_to_pool(0, -20.0, 0.004).unwrap();
    for i in 0..3 {
        water.add_to_pool(1, i as f64 * 10.0 + 5.0, 0.9).unwrap();
    }
    water.step(1.0 / 30.0).unwrap();
    assert!(
        water.parcels.is_empty(),
        "outfall must be caught in its birth half-step"
    );
    assert_eq!(water.stats().impact_transfers, 1);
    assert!(water.pools[1].velocity[1] > 0.0);
    assert_accounting(&water);
}
