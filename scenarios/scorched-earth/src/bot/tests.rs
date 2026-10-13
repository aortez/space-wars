use super::*;

fn step(state: &mut ScorchedState) {
    state.advance(0.0);
}

fn flat(shape: GrainShape, policy: CpuPolicy) -> ScorchedState {
    let mut state = driving::tests::fixture(shape, BankStrength::Off, |_| -5.0);
    state.config.demo = true;
    state.config.cpu_policy = [policy; 2];
    state.tanks[0].next_shot = state.tick;
    state
}

#[test]
fn flat_ranging_learns_from_real_shots_and_retains_a_useful_solution() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut state = flat(shape, CpuPolicy::Calibrated);
        // A controlled stationary target, still hit through the real swept
        // shell/blast path. Ordinary moving matches are tested separately.
        for tank in state.tanks {
            state
                .physics
                .set_body_kind(tank.body, BodyKind::Fixed, true);
        }
        for _ in 0..2100 {
            step(&mut state);
            if state.bots[0].useful_shots > 0 {
                break;
            }
        }
        let shots = state.shot_observations();
        assert!(state.bots[0].useful_shots > 0, "{shape:?}: {shots:?}");
        assert!(shots.len() <= 4, "{shots:?}");
        assert_eq!(shots[0].assessment, "long", "{shots:?}");
        assert!(
            shots.last().unwrap().closest < shots[0].closest,
            "{shots:?}"
        );
        assert!(state.bots[0].aim.confident);
        let power = state.bots[0].aim.power;
        state.aim_bot(0);
        assert_eq!(state.tanks[0].speed, power);
        state.audit().unwrap();
    }
}

#[test]
fn enemy_damage_is_attributed_but_self_and_unowned_damage_do_not_trigger_escape() {
    let mut state = flat(GrainShape::Hexagon, CpuPolicy::Reactive);
    let position = state.physics.motion(state.tanks[0].body).unwrap().position;
    state.explode_from(position + Vec2::Y * 4.5, Some(0));
    assert!(state.tanks[0].health < 100.0);
    assert_eq!(state.bots[0].hit_serial, 0);
    state.explode(position + Vec2::Y * 4.5);
    assert_eq!(state.bots[0].hit_serial, 0);
    state.explode_from(position + Vec2::Y * 4.5, Some(1));
    assert_eq!(state.bots[0].hit_serial, 1);
    assert!(state.bots[0].last_hit.unwrap().1 > 0.0);
}

#[test]
fn a_hit_defers_a_ready_shot_before_the_hull_has_started_moving() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut state = flat(shape, CpuPolicy::Reactive);
        state.config.cpu_driving = true;
        let position = state.physics.motion(state.tanks[0].body).unwrap().position;
        state.explode_from(position + Vec2::Y * 4.5, Some(1));
        step(&mut state);
        assert_eq!(state.tanks[0].shots, 0);
        assert_eq!(state.bots[0].relocations, 1);
        assert!(!state.bot_allows_fire(0));
        let deadline = state.bots[0].relocation.as_ref().unwrap().deadline;
        for _ in 0..540 {
            // Sustained incoming fire must not renew this attempt's deadline.
            state.bots[0].hit(state.tick, 1.0);
            step(&mut state);
            if let Some(goal) = &state.bots[0].relocation {
                assert_eq!(goal.deadline, deadline);
            }
        }
        assert_eq!(state.bots[0].relocations, 1);
        assert!(state.bots[0].relocation.is_none());
        assert!(state.tanks[0].shots > 0, "must resume firing");
        assert!(
            (state
                .physics
                .motion(state.tanks[0].body)
                .unwrap()
                .position
                .x
                - position.x)
                .abs()
                > 0.75
        );
        state.audit().unwrap();
    }
}

#[test]
fn confident_or_cooldown_bots_do_not_restart_relocation_on_a_hit() {
    let mut state = flat(GrainShape::Round, CpuPolicy::Reactive);
    state.config.cpu_driving = true;
    state.bots[0].aim.refresh(Geometry::read(&state, 0));
    state.bots[0].aim.confident = true;
    state.bots[0].hit(state.tick, 10.0);
    assert_eq!(state.bot_drive(0), None);
    assert!(state.bot_allows_fire(0));
    state.bots[0].aim.confident = false;
    state.bots[0].relocate_after = state.tick + 100;
    assert_eq!(state.bot_drive(0), None);
    assert_eq!(state.bots[0].relocations, 0);
}

