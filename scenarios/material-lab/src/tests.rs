use super::*;
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
        .filter(|c| *c != Case::LoadRelease && c.free_pair().is_none())
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
