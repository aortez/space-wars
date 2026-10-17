use super::*;
use crate::test_support::{self as support, Fault, Fixture};
use std::sync::atomic::{AtomicUsize, Ordering};

async fn context(images: &[ImageDescriptor]) -> (Fixture, support::MemoryTransport, Authority) {
    let fixture = Fixture::new().await;
    let feed = fixture
        .publish(1, images, support::EXPIRES.parse().unwrap(), Fault::None)
        .await;
    let authority = Authority::load(
        &fixture.root_bytes,
        &feed.files["1.targets.json"],
        support::time().unwrap(),
    )
    .unwrap();
    (fixture, feed, authority)
}
fn request(change: Change) -> Request {
    Request {
        prepared_at: support::NOW.parse().unwrap(),
        version: None,
        change,
    }
}
fn text(feed: &support::MemoryTransport, name: &str) -> String {
    String::from_utf8(feed.files[name].clone()).unwrap()
}

#[tokio::test]
async fn reserved_versions_skip_abandoned_numbers_and_bind_every_role_proposal() {
    let (_, feed, authority) = context(&[]).await;
    let changes = [
        Change::Channel {
            channel: Channel::Nightly,
            previous: Some(text(&feed, "1.nightly.json")),
            image: None,
        },
        Change::Channel {
            channel: Channel::Release,
            previous: Some(text(&feed, "1.release.json")),
            image: None,
        },
        Change::TargetsRenewal {},
        Change::Snapshot {
            previous: Some(text(&feed, "1.snapshot.json")),
            targets: text(&feed, "1.targets.json"),
            release: text(&feed, "1.release.json"),
            nightly: text(&feed, "1.nightly.json"),
        },
        Change::Timestamp {
            previous: Some(text(&feed, "timestamp.json")),
            snapshot: text(&feed, "1.snapshot.json"),
        },
    ];
    for change in changes {
        let mut selected = request(change);
        selected.version = NonZeroU64::new(9);
        let proposal = authority
            .prepare(selected, support::time().unwrap())
            .unwrap();
        let approval: serde_json::Value =
            serde_json::from_slice(proposal.approval_bytes()).unwrap();
        assert_eq!(approval["signed"]["version"], 9);
    }
    let ordinary = authority
        .prepare(request(Change::TargetsRenewal {}), support::time().unwrap())
        .unwrap();
    let mut selected = request(Change::TargetsRenewal {});
    selected.version = NonZeroU64::new(9);
    let reserved = authority
        .prepare(selected, support::time().unwrap())
        .unwrap();
    assert_ne!(ordinary.digest(), reserved.digest());
    let mut selected = request(Change::TargetsRenewal {});
    selected.version = NonZeroU64::new(1);
    assert!(
        authority
            .prepare(selected, support::time().unwrap())
            .is_err()
    );
}

#[test]
fn version_reservations_reject_zero_rollback_and_exhaustion() {
    assert_eq!(next_version(None, None).unwrap().get(), 1);
    assert_eq!(
        next_version(NonZeroU64::new(4), NonZeroU64::new(9))
            .unwrap()
            .get(),
        9
    );
    assert!(next_version(NonZeroU64::new(4), NonZeroU64::new(4)).is_err());
    assert!(next_version(NonZeroU64::new(u64::MAX), None).is_err());
    assert!(next_version(NonZeroU64::new(u64::MAX), NonZeroU64::new(u64::MAX)).is_err());
    for version in ["0", "18446744073709551616"] {
        let raw = format!(
            r#"{{"prepared_at":"{}","version":{},"change":{{"role":"targets-renewal"}}}}"#,
            support::NOW,
            version
        );
        assert!(Request::parse(raw.as_bytes()).is_err());
    }
}

