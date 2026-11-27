use super::*;

#[test]
fn paired_collapse_pauses_then_coasts_with_surviving_capacity_and_one_energy_ledger() {
    let mut s = MaterialLabState::new(Case::PairFailure);
    s.paused = false;
    let start = s.pair().unwrap();
    assert_eq!(start.bodies.each_ref().map(|b| b.capacity), [30.0, 1200.0]);
    assert!(
        start
            .bodies
            .iter()
            .all(|b| b.remaining == b.capacity && !b.failed)
    );
    let mut boundary = None;
    while s.stopped.is_none() {
        s.advance();
        // Drawing every state runs the collapse view without a display.
        render::frame(&s);
        let pair = s.pair().unwrap();
        assert!((pair.accounted_energy() - pair.initial_energy).abs() < 1e-8);
        assert!((pair.total(|b| b.momentum) - start.total(|b| b.momentum)).abs() < 1e-3);
        if let Some(failure) = s.paired_failure() {
            if boundary.is_none() {
                assert!(s.paused && s.stopped.is_none());
                assert_eq!(s.phase(), Phase::Collapsed);
                assert_eq!(pair.bodies.each_ref().map(|b| b.failed), [true, false]);
                assert_eq!(pair.bodies[0].remaining, 0.0);
                assert_eq!(pair.bodies[0].damage_work, 30.0);
                assert!(pair.bodies[1].remaining > 1100.0 && pair.bodies[1].damage_work > 0.0);
                assert!(failure.layers.iter().all(|l| l.released_elastic > 0.0));
                assert_eq!(pair.collapse_loss, failure.dissipated());
                let bytes = s.snapshot_bytes().unwrap();
                assert_eq!(&bytes[..8], PAIR_FAILURE_SAVE_MAGIC);
                for load in [Load::Released, Load::Light, Load::Heavy] {
                    assert!(s.set_load(load).is_err());
                    assert_eq!(s.snapshot_bytes().unwrap(), bytes);
                }
                boundary = Some((s.steps, failure.clone(), pair.clone()));
                s = restored(&s);
                assert_eq!(s.paired_failure(), Some(failure.clone()));
                s.paused = false;
            }
            let (_, first, bodies) = boundary.as_ref().unwrap();
            assert_eq!(&failure, first);
            assert_eq!(s.paused, s.stopped.is_some());
            for i in 0..2 {
                let (now, then) = (&pair.bodies[i], &bodies.bodies[i]);
                assert_eq!(
                    (
                        now.velocity,
                        now.permanent_set,
                        now.damage_work,
                        now.remaining,
                        now.viscous_loss,
                        now.deformation
                    ),
                    (
                        then.velocity,
                        then.permanent_set,
                        then.damage_work,
                        then.remaining,
                        then.viscous_loss,
                        then.deformation
                    )
                );
                assert_eq!(now.elastic_energy, 0.0);
            }
        }
    }
    assert!(
        s.stopped.as_ref().unwrap().starts_with("Run complete"),
        "{}",
        s.diagnostics()
    );
    let (step, failure, at_failure) = boundary.unwrap();
    assert!(s.steps > step + 32);
    assert_eq!((s.impacts, s.separations), (1, 0));
    assert_eq!(
        s.events
            .iter()
            .map(|e| e.label.as_str())
            .collect::<Vec<_>>(),
        ["Impact", "Support", "Contact ended", "Collapse"]
    );
    for i in 0..2 {
        assert!(s.pair().unwrap().bodies[i].position > at_failure.bodies[i].position + 0.01);
    }
    assert_eq!(restored(&s).paired_failure(), Some(failure));
}

#[test]
fn paired_collapse_saves_require_their_own_case_envelope_and_complete_history() {
    let mut s = MaterialLabState::new(Case::PairFailure);
    for steps in [0, 1, 75, 76, 192] {
        for _ in s.steps..steps {
            s.advance();
        }
        assert_eq!(s.steps, steps);
        let bytes = s.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..8], PAIR_FAILURE_SAVE_MAGIC);
        assert_eq!(restored(&s).pair(), s.pair());
        for magic in [
            SAVE_MAGIC,
            LOAD_SAVE_MAGIC,
            PAIR_SAVE_MAGIC,
            FAILURE_SAVE_MAGIC,
        ] {
            let mut bad = bytes.clone();
            bad[..8].copy_from_slice(magic);
            assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
        }
        let changes: &[fn(&mut Save)] = &[
            |s| s.case = Case::WoodWood,
            |s| s.steps += 1,
            |s| s.impacts += 1,
            |s| {
                s.events.push(Event {
                    time: 0.0,
                    label: "Collapse".into(),
                    episode: 1,
                })
            },
            |s| s.stopped = Some("Run complete".into()),
        ];
        for change in changes {
            let mut save: Save = codec().deserialize(&bytes[8..]).unwrap();
            change(&mut save);
            let mut bad = PAIR_FAILURE_SAVE_MAGIC.to_vec();
            bad.extend(codec().serialize(&save).unwrap());
            assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
        }
    }
    for case in [Case::WoodWood, Case::SupportFailure] {
        let mut old = MaterialLabState::new(case);
        old.advance();
        let mut bytes = old.snapshot_bytes().unwrap();
        bytes[..8].copy_from_slice(PAIR_FAILURE_SAVE_MAGIC);
        assert!(MaterialLabState::from_snapshot_bytes(&bytes).is_err());
    }
    assert_eq!(
        codec().serialize(&Case::SupportFailure).unwrap(),
        7_u32.to_le_bytes()
    );
    assert_eq!(
        codec().serialize(&Case::PairFailure).unwrap(),
        8_u32.to_le_bytes()
    );
}
#[test]
fn collapsed_support_keeps_its_failure_and_damage_while_the_platen_falls() {
    let mut s = MaterialLabState::new(Case::SupportFailure);
    s.paused = false;
    assert_eq!(s.capacity(), 120.0);
    assert_eq!(s.remaining(), s.capacity());
    let mut first = None;
    while s.stopped.is_none() {
        s.advance();
        if let Some(failure) = s.failure() {
            if first.is_none() {
                assert!(s.paused);
                first = Some((failure.clone(), s.summary()));
                let before = s.snapshot_bytes().unwrap();
                assert!(s.set_load(Load::Light).is_err());
                assert_eq!(s.snapshot_bytes().unwrap(), before);
                assert_eq!(&before[..8], FAILURE_SAVE_MAGIC);
                s = restored(&s);
            }
            assert_eq!(s.failure(), first.as_ref().map(|(e, _)| e.clone()));
            assert_eq!(s.remaining(), 0.0);
            assert_eq!(s.summary().damage_work, 120.0);
            assert_eq!(s.summary().elastic_energy, 0.0);
            assert_eq!(s.phase(), Phase::Collapsed);
            let a = s.summary();
            assert!(
                (a.gravity_work
                    - a.kinetic_energy
                    - a.damage_work
                    - a.charge_error
                    - a.viscous_loss
                    - failure.dissipated()
                    - a.errors[0])
                    .abs()
                    < 1e-8
            );
        }
    }
    assert!(
        s.stopped.as_ref().unwrap().starts_with("Run complete"),
        "{}",
        s.diagnostics()
    );
    let (event, before) = first.unwrap();
    assert!(s.summary().velocity > before.velocity + 1.0);
    assert!(s.summary().indentation > before.indentation + 0.01);
    assert_eq!(s.events.iter().filter(|e| e.label == "Collapse").count(), 1);
    assert_eq!(s.separations, 0);
    assert_eq!(restored(&s).failure(), Some(event));
    let bytes = s.snapshot_bytes().unwrap();
    for magic in [SAVE_MAGIC, LOAD_SAVE_MAGIC, PAIR_SAVE_MAGIC] {
        let mut bad = bytes.clone();
        bad[..8].copy_from_slice(magic);
        assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
    }
}
#[test]
fn qualified_cases_finish_with_expected_material_behavior() {
    for case in Case::ALL.into_iter().filter(|c| {
        ![
            Case::LoadRelease,
            Case::RegionSplit,
            Case::BranchFailure,
            Case::ShellThroughMoss,
            Case::BranchBends,
        ]
        .contains(c)
            && c.free_pair().is_none()
    }) {
        let mut s = MaterialLabState::new(case);
        for _ in 0..case.parameters().2 {
            s.advance();
        }
        assert_eq!(
            s.steps,
            case.parameters().2,
            "{case:?}: {:?} {:?}",
            s.stopped,
            s.summary()
        );
        assert!(s.stopped.as_ref().unwrap().starts_with("Run complete"));
        assert!(s.impacts >= 1);
        match case {
            Case::Elastic => {
                assert_eq!(s.summary().damage_work, 0.0);
                assert!(s.separations >= 1);
            }
            Case::PlasticReturn => {
                assert!(s.summary().damage_work > 0.0);
                assert!(s.impacts >= 2);
            }
            _ => {
                assert!(s.summary().damage_work > 0.0);
                assert_eq!(s.separations, 0);
            }
        }
        println!(
            "{case:?}: steps={} impacts={} damage={}",
            s.steps,
            s.impacts,
            s.summary().damage_work
        );
    }
}
#[test]
fn pause_save_load_and_replay_preserve_physics_and_events() {
    for case in Case::ALL.into_iter().filter(|c| *c != Case::LoadRelease) {
        let mut s = MaterialLabState::new(case);
        for t in 0..case.parameters().2 {
            if t % 97 == 0 {
                let before = s.snapshot_bytes().unwrap();
                MaterialLabScenario::step(&mut s, &[], Duration::from_secs_f32(1.0 / 60.0));
                assert_eq!(s.snapshot_bytes().unwrap(), before);
                let mut loaded = MaterialLabState::from_snapshot_bytes(&before).unwrap();
                assert!(loaded.paused);
                s.advance();
                loaded.advance();
                assert_eq!(
                    s.snapshot_bytes().unwrap(),
                    loaded.snapshot_bytes().unwrap()
                );
            } else {
                s.advance();
            }
        }
        let bytes = s.snapshot_bytes().unwrap();
        assert_eq!(
            MaterialLabState::from_snapshot_bytes(&bytes)
                .unwrap()
                .snapshot_bytes()
                .unwrap(),
            bytes
        );
    }
}

