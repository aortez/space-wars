use super::*;

fn step(state: &mut ScorchedState, actions: &[Action]) {
    ScorchedScenario::step(
        state,
        actions,
        Duration::from_secs_f64(1.0 / FIXED_HZ as f64),
    );
}

#[test]
fn drop_comparison_distinguishes_hard_weak_landings_and_conserves_both_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for height in [DropHeight::Low, DropHeight::High] {
            for strength in [BankStrength::Off, BankStrength::Weak, BankStrength::Strong] {
                let mut state = ScorchedState::new(
                    ScorchedConfig {
                        shape,
                        cohesion: strength,
                        drop_height: Some(height),
                        breakup: true,
                        ..Default::default()
                    },
                    42,
                );
                let field = state.terrain[1].terrain.hash();
                for _ in 0..120 {
                    step(&mut state, &[]);
                }
                assert_eq!(state.terrain[1].terrain.hash(), field);
                assert_eq!(state.impact_diagnostics().failures, 0);
                let mut replay = state.clone();
                for tick in 0..600 {
                    step(&mut state, &[]);
                    step(&mut replay, &[]);
                    if tick % 60 == 0 {
                        state.audit().unwrap();
                        assert_eq!(state.observation_hash(), replay.observation_hash());
                    }
                }
                let d = state.impact_diagnostics();
                if height == DropHeight::High && strength == BankStrength::Weak {
                    assert!(d.failures > 0, "{shape:?} {height:?} {strength:?}: {d:?}");
                    assert!((1..=16).contains(&d.released_cells));
                } else {
                    assert_eq!(d.failures, 0, "{shape:?} {height:?} {strength:?}: {d:?}");
                }
                assert_eq!(state.shots, 0);
            }
        }
    }
}

#[test]
fn drop_controls_cycle_height_strength_and_shape_and_reset_the_fixture() {
    let mut state = ScorchedState::new(ScorchedConfig::default(), 42);
    for _ in 0..3 {
        state.command(Command::Barrage);
    }
    assert_eq!(state.config.drop_height, Some(DropHeight::Low));
    assert!(state.config.breakup);
    state.command(Command::Barrage);
    assert_eq!(state.config.drop_height, Some(DropHeight::High));
    state.command(Command::Collapse);
    assert_eq!(state.config.cohesion, BankStrength::Strong);
    let before = state.observation_hash();
    for _ in 0..250 {
        step(&mut state, &[]);
    }
    state.command(Command::Reset);
    assert_eq!(state.observation_hash(), before);
    state.command(Command::Shape);
    assert_eq!(state.config.shape, GrainShape::Round);
    assert_eq!(state.config.drop_height, Some(DropHeight::High));
    state.command(Command::Barrage);
    assert_eq!(state.config.drop_height, None);
    assert!(!state.config.undercut);
    assert_eq!(state.config.bombardment_seconds, 0);
}

#[test]
fn identical_support_cut_compares_bank_strength_and_replays_with_both_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut before_cut = Vec::new();
        for cohesion in [BankStrength::Off, BankStrength::Weak, BankStrength::Strong] {
            let mut state = ScorchedState::new(
                ScorchedConfig {
                    shape,
                    cohesion,
                    undercut: true,
                    ..Default::default()
                },
                42,
            );
            for _ in 0..121 {
                step(&mut state, &[]);
            }
            assert_eq!(state.impacts, 1);
            assert_eq!(state.loose_cells(), 33);
            before_cut.push(state.terrain[0].terrain.hash());
            let mut replay = state.clone();
            for tick in 0..600 {
                step(&mut state, &[]);
                step(&mut replay, &[]);
                if tick % 60 == 0 {
                    state.audit().unwrap();
                    assert_eq!(state.observation_hash(), replay.observation_hash());
                }
            }
            let d = state.cohesion_diagnostics();
            if cohesion == BankStrength::Weak {
                assert_eq!(d.failures, 1);
                assert_eq!(d.released_cells, 48);
                assert_eq!(state.terrain.len(), 2);
                let m = state
                    .physics
                    .motion(state.terrain[1].assembly.body())
                    .unwrap();
                assert!(m.position.y < 0.0, "chunk must actually fall: {m:?}");
                assert!(state.deposited_cells() > 0);
            } else {
                assert_eq!(d.failures, 0);
                assert_eq!(state.terrain.len(), 1);
                if cohesion == BankStrength::Strong {
                    assert!(d.held_checks > 0);
                }
            }
        }
        assert!(before_cut.iter().all(|h| *h == before_cut[0]));
    }
}

