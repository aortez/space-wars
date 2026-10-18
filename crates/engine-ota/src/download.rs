//! A single bounded check-and-stage session. Scheduling, network admission and
//! installation policy belong to the device service; this worker never installs.
//! Run on a dedicated IO thread/runtime: staging and TUF persistence use blocking
//! filesystem calls. The service must create the session before admitting its
//! connection, monitor its hold independently, and release that hold on all exits.
use crate::staging_linux::{ResponseHeaders, StagingStore, TRANSFER_CHUNK_BYTES, Transfer};
use crate::{
    Channel, DeviceIdentity, Eligibility, Error, ImageDescriptor, ImageHistory, MAX_METADATA_BYTES,
    Result, Selection, Sha256, TrustStore, UsableTime, VerifiedCandidate,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::stream;
use reqwest::{Client, Response, StatusCode, header};
use serde::{Deserialize, Serialize};
use std::error::Error as _;
use std::future::Future;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tough::{Transport, TransportError, TransportErrorKind, TransportStream};
use url::Url;

const PREPARATION_MS: u64 = 120_000;
const SESSION_MS: u64 = 30 * 60_000;
const PROGRESS_MS: u64 = 60_000;
const MAX_HTTP_REQUESTS: usize = 256;
const MAX_REDIRECTS: usize = 5;
const PAYLOAD_BYTES_PER_SECOND: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Checking,
    Downloading,
    Verifying,
    Ready,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Progress {
    pub phase: Phase,
    pub candidate_id: Option<Sha256>,
    pub bytes: u64,
    pub total: u64,
}

/// Fast local observations only. The service revokes this guard when selection,
/// live identity/history, usable time, cancellation or its connection hold
/// changes. A callback must not perform blocking network/D-Bus work. Hold renewal
/// runs independently so a stalled server cannot prevent renewal or cancellation.
pub trait Control: Send + Sync {
    fn checkpoint(&self) -> Result<Option<UsableTime>>;
    fn progress(&self, progress: Progress);
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Outcome {
    Ready {
        candidate: VerifiedCandidate,
    },
    NoCandidate,
    Ineligible {
        image: ImageDescriptor,
        reason: Eligibility,
    },
}

/// Consumed by run: a finished session cannot be reused to reset request or
/// elapsed-time budgets. Construct before the service requests network admission.
pub struct DownloadSession {
    http: Http,
}

struct Budget {
    started: u64,
    requests: AtomicUsize,
    control: Arc<dyn Control>,
    latest_time: Mutex<Option<UsableTime>>,
    #[cfg(test)]
    limits: TestLimits,
    #[cfg(test)]
    clock_offset: Arc<std::sync::atomic::AtomicU64>,
}

#[derive(Clone)]
struct Http {
    client: Client,
    budget: Arc<Budget>,
    #[cfg(test)]
    fixture: Option<Url>,
}

impl std::fmt::Debug for Http {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("bounded update HTTPS transport")
    }
}

#[derive(Clone, Copy)]
enum Deadline {
    Preparation,
    Session,
    Progress(u64),
}

impl DownloadSession {
    pub fn new(control: Arc<dyn Control>) -> Result<Self> {
        Self::new_at(control, boottime_ms()?)
    }

    /// The service supplies its original boot-relative request receipt so queue
    /// and preparation time remain part of the same session. No UI path or
    /// unvalidated wall-clock timestamp participates in these bounds.
    pub fn new_at(control: Arc<dyn Control>, started: u64) -> Result<Self> {
        let now = boottime_ms()?;
        if now
            .checked_sub(started)
            .is_none_or(|age| age >= PREPARATION_MS)
        {
            return Err(Error::Download("preparation deadline exceeded"));
        }
        let session = Self {
            http: Http {
                client: client(true)?,
                budget: Arc::new(Budget {
                    started,
                    requests: AtomicUsize::new(0),
                    control,
                    latest_time: Mutex::new(None),
                    #[cfg(test)]
                    limits: TestLimits::default(),
                    #[cfg(test)]
                    clock_offset: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                }),
                #[cfg(test)]
                fixture: None,
            },
        };
        session.http.budget.check_deadline(Deadline::Preparation)?;
        Ok(session)
    }

    pub async fn run(
        self,
        trust: &mut TrustStore,
        staging: &mut StagingStore,
        channel: Channel,
        device: &DeviceIdentity,
        history: &ImageHistory,
    ) -> Result<Outcome> {
        let result = self
            .run_inner(trust, staging, channel, device, history)
            .await;
        let final_guard = if result.is_ok() {
            self.http.budget.check(Deadline::Session).map(|_| ())
        } else {
            Ok(())
        };
        // Controlled cancellation and network failure must not forget a later
        // usable time observed during the request. The service cancels through
        // Control and awaits this return before releasing its connection hold.
        if let Some(time) = *self
            .http
            .budget
            .latest_time
            .lock()
            .expect("download time mutex")
        {
            trust.admit_time(time)?;
        }
        final_guard?;
        let outcome = result?;
        self.http.budget.check_deadline(Deadline::Session)?;
        if let Outcome::Ready { candidate } = &outcome {
            self.http.budget.report(
                Phase::Ready,
                Some(candidate.id()),
                candidate.image().compressed.bytes,
                candidate.image().compressed.bytes,
            );
        }
        Ok(outcome)
    }

    async fn run_inner(
        &self,
        trust: &mut TrustStore,
        staging: &mut StagingStore,
        channel: Channel,
        device: &DeviceIdentity,
        history: &ImageHistory,
    ) -> Result<Outcome> {
        let budget = &self.http.budget;
        let time = budget.check(Deadline::Preparation)?;
        budget.report(Phase::Checking, None, 0, 0);
        let selection = trust
            .refresh(
                Arc::new(self.http.clone()),
                Some(time),
                channel,
                device,
                history,
            )
            .await
            .map_err(feed_error)?;
        let time = budget.check(Deadline::Preparation)?;
        let candidate = match selection {
            Selection::Available { candidate } => candidate,
            Selection::NoCandidate => {
                staging.clear()?;
                return Ok(Outcome::NoCandidate);
            }
            Selection::Ineligible { image, reason } => {
                staging.clear()?;
                return Ok(Outcome::Ineligible { image, reason });
            }
        };
        if time.0 >= candidate.expires() {
            trust.admit_time(time)?;
            return Err(Error::Download("metadata expired during preparation"));
        }
        // Consistent-snapshot target transport prepends the digest to the
        // signed channel-scoped name. No pathname or URL comes from the UI.
        let target = trust
            .state
            .location
            .targets
            .join(&format!(
                "{}.{}",
                candidate.image().compressed.sha256,
                candidate.image().target_name()
            ))
            .map_err(|_| Error::Download("invalid immutable target location"))?;
        let mut transfer = staging.prepare(&candidate)?;
        budget.report(
            Phase::Downloading,
            Some(candidate.id()),
            transfer.offset(),
            transfer.total(),
        );
        if transfer.offset() < transfer.total() {
            let result = self.http.payload(target, &candidate, &mut transfer).await;
            // Do not hide a failed persistence checkpoint behind a successful
            // transfer; an unsuccessful network result remains unsuccessful.
            let checkpoint = transfer.checkpoint();
            result?;
            checkpoint?;
        }
        budget.check(Deadline::Session)?;
        budget.report(
            Phase::Verifying,
            Some(candidate.id()),
            transfer.offset(),
            transfer.total(),
        );
        let verified = transfer
            .finish(trust, device, history, || {
                budget.check(Deadline::Session).map(Some)
            })
            .await
            .map_err(feed_error)?;
        budget.check(Deadline::Session)?;
        Ok(Outcome::Ready {
            candidate: verified,
        })
    }
}

fn client(https_only: bool) -> Result<Client> {
    client_builder(https_only)
        .build()
        .map_err(|_| Error::Download("cannot initialize HTTPS client"))
}

fn client_builder(https_only: bool) -> reqwest::ClientBuilder {
    Client::builder()
        .https_only(https_only)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .referer(false)
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .http1_only()
        .http1_max_headers(32)
        .pool_max_idle_per_host(0)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_millis(PROGRESS_MS))
        .user_agent("SpaceWars-OTA/1")
}