fn restored(s: &MaterialLabState) -> MaterialLabState {
    let bytes = s.snapshot_bytes().unwrap();
    let r = MaterialLabState::from_snapshot_bytes(&bytes).unwrap();
    assert!(r.paused);
    assert_eq!(r.snapshot_bytes().unwrap(), bytes);
    r
}

#[test]
fn guided_load_release_and_return_keeps_damage_and_replays_commands() {
    let mut s = MaterialLabState::new(Case::LoadRelease);
    let initial = s.snapshot_bytes().unwrap();
    assert!(s.set_load(Load::Released).is_err());
    assert_eq!(s.snapshot_bytes().unwrap(), initial);
    s.paused = false;
    for _ in 0..YIELD_CHECKPOINT_STEP {
        MaterialLabScenario::step(&mut s, &[], Duration::from_secs_f32(1.0 / 60.0));
    }
    assert!(s.paused);
    assert_eq!(s.steps, YIELD_CHECKPOINT_STEP);
    assert!(s.summary().damage_work > 0.0);
    assert_eq!(s.impacts, 1);
    let yielded = s.summary();
    s = restored(&s);
    s.set_load(Load::Released).unwrap();
    assert_eq!(s.summary(), yielded);
    assert_eq!(s.weight(), 0.0);
    s = restored(&s);
    s.paused = false;
    for _ in 0..768 {
        MaterialLabScenario::step(&mut s, &[], Duration::from_secs_f32(1.0 / 60.0));
        if s.paused {
            break;
        }
    }
    assert_eq!(s.stopped, None, "{:?}", s.summary());
    assert_eq!(s.separations, 1);
    assert!(s.paused);
    let separated = s.summary();
    assert_eq!(separated.elastic_energy, 0.0);
    assert!(separated.damage_work >= yielded.damage_work);
    assert!(separated.plastic_indentation >= yielded.plastic_indentation);
    let work = separated.gravity_work;
    s = restored(&s);
    s.set_load(Load::Light).unwrap();
    assert_eq!(s.summary(), separated);
    let mut r = restored(&s);
    for _ in 0..768 {
        s.advance();
        r.advance();
        assert_eq!(s.snapshot_bytes().unwrap(), r.snapshot_bytes().unwrap());
        if s.stopped.is_some() {
            break;
        }
    }
    assert!(
        s.stopped.as_ref().unwrap().starts_with("Run complete"),
        "{:?}",
        s.diagnostics()
    );
    assert_eq!(s.impacts, 2);
    assert_eq!(s.summary().damage_work, separated.damage_work);
    assert_eq!(
        s.summary().plastic_indentation,
        separated.plastic_indentation
    );
    assert_ne!(s.summary().gravity_work, work);
    assert_eq!(s.load_changes.len(), 2);
    let r = restored(&s);
    assert!(r.paused);
    let terminal = s.snapshot_bytes().unwrap();
    assert!(s.set_load(Load::Heavy).is_err());
    assert_eq!(s.snapshot_bytes().unwrap(), terminal);
    let (save, mut commands): (Save, Vec<LoadChange>) =
        codec().deserialize(&terminal[8..]).unwrap();
    commands.push(LoadChange {
        step: s.steps,
        load: Load::Heavy,
    });
    let mut bad = LOAD_SAVE_MAGIC.to_vec();
    bad.extend(codec().serialize(&(save, commands)).unwrap());
    assert_eq!(
        MaterialLabState::from_snapshot_bytes(&bad).err().as_deref(),
        Some("Invalid run history")
    );
}

#[test]
fn load_changes_at_one_paused_tick_are_bounded_and_old_saves_keep_their_layout() {
    let mut s = MaterialLabState::new(Case::PlasticReturn);
    s.advance();
    // Spell the v1 field order independently, including the legacy case tag.
    let mut frozen = SAVE_MAGIC.to_vec();
    frozen.extend(
        codec()
            .serialize(&(
                1_u32,
                s.steps,
                s.impacts,
                s.separations,
                &s.events,
                &s.stopped,
                s.world.snapshot_bytes().unwrap(),
            ))
            .unwrap(),
    );
    assert_eq!(s.snapshot_bytes().unwrap(), frozen);
    s.set_load(Load::Light).unwrap();
    assert_eq!(s.snapshot_bytes().unwrap(), frozen);
    let summary = s.summary();
    for i in 0..MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES {
        s.set_load(if i % 2 == 0 {
            Load::Released
        } else {
            Load::Light
        })
        .unwrap();
        assert_eq!(s.summary(), summary);
    }
    s = restored(&s);
    let bytes = s.snapshot_bytes().unwrap();
    s.set_load(Load::Light).unwrap();
    assert_eq!(s.snapshot_bytes().unwrap(), bytes);
    assert!(s.set_load(Load::Heavy).unwrap_err().contains("limit"));
    assert_eq!(s.snapshot_bytes().unwrap(), bytes);
}

#[test]
fn changed_load_saves_refuse_forged_commands_and_wrong_versions() {
    let mut s = MaterialLabState::new(Case::PlasticReturn);
    s.advance();
    s.set_load(Load::Released).unwrap();
    s.advance();
    s.set_load(Load::Light).unwrap();
    let bytes = s.snapshot_bytes().unwrap();
    let changes: &[fn(&mut Save, &mut Vec<LoadChange>)] = &[
        |_, c| c.clear(),
        |_, c| c[0].step = 0,
        |_, c| c[0].step = 3,
        |_, c| c.reverse(),
        |_, c| c[0].load = Load::Heavy,
        |_, c| c.insert(1, c[0].clone()),
        |_, c| *c = vec![c[0].clone(); 33],
        |s, _| s.steps += 1,
        |s, _| {
            s.world = MaterialLabState::new(Case::PlasticReturn)
                .world
                .snapshot_bytes()
                .unwrap()
        },
    ];
    for change in changes {
        let (mut save, mut commands): (Save, Vec<LoadChange>) =
            codec().deserialize(&bytes[8..]).unwrap();
        change(&mut save, &mut commands);
        let mut bad = LOAD_SAVE_MAGIC.to_vec();
        bad.extend(codec().serialize(&(save, commands)).unwrap());
        assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
    }
    for cut in [0, 1, 8, bytes.len() - 1] {
        assert!(MaterialLabState::from_snapshot_bytes(&bytes[..cut]).is_err());
    }
    let mut bad = bytes.clone();
    bad[..8].copy_from_slice(SAVE_MAGIC);
    assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
    let mut bad = bytes.clone();
    bad.push(0);
    assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
}

