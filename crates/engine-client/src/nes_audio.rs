//! Bounded lock-free audio handoff and the optional CPAL output device.

#![cfg_attr(test, allow(dead_code))]

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI16, AtomicU32, AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
    SupportedBufferSize, SupportedStreamConfigRange,
};
use engine_nes::AUDIO_SAMPLE_RATE_HZ;

const AUDIO_RING_CAPACITY_SAMPLES: usize = 4_096;
const DESIRED_DEVICE_BUFFER_FRAMES: u32 = 256;
const MIN_PRIME_DEPTH_SAMPLES: usize = 768;
const FULL_VOLUME_Q15: u32 = 1 << 15;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RealtimeAudioTelemetry {
    pub available: bool,
    pub active: bool,
    pub primed: bool,
    pub muted: bool,
    pub capacity_samples: usize,
    pub target_depth_samples: usize,
    pub current_depth_samples: usize,
    pub high_water_samples: usize,
    pub consumed_samples: u64,
    pub underrun_samples: u64,
    pub overflow_samples: u64,
    pub callback_count: u64,
    pub device_errors: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AudioStartError {
    detail: String,
}

impl AudioStartError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for AudioStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for AudioStartError {}

/// Cloneable producer/control endpoint. The sample callback and emulator
/// worker share only atomics and fixed storage through this value.
#[derive(Clone)]
pub(crate) struct RealtimeAudioEndpoint {
    shared: Arc<AudioShared>,
}

impl fmt::Debug for RealtimeAudioEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RealtimeAudioEndpoint")
            .field("telemetry", &self.telemetry())
            .finish()
    }
}

impl RealtimeAudioEndpoint {
    pub(crate) fn new(target_depth_samples: usize) -> Self {
        Self {
            shared: Arc::new(AudioShared::new(
                AUDIO_RING_CAPACITY_SAMPLES,
                target_depth_samples.clamp(1, AUDIO_RING_CAPACITY_SAMPLES),
            )),
        }
    }

    pub fn push_samples(&self, samples: &[i16]) {
        if !self.shared.active.load(Ordering::Acquire) {
            return;
        }
        let overflow = self.shared.ring.push(samples);
        self.shared
            .overflow_samples
            .fetch_add(overflow, Ordering::Relaxed);
        self.shared
            .high_water_samples
            .fetch_max(self.shared.ring.depth() as u64, Ordering::Relaxed);
    }

    pub fn set_paused(&self, paused: bool) {
        if paused {
            self.shared.active.store(false, Ordering::Release);
            self.shared.primed.store(false, Ordering::Release);
            self.shared.ring.discard_current_contents();
        } else {
            self.shared.primed.store(false, Ordering::Release);
            self.shared.ring.discard_current_contents();
            self.shared.active.store(true, Ordering::Release);
        }
    }

    pub fn set_muted(&self, muted: bool) {
        self.shared.muted.store(muted, Ordering::Release);
    }

    pub fn set_volume(&self, volume: f32) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.shared.volume_q15.store(
            (volume * FULL_VOLUME_Q15 as f32).round() as u32,
            Ordering::Release,
        );
    }

    pub fn telemetry(&self) -> RealtimeAudioTelemetry {
        RealtimeAudioTelemetry {
            available: true,
            active: self.shared.active.load(Ordering::Acquire),
            primed: self.shared.primed.load(Ordering::Acquire),
            muted: self.shared.muted.load(Ordering::Acquire),
            capacity_samples: self.shared.ring.capacity(),
            target_depth_samples: self.shared.target_depth_samples,
            current_depth_samples: self.shared.ring.depth(),
            high_water_samples: self.shared.high_water_samples.load(Ordering::Relaxed) as usize,
            consumed_samples: self.shared.consumed_samples.load(Ordering::Relaxed),
            underrun_samples: self.shared.underrun_samples.load(Ordering::Relaxed),
            overflow_samples: self.shared.overflow_samples.load(Ordering::Relaxed),
            callback_count: self.shared.callback_count.load(Ordering::Relaxed),
            device_errors: self.shared.device_errors.load(Ordering::Relaxed),
        }
    }

    fn write_output<T>(&self, output: &mut [T], channels: usize)
    where
        T: SizedSample + FromSample<f32>,
    {
        self.shared.callback_count.fetch_add(1, Ordering::Relaxed);
        if channels == 0 {
            return;
        }
        let frame_count = output.len() / channels;
        if !self.shared.active.load(Ordering::Acquire) {
            fill_silence(output);
            self.shared.primed.store(false, Ordering::Release);
            self.shared.ring.apply_discard_floor();
            return;
        }

        let (mut read, write) = self.shared.ring.begin_read(&self.shared);
        let available = write.saturating_sub(read) as usize;
        if !self.shared.primed.load(Ordering::Acquire) {
            if available < self.shared.target_depth_samples {
                fill_silence(output);
                return;
            }
            self.shared.primed.store(true, Ordering::Release);
        }

        let muted = self.shared.muted.load(Ordering::Acquire);
        let volume = self.shared.volume_q15.load(Ordering::Acquire) as f32 / FULL_VOLUME_Q15 as f32;
        let frames_to_copy = frame_count.min(write.saturating_sub(read) as usize);
        for frame in output.chunks_mut(channels).take(frames_to_copy) {
            let sample = self.shared.ring.sample(read);
            read += 1;
            let normalized = if muted {
                0.0
            } else {
                f32::from(sample) / 32_768.0 * volume
            };
            let sample = T::from_sample(normalized);
            frame.fill(sample);
        }
        self.shared.ring.commit_read(read);
        self.shared
            .consumed_samples
            .fetch_add(frames_to_copy as u64, Ordering::Relaxed);

        if frames_to_copy < frame_count {
            fill_silence(&mut output[frames_to_copy * channels..]);
            self.shared
                .underrun_samples
                .fetch_add((frame_count - frames_to_copy) as u64, Ordering::Relaxed);
            self.shared.primed.store(false, Ordering::Release);
        }
    }
}

