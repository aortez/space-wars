//! Diagnostic counterfactuals, never consulted by the playable controller.
//! The prefix uses the retained three-unit selector. At a real incoming-hit
//! opportunity, clone everything and change only the subject's one decision.
use super::*;
use serde_json::{Value, json};

fn reference_destination(state: &ScorchedState, player: usize) -> Option<f32> {
    if !state.has_track_support(player) {
        return None;
    }
    let geometry = Geometry::read(state, player);
    let toward = (geometry.target.x - geometry.shooter.x).signum();
    let preferred = if (geometry.target.x - geometry.shooter.x).abs() > 29.0 {
        toward
    } else {
        -toward
    };
    [preferred, -preferred].into_iter().find_map(|direction| {
        let x = geometry.shooter.x + 3.0 * direction;
        (x.abs() <= 44.0
            && (12.0..=40.0).contains(&(geometry.target.x - x).abs())
            && state.safe_drive_direction(player, direction))
        .then_some(x)
    })
}

fn ready_bot(state: &ScorchedState, player: usize) -> Option<Bot> {
    if !state.fighting() || !state.config.cpu_driving || !state.calibrated_cpu(player) {
        return None;
    }
    let mut bot = state.bots[player].clone();
    bot.aim.refresh(Geometry::read(state, player));
    bot.wants_relocation(state, player).then_some(bot)
}

fn reference_step(state: &mut ScorchedState) {
    // Preserve the original prefix when the playable destination selector is
    // tuned. No body, shell, timer, driver, or aim memory is reinitialized.
    for player in 0..2 {
        if let Some(mut bot) = ready_bot(state, player) {
            bot.consider_relocation(state, player, reference_destination(state, player));
            state.bots[player] = bot;
        }
    }
    state.advance(0.0);
}

fn sample(state: &ScorchedState) -> Value {
    json!({"tick": state.tick, "tanks": driving_lab::combat_tanks(state),
        "bots": state.cpu_diagnostics()})
}

#[derive(Clone, Copy)]
enum Choice {
    ContinueAiming,
    Reference,
    Destination,
    Current,
}

fn fork(checkpoint: &ScorchedState, player: usize, choice: Choice) -> ScorchedState {
    let mut state = checkpoint.clone();
    assert_eq!(state.observation_hash(), checkpoint.observation_hash());
    let mut bot = ready_bot(&state, player).expect("eligible checkpoint");
    match choice {
        Choice::ContinueAiming => {
            // Decline only this hit; preserve aim, shot schedule, driver
            // history, and all other eligibility/cooldown/budget fields.
            bot.considered_hit = bot.hit_serial;
        }
        Choice::Reference | Choice::Destination => {
            let destination = match choice {
                Choice::Reference => reference_destination(&state, player),
                _ => state.escape_destination(player, bot.aim.elevation),
            };
            bot.consider_relocation(&state, player, destination);
        }
        Choice::Current => bot.react_to_hit(&state, player),
    }
    state.bots[player] = bot;
    state
}