#[test]
fn no_safe_destination_falls_back_to_aiming() {
    let mut state = driving::tests::fixture(GrainShape::Hexagon, BankStrength::Off, |x| {
        if (-22.0..-18.0).contains(&x) {
            -5.0
        } else {
            -12.0
        }
    });
    state.config.demo = true;
    state.config.cpu_driving = true;
    state.config.cpu_policy = [CpuPolicy::Reactive; 2];
    state.tanks[0].next_shot = state.tick;
    state.bots[0].hit(state.tick, 10.0);
    step(&mut state);
    assert_eq!(state.bots[0].relocations, 0);
    assert_eq!(state.bots[0].relocation_refusals, 1);
    assert_eq!(state.tanks[0].shots, 1);
}

#[test]
fn firing_destination_prefers_level_ground_over_a_drivable_slope() {
    let mut state = driving::tests::fixture(GrainShape::Hexagon, BankStrength::Off, |x| {
        -5.0 + (x + 19.0).clamp(0.0, 5.0) * 0.5
    });
    state.config.cpu_policy = [CpuPolicy::Reactive; 2];
    let enemy = state.physics.motion(state.tanks[1].body).unwrap();
    state.physics.set_pose(
        state.tanks[1].body,
        enemy.position - Vec2::X * 10.0,
        enemy.angle,
        true,
    );
    state.aim_calibrated(0);
    let x = Geometry::read(&state, 0).shooter.x;
    assert!(state.safe_drive_direction(0, 1.0));
    assert!(state.safe_drive_direction(0, -1.0));
    let sites = state.relocation_sites(0, state.bots[0].aim.elevation);
    assert!(
        sites
            .iter()
            .any(|s| s.x > x && s.rejection == Some("uneven firing footing"))
    );
    assert!(
        state
            .escape_destination(0, state.bots[0].aim.elevation)
            .unwrap()
            < x
    );
}

#[test]
fn changing_pose_invalidates_local_confidence_but_keeps_learned_calibration() {
    let mut state = flat(GrainShape::Hexagon, CpuPolicy::Calibrated);
    state.aim_calibrated(0);
    state.bots[0].aim.confident = true;
    state.bots[0].aim.observations = 3;
    let body = state.tanks[0].body;
    let motion = state.physics.motion(body).unwrap();
    state
        .physics
        .set_pose(body, motion.position + Vec2::X * 3.0, motion.angle, true);
    state.aim_calibrated(0);
    assert!(!state.bots[0].aim.confident);
    assert_eq!(state.bots[0].aim.observations, 3);
}

#[test]
fn target_motion_and_obstructions_are_not_power_brackets() {
    let mut state = flat(GrainShape::Round, CpuPolicy::Calibrated);
    state.aim_calibrated(0);
    state.fire(0);
    let launch = state.shells[0].flight.unwrap().launch;
    let mut shot = ShotObservation {
        launch,
        tick: state.tick + 30,
        end: launch.origin + Vec2::new(5.0, 3.0),
        termination: "terrain",
        assessment: "unclassified",
        closest: 20.0,
        crossing_height: None,
        target_moved: false,
        damage: [0.0; 2],
    };
    shot.assessment = aim::assess(&shot, Vec2::new(12.0, 8.0));
    assert_eq!(shot.assessment, "blocked arc");
    state.bots[0].aim.learn(&shot);
    assert!(state.bots[0].aim.elevation > launch.elevation);
    shot.target_moved = true;
    shot.assessment = aim::assess(&shot, Vec2::new(12.0, -8.0));
    assert_eq!(shot.assessment, "target moved");
    let power = state.bots[0].aim.power;
    state.bots[0].aim.learn(&shot);
    assert_eq!(state.bots[0].aim.power, power);
}

#[test]
fn off_arena_shot_reports_a_terminal_observation_and_unblocks_firing() {
    let mut state = flat(GrainShape::Round, CpuPolicy::Calibrated);
    state.tanks[0].elevation = 5.0_f32.to_radians();
    state.tanks[0].speed = 48.0;
    state.tanks[0].facing = -1.0;
    state.fire(0);
    state.tanks[0].next_shot = u64::MAX;
    for _ in 0..600 {
        step(&mut state);
    }
    assert_eq!(state.shot_observations.len(), 1);
    assert!(state.bots[0].pending.is_none());
    assert!(state.bot_allows_fire(0));
}

