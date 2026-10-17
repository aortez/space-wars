use async_trait::async_trait;
use aws_lc_rs::{rand::SystemRandom, signature::Ed25519KeyPair};
use bytes::Bytes;
use engine_ota::*;
use futures::stream;
use jiff::Timestamp;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use tough::editor::signed::SignedRole;
use tough::key_source::KeySource;
use tough::schema::{
    DelegatedRole, DelegatedTargets, Delegations, Hashes, KeyHolder, Metafile, PathPattern,
    PathSet, Role, RoleKeys, RoleType, Root, Snapshot, Target, Targets,
};
use tough::{TargetName, Transport, TransportError, TransportErrorKind, TransportStream};
use url::Url;

pub const NOW: &str = "2026-10-14T12:00:00Z";
pub const EXPIRES: &str = "2026-10-21T12:00:00Z";

pub fn time() -> Option<UsableTime> {
    Some(UsableTime(NOW.parse().unwrap()))
}

pub fn location() -> FeedLocation {
    FeedLocation::new(
        Url::parse("https://updates.example/metadata/").unwrap(),
        Url::parse("https://updates.example/images/").unwrap(),
    )
    .unwrap()
}

pub fn descriptor(channel: Channel, generation: u64) -> ImageDescriptor {
    let payload = payload(generation);
    ImageDescriptor {
        schema: DESCRIPTOR_SCHEMA,
        generation,
        source_commit: "0123456789abcdef0123456789abcdef01234567".into(),
        build_id: format!("fixture-{generation}"),
        channel,
        machine: "spacewars-rpi".into(),
        architecture: "aarch64".into(),
        layout: IMAGE_LAYOUT.into(),
        format: ImageFormat::RootfsExt4Zstd,
        compressed: Blob {
            bytes: payload.len() as u64,
            sha256: Sha256::digest(&payload),
        },
        expanded: Blob {
            bytes: 1024,
            sha256: Sha256::digest(b"expanded fixture image"),
        },
        boot_identity: Sha256::digest(b"fixture boot assets"),
        min_updater_protocol: 1,
        min_recovery_protocol: engine_ota::recovery::RECOVERY_PROTOCOL,
        data: DataCompatibility {
            reads: SchemaRange { min: 1, max: 2 },
            writes: 1,
        },
    }
}

pub fn payload(generation: u64) -> Vec<u8> {
    format!("fixture image generation {generation}").into_bytes()
}

pub fn device() -> DeviceIdentity {
    let image = descriptor(Channel::Release, 1);
    DeviceIdentity {
        machine: image.machine,
        architecture: image.architecture,
        layout: image.layout,
        boot_identity: image.boot_identity,
        updater_protocol: 1,
        recovery_protocol: engine_ota::recovery::RECOVERY_PROTOCOL,
        inactive_slot_bytes: 2048,
        persistent_schema: 1,
        rollback_reads: SchemaRange { min: 1, max: 1 },
    }
}

pub fn candidate(selection: Selection) -> VerifiedCandidate {
    match selection {
        Selection::Available { candidate } => candidate,
        other => panic!("expected candidate, got {other:?}"),
    }
}

#[derive(Clone)]
struct TestKey(Vec<u8>);

impl std::fmt::Debug for TestKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ephemeral test key")
    }
}

#[async_trait]
impl KeySource for TestKey {
    async fn as_sign(
        &self,
    ) -> std::result::Result<Box<dyn tough::sign::Sign>, Box<dyn std::error::Error + Send + Sync>>
    {
        Ok(Box::new(Ed25519KeyPair::from_pkcs8(&self.0)?))
    }
    async fn write(
        &self,
        _: &str,
        _: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("fixture keys are immutable".into())
    }
}

