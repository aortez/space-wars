//! Role-scoped publisher signing. These operations do not establish build or
//! workflow approval: the isolated caller must select current authority/parents,
//! qualify the exact image, and admit the proposal digest independently.
use crate::{Channel, Error, ImageDescriptor, MAX_METADATA_BYTES, Result, Sha256, UsableTime};
use async_trait::async_trait;
use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use jiff::Timestamp;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{BTreeSet, HashMap};
use std::num::NonZeroU64;
use std::sync::Arc;
use tough::TargetName;
use tough::editor::signed::SignedRole;
use tough::key_source::KeySource;
use tough::schema::{
    DelegatedTargets, Delegations, Hashes, KeyHolder, Metafile, Role, RoleKeys, RoleType, Root,
    Signed, Snapshot, Target, Targets,
};
use tough::sign::Sign;

pub const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
const DAY: i64 = 24 * 60 * 60;

/// Public, unsigned inputs only. No paths, scripts, private keys or authority
/// overrides can be supplied in a candidate's request.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub prepared_at: Timestamp,
    /// A publisher journal can reserve a later version after an interrupted or
    /// abandoned attempt. It must advance the supplied signed parent; the
    /// selected number is bound into the proposal before any key is read.
    #[serde(default)]
    pub version: Option<NonZeroU64>,
    pub change: Change,
}

#[derive(Deserialize)]
#[serde(tag = "role", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Change {
    Channel {
        channel: Channel,
        previous: Option<String>,
        image: Option<Box<ImageDescriptor>>,
    },
    TargetsRenewal {},
    Snapshot {
        previous: Option<String>,
        targets: String,
        release: String,
        nightly: String,
    },
    Timestamp {
        previous: Option<String>,
        snapshot: String,
    },
}

impl Request {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(invalid("signing request exceeds size limit"));
        }
        Ok(serde_json::from_slice(bytes)?)
    }
}

pub struct Authority {
    root: Root,
    targets: Targets,
    targets_hash: Sha256,
    fingerprint: Sha256,
}

enum Body {
    Channel(Channel, Targets),
    Targets(Targets),
    Snapshot(Snapshot),
    Timestamp(tough::schema::Timestamp),
}

/// Constructed only after checking signatures, namespaces, parent versions and
/// fixed expiration policy. A caller cannot deserialize or mutate this object.
pub struct Proposal {
    authority: Sha256,
    prepared_at: Timestamp,
    body: Body,
    approval_bytes: Vec<u8>,
}

impl Proposal {
    pub fn digest(&self) -> Sha256 {
        Sha256::digest(&self.approval_bytes)
    }
    pub fn approval_bytes(&self) -> &[u8] {
        &self.approval_bytes
    }
}

impl Authority {
    /// Both files must be selected independently by the trusted publisher. This
    /// checks their authentication but cannot discover the latest published root
    /// or delegation policy from an arbitrary candidate directory.
    pub fn load(root: &[u8], targets: &[u8], now: UsableTime) -> Result<Self> {
        let signed: Signed<Root> = parse(root)?;
        signed
            .signed
            .verify_role(&signed)
            .map_err(|_| invalid("invalid trusted root signature"))?;
        let top: Signed<Targets> = parse(targets)?;
        signed
            .signed
            .verify_role(&top)
            .map_err(|_| invalid("invalid delegation policy signature"))?;
        let delegations = crate::repository::channel_policy(&signed.signed, &top.signed)?;
        if signed.signed.roles.len() != 4
            || [
                RoleType::Root,
                RoleType::Targets,
                RoleType::Snapshot,
                RoleType::Timestamp,
            ]
            .iter()
            .any(|role| !signed.signed.roles.contains_key(role))
        {
            return Err(invalid(
                "expected exactly four top-level signing authorities",
            ));
        }
        let mut ids = BTreeSet::new();
        for role in signed.signed.roles.values() {
            if role.keyids.len() > 32 || role.threshold.get() > role.keyids.len() as u64 {
                return Err(invalid("unsupported signing key threshold"));
            }
            for id in &role.keyids {
                if !ids.insert(id.as_ref().to_vec()) {
                    return Err(invalid("top-level signing authorities must be separate"));
                }
            }
        }
        for role in &delegations.roles {
            if !role.terminating
                || role.keyids.len() > 32
                || role.threshold.get() > role.keyids.len() as u64
            {
                return Err(invalid("unsupported channel signing authority"));
            }
        }
        let value = Self {
            root: signed.signed,
            targets: top.signed,
            targets_hash: Sha256::digest(targets),
            fingerprint: Sha256::digest(&canonical(&(
                "spacewars-signing-authority-v1",
                Sha256::digest(root),
                Sha256::digest(targets),
            ))?),
        };
        value.current(now)?;
        Ok(value)
    }