#[test]
fn an_overloaded_modified_case_restores_its_last_valid_state_after_refusal() {
    let mut s = MaterialLabState::new(Case::Elastic);
    s.advance();
    s.set_load(Load::Heavy).unwrap();
    for _ in 0..s.case.parameters().2 {
        s.advance();
        if s.stopped.is_some() {
            break;
        }
    }
    assert!(s.stopped.as_ref().unwrap().starts_with("Run stopped"));
    assert!(s.steps < s.case.parameters().2);
    let mut r = restored(&s);
    let before = r.snapshot_bytes().unwrap();
    r.advance();
    assert_eq!(r.snapshot_bytes().unwrap(), before);
}
#[test]
fn malformed_and_mismatched_saves_refuse() {
    let mut s = MaterialLabState::new(Case::PlasticReturn);
    for _ in 0..200 {
        s.advance();
    }
    let bytes = s.snapshot_bytes().unwrap();
    for cut in [0, 1, 8, bytes.len() - 1] {
        assert!(MaterialLabState::from_snapshot_bytes(&bytes[..cut]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(MaterialLabState::from_snapshot_bytes(&trailing).is_err());
    let mut save: Save = codec().deserialize(&bytes[8..]).unwrap();
    save.steps += 1;
    let mut bad = SAVE_MAGIC.to_vec();
    bad.extend(codec().serialize(&save).unwrap());
    assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
}

#[test]
fn free_pairs_recoil_with_separate_damage_and_one_closed_energy_ledger() {
    for case in [Case::WoodWood, Case::WoodArmor] {
        let mut s = MaterialLabState::new(case);
        let start = s.pair().unwrap();
        for body in &start.bodies {
            assert_eq!((body.deformation, body.damage_work), (0.0, 0.0));
            assert_eq!(body.remaining, body.capacity);
        }
        assert_eq!(start.accounted_energy(), start.initial_energy);
        assert_eq!(
            start.bodies[1].position - start.bodies[0].position - 1.0,
            case.parameters().1
        );
        let momentum = start.total(|b| b.momentum);
        let mut both_stored_energy = false;
        while s.stopped.is_none() {
            s.advance();
            let p = s.pair().unwrap();
            // One ledger holds both bodies' motion and both materials' work.
            assert!(
                (p.accounted_energy() - p.initial_energy).abs() < 1e-9 * p.initial_energy,
                "{case:?} step {}: {p:?}",
                s.steps
            );
            assert!((p.total(|b| b.momentum) - momentum).abs() < 1e-3);
            both_stored_energy |= p.bodies.iter().all(|b| b.elastic_energy > 0.0);
            for body in &p.bodies {
                assert!(body.deformation >= body.permanent_set);
                assert!((body.capacity - body.damage_work - body.remaining).abs() < 1e-9);
            }
        }
        assert!(
            s.stopped.as_ref().unwrap().starts_with("Run complete"),
            "{case:?}: {:?}",
            s.diagnostics()
        );
        assert_eq!(s.steps, case.parameters().2);
        assert_eq!((s.impacts, s.separations), (1, 1));
        let labels: Vec<_> = s.events.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["Impact", "Support", "Separation"]);
        assert_eq!(s.phase(), Phase::Separated);
        assert!(both_stored_energy);
        let [a, b] = s.pair().unwrap().bodies;
        // The incoming body is thrown back and the other is driven away.
        assert!(a.velocity < 0.0 && b.velocity > start.bodies[1].velocity.max(0.0));
        for body in [&a, &b] {
            assert_eq!(body.elastic_energy, 0.0);
            assert_eq!(body.deformation, body.permanent_set);
        }
        assert!(a.damage_work > 0.0 && a.permanent_set > 0.0 && a.remaining < a.capacity);
        if case == Case::WoodWood {
            // Equal layers share the damage equally, whatever the two masses.
            assert!((a.damage_work - b.damage_work).abs() < 1e-9);
            assert!((a.permanent_set - b.permanent_set).abs() < 1e-12);
        } else {
            // The armor flexes and returns its energy; only the wood is spent.
            assert_eq!((b.damage_work, b.permanent_set), (0.0, 0.0));
            assert_eq!(b.remaining, b.capacity);
        }
        println!("{case:?}: A {a:?}\n  B {b:?}");
    }
}

#[test]
fn free_pair_saves_keep_their_own_layout_and_refuse_load_commands() {
    let mut s = MaterialLabState::new(Case::WoodArmor);
    let fresh = s.snapshot_bytes().unwrap();
    assert!(s.set_load(Load::Light).is_err());
    assert_eq!(s.snapshot_bytes().unwrap(), fresh);
    for _ in 0..100 {
        s.advance();
    }
    assert_eq!(s.phase(), Phase::Compression);
    let bytes = s.snapshot_bytes().unwrap();
    assert!(s.set_load(Load::Released).is_err());
    assert_eq!(s.snapshot_bytes().unwrap(), bytes);
    // Spell the field order independently, including the appended case tag.
    let mut frozen = PAIR_SAVE_MAGIC.to_vec();
    frozen.extend(
        codec()
            .serialize(&(
                6_u32,
                s.steps,
                s.impacts,
                s.separations,
                &s.events,
                &s.stopped,
                s.world.snapshot_bytes().unwrap(),
            ))
            .unwrap(),
    );
    assert_eq!(bytes, frozen);
    assert_eq!(restored(&s).pair(), s.pair());
    // A pair fits no other envelope, and no platen case fits this one.
    let retagged = |bytes: &[u8], magic: &[u8; 8]| {
        let mut bad = bytes.to_vec();
        bad[..8].copy_from_slice(magic);
        MaterialLabState::from_snapshot_bytes(&bad)
    };
    assert!(retagged(&bytes, SAVE_MAGIC).is_err());
    assert!(retagged(&bytes, LOAD_SAVE_MAGIC).is_err());
    let save: Save = codec().deserialize(&bytes[8..]).unwrap();
    let mut bad = LOAD_SAVE_MAGIC.to_vec();
    bad.extend(
        codec()
            .serialize(&(save, Vec::<LoadChange>::new()))
            .unwrap(),
    );
    assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
    let mut platen = MaterialLabState::new(Case::Elastic);
    platen.advance();
    assert!(retagged(&platen.snapshot_bytes().unwrap(), PAIR_SAVE_MAGIC).is_err());
    let mut save: Save = codec().deserialize(&bytes[8..]).unwrap();
    save.steps += 1;
    let mut bad = PAIR_SAVE_MAGIC.to_vec();
    bad.extend(codec().serialize(&save).unwrap());
    assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
    for cut in [8, bytes.len() - 1] {
        assert!(MaterialLabState::from_snapshot_bytes(&bytes[..cut]).is_err());
    }
}

#[test]
fn split_case_divides_the_damaged_wood_and_keeps_its_balance_through_the_rebuild() {
    let mut s = MaterialLabState::new(Case::RegionSplit);
    let start = s.split().unwrap();
    assert_eq!(start.wood.remaining, 1200.0);
    assert_eq!(start.wood.capacity, 1200.0);
    assert!(start.fragment.is_none());
    let mut pauses = Vec::new();
    let mut reference = None;
    s.paused = false;
    while s.stopped.is_none() {
        s.advance();
        // Drawing every state runs the split view without a display.
        render::frame(&s);
        let split = s.split().unwrap();
        if !s.paused || s.stopped.is_some() {
            continue;
        }
        pauses.push((s.steps, split.stage));
        match split.stage {
            Stage::Struck => {
                assert_eq!(s.phase(), Phase::Separated);
                assert_eq!(split.charges.len(), 1);
                assert_eq!(split.wood.remaining, 1200.0 - split.charges[0]);
                // About a fifth of the whole wood.
                assert!((240.0..260.0).contains(&split.charges[0]), "{split:?}");
            }
            Stage::Split => {
                let before = split.before_split.unwrap();
                let [kept, piece] = split.parts.unwrap();
                let fragment = split.fragment.as_ref().unwrap();
                assert_eq!((split.wood.volume, split.wood.capacity), (15.0, 900.0));
                assert_eq!((fragment.volume, fragment.capacity), (5.0, 300.0));
                assert_eq!((kept, piece), (split.wood.remaining, fragment.remaining));
                // The parts share the balance at its integrity and never hold more.
                assert!(kept + piece <= before && before - (kept + piece) < 1e-9);
                for integrity in [kept / 900.0, piece / 300.0] {
                    assert!((integrity - before / 1200.0).abs() < 1e-6);
                }
                assert_eq!((split.wood.left, split.wood.right), (0.0, 0.75));
                assert_eq!((fragment.left, fragment.right), (1.0, 1.25));
                // A copy that skips the rebuild is the second strike's reference.
                let mut world = s.world.clone();
                split::relaunch(&mut world);
                reference = Some((world, s.materials[1].unwrap()));
            }
            Stage::Rebuilt => {
                assert_eq!(split.wood.body, 4);
                assert_eq!(split.wood.remaining, split.parts.unwrap()[0]);
                assert_eq!(split.moved, Some(split.wood.remaining));
            }
            Stage::Strike | Stage::Restrike => panic!("unexpected pause: {split:?}"),
        }
        let bytes = s.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..8], SPLIT_SAVE_MAGIC);
        for load in [Load::Released, Load::Light, Load::Heavy] {
            assert!(s.set_load(load).is_err());
        }
        assert_eq!(s.snapshot_bytes().unwrap(), bytes);
        s = restored(&s);
        s.paused = false;
    }
    assert!(s.stopped.as_ref().unwrap().starts_with("Run complete"));
    assert_eq!(s.steps, Case::RegionSplit.parameters().2);
    assert_eq!(
        pauses,
        [
            (86, Stage::Struck),
            (87, Stage::Split),
            (88, Stage::Rebuilt)
        ]
    );
    assert_eq!((s.impacts, s.separations), (2, 2));
    assert_eq!(
        s.events
            .iter()
            .map(|e| e.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Impact",
            "Separation",
            "Split",
            "Rebuilt",
            "Impact",
            "Separation"
        ]
    );
    let split = s.split().unwrap();
    assert_eq!(split.stage, Stage::Restrike);
    assert_eq!(split.charges.len(), 2);
    assert!((split.wood.remaining - (split.moved.unwrap() - split.charges[1])).abs() < 1e-9);
    // The fragment was never struck.
    assert_eq!(
        split.fragment.as_ref().unwrap().remaining,
        split.parts.unwrap()[1]
    );
    // The same strike leaves the wood that skipped the rebuild exactly where it
    // leaves the rebuilt wood.
    let (mut world, wood) = reference.unwrap();
    for _ in 89..=s.steps {
        world.step(PHYSICS_DT);
    }
    assert_eq!(
        world.loading_material_remaining(wood),
        Some(split.wood.remaining)
    );
}

