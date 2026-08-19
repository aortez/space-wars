//! Bounded realtime ownership and handoffs for synchronous NES scenarios.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use engine_common::{NativePixelFormat, NativeVideoCrop, NativeVideoFrame};
use engine_nes::{
    ControllerButtons, FrameInput, NTSC_MASTER_CLOCK_NUMERATOR_HZ, NTSC_PPU_CLOCK_DENOMINATOR,
};

use crate::nes_audio::{CpalAudioOutput, RealtimeAudioEndpoint};

const VIDEO_SLOT_COUNT: usize = 3;
const NO_VIDEO_SLOT: usize = usize::MAX;
const MAX_CATCH_UP_FRAMES: usize = 4;

pub(crate) type FrameWaker = Arc<dyn Fn() + Send + Sync + 'static>;

/// Borrowed output from one synchronous scenario frame.
pub(crate) struct RealtimeNesFrame<'a> {
    pub video: NativeVideoFrame<'a>,
    pub audio_samples: &'a [i16],
    pub frame_ppu_clocks: u64,
}

/// Minimal boundary required by the generic worker. Implementations retain
/// ownership of their scenario state and advance through its normal frame API.
pub(crate) trait RealtimeNesCore: Send + 'static {
    fn current_frame(&self) -> RealtimeNesFrame<'_>;
    fn advance_frame(&mut self, input: FrameInput) -> Result<RealtimeNesFrame<'_>, String>;
}

#[derive(Debug)]
pub(crate) enum RealtimeStartError {
    InvalidInitialVideo,
    ThreadSpawn(std::io::Error),
}

impl fmt::Display for RealtimeStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInitialVideo => {
                formatter.write_str("realtime scenario exposed an invalid initial video frame")
            }
            Self::ThreadSpawn(error) => write!(formatter, "could not spawn NES worker: {error}"),
        }
    }
}

impl std::error::Error for RealtimeStartError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ThreadSpawn(error) => Some(error),
            Self::InvalidInitialVideo => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VideoHandoffError {
    InvalidFrame,
    FormatChanged,
    DestinationSize { expected: usize, actual: usize },
}

impl fmt::Display for VideoHandoffError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFrame => formatter.write_str("worker produced an invalid native frame"),
            Self::FormatChanged => {
                formatter.write_str("worker changed native video format after startup")
            }
            Self::DestinationSize { expected, actual } => write!(
                formatter,
                "native video destination has {actual} pixels; expected {expected}"
            ),
        }
    }
}

impl std::error::Error for VideoHandoffError {}

#[derive(Debug, Clone)]
pub(crate) struct NativeVideoDescriptor {
    pub width: u32,
    pub height: u32,
    pub visible_crop: NativeVideoCrop,
    pub pixel_format: NativePixelFormat,
    pub palette_rgb565: Arc<[u16]>,
    pixel_count: usize,
}

impl NativeVideoDescriptor {
    fn from_frame(frame: NativeVideoFrame<'_>) -> Result<Self, RealtimeStartError> {
        if !frame.has_valid_layout() {
            return Err(RealtimeStartError::InvalidInitialVideo);
        }
        Ok(Self {
            width: frame.width,
            height: frame.height,
            visible_crop: frame.visible_crop,
            pixel_format: frame.pixel_format,
            palette_rgb565: Arc::from(frame.palette_rgb565),
            pixel_count: frame.pixels.len(),
        })
    }

    pub fn pixel_count(&self) -> usize {
        self.pixel_count
    }