    fn current(&self, now: UsableTime) -> Result<()> {
        if self.root.expires <= now.0 || self.targets.expires <= now.0 {
            return Err(invalid(
                "publisher authority expired; refresh its independently selected policy",
            ));
        }
        Ok(())
    }

    fn delegations(&self) -> &Delegations {
        self.targets.delegations.as_ref().expect("validated policy")
    }

    fn root_role<T: Role + DeserializeOwned>(&self, bytes: &[u8]) -> Result<T> {
        let signed: Signed<T> = parse(bytes)?;
        self.root
            .verify_role(&signed)
            .map_err(|_| invalid("invalid top-level role signature"))?;
        Ok(signed.signed)
    }

    fn channel_role(&self, channel: Channel, bytes: &[u8]) -> Result<Targets> {
        let signed: Signed<Targets> = parse(bytes)?;
        self.delegations()
            .verify_role(&signed, channel.name())
            .map_err(|_| invalid("invalid channel signature"))?;
        crate::repository::channel_images(channel, &signed.signed)?;
        Ok(signed.signed)
    }

    pub fn prepare(&self, request: Request, now: UsableTime) -> Result<Proposal> {
        self.current(now)?;
        fresh(request.prepared_at, now)?;
        let expires = after(request.prepared_at, 30 * DAY)?;
        let (body, previous) = match request.change {
            Change::Channel {
                channel,
                previous,
                image,
            } => {
                let prior = previous
                    .as_ref()
                    .map(|bytes| self.channel_role(channel, bytes.as_bytes()))
                    .transpose()?;
                let mut next = Targets::new(
                    "1.0.0".into(),
                    next_version(prior.as_ref().map(|p| p.version), request.version)?,
                    expires,
                );
                if let Some(prior) = &prior {
                    next.targets = prior.targets.clone();
                    if expires < prior.expires {
                        return Err(invalid("channel renewal would shorten its expiry"));
                    }
                }
                if let Some(image) = image {
                    let image = *image;
                    image.validate()?;
                    if image.channel != channel {
                        return Err(invalid("image belongs to another channel"));
                    }
                    let mut images = crate::repository::channel_images(channel, &next)?;
                    images.push(image.clone());
                    image_identities(&images)?;
                    next.targets.insert(
                        TargetName::new(image.target_name())?,
                        Target {
                            length: image.compressed.bytes,
                            hashes: hashes(image.compressed.sha256),
                            custom: HashMap::from([(
                                "spacewars".into(),
                                serde_json::to_value(&image)?,
                            )]),
                            _extra: HashMap::new(),
                        },
                    );
                }
                (Body::Channel(channel, next), previous)
            }
            Change::TargetsRenewal {} => {
                let mut next = self.targets.clone();
                next.version = next_version(Some(next.version), request.version)?;
                if expires < next.expires {
                    return Err(invalid("policy renewal would shorten its expiry"));
                }
                next.expires = expires;
                // No delegation management in an ordinary renewal operation.
                (Body::Targets(next), None)
            }
            Change::Snapshot {
                previous,
                targets,
                release,
                nightly,
            } => {
                let top: Targets = self.root_role(targets.as_bytes())?;
                let mut policy = top.clone();
                policy.version = self.targets.version;
                policy.expires = self.targets.expires;
                if canonical(&policy)? != canonical(&self.targets)?
                    || top.version < self.targets.version
                    || (top.version == self.targets.version
                        && Sha256::digest(targets.as_bytes()) != self.targets_hash)
                {
                    return Err(invalid(
                        "snapshot cannot change or roll back the selected delegation policy",
                    ));
                }
                let release_role = self.channel_role(Channel::Release, release.as_bytes())?;
                let nightly_role = self.channel_role(Channel::Nightly, nightly.as_bytes())?;
                let mut images =
                    crate::repository::channel_images(Channel::Release, &release_role)?;
                images.extend(crate::repository::channel_images(
                    Channel::Nightly,
                    &nightly_role,
                )?);
                image_identities(&images)?;
                for expiration in [top.expires, release_role.expires, nightly_role.expires] {
                    if expiration <= now.0 || expiration > expires {
                        return Err(invalid(
                            "snapshot component has expired or exceeds the metadata lifetime",
                        ));
                    }
                }
                let prior: Option<Snapshot> = previous
                    .as_ref()
                    .map(|bytes| self.root_role(bytes.as_bytes()))
                    .transpose()?;
                let mut next = Snapshot::new(
                    "1.0.0".into(),
                    next_version(prior.as_ref().map(|p| p.version), request.version)?,
                    expires,
                );
                next.meta = HashMap::from([
                    (
                        "targets.json".into(),
                        metafile(top.version, targets.as_bytes()),
                    ),
                    (
                        "release.json".into(),
                        metafile(release_role.version, release.as_bytes()),
                    ),
                    (
                        "nightly.json".into(),
                        metafile(nightly_role.version, nightly.as_bytes()),
                    ),
                ]);
                if let Some(prior) = prior {
                    advance_refs(&prior.meta, &next.meta)?;
                }
                (Body::Snapshot(next), previous)
            }
            Change::Timestamp { previous, snapshot } => {
                let snapshot_role: Snapshot = self.root_role(snapshot.as_bytes())?;
                if snapshot_role.expires <= now.0 || snapshot_role.expires > expires {
                    return Err(invalid(
                        "timestamp snapshot has expired or exceeds its lifetime",
                    ));
                }
                let prior: Option<tough::schema::Timestamp> = previous
                    .as_ref()
                    .map(|bytes| self.root_role(bytes.as_bytes()))
                    .transpose()?;
                let mut next = tough::schema::Timestamp::new(
                    "1.0.0".into(),
                    next_version(prior.as_ref().map(|p| p.version), request.version)?,
                    after(request.prepared_at, 7 * DAY)?,
                );
                next.meta.insert(
                    "snapshot.json".into(),
                    metafile(snapshot_role.version, snapshot.as_bytes()),
                );
                if let Some(prior) = prior {
                    advance_refs(&prior.meta, &next.meta)?;
                }
                (Body::Timestamp(next), previous)
            }
        };
        let approval_bytes = canonical(&serde_json::json!({
            "schema": 1, "authority": self.fingerprint,
            "prepared_at": request.prepared_at, "role": body.name(),
            "previous_sha256": previous.map(|bytes| Sha256::digest(bytes.as_bytes())),
            "signed": body.value()?,
        }))?;
        if approval_bytes.len() > MAX_METADATA_BYTES {
            return Err(invalid("signing proposal exceeds metadata limit"));
        }
        Ok(Proposal {
            authority: self.fingerprint,
            prepared_at: request.prepared_at,
            body,
            approval_bytes,
        })
    }