fn continuation(checkpoint: &ScorchedState, player: usize, choice: Choice, seconds: u32) -> Value {
    let initial_hash = checkpoint.observation_hash();
    let start = checkpoint.tick;
    let origin = Geometry::read(checkpoint, player).shooter;
    let health = checkpoint.tanks.map(|tank| tank.health);
    let own_shots = checkpoint.tanks[player].shots;
    let mut trace = vec![sample(checkpoint)];
    let mut state = fork(checkpoint, player, choice);
    let destination = state.bots[player]
        .relocation
        .as_ref()
        .map(|r| r.destination_x);
    let mut first_fire = None;
    let mut first_useful = None;
    let mut max_displacement = 0.0_f32;
    let mut travel = 0.0;
    let mut previous = origin;
    for _ in 0..seconds * FIXED_HZ {
        // Isolate one relocation opportunity. In every arm, further hits
        // cannot start a new relocation for this tank during the window.
        // The opponent and ordinary range/footing behavior remain enabled.
        state.bots[player].considered_hit = state.bots[player].hit_serial;
        let before_shots = state.tanks[player].shots;
        reference_step(&mut state);
        if first_fire.is_none() && state.tanks[player].shots > before_shots {
            first_fire = Some(state.tick - 1 - start);
        }
        if first_useful.is_none() {
            first_useful = state.shot_observations.iter().find_map(|shot| {
                (shot.launch.owner == player
                    && shot.launch.tick >= start
                    && shot.assessment == "useful hit")
                    .then(|| shot.tick - start)
            });
        }
        let position = Geometry::read(&state, player).shooter;
        travel += position.distance_to(previous);
        max_displacement = max_displacement.max((position.x - origin.x).abs());
        previous = position;
        if (state.tick - start).is_multiple_of(u64::from(FIXED_HZ) / 2) {
            trace.push(sample(&state));
            state.audit().unwrap();
        }
        if !state.fighting() {
            break;
        }
    }
    trace.push(sample(&state));
    let elapsed = |ticks: Option<u64>| ticks.map(|t| t as f64 / f64::from(FIXED_HZ));
    json!({
        "initial_hash": format!("{initial_hash:016x}"),
        "final_hash": format!("{:016x}", state.observation_hash()),
        "destination_x": destination,
        "elapsed_seconds": (state.tick - start) as f64 / f64::from(FIXED_HZ),
        "match_ended": !state.fighting(),
        "damage_taken": health[player] - state.tanks[player].health,
        "enemy_health_lost": health[1 - player] - state.tanks[1 - player].health,
        "health": state.tanks.map(|tank| tank.health),
        "own_shots_fired": state.tanks[player].shots - own_shots,
        "first_fire_seconds": elapsed(first_fire),
        "first_useful_shot_seconds": elapsed(first_useful),
        "displacement_x": previous.x - origin.x,
        "max_displacement_x": max_displacement,
        "travel_distance": travel,
        "relocation": state.bots[player].last_relocation.as_ref().filter(|r| r.started >= start),
        "relocation_active_at_end": state.bots[player].relocation.is_some(),
        "shot_priority": state.bots[player].shot_priority,
        "shots": state.shot_observations.iter().filter(|s| s.tick >= start).collect::<Vec<_>>(),
        "trace": trace,
        "material": state.audit().unwrap(),
    })
}