#[test]
fn split_case_saves_keep_their_own_layout_and_replay_every_edit() {
    let mut s = MaterialLabState::new(Case::RegionSplit);
    for steps in [0, 1, 17, 86, 87, 88, 89, 105, 174] {
        // A stopped run no longer steps, so an early stop fails here.
        while s.steps < steps && s.stopped.is_none() {
            s.advance();
        }
        assert_eq!(s.steps, steps, "{:?}", s.stopped);
        let bytes = s.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..8], SPLIT_SAVE_MAGIC);
        // The native snapshot keeps the response-enabled v3 layout through every edit.
        let world = s.world.snapshot_bytes().unwrap();
        assert_eq!(u32::from_le_bytes(world[..4].try_into().unwrap()), 3);
        let mut loaded = restored(&s);
        assert_eq!(loaded.split(), s.split());
        assert_eq!(loaded.events, s.events);
        for magic in [
            SAVE_MAGIC,
            LOAD_SAVE_MAGIC,
            PAIR_SAVE_MAGIC,
            FAILURE_SAVE_MAGIC,
            PAIR_FAILURE_SAVE_MAGIC,
        ] {
            let mut bad = bytes.clone();
            bad[..8].copy_from_slice(magic);
            assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
        }
        let changes: &[fn(&mut Save)] = &[
            |s| s.case = Case::WoodWood,
            |s| s.steps += 1,
            |s| s.impacts += 1,
            |s| {
                s.events.push(Event {
                    time: 0.0,
                    label: "Split".into(),
                    episode: 1,
                })
            },
            |s| s.stopped = Some("Run complete".into()),
        ];
        for change in changes {
            let mut save: Save = codec().deserialize(&bytes[8..]).unwrap();
            change(&mut save);
            let mut bad = SPLIT_SAVE_MAGIC.to_vec();
            bad.extend(codec().serialize(&save).unwrap());
            assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
        }
        // A restored run continues exactly, through the next edit.
        loaded.advance();
        s.advance();
        assert_eq!(
            loaded.snapshot_bytes().unwrap(),
            s.snapshot_bytes().unwrap()
        );
    }
    for case in [Case::WoodWood, Case::PairFailure] {
        let mut old = MaterialLabState::new(case);
        old.advance();
        let mut bytes = old.snapshot_bytes().unwrap();
        bytes[..8].copy_from_slice(SPLIT_SAVE_MAGIC);
        assert!(MaterialLabState::from_snapshot_bytes(&bytes).is_err());
    }
    assert_eq!(
        codec().serialize(&Case::RegionSplit).unwrap(),
        9_u32.to_le_bytes()
    );
}

#[test]
fn split_case_stops_at_a_refused_response_and_keeps_its_last_valid_world() {
    let mut s = MaterialLabState::new(Case::RegionSplit);
    // A slower striker meets a speculative contact before it touches.
    s.world
        .set_velocity(scene::body(0), engine_core::Vec2::X * 5.0, 0.0, true);
    let mut last = s.world.snapshot_bytes().unwrap();
    while s.stopped.is_none() {
        last = s.world.snapshot_bytes().unwrap();
        s.advance();
    }
    assert!(
        s.stopped.as_ref().unwrap().contains("SpeculativeArrival"),
        "{:?}",
        s.stopped
    );
    assert!(s.paused);
    assert_eq!(s.world.snapshot_bytes().unwrap(), last);
    assert_eq!(s.split().unwrap().wood.remaining, 1200.0);
    assert_eq!(s.impacts, 0);
}

/// The law's charge for one lab step of `moment` on the weld's beam.
fn weld_charge_per_step(moment: f32) -> f32 {
    let probe = engine_material::Connection::new(
        engine_material::Material {
            toughness: 1.0e15,
            ..engine_material::WOOD
        },
        branch::beam(),
    )
    .unwrap();
    engine_material::connection(
        &probe,
        engine_material::ConnectionLoad {
            moment,
            axial: 0.0,
            duration: PHYSICS_DT,
            measured_plastic_rotation: None,
            measured_plastic_extension: None,
        },
    )
    .unwrap()
    .consumed[0]
}

/// The root moment of the branch's own weight at its center and of the
/// weight at its reach.
fn root_moment(load: Load) -> f32 {
    branch::BRANCH_MASS * branch::GRAVITY * branch::BRANCH_HALF.x
        + load.weight() * branch::WEIGHT_REACH
}

/// Run until the lab pauses, drawing every state.
fn run_until_paused(s: &mut MaterialLabState) {
    s.paused = false;
    while !s.paused {
        s.advance();
        render::frame(s);
    }
}