#[test]
fn bank_controls_reset_the_same_fixture_and_chords_do_not_toggle_dirt_or_slopes() {
    let mut state = ScorchedState::new(ScorchedConfig::default(), 42);
    state.command(Command::Barrage);
    assert_eq!(state.config.bombardment_seconds, 60);
    state.command(Command::Barrage);
    assert!(state.config.undercut);
    assert_eq!(state.config.cohesion, BankStrength::Weak);
    state.command(Command::Collapse);
    assert_eq!(state.config.cohesion, BankStrength::Strong);
    assert!(!state.config.slumping);
    let hash = state.terrain[0].terrain.hash();
    for _ in 0..180 {
        step(&mut state, &[]);
    }
    state.command(Command::Reset);
    assert_eq!(state.terrain[0].terrain.hash(), hash);
    state.command(Command::Barrage);
    assert!(!state.config.undercut);
    let c = ScorchedAction::Controls(Controls {
        select: true,
        shape: true,
        ..Default::default()
    })
    .encode();
    for _ in 0..60 {
        step(&mut state, std::slice::from_ref(&c));
    }
    assert!(state.config.undercut);
    assert_eq!(state.tick, 60);
    assert_eq!(state.config.shape, GrainShape::Hexagon);
    step(
        &mut state,
        &[ScorchedAction::Controls(Controls {
            shape: true,
            ..Default::default()
        })
        .encode()],
    );
    assert_eq!(
        state.config.shape,
        GrainShape::Hexagon,
        "releasing chord modifier is not a new press"
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
fn saturated_crater_still_pushes_existing_dirt_once_and_damages_tanks() {
    use engine_rapier::terrain::{PreparedRelease, TerrainBodyMut};
    use engine_terrain::{Brush, EditMode, TerrainEdit};

    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for free_slots in [0, 1] {
            let mut state = ScorchedState::new(
                ScorchedConfig {
                    shape,
                    max_grains: 5 + free_slots,
                    ..Default::default()
                },
                42,
            );
            let ground = &mut state.terrain[0];
            let plan = PreparedRelease::new(
                &ground.terrain,
                &[TerrainEdit {
                    brush: Brush::Circle {
                        center: ground
                            .terrain
                            .local_to_cell(Vec2::new(-10.0, -15.0))
                            .unwrap(),
                        radius: 1,
                    },
                    mode: EditMode::Remove,
                }],
            )
            .unwrap();
            assert_eq!(plan.grain_count(), 5);
            let released = state
                .loose
                .commit(
                    &mut state.physics,
                    TerrainBodyMut {
                        terrain: &mut ground.terrain,
                        geometry: &mut ground.geometry,
                        assembly: &mut ground.assembly,
                    },
                    plan,
                    &mut state.next_id,
                )
                .unwrap();
            assert!(released.fragments.is_empty());
            ground.hash = ground.terrain.hash();
            let point = Vec2::new(0.0, hill(0.0, 42));
            let bodies = state.loose.iter().map(|g| g.body()).collect::<Vec<_>>();
            for (i, &body) in bodies.iter().enumerate() {
                // Four nearby grains and one outside the impulse radius.
                let offset = if i == 4 { 10.0 } else { 0.5 + i as f32 * 0.6 };
                state
                    .physics
                    .set_pose(body, point + Vec2::Y * offset, 0.0, true);
                state.physics.set_velocity(body, Vec2::ZERO, 0.0, true);
            }
            let tank = state.tanks[0].body;
            state
                .physics
                .set_pose(tank, point + Vec2::new(4.0, 2.0), 0.0, true);
            let initial = state.audit().unwrap();
            let field = state.terrain[0].terrain.clone();
            let next_id = state.next_id;
            let before = bodies
                .iter()
                .map(|&b| state.physics.motion(b).unwrap())
                .collect::<Vec<_>>();
            let mut replay = state.clone();
            state.explode(point);
            replay.explode(point);
            assert_eq!(state.rejected_blasts, 1);
            let capacity = state.last_capacity_rejection.unwrap();
            assert_eq!(capacity.available, free_slots);
            assert!(!capacity.fits());
            assert_eq!(state.terrain[0].terrain, field);
            assert_eq!(state.next_id, next_id);
            assert_eq!(state.audit().unwrap(), initial);
            assert!(state.tanks[0].health < 100.0);
            for (i, &body) in bodies.iter().enumerate() {
                let after = state.physics.motion(body).unwrap();
                assert_eq!(after.position, before[i].position);
                assert_eq!(after.angle, before[i].angle);
                if i == 4 {
                    assert_eq!(after, before[i]);
                } else {
                    let offset = state.physics.center_of_mass(body).unwrap() - point;
                    let expected =
                        offset / offset.length() * (16.0 * (1.0 - offset.length() / 3.5));
                    assert!(
                        after.linear_velocity.distance_to(expected) < 0.0001,
                        "{shape:?}: {after:?}"
                    );
                }
            }
            for _ in 0..120 {
                step(&mut state, &[]);
                step(&mut replay, &[]);
            }
            assert_eq!(state.observation_hash(), replay.observation_hash());
            state.audit().unwrap();
        }
    }
}

#[test]
fn admitted_crater_pushes_each_new_grain_once() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let mut state = ScorchedState::new(
            ScorchedConfig {
                shape,
                ..Default::default()
            },
            42,
        );
        let point = Vec2::new(0.0, hill(0.0, 42));
        state.explode(point);
        assert_eq!(state.rejected_blasts, 0);
        assert_eq!(state.last_capacity_rejection, None);
        assert!(!state.loose.is_empty());
        for grain in state.loose.iter() {
            let offset = state.physics.center_of_mass(grain.body()).unwrap() - point;
            let expected = offset / offset.length() * (16.0 * (1.0 - offset.length() / 3.5));
            let actual = state.physics.motion(grain.body()).unwrap().linear_velocity;
            assert!(
                actual.distance_to(expected) < 0.0001,
                "{shape:?}: {actual:?}"
            );
        }
        state.audit().unwrap();
    }
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
    assert_eq!(
        state.selected, 0,
        "Tank waits for release to distinguish a drive chord"
    );
    step(&mut state, &[]);
    assert_eq!(state.selected, 1);
    step(&mut state, &[select]);
    step(&mut state, &[]);
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
        flight: None,
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