    fn matches(&self, frame: NativeVideoFrame<'_>) -> bool {
        frame.has_valid_layout()
            && frame.width == self.width
            && frame.height == self.height
            && frame.visible_crop == self.visible_crop
            && frame.pixel_format == self.pixel_format
            && frame.pixels.len() == self.pixel_count
            && frame.palette_rgb565 == self.palette_rgb565.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RealtimeVideoMetadata {
    pub generation: u64,
    pub frame_id: u64,
    pub emulated_ticks: u64,
    pub input_sequence_id: u64,
    pub input_observed_at: Duration,
    pub worker_sampled_at: Duration,
    pub frame_completed_at: Duration,
    pub frame_published_at: Duration,
}

impl Default for RealtimeVideoMetadata {
    fn default() -> Self {
        Self {
            generation: 0,
            frame_id: 0,
            emulated_ticks: 0,
            input_sequence_id: 0,
            input_observed_at: Duration::ZERO,
            worker_sampled_at: Duration::ZERO,
            frame_completed_at: Duration::ZERO,
            frame_published_at: Duration::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AppliedInputTelemetry {
    pub sequence_id: u64,
    pub frame_id: u64,
    pub controllers: [ControllerButtons; 2],
    pub observed_at: Duration,
    pub sampled_at: Duration,
    pub frame_completed_at: Duration,
    pub frame_published_at: Duration,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RealtimeTelemetry {
    pub emulated_frames: u64,
    pub produced_video_frames: u64,
    pub consumed_video_frames: u64,
    pub submitted_video_frames: u64,
    pub displayed_loop_iterations: u64,
    pub coalesced_video_frames: u64,
    pub duplicate_video_polls: u64,
    pub wake_requests: u64,
    pub coalesced_wake_requests: u64,
    pub catch_up_rebases: u64,
    pub produced_audio_samples: u64,
    pub audio_available: bool,
    pub audio_active: bool,
    pub audio_primed: bool,
    pub audio_muted: bool,
    pub audio_capacity_samples: usize,
    pub audio_target_depth_samples: usize,
    pub audio_queue_depth_samples: usize,
    pub audio_high_water_samples: usize,
    pub consumed_audio_samples: u64,
    pub audio_underrun_samples: u64,
    pub audio_overflow_samples: u64,
    pub audio_callback_count: u64,
    pub audio_device_errors: u64,
    pub last_produced_frame_id: u64,
    pub last_submitted_frame_id: u64,
    pub latest_input: Option<AppliedInputTelemetry>,
}

#[derive(Debug, Default)]
struct TelemetryCounters {
    emulated_frames: AtomicU64,
    produced_video_frames: AtomicU64,
    consumed_video_frames: AtomicU64,
    submitted_video_frames: AtomicU64,
    displayed_loop_iterations: AtomicU64,
    coalesced_video_frames: AtomicU64,
    duplicate_video_polls: AtomicU64,
    wake_requests: AtomicU64,
    coalesced_wake_requests: AtomicU64,
    catch_up_rebases: AtomicU64,
    produced_audio_samples: AtomicU64,
    last_produced_frame_id: AtomicU64,
    last_submitted_frame_id: AtomicU64,
    latest_input: Mutex<Option<AppliedInputTelemetry>>,
}

impl TelemetryCounters {
    fn snapshot(&self) -> RealtimeTelemetry {
        RealtimeTelemetry {
            emulated_frames: self.emulated_frames.load(Ordering::Relaxed),
            produced_video_frames: self.produced_video_frames.load(Ordering::Relaxed),
            consumed_video_frames: self.consumed_video_frames.load(Ordering::Relaxed),
            submitted_video_frames: self.submitted_video_frames.load(Ordering::Relaxed),
            displayed_loop_iterations: self.displayed_loop_iterations.load(Ordering::Relaxed),
            coalesced_video_frames: self.coalesced_video_frames.load(Ordering::Relaxed),
            duplicate_video_polls: self.duplicate_video_polls.load(Ordering::Relaxed),
            wake_requests: self.wake_requests.load(Ordering::Relaxed),
            coalesced_wake_requests: self.coalesced_wake_requests.load(Ordering::Relaxed),
            catch_up_rebases: self.catch_up_rebases.load(Ordering::Relaxed),
            produced_audio_samples: self.produced_audio_samples.load(Ordering::Relaxed),
            last_produced_frame_id: self.last_produced_frame_id.load(Ordering::Relaxed),
            last_submitted_frame_id: self.last_submitted_frame_id.load(Ordering::Relaxed),
            latest_input: *lock_unpoisoned(&self.latest_input),
            ..RealtimeTelemetry::default()
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct InputRequest {
    sequence_id: u64,
    controllers: [ControllerButtons; 2],
    observed_at: Duration,
}

#[derive(Debug)]
struct InputMailbox {
    latest: Mutex<InputRequest>,
    next_sequence_id: AtomicU64,
    epoch: Instant,
}

impl InputMailbox {
    fn new(epoch: Instant) -> Self {
        Self {
            latest: Mutex::new(InputRequest {
                sequence_id: 0,
                controllers: [ControllerButtons::NONE; 2],
                observed_at: Duration::ZERO,
            }),
            next_sequence_id: AtomicU64::new(1),
            epoch,
        }
    }

    fn publish(&self, controllers: [ControllerButtons; 2], observed_at: Instant) -> u64 {
        let sequence_id = self.next_sequence_id.fetch_add(1, Ordering::Relaxed);
        *lock_unpoisoned(&self.latest) = InputRequest {
            sequence_id,
            controllers,
            observed_at: observed_at.saturating_duration_since(self.epoch),
        };
        sequence_id
    }

    fn neutralize(&self) -> u64 {
        self.publish([ControllerButtons::NONE; 2], Instant::now())
    }

    fn latest(&self) -> InputRequest {
        *lock_unpoisoned(&self.latest)
    }
}

#[derive(Debug)]
struct VideoSlotContent {
    generation: u64,
    metadata: RealtimeVideoMetadata,
    pixels: Box<[u8]>,
}

struct VideoHandoff {
    descriptor: NativeVideoDescriptor,
    slots: [Mutex<VideoSlotContent>; VIDEO_SLOT_COUNT],
    next_generation: AtomicU64,
    latest_generation: AtomicU64,
    latest_slot: AtomicUsize,
    consumed_generation: AtomicU64,
    wake_pending: AtomicBool,
    waker: Mutex<Option<FrameWaker>>,
    telemetry: Arc<TelemetryCounters>,
}

impl fmt::Debug for VideoHandoff {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VideoHandoff")
            .field("descriptor", &self.descriptor)
            .field(
                "latest_generation",
                &self.latest_generation.load(Ordering::Relaxed),
            )
            .field("latest_slot", &self.latest_slot.load(Ordering::Relaxed))
            .field(
                "consumed_generation",
                &self.consumed_generation.load(Ordering::Relaxed),
            )
            .field("wake_pending", &self.wake_pending.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl VideoHandoff {
    fn new(descriptor: NativeVideoDescriptor, telemetry: Arc<TelemetryCounters>) -> Self {
        let pixel_count = descriptor.pixel_count;
        Self {
            descriptor,
            slots: std::array::from_fn(|_| {
                Mutex::new(VideoSlotContent {
                    generation: 0,
                    metadata: RealtimeVideoMetadata::default(),
                    pixels: vec![0; pixel_count].into_boxed_slice(),
                })
            }),
            next_generation: AtomicU64::new(1),
            latest_generation: AtomicU64::new(0),
            latest_slot: AtomicUsize::new(NO_VIDEO_SLOT),
            consumed_generation: AtomicU64::new(0),
            wake_pending: AtomicBool::new(false),
            waker: Mutex::new(None),
            telemetry,
        }
    }

    fn publish(
        &self,
        frame: NativeVideoFrame<'_>,
        mut metadata: RealtimeVideoMetadata,
    ) -> Result<u64, VideoHandoffError> {
        if !frame.has_valid_layout() {
            return Err(VideoHandoffError::InvalidFrame);
        }
        if !self.descriptor.matches(frame) {
            return Err(VideoHandoffError::FormatChanged);
        }

        let previous_generation = self.latest_generation.load(Ordering::Acquire);
        if previous_generation > self.consumed_generation.load(Ordering::Acquire) {
            self.telemetry
                .coalesced_video_frames
                .fetch_add(1, Ordering::Relaxed);
        }

        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let slot_index = (generation.saturating_sub(1) as usize) % VIDEO_SLOT_COUNT;
        metadata.generation = generation;
        {
            let mut slot = lock_unpoisoned(&self.slots[slot_index]);
            slot.pixels.copy_from_slice(frame.pixels);
            slot.metadata = metadata;
            slot.generation = generation;
        }

        self.latest_slot.store(slot_index, Ordering::Release);
        self.latest_generation.store(generation, Ordering::Release);
        self.telemetry
            .produced_video_frames
            .fetch_add(1, Ordering::Relaxed);
        self.telemetry
            .last_produced_frame_id
            .store(frame.frame_id, Ordering::Relaxed);
        self.request_wake();
        Ok(generation)
    }

    fn set_waker(&self, waker: FrameWaker) {
        *lock_unpoisoned(&self.waker) = Some(waker);
        if self.latest_generation.load(Ordering::Acquire)
            > self.consumed_generation.load(Ordering::Acquire)
        {
            self.request_wake();
        }
    }

    fn request_wake(&self) {
        let Some(waker) = lock_unpoisoned(&self.waker).clone() else {
            return;
        };
        if self
            .wake_pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.telemetry.wake_requests.fetch_add(1, Ordering::Relaxed);
            waker();
        } else {
            self.telemetry
                .coalesced_wake_requests
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    fn complete_wakeup(&self, consumed_generation: u64) {
        self.wake_pending.store(false, Ordering::Release);
        if self.latest_generation.load(Ordering::Acquire) > consumed_generation {
            self.request_wake();
        }
    }
}

#[derive(Clone)]
pub(crate) struct RealtimeVideoConsumer {
    handoff: Arc<VideoHandoff>,
}

impl fmt::Debug for RealtimeVideoConsumer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RealtimeVideoConsumer")
            .field("handoff", &self.handoff)
            .finish()
    }
}

impl RealtimeVideoConsumer {
    pub fn descriptor(&self) -> &NativeVideoDescriptor {
        &self.handoff.descriptor
    }

    pub fn set_waker(&self, waker: FrameWaker) {
        self.handoff.set_waker(waker);
    }

    pub fn try_copy_latest(
        &self,
        destination: &mut [u8],
    ) -> Result<Option<RealtimeVideoMetadata>, VideoHandoffError> {
        let expected = self.handoff.descriptor.pixel_count;
        if destination.len() != expected {
            return Err(VideoHandoffError::DestinationSize {
                expected,
                actual: destination.len(),
            });
        }

        let consumed = self.handoff.consumed_generation.load(Ordering::Acquire);
        let latest = self.handoff.latest_generation.load(Ordering::Acquire);
        let slot_index = self.handoff.latest_slot.load(Ordering::Acquire);
        if latest <= consumed || slot_index == NO_VIDEO_SLOT {
            self.handoff
                .telemetry
                .duplicate_video_polls
                .fetch_add(1, Ordering::Relaxed);
            self.handoff.complete_wakeup(consumed);
            return Ok(None);
        }

        let slot = match self.handoff.slots[slot_index].try_lock() {
            Ok(slot) => slot,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
            Err(TryLockError::WouldBlock) => {
                self.handoff
                    .telemetry
                    .duplicate_video_polls
                    .fetch_add(1, Ordering::Relaxed);
                self.handoff.complete_wakeup(consumed);
                return Ok(None);
            }
        };
        if slot.generation <= consumed {
            self.handoff
                .telemetry
                .duplicate_video_polls
                .fetch_add(1, Ordering::Relaxed);
            self.handoff.complete_wakeup(consumed);
            return Ok(None);
        }

        destination.copy_from_slice(&slot.pixels);
        let metadata = slot.metadata;
        drop(slot);
        self.handoff
            .consumed_generation
            .store(metadata.generation, Ordering::Release);
        self.handoff
            .telemetry
            .consumed_video_frames
            .fetch_add(1, Ordering::Relaxed);
        self.handoff.complete_wakeup(metadata.generation);
        Ok(Some(metadata))
    }

    pub fn mark_submitted(&self, frame_id: u64) {
        self.handoff
            .telemetry
            .submitted_video_frames
            .fetch_add(1, Ordering::Relaxed);
        self.handoff
            .telemetry
            .last_submitted_frame_id
            .store(frame_id, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum WorkerMode {
    Paused = 0,
    Running = 1,
    Stopped = 2,
}

impl WorkerMode {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Running,
            2 => Self::Stopped,
            _ => Self::Paused,
        }
    }
}

#[derive(Debug)]
struct WorkerControl {
    mode: AtomicU8,
    wait_lock: Mutex<()>,
    changed: Condvar,
}

impl WorkerControl {
    fn new() -> Self {
        Self {
            mode: AtomicU8::new(WorkerMode::Paused as u8),
            wait_lock: Mutex::new(()),
            changed: Condvar::new(),
        }
    }

    fn mode(&self) -> WorkerMode {
        WorkerMode::from_u8(self.mode.load(Ordering::Acquire))
    }

    fn set_mode(&self, mode: WorkerMode) {
        let _guard = lock_unpoisoned(&self.wait_lock);
        self.mode.store(mode as u8, Ordering::Release);
        self.changed.notify_all();
    }

    fn wait_until_running_or_stopped(&self) -> WorkerMode {
        let mut guard = lock_unpoisoned(&self.wait_lock);
        loop {
            match self.mode() {
                WorkerMode::Paused => guard = wait_unpoisoned(&self.changed, guard),
                mode => return mode,
            }
        }
    }

    fn wait_until_deadline_or_change(&self, deadline: Instant) -> WorkerMode {
        let mut guard = lock_unpoisoned(&self.wait_lock);
        loop {
            let mode = self.mode();
            if mode != WorkerMode::Running {
                return mode;
            }
            let now = Instant::now();
            if now >= deadline {
                return mode;
            }
            let timeout = deadline.saturating_duration_since(now);
            let (next_guard, result) = wait_timeout_unpoisoned(&self.changed, guard, timeout);
            guard = next_guard;
            if result.timed_out() {
                return self.mode();
            }
        }
    }
}

#[derive(Debug)]
struct RuntimeShared {
    epoch: Instant,
    input: InputMailbox,
    video: Arc<VideoHandoff>,
    control: WorkerControl,
    telemetry: Arc<TelemetryCounters>,
    audio: Option<RealtimeAudioEndpoint>,
    runtime_error: Mutex<Option<String>>,
}

pub(crate) struct NesRealtimeRuntime {
    shared: Arc<RuntimeShared>,
    worker: Option<JoinHandle<()>>,
    _audio_device: Option<CpalAudioOutput>,
}

impl fmt::Debug for NesRealtimeRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NesRealtimeRuntime")
            .field("mode", &self.shared.control.mode())
            .field("telemetry", &self.telemetry())
            .finish_non_exhaustive()
    }
}

impl NesRealtimeRuntime {
    pub fn spawn<C: RealtimeNesCore>(core: C) -> Result<Self, RealtimeStartError> {
        Self::spawn_internal(core, None, None)
    }

    pub fn spawn_with_audio<C: RealtimeNesCore>(
        core: C,
        audio_device: CpalAudioOutput,
    ) -> Result<Self, RealtimeStartError> {
        let audio = Some(audio_device.endpoint());
        Self::spawn_internal(core, Some(audio_device), audio)
    }

    #[cfg(test)]
    fn spawn_with_audio_endpoint<C: RealtimeNesCore>(
        core: C,
        audio: RealtimeAudioEndpoint,
    ) -> Result<Self, RealtimeStartError> {
        Self::spawn_internal(core, None, Some(audio))
    }

    fn spawn_internal<C: RealtimeNesCore>(
        core: C,
        audio_device: Option<CpalAudioOutput>,
        audio: Option<RealtimeAudioEndpoint>,
    ) -> Result<Self, RealtimeStartError> {
        let epoch = Instant::now();
        let initial = core.current_frame();
        let descriptor = NativeVideoDescriptor::from_frame(initial.video)?;
        let telemetry = Arc::new(TelemetryCounters::default());
        let video = Arc::new(VideoHandoff::new(descriptor, Arc::clone(&telemetry)));
        let shared = Arc::new(RuntimeShared {
            epoch,
            input: InputMailbox::new(epoch),
            video,
            control: WorkerControl::new(),
            telemetry,
            audio,
            runtime_error: Mutex::new(None),
        });
        publish_initial_frame(&shared, initial.video)
            .map_err(|_| RealtimeStartError::InvalidInitialVideo)?;

        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("spacewars-nes-realtime".into())
            .spawn(move || run_worker(core, worker_shared))
            .map_err(RealtimeStartError::ThreadSpawn)?;

        Ok(Self {
            shared,
            worker: Some(worker),
            _audio_device: audio_device,
        })
    }

    pub fn video_consumer(&self) -> RealtimeVideoConsumer {
        RealtimeVideoConsumer {
            handoff: Arc::clone(&self.shared.video),
        }
    }

    pub fn publish_input(&self, controllers: [ControllerButtons; 2], observed_at: Instant) -> u64 {
        self.shared.input.publish(controllers, observed_at)
    }

    pub fn set_paused(&self, paused: bool) {
        let desired = if paused {
            WorkerMode::Paused
        } else {
            WorkerMode::Running
        };
        if self.shared.control.mode() == desired {
            return;
        }
        if !paused && lock_unpoisoned(&self.shared.runtime_error).is_some() {
            return;
        }
        self.shared.input.neutralize();
        if paused {
            self.shared.control.set_mode(desired);
            if let Some(audio) = &self.shared.audio {
                audio.set_paused(true);
            }
        } else {
            if let Some(audio) = &self.shared.audio {
                audio.set_paused(false);
            }
            self.shared.control.set_mode(desired);
        }
    }

    pub fn record_displayed_loop_iteration(&self) {
        self.shared
            .telemetry
            .displayed_loop_iterations
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_audio_volume(&self, volume: f32) {
        if let Some(audio) = &self.shared.audio {
            audio.set_volume(volume);
        }
    }

    pub fn set_audio_muted(&self, muted: bool) {
        if let Some(audio) = &self.shared.audio {
            audio.set_muted(muted);
        }
    }

    pub fn telemetry(&self) -> RealtimeTelemetry {
        let mut telemetry = self.shared.telemetry.snapshot();
        if let Some(audio) = &self.shared.audio {
            let audio = audio.telemetry();
            telemetry.audio_available = audio.available;
            telemetry.audio_active = audio.active;
            telemetry.audio_primed = audio.primed;
            telemetry.audio_muted = audio.muted;
            telemetry.audio_capacity_samples = audio.capacity_samples;
            telemetry.audio_target_depth_samples = audio.target_depth_samples;
            telemetry.audio_queue_depth_samples = audio.current_depth_samples;
            telemetry.audio_high_water_samples = audio.high_water_samples;
            telemetry.consumed_audio_samples = audio.consumed_samples;
            telemetry.audio_underrun_samples = audio.underrun_samples;
            telemetry.audio_overflow_samples = audio.overflow_samples;
            telemetry.audio_callback_count = audio.callback_count;
            telemetry.audio_device_errors = audio.device_errors;
        }
        telemetry
    }

    pub fn runtime_error(&self) -> Option<String> {
        lock_unpoisoned(&self.shared.runtime_error).clone()
    }

    pub fn stop_and_join(&mut self) {
        if self.worker.is_none() {
            return;
        }
        self.shared.input.neutralize();
        self.shared.control.set_mode(WorkerMode::Stopped);
        if let Some(audio) = &self.shared.audio {
            audio.set_paused(true);
        }
        let worker = self.worker.take().expect("worker was checked above");
        if worker.join().is_err() {
            *lock_unpoisoned(&self.shared.runtime_error) =
                Some("NES realtime worker panicked during shutdown".into());
        }
    }
}

impl Drop for NesRealtimeRuntime {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

fn publish_initial_frame(
    shared: &RuntimeShared,
    frame: NativeVideoFrame<'_>,
) -> Result<(), VideoHandoffError> {
    let timing = frame.timing;
    shared.video.publish(
        frame,
        RealtimeVideoMetadata {
            frame_id: frame.frame_id,
            emulated_ticks: timing.map_or(0, |timing| timing.emulated_ticks),
            input_sequence_id: timing.map_or(0, |timing| timing.input_sequence_id),
            ..RealtimeVideoMetadata::default()
        },
    )?;
    Ok(())
}

fn run_worker<C: RealtimeNesCore>(mut core: C, shared: Arc<RuntimeShared>) {
    loop {
        match shared.control.wait_until_running_or_stopped() {
            WorkerMode::Stopped => return,
            WorkerMode::Paused => continue,
            WorkerMode::Running => {}
        }

        let mut pacer = RationalPacer::new(Instant::now());
        while shared.control.mode() == WorkerMode::Running {
            if shared
                .control
                .wait_until_deadline_or_change(pacer.deadline())
                != WorkerMode::Running
            {
                break;
            }

            let mut catch_up_frames = 0;
            while shared.control.mode() == WorkerMode::Running
                && Instant::now() >= pacer.deadline()
                && catch_up_frames < MAX_CATCH_UP_FRAMES
            {
                let input = shared.input.latest();
                let sampled_at = shared.epoch.elapsed();
                let output = match core
                    .advance_frame(FrameInput::new(input.sequence_id, input.controllers))
                {
                    Ok(output) => output,
                    Err(error) => {
                        pause_after_error(&shared, error);
                        break;
                    }
                };
                let frame_completed_at = shared.epoch.elapsed();
                shared
                    .telemetry
                    .emulated_frames
                    .fetch_add(1, Ordering::Relaxed);
                // A lifecycle transition may arrive while the synchronous
                // machine frame is executing. The core has reached a clean
                // boundary, but stale post-transition audio/video must not be
                // published into freshly flushed handoffs.
                if shared.control.mode() != WorkerMode::Running {
                    break;
                }
                let timing = output.video.timing;
                let frame_id = output.video.frame_id;
                if let Some(audio) = &shared.audio {
                    audio.push_samples(output.audio_samples);
                }
                let published_at = shared.epoch.elapsed();
                let metadata = RealtimeVideoMetadata {
                    generation: 0,
                    frame_id,
                    emulated_ticks: timing.map_or(0, |timing| timing.emulated_ticks),
                    input_sequence_id: input.sequence_id,
                    input_observed_at: input.observed_at,
                    worker_sampled_at: sampled_at,
                    frame_completed_at,
                    frame_published_at: published_at,
                };
                if let Err(error) = shared.video.publish(output.video, metadata) {
                    pause_after_error(&shared, error.to_string());
                    break;
                }

                shared
                    .telemetry
                    .produced_audio_samples
                    .fetch_add(output.audio_samples.len() as u64, Ordering::Relaxed);
                *lock_unpoisoned(&shared.telemetry.latest_input) = Some(AppliedInputTelemetry {
                    sequence_id: input.sequence_id,
                    frame_id,
                    controllers: input.controllers,
                    observed_at: input.observed_at,
                    sampled_at,
                    frame_completed_at,
                    frame_published_at: published_at,
                });

                if output.frame_ppu_clocks == 0 {
                    pause_after_error(
                        &shared,
                        "NES worker produced a frame with zero PPU clocks".into(),
                    );
                    break;
                }
                pacer.advance_ppu_clocks(output.frame_ppu_clocks);
                catch_up_frames += 1;
            }

            if catch_up_frames == MAX_CATCH_UP_FRAMES && Instant::now() >= pacer.deadline() {
                pacer.rebase(Instant::now());
                shared
                    .telemetry
                    .catch_up_rebases
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

fn pause_after_error(shared: &RuntimeShared, error: String) {
    *lock_unpoisoned(&shared.runtime_error) = Some(error);
    shared.control.set_mode(WorkerMode::Paused);
    if let Some(audio) = &shared.audio {
        audio.set_paused(true);
    }
}

#[derive(Debug, Clone, Copy)]
struct RationalPacer {
    deadline: Instant,
    remainder: u64,
}

impl RationalPacer {
    fn new(origin: Instant) -> Self {
        Self {
            deadline: origin,
            remainder: 0,
        }
    }

    fn deadline(self) -> Instant {
        self.deadline
    }

    fn advance_ppu_clocks(&mut self, clocks: u64) {
        let numerator =
            u128::from(clocks) * u128::from(NTSC_PPU_CLOCK_DENOMINATOR) * 1_000_000_000_u128
                + u128::from(self.remainder);
        let denominator = u128::from(NTSC_MASTER_CLOCK_NUMERATOR_HZ);
        let nanos = numerator / denominator;
        self.remainder = (numerator % denominator) as u64;
        self.deadline += Duration::from_nanos(nanos as u64);
    }

    fn rebase(&mut self, now: Instant) {
        self.deadline = now;
        self.remainder = 0;
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn wait_unpoisoned<'a, T>(condvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condvar
        .wait(guard)
        .unwrap_or_else(|error| error.into_inner())
}

fn wait_timeout_unpoisoned<'a, T>(
    condvar: &Condvar,
    guard: MutexGuard<'a, T>,
    timeout: Duration,
) -> (MutexGuard<'a, T>, std::sync::WaitTimeoutResult) {
    condvar
        .wait_timeout(guard, timeout)
        .unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{NativeVideoCrop, NativeVideoTiming};

    const TEST_PALETTE: [u16; 2] = [0, 0xffff];

    #[derive(Debug)]
    struct FakeCore {
        pixels: [u8; 4],
        audio_samples: [i16; 4],
        frame_id: u64,
        emulated_ticks: u64,
        last_input_sequence: u64,
        advance_delay: Duration,
    }

    impl FakeCore {
        fn new() -> Self {
            Self {
                pixels: [0; 4],
                audio_samples: [1, 2, 3, 4],
                frame_id: 1,
                emulated_ticks: 10,
                last_input_sequence: 0,
                advance_delay: Duration::ZERO,
            }
        }

        fn with_advance_delay(mut self, advance_delay: Duration) -> Self {
            self.advance_delay = advance_delay;
            self
        }

        fn frame(&self, frame_ppu_clocks: u64) -> RealtimeNesFrame<'_> {
            RealtimeNesFrame {
                video: NativeVideoFrame {
                    width: 2,
                    height: 2,
                    visible_crop: NativeVideoCrop::full(2, 2),
                    pixel_format: NativePixelFormat::Indexed8Rgb565,
                    frame_id: self.frame_id,
                    pixels: &self.pixels,
                    palette_rgb565: &TEST_PALETTE,
                    timing: Some(NativeVideoTiming {
                        emulated_ticks: self.emulated_ticks,
                        input_sequence_id: self.last_input_sequence,
                    }),
                },
                audio_samples: &self.audio_samples,
                frame_ppu_clocks,
            }
        }
    }

    impl RealtimeNesCore for FakeCore {
        fn current_frame(&self) -> RealtimeNesFrame<'_> {
            self.frame(0)
        }

        fn advance_frame(&mut self, input: FrameInput) -> Result<RealtimeNesFrame<'_>, String> {
            std::thread::sleep(self.advance_delay);
            self.frame_id += 1;
            self.emulated_ticks += 89_342;
            self.last_input_sequence = input.sequence_id;
            self.pixels.fill(u8::from(
                input.controllers[0].contains(ControllerButtons::A),
            ));
            Ok(self.frame(89_342))
        }
    }

    #[test]
    fn rational_pacer_accumulates_exact_ntsc_fraction_without_rounding_each_frame() {
        let origin = Instant::now();
        let mut pacer = RationalPacer::new(origin);
        let clocks = [89_342_u64, 89_341, 89_342, 89_341];
        for clocks in clocks {
            pacer.advance_ppu_clocks(clocks);
        }

        let total_clocks = clocks.into_iter().sum::<u64>();
        let expected_nanos =
            u128::from(total_clocks) * u128::from(NTSC_PPU_CLOCK_DENOMINATOR) * 1_000_000_000_u128
                / u128::from(NTSC_MASTER_CLOCK_NUMERATOR_HZ);
        assert_eq!(
            pacer.deadline().duration_since(origin).as_nanos(),
            expected_nanos
        );
    }

    #[test]
    fn video_handoff_keeps_only_three_slots_and_reports_coalescing() {
        let core = FakeCore::new();
        let initial = core.current_frame();
        let telemetry = Arc::new(TelemetryCounters::default());
        let handoff = Arc::new(VideoHandoff::new(
            NativeVideoDescriptor::from_frame(initial.video).unwrap(),
            Arc::clone(&telemetry),
        ));
        for frame_id in 1..=8 {
            let frame = NativeVideoFrame {
                frame_id,
                ..initial.video
            };
            handoff
                .publish(
                    frame,
                    RealtimeVideoMetadata {
                        frame_id,
                        ..RealtimeVideoMetadata::default()
                    },
                )
                .unwrap();
        }

        let consumer = RealtimeVideoConsumer { handoff };
        let mut pixels = [0_u8; 4];
        let latest = consumer.try_copy_latest(&mut pixels).unwrap().unwrap();
        assert_eq!(latest.frame_id, 8);
        assert_eq!(consumer.handoff.slots.len(), VIDEO_SLOT_COUNT);
        assert_eq!(telemetry.coalesced_video_frames.load(Ordering::Relaxed), 7);
    }

    #[test]
    fn worker_neutralizes_resume_then_samples_only_the_latest_input() {
        let runtime = NesRealtimeRuntime::spawn(FakeCore::new()).unwrap();
        let consumer = runtime.video_consumer();
        let mut pixels = [0_u8; 4];
        assert_eq!(
            consumer
                .try_copy_latest(&mut pixels)
                .unwrap()
                .unwrap()
                .frame_id,
            1
        );
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(runtime.telemetry().emulated_frames, 0);

        runtime.publish_input(
            [ControllerButtons::LEFT, ControllerButtons::NONE],
            Instant::now(),
        );
        runtime.publish_input(
            [ControllerButtons::A, ControllerButtons::NONE],
            Instant::now(),
        );
        runtime.set_paused(false);
        wait_for(Duration::from_secs(1), || {
            runtime.telemetry().emulated_frames >= 1
        });
        let telemetry = runtime.telemetry();
        let applied = telemetry.latest_input.unwrap();
        // Resume deliberately inserts one neutral sequence. This proves stale
        // held input cannot leak across the lifecycle boundary.
        assert_eq!(applied.controllers, [ControllerButtons::NONE; 2]);

        runtime.publish_input(
            [ControllerButtons::LEFT, ControllerButtons::NONE],
            Instant::now(),
        );
        let expected_sequence = runtime.publish_input(
            [ControllerButtons::A, ControllerButtons::NONE],
            Instant::now(),
        );
        wait_for(Duration::from_secs(1), || {
            runtime
                .telemetry()
                .latest_input
                .is_some_and(|input| input.sequence_id == expected_sequence)
        });
        runtime.set_paused(true);

        let applied = runtime.telemetry().latest_input.unwrap();
        assert_eq!(applied.sequence_id, expected_sequence);
        assert_eq!(
            applied.controllers,
            [ControllerButtons::A, ControllerButtons::NONE]
        );
    }

    #[test]
    fn one_wakeup_is_outstanding_while_new_frames_coalesce() {
        let core = FakeCore::new();
        let initial = core.current_frame();
        let telemetry = Arc::new(TelemetryCounters::default());
        let handoff = Arc::new(VideoHandoff::new(
            NativeVideoDescriptor::from_frame(initial.video).unwrap(),
            Arc::clone(&telemetry),
        ));
        let wakes = Arc::new(AtomicU64::new(0));
        let wake_counter = Arc::clone(&wakes);
        handoff.set_waker(Arc::new(move || {
            wake_counter.fetch_add(1, Ordering::Relaxed);
        }));
        for frame_id in 1..=5 {
            handoff
                .publish(
                    NativeVideoFrame {
                        frame_id,
                        ..initial.video
                    },
                    RealtimeVideoMetadata {
                        frame_id,
                        ..RealtimeVideoMetadata::default()
                    },
                )
                .unwrap();
        }
        assert_eq!(wakes.load(Ordering::Relaxed), 1);

        let consumer = RealtimeVideoConsumer { handoff };
        let mut pixels = [0_u8; 4];
        assert_eq!(
            consumer
                .try_copy_latest(&mut pixels)
                .unwrap()
                .unwrap()
                .frame_id,
            5
        );
        assert!(!consumer.handoff.wake_pending.load(Ordering::Acquire));
    }

    #[test]
    fn worker_rebases_after_bounded_overload_catch_up() {
        let runtime = NesRealtimeRuntime::spawn(
            FakeCore::new().with_advance_delay(Duration::from_millis(25)),
        )
        .unwrap();
        runtime.set_paused(false);
        wait_for(Duration::from_secs(1), || {
            runtime.telemetry().catch_up_rebases >= 1
        });
        runtime.set_paused(true);

        let telemetry = runtime.telemetry();
        assert!(telemetry.emulated_frames >= MAX_CATCH_UP_FRAMES as u64);
        assert!(telemetry.catch_up_rebases >= 1);
    }

    #[test]
    fn worker_feeds_and_flushes_the_bounded_audio_endpoint() {
        let audio = RealtimeAudioEndpoint::new(1);
        let runtime =
            NesRealtimeRuntime::spawn_with_audio_endpoint(FakeCore::new(), audio).unwrap();
        runtime.set_paused(false);
        wait_for(Duration::from_secs(1), || {
            runtime.telemetry().produced_audio_samples >= 4
        });

        let running = runtime.telemetry();
        assert!(running.audio_available);
        assert!(running.audio_active);
        assert!(running.audio_queue_depth_samples > 0);
        assert!(running.audio_queue_depth_samples <= running.audio_capacity_samples);

        runtime.set_paused(true);
        let paused = runtime.telemetry();
        assert!(!paused.audio_active);
        assert_eq!(paused.audio_queue_depth_samples, 0);
    }

    fn wait_for(timeout: Duration, condition: impl Fn() -> bool) {
        let deadline = Instant::now() + timeout;
        while !condition() {
            assert!(Instant::now() < deadline, "condition timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
