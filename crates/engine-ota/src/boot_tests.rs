use super::*;

fn fixture() -> BootManifest {
    BootManifest::parse(
        include_bytes!("../tests/fixtures/boot-manifest-v2.json"),
        include_str!("../tests/fixtures/boot-manifest-v2.sha256")
            .trim()
            .to_owned()
            .try_into()
            .unwrap(),
    )
    .unwrap()
}

fn template() -> BootCommandLine {
    BootCommandLine::parse(
        b"root=@SPACEWARS_ROOT@ rootfstype=ext4 rootwait roottimeout=30 console=tty1\n",
    )
    .unwrap()
}

fn roots() -> SlotRoots {
    SlotRoots::new("076c4a2a-02".into(), "076c4a2a-03".into()).unwrap()
}

#[test]
fn python_manifest_identity_binds_files_and_excludes_only_defined_mutable_state() {
    let manifest = fixture();
    assert!(!manifest.files().contains_key("cmdline.txt"));
    assert!(manifest.files().contains_key("spacewars-cmdline.txt"));
    let bytes = serde_json::to_vec(&manifest).unwrap();
    BootManifest::parse(&bytes, manifest.identity()).unwrap();
    assert!(BootManifest::parse(&bytes, Sha256::digest(b"different boot set")).is_err());
    let mut modified = manifest.clone();
    modified.files.get_mut("kernel8.img").unwrap().sha256 = Sha256::digest(b"different kernel");
    assert_ne!(modified.identity(), manifest.identity());
    assert!(modified.validate().is_err());
}

#[test]
fn malformed_asset_sets_cannot_hide_missing_mutable_or_ambiguous_files() {
    let manifest = fixture();
    for name in [
        "cmdline.txt",
        "CMDLINE.TXT",
        "hostname.txt",
        "HOSTNAME.TXT",
        "../escape",
        "/absolute",
        "overlays/../escape",
        "config.TXT",
        "overlays//extra",
    ] {
        let mut changed = manifest.clone();
        changed.files.insert(
            name.into(),
            Blob {
                bytes: 1,
                sha256: Sha256::digest(b"x"),
            },
        );
        assert!(changed.validate().is_err(), "{name}");
    }
    let mut changed = manifest.clone();
    changed.files.remove("spacewars-recovery.cpio.gz");
    assert!(changed.validate().is_err());
    let mut changed = manifest.clone();
    changed.files.get_mut("tryboot.txt").unwrap().bytes = MAX_BOOT_ASSET_BYTES + 1;
    assert!(changed.validate().is_err());
    let bytes = serde_json::to_string(&manifest).unwrap();
    let blob = serde_json::to_string(&manifest.files["config.txt"]).unwrap();
    let repeated = bytes.replacen(
        "\"files\":{",
        &format!("\"files\":{{\"config.txt\":{blob},"),
        1,
    );
    assert!(BootManifest::parse(repeated.as_bytes(), manifest.identity()).is_err());
    assert!(
        BootManifest::parse(
            &vec![b' '; MAX_BOOT_MANIFEST_BYTES + 1],
            manifest.identity()
        )
        .is_err()
    );
}

#[test]
fn root_selection_accepts_only_canonical_provisioned_normal_or_trial_files() {
    let template = template();
    let roots = roots();
    let id = Sha256::digest(b"one durable transaction");
    assert_eq!(
        template.trial(&roots, UNPREPARED_TRIAL.as_bytes()).unwrap(),
        None
    );
    for slot in [Slot::A, Slot::B] {
        let normal = template.render(&roots, slot, None);
        assert_eq!(
            template.default_slot(&roots, normal.as_bytes()).unwrap(),
            slot
        );
        let trial = template.render(&roots, slot, Some(id));
        assert_eq!(
            template.trial(&roots, trial.as_bytes()).unwrap(),
            Some(TrialSelection {
                slot,
                transaction: id
            })
        );
        assert!(template.default_slot(&roots, trial.as_bytes()).is_err());
        assert!(template.trial(&roots, normal.as_bytes()).is_err());
        assert!(
            template
                .trial(
                    &roots,
                    format!("{} spacewars.trial={id}\n", trial.trim()).as_bytes()
                )
                .is_err()
        );
        assert!(
            template
                .default_slot(
                    &roots,
                    normal.replace("PARTUUID=076c4a2a-", "/dev/sda").as_bytes()
                )
                .is_err()
        );
    }
    assert!(SlotRoots::new("076c4a2a-02".into(), "076c4a2a-02".into()).is_err());
    assert!(SlotRoots::new("/dev/sda2".into(), "076c4a2a-03".into()).is_err());
    SlotRoots::new(
        "11111111-2222-3333-4444-555555555555".into(),
        "11111111-2222-3333-4444-666666666666".into(),
    )
    .unwrap();
}

