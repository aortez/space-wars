use super::*;

fn step(state: &mut ScorchedState, actions: &[Action]) {
    ScorchedScenario::step(
        state,
        actions,
        Duration::from_secs_f64(1.0 / FIXED_HZ as f64),
    );
}

#[test]
fn scripted_duel_displaces_and_redeposits_conserved_dirt_for_both_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut state = ScorchedState::new(
            ScorchedConfig {
                shape,
                demo: true,
                ..Default::default()
            },
            42,
        );
        let mut replay = state.clone();
        let mut peak_loose = 0;
        for tick in 0..1800 {
            step(&mut state, &[]);
            step(&mut replay, &[]);
            peak_loose = peak_loose.max(state.loose_cells());
            if tick % 60 == 0 {
                assert_eq!(
                    ScorchedScenario::observe(&state).payload,
                    ScorchedScenario::observe(&replay).payload,
                    "{shape:?} tick {tick}"
                );
                state.audit().unwrap();
            }
        }
        assert!(
            state.shots >= 4 && state.impacts >= 4,
            "shots {}, impacts {}",
            state.shots,
            state.impacts
        );
        assert!(peak_loose > 10, "must exercise actual material release");
        assert!(
            state.deposited_cells() > 0,
            "must exercise the shared return path"
        );
        assert!(state.tanks.iter().any(|tank| tank.health < 100.0));
        state.audit().unwrap();
        let mut continuation = state.clone();
        for _ in 0..120 {
            step(&mut state, &[]);
            step(&mut continuation, &[]);
        }
        assert_eq!(
            ScorchedScenario::observe(&state).payload,
            ScorchedScenario::observe(&continuation).payload
        );
    }
}

#[test]
fn capacity_rejection_preserves_all_material_and_ids() {
    let mut state = ScorchedState::new(
        ScorchedConfig {
            max_grains: 1,
            ..Default::default()
        },
        42,
    );
    let initial = state.audit().unwrap();
    let hash = state.terrain[0].terrain.hash();
    let next = state.next_id;
    state.explode(Vec2::new(0.0, hill(0.0, 42)));
    assert_eq!(state.rejected_blasts, 1);
    assert_eq!(state.terrain[0].terrain.hash(), hash);
    assert_eq!(state.next_id, next);
    assert_eq!(state.audit().unwrap(), initial);
}

#[test]
fn destroying_the_ground_under_a_tank_removes_its_support() {
    let mut state = ScorchedState::new(ScorchedConfig::default(), 42);
    for _ in 0..120 {
        step(&mut state, &[]);
    }
    let body = state.tanks[0].body;
    let initial = state.physics.motion(body).unwrap().position;
    state.explode(initial - Vec2::Y * 1.7);
    let mut moved = false;
    for _ in 0..180 {
        step(&mut state, &[]);
        moved |= state
            .physics
            .motion(body)
            .unwrap()
            .position
            .distance_to(initial)
            > 1.0;
    }
    assert!(moved, "tank cannot stay anchored over missing ground");
    state.audit().unwrap();
}

#[test]
fn zero_time_bad_input_reset_and_held_toggles_preserve_session_contract() {
    let mut state = ScorchedState::new(ScorchedConfig::default(), 42);
    let initial = ScorchedScenario::observe(&state).payload;
    let fire = ScorchedAction::Controls(Controls {
        fire: true,
        ..Default::default()
    })
    .encode();
    ScorchedScenario::step(&mut state, &[fire], Duration::ZERO);
    assert_eq!(ScorchedScenario::observe(&state).payload, initial);
    let invalid = ScorchedAction::Controls(Controls {
        aim: f32::NAN,
        ..Default::default()
    })
    .encode();
    assert!(ScorchedAction::decode(&invalid).is_none());
    let select = ScorchedAction::Controls(Controls {
        select: true,
        ..Default::default()
    })
    .encode();
    for _ in 0..20 {
        step(&mut state, std::slice::from_ref(&select));
    }
    assert_eq!(state.selected, 1);
    step(&mut state, &[]);
    step(&mut state, &[select]);
    assert_eq!(state.selected, 0);
    state.command(Command::Reset);
    assert_eq!(ScorchedScenario::observe(&state).payload, initial);
    let other_seed = ScorchedState::new(ScorchedConfig::default(), 43);
    assert_ne!(
        state.terrain[0].terrain.hash(),
        other_seed.terrain[0].terrain.hash()
    );
}

