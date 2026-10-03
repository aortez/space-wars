use super::*;
use crate::tests::{assert_accounting, spec};
use crate::{Boundary, SolidBox, WaterWorld};

fn source() -> Parcel {
    Parcel {
        position: Vec2::ZERO,
        velocity: Vec2::new(60.0, -20.0),
        volume: 12.0,
        duration: 1.0 / 60.0,
        horizontal_bounds: None,
    }
}

fn varied(seed: u64) -> WaterConfig {
    WaterConfig {
        splash: Some(SplashConfig {
            interval: 0.25,
            variation: Some(SplashVariation {
                seed,
                max_interval: 0.55,
            }),
            ..SplashConfig::default()
        }),
        ..WaterConfig::default()
    }
}

fn sequence(seed: u64, rejected: bool) -> Vec<([Parcel; 3], f64)> {
    let config = varied(seed);
    let mut state = State::default();
    let mut result = Vec::new();
    for _ in 0..32 {
        let mut p = source();
        let impact = Impact {
            position: Vec2::ZERO,
            normal: Vec2::ZERO,
            speed: 60.0,
        };
        if rejected {
            assert!(!state.split(&mut p, impact, 10, config));
            assert_eq!(p, source());
            assert!(!state.split(
                &mut p,
                Impact {
                    speed: 1.0,
                    ..impact
                },
                32,
                config
            ));
        }
        assert!(state.split(&mut p, impact, 32, config));
        result.push((state.take().unwrap(), state.cooldown));
        state.begin_step(0.55);
    }
    result
}

#[test]
fn seeded_bursts_replay_vary_and_ignore_rejected_attempts() {
    let a = sequence(7, false);
    assert_eq!(a, sequence(7, false));
    assert_eq!(a, sequence(7, true));
    assert_ne!(a, sequence(19, false));
    let intervals: Vec<_> = a.iter().map(|(_, interval)| *interval).collect();
    assert!(intervals.iter().all(|t| (0.25..=0.55).contains(t)));
    assert!(
        intervals.iter().copied().reduce(f64::max).unwrap()
            - intervals.iter().copied().reduce(f64::min).unwrap()
            > 0.15
    );
    for (drops, _) in a {
        assert!((drops.iter().map(|d| d.volume).sum::<f64>() - 12.0 * 0.12).abs() < 1e-12);
        assert!(drops.iter().all(|d| d.velocity.y > 0.0));
    }
}

#[test]
fn varied_fans_stay_in_free_space_and_inside_water_energy_and_speed_budgets() {
    for max_speed in [80.0, 1000.0] {
        for seed in [0, 7, 19, u64::MAX] {
            for tilt in [-0.5_f32, 0.0, 0.5] {
                for sign in [-1.0, 1.0] {
                    let normal = Vec2::new(sign * tilt.cos(), tilt.sin());
                    let mut state = State::default();
                    for _ in 0..32 {
                        let mut p = source();
                        let config = WaterConfig {
                            max_speed,
                            ..varied(seed)
                        };
                        assert!(state.split(
                            &mut p,
                            Impact {
                                position: Vec2::ZERO,
                                normal,
                                speed: 60.0
                            },
                            32,
                            config
                        ));
                        let drops = state.take().unwrap();
                        assert!(
                            (p.volume + drops.iter().map(|p| p.volume).sum::<f64>() - 12.0).abs()
                                < 1e-12
                        );
                        assert!(drops.iter().all(|p| p.volume > 0.0
                            && p.velocity.y > 0.0
                            && p.velocity.dot(normal) >= 0.0
                            && p.velocity.length() <= max_speed as f32 + 0.0001));
                        let energy: f64 = drops
                            .iter()
                            .map(|p| p.volume * f64::from(p.velocity.length_squared()) * 0.5)
                            .sum();
                        assert!(energy <= 0.5 * 12.0 * 60.0_f64.powi(2) * 0.75);
                        state.begin_step(0.55);
                    }
                }
            }
        }
    }
}

#[test]
fn varied_wall_fans_mirror_with_the_same_seed() {
    for burst in 0..32 {
        let a = Pattern::new(
            varied(19).splash.unwrap(),
            Vec2::new(-0.98, -0.2).normalized(),
            burst,
        );
        let b = Pattern::new(
            varied(19).splash.unwrap(),
            Vec2::new(0.98, -0.2).normalized(),
            burst,
        );
        assert_eq!(a.interval, b.interval);
        assert_eq!(a.weights, b.weights);
        assert_eq!(a.speed_factors, b.speed_factors);
        for (a, b) in a.directions.iter().zip(b.directions) {
            assert_eq!(a.x, -b.x);
            assert_eq!(a.y, b.y);
        }
    }
}

