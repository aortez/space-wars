use super::*;

fn corrupt(world: &PhysicsWorld) {
    assert!(PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).is_err());
}

#[test]
fn gravity_stack_state_rejects_incoherent_owners_topology_and_numerical_limits() {
    let world = enrolled([11, 2, 7], GravitySettling::BoundedNumerical, 3);
    for case in 0..48 {
        let mut w = world.clone();
        match case {
            0 => {
                w.loading
                    .as_mut()
                    .unwrap()
                    .gravity
                    .as_mut()
                    .unwrap()
                    .history
                    .revision = 5
            }
            1 => {
                w.loading
                    .as_mut()
                    .unwrap()
                    .gravity
                    .as_mut()
                    .unwrap()
                    .history
                    .reference[2] = 0.0
            }
            2 => {
                w.loading
                    .as_mut()
                    .unwrap()
                    .gravity
                    .as_mut()
                    .unwrap()
                    .history
                    .reference[3] = f64::INFINITY
            }
            3 => {
                w.loading
                    .as_mut()
                    .unwrap()
                    .gravity
                    .as_mut()
                    .unwrap()
                    .history
                    .scales[0] *= 2.0
            }
            4 => {
                w.loading
                    .as_mut()
                    .unwrap()
                    .gravity
                    .as_mut()
                    .unwrap()
                    .history
                    .errors[0] = f64::NAN
            }
            5 => {
                w.loading
                    .as_mut()
                    .unwrap()
                    .gravity
                    .as_mut()
                    .unwrap()
                    .history
                    .errors[1] = 1.0
            }
            6 => {
                w.loading
                    .as_mut()
                    .unwrap()
                    .gravity
                    .as_mut()
                    .unwrap()
                    .history
                    .settling = GravitySettling::Refuse
            }
            7 => state_mut(&mut w).corrections.impulse = f64::NAN,
            8 => state_mut(&mut w).corrections.work = -1.0e-12,
            9 => state_mut(&mut w).corrections.work = f64::INFINITY,
            10 => state_mut(&mut w).corrections.impulse = 1.0,
            11 => state_mut(&mut w).corrections.work = 1.0,
            12 => state_mut(&mut w).corrections.work = 0.0,
            13 => active(&mut w).plates[1].body = active(&mut w).plates[0].body,
            14 => active(&mut w).plates[2].source = active(&mut w).plates[1].source,
            15 => active(&mut w).plates[1].source.region.incarnation += 1,
            16 => active(&mut w).plates[1].source.patch += 1,
            17 => active(&mut w).plates[1].endpoint.position[0] = 1.000001,
            18 => active(&mut w).plates[2].endpoint.position[0] = 2.000001,
            19 => active(&mut w).plates[1].endpoint.velocity[0] = 1.0e-8,
            20 => active(&mut w).plates[1].endpoint.mass = 19.0,
            21 => active(&mut w).plates[1].endpoint.local_com[0] = f32::NAN,
            22 => active(&mut w).plates[2].material.parameters[1] *= 2.0,
            23 => active(&mut w).plates[1].colliders[0] = None,
            24 => active(&mut w).plates[1].colliders.swap(0, 1),
            25 => active(&mut w).plates[1].colliders[4] = active(&mut w).plates[1].colliders[0],
            26 => active(&mut w).plates[1].slices = 3,
            27 => active(&mut w).plates[2].copies = 0,
            28 => active(&mut w).capture.touch_tolerance = 0.0,
            29 => active(&mut w).integration.dt = 1.0,
            30 => active(&mut w).integration.warmstart_coefficient = f32::NAN,
            31 => active(&mut w).plates[1].additional_iterations = usize::MAX,
            32 => active(&mut w).support.sources.swap(0, 1),
            33 => active(&mut w).support.id = EpisodeId(999),
            34 => active(&mut w).incoming.as_mut().unwrap().sources.swap(0, 1),
            35 => active(&mut w).incoming.as_mut().unwrap().id = EpisodeId(999),
            36 => active(&mut w).incoming.as_mut().unwrap().turned = false,
            37 => active(&mut w).incoming.as_mut().unwrap().gap_upper = 0.1,
            38 => active(&mut w).incoming.as_mut().unwrap().settled_work = 1.0,
            39 => active(&mut w).incoming = None,
            40 => active(&mut w).plates[0].endpoint.velocity[0] = 0.001,
            41 => {
                let a = active(&mut w);
                let incoming = a.incoming.as_mut().unwrap();
                incoming.id = a.support.id;
                let pair = incoming.sources;
                let id = incoming.id;
                w.loading
                    .as_mut()
                    .unwrap()
                    .episodes
                    .get_mut(&pair)
                    .unwrap()
                    .id = id;
            }
            42 => {
                let pair = active(&mut w).support.sources;
                w.loading.as_mut().unwrap().episodes.remove(&pair);
            }
            43 => {
                let pair = active(&mut w).support.sources;
                w.loading
                    .as_mut()
                    .unwrap()
                    .episodes
                    .get_mut(&pair)
                    .unwrap()
                    .absent_seconds = 0.01;
            }
            44 => {
                let pair = active(&mut w).incoming.as_ref().unwrap().sources;
                w.loading
                    .as_mut()
                    .unwrap()
                    .episodes
                    .get_mut(&pair)
                    .unwrap()
                    .absent_seconds = 0.01;
            }
            45 => {
                let a = active(&mut w);
                let mut pair = [a.plates[0].source, a.plates[2].source];
                pair.sort();
                let l = w.loading.as_mut().unwrap();
                let id = EpisodeId(l.next_episode);
                l.next_episode += 1;
                l.episodes.insert(
                    pair,
                    Episode {
                        id,
                        absent_seconds: 0.0,
                    },
                );
            }
            46 => active(&mut w).incoming.as_mut().unwrap().energy = f64::MAX,
            _ => active(&mut w).incoming.as_mut().unwrap().held = false,
        }
        assert!(
            PhysicsWorld::from_snapshot_bytes(&w.snapshot_bytes().unwrap()).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn gravity_stack_state_correction_bounds_remain_fixed_after_invalidation() {
    for invalidated in [false, true] {
        let mut w = enrolled([11, 2, 7], GravitySettling::Refuse, 0);
        let h = history(&w).clone();
        let bounds = [1.0e-5 * h.scales[1], 1.0e-5 * h.scales[0]];
        if invalidated {
            for id in [11, 2, 7] {
                w.remove_entity(body(id).entity);
            }
        }
        for sign in [-1.0, 1.0] {
            state_mut(&mut w).corrections = Corrections {
                impulse: sign * bounds[0],
                work: bounds[1],
            };
            roundtrip(&w);
            for axis in 0..2 {
                let mut exceeded = w.clone();
                let c = &mut state_mut(&mut exceeded).corrections;
                if axis == 0 {
                    c.impulse = sign * f64::from_bits(bounds[0].to_bits() + 1);
                } else {
                    c.work = f64::from_bits(bounds[1].to_bits() + 1);
                }
                corrupt(&exceeded);
            }
        }
        // An initially stationary reference grants no impulse/energy allowance.
        if !invalidated {
            add_incoming(&mut w, 2);
        }
        let saved = w.loading.as_mut().unwrap().gravity.as_mut().unwrap();
        saved.history.reference = [0.0, 0.0, 20.0, 10.0];
        saved.history.scales = [0.0, 0.0, 1.0];
        saved.history.errors = [0.0; 3];
        state_mut(&mut w).corrections = Corrections {
            impulse: 0.0,
            work: 0.0,
        };
        roundtrip(&w);
        state_mut(&mut w).corrections.work = f64::from_bits(1);
        corrupt(&w);
    }
}

#[test]
fn gravity_stack_state_rejects_wrong_versions_tags_truncation_and_trailing_data() {
    for invalidated in [false, true] {
        for ball in [false, true] {
            for flat in [false, true] {
                let mut w = enrolled([11, 2, 7], GravitySettling::BoundedNumerical, 3);
                if invalidated {
                    for id in [11, 2, 7] {
                        w.remove_entity(body(id).entity);
                    }
                }
                let l = w.loading.as_mut().unwrap();
                l.arrival_departures = ball.then(Default::default);
                l.flat_departures = flat.then(Default::default);
                roundtrip(&w);
                let before = w.snapshot_bytes().unwrap();
                let mut reader = std::io::Cursor::new(before.as_slice());
                let _: crate::world::PhysicsSnapshot =
                    bincode::deserialize_from(&mut reader).unwrap();
                let mut l: LoadingState = bincode::deserialize_from(&mut reader).unwrap();
                l.deserialize_response(&mut reader).unwrap();
                let _: Option<super::super::super::super::ball_arrival::DepartureTracker> =
                    bincode::deserialize_from(&mut reader).unwrap();
                let _: Option<super::super::super::super::flat_arrival::DepartureTracker> =
                    bincode::deserialize_from(&mut reader).unwrap();
                let offset = reader.position() as usize;
                for version in [6_u32, 8] {
                    let mut wrong = before.clone();
                    wrong[..4].copy_from_slice(&version.to_le_bytes());
                    assert!(PhysicsWorld::from_snapshot_bytes(&wrong).is_err());
                }
                // History policy, outer Stack tag, inner lifecycle, and cause.
                for tag in [offset + 4, offset + 88, offset + 108]
                    .into_iter()
                    .chain(invalidated.then_some(offset + 112))
                {
                    let mut wrong = before.clone();
                    wrong[tag..tag + 4].copy_from_slice(&u32::MAX.to_le_bytes());
                    assert!(PhysicsWorld::from_snapshot_bytes(&wrong).is_err());
                }
                for length in [1, offset, offset + 88, before.len() - 1, before.len() - 8] {
                    assert!(PhysicsWorld::from_snapshot_bytes(&before[..length]).is_err());
                }
                let mut trailing = before;
                trailing.push(0);
                assert!(PhysicsWorld::from_snapshot_bytes(&trailing).is_err());
            }
        }
    }
    let mut legacy = stack_world([11, 2, 7], false, [1, 1, 1], 1)
        .snapshot_bytes()
        .unwrap();
    legacy[..4].copy_from_slice(&7_u32.to_le_bytes());
    assert!(PhysicsWorld::from_snapshot_bytes(&legacy).is_err());
}

#[test]
fn gravity_stack_state_cannot_share_active_claims_with_other_departure_modes() {
    for ball in [false, true] {
        let mut w = enrolled([11, 2, 7], GravitySettling::Refuse, 0);
        let owner = active(&mut w).support.clone();
        let l = w.loading.as_mut().unwrap();
        if ball {
            let pending = BTreeMap::from([(owner.sources, (owner.id, [Vec2::X, Vec2::X * 2.0]))]);
            l.arrival_departures =
                Some(bincode::deserialize(&bincode::serialize(&pending).unwrap()).unwrap());
        } else {
            let pending = BTreeMap::from([(
                owner.sources,
                (owner.id, [Vec2::X, Vec2::X * 2.0], [1.0_f64, 0.0]),
            )]);
            l.flat_departures =
                Some(bincode::deserialize(&bincode::serialize(&pending).unwrap()).unwrap());
        }
        corrupt(&w);
        w.loading
            .as_mut()
            .unwrap()
            .invalidate_gravity(Invalidation::OtherStep);
        roundtrip(&w);
        state_mut(&mut w).corrections.work = f64::NAN;
        corrupt(&w);
    }
}
