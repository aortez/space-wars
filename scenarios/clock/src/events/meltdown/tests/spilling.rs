use super::*;
use crate::events::ActiveEvent;
use engine_rapier::world::{ColliderId, ColliderRole};

fn preview(aspect: f32, mode: ClockWaterLab) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: aspect,
            water_lab: mode,
            event_profile: ClockEventProfile::Off,
            ..ClockConfig::default()
        },
        42,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(ClockReading::new(8, 8, 0).unwrap()),
            ClockAction::preview_event(ClockEventKind::Meltdown),
        ],
        Duration::ZERO,
    );
    state
}

fn pool_volume(event: &MeltdownEvent, pool: usize) -> f64 {
    event.water.pools()[pool].columns().map(|c| c.volume).sum()
}

#[test]
fn body_entry_spills_collects_replays_and_cleans_up_at_device_aspects() {
    for aspect in [0.6, 800.0 / 480.0, 1024.0 / 768.0, 1280.0 / 720.0, 2.4] {
        let mut state = preview(aspect, ClockWaterLab::Spilling);
        let mut replay = preview(aspect, ClockWaterLab::Spilling);
        let mut control = preview(aspect, ClockWaterLab::SpillingControl);
        let mut saw_flight = false;
        let mut saw_collection = false;
        let mut contact = false;
        let mut wet = [false; 3];
        let mut initial_volume = 0.0;
        let mut previous_source = f64::INFINITY;
        for elapsed in 0..510 {
            assert_volume(&state);
            assert_volume(&control);
            assert_eq!((state.body_count(), state.collider_count()), (4, 9));
            let Some(ActiveEvent::Meltdown(event)) = &state.active_event else {
                panic!()
            };
            let Some(ActiveEvent::Meltdown(baseline)) = &control.active_event else {
                panic!()
            };
            if elapsed == 0 {
                initial_volume = event.water.stats().injected;
                assert_eq!(initial_volume, baseline.water.stats().injected);
            }
            let stats = event.water.stats();
            assert_eq!(stats.drained, 0.0, "collector must catch the spill");
            assert_eq!(baseline.water.stats().displaced, 0.0);
            assert!(pool_volume(baseline, 1) < 1e-7);
            assert!(baseline.water.stats().in_flight < 1e-7);
            if elapsed < 420 {
                assert!((pool_volume(baseline, 0) - initial_volume).abs() < 1e-7);
            }
            let source = pool_volume(event, 0);
            assert!(source <= previous_source + 1e-7);
            previous_source = source;
            saw_flight |= stats.in_flight > initial_volume * 0.001;
            saw_collection |= pool_volume(event, 1) > initial_volume * 0.01;
            let lab = event.floats.as_ref().unwrap();
            assert!(lab.piston.is_none(), "all bodies must move through physics");
            for (i, body) in lab.bodies.iter().enumerate() {
                wet[i] |= body.report.submerged_fraction > 0.1;
                let pose = lab.world.motion(body.body.body()).unwrap();
                assert!(pose.position.x.is_finite() && pose.position.y.is_finite());
                assert!(pose.angle.is_finite() && pose.angular_velocity.is_finite());
                let collider = ColliderId::new(body.body.body().entity, ColliderRole::PRIMARY, 0);
                contact |= lab.world.surface_contacts(collider).any(|c| {
                    lab.bodies
                        .iter()
                        .any(|b| b.body.body().entity == c.collider.entity)
                });
            }
            if elapsed == 360 {
                eprintln!(
                    "spilling aspect={aspect}: source={} collector={} in_flight={} displaced={}",
                    source / initial_volume,
                    pool_volume(event, 1) / initial_volume,
                    stats.in_flight / initial_volume,
                    stats.displaced / initial_volume
                );
                assert!(source < initial_volume * 0.98);
                assert!(stats.displaced > 0.0);
            }
            if elapsed % 30 == 0 {
                assert_eq!(
                    ClockScenario::render_frame(&state),
                    ClockScenario::render_frame(&replay)
                );
            }
            if elapsed == 180 {
                let frozen = ClockScenario::render_frame(&state);
                ClockScenario::step(&mut state, &[], Duration::ZERO);
                assert_eq!(frozen, ClockScenario::render_frame(&state));
            }
            tick(&mut state);
            tick(&mut replay);
            tick(&mut control);
        }
        assert!(saw_flight && saw_collection, "aspect={aspect}");
        assert!(contact && wet.into_iter().all(|v| v), "aspect={aspect}");
        for s in [&state, &replay, &control] {
            assert!(s.meltdown_state().is_none());
            assert_eq!((s.body_count(), s.collider_count()), (0, 0));
        }
        for mode in [ClockWaterLab::Spilling, ClockWaterLab::SpillingControl] {
            for resize in [false, true] {
                let mut state = preview(aspect, mode);
                for _ in 0..210 {
                    tick(&mut state);
                }
                if resize {
                    state.set_aspect_ratio(1.0);
                } else {
                    ClockScenario::step(
                        &mut state,
                        &[ClockAction::preview_event(ClockEventKind::ColorCycle)],
                        Duration::ZERO,
                    );
                }
                assert!(state.meltdown_state().is_none());
                assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            }
        }
    }
}