#[test]
fn spray_uses_existing_volume_and_at_most_the_lost_energy() {
    for normal in [Vec2::ZERO, Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)] {
        let config = WaterConfig {
            splash: Some(SplashConfig::default()),
            ..WaterConfig::default()
        };
        let mut state = State::default();
        let mut p = source();
        assert!(state.split(
            &mut p,
            Impact {
                position: Vec2::ZERO,
                normal,
                speed: 60.0
            },
            32,
            config
        ));
        let drops = state.take().unwrap();
        assert!((p.volume + drops.iter().map(|p| p.volume).sum::<f64>() - 12.0).abs() < 1.0e-12);
        let energy: f64 = drops
            .iter()
            .map(|p| p.volume * f64::from(p.velocity.length_squared()) * 0.5)
            .sum();
        assert!(energy <= 0.5 * 12.0 * 60.0_f64.powi(2) * 0.75);
        assert!(
            drops
                .iter()
                .all(|p| p.velocity.y > 0.0 && p.velocity.dot(normal) >= 0.0)
        );
        assert!(!state.split(
            &mut p,
            Impact {
                position: Vec2::ZERO,
                normal,
                speed: 60.0
            },
            32,
            config
        ));
    }
}

#[test]
fn quiet_impacts_disabled_spray_and_capacity_exhaustion_leave_sources_intact() {
    for (enabled, speed, free) in [(false, 60.0, 32), (true, 10.0, 32), (true, 60.0, 10)] {
        let config = WaterConfig {
            splash: enabled.then(SplashConfig::default),
            ..WaterConfig::default()
        };
        let mut state = State::default();
        let mut p = source();
        assert!(!state.split(
            &mut p,
            Impact {
                position: Vec2::ZERO,
                normal: Vec2::ZERO,
                speed
            },
            free,
            config
        ));
        assert_eq!(p, source());
        assert_eq!(state.pending_count(), 0);
    }
}

#[test]
fn rejects_invalid_splash_tuning() {
    for splash in [
        SplashConfig {
            min_speed: f64::NAN,
            ..SplashConfig::default()
        },
        SplashConfig {
            volume_fraction: 0.0,
            ..SplashConfig::default()
        },
        SplashConfig {
            volume_fraction: 0.26,
            ..SplashConfig::default()
        },
        SplashConfig {
            energy_fraction: f64::INFINITY,
            ..SplashConfig::default()
        },
        SplashConfig {
            interval: 0.0,
            ..SplashConfig::default()
        },
        SplashConfig {
            variation: Some(SplashVariation {
                seed: 7,
                max_interval: f64::NAN,
            }),
            ..SplashConfig::default()
        },
        SplashConfig {
            variation: Some(SplashVariation {
                seed: 7,
                max_interval: 0.1,
            }),
            ..SplashConfig::default()
        },
    ] {
        assert!(
            WaterWorld::new(
                WaterConfig {
                    splash: Some(splash),
                    ..WaterConfig::default()
                },
                vec![spec(0.0, 10.0, vec![0.0], [Boundary::Closed; 2])]
            )
            .is_err()
        );
    }
}

#[test]
fn actual_wall_and_opposed_outfalls_keep_storage_accounting_and_rate_bounded() {
    let fixed = WaterConfig {
        splash: Some(SplashConfig::default()),
        ..WaterConfig::default()
    };
    for config in [fixed, varied(7)] {
        for hz in [30, 60, 120] {
            for wall in [false, true] {
                let mut specs = vec![spec(
                    -30.0,
                    30.0,
                    vec![0.0],
                    [Boundary::Closed, Boundary::Spill { lip: 0.0 }],
                )];
                if !wall {
                    specs.push(spec(
                        8.0,
                        30.0,
                        vec![0.0],
                        [Boundary::Spill { lip: 0.0 }, Boundary::Closed],
                    ));
                }
                let mut w = WaterWorld::new(
                    WaterConfig {
                        max_parcels: 64,
                        ..config
                    },
                    specs,
                )
                .unwrap();
                if wall {
                    w.set_solid_boxes(&[SolidBox::new(
                        Vec2::new(9.0, -30.0),
                        Vec2::new(1.0, 30.0),
                        0.0,
                    )
                    .unwrap()])
                        .unwrap();
                }
                let ptrs = (w.parcels.as_ptr(), w.spills.as_ptr());
                let mut peak = 0;
                for tick in 0..hz * 8 {
                    if tick < hz * 4 {
                        for pool in 0..w.pools().len() {
                            let c = w.pools()[pool].columns().next().unwrap();
                            let missing = (10.0 * c.width - c.volume).max(0.0);
                            if missing > 0.0 {
                                w.add_to_pool(pool, c.left + c.width * 0.5, missing)
                                    .unwrap();
                            }
                        }
                    }
                    w.step(1.0 / f64::from(hz)).unwrap();
                    assert_accounting(&w);
                    assert_eq!(ptrs, (w.parcels.as_ptr(), w.spills.as_ptr()));
                    peak = peak.max(w.parcels.len());
                    assert!(
                        w.stats().splash_bursts
                            <= 1 + (f64::from(tick + 1)
                                / f64::from(hz)
                                / config.splash.unwrap().interval)
                                .floor() as u64
                    );
                    if wall {
                        assert!(w.parcels.iter().all(|p| p.position.x < 8.001
                            || p.position.x > 9.999
                            || p.position.y < -59.999
                            || p.position.y > -0.001));
                    }
                }
                assert!(w.stats().splash_bursts > 0, "{hz} Hz wall={wall}");
                assert!(peak <= 64);
                w.reclaim();
                assert_eq!(w.stats().in_flight + w.stats().pooled, 0.0);
                assert_accounting(&w);
            }
        }
    }
}
