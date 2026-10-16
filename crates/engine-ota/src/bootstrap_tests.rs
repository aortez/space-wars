use super::*;

const CONFIG: &str = "# firmware configuration\n[pi4]\ndtoverlay=vc4-kms-v3d-pi4\n[pi5]\ndtoverlay=vc4-kms-v3d-pi5\n[all]\narm_64bit=1\ninitramfs spacewars-recovery.cpio.gz followkernel\ninclude spacewars-device.txt\n[all]\n";
pub(super) const TEMPLATE: &[u8] =
    b"root=@SPACEWARS_ROOT@ rootfstype=ext4 rootwait roottimeout=30 console=tty1\n";

pub(super) fn request(board: Board, profile: &str, target: Slot) -> PlanRequest {
    PlanRequest {
        board,
        profile: profile.into(),
        target,
        roots: SlotRoots::new("076c4a2a-02".into(), "076c4a2a-03".into()).unwrap(),
        trial_id: Sha256::digest(b"supervised attempt"),
    }
}

pub(super) struct Assets {
    pub(super) image: ImageIdentity,
    manifest: BootManifest,
    pub(super) contents: BTreeMap<String, Vec<u8>>,
}

impl Assets {
    pub(super) fn new() -> Self {
        let names: BootManifest =
            serde_json::from_slice(include_bytes!("../tests/fixtures/boot-manifest-v2.json"))
                .unwrap();
        let mut contents: BTreeMap<String, Vec<u8>> = names
            .files()
            .keys()
            .map(|p| (p.clone(), format!("fixture: {p}\n").into_bytes()))
            .collect();
        for path in [
            "kernel8.img",
            "kernel_2712.img",
            "bcm2711-rpi-4-b.dtb",
            "bcm2712-rpi-5-b.dtb",
            "bootcode.bin",
            "start4.elf",
            "fixup4.dat",
            "overlays/vc4-kms-v3d-pi4.dtbo",
            "overlays/vc4-kms-v3d-pi5.dtbo",
            "overlays/picade.dtbo",
            "overlays/vc4-kms-dpi-hyperpixel4.dtbo",
        ] {
            contents.insert(path.into(), b"same fixture bytes".to_vec());
        }
        contents.insert("config.txt".into(), CONFIG.as_bytes().to_vec());
        contents.insert("spacewars-cmdline.txt".into(), TEMPLATE.to_vec());
        for profile in ["hdmi", "hyperpixel", "picade"] {
            let hardware = match profile {
                "hyperpixel" => "[all]\ndtoverlay=vc4-kms-dpi-hyperpixel4\n",
                "picade" => {
                    "[pi4]\ndtparam=audio=off\ndtoverlay=picade,noactled\ndtparam=button1=304\n"
                }
                _ => "",
            };
            contents.insert(
                format!("spacewars-profiles/{profile}.txt"),
                format!("{hardware}[all]\ndtoverlay=spacewars-profile-{profile}\n").into_bytes(),
            );
        }
        let mut image =
            ImageIdentity::parse(include_bytes!("../tests/fixtures/image-identity-v1.json"))
                .unwrap();
        let manifest = manifest(&contents);
        image.boot_identity = manifest.identity();
        Self {
            image,
            manifest,
            contents,
        }
    }

    pub(super) fn plan(&self, request: &PlanRequest) -> Result<BootPlan> {
        BootPlan::create(
            &self.image,
            &self.manifest,
            CONFIG,
            std::str::from_utf8(
                &self.contents[&format!("spacewars-profiles/{}.txt", request.profile)],
            )
            .unwrap(),
            &BootCommandLine::parse(TEMPLATE).unwrap(),
            request,
        )
    }
}

fn manifest(contents: &BTreeMap<String, Vec<u8>>) -> BootManifest {
    let files: BTreeMap<_, _> = contents
        .iter()
        .map(|(name, bytes)| {
            (
                name,
                Blob {
                    bytes: bytes.len() as u64,
                    sha256: Sha256::digest(bytes),
                },
            )
        })
        .collect();
    let raw = serde_json::to_vec(&serde_json::json!({"schema": 2, "files": files})).unwrap();
    let parsed: BootManifest = serde_json::from_slice(&raw).unwrap();
    BootManifest::parse(&raw, parsed.identity()).unwrap()
}