#[test]
fn hillside_shells_release_and_redeposit_conserved_dirt_with_replay() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for strength in [BankStrength::Off, BankStrength::Weak, BankStrength::Strong] {
            let mut state = ScorchedState::new(
                ScorchedConfig {
                    hillside: true,
                    demo: true,
                    shape,
                    ..Default::default()
                }
                .with_destruction(strength),
                42,
            );
            let mut replay = state.clone();
            let mut peak_loose = 0;
            for tick in 0..1800 {
                step(&mut state, &[]);
                step(&mut replay, &[]);
                peak_loose = peak_loose.max(state.loose_cells());
                if tick % 60 == 0 {
                    state.audit().unwrap();
                    assert_eq!(
                        state.observation_hash(),
                        replay.observation_hash(),
                        "{shape:?} {strength:?} tick {tick}"
                    );
                }
            }
            state.audit().unwrap();
            assert_eq!(
                state.observation_hash(),
                replay.observation_hash(),
                "{shape:?} {strength:?} final state"
            );
            assert_eq!(state.shots, 3, "{shape:?} {strength:?}");
            assert_eq!(state.impacts, 3, "{shape:?} {strength:?}");
            assert!(
                peak_loose > 0,
                "{shape:?} {strength:?}: no material released"
            );
            assert!(
                state.deposited_cells() > 0,
                "{shape:?} {strength:?}: no material returned"
            );
            // System math differences can change the craters and landing severity.
            // The controlled bank, drop and roof tests require collapse, impact
            // fracture and loss of tank support; this encounter can validly hold.
            if strength != BankStrength::Weak {
                assert_eq!(
                    state.cohesion_diagnostics().failures,
                    0,
                    "{shape:?} {strength:?}"
                );
                assert_eq!(
                    state.impact_diagnostics().failures,
                    0,
                    "{shape:?} {strength:?}"
                );
            }
        }
    }
}