#[test]
fn branch_case_holds_its_light_load_for_nothing_then_severs_on_the_laws_schedule() {
    let mut s = MaterialLabState::new(Case::BranchFailure);
    let capacity = f64::from(branch::weld().region().capacity());
    let start = s.branch().unwrap();
    assert_eq!((start.remaining, start.spent), (capacity, 0.0));
    assert_eq!((start.safe_moment, start.tip_load), (32000.0, 1000.0));
    assert_eq!(start.safe_moment, 2.0 * root_moment(Load::Released));
    assert_eq!(s.phase(), Phase::Holding);
    assert!(
        s.set_load(Load::Heavy)
            .unwrap_err()
            .starts_with("Start the run")
    );
    // The light load holds to the checkpoint and never charges the weld. The
    // root carries the moment of both weights, and their sum as shear.
    s.paused = false;
    let mut errors = [0.0_f32; 2];
    while !s.paused {
        s.advance();
        render::frame(&s);
        let b = s.branch().unwrap();
        assert_eq!((b.load.charged, b.remaining), (0.0, capacity));
        assert_eq!(b.time_to_failure, None);
        if s.steps >= 8 {
            let weights = branch::BRANCH_MASS * branch::GRAVITY + 1000.0;
            let moment = (b.load.moment + root_moment(Load::Light)) / root_moment(Load::Light);
            errors[0] = errors[0].max(moment.abs());
            errors[1] = errors[1].max(((b.load.shear + weights) / weights).abs());
        }
    }
    assert!(errors[0] < 1e-3 && errors[1] < 3e-3, "{errors:?}");
    assert_eq!(s.steps, BRANCH_CHECKPOINT_STEP);
    assert_eq!(s.phase(), Phase::Holding);
    assert_eq!(
        s.guidance(),
        Some("Apply heavy load, then Run until the weld gives way.")
    );
    let held = s.branch().unwrap();
    assert!((held.load.utilization - root_moment(Load::Light) / 32000.0).abs() < 1e-3);
    // The heavy load drains the weld at the law's rate until it severs, and
    // the lab pauses with the branch still in place.
    s.set_load(Load::Heavy).unwrap();
    assert_eq!(s.weight(), 8000.0);
    let per_step = weld_charge_per_step(root_moment(Load::Heavy));
    let schedule = capacity / f64::from(per_step);
    run_until_paused(&mut s);
    let overload = s.steps - BRANCH_CHECKPOINT_STEP;
    assert_eq!(s.severed_at, Some(s.steps));
    assert!(
        f64::from(overload) > schedule && f64::from(overload) < schedule + 3.0,
        "{overload} steps against {schedule}"
    );
    let severed = s.branch().unwrap();
    assert_eq!((severed.remaining, severed.spent), (0.0, capacity));
    assert!(severed.excess.unwrap() >= 0.0 && severed.load.charged > 0.0);
    assert!((severed.load.utilization - 1.375).abs() < 1e-3);
    // The weld held the branch in place, to its own slight give.
    assert!(severed.drop.abs() < 1e-4, "{severed:?}");
    assert_eq!((severed.time_to_failure, s.phase()), (None, Phase::Falling));
    assert!(s.collapsed());
    assert_eq!(
        s.events,
        [Event {
            time: s.elapsed(),
            label: "Severed".into(),
            episode: 0,
        }]
    );
    assert_eq!(
        s.guidance(),
        Some("The weld has severed. Run to watch the branch fall.")
    );
    // Rerun the overload to check each step's charge and prediction.
    let mut r = MaterialLabState::new(Case::BranchFailure);
    while r.steps < BRANCH_CHECKPOINT_STEP {
        r.advance();
    }
    r.set_load(Load::Heavy).unwrap();
    let mut predictions = Vec::new();
    while r.severed_at.is_none() {
        r.advance();
        let b = r.branch().unwrap();
        if r.steps > BRANCH_CHECKPOINT_STEP + 8 && r.severed_at.is_none() {
            // The weight settles on the branch within a few steps of the jump,
            // then rocks slightly on its two contacts.
            assert!((b.load.charged - per_step).abs() < 5e-3 * per_step, "{b:?}");
            assert_eq!(r.phase(), Phase::Overloaded);
            predictions.push((r.steps, b.time_to_failure.unwrap()));
        }
    }
    // Each prediction names the severing step to within one step.
    for (step, prediction) in predictions {
        let actual = f64::from(r.steps - step) * f64::from(PHYSICS_DT);
        assert!(
            (prediction - actual).abs() <= f64::from(PHYSICS_DT),
            "{step}: {prediction} against {actual}"
        );
    }
    assert_eq!(r.snapshot_bytes().unwrap(), s.snapshot_bytes().unwrap());
    // A severed weld takes no more load, and the branch falls with the weight.
    let bytes = s.snapshot_bytes().unwrap();
    for load in [Load::Released, Load::Light, Load::Heavy] {
        assert!(
            s.set_load(load)
                .unwrap_err()
                .starts_with("The weld has severed")
        );
        assert_eq!(s.snapshot_bytes().unwrap(), bytes);
    }
    let at = severed.weight.y - severed.branch.y;
    run_until_paused(&mut s);
    assert!(
        s.stopped.as_ref().unwrap().starts_with("Run complete"),
        "{:?}",
        s.stopped
    );
    assert_eq!(s.steps, s.severed_at.unwrap() + branch::FALL_STEPS);
    let end = s.branch().unwrap();
    assert_eq!(
        (end.load, end.excess, end.remaining),
        (severed.load, severed.excess, 0.0)
    );
    // Free fall over the fall's steps, as the integrator takes it.
    let n = f64::from(branch::FALL_STEPS);
    let free = f64::from(branch::GRAVITY) * f64::from(PHYSICS_DT).powi(2) * n * (n + 1.0) / 2.0;
    assert!((f64::from(end.drop) - free).abs() < 2e-3 * free, "{end:?}");
    assert!((end.weight.y - end.branch.y - at).abs() < 0.01, "{end:?}");
    assert!(end.branch.angle.abs() < 0.01 && end.weight.angle.abs() < 0.01);
    assert_eq!(s.events.len(), 1);
}

#[test]
fn branch_case_keeps_the_capacity_an_overload_spent_after_the_load_is_lifted() {
    let mut s = MaterialLabState::new(Case::BranchFailure);
    while s.steps < BRANCH_CHECKPOINT_STEP {
        s.advance();
    }
    s.set_load(Load::Heavy).unwrap();
    for _ in 0..60 {
        s.advance();
    }
    let spent = s.branch().unwrap().spent;
    assert!(spent > 0.0);
    for load in [Load::Light, Load::Released] {
        s.set_load(load).unwrap();
        for _ in 0..64 {
            s.advance();
            let b = s.branch().unwrap();
            assert_eq!((b.spent, b.load.charged), (spent, 0.0));
        }
        let b = s.branch().unwrap();
        assert!((b.load.utilization - root_moment(load) / 32000.0).abs() < 1e-3);
        assert_eq!(s.phase(), Phase::Holding);
    }
    assert_eq!(
        s.guidance(),
        Some("Apply heavy load, then Run until the weld gives way.")
    );
    // The overload resumes where it stopped, so the weld severs after the same
    // overloaded steps in all.
    s.set_load(Load::Heavy).unwrap();
    let resumed = s.steps;
    while s.severed_at.is_none() {
        s.advance();
    }
    let overload = 60 + s.steps - resumed;
    let schedule = f64::from(branch::weld().region().capacity())
        / f64::from(weld_charge_per_step(root_moment(Load::Heavy)));
    assert!(
        f64::from(overload) > schedule && f64::from(overload) < schedule + 6.0,
        "{overload} steps against {schedule}"
    );
    assert_eq!(s.load_changes.len(), 4);
}

