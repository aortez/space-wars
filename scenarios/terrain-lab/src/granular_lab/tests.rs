use super::*;
use engine_common::PointerAction;

fn tick(state: &mut GranularLabState, actions: &[Action]) {
    GranularLabScenario::step(state, actions, Duration::from_secs_f64(1.0 / 60.0));
}

#[test]
fn controls_fire_once_release_and_survive_fixture_rebuild_without_repeating() {
    let mut state = GranularLabScenario::init(Default::default(), 42);
    let held = GranularLabAction::Controls(GranularControls {
        fire: true,
        ..Default::default()
    })
    .encode();
    for _ in 0..20 {
        tick(&mut state, std::slice::from_ref(&held));
    }
    assert_eq!(state.lab.blasts, 1);
    assert!(state.lab.grains().count() > 0);
    let cycle = GranularLabAction::Controls(GranularControls {
        fixture: true,
        ..Default::default()
    })
    .encode();
    for _ in 0..20 {
        tick(&mut state, std::slice::from_ref(&cycle));
    }
    assert_eq!(state.config.fixture, Fixture::Slope);
    tick(
        &mut state,
        &[GranularLabAction::Controls(Default::default()).encode()],
    );
    tick(&mut state, &[cycle]);
    assert_eq!(state.config.fixture, Fixture::MovingPlanet);
}

#[test]
fn pointer_buttons_do_not_blast_and_pause_step_slow_and_host_release_work() {
    let mut state = GranularLabScenario::init(Default::default(), 42);
    let press = |p: Vec2| {
        Action::Pointer(PointerAction {
            position: RenderPoint::new(p.x, p.y),
            phase: PointerPhase::Press,
        })
    };
    let button = state.button_center(Command::Pause);
    tick(&mut state, &[press(button)]);
    assert!(state.paused);
    assert_eq!(state.lab.tick, 0);
    let aim = state.aim();
    tick(&mut state, &[press(aim)]);
    assert_eq!(state.lab.blasts, 1);
    tick(
        &mut state,
        &[GranularLabAction::Command(Command::Step).encode()],
    );
    assert_eq!(state.lab.tick, 1);
    state.command(Command::Speed);
    state.command(Command::Pause);
    for _ in 0..4 {
        tick(&mut state, &[]);
    }
    assert_eq!(state.lab.tick, 2);
    state.controls.aim = Vec2::X;
    GranularLabScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(state.controls, GranularControls::default());
    assert_eq!(state.lab.tick, 2);
}

#[test]
fn invalid_packets_do_not_change_controls_and_reset_restores_selected_configuration() {
    for action in [
        Action::scenario(20, vec![0; 12]),
        Action::scenario(21, vec![255]),
        GranularLabAction::Aim(Vec2::new(f32::NAN, 0.0)).encode(),
    ] {
        assert_eq!(GranularLabAction::decode(&action), None);
    }
    let mut state = GranularLabScenario::init(
        GranularLabConfig {
            fixture: Fixture::Slope,
            preset: GrainPreset::Angular,
            ..Default::default()
        },
        73,
    );
    let initial = state.lab.content_motion_hash();
    state.command(Command::Fire);
    state.command(Command::Drop);
    for _ in 0..30 {
        tick(&mut state, &[]);
    }
    state.command(Command::Reset);
    assert_eq!(state.config.preset, GrainPreset::Angular);
    assert_eq!(state.lab.content_motion_hash(), initial);
    assert!(state.lab.probe_snapshot().is_none());
}

#[test]
fn live_actions_replay_exactly_including_box_and_moving_ground() {
    let config = GranularLabConfig {
        fixture: Fixture::MovingPlanet,
        preset: GrainPreset::Angular,
        ..Default::default()
    };
    let mut state = GranularLabScenario::init(config, 42);
    let mut replay = GranularLabScenario::init(config, 42);
    for frame in 0..300 {
        let actions = match frame {
            10 | 190 => vec![GranularLabAction::Command(Command::Fire).encode()],
            140 => vec![GranularLabAction::Command(Command::Drop).encode()],
            _ => vec![],
        };
        tick(&mut state, &actions);
        tick(&mut replay, &actions);
        assert_eq!(
            GranularLabScenario::observe(&state).payload,
            GranularLabScenario::observe(&replay).payload
        );
        state.lab.audit().unwrap();
        if frame == 150 {
            replay = state.clone();
        }
    }
    assert!(state.lab.probe_snapshot().is_some());
}