#[tokio::test]
async fn returned_signature_must_match_exact_approved_body_and_authority() {
    let (fixture, feed, authority) = context(&[]).await;
    let proposal = authority
        .prepare(
            request(Change::Channel {
                channel: Channel::Nightly,
                previous: Some(text(&feed, "1.nightly.json")),
                image: None,
            }),
            support::time().unwrap(),
        )
        .unwrap();
    let signed = authority
        .sign(
            &proposal,
            proposal.digest(),
            &fixture.signing_keys(&[6]),
            support::time().unwrap(),
        )
        .await
        .unwrap();
    authority
        .verify_signed(
            &proposal,
            proposal.digest(),
            &signed,
            support::time().unwrap(),
        )
        .unwrap();
    assert!(
        authority
            .verify_signed(
                &proposal,
                Sha256::digest(b"other approval"),
                &signed,
                support::time().unwrap()
            )
            .is_err()
    );
    let mut changed: serde_json::Value = serde_json::from_slice(&signed).unwrap();
    changed["signed"]["version"] = 3.into();
    assert!(
        authority
            .verify_signed(
                &proposal,
                proposal.digest(),
                &serde_json::to_vec(&changed).unwrap(),
                support::time().unwrap()
            )
            .is_err()
    );
    let mut changed: serde_json::Value = serde_json::from_slice(&signed).unwrap();
    changed["signatures"][0]["sig"] = "00".repeat(64).into();
    assert!(
        authority
            .verify_signed(
                &proposal,
                proposal.digest(),
                &serde_json::to_vec(&changed).unwrap(),
                support::time().unwrap()
            )
            .is_err()
    );
    let (_, _, other) = context(&[]).await;
    assert!(
        other
            .verify_signed(
                &proposal,
                proposal.digest(),
                &signed,
                support::time().unwrap()
            )
            .is_err()
    );
    let later = UsableTime(after(support::time().unwrap().0, DAY + 1).unwrap());
    authority
        .verify_signed(&proposal, proposal.digest(), &signed, later)
        .unwrap();
}

#[tokio::test]
async fn retained_response_verification_does_not_reopen_signing_or_extend_expiry() {
    let (fixture, feed, authority) = context(&[]).await;
    let selected = || {
        request(Change::Timestamp {
            previous: Some(text(&feed, "timestamp.json")),
            snapshot: text(&feed, "1.snapshot.json"),
        })
    };
    let now = support::time().unwrap();
    let proposal = authority.prepare(selected(), now).unwrap();
    let signed = authority
        .sign(
            &proposal,
            proposal.digest(),
            &fixture.signing_keys(&[3]),
            now,
        )
        .await
        .unwrap();
    let later = UsableTime(after(now.0, DAY + 1).unwrap());
    let retained = authority.prepare_verification(selected(), later).unwrap();
    assert_eq!(retained.digest(), proposal.digest());
    authority
        .verify_signed(&retained, proposal.digest(), &signed, later)
        .unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    let keys: Vec<Box<dyn KeySource>> = fixture
        .signing_keys(&[3])
        .into_iter()
        .map(|inner| {
            Box::new(Counted {
                reads: reads.clone(),
                inner,
            }) as Box<dyn KeySource>
        })
        .collect();
    assert!(
        authority
            .sign(&retained, proposal.digest(), &keys, later)
            .await
            .is_err()
    );
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    let expired = UsableTime(after(now.0, 7 * DAY).unwrap());
    assert!(authority.prepare_verification(selected(), expired).is_err());
    assert!(
        authority
            .verify_signed(&retained, proposal.digest(), &signed, expired)
            .is_err()
    );
    let before = UsableTime(
        now.0
            .checked_sub(jiff::SignedDuration::from_secs(1))
            .unwrap(),
    );
    assert!(authority.prepare_verification(selected(), before).is_err());
}

#[derive(Debug)]
struct Counted {
    reads: Arc<AtomicUsize>,
    inner: Box<dyn KeySource>,
}
#[async_trait]
impl KeySource for Counted {
    async fn as_sign(
        &self,
    ) -> std::result::Result<Box<dyn Sign>, Box<dyn std::error::Error + Send + Sync>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.as_sign().await
    }
    async fn write(
        &self,
        _: &str,
        _: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        panic!("read-only")
    }
}

