use super::*;
use crate::SchemaRange;

const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/image-identity-v1.json");

fn identity() -> ImageIdentity {
    ImageIdentity::parse(FIXTURE).unwrap()
}

fn image(identity: &ImageIdentity) -> DevelopmentImage {
    DevelopmentImage::from_identity(
        identity,
        Blob {
            bytes: 64,
            sha256: Sha256::digest(b"compressed fixture"),
        },
        Blob {
            bytes: 1024,
            sha256: Sha256::digest(b"expanded fixture"),
        },
    )
}

fn device(identity: &ImageIdentity) -> DeviceIdentity {
    DeviceIdentity {
        machine: identity.machine.clone(),
        architecture: identity.architecture.clone(),
        layout: identity.layout.clone(),
        boot_identity: identity.boot_identity,
        updater_protocol: identity.updater_protocol,
        recovery_protocol: identity.recovery_protocol,
        inactive_slot_bytes: 4096,
        persistent_schema: 1,
        rollback_reads: SchemaRange { min: 1, max: 1 },
    }
}

#[test]
fn packed_descriptor_round_trips_and_matches_its_image() {
    let identity = identity();
    let image = image(&identity);
    let encoded = serde_json::to_vec(&image).unwrap();
    assert_eq!(DevelopmentImage::parse(&encoded).unwrap(), image);
    image.matches_identity(&identity).unwrap();
    image.check_device(&device(&identity)).unwrap();
    assert_eq!(image.candidate_id(), Sha256::digest(&encoded));
}

#[test]
fn dirty_sources_are_allowed_but_must_match() {
    let mut dirty = identity();
    dirty.source.dirty = true;
    dirty.source.revision.push_str("-dirty");
    let image = image(&dirty);
    assert!(image.source_dirty);
    image.matches_identity(&dirty).unwrap();
    assert!(image.matches_identity(&identity()).is_err());
}

#[test]
fn descriptor_cannot_relabel_the_written_image() {
    let identity = identity();
    for field in 0..10 {
        let mut claimed = image(&identity);
        match field {
            0 => claimed.source_commit = "c".repeat(40),
            1 => claimed.source_dirty = true,
            2 => claimed.build_id = "different-build".into(),
            3 => claimed.machine = "different-machine".into(),
            4 => claimed.architecture = "x86_64".into(),
            5 => claimed.boot_identity = Sha256::digest(b"different boot"),
            6 => claimed.data.reads.max += 1,
            7 => claimed.min_updater_protocol += 1,
            8 => claimed.min_recovery_protocol += 1,
            9 => claimed.data.writes = 2,
            _ => unreachable!(),
        }
        assert!(
            claimed.matches_identity(&identity).is_err(),
            "field {field}"
        );
    }
}

#[test]
fn units_refuse_images_built_for_other_boot_files_protocols_or_data() {
    let identity = identity();
    let image = image(&identity);
    for (field, reason) in [
        (0, "another machine"),
        (1, "boot files"),
        (2, "newer updater"),
        (3, "recovery protocol"),
        (4, "inactive slot"),
        (5, "persistent data"),
        (6, "persistent data"),
    ] {
        let mut unit = device(&identity);
        match field {
            0 => unit.machine = "other".into(),
            1 => unit.boot_identity = Sha256::digest(b"other boot"),
            2 => unit.updater_protocol = 0,
            3 => unit.recovery_protocol += 1,
            4 => unit.inactive_slot_bytes = 1023,
            5 => unit.persistent_schema = 2,
            6 => unit.rollback_reads = SchemaRange { min: 2, max: 2 },
            _ => unreachable!(),
        }
        let error = image.check_device(&unit).unwrap_err().to_string();
        assert!(error.contains(reason), "field {field}: {error}");
    }
}

#[test]
fn malformed_descriptors_are_rejected() {
    let baseline = serde_json::to_value(image(&identity())).unwrap();
    for (pointer, replacement) in [
        ("/schema", serde_json::json!(2)),
        ("/source_commit", serde_json::json!("unknown")),
        ("/build_id", serde_json::json!("")),
        ("/layout", serde_json::json!("other-layout")),
        ("/compressed/bytes", serde_json::json!(0)),
        ("/expanded/bytes", serde_json::json!(0)),
        (
            "/boot_identity",
            serde_json::json!("0000000000000000000000000000000000000000000000000000000000000000"),
        ),
        ("/min_recovery_protocol", serde_json::json!(0)),
        ("/data/writes", serde_json::json!(9)),
    ] {
        let mut value = baseline.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            DevelopmentImage::parse(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let mut value = baseline;
    value["generation"] = serde_json::json!(1);
    assert!(DevelopmentImage::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(DevelopmentImage::parse(&vec![b' '; MAX_DEVELOPMENT_BYTES + 1]).is_err());
}
