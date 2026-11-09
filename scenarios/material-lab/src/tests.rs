use super::*;
#[test]
fn qualified_cases_finish_with_expected_material_behavior() {
    for case in Case::ALL.into_iter().filter(|c| *c != Case::LoadRelease) {
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
    for _ in 0..128 {
        MaterialLabScenario::step(&mut s, &[], Duration::from_secs_f32(1.0 / 60.0));
    }
    assert!(s.paused);
    assert_eq!(s.steps, 128);
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