#[test]
fn supported_plans_keep_one_explicit_kernel_and_distinct_trial_and_normal_roots() {
    let assets = Assets::new();
    for (board, profile) in [
        (Board::Pi4, "picade"),
        (Board::Pi4, "hdmi"),
        (Board::Pi5, "hdmi"),
        (Board::Pi4, "hyperpixel"),
        (Board::Pi5, "hyperpixel"),
    ] {
        for slot in [Slot::A, Slot::B] {
            let request = request(board, profile, slot);
            let plan = assets.plan(&request).unwrap();
            assert!(plan.trial_config.contains(&format!(
                "device_tree=/{STAGE_DIRECTORY}/{}\n",
                board.device_tree()
            )));
            assert!(
                plan.trial_config
                    .contains("kernel=/sw-bootstrap/kernel8.img\n")
            );
            assert!(
                plan.trial_config
                    .contains("initramfs /sw-bootstrap/spacewars-recovery.cpio.gz followkernel\n")
            );
            assert!(
                plan.trial_config
                    .contains("overlay_prefix=sw-bootstrap/overlays/\n")
            );
            assert_eq!(
                plan.trial_config
                    .replace("trial-cmdline.txt", "normal-cmdline.txt"),
                plan.normal_config
            );
            assert!(!plan.trial_config.contains("include "));
            assert!(!plan.trial_config.contains("os_prefix"));
            assert!(!plan.staged_assets.contains_key("kernel_2712.img"));
            assert_eq!(
                plan.staged_assets
                    .keys()
                    .filter(|p| p.starts_with("bcm"))
                    .count(),
                1
            );
            assert!(plan.unchanged_firmware.contains_key("start4.elf"));
            assert!(
                plan.staged_assets
                    .keys()
                    .all(|p| !plan.unchanged_firmware.contains_key(p))
            );
            assert_eq!(
                plan.staged_asset_bytes,
                plan.staged_assets.values().map(|b| b.bytes).sum::<u64>()
            );
            let template = BootCommandLine::parse(TEMPLATE).unwrap();
            assert_eq!(
                template
                    .default_slot(&request.roots, plan.normal_cmdline.as_bytes())
                    .unwrap(),
                slot
            );
            let selection = template
                .trial(&request.roots, plan.trial_cmdline.as_bytes())
                .unwrap()
                .unwrap();
            assert_eq!(selection.slot, slot);
            assert_eq!(selection.transaction, request.trial_id);
        }
    }
    assert!(
        assets
            .plan(&request(Board::Pi5, "picade", Slot::A))
            .is_err()
    );
    assert_eq!(
        Board::from_model("Raspberry Pi 5 Model B Rev 1.1").unwrap(),
        Board::Pi5
    );
    assert!(Board::from_model("Raspberry Pi Compute Module 4 Rev 1.0").is_err());
}

#[test]
fn boot_directives_cannot_hide_a_different_root_firmware_profile_or_kernel() {
    let assets = Assets::new();
    let request = request(Board::Pi5, "hyperpixel", Slot::A);
    let profile =
        std::str::from_utf8(&assets.contents["spacewars-profiles/hyperpixel.txt"]).unwrap();
    let template = BootCommandLine::parse(TEMPLATE).unwrap();
    for extra in [
        "cmdline=old.txt",
        "kernel=old.img",
        "device_tree=old.dtb",
        "os_prefix=old/",
        "tryboot_a_b=1",
        "boot_partition=2",
        "start_file=other.elf",
        "include config.txt",
        "initramfs other.gz followkernel",
        "[tryboot]",
        "dtoverlay=missing",
        "arm_64bit=1",
    ] {
        let config = format!("{CONFIG}{extra}\n");
        assert!(
            BootPlan::create(
                &assets.image,
                &assets.manifest,
                &config,
                profile,
                &template,
                &request
            )
            .is_err(),
            "{extra}"
        );
    }
    let conditional = CONFIG.replace("[all]\narm_64bit=1", "[pi4]\narm_64bit=1");
    assert!(
        BootPlan::create(
            &assets.image,
            &assets.manifest,
            &conditional,
            profile,
            &template,
            &request
        )
        .is_err()
    );
    for extra in [
        "include other.txt",
        "dtoverlay=spacewars-profile-hdmi",
        "dtparam=x;other",
        "dtoverlay=../../other",
        "[tryboot]",
        "arm_64bit=0",
    ] {
        let changed = format!("{profile}{extra}\n");
        assert!(
            BootPlan::create(
                &assets.image,
                &assets.manifest,
                CONFIG,
                &changed,
                &template,
                &request
            )
            .is_err(),
            "{extra}"
        );
    }
    let conditional = profile.replace(
        "[all]\ndtoverlay=spacewars-profile-",
        "[pi4]\ndtoverlay=spacewars-profile-",
    );
    assert!(
        BootPlan::create(
            &assets.image,
            &assets.manifest,
            CONFIG,
            &conditional,
            &template,
            &request
        )
        .is_err()
    );
}