#[derive(Debug)]
struct AudioShared {
    ring: AudioRing,
    target_depth_samples: usize,
    active: AtomicBool,
    primed: AtomicBool,
    muted: AtomicBool,
    volume_q15: AtomicU32,
    high_water_samples: AtomicU64,
    consumed_samples: AtomicU64,
    underrun_samples: AtomicU64,
    overflow_samples: AtomicU64,
    callback_count: AtomicU64,
    device_errors: AtomicU64,
}

impl AudioShared {
    fn new(capacity: usize, target_depth_samples: usize) -> Self {
        Self {
            ring: AudioRing::new(capacity),
            target_depth_samples,
            active: AtomicBool::new(false),
            primed: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            volume_q15: AtomicU32::new(FULL_VOLUME_Q15),
            high_water_samples: AtomicU64::new(0),
            consumed_samples: AtomicU64::new(0),
            underrun_samples: AtomicU64::new(0),
            overflow_samples: AtomicU64::new(0),
            callback_count: AtomicU64::new(0),
            device_errors: AtomicU64::new(0),
        }
    }
}

#[derive(Debug)]
struct AudioRing {
    samples: Box<[AtomicI16]>,
    write_index: AtomicU64,
    read_index: AtomicU64,
    discard_through: AtomicU64,
}

impl AudioRing {
    fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            samples: (0..capacity)
                .map(|_| AtomicI16::new(0))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            write_index: AtomicU64::new(0),
            read_index: AtomicU64::new(0),
            discard_through: AtomicU64::new(0),
        }
    }

    fn capacity(&self) -> usize {
        self.samples.len()
    }

    fn push(&self, samples: &[i16]) -> u64 {
        let mut write = self.write_index.load(Ordering::Relaxed);
        let mut overflow = 0;
        for sample in samples {
            let read = self.read_index.load(Ordering::Acquire);
            if write.saturating_sub(read) >= self.capacity() as u64 {
                overflow += 1;
                continue;
            }
            self.samples[write as usize % self.capacity()].store(*sample, Ordering::Relaxed);
            write += 1;
            self.write_index.store(write, Ordering::Release);
        }
        overflow
    }

    fn discard_current_contents(&self) {
        let write = self.write_index.load(Ordering::Acquire);
        self.discard_through.store(write, Ordering::Release);
        // This lifecycle-only advance lets the producer reuse discarded slots
        // immediately. A callback already in flight cannot move the index
        // backwards because its commit also uses fetch_max; individual slots
        // are atomic so transition-time overlap remains data-race-free.
        self.read_index.fetch_max(write, Ordering::Release);
    }

    fn begin_read(&self, telemetry: &AudioShared) -> (u64, u64) {
        let write = self.write_index.load(Ordering::Acquire);
        let floor = self.discard_through.load(Ordering::Acquire);
        let read = self.read_index.load(Ordering::Relaxed).max(floor);
        update_high_water(
            telemetry,
            write.saturating_sub(read).min(self.capacity() as u64),
        );
        (read, write)
    }

    fn sample(&self, index: u64) -> i16 {
        self.samples[index as usize % self.capacity()].load(Ordering::Relaxed)
    }

    fn commit_read(&self, read: u64) {
        self.read_index.fetch_max(read, Ordering::Release);
    }

    fn apply_discard_floor(&self) {
        self.commit_read(self.discard_through.load(Ordering::Acquire));
    }

    fn depth(&self) -> usize {
        let write = self.write_index.load(Ordering::Acquire);
        let read = self
            .read_index
            .load(Ordering::Acquire)
            .max(self.discard_through.load(Ordering::Acquire));
        write.saturating_sub(read).min(self.capacity() as u64) as usize
    }
}