#[test]
fn firmware_prefix_is_allowed_but_cannot_override_recovery_arguments() {
    let template = template();
    let roots = roots();
    let id = Sha256::digest(b"trial");
    let selected = template.render(&roots, Slot::B, Some(id));
    let actual = format!("reboot=w coherent_pool=1M numa=fake=8 {selected}");
    template
        .check_running(&roots, Slot::B, Some(id), &actual)
        .unwrap();
    assert!(
        template
            .check_running(&roots, Slot::A, Some(id), &actual)
            .is_err()
    );
    assert!(
        template
            .check_running(&roots, Slot::B, None, &actual)
            .is_err()
    );
    for prefix in [
        "init=/bin/sh",
        "rdinit=/bin/sh",
        "rootdelay=0",
        "root=PARTUUID=076c4a2a-02",
        "spacewars.trial=other",
        "spacewars.trial",
        "rootfstype=xfs",
        "roottimeout=999",
    ] {
        assert!(
            template
                .check_running(&roots, Slot::B, Some(id), &format!("{prefix} {selected}"))
                .is_err(),
            "{prefix}"
        );
    }
    assert!(
        template
            .check_running(&roots, Slot::B, Some(id), &format!("{selected}\0"))
            .is_err()
    );
}

#[test]
fn unsafe_or_ambiguous_templates_are_rejected_before_rendering() {
    let baseline = template().0;
    for addition in [
        "init=/bin/sh",
        "init",
        "rdinit=/bin/sh",
        "rootdelay=0",
        "root=other",
        "rootfstype=xfs",
        "roottimeout=999",
        "spacewars.trial=bad",
        "x=$(command)",
    ] {
        let changed = format!("{} {addition}\n", baseline.trim());
        assert!(
            BootCommandLine::parse(changed.as_bytes()).is_err(),
            "{addition}"
        );
    }
    assert!(
        BootCommandLine::parse(
            baseline
                .replace("root=@SPACEWARS_ROOT@", "root=/dev/sda2")
                .as_bytes()
        )
        .is_err()
    );
    assert!(
        BootCommandLine::parse(
            baseline
                .replace("roottimeout=30", "roottimeout=500")
                .as_bytes()
        )
        .is_err()
    );
}

#[test]
fn real_pi_kernel_notes_decode_and_truncation_duplicates_are_rejected() {
    let notes = include_bytes!("../tests/fixtures/kernel-notes-pi5.bin");
    assert_eq!(
        kernel_build_id(notes).unwrap(),
        "4d7dcbe8205972a1d0673a2e40d6f1f2cb1a8fa6"
    );
    for length in 0..36 {
        assert!(kernel_build_id(&notes[..length]).is_err(), "{length}");
    }
    let mut duplicate = notes.to_vec();
    duplicate.extend_from_slice(&notes[..36]);
    assert!(kernel_build_id(&duplicate).is_err());
    let mut malformed = notes.to_vec();
    malformed[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(kernel_build_id(&malformed).is_err());
    assert!(kernel_build_id(&notes[..notes.len() - 1]).is_err());
}

#[test]
fn hardware_selection_is_data_only_and_requires_one_known_profile() {
    assert_eq!(
        hardware_profile("# comment\n[all]\ninclude spacewars-profiles/picade.txt\n[all]\n")
            .unwrap(),
        "picade"
    );
    assert_eq!(
        hardware_profile("[all]\r\ninclude spacewars-profiles/hyperpixel.txt\r\n").unwrap(),
        "hyperpixel"
    );
    for invalid in [
        "",
        "[pi4]\ninclude spacewars-profiles/picade.txt\n",
        "include spacewars-profiles/unknown.txt\n",
        "include spacewars-profiles/hdmi.txt\ninclude spacewars-profiles/picade.txt\n",
        "exit 0\ninclude spacewars-profiles/hdmi.txt\n",
    ] {
        assert!(hardware_profile(invalid).is_err());
    }
}