#[test]
fn hillside_manual_fire_and_combined_presets_share_the_demo_weapon_path() {
    let mut state = ScorchedState::new(ScorchedConfig::default(), 42);
    state.command(Command::Hillside);
    assert!(state.config.demo && state.config.hillside);
    state.command(Command::Demo);
    assert!(!state.config.demo);
    assert_eq!(state.selected, 1);
    for _ in 0..120 {
        step(&mut state, &[]);
    }
    assert_eq!(state.shots, 0);
    state.command(Command::Fire);
    for _ in 0..600 {
        step(&mut state, &[]);
    }
    assert_eq!(state.shots, 1);
    assert_eq!(state.impacts, 1);
    assert_eq!(
        state.cohesion_diagnostics().failures,
        0,
        "one shallow crater still holds"
    );
    for _ in 0..2 {
        state.command(Command::Fire);
        for _ in 0..480 {
            step(&mut state, &[]);
        }
    }
    assert_eq!(state.shots, 3);
    assert_eq!(state.impacts, 3);
    assert!(state.deposited_cells() > 0);
    state.audit().unwrap();
    for strength in [BankStrength::Strong, BankStrength::Off, BankStrength::Weak] {
        state.command(Command::Collapse);
        assert_eq!(state.config.cohesion, strength);
        assert_eq!(state.config.slumping, strength != BankStrength::Off);
        assert_eq!(state.config.breakup, strength != BankStrength::Off);
        assert_eq!(state.tick, 0);
    }
    state.command(Command::Barrage);
    assert!(!state.config.hillside);
}

#[test]
fn roof_shot_cracks_weak_material_and_preserves_a_strong_span() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for strength in [BankStrength::Off, BankStrength::Weak, BankStrength::Strong] {
            let mut s = ScorchedState::new(
                ScorchedConfig {
                    roof: true,
                    demo: true,
                    shape,
                    ..Default::default()
                }
                .with_destruction(strength),
                42,
            );
            let sound = s.terrain[0]
                .terrain
                .roof_span(
                    engine_terrain::CellCoord::new(48, 40),
                    Vec2::new(0.0, -GRAVITY),
                    500.0,
                )
                .unwrap();
            assert!(!sound.overloaded(), "intact weak roof holds");
            for tick in 0..1800 {
                step(&mut s, &[]);
                if tick % 60 == 0 {
                    s.audit().unwrap();
                }
            }
            assert_eq!(s.shots, 1);
            assert_eq!(s.impacts, 1);
            let d = s.cohesion_diagnostics();
            if strength == BankStrength::Weak {
                assert_eq!(d.roof_failures, 1);
                assert!(d.failures > d.roof_failures);
                assert!(s.impact_diagnostics().failures > 0);
                assert!(s.deposited_cells() > 0);
                assert!(s.physics.motion(s.tanks[0].body).unwrap().position.y < 8.0);
            } else {
                assert_eq!(d.failures, 0);
                if strength == BankStrength::Strong {
                    assert!(d.roof_held_checks > 0);
                }
                assert!(s.physics.motion(s.tanks[0].body).unwrap().position.y > 13.0);
            }
        }
    }
}

#[test]
fn roof_controls_reset_the_scene_and_preserve_shared_preset_selection() {
    let mut s = ScorchedState::new(ScorchedConfig::default(), 42);
    s.command(Command::Hillside);
    s.command(Command::Barrage);
    assert!(s.config.roof && s.config.demo && !s.config.hillside);
    for _ in 0..8 {
        step(
            &mut s,
            &[ScorchedAction::Controls(Controls {
                collapse: true,
                ..Default::default()
            })
            .encode()],
        );
    }
    assert_eq!(s.config.cohesion, BankStrength::Strong);
    s.command(Command::Shape);
    assert!(s.config.roof && s.config.demo);
    s.command(Command::Demo);
    assert!(!s.config.demo);
    s.command(Command::Fire);
    assert_eq!(s.shots, 1);
    s.command(Command::Reset);
    assert_eq!(s.shots, 0);
    assert!(s.config.roof);
    s.command(Command::Barrage);
    assert!(!s.config.roof && !s.config.hillside);
    s.command(Command::Roof);
    assert!(s.config.roof && s.config.demo);
}