#[tokio::test]
async fn approval_authority_age_and_substitution_fail_before_reading_credentials() {
    let (fixture, feed, authority) = context(&[]).await;
    let reads = Arc::new(AtomicUsize::new(0));
    let keys: Vec<Box<dyn KeySource>> = fixture
        .signing_keys(&[6])
        .into_iter()
        .map(|inner| {
            Box::new(Counted {
                reads: reads.clone(),
                inner,
            }) as Box<dyn KeySource>
        })
        .collect();
    let proposal = authority
        .prepare(
            request(Change::Channel {
                channel: Channel::Nightly,
                previous: Some(text(&feed, "1.nightly.json")),
                image: None,
            }),
            support::time().unwrap(),
        )
        .unwrap();
    assert!(
        authority
            .sign(
                &proposal,
                Sha256::digest(b"unapproved"),
                &keys,
                support::time().unwrap()
            )
            .await
            .is_err()
    );
    let changed = authority
        .prepare(
            request(Change::Channel {
                channel: Channel::Nightly,
                previous: Some(text(&feed, "1.nightly.json")),
                image: Some(Box::new(support::descriptor(Channel::Nightly, 1))),
            }),
            support::time().unwrap(),
        )
        .unwrap();
    assert!(
        authority
            .sign(&changed, proposal.digest(), &keys, support::time().unwrap())
            .await
            .is_err()
    );
    let (_, _, other) = context(&[]).await;
    assert!(
        other
            .sign(
                &proposal,
                proposal.digest(),
                &keys,
                support::time().unwrap()
            )
            .await
            .is_err()
    );
    let later = UsableTime(after(support::time().unwrap().0, DAY + 1).unwrap());
    assert!(
        authority
            .sign(&proposal, proposal.digest(), &keys, later)
            .await
            .is_err()
    );
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    authority
        .sign(
            &proposal,
            proposal.digest(),
            &keys,
            support::time().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        reads.load(Ordering::SeqCst),
        1,
        "keys are loaded exactly once"
    );
}