    /// Reconstruct an already signed response after an interrupted collection.
    /// This permits verification after the approval's one-day signing window,
    /// while enforcing current authority and the original metadata expiry.
    /// sign() still refuses to read credentials for an old proposal.
    pub fn prepare_verification(&self, request: Request, now: UsableTime) -> Result<Proposal> {
        self.current(now)?;
        let prepared_at = request.prepared_at;
        if prepared_at > now.0 {
            return Err(invalid("signed response is future-dated"));
        }
        let proposal = self.prepare(request, UsableTime(prepared_at))?;
        if proposal.body.expires() <= now.0 {
            return Err(invalid("signed response metadata has expired"));
        }
        Ok(proposal)
    }

    /// Keys are first read after approval, current authority and proposal age
    /// checks. Reject every foreign or duplicate credential, even if enough
    /// other keys could satisfy the requested role's signature threshold.
    pub async fn sign(
        &self,
        proposal: &Proposal,
        approved: Sha256,
        keys: &[Box<dyn KeySource>],
        now: UsableTime,
    ) -> Result<Vec<u8>> {
        if proposal.authority != self.fingerprint || proposal.digest() != approved {
            return Err(invalid(
                "proposal differs from independently admitted approval",
            ));
        }
        self.current(now)?;
        fresh(proposal.prepared_at, now)?;
        let (holder, allowed) = match &proposal.body {
            Body::Channel(channel, _) => {
                let role = self
                    .delegations()
                    .roles
                    .iter()
                    .find(|r| r.name == channel.name())
                    .unwrap();
                (
                    KeyHolder::Delegations(self.delegations().clone()),
                    RoleKeys {
                        keyids: role.keyids.clone(),
                        threshold: role.threshold,
                        _extra: HashMap::new(),
                    },
                )
            }
            body => {
                let role = match body {
                    Body::Targets(_) => RoleType::Targets,
                    Body::Snapshot(_) => RoleType::Snapshot,
                    Body::Timestamp(_) => RoleType::Timestamp,
                    Body::Channel(..) => unreachable!(),
                };
                (
                    KeyHolder::Root(self.root.clone()),
                    self.root
                        .roles
                        .get(&role)
                        .ok_or_else(|| invalid("missing signing authority"))?
                        .clone(),
                )
            }
        };
        if keys.len() < allowed.threshold.get() as usize || keys.len() > allowed.keyids.len() {
            return Err(invalid("credential count does not satisfy this role"));
        }
        let mut loaded: Vec<Box<dyn KeySource>> = Vec::new();
        let mut ids = BTreeSet::new();
        for source in keys {
            let signer = source
                .as_sign()
                .await
                .map_err(|_| invalid("signing credential could not be loaded"))?;
            let id = signer
                .tuf_key()
                .key_id()
                .map_err(|_| invalid("invalid public signing key"))?;
            if !allowed.keyids.contains(&id) || !ids.insert(id.as_ref().to_vec()) {
                return Err(invalid("foreign or repeated signing credential"));
            }
            loaded.push(Box::new(Loaded(Arc::from(signer))));
        }
        let bytes = match &proposal.body {
            Body::Channel(channel, targets) => {
                sign(
                    DelegatedTargets {
                        name: channel.name().into(),
                        targets: targets.clone(),
                    },
                    &holder,
                    &loaded,
                )
                .await?
            }
            Body::Targets(targets) => sign(targets.clone(), &holder, &loaded).await?,
            Body::Snapshot(snapshot) => sign(snapshot.clone(), &holder, &loaded).await?,
            Body::Timestamp(timestamp) => sign(timestamp.clone(), &holder, &loaded).await?,
        };
        self.verify_signed(proposal, approved, &bytes, now)?;
        Ok(bytes)
    }