#[test]
fn branch_case_saves_keep_their_own_layout_and_replay_its_load_changes() {
    let mut s = MaterialLabState::new(Case::BranchFailure);
    let mut checkpoints = vec![0, 1, BRANCH_CHECKPOINT_STEP, 200, 250];
    while s.stopped.is_none() {
        if s.steps == BRANCH_CHECKPOINT_STEP && s.load_changes.is_empty() {
            // Two changes at one paused step replay in order.
            s.set_load(Load::Released).unwrap();
            s.set_load(Load::Heavy).unwrap();
        }
        if checkpoints.first() == Some(&s.steps) || s.severed_at.is_some_and(|at| s.steps <= at + 1)
        {
            checkpoints.retain(|c| *c != s.steps);
            let bytes = s.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..8], BRANCH_SAVE_MAGIC);
            // With no loading capture the native snapshot keeps the v1 layout,
            // with the weld in its connection section.
            let world = s.world.snapshot_bytes().unwrap();
            assert_eq!(u32::from_le_bytes(world[..4].try_into().unwrap()), 1);
            let mut loaded = restored(&s);
            assert_eq!(loaded.branch(), s.branch());
            assert_eq!(
                (&loaded.events, loaded.severed_at),
                (&s.events, s.severed_at)
            );
            for magic in [
                SAVE_MAGIC,
                LOAD_SAVE_MAGIC,
                PAIR_SAVE_MAGIC,
                FAILURE_SAVE_MAGIC,
                PAIR_FAILURE_SAVE_MAGIC,
                SPLIT_SAVE_MAGIC,
            ] {
                let mut bad = bytes.clone();
                bad[..8].copy_from_slice(magic);
                assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
            }
            let changes: &[fn(&mut Save, &mut Vec<LoadChange>)] = &[
                |s, _| s.case = Case::RegionSplit,
                |s, _| s.steps += 1,
                |s, _| s.impacts += 1,
                |s, _| {
                    s.events.push(Event {
                        time: 0.0,
                        label: "Severed".into(),
                        episode: 0,
                    })
                },
                |s, _| s.stopped = Some("Run complete".into()),
                |s, c| {
                    c.push(LoadChange {
                        step: s.steps,
                        load: Load::Released,
                    })
                },
            ];
            for change in changes {
                let (mut save, mut commands): (Save, Vec<LoadChange>) =
                    codec().deserialize(&bytes[8..]).unwrap();
                change(&mut save, &mut commands);
                let mut bad = BRANCH_SAVE_MAGIC.to_vec();
                bad.extend(codec().serialize(&(save, commands)).unwrap());
                assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
            }
            loaded.advance();
            s.advance();
            assert_eq!(
                loaded.snapshot_bytes().unwrap(),
                s.snapshot_bytes().unwrap()
            );
            continue;
        }
        s.advance();
    }
    assert!(s.stopped.as_ref().unwrap().starts_with("Run complete"));
    assert_eq!(s.load_changes.len(), 2);
    let terminal = restored(&s);
    assert_eq!(terminal.branch(), s.branch());
    // Commands the run refuses cannot come back from a save.
    let bytes = s.snapshot_bytes().unwrap();
    let forged: &[fn(&mut Vec<LoadChange>)] = &[
        |c| c.clear(),
        |c| c[0].step = 0,
        |c| c.reverse(),
        |c| c[1].load = Load::Light,
        |c| c[1].step = 400,
        |c| *c = vec![c[0].clone(); 33],
    ];
    for forge in forged {
        let (save, mut commands): (Save, Vec<LoadChange>) =
            codec().deserialize(&bytes[8..]).unwrap();
        forge(&mut commands);
        let mut bad = BRANCH_SAVE_MAGIC.to_vec();
        bad.extend(codec().serialize(&(save, commands)).unwrap());
        assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
    }
    // Other cases' saves never pass as this layout, nor this case's as theirs.
    for case in [Case::PlasticReturn, Case::LoadRelease, Case::RegionSplit] {
        let mut old = MaterialLabState::new(case);
        old.advance();
        let mut bytes = old.snapshot_bytes().unwrap();
        bytes[..8].copy_from_slice(BRANCH_SAVE_MAGIC);
        assert!(MaterialLabState::from_snapshot_bytes(&bytes).is_err());
    }
    assert_eq!(
        codec().serialize(&Case::BranchFailure).unwrap(),
        10_u32.to_le_bytes()
    );
}

#[test]
fn branch_case_load_changes_are_bounded() {
    let mut s = MaterialLabState::new(Case::BranchFailure);
    s.advance();
    let bytes = s.snapshot_bytes().unwrap();
    s.set_load(Load::Light).unwrap();
    assert_eq!(s.snapshot_bytes().unwrap(), bytes);
    for i in 0..MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES {
        s.set_load(if i % 2 == 0 {
            Load::Released
        } else {
            Load::Light
        })
        .unwrap();
    }
    s = restored(&s);
    let bytes = s.snapshot_bytes().unwrap();
    s.set_load(Load::Light).unwrap();
    assert!(s.set_load(Load::Heavy).unwrap_err().contains("limit"));
    assert_eq!(s.snapshot_bytes().unwrap(), bytes);
}

/// The bend at which the bending branch rests under `load`: where its
/// hinge's spring holds the branch's weight at its center and the load at the
/// weight's center, each at its lever arm as the branch lies. The weight's
/// center is half a unit above the branch's top face, so it moves outward as
/// the branch turns down.
fn bending_balance(load: Load) -> f32 {
    let stiffness = engine_rapier::world::hinge_stiffness(&branch::hinge());
    let own = branch::BRANCH_MASS * branch::GRAVITY;
    let rise = branch::BRANCH_HALF.y + branch::WEIGHT_HALF.y;
    let mut turn = 0.0_f32;
    for _ in 0..64 {
        let (sin, cos) = turn.sin_cos();
        let moment = own * branch::BRANCH_HALF.x * cos
            + load.weight() * (branch::WEIGHT_REACH * cos + rise * sin);
        turn = moment / stiffness;
    }
    turn
}

/// Run the bending branch for `seconds` under `load`. Its swing has not died
/// away, so check over the many swings of that time: its mean droop is within
/// 1% of its balance, and its hinge's mean moment within 1% of the weights'
/// own at their lever arms as they lie. Nothing charges the hinge. Returns its
/// mean droop.
fn bending_rest(s: &mut MaterialLabState, seconds: u32, load: Load) -> f32 {
    let (mut droop, mut moment, mut statics) = (0.0_f64, 0.0_f64, 0.0_f64);
    let steps = (f64::from(seconds) / f64::from(branch::BEND_DT)).round() as u32;
    for _ in 0..steps {
        s.advance();
        render::frame(s);
        let b = s.branch().unwrap();
        assert_eq!(b.load.charged, 0.0, "{b:?}");
        assert!(b.load.utilization < 1.0, "{b:?}");
        droop -= f64::from(b.deflection.unwrap());
        moment -= f64::from(b.load.moment);
        statics += f64::from(
            branch::BRANCH_MASS * branch::GRAVITY * b.branch.x + load.weight() * b.weight.x,
        );
    }
    let droop = (droop / f64::from(steps)) as f32;
    let balance = bending_balance(load);
    assert!(
        (droop - balance).abs() < 0.01 * balance,
        "{load:?}: {droop} against {balance}"
    );
    assert!(
        (moment - statics).abs() < 0.01 * statics,
        "{load:?}: {moment} against {statics}"
    );
    droop
}

#[test]
fn bending_branch_sags_springs_back_and_breaks_under_the_heavy_load() {
    let mut s = MaterialLabState::new(Case::BranchBends);
    let start = s.branch().unwrap();
    assert_eq!((start.tip_load, start.deflection), (0.0, Some(0.0)));
    assert_eq!((start.safe_moment, s.phase()), (40000.0, Phase::Holding));
    assert_eq!(
        s.guidance(),
        Some("Run to watch the branch sag and swing; the lab will pause.")
    );
    // Released level, the branch swings down under its own weight to about
    // twice the bend it comes to rest at, within its safe moment, and rings.
    s.paused = false;
    let (mut peak, mut lowest) = (0.0_f32, 0.0_f32);
    while !s.paused {
        s.advance();
        render::frame(&s);
        let b = s.branch().unwrap();
        assert_eq!(b.load.charged, 0.0, "{b:?}");
        peak = peak.max(b.load.utilization);
        lowest = lowest.max(-b.deflection.unwrap());
    }
    assert_eq!(s.steps, branch::BEND_CHECKPOINT_STEP);
    assert!((s.elapsed() - 2.0).abs() < 1e-6, "{}", s.elapsed());
    assert!(peak < 0.85, "{peak}");
    let balance = bending_balance(Load::Released);
    assert!(lowest > 1.7 * balance && lowest < 2.0 * balance, "{lowest}");
    assert_eq!(
        s.guidance(),
        Some("Apply a load, then Run; the heavy load breaks the hinge.")
    );
    s.paused = false;
    let resting = bending_rest(&mut s, 6, Load::Released);
    // The light load bends it further, and lifting it lets the branch spring
    // back to where it rested. Nothing charges the hinge.
    s.set_load(Load::Light).unwrap();
    assert_eq!(
        s.guidance(),
        Some("Release load to let it spring back, or apply heavy load.")
    );
    let loaded = bending_rest(&mut s, 6, Load::Light);
    assert!(
        loaded > resting + 1.0_f32.to_radians(),
        "{loaded} {resting}"
    );
    s.set_load(Load::Released).unwrap();
    let back = bending_rest(&mut s, 6, Load::Released);
    assert!((back - resting).abs() < 0.01 * resting, "{back} {resting}");
    let b = s.branch().unwrap();
    assert_eq!((b.spent, s.phase()), (0.0, Phase::Holding));
    // The weight rides the branch, fastened where it was placed.
    let fastened = |b: &branch::Branch| {
        let (sin, cos) = b.branch.angle.sin_cos();
        let (dx, dy) = (b.weight.x - b.branch.x, b.weight.y - b.branch.y);
        (
            dx * cos + dy * sin,
            dy * cos - dx * sin,
            b.weight.angle - b.branch.angle,
        )
    };
    let place = fastened(&b);
    assert!(
        (place.0 - 1.5).abs() < 1e-3 && (place.1 - 0.75).abs() < 1e-3,
        "{place:?}"
    );
    assert!(place.2.abs() < 1e-3, "{place:?}");
    // The heavy load swings it past its safe moment, and the overload drains
    // the hinge within a few steps.
    s.set_load(Load::Heavy).unwrap();
    assert_eq!(
        s.guidance(),
        Some("Run until the hinge gives way; the lab will pause.")
    );
    let before = s.steps;
    run_until_paused(&mut s);
    let severed = s.branch().unwrap();
    assert_eq!(s.severed_at, Some(s.steps));
    assert!(s.steps - before <= 32, "{}", s.steps - before);
    assert_eq!((severed.remaining, severed.deflection), (0.0, None));
    assert!(severed.load.utilization > 1.0 && severed.excess.unwrap() >= 0.0);
    assert_eq!((severed.time_to_failure, s.phase()), (None, Phase::Falling));
    assert_eq!(
        s.guidance(),
        Some("The hinge has severed. Run to watch the branch fall.")
    );
    assert!(
        s.set_load(Load::Light)
            .unwrap_err()
            .starts_with("The hinge has severed")
    );
    // The branch falls with the weight still fastened on, faster than from
    // rest since it was swinging down, and ends above the trunk's foot.
    run_until_paused(&mut s);
    assert!(s.stopped.as_ref().unwrap().starts_with("Run complete"));
    assert_eq!(s.steps, s.severed_at.unwrap() + branch::BEND_FALL_STEPS);
    let end = s.branch().unwrap();
    let fall = branch::BEND_FALL_STEPS as f32 * branch::BEND_DT;
    assert!(
        end.drop > severed.drop + 0.5 * branch::GRAVITY * fall * fall,
        "{end:?}"
    );
    let (sin, cos) = end.branch.angle.sin_cos();
    let lowest =
        end.branch.y - sin.abs() * branch::BRANCH_HALF.x - cos.abs() * branch::BRANCH_HALF.y;
    assert!(
        lowest > branch::TRUNK_CENTER.y - branch::TRUNK_HALF.y,
        "{lowest}"
    );
    let after = fastened(&end);
    assert!(
        (after.0 - place.0).abs() < 1e-3 && (after.1 - place.1).abs() < 1e-3,
        "{after:?}"
    );
    assert_eq!(s.events.len(), 1);
}