#[test]
fn dirty_mismatched_and_incomplete_images_are_not_bootstrap_plans() {
    let mut assets = Assets::new();
    let request = request(Board::Pi4, "picade", Slot::B);
    assets.image.source.dirty = true;
    assets.image.source.revision.push_str("-dirty");
    assert!(assets.plan(&request).is_err());
    assets = Assets::new();
    assets.image.boot_identity = Sha256::digest(b"other");
    assert!(assets.plan(&request).is_err());
    for path in ["start4.elf", "bcm2711-rpi-4-b.dtb"] {
        let mut assets = Assets::new();
        assets.contents.remove(path);
        assets.manifest = manifest(&assets.contents);
        assets.image.boot_identity = assets.manifest.identity();
        assert!(assets.plan(&request).is_err());
    }
    assets = Assets::new();
    assets
        .contents
        .insert("unexpected.txt".into(), b"unknown".to_vec());
    assets.manifest = manifest(&assets.contents);
    assets.image.boot_identity = assets.manifest.identity();
    assert!(assets.plan(&request).is_err());
}

#[cfg(target_os = "linux")]
pub(crate) mod linux {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::PathBuf;
    use std::process::Command;

    pub(crate) struct Fixture {
        pub(crate) directory: tempfile::TempDir,
        root: PathBuf,
        boot: PathBuf,
        assets: Assets,
    }
    impl Fixture {
        pub(crate) fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let boot = directory.path().join("boot");
            let mut assets = Assets::new();
            for (name, bytes) in &assets.contents {
                let path = boot.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, bytes).unwrap();
            }
            fs::write(
                boot.join("spacewars-boot-manifest.json"),
                serde_json::to_vec(&assets.manifest).unwrap(),
            )
            .unwrap();
            fs::write(
                boot.join("spacewars-boot-id"),
                format!("{}\n", assets.manifest.identity()),
            )
            .unwrap();
            let source = directory.path().join("root");
            fs::create_dir_all(source.join("etc/spacewars")).unwrap();
            fs::create_dir_all(source.join("usr/bin")).unwrap();
            for (name, blob) in [
                ("engine-client", &mut assets.image.binaries.client),
                ("engine-os-manager", &mut assets.image.binaries.os_manager),
            ] {
                let bytes = format!("fixture binary {name}");
                fs::write(source.join("usr/bin").join(name), &bytes).unwrap();
                *blob = Blob {
                    bytes: bytes.len() as u64,
                    sha256: Sha256::digest(bytes.as_bytes()),
                };
            }
            fs::write(
                source.join("etc/spacewars/image.json"),
                serde_json::to_vec(&assets.image).unwrap(),
            )
            .unwrap();
            fs::write(
                source.join("etc/spacewars/bootstrap-runtime.json"),
                include_bytes!("../../../yocto/meta-spacewars/recipes-spacewars/spacewars/files/bootstrap-runtime.json"),
            )
            .unwrap();
            let mut legacy_paths = std::collections::BTreeSet::new();
            legacy_paths.insert("/".to_owned());
            for &(path, mode) in super::super::legacy_assets::FILES {
                let destination = source.join(path);
                fs::create_dir_all(destination.parent().unwrap()).unwrap();
                fs::write(&destination, format!("fixture legacy asset {path}\n")).unwrap();
                fs::set_permissions(&destination, fs::Permissions::from_mode(mode)).unwrap();
                legacy_paths.insert(format!("/{path}"));
                let mut parent = destination.parent().unwrap();
                while parent.starts_with(&source) {
                    fs::set_permissions(parent, fs::Permissions::from_mode(0o755)).unwrap();
                    legacy_paths.insert(format!(
                        "/{}",
                        parent.strip_prefix(&source).unwrap().display()
                    ));
                    if parent == source {
                        break;
                    }
                    parent = parent.parent().unwrap();
                }
            }
            let root = directory.path().join("fixture.ext4");
            let result = Command::new("/usr/sbin/mke2fs")
                .args([
                    "-q",
                    "-t",
                    "ext4",
                    "-b",
                    "1024",
                    "-E",
                    "lazy_itable_init=0,lazy_journal_init=0",
                    "-d",
                ])
                .arg(source)
                .arg(&root)
                .arg("16384")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            // Normalize only the fixture image's numeric ownership. No host
            // file is chowned and the tests still run as an ordinary user.
            let commands = directory.path().join("fixture-inode-owners");
            fs::write(
                &commands,
                legacy_paths
                    .iter()
                    .map(|path| {
                        format!("set_inode_field {path} uid 0\nset_inode_field {path} gid 0\n")
                    })
                    .collect::<String>(),
            )
            .unwrap();
            let result = Command::new("/usr/sbin/debugfs")
                .args(["-w", "-f"])
                .arg(&commands)
                .arg(&root)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            Self {
                directory,
                root,
                boot,
                assets,
            }
        }
        pub(crate) fn output(&self) -> PathBuf {
            self.directory.path().join("package")
        }
        pub(crate) fn build(&self) -> Result<BootstrapPackage> {
            build_package(
                &self.root,
                &self.boot,
                &self.output(),
                &request(Board::Pi5, "hyperpixel", Slot::A),
            )
        }
    }

    #[test]
    fn package_snapshots_real_ext4_content_and_does_not_replace_previous_output() {
        let fixture = Fixture::new();
        let original = fs::read(&fixture.root).unwrap();
        let package = fixture.build().unwrap();
        assert_eq!(package.schema, 3);
        assert_eq!(package.bootstrap_protocol, BOOTSTRAP_PROTOCOL);
        assert_eq!(package.purpose, "supervised-bootstrap");
        assert_eq!(package.image, fixture.assets.image);
        assert_eq!(package.expanded.sha256, Sha256::digest(&original));
        assert_eq!(fs::read(&fixture.root).unwrap(), original);
        let compressed = fs::read(fixture.output().join("rootfs.ext4.zst")).unwrap();
        assert_eq!(&compressed[..4], &[0x28, 0xb5, 0x2f, 0xfd]);
        assert_eq!(
            zstd::stream::decode_all(compressed.as_slice()).unwrap(),
            original
        );
        assert_eq!(package.compressed.sha256, Sha256::digest(&compressed));
        assert_eq!(
            fs::metadata(fixture.output()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for (path, expected) in &package.files {
            let bytes = fs::read(fixture.output().join(path)).unwrap();
            assert_eq!(expected.bytes, bytes.len() as u64, "{path}");
            assert_eq!(expected.sha256, Sha256::digest(&bytes), "{path}");
        }
        for &(path, _) in super::super::legacy_assets::FILES {
            let captured = fs::read(fixture.output().join(format!("legacy-root/{path}"))).unwrap();
            assert_eq!(
                captured,
                format!("fixture legacy asset {path}\n").as_bytes()
            );
        }
        assert!(!fixture.output().join("boot/hostname.txt").exists());
        assert_eq!(
            fs::read_to_string(fixture.output().join("boot/spacewars-trial.txt")).unwrap(),
            crate::boot::UNPREPARED_TRIAL
        );
        assert!(
            Command::new("sha256sum")
                .arg("--check")
                .arg("SHA256SUMS")
                .current_dir(fixture.output())
                .output()
                .unwrap()
                .status
                .success()
        );
        let record = fs::read(fixture.output().join("bundle.json")).unwrap();
        assert!(fixture.build().is_err());
        assert_eq!(
            fs::read(fixture.output().join("bundle.json")).unwrap(),
            record
        );
    }

    #[test]
    fn changed_assets_and_symlinks_leave_no_published_package() {
        let fixture = Fixture::new();
        let kernel = fixture.boot.join("kernel8.img");
        let bytes = fs::read(&kernel).unwrap();
        fs::write(&kernel, vec![b'x'; bytes.len()]).unwrap();
        assert!(fixture.build().is_err());
        assert!(!fixture.output().exists());
        fs::remove_file(&kernel).unwrap();
        symlink("kernel_2712.img", &kernel).unwrap();
        assert!(fixture.build().is_err());
        fs::remove_file(&kernel).unwrap();
        fs::write(&kernel, bytes).unwrap();
        let original_overlays = fixture.directory.path().join("outside-overlays");
        fs::rename(fixture.boot.join("overlays"), &original_overlays).unwrap();
        symlink(&original_overlays, fixture.boot.join("overlays")).unwrap();
        assert!(fixture.build().is_err());
        assert!(!fixture.output().exists());
        assert!(
            !fs::read_dir(fixture.directory.path())
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".sw-bootstrap-"))
        );
        let linked = fixture.directory.path().join("root-link");
        symlink(&fixture.root, &linked).unwrap();
        assert!(
            build_package(
                &linked,
                &fixture.boot,
                &fixture.output(),
                &request(Board::Pi5, "hyperpixel", Slot::A)
            )
            .is_err()
        );
        let error = build_package(
            std::path::Path::new("/dev/null"),
            &fixture.boot,
            &fixture.output(),
            &request(Board::Pi5, "hyperpixel", Slot::A),
        )
        .unwrap_err();
        assert!(error.to_string().contains("regular artifact file"));
    }

    #[test]
    fn root_header_and_packaged_executable_mismatch_are_rejected() {
        let fixture = Fixture::new();
        let original = fs::read(&fixture.root).unwrap();
        let mut changed = original.clone();
        changed[1024 + 58] = 2; // ext4 records filesystem errors.
        fs::write(&fixture.root, changed).unwrap();
        assert!(fixture.build().is_err());
        fs::write(&fixture.root, original).unwrap();
        let altered = fixture.directory.path().join("altered-client");
        fs::write(&altered, b"not the identified binary").unwrap();
        let result = Command::new("/usr/sbin/debugfs")
            .args(["-w", "-R", "rm /usr/bin/engine-client"])
            .arg(&fixture.root)
            .output()
            .unwrap();
        assert!(result.status.success());
        let result = Command::new("/usr/sbin/debugfs")
            .args([
                "-w",
                "-R",
                &format!("write {} /usr/bin/engine-client", altered.display()),
            ])
            .arg(&fixture.root)
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(fixture.build().is_err());
        assert!(!fixture.output().exists());
    }

    #[test]
    fn bootstrap_requires_the_supported_runtime_in_the_actual_ext4() {
        for marker in [
            None,
            Some(r#"{"schema":1,"protocol":0}"#),
            Some(r#"{"schema":1,"protocol":1}"#),
            Some(r#"{"schema":2,"protocol":2}"#),
            Some(r#"{"schema":1,"protocol":2,"bypass":true}"#),
            Some("not a runtime record"),
        ] {
            let fixture = Fixture::new();
            let mut commands = "rm /etc/spacewars/bootstrap-runtime.json\n".to_string();
            if let Some(contents) = marker {
                let path = fixture.directory.path().join("marker");
                fs::write(&path, contents).unwrap();
                commands.push_str(&format!(
                    "write {} /etc/spacewars/bootstrap-runtime.json\n",
                    path.display()
                ));
            }
            let script = fixture.directory.path().join("mutate-runtime");
            fs::write(&script, commands).unwrap();
            let result = Command::new("/usr/sbin/debugfs")
                .args(["-w", "-f"])
                .arg(&script)
                .arg(&fixture.root)
                .output()
                .unwrap();
            assert!(result.status.success());
            // A valid production identity alone does not qualify this runtime
            // for the first-image path, either during packaging or readback.
            let root = fs::File::open(&fixture.root).unwrap();
            let mut tools = crate::image_tools::ImageTools::default();
            tools.check(&root).unwrap();
            assert!(tools.check_bootstrap(&root).is_err(), "{marker:?}");
            assert!(fixture.build().is_err(), "{marker:?}");
            assert!(!fixture.output().exists());
        }
    }

    #[test]
    fn missing_linked_or_unsafe_legacy_sources_never_publish_a_package() {
        for commands in [
            "rm /usr/sbin/ab-update\n",
            "set_inode_field /usr/sbin/ab-update mode 0100777\n",
            "set_inode_field /usr/sbin/ab-update uid 1000\n",
            "set_inode_field /usr/sbin mode 040777\n",
            "rm /usr/sbin/ab-update\nsymlink /usr/sbin/ab-update /usr/sbin/ab-update-with-key\n",
            "rm /etc/ssh/sshd_config.d/10-spacewars-access.conf\nrmdir /etc/ssh/sshd_config.d\nsymlink /etc/ssh/sshd_config.d /usr/sbin\n",
        ] {
            let fixture = Fixture::new();
            let script = fixture.directory.path().join("mutate-fixture");
            fs::write(&script, commands).unwrap();
            let result = Command::new("/usr/sbin/debugfs")
                .args(["-w", "-f"])
                .arg(&script)
                .arg(&fixture.root)
                .output()
                .unwrap();
            assert!(result.status.success());
            assert!(fixture.build().is_err(), "{commands}");
            assert!(!fixture.output().exists());
        }
    }
}