    /// Check a separately produced signature before retaining its exact bytes.
    /// This reads no credentials and does not authorize the proposal itself.
    pub fn verify_signed(
        &self,
        proposal: &Proposal,
        approved: Sha256,
        bytes: &[u8],
        now: UsableTime,
    ) -> Result<()> {
        if proposal.authority != self.fingerprint || proposal.digest() != approved {
            return Err(invalid(
                "signed response differs from independently admitted approval",
            ));
        }
        self.current(now)?;
        if proposal.prepared_at > now.0 || proposal.body.expires() <= now.0 {
            return Err(invalid("signed response is future-dated or expired"));
        }
        let value: serde_json::Value = parse(bytes)?;
        if canonical(&value["signed"])? != canonical(&proposal.body.value()?)? {
            return Err(invalid("signed response differs from the exact proposal"));
        }
        match &proposal.body {
            Body::Channel(channel, _) => {
                self.channel_role(*channel, bytes)?;
            }
            Body::Targets(_) => {
                self.root_role::<Targets>(bytes)?;
            }
            Body::Snapshot(_) => {
                self.root_role::<Snapshot>(bytes)?;
            }
            Body::Timestamp(_) => {
                self.root_role::<tough::schema::Timestamp>(bytes)?;
            }
        }
        Ok(())
    }
}

impl Body {
    fn expires(&self) -> Timestamp {
        match self {
            Self::Channel(_, targets) | Self::Targets(targets) => targets.expires,
            Self::Snapshot(snapshot) => snapshot.expires,
            Self::Timestamp(timestamp) => timestamp.expires,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Channel(channel, _) => channel.name(),
            Self::Targets(_) => "targets",
            Self::Snapshot(_) => "snapshot",
            Self::Timestamp(_) => "timestamp",
        }
    }
    fn value(&self) -> Result<serde_json::Value> {
        Ok(match self {
            Self::Channel(_, targets) | Self::Targets(targets) => serde_json::to_value(targets)?,
            Self::Snapshot(snapshot) => serde_json::to_value(snapshot)?,
            Self::Timestamp(timestamp) => serde_json::to_value(timestamp)?,
        })
    }
}