fn update_high_water(telemetry: &AudioShared, depth: u64) {
    telemetry
        .high_water_samples
        .fetch_max(depth, Ordering::Relaxed);
}

pub(crate) struct CpalAudioOutput {
    _stream: Stream,
    endpoint: RealtimeAudioEndpoint,
    device_name: String,
    channels: u16,
    buffer_frames: usize,
    sample_format: SampleFormat,
}

impl fmt::Debug for CpalAudioOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CpalAudioOutput")
            .field("device_name", &self.device_name)
            .field("channels", &self.channels)
            .field("buffer_frames", &self.buffer_frames)
            .field("sample_format", &self.sample_format)
            .finish_non_exhaustive()
    }
}

impl CpalAudioOutput {
    pub fn try_default() -> Result<Self, AudioStartError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| AudioStartError::new("no default audio output device is available"))?;
        let supported = choose_output_config(&device)?;
        let sample_format = supported.sample_format();
        let (buffer_size, requested_buffer_frames) = choose_buffer_size(*supported.buffer_size());
        let mut config = supported.with_sample_rate(AUDIO_SAMPLE_RATE_HZ).config();
        config.buffer_size = buffer_size;
        let target_depth = MIN_PRIME_DEPTH_SAMPLES
            .max(requested_buffer_frames.saturating_mul(2))
            .min(AUDIO_RING_CAPACITY_SAMPLES);
        let endpoint = RealtimeAudioEndpoint::new(target_depth);
        let (stream, buffer_frames) = match build_stream(
            &device,
            &config,
            sample_format,
            endpoint.clone(),
        ) {
            Ok(stream) => (stream, requested_buffer_frames),
            Err(fixed_error) if matches!(config.buffer_size, BufferSize::Fixed(_)) => {
                config.buffer_size = BufferSize::Default;
                let stream = build_stream(&device, &config, sample_format, endpoint.clone())
                        .map_err(|default_error| {
                            AudioStartError::new(format!(
                                "fixed-buffer audio startup failed ({fixed_error}); default-buffer fallback failed ({default_error})"
                            ))
                        })?;
                (stream, 0)
            }
            Err(error) => return Err(error),
        };
        stream.play().map_err(|error| {
            AudioStartError::new(format!("could not start audio output: {error}"))
        })?;

        Ok(Self {
            _stream: stream,
            endpoint,
            device_name: device.to_string(),
            channels: config.channels,
            buffer_frames,
            sample_format,
        })
    }

    pub fn endpoint(&self) -> RealtimeAudioEndpoint {
        self.endpoint.clone()
    }

    pub fn describe(&self) -> String {
        let buffer = if self.buffer_frames == 0 {
            "default buffer".into()
        } else {
            format!("{}-frame requested buffer", self.buffer_frames)
        };
        format!(
            "{}; {} channels; {} Hz; {:?}; {}",
            self.device_name, self.channels, AUDIO_SAMPLE_RATE_HZ, self.sample_format, buffer
        )
    }
}

fn choose_output_config(device: &Device) -> Result<SupportedStreamConfigRange, AudioStartError> {
    let mut choices = device
        .supported_output_configs()
        .map_err(|error| {
            AudioStartError::new(format!("could not query audio output formats: {error}"))
        })?
        .filter(|config| {
            config.min_sample_rate() <= AUDIO_SAMPLE_RATE_HZ
                && AUDIO_SAMPLE_RATE_HZ <= config.max_sample_rate()
                && sample_format_rank(config.sample_format()).is_some()
        })
        .collect::<Vec<_>>();
    choices.sort_by_key(|config| {
        (
            sample_format_rank(config.sample_format()).unwrap_or(u8::MAX),
            config.channels().abs_diff(2),
            config.channels(),
        )
    });
    choices.into_iter().next().ok_or_else(|| {
        AudioStartError::new(format!(
            "audio device {device} has no 48 kHz PCM output configuration"
        ))
    })
}

fn sample_format_rank(format: SampleFormat) -> Option<u8> {
    match format {
        SampleFormat::F32 => Some(0),
        SampleFormat::I16 => Some(1),
        SampleFormat::U16 => Some(2),
        _ => None,
    }
}

