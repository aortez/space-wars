use super::*;

const FIXTURES: [Fixture; 3] = [Fixture::Flat, Fixture::Slope, Fixture::MovingPlanet];
const MODES: [BlastMode; 2] = [BlastMode::Grains, BlastMode::GrainPulse];

#[test]
fn most_of_an_unobstructed_pile_returns_to_terrain_and_can_be_reblasted() {
    for fixture in [Fixture::Flat, Fixture::MovingPlanet] {
        for grain_shape in [GrainShape::Round, GrainShape::Hexagon] {
            let mut lab = BlastLab::new(BlastLabConfig {
                fixture,
                grain_shape,
                mode: BlastMode::Grains,
                deposition: true,
                max_loose_bodies: 192,
                friction: 0.6,
                ..Default::default()
            })
            .unwrap();
            let blast = shot(&lab, 18.0);
            let released = lab.blast(blast).unwrap();
            assert!(released.admitted && released.spawned_grains > 50);
            for _ in 0..600 {
                lab.step();
                assert_eq!(lab.audit().unwrap().removed, 0);
                let d = lab.settling_diagnostics();
                assert_eq!(
                    d.unsupported + d.moving + d.waiting + d.no_room + d.obstructed + d.budget,
                    lab.grains.len()
                );
            }
            eprintln!(
                "{fixture:?} {grain_shape:?}: {} / {} returned, {:?}",
                lab.deposited_cells(),
                released.spawned_grains,
                lab.settling_diagnostics()
            );
            assert!(
                lab.deposited_cells() as usize * 5 >= released.spawned_grains * 4,
                "{fixture:?} {grain_shape:?}: {} / {} returned, {:?}",
                lab.deposited_cells(),
                released.spawned_grains,
                lab.settling_diagnostics()
            );
            let before = lab.deposited_cells();
            let second = lab
                .blast(Blast {
                    center: lab.surface_point(0.0, 0.5),
                    ..blast
                })
                .unwrap();
            assert!(second.admitted && second.spawned_grains > 0);
            let mut replay = lab.clone();
            for _ in 0..600 {
                lab.step();
                replay.step();
                assert_eq!(lab.content_motion_hash(), replay.content_motion_hash());
                lab.audit().unwrap();
            }
            assert!(lab.deposited_cells() > before);
        }
    }
}

#[test]
fn conserved_deposition_and_reblasts_work_in_real_stationary_and_moving_fixtures() {
    for fixture in [Fixture::Flat, Fixture::MovingPlanet] {
        for grain_shape in [GrainShape::Round, GrainShape::Hexagon] {
            let mut lab = BlastLab::new(BlastLabConfig {
                fixture,
                grain_shape,
                mode: BlastMode::Grains,
                deposition: true,
                max_loose_bodies: 64,
                ..Default::default()
            })
            .unwrap();
            let mut replay = lab.clone();
            let mut total_released = 0;
            let mut aim = (lab.surface_point(0.0, 0.25) - lab.ground_motion().position)
                .rotate_radians(-lab.ground_motion().angle);
            let mut aiming_at_deposit = false;
            let mut reblasted_deposit = false;
            for _ in 0..4 {
                let blast = Blast {
                    center: lab.ground_motion().position
                        + aim.rotate_radians(lab.ground_motion().angle),
                    radius: 0.75,
                    speed: 6.0,
                };
                let result = lab.blast(blast).unwrap();
                assert_eq!(result, replay.blast(blast).unwrap());
                assert!(result.admitted, "{fixture:?} {grain_shape:?}: {result:?}");
                if aiming_at_deposit {
                    assert!(result.spawned_grains > 0);
                    reblasted_deposit = true;
                }
                total_released += result.spawned_grains as u64;
                let after_blast = lab.bodies[0].terrain.clone();
                for _ in 0..300 {
                    lab.step();
                    replay.step();
                    assert_eq!(lab.content_motion_hash(), replay.content_motion_hash());
                    assert_eq!(lab.audit().unwrap().removed, 0);
                    assert!(lab.loose_body_count() <= 64);
                }
                let ground = &lab.bodies[0].terrain;
                let deposited = ground
                    .cells()
                    .iter()
                    .zip(after_blast.cells())
                    .enumerate()
                    .filter(|(_, (now, before))| {
                        now.material != MaterialId::VOID && before.material == MaterialId::VOID
                    })
                    .map(|(i, _)| {
                        ground.cell_center(CellCoord::new(
                            (i as u32 % ground.width()) as i32,
                            (i as u32 / ground.width()) as i32,
                        ))
                    })
                    .min_by(|a, b| a.distance_to(aim).total_cmp(&b.distance_to(aim)));
                aiming_at_deposit = deposited.is_some();
                if let Some(position) = deposited {
                    aim = position;
                }
            }
            assert!(
                lab.deposited_cells() > 0,
                "{fixture:?} {grain_shape:?}: no deposition"
            );
            // Solid fragments also count toward loose_body_count; all grains
            // may return while a disconnected rigid fragment remains physical.
            assert_eq!(
                lab.deposited_cells() + lab.grains.len() as u64,
                total_released
            );
            assert_eq!(lab.rejected_blasts, 0);
            assert!(
                reblasted_deposit,
                "{fixture:?} {grain_shape:?}: no deposited cell reblasted"
            );
        }
    }
}