#[tokio::test]
async fn nightly_credentials_cannot_sign_release_or_any_top_level_role() {
    let (fixture, feed, authority) = context(&[]).await;
    let changes = [
        Change::Channel {
            channel: Channel::Release,
            previous: Some(text(&feed, "1.release.json")),
            image: None,
        },
        Change::TargetsRenewal {},
        Change::Snapshot {
            previous: Some(text(&feed, "1.snapshot.json")),
            targets: text(&feed, "1.targets.json"),
            release: text(&feed, "1.release.json"),
            nightly: text(&feed, "1.nightly.json"),
        },
        Change::Timestamp {
            previous: Some(text(&feed, "timestamp.json")),
            snapshot: text(&feed, "1.snapshot.json"),
        },
    ];
    for change in changes {
        let proposal = authority
            .prepare(request(change), support::time().unwrap())
            .unwrap();
        assert!(
            authority
                .sign(
                    &proposal,
                    proposal.digest(),
                    &fixture.signing_keys(&[6]),
                    support::time().unwrap()
                )
                .await
                .is_err()
        );
    }
    let proposal = authority
        .prepare(
            request(Change::Channel {
                channel: Channel::Nightly,
                previous: None,
                image: None,
            }),
            support::time().unwrap(),
        )
        .unwrap();
    for ids in [&[0][..], &[1], &[2], &[3], &[4], &[5], &[6, 0], &[6, 6]] {
        assert!(
            authority
                .sign(
                    &proposal,
                    proposal.digest(),
                    &fixture.signing_keys(ids),
                    support::time().unwrap()
                )
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn release_threshold_rejects_missing_duplicate_and_foreign_keys() {
    let (fixture, _, authority) = context(&[]).await;
    let proposal = authority
        .prepare(
            request(Change::Channel {
                channel: Channel::Release,
                previous: None,
                image: Some(Box::new(support::descriptor(Channel::Release, 1))),
            }),
            support::time().unwrap(),
        )
        .unwrap();
    for keys in [&[4][..], &[4, 4], &[4, 6], &[4, 5, 6]] {
        assert!(
            authority
                .sign(
                    &proposal,
                    proposal.digest(),
                    &fixture.signing_keys(keys),
                    support::time().unwrap()
                )
                .await
                .is_err()
        );
    }
    let bytes = authority
        .sign(
            &proposal,
            proposal.digest(),
            &fixture.signing_keys(&[4, 5]),
            support::time().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        parse::<Signed<Targets>>(&bytes).unwrap().signatures.len(),
        2
    );
    for _ in 0..4 {
        let repeated = authority
            .sign(
                &proposal,
                proposal.digest(),
                &fixture.signing_keys(&[5, 4]),
                support::time().unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            bytes, repeated,
            "Ed25519 output is independent of key or map iteration order"
        );
    }
}

#[tokio::test]
async fn channel_updates_preserve_existing_compatibility_candidates_and_identity() {
    let old = support::descriptor(Channel::Nightly, 1);
    let (fixture, feed, authority) = context(std::slice::from_ref(&old)).await;
    let mut newer = support::descriptor(Channel::Nightly, 2);
    newer.boot_identity = Sha256::digest(b"new boot generation");
    let proposal = authority
        .prepare(
            request(Change::Channel {
                channel: Channel::Nightly,
                previous: Some(text(&feed, "1.nightly.json")),
                image: Some(Box::new(newer.clone())),
            }),
            support::time().unwrap(),
        )
        .unwrap();
    let bytes = authority
        .sign(
            &proposal,
            proposal.digest(),
            &fixture.signing_keys(&[6]),
            support::time().unwrap(),
        )
        .await
        .unwrap();
    let targets = authority.channel_role(Channel::Nightly, &bytes).unwrap();
    assert_eq!(targets.targets.len(), 2);
    assert_eq!(targets.version.get(), 2);
    assert_eq!(
        targets.expires,
        after(support::time().unwrap().0, 30 * DAY).unwrap()
    );
    for mut conflict in [old.clone(), newer] {
        conflict.generation = 1;
        conflict.min_updater_protocol += 1;
        assert!(
            authority
                .prepare(
                    request(Change::Channel {
                        channel: Channel::Nightly,
                        previous: Some(text(&feed, "1.nightly.json")),
                        image: Some(Box::new(conflict))
                    }),
                    support::time().unwrap()
                )
                .is_err()
        );
    }
    assert!(
        authority
            .prepare(
                request(Change::Channel {
                    channel: Channel::Release,
                    previous: None,
                    image: Some(Box::new(old))
                }),
                support::time().unwrap()
            )
            .is_err()
    );
}

#[tokio::test]
async fn independently_signed_roles_form_a_feed_accepted_by_real_clients_and_delivery_inventory() {
    let nightly = support::descriptor(Channel::Nightly, 2);
    let (fixture, mut feed, authority) = context(std::slice::from_ref(&nightly)).await;
    let mut release = nightly.clone();
    release.channel = Channel::Release;
    let mut roles = HashMap::new();
    for (name, change, keys) in [
        ("targets", Change::TargetsRenewal {}, vec![1]),
        (
            "release",
            Change::Channel {
                channel: Channel::Release,
                previous: Some(text(&feed, "1.release.json")),
                image: Some(Box::new(release.clone())),
            },
            vec![4, 5],
        ),
        (
            "nightly",
            Change::Channel {
                channel: Channel::Nightly,
                previous: Some(text(&feed, "1.nightly.json")),
                image: None,
            },
            vec![6],
        ),
    ] {
        let proposal = authority
            .prepare(request(change), support::time().unwrap())
            .unwrap();
        let bytes = authority
            .sign(
                &proposal,
                proposal.digest(),
                &fixture.signing_keys(&keys),
                support::time().unwrap(),
            )
            .await
            .unwrap();
        roles.insert(name, String::from_utf8(bytes.clone()).unwrap());
        feed.files.remove(&format!("1.{name}.json"));
        feed.files.insert(format!("2.{name}.json"), bytes);
    }
    let snapshot = authority
        .prepare(
            request(Change::Snapshot {
                previous: Some(text(&feed, "1.snapshot.json")),
                targets: roles["targets"].clone(),
                release: roles["release"].clone(),
                nightly: roles["nightly"].clone(),
            }),
            support::time().unwrap(),
        )
        .unwrap();
    let bytes = authority
        .sign(
            &snapshot,
            snapshot.digest(),
            &fixture.signing_keys(&[2]),
            support::time().unwrap(),
        )
        .await
        .unwrap();
    let timestamp = authority
        .prepare(
            request(Change::Timestamp {
                previous: Some(text(&feed, "timestamp.json")),
                snapshot: String::from_utf8(bytes.clone()).unwrap(),
            }),
            support::time().unwrap(),
        )
        .unwrap();
    feed.files.remove("1.snapshot.json");
    feed.files.insert("2.snapshot.json".into(), bytes);
    let bytes = authority
        .sign(
            &timestamp,
            timestamp.digest(),
            &fixture.signing_keys(&[3]),
            support::time().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        parse::<Signed<tough::schema::Timestamp>>(&bytes)
            .unwrap()
            .signed
            .expires,
        after(support::time().unwrap().0, 7 * DAY).unwrap()
    );
    feed.files.insert("timestamp.json".into(), bytes);
    for channel in [Channel::Release, Channel::Nightly] {
        let home = tempfile::tempdir().unwrap();
        let mut store = crate::TrustStore::initialize(
            &home.path().join("trust"),
            &fixture.root_bytes,
            support::location(),
        )
        .unwrap();
        let selected = store
            .refresh(
                Arc::new(feed.clone()),
                support::time(),
                channel,
                &support::device(),
                &crate::ImageHistory::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            support::candidate(selected).image().compressed,
            nightly.compressed
        );
    }
    let directory = tempfile::tempdir().unwrap();
    for (name, bytes) in &feed.files {
        std::fs::write(directory.path().join(name), bytes).unwrap();
    }
    for image in [nightly, release] {
        std::fs::write(
            directory.path().join(format!(
                "{}.{}",
                image.compressed.sha256,
                image.target_name()
            )),
            support::payload(image.generation),
        )
        .unwrap();
    }
    crate::publication::inventory(
        &fixture.root_bytes,
        directory.path(),
        support::time().unwrap(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn proposal_serialization_is_stable_and_unknown_operations_are_rejected() {
    let (_, feed, authority) = context(&[]).await;
    let first = authority
        .prepare(
            request(Change::Timestamp {
                previous: Some(text(&feed, "timestamp.json")),
                snapshot: text(&feed, "1.snapshot.json"),
            }),
            support::time().unwrap(),
        )
        .unwrap();
    let second = authority
        .prepare(
            request(Change::Timestamp {
                previous: Some(text(&feed, "timestamp.json")),
                snapshot: text(&feed, "1.snapshot.json"),
            }),
            support::time().unwrap(),
        )
        .unwrap();
    assert_eq!(first.approval_bytes(), second.approval_bytes());
    for change in [
        serde_json::json!({"role":"root"}),
        serde_json::json!({"role":"targets-renewal", "keys":[]}),
        serde_json::json!({"role":"channel", "channel":"nightly", "previous":null, "image":null, "script":"command"}),
    ] {
        assert!(
            Request::parse(
                &serde_json::to_vec(
                    &serde_json::json!({"prepared_at":support::NOW, "change":change})
                )
                .unwrap()
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn snapshot_and_timestamp_parents_reject_rollbacks_and_same_version_substitution() {
    let (fixture, first, authority) = context(&[]).await;
    let second = fixture
        .publish(2, &[], support::EXPIRES.parse().unwrap(), Fault::None)
        .await;
    assert!(
        authority
            .prepare(
                request(Change::Snapshot {
                    previous: Some(text(&second, "2.snapshot.json")),
                    targets: text(&first, "1.targets.json"),
                    release: text(&first, "1.release.json"),
                    nightly: text(&first, "1.nightly.json"),
                }),
                support::time().unwrap()
            )
            .is_err()
    );
    assert!(
        authority
            .prepare(
                request(Change::Timestamp {
                    previous: Some(text(&second, "timestamp.json")),
                    snapshot: text(&first, "1.snapshot.json"),
                }),
                support::time().unwrap()
            )
            .is_err()
    );
    let different = authority
        .prepare(
            request(Change::Channel {
                channel: Channel::Nightly,
                previous: Some(text(&first, "1.nightly.json")),
                image: None,
            }),
            support::time().unwrap(),
        )
        .unwrap();
    let bytes = authority
        .sign(
            &different,
            different.digest(),
            &fixture.signing_keys(&[6]),
            support::time().unwrap(),
        )
        .await
        .unwrap();
    assert!(
        authority
            .prepare(
                request(Change::Snapshot {
                    previous: Some(text(&second, "2.snapshot.json")),
                    targets: text(&second, "2.targets.json"),
                    release: text(&second, "2.release.json"),
                    nightly: String::from_utf8(bytes).unwrap(),
                }),
                support::time().unwrap()
            )
            .is_err()
    );
    let different = authority
        .prepare(
            request(Change::Snapshot {
                previous: Some(text(&first, "1.snapshot.json")),
                targets: text(&second, "2.targets.json"),
                release: text(&second, "2.release.json"),
                nightly: text(&second, "2.nightly.json"),
            }),
            support::time().unwrap(),
        )
        .unwrap();
    let bytes = authority
        .sign(
            &different,
            different.digest(),
            &fixture.signing_keys(&[2]),
            support::time().unwrap(),
        )
        .await
        .unwrap();
    assert!(
        authority
            .prepare(
                request(Change::Timestamp {
                    previous: Some(text(&second, "timestamp.json")),
                    snapshot: String::from_utf8(bytes).unwrap(),
                }),
                support::time().unwrap()
            )
            .is_err()
    );
}

#[tokio::test]
async fn invalid_expired_or_changed_policy_cannot_authorize_signing() {
    let (fixture, feed, authority) = context(&[]).await;
    let expired = UsableTime("2026-10-22T00:00:00Z".parse().unwrap());
    assert!(Authority::load(&fixture.root_bytes, &feed.files["1.targets.json"], expired).is_err());
    let mut corrupt: serde_json::Value =
        serde_json::from_slice(&feed.files["1.targets.json"]).unwrap();
    corrupt["signatures"] = serde_json::json!([]);
    assert!(
        Authority::load(
            &fixture.root_bytes,
            &serde_json::to_vec(&corrupt).unwrap(),
            support::time().unwrap()
        )
        .is_err()
    );
    let changed = fixture
        .publish(
            2,
            &[],
            support::EXPIRES.parse().unwrap(),
            Fault::SharedChannelKey,
        )
        .await;
    assert!(
        authority
            .prepare(
                request(Change::Snapshot {
                    previous: Some(text(&feed, "1.snapshot.json")),
                    targets: text(&changed, "2.targets.json"),
                    release: text(&changed, "2.release.json"),
                    nightly: text(&changed, "2.nightly.json"),
                }),
                support::time().unwrap()
            )
            .is_err()
    );
    let mut root = fixture.root.clone();
    root.roles.get_mut(&RoleType::Snapshot).unwrap().keyids =
        root.roles[&RoleType::Timestamp].keyids.clone();
    let signed = SignedRole::new(
        root.clone(),
        &KeyHolder::Root(root),
        &fixture.signing_keys(&[0]),
        &SystemRandom::new(),
    )
    .await
    .unwrap();
    assert!(
        Authority::load(
            signed.buffer(),
            &feed.files["1.targets.json"],
            support::time().unwrap()
        )
        .is_err()
    );
}

#[tokio::test]
async fn metadata_limits_expiry_and_version_exhaustion_are_enforced() {
    let (_, _, authority) = context(&[]).await;
    let mut future = request(Change::TargetsRenewal {});
    future.prepared_at = after(future.prepared_at, 1).unwrap();
    assert!(authority.prepare(future, support::time().unwrap()).is_err());
    assert!(Request::parse(&vec![b' '; MAX_REQUEST_BYTES + 1]).is_err());
    assert!(next_version(NonZeroU64::new(u64::MAX), None).is_err());
    assert!(
        authority
            .prepare(
                request(Change::Timestamp {
                    previous: None,
                    snapshot: "x".repeat(MAX_METADATA_BYTES + 1),
                }),
                support::time().unwrap()
            )
            .is_err()
    );
    let (fixture, _, authority) = context(&[]).await;
    for expires in ["2026-10-13T00:00:00Z", "2027-01-01T00:00:00Z"] {
        let feed = fixture
            .publish(2, &[], expires.parse().unwrap(), Fault::None)
            .await;
        assert!(
            authority
                .prepare(
                    request(Change::Snapshot {
                        previous: None,
                        targets: text(&feed, "2.targets.json"),
                        release: text(&feed, "2.release.json"),
                        nightly: text(&feed, "2.nightly.json"),
                    }),
                    support::time().unwrap()
                )
                .is_err()
        );
        assert!(
            authority
                .prepare(
                    request(Change::Timestamp {
                        previous: None,
                        snapshot: text(&feed, "2.snapshot.json"),
                    }),
                    support::time().unwrap()
                )
                .is_err()
        );
    }
}