fn choose_buffer_size(supported: SupportedBufferSize) -> (BufferSize, usize) {
    match supported {
        SupportedBufferSize::Range { min, max } => {
            let frames = DESIRED_DEVICE_BUFFER_FRAMES.clamp(min, max);
            (BufferSize::Fixed(frames), frames as usize)
        }
        SupportedBufferSize::Unknown => (BufferSize::Default, 512),
    }
}

fn build_stream(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    endpoint: RealtimeAudioEndpoint,
) -> Result<Stream, AudioStartError> {
    match format {
        SampleFormat::F32 => build_typed_stream::<f32>(device, config, endpoint),
        SampleFormat::I16 => build_typed_stream::<i16>(device, config, endpoint),
        SampleFormat::U16 => build_typed_stream::<u16>(device, config, endpoint),
        _ => Err(AudioStartError::new(format!(
            "unsupported audio output sample format {format}"
        ))),
    }
}

fn build_typed_stream<T>(
    device: &Device,
    config: &StreamConfig,
    endpoint: RealtimeAudioEndpoint,
) -> Result<Stream, AudioStartError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    let output_endpoint = endpoint.clone();
    let error_endpoint = endpoint;
    device
        .build_output_stream(
            *config,
            move |output: &mut [T], _| output_endpoint.write_output(output, channels),
            move |error| {
                error_endpoint
                    .shared
                    .device_errors
                    .fetch_add(1, Ordering::Relaxed);
                tracing::warn!(%error, "NES audio output stream error.");
            },
            None,
        )
        .map_err(|error| {
            AudioStartError::new(format!("could not build audio output stream: {error}"))
        })
}

fn fill_silence<T>(output: &mut [T])
where
    T: SizedSample + FromSample<f32>,
{
    output.fill(T::from_sample(0.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_ring_drops_overflow_without_exceeding_capacity() {
        let endpoint = RealtimeAudioEndpoint {
            shared: Arc::new(AudioShared::new(4, 1)),
        };
        endpoint.push_samples(&[1, 2]);
        endpoint.set_paused(false);
        // Resume intentionally discards pre-transition samples.
        endpoint.push_samples(&[10, 11, 12, 13, 14, 15]);
        let (mut read, write) = endpoint.shared.ring.begin_read(&endpoint.shared);
        let mut samples = Vec::new();
        while read < write {
            samples.push(endpoint.shared.ring.sample(read));
            read += 1;
        }
        endpoint.shared.ring.commit_read(read);

        assert_eq!(samples, vec![10, 11, 12, 13]);
        assert_eq!(endpoint.telemetry().overflow_samples, 2);
        assert_eq!(endpoint.telemetry().capacity_samples, 4);
    }

    #[test]
    fn pause_and_resume_discard_old_samples_without_counting_silence_as_underrun() {
        let endpoint = RealtimeAudioEndpoint {
            shared: Arc::new(AudioShared::new(8, 1)),
        };
        endpoint.push_samples(&[10]);
        endpoint.set_paused(false);
        endpoint.push_samples(&[16_384]);
        let mut output = [0.0_f32; 2];
        endpoint.write_output(&mut output, 2);
        assert_eq!(output, [0.5, 0.5]);

        endpoint.set_paused(true);
        endpoint.push_samples(&[32_000]);
        endpoint.write_output(&mut output, 2);
        assert_eq!(output, [0.0, 0.0]);
        assert_eq!(endpoint.telemetry().underrun_samples, 0);
    }

    #[test]
    fn underrun_returns_to_priming_instead_of_growing_latency() {
        let endpoint = RealtimeAudioEndpoint {
            shared: Arc::new(AudioShared::new(8, 2)),
        };
        endpoint.set_paused(false);
        endpoint.push_samples(&[8_192, 16_384]);
        let mut output = [0.0_f32; 4];
        endpoint.write_output(&mut output, 1);

        assert_eq!(output, [0.25, 0.5, 0.0, 0.0]);
        let telemetry = endpoint.telemetry();
        assert_eq!(telemetry.underrun_samples, 2);
        assert!(!telemetry.primed);
    }

    #[test]
    fn mute_and_volume_do_not_stop_consumption() {
        let endpoint = RealtimeAudioEndpoint {
            shared: Arc::new(AudioShared::new(8, 1)),
        };
        endpoint.set_paused(false);
        endpoint.set_volume(0.5);
        endpoint.push_samples(&[16_384]);
        let mut output = [0.0_f32; 1];
        endpoint.write_output(&mut output, 1);
        assert_eq!(output, [0.25]);

        endpoint.set_muted(true);
        endpoint.push_samples(&[16_384]);
        endpoint.write_output(&mut output, 1);
        assert_eq!(output, [0.0]);
        assert_eq!(endpoint.telemetry().consumed_samples, 2);
    }
}