impl Budget {
    fn now(&self) -> Result<u64> {
        let now = boottime_ms()?;
        #[cfg(test)]
        let now = now.saturating_add(self.clock_offset.load(Ordering::SeqCst));
        Ok(now)
    }

    fn deadline(&self, kind: Deadline) -> u64 {
        #[cfg(not(test))]
        let (prepare, total, progress) = (PREPARATION_MS, SESSION_MS, PROGRESS_MS);
        #[cfg(test)]
        let (prepare, total, progress) =
            (self.limits.prepare, self.limits.total, self.limits.progress);
        let end = self.started.saturating_add(total);
        match kind {
            Deadline::Preparation => end.min(self.started.saturating_add(prepare)),
            Deadline::Session => end,
            Deadline::Progress(last) => end.min(last.saturating_add(progress)),
        }
    }

    fn check_deadline(&self, kind: Deadline) -> Result<()> {
        let now = self.now()?;
        if now < self.started || now >= self.deadline(kind) {
            return Err(Error::Download(match kind {
                Deadline::Preparation => "preparation deadline exceeded",
                Deadline::Session => "session deadline exceeded",
                Deadline::Progress(_) if now >= self.deadline(Deadline::Session) => {
                    "session deadline exceeded"
                }
                Deadline::Progress(_) => "payload made no progress before its deadline",
            }));
        }
        Ok(())
    }