#[test]
fn bending_branch_saves_replay_its_swing_and_load_changes() {
    let mut s = MaterialLabState::new(Case::BranchBends);
    let mut saved = 0;
    while s.stopped.is_none() {
        match s.steps {
            600 => s.set_load(Load::Light).unwrap(),
            640 => s.set_load(Load::Heavy).unwrap(),
            _ => {}
        }
        // Mid-swing, at the checkpoint, beside each load change, and at and
        // after the severing.
        if [0, 1, 20, branch::BEND_CHECKPOINT_STEP, 600, 601, 640, 641].contains(&s.steps)
            || s.severed_at.is_some()
        {
            saved += 1;
            let bytes = s.snapshot_bytes().unwrap();
            assert_eq!(&bytes[..8], BRANCH_SAVE_MAGIC);
            // The native snapshot keeps the v1 layout, with the hinge and the
            // fastening in its connection section.
            let world = s.world.snapshot_bytes().unwrap();
            assert_eq!(u32::from_le_bytes(world[..4].try_into().unwrap()), 1);
            let mut loaded = restored(&s);
            assert_eq!(loaded.branch(), s.branch());
            assert_eq!(
                (&loaded.events, loaded.severed_at),
                (&s.events, s.severed_at)
            );
            // A save never passes as the welded branch's.
            let mut save: (Save, Vec<LoadChange>) = codec().deserialize(&bytes[8..]).unwrap();
            save.0.case = Case::BranchFailure;
            let mut forged = BRANCH_SAVE_MAGIC.to_vec();
            forged.extend(codec().serialize(&save).unwrap());
            assert!(MaterialLabState::from_snapshot_bytes(&forged).is_err());
            // A restored run continues exactly.
            loaded.advance();
            s.advance();
            assert_eq!(
                loaded.snapshot_bytes().unwrap(),
                s.snapshot_bytes().unwrap()
            );
            continue;
        }
        s.advance();
    }
    assert!(saved > 10, "{saved}");
    assert_eq!(
        codec().serialize(&Case::BranchBends).unwrap(),
        12_u32.to_le_bytes()
    );
}

/// The step the shell's sweep reaches and cuts the last tuft on its path, and
/// the step it stops on the plate's face.
const LAST_TUFT: u32 = 65;
const ON_THE_PLATE: u32 = 108;

#[test]
fn shell_case_cuts_the_moss_strikes_the_plate_and_bursts_where_it_stops() {
    let mut s = MaterialLabState::new(Case::ShellThroughMoss);
    let mass = s.world.body_mass(scene::body(shell::SHELL_BODY)).unwrap();
    assert_eq!(
        s.guidance(),
        Some("Run to fire the shell; the lab pauses once it has cut the last tuft on its path.")
    );
    // The untouched path's work reads as zero, not minus zero.
    let texts: Vec<String> = render::frame(&s)
        .layers
        .into_iter()
        .flat_map(|layer| layer.primitives)
        .filter_map(|primitive| match primitive {
            engine_common::RenderPrimitive::Text(text) => Some(text.text),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|text| text == "0.000  ·  0 cut"));
    run_until_paused(&mut s);

    // At the last tuft: each tuft on the path is cut by its pass, and the
    // shell keeps its speed and course. Nothing else is touched.
    assert_eq!(s.steps, LAST_TUFT);
    assert!(s.stopped.is_none());
    assert_eq!(s.phase(), Phase::Flight);
    assert!(
        s.guidance()
            .unwrap()
            .starts_with("The shell kept its speed")
    );
    let view = s.shell().unwrap();
    assert_eq!(view.stage, ShellStage::Clear);
    assert_eq!(view.speed, Some(shell::SPEED));
    assert_eq!(view.position.unwrap().y, 0.0);
    for tuft in &view.tufts[..shell::PATH] {
        assert!(tuft.cut);
        assert_eq!(tuft.pass, Some(tuft.capacity));
        assert_eq!(tuft.remaining, 0.0);
    }
    for tuft in &view.tufts[shell::PATH..] {
        assert!(!tuft.cut);
        assert_eq!(tuft.remaining, tuft.capacity);
    }
    assert_eq!(view.plate_remaining, view.plate_capacity);
    let labels: Vec<&str> = s.events.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, ["Cut M1", "Cut M2", "Cut M3"]);
    // The first tuft is reached on step 24, as the shell's front crosses 1.4.
    assert_eq!(s.events[0].time, 24.0 * f64::from(PHYSICS_DT));

    // On the plate: the hit is the impact law's outcome for the shell's
    // casing against the armor at full speed, the shell bursts where it
    // stopped, and the run is complete.
    run_until_paused(&mut s);
    assert_eq!(s.steps, ON_THE_PLATE);
    assert!(s.stopped.as_ref().unwrap().starts_with("Run complete"));
    assert_eq!(s.phase(), Phase::Spent);
    assert_eq!(s.impacts, 1);
    let view = s.shell().unwrap();
    assert_eq!(view.stage, ShellStage::Spent);
    assert_eq!(view.position, None);
    assert!(s.world.motion(scene::body(shell::SHELL_BODY)).is_none());
    let contact = engine_material::ContactGeometry::new(
        engine_material::patch_width_from_radii(shell::SHELL_RADIUS, None).unwrap(),
        shell::CASING,
        shell::PLATE_SECTION,
    );
    let load = engine_material::Impact {
        closing_speed: shell::SPEED,
        effective_mass: engine_material::effective_mass(
            engine_material::ContactBody::translating(mass),
            engine_material::ContactBody::IMMOVABLE,
        )
        .unwrap(),
    };
    let hit = view.hit.unwrap();
    assert_eq!(
        hit,
        engine_material::impact(&shell::casing(), &shell::armor(), contact, load).unwrap()
    );
    // The moss left the shell its whole launch energy, and its casing is
    // spent.
    assert_eq!(hit.ledger.available, load.energy());
    assert!(hit.failed[0]);
    assert_eq!(hit.consumed[0], shell::casing().capacity());
    let burst = view.burst.unwrap();
    assert!(
        (5.25..5.3).contains(&burst.x) && burst.y == 0.0,
        "{burst:?}"
    );
    // The blast reaches the plate and the tuft in the open, which it cuts;
    // the plate shadows the tuft behind it, and cut tufts are gone.
    let plate_blast = view.plate_blast.unwrap();
    assert!(plate_blast > 0.0);
    let [.., open, behind] = view.tufts[..] else {
        unreachable!()
    };
    assert!(open.cut && open.blast == Some(open.capacity) && open.pass.is_none());
    assert!(!behind.cut && behind.blast.is_none());
    assert_eq!(behind.remaining, behind.capacity);
    assert!(view.tufts[..shell::PATH].iter().all(|t| t.blast.is_none()));
    assert_eq!(
        view.plate_remaining,
        view.plate_capacity - f64::from(hit.consumed[1]) - f64::from(plate_blast)
    );
    // The ledger holds every charge: the passes and the hit for the kinetic
    // part, which delivers more than its budget since the hit is measured at
    // full speed, and the blast's shares for the payload, which open
    // directions leave short of its budget.
    let ledger = view.ledger;
    let passes: f64 = view
        .tufts
        .iter()
        .filter_map(|t| t.pass)
        .map(f64::from)
        .sum();
    assert_eq!(
        ledger.kinetic.budget,
        0.5 * f64::from(mass) * f64::from(shell::SPEED).powi(2)
    );
    assert_eq!(f64::from(hit.ledger.available), ledger.kinetic.budget);
    assert!(ledger.kinetic.delivered > ledger.kinetic.budget);
    assert!(
        (ledger.kinetic.absorbed - passes - f64::from(hit.consumed[1])).abs() < 1.0e-9,
        "{ledger:?}"
    );
    assert!(
        (ledger.payload.absorbed - f64::from(plate_blast) - f64::from(open.capacity)).abs()
            < 1.0e-9,
        "{ledger:?}"
    );
    assert_eq!(ledger.payload.budget, f64::from(shell::PAYLOAD.energy));
    assert!(ledger.payload.delivered < 0.5 * ledger.payload.budget);
    // The figures the case's note gives.
    let figures = [
        (f64::from(hit.consumed[1]), 8310.71),
        (f64::from(plate_blast), 2096.12),
        (view.plate_remaining, 29593.16),
        (ledger.kinetic.delivered - ledger.kinetic.budget, 9.43),
        (ledger.kinetic.absorbed, 8314.31),
        (ledger.kinetic.excess, 1434.80),
        (ledger.payload.delivered, 2266.87),
        (ledger.payload.absorbed, 2097.32),
    ];
    for (value, figure) in figures {
        assert!((value - figure).abs() < 0.005, "{value} against {figure}");
    }
    // The contact response refused the shell's arrival once and applied
    // nothing.
    assert_eq!(view.refused, 1);
    let response = s.world.material_response_diagnostics();
    assert_eq!(response.refused_batches, 1);
    assert_eq!(
        response.last_rejection,
        Some(engine_rapier::world::loading::response::ResponseRejection::UnboundContact)
    );
    assert_eq!(response.applied_batches, 0);
    let labels: Vec<&str> = s.events.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Cut M1",
            "Cut M2",
            "Cut M3",
            "Hit the plate",
            "Burst",
            "Cut M4"
        ]
    );
    assert!(s.guidance().is_none());
    assert!(s.set_load(Load::Heavy).is_err());
}