#[test]
fn learned_and_reactive_duels_clone_replay_and_conserve_both_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for policy in [CpuPolicy::Calibrated, CpuPolicy::Reactive] {
            let mut state = ScorchedState::new(
                ScorchedConfig {
                    shape,
                    demo: true,
                    suspension: true,
                    cpu_footing: [true; 2],
                    cpu_policy: [policy; 2],
                    ..Default::default()
                },
                42,
            );
            for _ in 0..330 {
                step(&mut state);
            }
            let mut replay = state.clone();
            for i in 0..1200 {
                step(&mut state);
                step(&mut replay);
                if i % 60 == 0 {
                    assert_eq!(state.observation_hash(), replay.observation_hash());
                    state.audit().unwrap();
                }
            }
            assert!(state.shots >= 4);
            assert!(!state.shot_observations.is_empty());
        }
    }
}

#[test]
fn bot_comparison_button_restarts_same_seed_and_clears_learning() {
    let mut state = flat(GrainShape::Round, CpuPolicy::Sweep);
    state.bots[0].hit(state.tick, 10.0);
    state.command(Command::CpuAim);
    assert_eq!(state.config.cpu_policy, [CpuPolicy::Calibrated; 2]);
    assert_eq!(state.tick, 0);
    assert_eq!(state.bots[0].hit_serial, 0);
    state.command(Command::CpuAim);
    assert_eq!(state.config.cpu_policy, [CpuPolicy::Reactive; 2]);
    state.command(Command::CpuAim);
    assert_eq!(state.config.cpu_policy, [CpuPolicy::Sweep; 2]);
}

#[test]
fn height_and_ridge_cases_acquire_aim_instead_of_accepting_weak_splash() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for case in [
            bot_lab::AimCase::Uphill,
            bot_lab::AimCase::Downhill,
            bot_lab::AimCase::Ridge,
        ] {
            let mut state = bot_lab::stationary_case(shape, case, CpuPolicy::Calibrated);
            for _ in 0..2100 {
                step(&mut state);
                if state.bots[0].aim.confident {
                    break;
                }
            }
            assert!(
                state.bots[0].aim.confident,
                "{shape:?} {case:?}: {:?}",
                state.shot_observations()
            );
            assert!(
                state.shot_observations.len() <= 4,
                "{case:?}: {:?}",
                state.shot_observations()
            );
            if matches!(case, bot_lab::AimCase::Ridge) {
                assert_eq!(state.shot_observations[0].assessment, "blocked arc");
                assert!(state.bots[0].aim.elevation > state.shot_observations[0].launch.elevation);
            }
            state.audit().unwrap();
        }
    }
}

#[test]
fn disabling_driving_during_relocation_does_not_block_stationary_firing() {
    let mut state = flat(GrainShape::Round, CpuPolicy::Reactive);
    state.config.cpu_driving = true;
    state.bots[0].hit(state.tick, 10.0);
    state.bot_drive(0);
    assert!(state.bots[0].relocation.is_some());
    assert!(!state.bot_allows_fire(0));
    state.command(Command::CpuDrive);
    assert!(state.bot_allows_fire(0));
    step(&mut state);
    assert_eq!(state.tanks[0].shots, 1);
    state.command(Command::CpuDrive);
    for _ in 0..300 {
        step(&mut state);
    }
    assert!(state.bots[0].relocation.is_none());
    assert_eq!(state.bots[0].relocations, 1);
}

#[test]
fn unrelated_dirt_edits_do_not_erase_aim_and_lethal_hits_remain_hits() {
    let mut state = flat(GrainShape::Round, CpuPolicy::Calibrated);
    state.aim_calibrated(0);
    state.bots[0].aim.confident = true;
    state.explode(Vec2::new(40.0, -5.0));
    state.aim_calibrated(0);
    assert!(state.bots[0].aim.confident);
    state.fire(0);
    let launch = state.shells[0].flight.unwrap().launch;
    let shot = ShotObservation {
        launch,
        tick: state.tick,
        end: launch.target,
        termination: "opponent",
        assessment: "unclassified",
        closest: 1.0,
        crossing_height: Some(2.0),
        target_moved: false,
        damage: [0.0, 1.0],
    };
    assert_eq!(aim::assess(&shot, Vec2::new(10.0, -10.0)), "useful hit");
}