fn image_identities(images: &[ImageDescriptor]) -> Result<()> {
    let mut generations = HashMap::new();
    let mut payloads = HashMap::new();
    for image in images {
        let mut identity = image.clone();
        identity.channel = Channel::Release;
        if generations
            .insert(image.generation, identity.candidate_id())
            .is_some_and(|old| old != identity.candidate_id())
            || payloads
                .insert(image.compressed.sha256, image.generation)
                .is_some_and(|old| old != image.generation)
        {
            return Err(invalid(
                "image generation or immutable identity was reassigned",
            ));
        }
    }
    Ok(())
}

fn advance_refs(old: &HashMap<String, Metafile>, new: &HashMap<String, Metafile>) -> Result<()> {
    if old.len() != new.len() {
        return Err(invalid("unexpected parent metadata references"));
    }
    for (name, current) in new {
        let prior = old
            .get(name)
            .ok_or_else(|| invalid("missing parent metadata reference"))?;
        if current.version < prior.version || (current.version == prior.version && current != prior)
        {
            return Err(invalid(
                "metadata reference rolled back or reused a version for different bytes",
            ));
        }
    }
    Ok(())
}

fn hashes(sha256: Sha256) -> Hashes {
    Hashes {
        sha256: sha256.as_bytes().to_vec().into(),
        _extra: HashMap::new(),
    }
}
fn metafile(version: NonZeroU64, bytes: &[u8]) -> Metafile {
    Metafile {
        version,
        length: Some(bytes.len() as u64),
        hashes: Some(hashes(Sha256::digest(bytes))),
        _extra: HashMap::new(),
    }
}
fn next_version(previous: Option<NonZeroU64>, reserved: Option<NonZeroU64>) -> Result<NonZeroU64> {
    let next = previous
        .map_or(0, NonZeroU64::get)
        .checked_add(1)
        .and_then(NonZeroU64::new)
        .ok_or_else(|| invalid("metadata version exhausted"))?;
    match reserved {
        Some(version) if version < next => {
            Err(invalid("reserved metadata version must advance the parent"))
        }
        Some(version) => Ok(version),
        None => Ok(next),
    }
}
fn fresh(prepared: Timestamp, now: UsableTime) -> Result<()> {
    if prepared > now.0 || now.0 > after(prepared, DAY)? {
        return Err(invalid("proposal is future-dated or older than 24 hours"));
    }
    Ok(())
}
fn after(now: Timestamp, seconds: i64) -> Result<Timestamp> {
    now.checked_add(std::time::Duration::from_secs(seconds as u64))
        .map_err(|_| invalid("metadata expiration out of range"))
}
fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(invalid("metadata exceeds size limit"));
    }
    Ok(serde_json::from_slice(bytes)?)
}
fn canonical(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    value.serialize(&mut serde_json::Serializer::with_formatter(
        &mut bytes,
        olpc_cjson::CanonicalFormatter::new(),
    ))?;
    Ok(bytes)
}
fn invalid(reason: &str) -> Error {
    Error::Feed(reason.into())
}

#[derive(Clone)]
struct Loaded(Arc<dyn Sign>);
impl std::fmt::Debug for Loaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("loaded signing credential")
    }
}
#[async_trait]
impl Sign for Loaded {
    fn tuf_key(&self) -> tough::schema::key::Key {
        self.0.tuf_key()
    }
    async fn sign(
        &self,
        message: &[u8],
        rng: &(dyn SecureRandom + Sync),
    ) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        self.0.sign(message, rng).await
    }
}
#[async_trait]
impl KeySource for Loaded {
    async fn as_sign(
        &self,
    ) -> std::result::Result<Box<dyn Sign>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Box::new(self.clone()))
    }
    async fn write(
        &self,
        _: &str,
        _: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("signing credentials are read-only".into())
    }
}
async fn sign<T: Role + Serialize>(
    role: T,
    holder: &KeyHolder,
    keys: &[Box<dyn KeySource>],
) -> Result<Vec<u8>> {
    let signed = SignedRole::new(role, holder, keys, &SystemRandom::new())
        .await
        .map_err(|_| invalid("role signing failed"))?;
    // Stable object/signature order avoids inventing different immutable bytes
    // just because hash-map iteration changed. Randomized signing algorithms
    // still require retaining the original signed output across a retry.
    let mut signatures = signed.signed().signatures.clone();
    signatures.sort_by(|a, b| a.keyid.as_ref().cmp(b.keyid.as_ref()));
    let mut bytes = canonical(&Signed {
        signed: &signed.signed().signed,
        signatures,
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[cfg(test)]
#[path = "signing_tests.rs"]
mod tests;
