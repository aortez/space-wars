use super::*;
use serde_json::json;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::process::Command;

struct Fixture {
    directory: tempfile::TempDir,
    tree: PathBuf,
    root: PathBuf,
    image: ImageIdentity,
    policy: Policy,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let tree = directory.path().join("tree");
        let root = directory.path().join("root.ext4");
        let mut image =
            ImageIdentity::parse(include_bytes!("../tests/fixtures/image-identity-v1.json"))
                .unwrap();
        let manifest_bytes = include_bytes!("../tests/fixtures/boot-manifest-v2.json");
        let manifest: BootManifest = serde_json::from_slice(manifest_bytes).unwrap();
        image.boot_identity = manifest.identity();
        let inputs = serde_json::to_vec(&json!({
            "schema": 1, "source_commit": image.source.commit, "kas_version": "5.4",
            "machine": "raspberrypi-spacewars", "target": "spacewars-image",
            "layers": (["meta-openembedded", "meta-pi-base", "meta-raspberrypi", "meta-rust-bin", "poky"]
                .into_iter().map(|name| (name, json!({"url": "https://example.invalid/fixture", "commit": "b".repeat(40)}))).collect::<BTreeMap<_, _>>()),
            "local_layer": {"path": "yocto/meta-spacewars", "source_commit": image.source.commit},
            "configuration": (["Cargo.lock", "yocto/kas-spacewars.yml", "yocto/meta-spacewars/conf/machine/raspberrypi-spacewars.conf"]
                .into_iter().map(|name| (name, blob(b"fixture config"))).collect::<BTreeMap<_, _>>()),
            "expanded_kas_sha256": Sha256::digest(b"fixture kas config"),
        })).unwrap();
        let policy = Policy {
            schema: 1,
            source_commit: image.source.commit.clone(),
            build_inputs_sha256: Sha256::digest(&inputs),
            rust_version: "1.94.0".into(),
            cargo_version: "1.94.0".into(),
            cc_version_line: "aarch64-poky-linux-gcc (GCC) 13.5.0".into(),
        };
        let mut fixture = Self {
            directory,
            tree,
            root,
            image,
            policy,
        };
        fixture.write(
            "usr/share/spacewars/source.json",
            &serde_json::to_vec(&fixture.image.source).unwrap(),
            0o644,
        );
        fixture.write("usr/share/spacewars/build-inputs.json", &inputs, 0o644);
        fixture.write("usr/share/spacewars/build.json", &serde_json::to_vec(&json!({
            "schema": 1, "source": fixture.image.source, "rust_target": "aarch64-unknown-linux-gnu",
            "profile": "release", "kiosk_feature": "pi-kiosk-gpu",
            "tools": {
                "rustc": {"version": "1.94.0", "details": "rustc 1.94.0 (fixture)\nrelease: 1.94.0"},
                "cargo": {"version": "1.94.0", "details": "cargo 1.94.0 (fixture)"},
                "cc": {"details": fixture.policy.cc_version_line},
            }
        })).unwrap(), 0o644);
        fixture.write("etc/spacewars/boot-manifest.json", manifest_bytes, 0o644);
        fixture.write(
            "etc/spacewars/boot-id",
            format!("{}\n", fixture.image.boot_identity).as_bytes(),
            0o644,
        );
        fixture.write("etc/spacewars/bootstrap-runtime.json", include_bytes!("../../../yocto/meta-spacewars/recipes-spacewars/spacewars/files/bootstrap-runtime.json"), 0o644);
        // Only a parser fixture, not runnable AArch64 program or boot evidence.
        let mut elf = vec![0u8; 64];
        elf[..6].copy_from_slice(b"\x7fELF\x02\x01");
        elf[18..20].copy_from_slice(&183u16.to_le_bytes());
        fixture.write("usr/bin/engine-client", &elf, 0o755);
        fixture.write("usr/bin/engine-os-manager", &elf, 0o755);
        fixture.image.binaries.client = blob(&elf);
        fixture.image.binaries.os_manager = blob(&elf);
        fixture.identity();
        let mut directories = vec![fixture.tree.clone()];
        while let Some(directory) = directories.pop() {
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
            for entry in fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    directories.push(entry.path());
                }
            }
        }
        fixture
    }

    fn write(&self, name: &str, bytes: &[u8], mode: u32) {
        let path = self.tree.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn identity(&self) {
        self.write(
            "etc/spacewars/image.json",
            &serde_json::to_vec(&self.image).unwrap(),
            0o644,
        );
    }

    fn build(&self) {
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
            .arg(&self.tree)
            .arg(&self.root)
            .arg("16384")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        // Numeric ownership is changed in the private ext4 fixture only. Tests
        // never chown host files, mount an image or need root.
        fn owners(tree: &Path, here: &Path, text: &mut String) {
            let path = format!("/{}", here.strip_prefix(tree).unwrap().display());
            text.push_str(&format!(
                "set_inode_field {path} uid 0\nset_inode_field {path} gid 0\n"
            ));
            if here.symlink_metadata().unwrap().is_dir() {
                for entry in fs::read_dir(here).unwrap() {
                    owners(tree, &entry.unwrap().path(), text);
                }
            }
        }
        let mut commands = String::new();
        owners(&self.tree, &self.tree, &mut commands);
        let path = self.directory.path().join("owners.txt");
        fs::write(&path, commands).unwrap();
        let result = Command::new("/usr/sbin/debugfs")
            .args(["-w", "-f"])
            .arg(path)
            .arg(&self.root)
            .output()
            .unwrap();
        assert!(result.status.success());
    }

    fn inspect(&self) -> Result<Qualification> {
        inspect(&mut regular(&self.root)?, &self.policy)
    }
}