#[test]
fn one_tick_pulse_has_the_same_motion_as_the_instantaneous_kick() {
    let config = BlastLabConfig {
        mode: BlastMode::Grains,
        pulse_ticks: 1,
        ..BlastLabConfig::default()
    };
    let mut kick = BlastLab::new(config).unwrap();
    let mut pulse = BlastLab::new(BlastLabConfig {
        mode: BlastMode::GrainPulse,
        ..config
    })
    .unwrap();
    let blast = shot(&kick, 18.0);
    assert_eq!(kick.blast(blast).unwrap(), pulse.blast(blast).unwrap());
    for _ in 0..60 {
        assert_eq!(kick.content_motion_hash(), pulse.content_motion_hash());
        kick.step();
        pulse.step();
    }
    assert_eq!(pulse.active_pulses(), 0);
}

#[test]
fn cells_become_grains_at_their_original_positions_with_material_and_point_motion() {
    for fixture in FIXTURES {
        let mut lab = BlastLab::new(BlastLabConfig {
            fixture,
            mode: BlastMode::Grains,
            ..BlastLabConfig::default()
        })
        .unwrap();
        for _ in 0..60 {
            lab.step();
        }
        let source = lab.bodies[0].terrain.clone();
        let parent = lab.bodies().next().unwrap().1;
        let parent_center = lab
            .physics
            .center_of_mass(lab.bodies[0].assembly.body())
            .unwrap();
        let result = lab.blast(shot(&lab, 0.0)).unwrap();
        assert!(result.admitted && result.spawned_grains > 0);
        assert_eq!(result.spawned_fragments, 0);
        assert_eq!(result.spawned_grains as u64, result.selected_cells);
        for (grain, motion) in lab.grains() {
            let local = (motion.position - parent.position).rotate_radians(-parent.angle);
            let coordinate = source.local_to_cell(local).unwrap();
            assert_eq!(source.cell(coordinate), Some(grain.cell()));
            assert!(source.cell_center(coordinate).distance_to(local) < 0.0001);
            assert_eq!(
                lab.bodies[0].terrain.cell(coordinate).unwrap().material,
                MaterialId::VOID
            );
            let offset = motion.position - parent_center;
            let velocity =
                parent.linear_velocity + Vec2::new(-offset.y, offset.x) * parent.angular_velocity;
            assert!(motion.linear_velocity.distance_to(velocity) < 0.0001);
            assert_eq!(motion.angular_velocity, parent.angular_velocity);
        }
        assert_eq!(lab.audit().unwrap().removed, 0);
    }
}

#[test]
fn grains_eject_fall_back_and_find_actual_support_in_all_fixtures() {
    for fixture in FIXTURES {
        let mut lab = BlastLab::new(BlastLabConfig {
            fixture,
            mode: BlastMode::Grains,
            ..BlastLabConfig::default()
        })
        .unwrap();
        for _ in 0..60 {
            lab.step();
        }
        assert!(lab.blast(shot(&lab, 18.0)).unwrap().admitted);
        let mut peak: f32 = 0.0;
        let mut airborne = 0;
        for _ in 0..600 {
            lab.step();
            assert_eq!(lab.audit().unwrap().removed, 0);
            let stats = lab.motion_stats();
            peak = peak.max(stats.max_clearance);
            airborne = airborne.max(stats.above_surface_cells);
        }
        let final_state = lab.motion_stats();
        assert!(airborne > 0 && peak > 1.0, "{fixture:?}: no ejection");
        assert!(
            final_state.max_clearance < peak - 0.5,
            "{fixture:?}: material did not fall back"
        );
        assert!(
            final_state.supported_slow_cells > 0,
            "{fixture:?}: no supported resting material"
        );
        assert_eq!(lab.rejected_blasts, 0);
    }
}

