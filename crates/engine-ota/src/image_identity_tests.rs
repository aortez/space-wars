use super::*;
use crate::{Channel, ImageFormat};

const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/image-identity-v1.json");

fn descriptor(identity: &ImageIdentity) -> ImageDescriptor {
    ImageDescriptor {
        schema: 1,
        generation: 1,
        source_commit: identity.source.commit.clone(),
        build_id: identity.build_id.clone(),
        channel: Channel::Nightly,
        machine: identity.machine.clone(),
        architecture: identity.architecture.clone(),
        layout: identity.layout.clone(),
        format: ImageFormat::RootfsExt4Zstd,
        compressed: Blob {
            bytes: 64,
            sha256: Sha256::digest(b"compressed fixture"),
        },
        expanded: Blob {
            bytes: 1024,
            sha256: Sha256::digest(b"expanded fixture"),
        },
        boot_identity: identity.boot_identity,
        min_updater_protocol: 1,
        min_recovery_protocol: 1,
        data: identity.data,
    }
}

#[test]
fn python_generated_image_matches_descriptor_and_survives_channel_promotion() {
    let identity = ImageIdentity::parse(FIXTURE).unwrap();
    assert_eq!(identity.updater_protocol, UPDATER_PROTOCOL);
    assert_eq!(
        identity.recovery_protocol,
        crate::recovery::RECOVERY_PROTOCOL
    );
    let mut signed = descriptor(&identity);
    identity.matches_descriptor(&signed).unwrap();
    signed.channel = Channel::Release;
    identity.matches_descriptor(&signed).unwrap();
    signed.generation += 1;
    // Generation/payload uniqueness is enforced by the verifier's history;
    // a publisher allocates a generation after the image has been built.
    identity.matches_descriptor(&signed).unwrap();
    let encoded = serde_json::to_vec(&identity).unwrap();
    assert_eq!(ImageIdentity::parse(&encoded).unwrap(), identity);
}

#[test]
fn signed_metadata_cannot_relabel_packaged_content_or_compatibility() {
    let identity = ImageIdentity::parse(FIXTURE).unwrap();
    for field in 0..9 {
        let mut signed = descriptor(&identity);
        match field {
            0 => signed.source_commit = "c".repeat(40),
            1 => signed.build_id = "different-build".into(),
            2 => signed.machine = "different-machine".into(),
            3 => signed.architecture = "x86_64".into(),
            4 => signed.layout = "different-layout".into(),
            5 => signed.boot_identity = Sha256::digest(b"different boot"),
            6 => signed.data.reads.max += 1,
            7 => signed.min_updater_protocol += 1,
            8 => signed.min_recovery_protocol += 1,
            _ => unreachable!(),
        }
        assert!(
            identity.matches_descriptor(&signed).is_err(),
            "field {field}"
        );
    }
    let mut dirty = identity.clone();
    dirty.source.dirty = true;
    dirty.source.revision.push_str("-dirty");
    dirty.validate().unwrap(); // Available for supervised development diagnosis.
    assert!(dirty.matches_descriptor(&descriptor(&identity)).is_err());
}

#[test]
fn malformed_or_incoherent_image_records_are_rejected() {
    let baseline: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    for pointer in [
        "/schema",
        "/source/schema",
        "/updater_protocol",
        "/recovery_protocol",
        "/binaries/client/bytes",
        "/binaries/os_manager/bytes",
    ] {
        let mut value = baseline.clone();
        *value.pointer_mut(pointer).unwrap() = serde_json::json!(0);
        assert!(
            ImageIdentity::parse(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{pointer}"
        );
    }
    for (pointer, replacement) in [
        ("/source/commit", "unknown"),
        ("/source/revision", "mismatched-revision"),
        ("/build_id", "mismatched-tree"),
        ("/machine", "../escape"),
        ("/architecture", ""),
        ("/layout", "unsupported"),
        (
            "/boot_identity",
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
    ] {
        let mut value = baseline.clone();
        *value.pointer_mut(pointer).unwrap() = serde_json::json!(replacement);
        assert!(
            ImageIdentity::parse(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let mut value = baseline;
    value["unexpected"] = serde_json::json!(true);
    assert!(ImageIdentity::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(ImageIdentity::parse(&vec![b' '; MAX_IMAGE_IDENTITY_BYTES + 1]).is_err());
    assert!(ImageIdentity::parse(&FIXTURE[..FIXTURE.len() / 2]).is_err());
}
