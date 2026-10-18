use crate::store::{ImageRecord, RetainedFeed, trusted_fingerprint};
use crate::{
    Channel, DeviceIdentity, Eligibility, Error, ImageDescriptor, ImageHistory, Result, Sha256,
    TrustStore, verify_blob,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{StreamExt, stream};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tough::{
    Limits, RepositoryLoader, Transport, TransportError, TransportErrorKind, TransportStream,
};
use url::Url;

pub const MAX_METADATA_BYTES: usize = 256 * 1024;
const MAX_METADATA_TOTAL: usize = 8 * 1024 * 1024;
const MAX_METADATA_REQUESTS: usize = 40;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedLocation {
    pub metadata: Url,
    pub targets: Url,
}

impl FeedLocation {
    pub fn new(metadata: Url, targets: Url) -> Result<Self> {
        let value = Self { metadata, targets };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        for url in [&self.metadata, &self.targets] {
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || !url.path().ends_with('/')
            {
                return Err(Error::Feed(
                    "expected credential-free HTTPS base URLs ending in /".into(),
                ));
            }
        }
        Ok(())
    }
}

/// A reading already admitted by the device time service (#256). The verifier
/// does not infer usable time from the host's date, link, or a UI assertion.
#[derive(Clone, Copy, Debug)]
pub struct UsableTime(pub Timestamp);

impl UsableTime {
    /// Convert a reading already admitted by the independent time service.
    pub fn from_unix_microseconds(us: u64) -> Result<Self> {
        let us = i64::try_from(us).map_err(|_| Error::WaitingForTime)?;
        Timestamp::from_microsecond(us)
            .map(Self)
            .map_err(|_| Error::WaitingForTime)
    }
}

impl tough::Clock for UsableTime {
    fn now(&self) -> Timestamp {
        self.0
    }
}

/// Only produced after TUF verification and device eligibility checks. It has
/// no deserializer: a status object received from the UI cannot mint evidence.
#[derive(Clone, Debug, Serialize)]
pub struct VerifiedCandidate {
    id: Sha256,
    image: ImageDescriptor,
    metadata_fingerprint: Sha256,
    expires: Timestamp,
}

impl VerifiedCandidate {
    pub fn id(&self) -> Sha256 {
        self.id
    }
    pub fn image(&self) -> &ImageDescriptor {
        &self.image
    }
    pub fn expires(&self) -> Timestamp {
        self.expires
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Selection {
    Available {
        candidate: VerifiedCandidate,
    },
    Ineligible {
        image: ImageDescriptor,
        reason: Eligibility,
    },
    NoCandidate,
}

impl TrustStore {
    /// Refresh through a transport supplied by the service. Production uses a
    /// bounded HTTPS-only transport; fixtures implement the same trait in memory.
    /// No installed image or boot state is changed by checking a feed.
    pub async fn refresh(
        &mut self,
        transport: Arc<dyn Transport>,
        time: Option<UsableTime>,
        channel: Channel,
        device: &DeviceIdentity,
        history: &ImageHistory,
    ) -> Result<Selection> {
        let time = time.ok_or(Error::WaitingForTime)?;
        self.admit_time(time)?;
        let captured = CaptureTransport::new(transport, self.state.location.metadata.clone());
        let work = self.working_datastore()?;
        let root = self
            .state
            .trusted
            .get("root.json")
            .ok_or_else(|| Error::Store("missing root".into()))?;
        let loaded = tokio::time::timeout(
            Duration::from_secs(120),
            RepositoryLoader::new(
                &root.as_bytes(),
                self.state.location.metadata.clone(),
                self.state.location.targets.clone(),
            )
            .datastore(work.path())
            .clock(Arc::new(time))
            .limits(metadata_limits())
            .transport(captured.clone())
            .load(),
        )
        .await
        .map_err(|_| Error::Timeout)
        .and_then(|result| result.map_err(Error::from));

        // Persist accepted TUF versions even when a later role/descriptor fails.
        // Publication of the complete store is the transaction's commit point;
        // no successful candidate is returned before the durable replacement.
        self.retain_datastore(work.path())?;
        let result = loaded.and_then(|repository| {
            let images = validate_catalog(&repository)?;
            self.remember_images(&images)?;
            let metadata = captured
                .recorded
                .lock()
                .expect("capture mutex")
                .files
                .clone();
            let fingerprint = trusted_fingerprint(&self.state.trusted);
            self.state.feed = Some(RetainedFeed {
                metadata,
                trusted_fingerprint: fingerprint,
            });
            Ok(select(
                images,
                channel,
                device,
                history,
                fingerprint,
                earliest_expiration(&repository),
            ))
        });
        self.persist()?;
        result
    }

    /// Reverify retained signatures/expiry against the current persistent trust
    /// floor. No transport from outside the store is used, so an eligible cached
    /// image can be installed without Internet access.
    pub async fn cached(
        &mut self,
        time: Option<UsableTime>,
        channel: Channel,
        device: &DeviceIdentity,
        history: &ImageHistory,
    ) -> Result<Selection> {
        let time = time.ok_or(Error::WaitingForTime)?;
        let feed = self.state.feed.as_ref().ok_or(Error::RefreshRequired)?;
        if feed.trusted_fingerprint != trusted_fingerprint(&self.state.trusted) {
            return Err(Error::RefreshRequired);
        }
        let transport = Arc::new(RetainedTransport {
            base: self.state.location.metadata.clone(),
            files: feed.metadata.clone(),
        });
        self.refresh(transport, Some(time), channel, device, history)
            .await
    }

    /// The installer uses the same selection rules as the downloader, then
    /// hashes its opened private staging file again. Candidate IDs authorize no
    /// arbitrary path, target device, command, or older rollback image.
    pub async fn revalidate_image(
        &mut self,
        id: Sha256,
        reader: &mut impl Read,
        time: Option<UsableTime>,
        channel: Channel,
        device: &DeviceIdentity,
        history: &ImageHistory,
    ) -> Result<VerifiedCandidate> {
        match self.cached(time, channel, device, history).await? {
            Selection::Available { candidate } if candidate.id == id => {
                verify_blob(reader, &candidate.image.compressed)?;
                Ok(candidate)
            }
            _ => Err(Error::CandidateChanged),
        }
    }

    fn remember_images(&mut self, images: &[ImageDescriptor]) -> Result<()> {
        let mut records = self.state.images.clone();
        for image in images {
            let mut identity = image.clone();
            identity.channel = Channel::Release;
            let record = ImageRecord {
                payload: image.compressed.sha256,
                descriptor: identity.candidate_id(),
            };
            if records
                .get(&image.generation)
                .is_some_and(|prior| *prior != record)
                || records.iter().any(|(generation, prior)| {
                    *generation != image.generation && prior.payload == record.payload
                })
            {
                return Err(Error::Feed(
                    "image generation or immutable descriptor was reassigned".into(),
                ));
            }
            records.insert(image.generation, record);
        }
        self.state.images = records;
        Ok(())
    }

    /// Retain a reading admitted by the time service, including final checks
    /// after a download. Later clock corrections cannot revive expired metadata.
    pub fn admit_time(&mut self, time: UsableTime) -> Result<()> {
        if let Some(previous) = self.state.trusted.get("latest_known_time.json") {
            let previous: Timestamp = serde_json::from_str(previous)?;
            if time.0 < previous {
                return Err(Error::TimeMovedBackwards);
            }
            if time.0 == previous {
                return Ok(());
            }
        }
        self.state.trusted.insert(
            "latest_known_time.json".into(),
            serde_json::to_string(&time.0)?,
        );
        // A cancelled or crashed network attempt must not forget a usable time
        // already observed and later revive expired metadata after a correction.
        self.persist()
    }
}

pub(crate) fn metadata_limits() -> Limits {
    Limits {
        max_root_size: MAX_METADATA_BYTES as u64,
        max_targets_size: MAX_METADATA_BYTES as u64,
        max_timestamp_size: MAX_METADATA_BYTES as u64,
        max_snapshot_size: MAX_METADATA_BYTES as u64,
        max_root_updates: 32,
    }
}

pub(crate) fn validate_catalog(repository: &tough::Repository) -> Result<Vec<ImageDescriptor>> {
    let delegations = channel_policy(&repository.root().signed, &repository.targets().signed)?;
    let mut images = Vec::new();
    for channel in [Channel::Release, Channel::Nightly] {
        let targets = &delegations
            .roles
            .iter()
            .find(|role| role.name == channel.name())
            .expect("checked channel policy")
            .targets
            .as_ref()
            .ok_or_else(|| Error::Feed("unverified channel role".into()))?
            .signed;
        images.extend(channel_images(channel, targets)?);
    }
    Ok(images)
}

pub(crate) fn channel_policy<'a>(
    root: &tough::schema::Root,
    top: &'a tough::schema::Targets,
) -> Result<&'a tough::schema::Delegations> {
    let invalid = |reason: &str| Error::Feed(reason.into());
    if !root.consistent_snapshot {
        return Err(invalid("consistent snapshots are required"));
    }
    if !top.targets.is_empty() {
        return Err(invalid("images must belong to channel delegations"));
    }
    let delegations = top
        .delegations
        .as_ref()
        .ok_or_else(|| invalid("missing channel delegations"))?;
    if delegations.roles.len() != 2 {
        return Err(invalid("expected exactly Release and Nightly roles"));
    }
    let mut keys = BTreeSet::new();
    for role in root.roles.values() {
        keys.extend(role.keyids.iter().map(|key| key.as_ref().to_vec()));
    }
    for channel in [Channel::Release, Channel::Nightly] {
        let role = delegations
            .roles
            .iter()
            .find(|role| role.name == channel.name())
            .ok_or_else(|| invalid("missing selected channel role"))?;
        let expected_pattern = format!("{}.*", channel.name());
        if !matches!(&role.paths, tough::schema::PathSet::Paths(paths)
            if paths.len() == 1 && paths[0].value() == expected_pattern)
        {
            return Err(invalid("channel delegation escapes its namespace"));
        }
        for key in &role.keyids {
            if !keys.insert(key.as_ref().to_vec()) {
                return Err(invalid(
                    "channel signing keys must be distinct from other roles",
                ));
            }
        }
    }
    Ok(delegations)
}

pub(crate) fn channel_images(
    channel: Channel,
    targets: &tough::schema::Targets,
) -> Result<Vec<ImageDescriptor>> {
    let invalid = |reason: &str| Error::Feed(reason.into());
    let mut images = Vec::new();
    if targets
        .delegations
        .as_ref()
        .is_some_and(|delegations| !delegations.roles.is_empty() || !delegations.keys.is_empty())
    {
        return Err(invalid("nested channel delegations are unsupported"));
    }
    for (name, target) in &targets.targets {
        if target.custom.len() != 1 {
            return Err(invalid("expected custom.spacewars descriptor"));
        }
        let image: ImageDescriptor = serde_json::from_value(
            target
                .custom
                .get("spacewars")
                .ok_or_else(|| invalid("missing image descriptor"))?
                .clone(),
        )?;
        image.validate()?;
        if image.channel != channel
            || name.raw() != image.target_name()
            || name.raw() != name.resolved()
            || target.length != image.compressed.bytes
            || target.hashes.sha256.as_ref() != image.compressed.sha256.as_bytes()
        {
            return Err(invalid(
                "target name, channel, length or hash disagrees with descriptor",
            ));
        }
        images.push(image);
    }
    Ok(images)
}

fn earliest_expiration(repository: &tough::Repository) -> Timestamp {
    let mut expiration = [
        repository.root().signed.expires,
        repository.timestamp().signed.expires,
        repository.snapshot().signed.expires,
        repository.targets().signed.expires,
    ]
    .into_iter()
    .min()
    .unwrap();
    for role in &repository
        .targets()
        .signed
        .delegations
        .as_ref()
        .unwrap()
        .roles
    {
        expiration = expiration.min(role.targets.as_ref().unwrap().signed.expires);
    }
    expiration
}

fn select(
    images: Vec<ImageDescriptor>,
    channel: Channel,
    device: &DeviceIdentity,
    history: &ImageHistory,
    metadata_fingerprint: Sha256,
    expires: Timestamp,
) -> Selection {
    let matches_hardware = |image: &ImageDescriptor| {
        image.machine == device.machine
            && image.architecture == device.architecture
            && image.layout == device.layout
            && image.boot_identity == device.boot_identity
    };
    let image = images
        .into_iter()
        .filter(|image| image.channel == channel)
        .max_by_key(|image| (matches_hardware(image), image.generation));
    let Some(image) = image else {
        return Selection::NoCandidate;
    };
    let reason = image.eligibility(device, history);
    if reason != Eligibility::Eligible {
        return Selection::Ineligible { image, reason };
    }
    Selection::Available {
        candidate: VerifiedCandidate {
            id: image.candidate_id(),
            image,
            metadata_fingerprint,
            expires,
        },
    }
}

#[derive(Debug, Default)]
struct Recorded {
    requests: usize,
    bytes: usize,
    files: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct CaptureTransport {
    inner: Arc<dyn Transport>,
    base: Url,
    recorded: Arc<Mutex<Recorded>>,
}

impl CaptureTransport {
    fn new(inner: Arc<dyn Transport>, base: Url) -> Self {
        Self {
            inner,
            base,
            recorded: Arc::new(Mutex::new(Recorded::default())),
        }
    }
}

fn metadata_name(base: &Url, url: &Url) -> Option<String> {
    let name = url.as_str().strip_prefix(base.as_str())?;
    let (version, role) = name.split_once('.').map_or((None, name), |(first, rest)| {
        if first.bytes().all(|b| b.is_ascii_digit()) && !first.is_empty() {
            (Some(first), rest)
        } else {
            (None, name)
        }
    });
    if ![
        "root.json",
        "timestamp.json",
        "snapshot.json",
        "targets.json",
        "release.json",
        "nightly.json",
    ]
    .contains(&role)
        || (role == "root.json" && version.is_none())
        || (role == "timestamp.json" && version.is_some())
    {
        return None;
    }
    Some(name.into())
}

fn transport_error(url: &Url, message: &str) -> TransportError {
    TransportError::new_with_cause(TransportErrorKind::Other, url, message.to_owned())
}

#[async_trait]
impl Transport for CaptureTransport {
    async fn fetch(&self, url: Url) -> std::result::Result<TransportStream, TransportError> {
        let name = metadata_name(&self.base, &url)
            .ok_or_else(|| transport_error(&url, "unsupported metadata path"))?;
        {
            let mut state = self.recorded.lock().expect("capture mutex");
            state.requests += 1;
            if state.requests > MAX_METADATA_REQUESTS {
                return Err(transport_error(&url, "metadata request limit"));
            }
        }
        let mut stream = self.inner.fetch(url.clone()).await?;
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let mut state = self.recorded.lock().expect("capture mutex");
            if chunk.len() > MAX_METADATA_BYTES.saturating_sub(bytes.len())
                || chunk.len() > MAX_METADATA_TOTAL.saturating_sub(state.bytes)
            {
                return Err(transport_error(&url, "metadata byte limit"));
            }
            state.bytes += chunk.len();
            bytes.extend_from_slice(&chunk);
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| transport_error(&url, "metadata is not UTF-8"))?;
        self.recorded
            .lock()
            .expect("capture mutex")
            .files
            .insert(name, text.into());
        Ok(Box::pin(stream::once(std::future::ready(Ok(Bytes::from(
            bytes,
        ))))))
    }
}

#[derive(Debug)]
struct RetainedTransport {
    base: Url,
    files: BTreeMap<String, String>,
}

// TUF's transport trait requires a clonable transport. This one never performs
// an OS/network lookup, including when the next root version is unavailable.
impl Clone for RetainedTransport {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            files: self.files.clone(),
        }
    }
}

#[async_trait]
impl Transport for RetainedTransport {
    async fn fetch(&self, url: Url) -> std::result::Result<TransportStream, TransportError> {
        let name = metadata_name(&self.base, &url)
            .ok_or_else(|| transport_error(&url, "unsupported metadata path"))?;
        let data = self
            .files
            .get(&name)
            .ok_or_else(|| TransportError::new(TransportErrorKind::FileNotFound, &url))?;
        Ok(Box::pin(stream::once(std::future::ready(Ok(
            Bytes::copy_from_slice(data.as_bytes()),
        )))))
    }
}
