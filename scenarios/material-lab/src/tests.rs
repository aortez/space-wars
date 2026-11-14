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
    for case in Case::ALL
        .into_iter()
        .filter(|c| ![Case::LoadRelease, Case::RegionSplit].contains(c) && c.free_pair().is_none())
    {
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