    fn check(&self, kind: Deadline) -> Result<UsableTime> {
        self.check_deadline(kind)?;
        let time = self.control.checkpoint()?.ok_or(Error::WaitingForTime)?;
        let mut latest = self.latest_time.lock().expect("download time mutex");
        if latest.is_some_and(|prior| time.0 < prior.0) {
            return Err(Error::TimeMovedBackwards);
        }
        *latest = Some(time);
        Ok(time)
    }

    async fn wait<T>(&self, kind: Deadline, future: impl Future<Output = T>) -> Result<T> {
        tokio::pin!(future);
        loop {
            self.check(kind)?;
            let left = self.deadline(kind).saturating_sub(self.now()?);
            tokio::select! {
                biased;
                _ = tokio::time::sleep(Duration::from_millis(left.clamp(1, 250))) => {},
                value = &mut future => { self.check(kind)?; return Ok(value); },
            }
        }
    }

    fn report(&self, phase: Phase, candidate_id: Option<Sha256>, bytes: u64, total: u64) {
        self.control.progress(Progress {
            phase,
            candidate_id,
            bytes,
            total,
        });
    }
}

impl Http {
    async fn get(
        &self,
        mut url: Url,
        range: Option<(u64, &str)>,
        deadline: Deadline,
    ) -> Result<Response> {
        for redirects in 0..=MAX_REDIRECTS {
            validate_url(&url)?;
            self.budget.check(deadline)?;
            if self.budget.requests.fetch_add(1, Ordering::Relaxed) >= MAX_HTTP_REQUESTS {
                return Err(Error::Download("HTTP request budget exhausted"));
            }
            let mut request = self
                .client
                .get(self.request_url(&url)?)
                .header(header::ACCEPT_ENCODING, "identity");
            if let Some((offset, etag)) = range {
                request = request
                    .header(header::RANGE, format!("bytes={offset}-"))
                    .header(header::IF_RANGE, etag);
            }
            let response = self
                .budget
                .wait(deadline, request.send())
                .await?
                .map_err(|_| Error::Download("HTTPS request failed"))?;
            if matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                if redirects == MAX_REDIRECTS {
                    return Err(Error::Download("redirect budget exhausted"));
                }
                let location = one(response.headers(), header::LOCATION)?
                    .filter(|location| location.len() <= 8192)
                    .ok_or(Error::Download("invalid redirect"))?;
                url = url
                    .join(location)
                    .map_err(|_| Error::Download("invalid redirect"))?;
                // Drop the response without reading its body. The next loop
                // revalidates scheme/credentials before sending any request.
                continue;
            }
            return Ok(response);
        }
        unreachable!()
    }

    fn request_url(&self, url: &Url) -> Result<Url> {
        #[cfg(test)]
        if let Some(base) = &self.fixture {
            if url.origin() != Url::parse("https://updates.example/").unwrap().origin() {
                return Err(Error::Download("fixture refused an external origin"));
            }
            let mut mapped = base.clone();
            mapped.set_path(url.path());
            mapped.set_query(url.query());
            return Ok(mapped);
        }
        Ok(url.clone())
    }

    async fn payload(
        &self,
        url: Url,
        candidate: &VerifiedCandidate,
        transfer: &mut Transfer<'_>,
    ) -> Result<()> {
        let mut last_progress = self.budget.now()?;
        for attempt in 0..2 {
            let offset = transfer.offset();
            let range = if offset > 0 {
                Some((offset, transfer.etag().ok_or(Error::DownloadResponse)?))
            } else {
                None
            };
            let mut response = self
                .get(url.clone(), range, Deadline::Progress(last_progress))
                .await?;
            if response.status().is_server_error() {
                return Err(Error::Download("image server failed"));
            }
            if ![200, 206, 416].contains(&response.status().as_u16()) {
                return Err(Error::Download("image server refused the request"));
            }
            let admitted = response_headers(&response).and_then(|headers| transfer.accept(headers));
            match admitted {
                Ok(()) => {}
                Err(Error::DownloadResponse) if offset > 0 && attempt == 0 => {
                    // An inconsistent suffix cannot poison the next attempt or
                    // reset the session/no-progress budget. One full restart.
                    transfer.restart()?;
                    continue;
                }
                Err(error) => return Err(error),
            }
            while let Some(chunk) = self
                .budget
                .wait(Deadline::Progress(last_progress), response.chunk())
                .await?
                .map_err(|_| Error::Download("image response interrupted"))?
            {
                if chunk.is_empty() {
                    continue;
                }
                for bytes in chunk.chunks(TRANSFER_CHUNK_BYTES) {
                    self.budget.check(Deadline::Progress(last_progress))?;
                    transfer.append(bytes)?;
                    last_progress = self.budget.now()?;
                    self.budget.report(
                        Phase::Downloading,
                        Some(candidate.id()),
                        transfer.offset(),
                        transfer.total(),
                    );
                    // No accumulated burst credit: fast links remain bounded
                    // even after a slow connection or a long filesystem flush.
                    let pause = (bytes.len() as u64 * 1000).div_ceil(PAYLOAD_BYTES_PER_SECOND);
                    self.budget
                        .wait(
                            Deadline::Progress(last_progress),
                            tokio::time::sleep(Duration::from_millis(pause)),
                        )
                        .await?;
                }
            }
            if transfer.offset() != transfer.total() {
                return Err(Error::ImageMismatch);
            }
            return Ok(());
        }
        Err(Error::DownloadResponse)
    }
}