#[derive(Clone, Debug, Default)]
pub struct MemoryTransport {
    pub files: BTreeMap<String, Vec<u8>>,
    pub requests: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Transport for MemoryTransport {
    async fn fetch(&self, url: Url) -> std::result::Result<TransportStream, TransportError> {
        let name = url.path().strip_prefix("/metadata/").unwrap_or("invalid");
        self.requests.lock().unwrap().push(name.to_owned());
        let data = self
            .files
            .get(name)
            .ok_or_else(|| TransportError::new(TransportErrorKind::FileNotFound, &url))?;
        Ok(Box::pin(stream::once(std::future::ready(Ok(
            Bytes::copy_from_slice(data),
        )))))
    }
}

#[derive(Clone, Copy, Default)]
pub enum Fault {
    #[default]
    None,
    MissingReleaseSignature,
    RepeatedReleaseSignature,
    WrongReleaseSignature,
    CrossChannelName,
    UnsafeName,
    DescriptorHash,
    ExtraDelegation,
    SharedChannelKey,
    // These cases are used by publisher unit tests. This shared fixture is
    // also compiled separately by the device-verifier integration tests.
    #[allow(dead_code)]
    UnboundSnapshotReference,
    #[allow(dead_code)]
    ExtraSnapshotReference,
}

pub struct Fixture {
    keys: Vec<TestKey>,
    pub root: Root,
    pub root_bytes: Vec<u8>,
    pub roots: BTreeMap<String, Vec<u8>>,
}

impl Fixture {
    pub async fn new() -> Self {
        let rng = SystemRandom::new();
        let keys: Vec<_> = (0..8)
            .map(|_| {
                TestKey(
                    Ed25519KeyPair::generate_pkcs8(&rng)
                        .unwrap()
                        .as_ref()
                        .to_vec(),
                )
            })
            .collect();
        let mut root = Root {
            spec_version: "1.0.0".into(),
            consistent_snapshot: true,
            version: nz(1),
            expires: "2027-10-14T12:00:00Z".parse().unwrap(),
            keys: HashMap::new(),
            roles: HashMap::new(),
            _extra: HashMap::new(),
        };
        for (index, role) in [
            RoleType::Root,
            RoleType::Targets,
            RoleType::Snapshot,
            RoleType::Timestamp,
        ]
        .into_iter()
        .enumerate()
        {
            let key = keys[index].as_sign().await.unwrap().tuf_key();
            let id = key.key_id().unwrap();
            root.keys.insert(id.clone(), key);
            root.roles.insert(
                role,
                RoleKeys {
                    keyids: vec![id],
                    threshold: nz(1),
                    _extra: HashMap::new(),
                },
            );
        }
        let signed = sign(root.clone(), &KeyHolder::Root(root.clone()), &keys, &[0]).await;
        let root_bytes = signed.buffer().clone();
        Self {
            keys,
            root,
            root_bytes: root_bytes.clone(),
            roots: BTreeMap::from([("1.root.json".into(), root_bytes)]),
        }
    }

    pub async fn rotate_root(&mut self) {
        let old_root = self.root.clone();
        let new_key = self.keys[7].as_sign().await.unwrap().tuf_key();
        let id = new_key.key_id().unwrap();
        self.root.version = nz(2);
        self.root.keys.insert(id.clone(), new_key.clone());
        self.root.roles.get_mut(&RoleType::Root).unwrap().keyids = vec![id.clone()];
        let mut signer = old_root;
        signer.keys.insert(id.clone(), new_key);
        signer
            .roles
            .get_mut(&RoleType::Root)
            .unwrap()
            .keyids
            .push(id);
        let signed = sign(
            self.root.clone(),
            &KeyHolder::Root(signer),
            &self.keys,
            &[0, 7],
        )
        .await;
        self.roots
            .insert("2.root.json".into(), signed.buffer().clone());
    }