/// Search ordinary combat for the Nth eligible hit with a reference route.
/// A missing opportunity is reported, never replaced by an injected hit/reset.
/// The subject declines, uses either retained move selector, or today's policy.
pub fn compare_relocation(
    config: ScorchedConfig,
    seed: u64,
    opportunity: u32,
    search_seconds: u32,
    seconds: u32,
) -> Value {
    assert!(opportunity > 0 && (1..=120).contains(&search_seconds) && (1..=60).contains(&seconds));
    assert!(config.demo && config.cpu_driving && config.cpu_policy == [CpuPolicy::Reactive; 2]);
    let mut state = ScorchedState::new(config, seed);
    let mut found = 0;
    for _ in 0..search_seconds * FIXED_HZ {
        for player in 0..2 {
            if ready_bot(&state, player).is_none() {
                continue;
            }
            let Some(_) = reference_destination(&state, player) else {
                continue;
            };
            found += 1;
            if found != opportunity {
                continue;
            }
            let elevation = ready_bot(&state, player).unwrap().aim.elevation;
            return json!({
                "found": true, "seed": seed, "opportunity": opportunity,
                "shape": format!("{:?}", state.config.shape), "player": player,
                "prefix": "original three-unit selector", "seconds": seconds,
                "intervention": "one relocation decision; suppress later subject relocations in every arm; opponent and range/footing unchanged",
                "checkpoint_hash": format!("{:016x}", state.observation_hash()),
                "checkpoint": sample(&state),
                "candidate_sites": state.relocation_sites(player, elevation),
                "checkpoint_shots": state.shot_observations(),
                "continue_aiming": continuation(&state, player, Choice::ContinueAiming, seconds),
                "reference_move": continuation(&state, player, Choice::Reference, seconds),
                "destination_move": continuation(&state, player, Choice::Destination, seconds),
                "current_move": continuation(&state, player, Choice::Current, seconds),
            });
        }
        if !state.fighting() {
            break;
        }
        reference_step(&mut state);
    }
    json!({"found": false, "seed": seed, "opportunity": opportunity,
        "shape": format!("{:?}", state.config.shape), "eligible_routes": found,
        "search_seconds": search_seconds, "final_tick": state.tick,
        "material": state.audit().unwrap()})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(shape: GrainShape) -> ScorchedConfig {
        ScorchedConfig {
            shape,
            demo: true,
            cpu_policy: [CpuPolicy::Reactive; 2],
            cpu_footing: [true; 2],
            suspension: true,
            ..Default::default()
        }
    }

    fn capture(shape: GrainShape, seed: u64, tick: u64) -> ScorchedState {
        let mut state = ScorchedState::new(config(shape), seed);
        while state.tick < tick {
            reference_step(&mut state);
        }
        state
    }

    #[test]
    fn post_hit_forks_preserve_physics_inflight_shots_and_learned_history() {
        let state = capture(GrainShape::Round, 7, 1035);
        assert_eq!(state.observation_hash(), 0x5c14e7ab67d0e8ef);
        assert!(state.bots[0].pending.is_some());
        assert_eq!(state.bots[0].aim.observations, 3);
        for choice in [Choice::ContinueAiming, Choice::Reference, Choice::Current] {
            let branch = fork(&state, 0, choice);
            assert_eq!(
                driving_lab::physics_snapshot(&branch),
                driving_lab::physics_snapshot(&state)
            );
            assert_eq!(json!(branch.tanks), json!(state.tanks));
            assert_eq!(json!(branch.driving), json!(state.driving));
            assert_eq!(json!(branch.shells), json!(state.shells));
            assert_eq!(
                json!(branch.shells.iter().map(|s| s.flight).collect::<Vec<_>>()),
                json!(state.shells.iter().map(|s| s.flight).collect::<Vec<_>>())
            );
            assert_eq!(json!(branch.bots[1]), json!(state.bots[1]));
            assert_eq!(
                json!(branch.bots[0].aim),
                json!(ready_bot(&state, 0).unwrap().aim)
            );
            for field in [
                "pending",
                "outcomes",
                "useful_shots",
                "last_hit",
                "last_shot",
            ] {
                assert_eq!(json!(branch.bots[0])[field], json!(state.bots[0])[field]);
            }
        }
        let declined = fork(&state, 0, Choice::Current);
        assert!(
            declined.bots[0].relocation.is_none(),
            "rough crater must be rejected"
        );
        assert_eq!(
            declined.bots[0].relocation_refusals,
            state.bots[0].relocation_refusals + 1
        );
    }

    #[test]
    fn selected_native_firing_patch_is_reached_and_shooting_resumes() {
        let state = capture(GrainShape::Hexagon, 42, 754);
        let report = continuation(&state, 1, Choice::Current, 7);
        assert_eq!(report["relocation"]["reason"], "arrived");
        assert!(report["max_displacement_x"].as_f64().unwrap() > 1.5);
        assert!(report["first_useful_shot_seconds"].as_f64().unwrap() < 6.0);
        assert!(report["relocation"]["ended"].as_u64().unwrap() <= state.tick + 240);
        assert_eq!(report, continuation(&state, 1, Choice::Current, 7));
    }

    #[test]
    fn a_retained_correction_gets_one_real_shot_and_preserves_the_firing_schedule() {
        let state = capture(GrainShape::Hexagon, 77, 1032);
        let mut candidate = fork(&state, 0, Choice::Current);
        assert_eq!(json!(candidate.tanks), json!(state.tanks));
        assert_eq!(json!(candidate.bots[0].aim), json!(state.bots[0].aim));
        assert_eq!(
            driving_lab::physics_snapshot(&candidate),
            driving_lab::physics_snapshot(&state)
        );
        assert!(candidate.bots[0].shot_priority.is_some());
        assert!(candidate.bots[0].relocation.is_none());
        while candidate.tick <= state.tanks[0].next_shot {
            reference_step(&mut candidate);
        }
        let shot = candidate.bots[0]
            .shot_priority
            .as_ref()
            .unwrap()
            .shot
            .unwrap();
        assert_eq!(candidate.bots[0].pending, Some(shot));
        let mut replay = candidate.clone();
        for _ in 0..180 {
            reference_step(&mut candidate);
            reference_step(&mut replay);
            assert_eq!(candidate.observation_hash(), replay.observation_hash());
        }
        let priority = candidate.bots[0].shot_priority.as_ref().unwrap();
        assert_eq!(priority.reason, "useful hit");
        assert!(priority.ended.unwrap() < priority.deadline);
        assert_eq!(candidate.bots[0].last_shot.unwrap().launch.id, shot);
        assert!(candidate.bots[0].last_shot.unwrap().damage[1] > 0.0);
        candidate.audit().unwrap();
    }

    #[test]
    fn repeated_hits_cannot_extend_an_unfired_correction_or_rearm_it() {
        let state = capture(GrainShape::Hexagon, 77, 1032);
        let mut candidate = fork(&state, 0, Choice::Current);
        let deadline = candidate.bots[0]
            .shot_priority
            .as_ref()
            .unwrap()
            .fire_deadline;
        let observations = candidate.bots[0].aim.observations;
        // Withhold the scheduled shot after the preference starts. The wait
        // must still expire; a stream of new hit notices cannot renew it.
        candidate.tanks[0].next_shot = u64::MAX;
        while candidate.tick <= deadline {
            candidate.bots[0].hit(candidate.tick, 1.0);
            candidate.advance(0.0);
            assert_eq!(
                candidate.bots[0]
                    .shot_priority
                    .as_ref()
                    .unwrap()
                    .fire_deadline,
                deadline
            );
        }
        let priority = candidate.bots[0].shot_priority.as_ref().unwrap();
        assert_eq!(priority.reason, "firing deadline");
        assert_eq!(priority.ended, Some(deadline));
        assert!(priority.shot.is_none());
        assert_eq!(candidate.bots[0].aim.observations, observations);
        candidate.bots[0].hit(candidate.tick, 1.0);
        candidate.bot_drive(0);
        assert_eq!(
            candidate.bots[0].shot_priority.as_ref().unwrap().started,
            state.tick
        );
        assert_eq!(
            candidate.bots[0].shot_priority.as_ref().unwrap().ended,
            Some(deadline)
        );
    }

    #[test]
    fn changed_target_or_new_barrel_obstruction_cancels_the_preference() {
        let state = capture(GrainShape::Hexagon, 77, 1032);
        let mut moved = fork(&state, 0, Choice::Current);
        let body = moved.tanks[1].body;
        let motion = moved.physics.motion(body).unwrap();
        moved
            .physics
            .set_pose(body, motion.position + Vec2::X * 2.0, motion.angle, true);
        moved.bot_drive(0);
        assert_eq!(
            moved.bots[0].shot_priority.as_ref().unwrap().reason,
            "geometry changed"
        );

        let mut blocked = fork(&state, 0, Choice::Current);
        let (pivot, muzzle) = blocked.muzzle(0);
        let id = PhysicsId::new(99_999);
        assert!(blocked.physics.insert_body(
            BodyId::new(id, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: (pivot + muzzle) * 0.5,
                ..Default::default()
            },
            &[ColliderSpec::convex_polygon(
                ColliderId::new(id, ColliderRole::PRIMARY, 0),
                vec![
                    Vec2::new(-0.3, -0.3),
                    Vec2::new(0.3, -0.3),
                    Vec2::new(0.3, 0.3),
                    Vec2::new(-0.3, 0.3),
                ]
            )],
        ));
        // Native ray queries use the completed-step broad phase.
        blocked.physics.step(DT);
        blocked.bot_drive(0);
        assert_eq!(
            blocked.bots[0].shot_priority.as_ref().unwrap().reason,
            "blocked barrel"
        );
        assert!(
            blocked.bots[0]
                .shot_priority
                .as_ref()
                .unwrap()
                .ended
                .is_some()
        );
    }

    #[test]
    fn driving_toggle_and_expired_observation_do_not_extend_the_preference() {
        let state = capture(GrainShape::Hexagon, 77, 1032);
        let mut candidate = fork(&state, 0, Choice::Current);
        let deadline = candidate.bots[0].shot_priority.as_ref().unwrap().deadline;
        candidate.command(Command::CpuDrive);
        for _ in 0..370 {
            candidate.advance(0.0);
        }
        assert!(candidate.tanks[0].shots > state.tanks[0].shots);
        assert!(
            candidate.bots[0]
                .shot_priority
                .as_ref()
                .unwrap()
                .ended
                .is_some()
        );
        candidate.command(Command::CpuDrive);
        candidate.bot_drive(0);
        assert_eq!(
            candidate.bots[0].shot_priority.as_ref().unwrap().deadline,
            deadline
        );

        // A late outcome must not retroactively mark an expired observation
        // window successful, even if driving was disabled during the flight.
        let mut bot = fork(&state, 0, Choice::Current).bots[0].clone();
        bot.priority_launched(999, state.tick);
        let mut outcome = bot.last_shot.unwrap();
        outcome.launch.id = 999;
        outcome.tick = deadline + 1;
        outcome.assessment = "useful hit";
        bot.priority_observed(&outcome);
        assert_eq!(
            bot.shot_priority.as_ref().unwrap().reason,
            "observation deadline"
        );
    }

    #[test]
    fn a_missing_hit_opportunity_is_not_replaced_by_a_synthetic_case() {
        let report = compare_relocation(config(GrainShape::Round), 7, 1, 1, 1);
        assert_eq!(report["found"], false);
        assert_eq!(report["eligible_routes"], 0);
    }
}
