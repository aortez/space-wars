//! Rain stepping only. No renderer, window, sleeping, or wall-clock assertions.
use engine_common::{ClockEventKind, ClockEventProfile, ClockRainAmount, Scenario};
use scenario_clock::{
    ClockAction, ClockConfig, ClockReading, ClockScenario, water_fixture::DigitRainFixture,
};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

fn report(label: &str, times: &mut [f64]) {
    times.sort_by(f64::total_cmp);
    println!(
        "{label}: step_us p50={:.2} p95={:.2} p99={:.2}",
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        times[times.len() * 99 / 100]
    );
}

fn live(seed: u64, aspect_ratio: f32, change: bool) {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio,
            event_profile: ClockEventProfile::Off,
            time_format: engine_common::ClockTimeFormat::TwentyFourHour,
            rain_amount: ClockRainAmount::Heavy,
            ..ClockConfig::default()
        },
        seed,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(if change {
                ClockReading::new(8, 8, 0).unwrap()
            } else {
                ClockReading::new(12, 34, 56).unwrap()
            }),
            ClockAction::preview_event(ClockEventKind::Rain),
        ],
        Duration::ZERO,
    );
    let mut times = Vec::with_capacity(1200);
    let mut peak = 0;
    let mut pending = 0;
    let mut surface = 0;
    let mut prechange = (0, 0);
    for tick in 0..1200 {
        if tick == 600 {
            let rain = state.rain_state().unwrap();
            prechange = (rain.source_limited_ticks, rain.water_limited_ticks);
        }
        let actions = if change && tick == 600 {
            vec![ClockAction::set_reading(
                ClockReading::new(11, 11, 0).unwrap(),
            )]
        } else {
            vec![]
        };
        let start = Instant::now();
        ClockScenario::step(
            black_box(&mut state),
            &actions,
            Duration::from_nanos(16_666_667),
        );
        let us = start.elapsed().as_secs_f64() * 1e6;
        if tick >= 120 {
            times.push(us);
        }
        let rain = state.rain_state().unwrap();
        peak = peak.max(rain.parcels);
        pending = pending.max(
            rain.scheduled_microunits
                .saturating_sub(rain.injected_microunits),
        );
        surface = surface.max(rain.surface_water_microunits);
    }
    let rain = state.rain_state().unwrap();
    report(
        &format!(
            "clock-heavy aspect={aspect_ratio:.3} change={change} seed={seed} peak={peak} source_limited={} outlet_limited={} prechange={prechange:?} deferrals={} floor_deferrals={} floor_open={} drips={} impacts={} peak_surface={:.2} cells max_pending={:.2} cells delivered={:.1}%",
            rain.source_limited_ticks,
            rain.water_limited_ticks,
            rain.surface_change_deferrals,
            rain.floor_motion_deferrals,
            rain.floor_open_milli,
            rain.drip_parcels_emitted,
            rain.surface_impacts,
            surface as f64 / 1e6,
            pending as f64 / 1e6,
            100.0 * rain.injected_microunits as f64 / rain.requested_microunits as f64,
        ),
        &mut times,
    );
}

fn main() {
    let live_only = std::env::args().any(|arg| arg == "--live-only");
    for seed in [0, 7, 19] {
        live(seed, 4.0 / 3.0, false);
        for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
            live(seed, aspect, true);
        }
        if live_only {
            continue;
        }

        for slots in [1, 4] {
            for drips in [false, true] {
                let mut fixture = if slots == 1 {
                    DigitRainFixture::with_drips(8, seed, drips)
                } else {
                    DigitRainFixture::row([8; 4], seed, drips)
                };
                let mut times = Vec::with_capacity(1200);
                let mut peak = 0;
                let mut changed_at = None;
                let mut source_before_change = 0;
                let mut outlets_before_change = 0;
                let mut pending: f64 = 0.0;
                for tick in 0..1200 {
                    if tick == 600 {
                        source_before_change = fixture.source_limited_ticks();
                        outlets_before_change = fixture.water.stats().capacity_limited_ticks;
                    }
                    if tick >= 600
                        && changed_at.is_none()
                        && fixture.set_digits(&[1; 4][..slots]).is_ok()
                    {
                        changed_at = Some(tick);
                    }
                    let start = Instant::now();
                    black_box(&mut fixture).step(tick < 900);
                    let us = start.elapsed().as_secs_f64() * 1e6;
                    if tick >= 120 {
                        times.push(us);
                    }
                    peak = peak.max(fixture.water.stats().parcels);
                    pending =
                        pending.max(fixture.scheduled_volume() - fixture.water.stats().injected);
                }
                let water = fixture.water.stats();
                assert!(changed_at.is_some());
                assert!(
                    (water.injected
                        - water.pooled
                        - water.in_flight
                        - water.drained
                        - water.reclaimed)
                        .abs()
                        < water.injected * 1e-10
                );
                report(
                    &format!(
                        "digits={slots} drips={drips} seed={seed} pools={} peak={peak} source_limited={} (pre={source_before_change}) outlet_limited={} (pre={outlets_before_change}) drips_emitted={} max_pending={pending:.1} changed_at={changed_at:?} delivered={:.1}%",
                        fixture.water.pools().len(),
                        fixture.source_limited_ticks(),
                        water.capacity_limited_ticks,
                        water.drip_parcels_emitted,
                        100.0 * water.injected / fixture.scheduled_volume()
                    ),
                    &mut times,
                );
            }
        }
    }
}