#[test]
fn swept_shell_hits_ground_instead_of_skipping_through_it() {
    let mut state = ScorchedState::new(ScorchedConfig::default(), 42);
    state.shells.push(Shell {
        owner: 0,
        position: Vec2::new(0.0, 5.0),
        velocity: Vec2::new(0.0, -1000.0),
        age: 10,
    });
    step(&mut state, &[]);
    assert!(state.shells.is_empty());
    assert_eq!(state.impacts, 1);
    assert!(state.loose_cells() > 0);
    state.audit().unwrap();
}

#[test]
fn full_width_seed_can_drive_both_tanks() {
    let mut state = ScorchedState::new(
        ScorchedConfig {
            demo: true,
            ..Default::default()
        },
        u64::MAX,
    );
    state.aim_bot(0);
    state.aim_bot(1);
    step(&mut state, &[]);
    state.audit().unwrap();
}

#[test]
fn bombardment_ignores_tank_deaths_and_stops_launching_at_the_tail() {
    let mut state = ScorchedState::new(
        ScorchedConfig {
            bombardment_seconds: 6,
            ..Default::default()
        },
        42,
    );
    state.tanks.iter_mut().for_each(|t| t.health = 0.0);
    for _ in 0..600 {
        step(&mut state, &[]);
    }
    assert_eq!(state.shots, 3);
    assert_eq!(state.impacts, 3);
    assert!(state.shells.is_empty());
    let hash = state.observation_hash();
    let mut replay = ScorchedState::new(state.config, 42);
    replay.tanks.iter_mut().for_each(|t| t.health = 0.0);
    for _ in 0..600 {
        step(&mut replay, &[]);
    }
    assert_eq!(hash, replay.observation_hash());
    state.audit().unwrap();
}

#[test]
fn collapse_and_barrage_toggle_once_when_held_and_reset_comparisons() {
    let mut state = ScorchedState::new(ScorchedConfig::default(), 42);
    let controls = ScorchedAction::Controls(Controls {
        collapse: true,
        barrage: true,
        ..Default::default()
    })
    .encode();
    for _ in 0..60 {
        step(&mut state, std::slice::from_ref(&controls));
    }
    assert!(state.config.slumping);
    assert_eq!(state.config.bombardment_seconds, 60);
    assert_eq!(state.tick, 60);
    assert_eq!(state.shots, 1);
    state.command(Command::Shape);
    assert!(state.config.slumping);
    assert_eq!(state.config.bombardment_seconds, 60);
    assert_eq!(state.tick, 0);
    state.command(Command::Demo);
    assert!(state.config.demo);
    assert_eq!(state.config.bombardment_seconds, 0);
    // Existing recordings keep their twelve-byte continuous input payload.
    let Action::Scenario { kind, mut payload } = controls else {
        unreachable!()
    };
    payload.truncate(12);
    assert_eq!(
        ScorchedAction::decode(&Action::scenario(kind, payload)),
        Some(ScorchedAction::Controls(Controls::default()))
    );
}

#[test]
fn sustained_collapse_conserves_and_quiet_tail_stops_releasing_for_both_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut state = ScorchedState::new(
            ScorchedConfig {
                shape,
                slumping: true,
                bombardment_seconds: 60,
                ..Default::default()
            },
            42,
        );
        let mut released_at_90 = 0;
        for tick in 1..=7200 {
            step(&mut state, &[]);
            if tick % 60 == 0 {
                state.audit().unwrap();
            }
            if tick == 5400 {
                released_at_90 = state.slumping_diagnostics().released_cells;
            }
        }
        assert_eq!(state.shots, 30);
        assert_eq!(state.impacts, 30);
        assert!(released_at_90 > 0);
        assert_eq!(
            state.slumping_diagnostics().released_cells,
            released_at_90,
            "quiet tail must not keep recycling banks"
        );
        let mut replay = state.clone();
        for _ in 0..120 {
            step(&mut state, &[]);
            step(&mut replay, &[]);
        }
        assert_eq!(state.observation_hash(), replay.observation_hash());
    }
}
