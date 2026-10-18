//! Offline inventory for the delivery stage of a publisher. This verifies TUF
//! and the compressed payloads; it does not authorize a build, sign metadata,
//! qualify an image, allocate generations, or approve Release promotion.
use crate::{Blob, Channel, Error, MAX_METADATA_BYTES, Result, Sha256, UsableTime};
use async_trait::async_trait;
use bytes::Bytes;
use futures::stream;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tough::{RepositoryLoader, Transport, TransportError, TransportErrorKind, TransportStream};
use url::Url;

const BASE: &str = "https://offline.invalid/";
const MAX_FILES: usize = 1000;
const MAX_METADATA_TOTAL: usize = 8 * 1024 * 1024;

/// Serialized bytes must be bound to the publisher's separately admitted
/// authorization. A checksum of an arbitrary manifest is not build approval.
#[derive(Debug, Serialize)]
pub struct DeliveryManifest {
    schema: u32,
    trusted_root_sha256: Sha256,
    timestamp_version: u64,
    files: BTreeMap<String, Blob>,
}

#[derive(Clone, Debug)]
struct Offline {
    files: Arc<BTreeMap<String, Vec<u8>>>,
    read: Arc<Mutex<BTreeSet<String>>>,
}

#[async_trait]
impl Transport for Offline {
    async fn fetch(&self, url: Url) -> std::result::Result<TransportStream, TransportError> {
        let name = url.as_str().strip_prefix(BASE).unwrap_or("");
        let bytes = self.files.get(name).ok_or_else(|| {
            TransportError::new(TransportErrorKind::FileNotFound, "[offline metadata]")
        })?;
        self.read
            .lock()
            .expect("metadata reads")
            .insert(name.into());
        Ok(Box::pin(stream::once(std::future::ready(Ok(
            Bytes::copy_from_slice(bytes),
        )))))
    }
}

/// Verify a complete, flat publication directory without using the network.
/// The trusted root is selected independently of that directory. `now` is the
/// publisher's admitted UTC time; expiry enforcement is never disabled.
pub async fn inventory(root: &[u8], directory: &Path, now: UsableTime) -> Result<DeliveryManifest> {
    if root.len() > MAX_METADATA_BYTES {
        return Err(invalid("trusted root exceeds metadata limit"));
    }
    let pinned: tough::schema::Signed<tough::schema::Root> = serde_json::from_slice(root)?;
    let pinned_name = format!("{}.root.json", pinned.signed.version);
    let mut names = BTreeSet::new();
    let mut metadata = BTreeMap::new();
    let mut total = 0usize;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("non-UTF-8 asset name"))?;
        if !entry.file_type()?.is_file() || names.len() >= MAX_FILES {
            return Err(invalid("non-file asset or asset count limit exceeded"));
        }
        if name.ends_with(".json") {
            if !metadata_name(&name) {
                return Err(invalid("unsupported metadata asset name"));
            }
            let mut bytes = Vec::new();
            open_file(&entry.path())?
                .take(MAX_METADATA_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            total += bytes.len();
            if bytes.len() > MAX_METADATA_BYTES || total > MAX_METADATA_TOTAL {
                return Err(invalid("publication exceeds metadata limits"));
            }
            metadata.insert(name.clone(), bytes);
        }
        names.insert(name);
    }
    if metadata.get(&pinned_name).map(Vec::as_slice) != Some(root) {
        return Err(invalid("publication must include the exact pinned root"));
    }
    let transport = Offline {
        files: Arc::new(metadata),
        read: Arc::default(),
    };
    let repository = RepositoryLoader::new(&root, BASE.parse().unwrap(), BASE.parse().unwrap())
        .clock(Arc::new(now))
        .limits(crate::repository::metadata_limits())
        .transport(transport.clone())
        .load()
        .await?;
    let images = crate::repository::validate_catalog(&repository)?;
    for (meta, expected) in [
        (&repository.timestamp().signed.meta, &["snapshot.json"][..]),
        (
            &repository.snapshot().signed.meta,
            &["targets.json", "release.json", "nightly.json"][..],
        ),
    ] {
        if meta.len() != expected.len() {
            return Err(invalid("unexpected metadata references"));
        }
        for name in expected {
            let reference = meta
                .get(*name)
                .ok_or_else(|| invalid("missing metadata reference"))?;
            let bytes = transport
                .files
                .get(&format!("{}.{}", reference.version, name))
                .ok_or_else(|| invalid("missing referenced metadata"))?;
            if reference.length != Some(bytes.len() as u64)
                || reference
                    .hashes
                    .as_ref()
                    .map(|hashes| hashes.sha256.as_ref())
                    != Some(Sha256::digest(bytes).as_bytes().as_slice())
            {
                return Err(invalid(
                    "metadata references must bind exact length and SHA-256",
                ));
            }
        }
    }
    let mut used = transport.read.lock().expect("metadata reads").clone();
    used.insert(pinned_name);
    // No orphan metadata, old snapshots or a gap in the supported root chain
    // can be silently uploaded as part of this publication.
    if used.len() != transport.files.len() {
        return Err(invalid("unreferenced metadata in publication"));
    }
    let mut files: BTreeMap<_, _> = transport
        .files
        .iter()
        .map(|(name, bytes)| {
            (
                name.clone(),
                Blob {
                    bytes: bytes.len() as u64,
                    sha256: Sha256::digest(bytes),
                },
            )
        })
        .collect();
    let mut generations = BTreeMap::new();
    let mut payloads = BTreeMap::new();
    for mut image in images {
        let name = format!("{}.{}", image.compressed.sha256, image.target_name());
        // Promotion may change only the channel. Reject identity conflicts
        // within this snapshot; durable historical allocation belongs upstream.
        image.channel = Channel::Release;
        if generations
            .insert(image.generation, image.candidate_id())
            .is_some_and(|prior| prior != image.candidate_id())
            || payloads
                .insert(image.compressed.sha256, image.generation)
                .is_some_and(|prior| prior != image.generation)
        {
            return Err(invalid("conflicting image generation or identity"));
        }
        crate::verify_blob(&mut open_file(&directory.join(&name))?, &image.compressed)?;
        files.insert(name, image.compressed);
    }
    if names != files.keys().cloned().collect() {
        return Err(invalid(
            "publication contains missing or unreferenced assets",
        ));
    }
    Ok(DeliveryManifest {
        schema: 1,
        trusted_root_sha256: Sha256::digest(root),
        timestamp_version: repository.timestamp().signed.version.get(),
        files,
    })
}

fn open_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("asset is not a regular file"));
    }
    Ok(file)
}

fn metadata_name(name: &str) -> bool {
    if name == "timestamp.json" {
        return true;
    }
    let Some((version, role)) = name.split_once('.') else {
        return false;
    };
    version
        .parse::<u64>()
        .is_ok_and(|v| v > 0 && v.to_string() == version)
        && [
            "root.json",
            "targets.json",
            "snapshot.json",
            "release.json",
            "nightly.json",
        ]
        .contains(&role)
}

fn invalid(reason: &str) -> Error {
    Error::Feed(reason.into())
}

#[cfg(test)]
#[path = "publication_tests.rs"]
mod tests;