#[async_trait]
impl Transport for Http {
    async fn fetch(&self, url: Url) -> std::result::Result<TransportStream, TransportError> {
        let result: Result<_> = async {
            let mut response = self.get(url, None, Deadline::Preparation).await?;
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if response.status() != StatusCode::OK
                || one(response.headers(), header::CONTENT_ENCODING)?
                    .is_some_and(|value| value != "identity")
                || one(response.headers(), header::CONTENT_RANGE)?.is_some()
                || response
                    .content_length()
                    .is_some_and(|length| length > MAX_METADATA_BYTES as u64)
            {
                return Err(Error::Download("invalid metadata response"));
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = self
                .budget
                .wait(Deadline::Preparation, response.chunk())
                .await?
                .map_err(|_| Error::Download("metadata response interrupted"))?
            {
                if chunk.len() > MAX_METADATA_BYTES.saturating_sub(bytes.len()) {
                    return Err(Error::Download("metadata response exceeds its size limit"));
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(Some(bytes))
        }
        .await;
        match result {
            Ok(Some(bytes)) => Ok(Box::pin(stream::once(std::future::ready(Ok(Bytes::from(
                bytes,
            )))))),
            Ok(None) => Err(TransportError::new(
                TransportErrorKind::FileNotFound,
                "[update metadata]",
            )),
            Err(error) => Err(TransportError::new_with_cause(
                TransportErrorKind::Other,
                "[update metadata]",
                error,
            )),
        }
    }
}

fn one(headers: &header::HeaderMap, name: header::HeaderName) -> Result<Option<&str>> {
    let mut values = headers.get_all(name).iter();
    let value = values
        .next()
        .map(|value| value.to_str().map_err(|_| Error::DownloadResponse))
        .transpose()?;
    if values.next().is_some() {
        return Err(Error::DownloadResponse);
    }
    Ok(value)
}

fn response_headers(response: &Response) -> Result<ResponseHeaders<'_>> {
    let headers = response.headers();
    let length = one(headers, header::CONTENT_LENGTH)?
        .map(|value| {
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(Error::DownloadResponse);
            }
            value.parse::<u64>().map_err(|_| Error::DownloadResponse)
        })
        .transpose()?;
    Ok(ResponseHeaders {
        status: response.status().as_u16(),
        length,
        range: one(headers, header::CONTENT_RANGE)?,
        etag: one(headers, header::ETAG)?,
        encoding: one(headers, header::CONTENT_ENCODING)?,
    })
}

fn validate_url(url: &Url) -> Result<()> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.as_str().len() > 8192
    {
        return Err(Error::Download(
            "redirect or request is not credential-free HTTPS",
        ));
    }
    Ok(())
}