#[test]
fn repeat_impacts_and_clones_including_active_pulses_match_fresh_replay() {
    for fixture in FIXTURES {
        for mode in MODES {
            let config = BlastLabConfig {
                fixture,
                mode,
                ..BlastLabConfig::default()
            };
            let mut lab = BlastLab::new(config).unwrap();
            let mut replay = BlastLab::new(config).unwrap();
            for _ in 0..60 {
                lab.step();
                replay.step();
            }
            let first = shot(&lab, 18.0);
            assert_eq!(lab.blast(first).unwrap(), replay.blast(first).unwrap());
            lab.step();
            replay.step();
            let mut restored = lab.clone();
            if mode == BlastMode::GrainPulse {
                assert_eq!(restored.active_pulses(), 1);
            }
            for tick in 0..240 {
                if tick == 30 {
                    let grain_ids: Vec<_> = lab.grains().map(|(grain, _)| grain.id()).collect();
                    let second = Blast {
                        center: lab.grains().next().unwrap().1.position,
                        radius: 2.0,
                        speed: 9.0,
                    };
                    let result = lab.blast(second).unwrap();
                    assert!(result.admitted && result.loose_bodies_hit > 0);
                    assert_eq!(result, replay.blast(second).unwrap());
                    assert_eq!(result, restored.blast(second).unwrap());
                    for id in grain_ids {
                        assert!(lab.grains().any(|(grain, _)| grain.id() == id));
                    }
                }
                lab.step();
                replay.step();
                restored.step();
                assert_eq!(lab.content_motion_hash(), replay.content_motion_hash());
                assert_eq!(lab.content_motion_hash(), restored.content_motion_hash());
                assert_eq!(lab.audit().unwrap().removed, 0);
            }
            assert_eq!(lab.active_pulses(), 0);
        }
    }
}

#[test]
fn rejected_grain_blast_preserves_existing_material_motion_and_active_pulses() {
    for mode in MODES {
        let mut lab = BlastLab::new(BlastLabConfig {
            mode,
            max_loose_bodies: 100,
            ..BlastLabConfig::default()
        })
        .unwrap();
        assert!(lab.blast(shot(&lab, 18.0)).unwrap().admitted);
        let before = lab.clone();
        let result = lab
            .blast(Blast {
                center: lab.surface_point(8.0, 0.5),
                ..shot(&lab, 18.0)
            })
            .unwrap();
        assert!(!result.admitted);
        assert_eq!(result.rejection, Some("loose-body limit"));
        assert_eq!(
            (
                result.spawned_fragments,
                result.spawned_grains,
                result.accelerated_bodies
            ),
            (0, 0, 0)
        );
        assert_eq!(lab.bodies[0].terrain, before.bodies[0].terrain);
        assert_eq!(lab.next_id, before.next_id);
        assert_eq!(lab.pulses, before.pulses);
        let states = |state: &BlastLab| {
            state
                .grains()
                .map(|(grain, motion)| (grain.id(), grain.cell(), motion))
                .collect::<Vec<_>>()
        };
        assert_eq!(states(&lab), states(&before));
        assert_eq!(lab.audit().unwrap().removed, 0);
    }
}

#[test]
fn pulse_limit_is_explicit_and_finishes_after_the_requested_ticks() {
    let mut lab = BlastLab::new(BlastLabConfig {
        mode: BlastMode::GrainPulse,
        ..BlastLabConfig::default()
    })
    .unwrap();
    for _ in 0..MAX_ACTIVE_PULSES {
        assert!(lab.blast(shot(&lab, 1.0)).unwrap().admitted);
    }
    let before = lab.clone();
    let result = lab.blast(shot(&lab, 1.0)).unwrap();
    assert_eq!(result.rejection, Some("active-pulse limit"));
    assert_eq!(lab.pulses, before.pulses);
    assert_eq!(lab.next_id, before.next_id);
    for (actual, expected) in lab.grains().zip(before.grains()) {
        assert_eq!(actual.1, expected.1);
    }
    for _ in 0..lab.config.pulse_ticks - 1 {
        lab.step();
    }
    assert_eq!(lab.active_pulses(), MAX_ACTIVE_PULSES);
    lab.step();
    assert_eq!(lab.active_pulses(), 0);
    lab.audit().unwrap();
}

#[test]
fn stationary_airborne_contact_clusters_do_not_count_as_supported() {
    let mut lab = BlastLab::new(BlastLabConfig {
        mode: BlastMode::Grains,
        ..BlastLabConfig::default()
    })
    .unwrap();
    lab.blast(shot(&lab, 0.0)).unwrap();
    let placements: Vec<_> = lab
        .grains()
        .map(|(grain, motion)| (grain.body(), motion.position + Vec2::new(0.0, 20.0)))
        .collect();
    for (body, position) in placements {
        lab.physics.set_pose(body, position, 0.0, true);
    }
    // The mechanics world has zero global gravity: prime contacts without the
    // fixture's gravity so an airborne cluster can be both touching and still.
    lab.physics.step(DT);
    assert!(lab.physics.pair_diagnostics().active_contact_pairs > 0);
    assert_eq!(lab.motion_stats().supported_slow_cells, 0);
    lab.audit().unwrap();
}