#[test]
fn shell_case_takes_no_load_and_ends_its_window_if_nothing_stops_the_shell() {
    let mut s = MaterialLabState::new(Case::ShellThroughMoss);
    s.advance();
    let before = s.snapshot_bytes().unwrap();
    assert_eq!(
        s.set_load(Load::Light).unwrap_err(),
        "The shell case takes no external load. Choose a platen case."
    );
    assert_eq!(s.snapshot_bytes().unwrap(), before);
    // With the plate lifted clear, the shell flies on over the window.
    s.world.set_pose(
        scene::body(shell::PLATE),
        engine_core::Vec2::new(6.0, 10.0),
        0.0,
        true,
    );
    while s.stopped.is_none() {
        s.advance();
    }
    assert_eq!(s.steps, shell::WINDOW);
    assert!(s.stopped.as_ref().unwrap().starts_with("Run complete"));
    let view = s.shell().unwrap();
    assert_eq!(view.stage, ShellStage::Clear);
    assert_eq!(view.speed, Some(shell::SPEED));
    assert!(view.hit.is_none() && view.burst.is_none());
    assert_eq!(view.ledger.payload.delivered, 0.0);
}

#[test]
fn shell_case_stops_at_any_other_refused_response_and_keeps_its_last_valid_world() {
    let mut s = MaterialLabState::new(Case::ShellThroughMoss);
    // Bound to a region, the shell's approach is no longer the unbound
    // arrival the case expects.
    let body = scene::body(shell::SHELL_BODY);
    let token = s
        .world
        .create_loading_region(
            engine_rapier::world::loading::RegionId {
                entity: body.entity,
                part: 0,
            },
            body,
        )
        .unwrap();
    s.world
        .bind_loading_collider(
            scene::collider(shell::SHELL_BODY),
            engine_rapier::world::loading::ContactSource {
                region: token,
                patch: 0,
            },
        )
        .unwrap();
    let mut last = s.world.snapshot_bytes().unwrap();
    while s.stopped.is_none() {
        last = s.world.snapshot_bytes().unwrap();
        s.advance();
    }
    // The step that would reach the plate starts with the contact still
    // separated, and the response refuses it.
    assert!(
        s.stopped
            .as_ref()
            .unwrap()
            .contains("material response refused (SpeculativeArrival)"),
        "{:?}",
        s.stopped
    );
    assert_eq!(s.steps, ON_THE_PLATE - 1);
    assert!(s.paused);
    assert_eq!(s.world.snapshot_bytes().unwrap(), last);
    let view = s.shell().unwrap();
    assert!(view.hit.is_none());
    assert_eq!(view.plate_remaining, view.plate_capacity);
}

#[test]
fn shell_case_saves_keep_their_own_layout_and_replay_the_flight() {
    let mut s = MaterialLabState::new(Case::ShellThroughMoss);
    for steps in [0, 1, 24, LAST_TUFT, 66, ON_THE_PLATE - 1, ON_THE_PLATE] {
        while s.steps < steps && s.stopped.is_none() {
            s.advance();
        }
        assert_eq!(s.steps, steps, "{:?}", s.stopped);
        let bytes = s.snapshot_bytes().unwrap();
        assert_eq!(&bytes[..8], SHELL_SAVE_MAGIC);
        // The native snapshot keeps the response-enabled v3 layout.
        let world = s.world.snapshot_bytes().unwrap();
        assert_eq!(u32::from_le_bytes(world[..4].try_into().unwrap()), 3);
        let mut loaded = restored(&s);
        assert_eq!(loaded.shell(), s.shell());
        assert_eq!(loaded.events, s.events);
        for magic in [
            SAVE_MAGIC,
            LOAD_SAVE_MAGIC,
            PAIR_SAVE_MAGIC,
            FAILURE_SAVE_MAGIC,
            PAIR_FAILURE_SAVE_MAGIC,
            SPLIT_SAVE_MAGIC,
            BRANCH_SAVE_MAGIC,
        ] {
            let mut bad = bytes.clone();
            bad[..8].copy_from_slice(magic);
            assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
        }
        let changes: &[fn(&mut Save)] = &[
            |s| s.case = Case::RegionSplit,
            |s| s.steps += 1,
            |s| s.impacts += 1,
            |s| {
                s.events.push(Event {
                    time: 0.0,
                    label: "Burst".into(),
                    episode: 1,
                })
            },
            |s| s.stopped = Some("Run complete".into()),
        ];
        for change in changes {
            let mut save: Save = codec().deserialize(&bytes[8..]).unwrap();
            change(&mut save);
            let mut bad = SHELL_SAVE_MAGIC.to_vec();
            bad.extend(codec().serialize(&save).unwrap());
            assert!(MaterialLabState::from_snapshot_bytes(&bad).is_err());
        }
        // A restored run continues exactly.
        loaded.advance();
        s.advance();
        assert_eq!(
            loaded.snapshot_bytes().unwrap(),
            s.snapshot_bytes().unwrap()
        );
    }
    for case in [Case::RegionSplit, Case::BranchFailure] {
        let mut old = MaterialLabState::new(case);
        old.advance();
        let mut bytes = old.snapshot_bytes().unwrap();
        bytes[..8].copy_from_slice(SHELL_SAVE_MAGIC);
        assert!(MaterialLabState::from_snapshot_bytes(&bytes).is_err());
    }
    assert_eq!(
        codec().serialize(&Case::ShellThroughMoss).unwrap(),
        11_u32.to_le_bytes()
    );
}
