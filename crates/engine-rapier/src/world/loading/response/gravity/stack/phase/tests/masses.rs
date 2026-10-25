use super::*;
use crate::world::loading::response::gravity::tests::masses::set_masses;

#[test]
fn native_mass_stack_source_orders_preserve_impact_work_and_support_reactions() {
    let mut cases = 0;
    for ids in [
        [1, 2, 3],
        [1, 3, 2],
        [2, 1, 3],
        [2, 3, 1],
        [3, 1, 2],
        [3, 2, 1],
    ] {
        for masses in [[5.0, 20.0], [20.0, 5.0], [13.0, 31.0], [80.0, 20.0]] {
            let mut reference = stack_world(ids, false, [1; 3], 1);
            set_masses(&mut reference, &ids[..2], &masses);
            reference.set_gravity(Vec2::ZERO);
            reference.set_velocity(body(ids[0]), Vec2::ZERO, 0.0, true);
            reference.step(0.00001);
            reference.set_pose(body(ids[0]), Vec2::ZERO, 0.0, true);
            reference.set_velocity(body(ids[0]), Vec2::X * 4.0, 0.0, true);
            reference.step(0.00001);
            let expected = reference.material_response_batches()[0]
                .result
                .as_ref()
                .unwrap();
            for reverse in [false, true] {
                for parts in [[1; 3], [4, 2, 4]] {
                    for speed in [0.0, 4.0] {
                        let mut world = stack_world(ids, reverse, parts, 2);
                        set_masses(&mut world, &ids[..2], &masses);
                        world.set_pose(body(ids[0]), Vec2::ZERO, 0.0, true);
                        world.set_velocity(body(ids[0]), Vec2::X * speed, 0.0, true);
                        let mut replay = world.clone();
                        let policy = GravitySettling::BoundedNumerical;
                        let dt = if speed == 0.0 { 0.01 } else { 0.000001 };
                        let step = world.step_material_flat_gravity(dt, policy).unwrap_or_else(|e| {
                            panic!("ids={ids:?} masses={masses:?} reverse={reverse} parts={parts:?} speed={speed}: {e:?}")
                        });
                        let repeated = replay.step_material_flat_gravity(dt, policy).unwrap();
                        assert_eq!(step.records, repeated.records);
                        assert_eq!(step.numerical, repeated.numerical);
                        assert_eq!(
                            world.snapshot_bytes().unwrap(),
                            replay.snapshot_bytes().unwrap()
                        );
                        assert_eq!(
                            step.durations.iter().map(|d| f64::from(*d)).sum::<f64>(),
                            f64::from(dt)
                        );
                        let actual = ids[..2]
                            .iter()
                            .map(|id| f64::from(world.body_mass(body(*id)).unwrap()))
                            .collect::<Vec<_>>();
                        for entry in step.records {
                            let GravityRoles::Stack { bodies, .. } = entry.roles else {
                                panic!("stack roles")
                            };
                            assert_eq!(bodies, ids.map(body));
                            match entry.record {
                                GravityRecord::Impact(hit) => {
                                    near(
                                        hit.response.budget.components[0].available,
                                        8.0 * actual[0],
                                        1.0e-5 * 8.0 * actual[0],
                                    );
                                    near(
                                        loss(&hit.response),
                                        loss(expected),
                                        1.0e-5 * 8.0 * actual[0],
                                    );
                                    assert_eq!(hit.response.roots, [hit.event.id]);
                                }
                                GravityRecord::StackSupport(s) => {
                                    assert_eq!(
                                        s.forces,
                                        [actual[0] * 10.0, (actual[0] + actual[1]) * 10.0]
                                    );
                                    assert_eq!(s.kick_energy, s.projection_energy);
                                    assert_eq!(s.gravity_work, 0.0);
                                }
                                _ => {}
                            }
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 192);
}
