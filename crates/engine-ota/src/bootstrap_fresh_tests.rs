use super::*;

pub(in crate::bootstrap) struct Fixture {
    pub(in crate::bootstrap) image: ImageIdentity,
    pub(in crate::bootstrap) contents: BTreeMap<String, Vec<u8>>,
}

impl Fixture {
    pub(in crate::bootstrap) fn new() -> Self {
        let assets = crate::bootstrap::tests::Assets::new();
        let mut contents = assets.contents;
        let config = crate::bootstrap::inspect::LEGACY_CONFIG.replace(
            "include spacewars-device.txt\n",
            "initramfs spacewars-recovery.cpio.gz followkernel\ninclude spacewars-device.txt\n",
        );
        contents.insert("config.txt".into(), config.as_bytes().to_vec());
        contents.insert(
            "tryboot.txt".into(),
            format!(
                "{}\n[all]\ncmdline=spacewars-trial.txt\n",
                config.trim_end()
            )
            .into_bytes(),
        );
        contents.insert(
            "spacewars-kernel-build-id".into(),
            format!("{}\n", "a".repeat(40)).into_bytes(),
        );
        let manifest = crate::bootstrap::tests::manifest(&contents);
        let mut image = assets.image;
        image.boot_identity = manifest.identity();
        contents.insert(
            "spacewars-boot-manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        contents.insert(
            "spacewars-boot-id".into(),
            format!("{}\n", image.boot_identity).into_bytes(),
        );
        contents.insert(
            "spacewars-trial.txt".into(),
            UNPREPARED_TRIAL.as_bytes().to_vec(),
        );
        contents.insert(
            "cmdline.txt".into(),
            b"root=/dev/sda3 rootfstype=ext4 rootwait  roottimeout=30 console=tty1\n".to_vec(),
        );
        contents.insert(
            "spacewars-device.txt".into(),
            b"include spacewars-profiles/hyperpixel.txt\n".to_vec(),
        );
        contents.insert("hostname.txt".into(), b"fresh-fixture\n".to_vec());
        Self { image, contents }
    }

    fn inspect(&self, running: Option<&str>) -> Result<BootstrapOrigin> {
        let files = self
            .contents
            .iter()
            .map(|(path, bytes)| {
                (
                    path.clone(),
                    Blob {
                        bytes: bytes.len() as u64,
                        sha256: Sha256::digest(bytes),
                    },
                )
            })
            .collect();
        let default = std::str::from_utf8(&self.contents["cmdline.txt"]).unwrap();
        inspect(
            self.image.clone(),
            &files,
            "sda3",
            &"a".repeat(40),
            running.unwrap_or(default),
            &|path, limit| {
                let bytes = self
                    .contents
                    .get(path)
                    .ok_or_else(|| invalid("missing fixture input"))?;
                if bytes.len() as u64 > limit {
                    return Err(invalid("oversized fixture input"));
                }
                Ok(bytes.clone())
            },
        )
    }
}

#[test]
fn fresh_inventory_binds_the_image_kernel_and_complete_factory_boot_set() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.inspect(None).unwrap(),
        BootstrapOrigin::FreshImage {
            image: Box::new(fixture.image.clone())
        }
    );
    fixture
        .inspect(Some(&format!(
            "firmware=value {}",
            std::str::from_utf8(&fixture.contents["cmdline.txt"]).unwrap()
        )))
        .unwrap();
    for path in fixture.contents.keys().filter(|p| {
        p.as_str() != "cmdline.txt"
            && p.as_str() != "hostname.txt"
            && p.as_str() != "spacewars-device.txt"
    }) {
        let mut changed = Fixture::new();
        changed.contents.remove(path);
        assert!(changed.inspect(None).is_err(), "missing {path}");
    }
    for path in [
        "kernel8.img",
        "config.txt",
        "tryboot.txt",
        "spacewars-kernel-build-id",
        "spacewars-boot-id",
        "spacewars-boot-manifest.json",
        "spacewars-trial.txt",
    ] {
        let mut changed = Fixture::new();
        changed.contents.insert(path.into(), b"changed".to_vec());
        assert!(changed.inspect(None).is_err(), "changed {path}");
    }
    for path in [
        "autoboot.txt",
        "sw-bootstrap/kernel8.img",
        ".sw-bootstrap-tryboot.pending",
        "unknown.txt",
    ] {
        let mut changed = Fixture::new();
        changed.contents.insert(path.into(), b"extra".to_vec());
        assert!(changed.inspect(None).is_err(), "unexpected {path}");
    }
    for change in [0, 1, 2, 3] {
        let mut changed = Fixture::new();
        match change {
            0 => changed.image.source.dirty = true,
            1 => changed.image.boot_identity = Sha256::digest(b"other image"),
            2 => changed.image.recovery_protocol += 1,
            _ => changed.image.machine = "other-machine".into(),
        }
        assert!(changed.inspect(None).is_err());
    }
}

#[test]
fn prepared_or_running_trial_selectors_cannot_be_mistaken_for_a_fresh_flash() {
    for root in ["/dev/sda2", "PARTUUID=076c4a2a-03"] {
        let mut fixture = Fixture::new();
        let default = std::str::from_utf8(&fixture.contents["cmdline.txt"])
            .unwrap()
            .replace("/dev/sda3", root);
        fixture
            .contents
            .insert("cmdline.txt".into(), default.into_bytes());
        assert!(fixture.inspect(None).is_err(), "{root}");
    }
    let fixture = Fixture::new();
    let default = std::str::from_utf8(&fixture.contents["cmdline.txt"]).unwrap();
    for prefix in [
        "spacewars.trial=unprepared",
        "root=/dev/sda3",
        "init=/bin/sh",
        "rdinit=/bin/sh",
        "systemd.unit=rescue.target",
    ] {
        assert!(
            fixture
                .inspect(Some(&format!("{prefix} {default}")))
                .is_err(),
            "{prefix}"
        );
    }
}

#[test]
fn manifest_identity_alone_does_not_authorize_different_firmware_selectors() {
    for (path, bytes) in [
        ("config.txt", b"[all]\ninclude alternate.txt\n".as_slice()),
        ("tryboot.txt", b"[all]\ncmdline=alternate.txt\n"),
    ] {
        let mut fixture = Fixture::new();
        fixture.contents.insert(path.into(), bytes.to_vec());
        let immutable = fixture
            .contents
            .iter()
            .filter(|(path, _)| {
                !MUTABLE_BOOT_FILES.contains(&path.as_str())
                    && !OPTIONAL_BOOT_FILES.contains(&path.as_str())
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let manifest = crate::bootstrap::tests::manifest(&immutable);
        fixture.image.boot_identity = manifest.identity();
        fixture.contents.insert(
            "spacewars-boot-manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        fixture.contents.insert(
            "spacewars-boot-id".into(),
            format!("{}\n", manifest.identity()).into_bytes(),
        );
        assert!(fixture.inspect(None).is_err(), "{path}");
    }
}