fn blob(bytes: &[u8]) -> Blob {
    Blob {
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(bytes),
    }
}

#[test]
fn real_ext4_roundtrip_uses_installer_decoder_and_preserves_existing_output() {
    let fixture = Fixture::new();
    fixture.build();
    let output = fixture.directory.path().join("candidate");
    let candidate = pack(&fixture.policy, &fixture.root, &output).unwrap();
    assert_eq!(
        candidate.expanded.bytes,
        fs::metadata(&fixture.root).unwrap().len()
    );
    let parsed = Candidate::parse(&fs::read(output.join("candidate.json")).unwrap()).unwrap();
    let result = verify(
        &fixture.policy,
        &parsed,
        &output.join("image.ext4.zst"),
        fixture.directory.path(),
    )
    .unwrap();
    assert_eq!(result.image(), &fixture.image);
    assert_eq!(result.expanded, candidate.expanded);
    assert_eq!(fs::read_dir(&output).unwrap().count(), 3);
    let original = fs::read(output.join("candidate.json")).unwrap();
    assert!(pack(&fixture.policy, &fixture.root, &output).is_err());
    assert_eq!(fs::read(output.join("candidate.json")).unwrap(), original);
    assert!(fs::read_dir(fixture.directory.path()).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".ota-candidate-")
    }));
}

#[test]
fn independently_selected_source_inputs_and_toolchain_must_match() {
    let mut fixture = Fixture::new();
    fixture.build();
    let original = fixture.policy.clone();
    for change in 0..5 {
        fixture.policy = original.clone();
        match change {
            0 => fixture.policy.source_commit = "b".repeat(40),
            1 => fixture.policy.build_inputs_sha256 = Sha256::digest(b"different approved inputs"),
            2 => fixture.policy.rust_version = "1.89.0".into(),
            3 => fixture.policy.cargo_version = "1.89.0".into(),
            _ => fixture.policy.cc_version_line = "unexpected GCC".into(),
        }
        assert!(fixture.inspect().is_err(), "policy change {change}");
    }
}

#[test]
fn missing_provenance_and_mismatched_boot_or_binary_bytes_reject() {
    for change in 0..6 {
        let mut fixture = Fixture::new();
        match change {
            0 => {
                fs::remove_file(fixture.tree.join("usr/share/spacewars/build-inputs.json")).unwrap()
            }
            1 => fixture.write("usr/share/spacewars/source.json", b"{}", 0o644),
            2 => fixture.write("etc/spacewars/boot-id", b"wrong\n", 0o644),
            3 => fixture.write(
                "etc/spacewars/bootstrap-runtime.json",
                b"{\"schema\":1,\"protocol\":1}",
                0o644,
            ),
            4 => fixture.write("usr/bin/engine-client", b"replaced executable", 0o755),
            _ => {
                let path = fixture.tree.join("usr/bin/engine-client");
                let mut bytes = fs::read(path).unwrap();
                bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
                fixture.write("usr/bin/engine-client", &bytes, 0o755);
                fixture.image.binaries.client = blob(&bytes);
                fixture.identity();
            }
        }
        fixture.build();
        assert!(fixture.inspect().is_err(), "image change {change}");
    }
}

