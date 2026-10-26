use super::*;
#[test]
fn qualified_cases_finish_with_expected_material_behavior() {
    for case in Case::ALL {
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
    for case in Case::ALL {
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