    pub async fn publish(
        &self,
        version: u64,
        images: &[ImageDescriptor],
        expires: Timestamp,
        fault: Fault,
    ) -> MemoryTransport {
        let mut transport = MemoryTransport {
            files: self.roots.clone(),
            ..MemoryTransport::default()
        };
        let mut delegations = Delegations::default();
        for (channel, key_indices) in [
            (Channel::Release, vec![4, 5]),
            (
                Channel::Nightly,
                if matches!(fault, Fault::SharedChannelKey) {
                    vec![4]
                } else {
                    vec![6]
                },
            ),
        ] {
            let mut keyids = Vec::new();
            for i in &key_indices {
                let key = self.keys[*i].as_sign().await.unwrap().tuf_key();
                let id = key.key_id().unwrap();
                keyids.push(id.clone());
                delegations.keys.insert(id, key);
            }
            delegations.roles.push(DelegatedRole {
                name: channel.name().into(),
                keyids,
                threshold: nz(key_indices.len() as u64),
                paths: PathSet::Paths(vec![
                    PathPattern::new(format!("{}.*", channel.name())).unwrap(),
                ]),
                terminating: true,
                targets: None,
            });
        }
        let mut snapshot = Snapshot::new("1.0.0".into(), nz(version), expires);
        for (channel, key_indices) in [
            (Channel::Release, vec![4, 5]),
            (
                Channel::Nightly,
                if matches!(fault, Fault::SharedChannelKey) {
                    vec![4]
                } else {
                    vec![6]
                },
            ),
        ] {
            let mut targets = Targets::new("1.0.0".into(), nz(version), expires);
            if matches!(fault, Fault::ExtraDelegation) && channel == Channel::Release {
                targets.delegations = Some(delegations.clone());
            }
            for image in images.iter().filter(|image| image.channel == channel) {
                let mut custom = serde_json::to_value(image).unwrap();
                if matches!(fault, Fault::DescriptorHash) {
                    custom["compressed"]["sha256"] =
                        Value::String(Sha256::digest(b"other bytes").to_string());
                }
                let mut name = image.target_name();
                if channel == Channel::Release {
                    if matches!(fault, Fault::CrossChannelName) {
                        name = name.replace("release.", "nightly.");
                    }
                    if matches!(fault, Fault::UnsafeName) {
                        name = format!("../{name}");
                    }
                }
                targets.targets.insert(
                    TargetName::new(name).unwrap(),
                    Target {
                        length: image.compressed.bytes,
                        hashes: Hashes {
                            sha256: image.compressed.sha256.as_bytes().to_vec().into(),
                            _extra: HashMap::new(),
                        },
                        custom: HashMap::from([("spacewars".into(), custom)]),
                        _extra: HashMap::new(),
                    },
                );
            }
            let delegated = DelegatedTargets {
                name: channel.name().into(),
                targets,
            };
            let signed = sign(
                delegated,
                &KeyHolder::Delegations(delegations.clone()),
                &self.keys,
                &key_indices,
            )
            .await;
            let mut body = signed.signed().clone();
            if channel == Channel::Release {
                match fault {
                    Fault::MissingReleaseSignature => {
                        body.signatures.pop();
                    }
                    Fault::RepeatedReleaseSignature => {
                        body.signatures[1] = body.signatures[0].clone();
                    }
                    Fault::WrongReleaseSignature => {
                        body.signatures[0].sig = vec![0; 64].into();
                    }
                    _ => {}
                }
            }
            let bytes = serde_json::to_vec(&body).unwrap();
            snapshot
                .meta
                .insert(format!("{}.json", channel.name()), meta(version, &bytes));
            transport
                .files
                .insert(format!("{version}.{}.json", channel.name()), bytes);
        }
        let mut targets = Targets::new("1.0.0".into(), nz(version), expires);
        targets.delegations = Some(delegations);
        let targets = sign(
            targets,
            &KeyHolder::Root(self.root.clone()),
            &self.keys,
            &[1],
        )
        .await;
        snapshot
            .meta
            .insert("targets.json".into(), meta(version, targets.buffer()));
        if matches!(fault, Fault::UnboundSnapshotReference) {
            let reference = snapshot.meta.get_mut("release.json").unwrap();
            reference.length = None;
            reference.hashes = None;
        }
        if matches!(fault, Fault::ExtraSnapshotReference) {
            snapshot
                .meta
                .insert("obsolete.json".into(), meta(version, b"unused"));
        }
        transport
            .files
            .insert(format!("{version}.targets.json"), targets.buffer().clone());
        let snapshot = sign(
            snapshot,
            &KeyHolder::Root(self.root.clone()),
            &self.keys,
            &[2],
        )
        .await;
        let mut timestamp = tough::schema::Timestamp::new("1.0.0".into(), nz(version), expires);
        timestamp
            .meta
            .insert("snapshot.json".into(), meta(version, snapshot.buffer()));
        transport.files.insert(
            format!("{version}.snapshot.json"),
            snapshot.buffer().clone(),
        );
        let timestamp = sign(
            timestamp,
            &KeyHolder::Root(self.root.clone()),
            &self.keys,
            &[3],
        )
        .await;
        transport
            .files
            .insert("timestamp.json".into(), timestamp.buffer().clone());
        transport
    }
}

fn nz(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}

fn meta(version: u64, bytes: &[u8]) -> Metafile {
    Metafile {
        version: nz(version),
        length: Some(bytes.len() as u64),
        hashes: Some(Hashes {
            sha256: Sha256::digest(bytes).as_bytes().to_vec().into(),
            _extra: HashMap::new(),
        }),
        _extra: HashMap::new(),
    }
}

async fn sign<T: Role + serde::Serialize>(
    role: T,
    holder: &KeyHolder,
    keys: &[TestKey],
    indices: &[usize],
) -> SignedRole<T> {
    let sources: Vec<Box<dyn KeySource>> = indices
        .iter()
        .map(|i| Box::new(keys[*i].clone()) as Box<dyn KeySource>)
        .collect();
    SignedRole::new(role, holder, &sources, &SystemRandom::new())
        .await
        .unwrap()
}