#[test]
fn unsafe_image_modes_owners_links_and_parent_directories_reject() {
    for change in 0..6 {
        let fixture = Fixture::new();
        let provenance = fixture.tree.join("usr/share/spacewars/build.json");
        match change {
            0 => fs::set_permissions(&provenance, fs::Permissions::from_mode(0o666)).unwrap(),
            1 => fs::set_permissions(
                fixture.tree.join("usr/bin/engine-client"),
                fs::Permissions::from_mode(0o644),
            )
            .unwrap(),
            2 => fs::set_permissions(
                fixture.tree.join("usr/share"),
                fs::Permissions::from_mode(0o777),
            )
            .unwrap(),
            3 => {
                fs::rename(&provenance, provenance.with_extension("other")).unwrap();
                symlink("build.other", &provenance).unwrap();
            }
            4 => fs::hard_link(&provenance, provenance.with_extension("other")).unwrap(),
            _ => {}
        }
        fixture.build();
        if change == 5 {
            let result = Command::new("/usr/sbin/debugfs")
                .args([
                    "-w",
                    "-R",
                    "set_inode_field /usr/share/spacewars/build.json uid 1000",
                ])
                .arg(&fixture.root)
                .output()
                .unwrap();
            assert!(result.status.success());
        }
        assert!(fixture.inspect().is_err(), "metadata change {change}");
    }
}

#[test]
fn corrupted_trailing_concatenated_or_misdescribed_payloads_reject() {
    let fixture = Fixture::new();
    fixture.build();
    let output = fixture.directory.path().join("candidate");
    let candidate = pack(&fixture.policy, &fixture.root, &output).unwrap();
    let bytes = fs::read(output.join("image.ext4.zst")).unwrap();
    let altered = fixture.directory.path().join("altered.zst");
    for change in 0..5 {
        let mut current = candidate.clone();
        let mut bad = bytes.clone();
        match change {
            0 => bad[0] ^= 1,
            1 => bad.extend_from_slice(b"trailing bytes"),
            2 => bad.extend_from_slice(&bytes),
            3 => current.expanded.sha256 = Sha256::digest(b"wrong expanded hash"),
            _ => current.expanded.bytes -= 1024,
        }
        current.compressed = blob(&bad);
        fs::write(&altered, &bad).unwrap();
        assert!(
            verify(
                &fixture.policy,
                &current,
                &altered,
                fixture.directory.path()
            )
            .is_err(),
            "payload change {change}"
        );
    }
    // Correct compressed hash cannot conceal an invalid ext4 superblock.
    let mut raw = fs::read(&fixture.root).unwrap();
    raw[1080] = 0;
    let bad = zstd::stream::encode_all(raw.as_slice(), 3).unwrap();
    fs::write(&altered, &bad).unwrap();
    let current = Candidate {
        compressed: blob(&bad),
        expanded: blob(&raw),
        ..candidate
    };
    assert!(
        verify(
            &fixture.policy,
            &current,
            &altered,
            fixture.directory.path()
        )
        .is_err()
    );
}

#[test]
fn bounded_records_and_nonregular_host_inputs_reject() {
    let fixture = Fixture::new();
    let mut policy = serde_json::to_value(&fixture.policy).unwrap();
    policy["run"] = json!("untrusted command");
    assert!(Policy::parse(&serde_json::to_vec(&policy).unwrap()).is_err());
    assert!(Policy::parse(&vec![b' '; MAX_POLICY_BYTES + 1]).is_err());
    for size in [0, MAX_ROOT_BYTES + 1] {
        let candidate = Candidate {
            schema: 1,
            format: ImageFormat::RootfsExt4Zstd,
            compressed: blob(b"compressed"),
            expanded: Blob {
                bytes: size,
                sha256: Sha256::digest(b"root"),
            },
        };
        assert!(Candidate::parse(&serde_json::to_vec(&candidate).unwrap()).is_err());
    }
    let link = fixture.directory.path().join("link");
    symlink(fixture.tree.join("usr/share/spacewars/source.json"), &link).unwrap();
    assert!(regular(&link).is_err());
    assert!(regular(&fixture.tree).is_err());
    let fifo = fixture.directory.path().join("fifo");
    let name = CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: name is a valid path in this test's private temporary directory.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(regular(&fifo).is_err());
}