fn feed_error(error: Error) -> Error {
    // TUF's detailed errors can contain its input URLs. Keep transport reasons
    // that we generated ourselves, never the library's URL-bearing rendering.
    if matches!(error, Error::Tuf(_)) {
        let mut source = error.source();
        while let Some(cause) = source {
            if let Some(error) = cause.downcast_ref::<Error>() {
                match error {
                    Error::Download(reason) => return Error::Download(reason),
                    Error::WaitingForTime => return Error::WaitingForTime,
                    Error::TimeMovedBackwards => return Error::TimeMovedBackwards,
                    Error::CandidateChanged => return Error::CandidateChanged,
                    _ => {}
                }
            }
            source = cause.source();
        }
        return Error::Download("signed metadata verification failed");
    }
    match error {
        Error::Feed(_) | Error::Descriptor(_) | Error::Json(_) => {
            Error::Download("invalid signed metadata")
        }
        other => other,
    }
}

fn boottime_ms() -> Result<u64> {
    let mut clock = std::mem::MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: the pointer is valid and clock_gettime initializes it on success.
    if unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, clock.as_mut_ptr()) } != 0 {
        return Err(Error::Download("boot clock unavailable"));
    }
    let clock = unsafe { clock.assume_init() };
    let seconds = u64::try_from(clock.tv_sec).map_err(|_| Error::Download("invalid boot clock"))?;
    let nanos = u64::try_from(clock.tv_nsec).map_err(|_| Error::Download("invalid boot clock"))?;
    if nanos >= 1_000_000_000 {
        return Err(Error::Download("invalid boot clock"));
    }
    seconds
        .checked_mul(1000)
        .and_then(|seconds| seconds.checked_add(nanos / 1_000_000))
        .ok_or(Error::Download("boot clock overflow"))
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct TestLimits {
    prepare: u64,
    total: u64,
    progress: u64,
}

#[cfg(test)]
impl Default for TestLimits {
    fn default() -> Self {
        Self {
            prepare: PREPARATION_MS,
            total: SESSION_MS,
            progress: PROGRESS_MS,
        }
    }
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
