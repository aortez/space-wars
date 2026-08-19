use crate::{
    AudioOutput, OamDmaAlignment, StateError,
    state_codec::{StateReader, StateSink},
};

pub const AUDIO_SAMPLE_RATE_HZ: u32 = 48_000;
pub const MAX_AUDIO_SAMPLES_PER_FRAME: usize = 1_024;

// NTSC's 236.25 MHz color source divided by 11 and then by 12 is the RP2A03
// CPU rate. Keeping that exact rational avoids both rounded clock constants
// and platform-dependent floating-point sample cadence.
const NTSC_CPU_CLOCK_NUMERATOR_HZ: u32 = 236_250_000;
const NTSC_CPU_CLOCK_DENOMINATOR: u32 = 11 * 12;
const SAMPLE_PHASE_INCREMENT: u32 = AUDIO_SAMPLE_RATE_HZ * NTSC_CPU_CLOCK_DENOMINATOR;
const HIGH_PASS_COEFFICIENT_Q15: i32 = 32_384;

const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

const NOISE_PERIOD_TABLE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1_016, 2_034, 4_068,
];

const DMC_RATE_TABLE: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];

// These are the observed sequences starting from the position selected by a
// high-timer write. Incrementing through them is equivalent to the hardware's
// downward three-bit sequencer and its documented lookup table.
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1, 0, 0, 0],
    [1, 0, 0, 1, 1, 1, 1, 1],
];

const TRIANGLE_SEQUENCE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DmcDmaKind {
    Load,
    Reload,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmcDmaRequest {
    pub address: u16,
    pub kind: DmcDmaKind,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EnvelopeSnapshot {
    pub start: bool,
    pub divider: u8,
    pub decay_level: u8,
    pub period: u8,
    pub constant_volume: bool,
    pub loop_flag: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SweepSnapshot {
    pub enabled: bool,
    pub divider: u8,
    pub period: u8,
    pub negate: bool,
    pub shift: u8,
    pub reload: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PulseSnapshot {
    pub enabled: bool,
    pub duty: u8,
    pub sequence_position: u8,
    pub timer_period: u16,
    pub timer_counter: u16,
    pub length_counter: u8,
    pub pending_length_reload: Option<u8>,
    pub length_halt_before_write: Option<bool>,
    pub envelope: EnvelopeSnapshot,
    pub sweep: SweepSnapshot,
    pub output: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TriangleSnapshot {
    pub enabled: bool,
    pub timer_period: u16,
    pub timer_counter: u16,
    pub sequence_position: u8,
    pub output_level: u8,
    pub length_counter: u8,
    pub linear_counter: u8,
    pub linear_reload: u8,
    pub control: bool,
    pub reload_flag: bool,
    pub pending_length_reload: Option<u8>,
    pub length_halt_before_write: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoiseSnapshot {
    pub enabled: bool,
    pub timer_period: u16,
    pub timer_counter: u16,
    pub shift_register: u16,
    pub mode: bool,
    pub length_counter: u8,
    pub pending_length_reload: Option<u8>,
    pub length_halt_before_write: Option<bool>,
    pub envelope: EnvelopeSnapshot,
    pub output: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DmcSnapshot {
    pub irq_enabled: bool,
    pub loop_flag: bool,
    pub irq_pending: bool,
    pub rate_index: u8,
    pub timer_counter: u16,
    pub output_level: u8,
    pub sample_address: u16,
    pub sample_length: u16,
    pub current_address: u16,
    pub bytes_remaining: u16,
    pub sample_buffer: Option<u8>,
    pub shift_register: u8,
    pub bits_remaining: u8,
    pub silence: bool,
    pub delayed_dma_clocks: Option<u8>,
    pub delayed_dma_kind: Option<DmcDmaKind>,
    pub dma_request: Option<DmcDmaRequest>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameCounterSnapshot {
    pub sequence_cycle: u32,
    pub mode_five_step: bool,
    pub irq_inhibit: bool,
    pub irq_pending: bool,
    pub pending_write_cycle: Option<u64>,
    pub pending_mode_five_step: bool,
    pub pending_irq_inhibit: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApuSnapshot {
    pub pulse1: PulseSnapshot,
    pub pulse2: PulseSnapshot,
    pub triangle: TriangleSnapshot,
    pub noise: NoiseSnapshot,
    pub dmc: DmcSnapshot,
    pub frame_counter: FrameCounterSnapshot,
    pub cycles: u64,
    pub sample_phase: u32,
    pub total_samples: u64,
    pub frame_sample_count: usize,
    pub mix_accumulator: u64,
    pub mix_count: u16,
    pub high_pass_previous_input: i32,
    pub high_pass_previous_output: i32,
    pub audio_output: AudioOutput,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Envelope {
    start: bool,
    divider: u8,
    decay_level: u8,
    period: u8,
    constant_volume: bool,
    loop_flag: bool,
}

impl Envelope {
    fn write_control(&mut self, value: u8) {
        self.loop_flag = value & 0x20 != 0;
        self.constant_volume = value & 0x10 != 0;
        self.period = value & 0x0f;
    }

    fn restart(&mut self) {
        self.start = true;
    }

    fn clock(&mut self) {
        if self.start {
            self.start = false;
            self.decay_level = 15;
            self.divider = self.period;
        } else if self.divider == 0 {
            self.divider = self.period;
            if self.decay_level != 0 {
                self.decay_level -= 1;
            } else if self.loop_flag {
                self.decay_level = 15;
            }
        } else {
            self.divider -= 1;
        }
    }

    fn output(self) -> u8 {
        if self.constant_volume {
            self.period
        } else {
            self.decay_level
        }
    }

    fn snapshot(self) -> EnvelopeSnapshot {
        EnvelopeSnapshot {
            start: self.start,
            divider: self.divider,
            decay_level: self.decay_level,
            period: self.period,
            constant_volume: self.constant_volume,
            loop_flag: self.loop_flag,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Sweep {
    enabled: bool,
    divider: u8,
    period: u8,
    negate: bool,
    shift: u8,
    reload: bool,
}

impl Sweep {
    fn write(&mut self, value: u8) {
        self.enabled = value & 0x80 != 0;
        self.period = (value >> 4) & 7;
        self.negate = value & 8 != 0;
        self.shift = value & 7;
        self.reload = true;
    }

    fn snapshot(self) -> SweepSnapshot {
        SweepSnapshot {
            enabled: self.enabled,
            divider: self.divider,
            period: self.period,
            negate: self.negate,
            shift: self.shift,
            reload: self.reload,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Pulse {
    enabled: bool,
    duty: u8,
    sequence_position: u8,
    timer_period: u16,
    timer_counter: u16,
    length_counter: u8,
    pending_length_reload: Option<u8>,
    length_halt_before_write: Option<bool>,
    envelope: Envelope,
    sweep: Sweep,
}

impl Pulse {
    fn target_period(self, channel_one: bool) -> u16 {
        let change = self.timer_period >> self.sweep.shift;
        if self.sweep.negate {
            self.timer_period
                .saturating_sub(change + u16::from(channel_one))
        } else {
            self.timer_period.saturating_add(change)
        }
    }

    fn sweep_mutes(self, channel_one: bool) -> bool {
        self.timer_period < 8 || self.target_period(channel_one) > 0x07ff
    }

    fn clock_sweep(&mut self, channel_one: bool) {
        if self.sweep.divider == 0
            && self.sweep.enabled
            && self.sweep.shift != 0
            && !self.sweep_mutes(channel_one)
        {
            self.timer_period = self.target_period(channel_one);
        }

        if self.sweep.divider == 0 || self.sweep.reload {
            self.sweep.divider = self.sweep.period;
            self.sweep.reload = false;
        } else {
            self.sweep.divider -= 1;
        }
    }

    fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period;
            self.sequence_position = (self.sequence_position + 1) & 7;
        } else {
            self.timer_counter -= 1;
        }
    }

    fn output(self, channel_one: bool) -> u8 {
        if !self.enabled
            || self.length_counter == 0
            || self.sweep_mutes(channel_one)
            || DUTY_TABLE[usize::from(self.duty)][usize::from(self.sequence_position)] == 0
        {
            0
        } else {
            self.envelope.output()
        }
    }

    fn snapshot(self, channel_one: bool) -> PulseSnapshot {
        PulseSnapshot {
            enabled: self.enabled,
            duty: self.duty,
            sequence_position: self.sequence_position,
            timer_period: self.timer_period,
            timer_counter: self.timer_counter,
            length_counter: self.length_counter,
            pending_length_reload: self.pending_length_reload,
            length_halt_before_write: self.length_halt_before_write,
            envelope: self.envelope.snapshot(),
            sweep: self.sweep.snapshot(),
            output: self.output(channel_one),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Triangle {
    enabled: bool,
    timer_period: u16,
    timer_counter: u16,
    sequence_position: u8,
    output_level: u8,
    length_counter: u8,
    linear_counter: u8,
    linear_reload: u8,
    control: bool,
    reload_flag: bool,
    pending_length_reload: Option<u8>,
    length_halt_before_write: Option<bool>,
}

impl Triangle {
    fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period;
            if self.length_counter != 0 && self.linear_counter != 0 {
                self.sequence_position = (self.sequence_position + 1) & 31;
                self.output_level = TRIANGLE_SEQUENCE[usize::from(self.sequence_position)];
            }
        } else {
            self.timer_counter -= 1;
        }
    }

    fn clock_linear_counter(&mut self) {
        if self.reload_flag {
            self.linear_counter = self.linear_reload;
        } else if self.linear_counter != 0 {
            self.linear_counter -= 1;
        }
        if !self.control {
            self.reload_flag = false;
        }
    }

    fn snapshot(self) -> TriangleSnapshot {
        TriangleSnapshot {
            enabled: self.enabled,
            timer_period: self.timer_period,
            timer_counter: self.timer_counter,
            sequence_position: self.sequence_position,
            output_level: self.output_level,
            length_counter: self.length_counter,
            linear_counter: self.linear_counter,
            linear_reload: self.linear_reload,
            control: self.control,
            reload_flag: self.reload_flag,
            pending_length_reload: self.pending_length_reload,
            length_halt_before_write: self.length_halt_before_write,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Noise {
    enabled: bool,
    timer_period: u16,
    timer_counter: u16,
    shift_register: u16,
    mode: bool,
    length_counter: u8,
    pending_length_reload: Option<u8>,
    length_halt_before_write: Option<bool>,
    envelope: Envelope,
}

impl Default for Noise {
    fn default() -> Self {
        Self {
            enabled: false,
            timer_period: NOISE_PERIOD_TABLE[0],
            timer_counter: 0,
            shift_register: 1,
            mode: false,
            length_counter: 0,
            pending_length_reload: None,
            length_halt_before_write: None,
            envelope: Envelope::default(),
        }
    }
}

impl Noise {
    fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = self.timer_period - 1;
            let tap = if self.mode { 6 } else { 1 };
            let feedback = (self.shift_register & 1) ^ ((self.shift_register >> tap) & 1);
            self.shift_register = (self.shift_register >> 1) | (feedback << 14);
        } else {
            self.timer_counter -= 1;
        }
    }

    fn output(self) -> u8 {
        if !self.enabled || self.length_counter == 0 || self.shift_register & 1 != 0 {
            0
        } else {
            self.envelope.output()
        }
    }

    fn snapshot(self) -> NoiseSnapshot {
        NoiseSnapshot {
            enabled: self.enabled,
            timer_period: self.timer_period,
            timer_counter: self.timer_counter,
            shift_register: self.shift_register,
            mode: self.mode,
            length_counter: self.length_counter,
            pending_length_reload: self.pending_length_reload,
            length_halt_before_write: self.length_halt_before_write,
            envelope: self.envelope.snapshot(),
            output: self.output(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DelayedDmcDma {
    clocks_remaining: u8,
    kind: DmcDmaKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Dmc {
    irq_enabled: bool,
    loop_flag: bool,
    irq_pending: bool,
    rate_index: u8,
    timer_counter: u16,
    output_level: u8,
    sample_address_register: u8,
    sample_length_register: u8,
    current_address: u16,
    bytes_remaining: u16,
    sample_buffer: Option<u8>,
    shift_register: u8,
    bits_remaining: u8,
    silence: bool,
    delayed_dma: Option<DelayedDmcDma>,
    dma_request: Option<DmcDmaRequest>,
}

impl Default for Dmc {
    fn default() -> Self {
        Self {
            irq_enabled: false,
            loop_flag: false,
            irq_pending: false,
            rate_index: 0,
            timer_counter: DMC_RATE_TABLE[0] - 1,
            output_level: 0,
            sample_address_register: 0,
            sample_length_register: 0,
            current_address: 0xc000,
            bytes_remaining: 0,
            sample_buffer: None,
            shift_register: 0,
            bits_remaining: 8,
            silence: true,
            delayed_dma: None,
            dma_request: None,
        }
    }
}

impl Dmc {
    fn sample_address(self) -> u16 {
        0xc000 | (u16::from(self.sample_address_register) << 6)
    }

    fn sample_length(self) -> u16 {
        u16::from(self.sample_length_register) * 16 + 1
    }

    fn restart_sample(&mut self, kind: DmcDmaKind, clocks_remaining: u8) {
        self.current_address = self.sample_address();
        self.bytes_remaining = self.sample_length();
        if self.sample_buffer.is_none() && self.dma_request.is_none() {
            self.delayed_dma = Some(DelayedDmcDma {
                clocks_remaining,
                kind,
            });
        }
    }

    fn clock_dma_delay(&mut self) {
        let Some(mut delayed) = self.delayed_dma else {
            return;
        };
        delayed.clocks_remaining -= 1;
        if delayed.clocks_remaining == 0 {
            self.delayed_dma = None;
            if self.sample_buffer.is_none() && self.bytes_remaining != 0 {
                self.dma_request = Some(DmcDmaRequest {
                    address: self.current_address,
                    kind: delayed.kind,
                });
            }
        } else {
            self.delayed_dma = Some(delayed);
        }
    }

    fn request_reload_if_needed(&mut self) {
        if self.sample_buffer.is_none()
            && self.bytes_remaining != 0
            && self.delayed_dma.is_none()
            && self.dma_request.is_none()
        {
            self.dma_request = Some(DmcDmaRequest {
                address: self.current_address,
                kind: DmcDmaKind::Reload,
            });
        }
    }

    fn clock_timer(&mut self) {
        if self.timer_counter == 0 {
            self.timer_counter = DMC_RATE_TABLE[usize::from(self.rate_index)] - 1;
            if !self.silence {
                if self.shift_register & 1 != 0 {
                    if self.output_level <= 125 {
                        self.output_level += 2;
                    }
                } else if self.output_level >= 2 {
                    self.output_level -= 2;
                }
            }
            self.shift_register >>= 1;
            self.bits_remaining -= 1;
            if self.bits_remaining == 0 {
                self.bits_remaining = 8;
                if let Some(sample) = self.sample_buffer.take() {
                    self.shift_register = sample;
                    self.silence = false;
                    self.request_reload_if_needed();
                } else {
                    self.silence = true;
                }
            }
        } else {
            self.timer_counter -= 1;
        }
    }

    fn complete_dma(&mut self, value: u8) {
        if self.bytes_remaining == 0 {
            return;
        }
        self.sample_buffer = Some(value);
        self.current_address = if self.current_address == 0xffff {
            0x8000
        } else {
            self.current_address + 1
        };
        self.bytes_remaining -= 1;
        if self.bytes_remaining == 0 {
            if self.loop_flag {
                self.current_address = self.sample_address();
                self.bytes_remaining = self.sample_length();
            } else if self.irq_enabled {
                self.irq_pending = true;
            }
        }
    }

    fn snapshot(self) -> DmcSnapshot {
        DmcSnapshot {
            irq_enabled: self.irq_enabled,
            loop_flag: self.loop_flag,
            irq_pending: self.irq_pending,
            rate_index: self.rate_index,
            timer_counter: self.timer_counter,
            output_level: self.output_level,
            sample_address: self.sample_address(),
            sample_length: self.sample_length(),
            current_address: self.current_address,
            bytes_remaining: self.bytes_remaining,
            sample_buffer: self.sample_buffer,
            shift_register: self.shift_register,
            bits_remaining: self.bits_remaining,
            silence: self.silence,
            delayed_dma_clocks: self.delayed_dma.map(|dma| dma.clocks_remaining),
            delayed_dma_kind: self.delayed_dma.map(|dma| dma.kind),
            dma_request: self.dma_request,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingFrameCounterWrite {
    apply_cycle: u64,
    mode_five_step: bool,
    irq_inhibit: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FrameCounter {
    sequence_cycle: u32,
    mode_five_step: bool,
    irq_inhibit: bool,
    irq_pending: bool,
    pending_write: Option<PendingFrameCounterWrite>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FrameSignals {
    quarter: bool,
    half: bool,
}

impl FrameCounter {
    fn write(&mut self, value: u8, current_cycle: u64) {
        let irq_inhibit = value & 0x40 != 0;
        self.irq_inhibit = irq_inhibit;
        if irq_inhibit {
            self.irq_pending = false;
        }
        let delay = if current_cycle & 1 == 0 { 3 } else { 4 };
        self.pending_write = Some(PendingFrameCounterWrite {
            apply_cycle: current_cycle.wrapping_add(delay),
            mode_five_step: value & 0x80 != 0,
            irq_inhibit,
        });
    }

    fn clock(&mut self, current_cycle: u64) -> FrameSignals {
        if self
            .pending_write
            .is_some_and(|write| write.apply_cycle == current_cycle)
        {
            let write = self
                .pending_write
                .take()
                .expect("pending write was checked");
            self.sequence_cycle = 0;
            self.mode_five_step = write.mode_five_step;
            self.irq_inhibit = write.irq_inhibit;
            return FrameSignals {
                quarter: write.mode_five_step,
                half: write.mode_five_step,
            };
        }

        self.sequence_cycle += 1;
        if self.mode_five_step {
            match self.sequence_cycle {
                7_457 | 22_371 => FrameSignals {
                    quarter: true,
                    half: false,
                },
                14_913 => FrameSignals {
                    quarter: true,
                    half: true,
                },
                37_281 => FrameSignals {
                    quarter: true,
                    half: true,
                },
                37_282 => {
                    self.sequence_cycle = 0;
                    FrameSignals::default()
                }
                _ => FrameSignals::default(),
            }
        } else {
            match self.sequence_cycle {
                7_457 | 22_371 => FrameSignals {
                    quarter: true,
                    half: false,
                },
                14_913 => FrameSignals {
                    quarter: true,
                    half: true,
                },
                29_828 => {
                    if !self.irq_inhibit {
                        self.irq_pending = true;
                    }
                    FrameSignals::default()
                }
                29_829 => {
                    if !self.irq_inhibit {
                        self.irq_pending = true;
                    }
                    FrameSignals {
                        quarter: true,
                        half: true,
                    }
                }
                29_830 => {
                    self.sequence_cycle = 0;
                    if !self.irq_inhibit {
                        self.irq_pending = true;
                    }
                    FrameSignals::default()
                }
                _ => FrameSignals::default(),
            }
        }
    }

    fn snapshot(self) -> FrameCounterSnapshot {
        FrameCounterSnapshot {
            sequence_cycle: self.sequence_cycle,
            mode_five_step: self.mode_five_step,
            irq_inhibit: self.irq_inhibit,
            irq_pending: self.irq_pending,
            pending_write_cycle: self.pending_write.map(|write| write.apply_cycle),
            pending_mode_five_step: self.pending_write.is_some_and(|write| write.mode_five_step),
            pending_irq_inhibit: self.pending_write.is_some_and(|write| write.irq_inhibit),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Apu {
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    dmc: Dmc,
    frame_counter: FrameCounter,
    cycles: u64,
    sample_phase: u32,
    total_samples: u64,

    audio_output: AudioOutput,
    dma_alignment: OamDmaAlignment,
    mix_accumulator: u64,
    mix_count: u16,
    high_pass_previous_input: i32,
    high_pass_previous_output: i32,
    frame_samples: Box<[i16; MAX_AUDIO_SAMPLES_PER_FRAME]>,
    frame_sample_count: usize,
}

impl Apu {
    pub fn new(audio_output: AudioOutput) -> Self {
        Self {
            pulse1: Pulse::default(),
            pulse2: Pulse::default(),
            triangle: Triangle::default(),
            noise: Noise::default(),
            dmc: Dmc::default(),
            frame_counter: FrameCounter::default(),
            cycles: 0,
            sample_phase: 0,
            total_samples: 0,
            audio_output,
            dma_alignment: OamDmaAlignment::default(),
            mix_accumulator: 0,
            mix_count: 0,
            high_pass_previous_input: 0,
            high_pass_previous_output: 0,
            frame_samples: Box::new([0; MAX_AUDIO_SAMPLES_PER_FRAME]),
            frame_sample_count: 0,
        }
    }

    pub fn snapshot(&self) -> ApuSnapshot {
        ApuSnapshot {
            pulse1: self.pulse1.snapshot(true),
            pulse2: self.pulse2.snapshot(false),
            triangle: self.triangle.snapshot(),
            noise: self.noise.snapshot(),
            dmc: self.dmc.snapshot(),
            frame_counter: self.frame_counter.snapshot(),
            cycles: self.cycles,
            sample_phase: self.sample_phase,
            total_samples: self.total_samples,
            frame_sample_count: self.frame_sample_count,
            mix_accumulator: self.mix_accumulator,
            mix_count: self.mix_count,
            high_pass_previous_input: self.high_pass_previous_input,
            high_pass_previous_output: self.high_pass_previous_output,
            audio_output: self.audio_output,
        }
    }

    pub fn irq_pending(&self) -> bool {
        self.frame_counter.irq_pending || self.dmc.irq_pending
    }

    pub(crate) fn set_dma_alignment(&mut self, alignment: OamDmaAlignment) {
        self.dma_alignment = alignment;
    }

    pub fn peek_status(&self) -> u8 {
        u8::from(self.pulse1.length_counter != 0)
            | (u8::from(self.pulse2.length_counter != 0) << 1)
            | (u8::from(self.triangle.length_counter != 0) << 2)
            | (u8::from(self.noise.length_counter != 0) << 3)
            | (u8::from(self.dmc.bytes_remaining != 0) << 4)
            | (u8::from(self.frame_counter.irq_pending) << 6)
            | (u8::from(self.dmc.irq_pending) << 7)
    }

    pub fn read_status(&mut self) -> u8 {
        let status = self.peek_status();
        self.frame_counter.irq_pending = false;
        status
    }

    pub fn write_register(&mut self, address: u16, value: u8) {
        match address {
            0x4000 => {
                self.pulse1.duty = value >> 6;
                self.pulse1
                    .length_halt_before_write
                    .get_or_insert(self.pulse1.envelope.loop_flag);
                self.pulse1.envelope.write_control(value);
            }
            0x4001 => self.pulse1.sweep.write(value),
            0x4002 => {
                self.pulse1.timer_period = (self.pulse1.timer_period & 0x0700) | u16::from(value);
            }
            0x4003 => {
                self.pulse1.timer_period =
                    (self.pulse1.timer_period & 0x00ff) | (u16::from(value & 7) << 8);
                if self.pulse1.enabled {
                    self.pulse1.pending_length_reload =
                        Some(LENGTH_COUNTER_TABLE[usize::from(value >> 3)]);
                }
                self.pulse1.sequence_position = 0;
                self.pulse1.envelope.restart();
            }
            0x4004 => {
                self.pulse2.duty = value >> 6;
                self.pulse2
                    .length_halt_before_write
                    .get_or_insert(self.pulse2.envelope.loop_flag);
                self.pulse2.envelope.write_control(value);
            }
            0x4005 => self.pulse2.sweep.write(value),
            0x4006 => {
                self.pulse2.timer_period = (self.pulse2.timer_period & 0x0700) | u16::from(value);
            }
            0x4007 => {
                self.pulse2.timer_period =
                    (self.pulse2.timer_period & 0x00ff) | (u16::from(value & 7) << 8);
                if self.pulse2.enabled {
                    self.pulse2.pending_length_reload =
                        Some(LENGTH_COUNTER_TABLE[usize::from(value >> 3)]);
                }
                self.pulse2.sequence_position = 0;
                self.pulse2.envelope.restart();
            }
            0x4008 => {
                self.triangle
                    .length_halt_before_write
                    .get_or_insert(self.triangle.control);
                self.triangle.control = value & 0x80 != 0;
                self.triangle.linear_reload = value & 0x7f;
            }
            0x4009 | 0x400d => {}
            0x400a => {
                self.triangle.timer_period =
                    (self.triangle.timer_period & 0x0700) | u16::from(value);
            }
            0x400b => {
                self.triangle.timer_period =
                    (self.triangle.timer_period & 0x00ff) | (u16::from(value & 7) << 8);
                if self.triangle.enabled {
                    self.triangle.pending_length_reload =
                        Some(LENGTH_COUNTER_TABLE[usize::from(value >> 3)]);
                }
                self.triangle.reload_flag = true;
            }
            0x400c => {
                self.noise
                    .length_halt_before_write
                    .get_or_insert(self.noise.envelope.loop_flag);
                self.noise.envelope.write_control(value);
            }
            0x400e => {
                self.noise.mode = value & 0x80 != 0;
                self.noise.timer_period = NOISE_PERIOD_TABLE[usize::from(value & 0x0f)];
            }
            0x400f => {
                if self.noise.enabled {
                    self.noise.pending_length_reload =
                        Some(LENGTH_COUNTER_TABLE[usize::from(value >> 3)]);
                }
                self.noise.envelope.restart();
            }
            0x4010 => {
                self.dmc.irq_enabled = value & 0x80 != 0;
                self.dmc.loop_flag = value & 0x40 != 0;
                self.dmc.rate_index = value & 0x0f;
                if !self.dmc.irq_enabled {
                    self.dmc.irq_pending = false;
                }
            }
            0x4011 => self.dmc.output_level = value & 0x7f,
            0x4012 => self.dmc.sample_address_register = value,
            0x4013 => self.dmc.sample_length_register = value,
            0x4015 => self.write_status(value),
            0x4017 => self.frame_counter.write(value, self.cycles),
            _ => {}
        }
    }

    fn write_status(&mut self, value: u8) {
        self.pulse1.enabled = value & 1 != 0;
        self.pulse2.enabled = value & 2 != 0;
        self.triangle.enabled = value & 4 != 0;
        self.noise.enabled = value & 8 != 0;
        if !self.pulse1.enabled {
            self.pulse1.length_counter = 0;
            self.pulse1.pending_length_reload = None;
        }
        if !self.pulse2.enabled {
            self.pulse2.length_counter = 0;
            self.pulse2.pending_length_reload = None;
        }
        if !self.triangle.enabled {
            self.triangle.length_counter = 0;
            self.triangle.pending_length_reload = None;
        }
        if !self.noise.enabled {
            self.noise.length_counter = 0;
            self.noise.pending_length_reload = None;
        }

        self.dmc.irq_pending = false;
        if value & 0x10 == 0 {
            self.dmc.bytes_remaining = 0;
            self.dmc.delayed_dma = None;
            self.dmc.dma_request = None;
        } else if self.dmc.bytes_remaining == 0 {
            // A load DMA targets a get slot in the second APU cycle after the
            // write. Depending on which half-cycle the write occupies, that
            // is three or four CPU slots later.
            let clocks_remaining = if self.dma_alignment.needs_alignment(self.cycles) {
                4
            } else {
                3
            };
            self.dmc.restart_sample(DmcDmaKind::Load, clocks_remaining);
        }
    }

    pub fn begin_frame_output(&mut self) {
        self.frame_sample_count = 0;
    }

    pub fn frame_samples(&self) -> &[i16] {
        if matches!(self.audio_output, AudioOutput::Enabled) {
            &self.frame_samples[..self.frame_sample_count]
        } else {
            &[]
        }
    }

    pub fn take_dmc_dma_request(&mut self) -> Option<DmcDmaRequest> {
        self.dmc.dma_request.take()
    }

    pub fn dmc_dma_request(&self) -> Option<DmcDmaRequest> {
        self.dmc.dma_request
    }

    pub fn complete_dmc_dma(&mut self, value: u8) {
        self.dmc.complete_dma(value);
    }

    pub fn clock(&mut self) {
        self.cycles = self.cycles.wrapping_add(1);
        let signals = self.frame_counter.clock(self.cycles);
        if signals.quarter {
            self.clock_quarter_frame();
        }
        if signals.half {
            self.clock_half_frame();
        } else {
            self.apply_pending_length_reloads();
        }
        self.clear_length_halt_write_latches();

        if self.cycles & 1 == 0 {
            self.pulse1.clock_timer();
            self.pulse2.clock_timer();
        }
        self.triangle.clock_timer();
        self.noise.clock_timer();
        self.dmc.clock_timer();
        self.dmc.clock_dma_delay();
        self.clock_sample_output();
    }

    fn clock_quarter_frame(&mut self) {
        self.pulse1.envelope.clock();
        self.pulse2.envelope.clock();
        self.noise.envelope.clock();
        self.triangle.clock_linear_counter();
    }

    fn clock_half_frame(&mut self) {
        clock_length_counter(
            &mut self.pulse1.length_counter,
            self.pulse1.envelope.loop_flag,
            self.pulse1.length_halt_before_write,
            &mut self.pulse1.pending_length_reload,
        );
        clock_length_counter(
            &mut self.pulse2.length_counter,
            self.pulse2.envelope.loop_flag,
            self.pulse2.length_halt_before_write,
            &mut self.pulse2.pending_length_reload,
        );
        clock_length_counter(
            &mut self.triangle.length_counter,
            self.triangle.control,
            self.triangle.length_halt_before_write,
            &mut self.triangle.pending_length_reload,
        );
        clock_length_counter(
            &mut self.noise.length_counter,
            self.noise.envelope.loop_flag,
            self.noise.length_halt_before_write,
            &mut self.noise.pending_length_reload,
        );
        self.pulse1.clock_sweep(true);
        self.pulse2.clock_sweep(false);
    }

    fn apply_pending_length_reloads(&mut self) {
        for (counter, reload) in [
            (
                &mut self.pulse1.length_counter,
                &mut self.pulse1.pending_length_reload,
            ),
            (
                &mut self.pulse2.length_counter,
                &mut self.pulse2.pending_length_reload,
            ),
            (
                &mut self.triangle.length_counter,
                &mut self.triangle.pending_length_reload,
            ),
            (
                &mut self.noise.length_counter,
                &mut self.noise.pending_length_reload,
            ),
        ] {
            if let Some(value) = reload.take() {
                *counter = value;
            }
        }
    }

    fn clear_length_halt_write_latches(&mut self) {
        self.pulse1.length_halt_before_write = None;
        self.pulse2.length_halt_before_write = None;
        self.triangle.length_halt_before_write = None;
        self.noise.length_halt_before_write = None;
    }

    fn clock_sample_output(&mut self) {
        if matches!(self.audio_output, AudioOutput::Enabled) {
            self.mix_accumulator += u64::from(mix_channels(
                self.pulse1.output(true),
                self.pulse2.output(false),
                self.triangle.output_level,
                self.noise.output(),
                self.dmc.output_level,
            ));
            self.mix_count += 1;
        }

        self.sample_phase += SAMPLE_PHASE_INCREMENT;
        if self.sample_phase < NTSC_CPU_CLOCK_NUMERATOR_HZ {
            return;
        }
        self.sample_phase -= NTSC_CPU_CLOCK_NUMERATOR_HZ;
        self.total_samples = self.total_samples.wrapping_add(1);

        if matches!(self.audio_output, AudioOutput::Enabled) {
            let input = (self.mix_accumulator / u64::from(self.mix_count)) as i32;
            let output = input - self.high_pass_previous_input
                + ((self.high_pass_previous_output * HIGH_PASS_COEFFICIENT_Q15) >> 15);
            self.high_pass_previous_input = input;
            self.high_pass_previous_output = output;
            if self.frame_sample_count < MAX_AUDIO_SAMPLES_PER_FRAME {
                self.frame_samples[self.frame_sample_count] =
                    output.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
                self.frame_sample_count += 1;
            }
            self.mix_accumulator = 0;
            self.mix_count = 0;
        }
    }

    pub(crate) fn write_state<S: StateSink>(&self, sink: &mut S, include_output: bool) {
        write_pulse_state(sink, self.pulse1);
        write_pulse_state(sink, self.pulse2);
        write_triangle_state(sink, self.triangle);
        write_noise_state(sink, self.noise);
        write_dmc_state(sink, self.dmc);
        write_frame_counter_state(sink, self.frame_counter);
        sink.write_u64(self.cycles);
        sink.write_u32(self.sample_phase);
        sink.write_u64(self.total_samples);

        if include_output {
            sink.write_u64(self.mix_accumulator);
            sink.write_u16(self.mix_count);
            sink.write_u32(self.high_pass_previous_input as u32);
            sink.write_u32(self.high_pass_previous_output as u32);
            sink.write_u16(
                u16::try_from(self.frame_sample_count)
                    .expect("APU frame sample bound always fits u16"),
            );
            for sample in &self.frame_samples[..self.frame_sample_count] {
                sink.write_u16(*sample as u16);
            }
        }
    }

    pub(crate) fn read_state(
        &mut self,
        reader: &mut StateReader<'_>,
        include_output: bool,
    ) -> Result<(), StateError> {
        self.pulse1 = read_pulse_state(reader)?;
        self.pulse2 = read_pulse_state(reader)?;
        self.triangle = read_triangle_state(reader)?;
        self.noise = read_noise_state(reader)?;
        self.dmc = read_dmc_state(reader)?;
        self.frame_counter = read_frame_counter_state(reader)?;
        self.cycles = reader.read_u64()?;
        self.sample_phase = reader.read_u32()?;
        self.total_samples = reader.read_u64()?;

        if include_output {
            self.mix_accumulator = reader.read_u64()?;
            self.mix_count = reader.read_u16()?;
            self.high_pass_previous_input = reader.read_u32()? as i32;
            self.high_pass_previous_output = reader.read_u32()? as i32;
            self.frame_sample_count = usize::from(reader.read_u16()?);
            if self.frame_sample_count > MAX_AUDIO_SAMPLES_PER_FRAME {
                return Err(StateError::InvalidPayload(
                    "APU frame sample count exceeds its fixed buffer",
                ));
            }
            for sample in &mut self.frame_samples[..self.frame_sample_count] {
                *sample = reader.read_u16()? as i16;
            }
        }

        self.validate_state(include_output)
    }

    pub(crate) fn copy_emulated_state_from(&mut self, source: &Self) {
        let audio_output = self.audio_output;
        self.pulse1 = source.pulse1;
        self.pulse2 = source.pulse2;
        self.triangle = source.triangle;
        self.noise = source.noise;
        self.dmc = source.dmc;
        self.frame_counter = source.frame_counter;
        self.cycles = source.cycles;
        self.sample_phase = source.sample_phase;
        self.total_samples = source.total_samples;
        self.mix_accumulator = source.mix_accumulator;
        self.mix_count = source.mix_count;
        self.high_pass_previous_input = source.high_pass_previous_input;
        self.high_pass_previous_output = source.high_pass_previous_output;
        self.frame_samples
            .copy_from_slice(&source.frame_samples[..]);
        self.frame_sample_count = source.frame_sample_count;
        self.audio_output = audio_output;
    }

    fn validate_state(&self, include_output: bool) -> Result<(), StateError> {
        for pulse in [self.pulse1, self.pulse2] {
            if pulse.duty > 3
                || pulse.sequence_position > 7
                || pulse.timer_period > 0x07ff
                || pulse.envelope.period > 15
                || pulse.envelope.divider > 15
                || pulse.envelope.decay_level > 15
                || pulse.sweep.period > 7
                || pulse.sweep.divider > 7
                || pulse.sweep.shift > 7
                || pulse
                    .pending_length_reload
                    .is_some_and(|length| !LENGTH_COUNTER_TABLE.contains(&length))
            {
                return Err(StateError::InvalidPayload("pulse state is out of range"));
            }
        }
        if self.triangle.timer_period > 0x07ff
            || self.triangle.sequence_position > 31
            || self.triangle.output_level > 15
            || self.triangle.linear_counter > 0x7f
            || self.triangle.linear_reload > 0x7f
            || self
                .triangle
                .pending_length_reload
                .is_some_and(|length| !LENGTH_COUNTER_TABLE.contains(&length))
        {
            return Err(StateError::InvalidPayload("triangle state is out of range"));
        }
        if !NOISE_PERIOD_TABLE.contains(&self.noise.timer_period)
            || self.noise.shift_register == 0
            || self.noise.shift_register > 0x7fff
            || self.noise.envelope.period > 15
            || self.noise.envelope.divider > 15
            || self.noise.envelope.decay_level > 15
            || self
                .noise
                .pending_length_reload
                .is_some_and(|length| !LENGTH_COUNTER_TABLE.contains(&length))
        {
            return Err(StateError::InvalidPayload("noise state is out of range"));
        }
        if self.dmc.rate_index > 15
            || self.dmc.output_level > 127
            || self.dmc.current_address < 0x8000
            || !(1..=8).contains(&self.dmc.bits_remaining)
            || self
                .dmc
                .delayed_dma
                .is_some_and(|dma| dma.clocks_remaining == 0 || dma.clocks_remaining > 4)
            || (self.dmc.delayed_dma.is_some() && self.dmc.dma_request.is_some())
            || ((self.dmc.delayed_dma.is_some() || self.dmc.dma_request.is_some())
                && (self.dmc.sample_buffer.is_some() || self.dmc.bytes_remaining == 0))
            || self
                .dmc
                .dma_request
                .is_some_and(|request| request.address != self.dmc.current_address)
        {
            return Err(StateError::InvalidPayload("DMC state is out of range"));
        }
        let sequence_limit = if self.frame_counter.mode_five_step {
            37_282
        } else {
            29_830
        };
        if self.frame_counter.sequence_cycle >= sequence_limit
            || self
                .frame_counter
                .pending_write
                .is_some_and(|write| write.apply_cycle.wrapping_sub(self.cycles) > 4)
            || self.sample_phase >= NTSC_CPU_CLOCK_NUMERATOR_HZ
        {
            return Err(StateError::InvalidPayload(
                "APU frame or sample timing is out of range",
            ));
        }
        if include_output
            && (self.frame_sample_count > MAX_AUDIO_SAMPLES_PER_FRAME
                || self.mix_count > 64
                || self.mix_accumulator > u64::from(u16::MAX) * u64::from(self.mix_count))
        {
            return Err(StateError::InvalidPayload(
                "APU sample output state is out of range",
            ));
        }
        Ok(())
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new(AudioOutput::Enabled)
    }
}

fn clock_length_counter(
    counter: &mut u8,
    current_halt: bool,
    halt_before_write: Option<bool>,
    pending_reload: &mut Option<u8>,
) {
    let was_nonzero = *counter != 0;
    if was_nonzero && !halt_before_write.unwrap_or(current_halt) {
        *counter -= 1;
    }
    if let Some(value) = pending_reload.take()
        && !was_nonzero
    {
        *counter = value;
    }
}

fn write_envelope_state<S: StateSink>(sink: &mut S, envelope: Envelope) {
    sink.write_bool(envelope.start);
    sink.write_u8(envelope.divider);
    sink.write_u8(envelope.decay_level);
    sink.write_u8(envelope.period);
    sink.write_bool(envelope.constant_volume);
    sink.write_bool(envelope.loop_flag);
}

fn read_envelope_state(reader: &mut StateReader<'_>) -> Result<Envelope, StateError> {
    Ok(Envelope {
        start: reader.read_bool()?,
        divider: reader.read_u8()?,
        decay_level: reader.read_u8()?,
        period: reader.read_u8()?,
        constant_volume: reader.read_bool()?,
        loop_flag: reader.read_bool()?,
    })
}

fn write_sweep_state<S: StateSink>(sink: &mut S, sweep: Sweep) {
    sink.write_bool(sweep.enabled);
    sink.write_u8(sweep.divider);
    sink.write_u8(sweep.period);
    sink.write_bool(sweep.negate);
    sink.write_u8(sweep.shift);
    sink.write_bool(sweep.reload);
}

fn read_sweep_state(reader: &mut StateReader<'_>) -> Result<Sweep, StateError> {
    Ok(Sweep {
        enabled: reader.read_bool()?,
        divider: reader.read_u8()?,
        period: reader.read_u8()?,
        negate: reader.read_bool()?,
        shift: reader.read_u8()?,
        reload: reader.read_bool()?,
    })
}

fn write_pulse_state<S: StateSink>(sink: &mut S, pulse: Pulse) {
    sink.write_bool(pulse.enabled);
    sink.write_u8(pulse.duty);
    sink.write_u8(pulse.sequence_position);
    sink.write_u16(pulse.timer_period);
    sink.write_u16(pulse.timer_counter);
    sink.write_u8(pulse.length_counter);
    sink.write_optional_u8(pulse.pending_length_reload);
    write_optional_bool(sink, pulse.length_halt_before_write);
    write_envelope_state(sink, pulse.envelope);
    write_sweep_state(sink, pulse.sweep);
}

fn read_pulse_state(reader: &mut StateReader<'_>) -> Result<Pulse, StateError> {
    Ok(Pulse {
        enabled: reader.read_bool()?,
        duty: reader.read_u8()?,
        sequence_position: reader.read_u8()?,
        timer_period: reader.read_u16()?,
        timer_counter: reader.read_u16()?,
        length_counter: reader.read_u8()?,
        pending_length_reload: reader.read_optional_u8()?,
        length_halt_before_write: read_optional_bool(reader)?,
        envelope: read_envelope_state(reader)?,
        sweep: read_sweep_state(reader)?,
    })
}

fn write_triangle_state<S: StateSink>(sink: &mut S, triangle: Triangle) {
    sink.write_bool(triangle.enabled);
    sink.write_u16(triangle.timer_period);
    sink.write_u16(triangle.timer_counter);
    sink.write_u8(triangle.sequence_position);
    sink.write_u8(triangle.output_level);
    sink.write_u8(triangle.length_counter);
    sink.write_u8(triangle.linear_counter);
    sink.write_u8(triangle.linear_reload);
    sink.write_bool(triangle.control);
    sink.write_bool(triangle.reload_flag);
    sink.write_optional_u8(triangle.pending_length_reload);
    write_optional_bool(sink, triangle.length_halt_before_write);
}

fn read_triangle_state(reader: &mut StateReader<'_>) -> Result<Triangle, StateError> {
    Ok(Triangle {
        enabled: reader.read_bool()?,
        timer_period: reader.read_u16()?,
        timer_counter: reader.read_u16()?,
        sequence_position: reader.read_u8()?,
        output_level: reader.read_u8()?,
        length_counter: reader.read_u8()?,
        linear_counter: reader.read_u8()?,
        linear_reload: reader.read_u8()?,
        control: reader.read_bool()?,
        reload_flag: reader.read_bool()?,
        pending_length_reload: reader.read_optional_u8()?,
        length_halt_before_write: read_optional_bool(reader)?,
    })
}

fn write_noise_state<S: StateSink>(sink: &mut S, noise: Noise) {
    sink.write_bool(noise.enabled);
    sink.write_u16(noise.timer_period);
    sink.write_u16(noise.timer_counter);
    sink.write_u16(noise.shift_register);
    sink.write_bool(noise.mode);
    sink.write_u8(noise.length_counter);
    sink.write_optional_u8(noise.pending_length_reload);
    write_optional_bool(sink, noise.length_halt_before_write);
    write_envelope_state(sink, noise.envelope);
}

fn read_noise_state(reader: &mut StateReader<'_>) -> Result<Noise, StateError> {
    Ok(Noise {
        enabled: reader.read_bool()?,
        timer_period: reader.read_u16()?,
        timer_counter: reader.read_u16()?,
        shift_register: reader.read_u16()?,
        mode: reader.read_bool()?,
        length_counter: reader.read_u8()?,
        pending_length_reload: reader.read_optional_u8()?,
        length_halt_before_write: read_optional_bool(reader)?,
        envelope: read_envelope_state(reader)?,
    })
}

fn write_optional_bool<S: StateSink>(sink: &mut S, value: Option<bool>) {
    sink.write_optional_u8(value.map(u8::from));
}

fn read_optional_bool(reader: &mut StateReader<'_>) -> Result<Option<bool>, StateError> {
    reader
        .read_optional_u8()?
        .map(|value| match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(StateError::InvalidPayload(
                "optional boolean field is not zero or one",
            )),
        })
        .transpose()
}

fn write_dmc_kind<S: StateSink>(sink: &mut S, kind: DmcDmaKind) {
    sink.write_u8(match kind {
        DmcDmaKind::Load => 0,
        DmcDmaKind::Reload => 1,
    });
}

fn read_dmc_kind(reader: &mut StateReader<'_>) -> Result<DmcDmaKind, StateError> {
    match reader.read_u8()? {
        0 => Ok(DmcDmaKind::Load),
        1 => Ok(DmcDmaKind::Reload),
        _ => Err(StateError::InvalidPayload("invalid DMC DMA kind")),
    }
}

fn write_dmc_state<S: StateSink>(sink: &mut S, dmc: Dmc) {
    sink.write_bool(dmc.irq_enabled);
    sink.write_bool(dmc.loop_flag);
    sink.write_bool(dmc.irq_pending);
    sink.write_u8(dmc.rate_index);
    sink.write_u16(dmc.timer_counter);
    sink.write_u8(dmc.output_level);
    sink.write_u8(dmc.sample_address_register);
    sink.write_u8(dmc.sample_length_register);
    sink.write_u16(dmc.current_address);
    sink.write_u16(dmc.bytes_remaining);
    sink.write_optional_u8(dmc.sample_buffer);
    sink.write_u8(dmc.shift_register);
    sink.write_u8(dmc.bits_remaining);
    sink.write_bool(dmc.silence);
    match dmc.delayed_dma {
        None => sink.write_u8(0),
        Some(delayed) => {
            sink.write_u8(1);
            sink.write_u8(delayed.clocks_remaining);
            write_dmc_kind(sink, delayed.kind);
        }
    }
    match dmc.dma_request {
        None => sink.write_u8(0),
        Some(request) => {
            sink.write_u8(1);
            sink.write_u16(request.address);
            write_dmc_kind(sink, request.kind);
        }
    }
}

fn read_dmc_state(reader: &mut StateReader<'_>) -> Result<Dmc, StateError> {
    let irq_enabled = reader.read_bool()?;
    let loop_flag = reader.read_bool()?;
    let irq_pending = reader.read_bool()?;
    let rate_index = reader.read_u8()?;
    let timer_counter = reader.read_u16()?;
    let output_level = reader.read_u8()?;
    let sample_address_register = reader.read_u8()?;
    let sample_length_register = reader.read_u8()?;
    let current_address = reader.read_u16()?;
    let bytes_remaining = reader.read_u16()?;
    let sample_buffer = reader.read_optional_u8()?;
    let shift_register = reader.read_u8()?;
    let bits_remaining = reader.read_u8()?;
    let silence = reader.read_bool()?;
    let delayed_dma = match reader.read_u8()? {
        0 => None,
        1 => Some(DelayedDmcDma {
            clocks_remaining: reader.read_u8()?,
            kind: read_dmc_kind(reader)?,
        }),
        _ => {
            return Err(StateError::InvalidPayload(
                "invalid delayed DMC DMA presence tag",
            ));
        }
    };
    let dma_request = match reader.read_u8()? {
        0 => None,
        1 => Some(DmcDmaRequest {
            address: reader.read_u16()?,
            kind: read_dmc_kind(reader)?,
        }),
        _ => {
            return Err(StateError::InvalidPayload(
                "invalid DMC DMA request presence tag",
            ));
        }
    };
    Ok(Dmc {
        irq_enabled,
        loop_flag,
        irq_pending,
        rate_index,
        timer_counter,
        output_level,
        sample_address_register,
        sample_length_register,
        current_address,
        bytes_remaining,
        sample_buffer,
        shift_register,
        bits_remaining,
        silence,
        delayed_dma,
        dma_request,
    })
}

fn write_frame_counter_state<S: StateSink>(sink: &mut S, frame_counter: FrameCounter) {
    sink.write_u32(frame_counter.sequence_cycle);
    sink.write_bool(frame_counter.mode_five_step);
    sink.write_bool(frame_counter.irq_inhibit);
    sink.write_bool(frame_counter.irq_pending);
    match frame_counter.pending_write {
        None => sink.write_u8(0),
        Some(write) => {
            sink.write_u8(1);
            sink.write_u64(write.apply_cycle);
            sink.write_bool(write.mode_five_step);
            sink.write_bool(write.irq_inhibit);
        }
    }
}

fn read_frame_counter_state(reader: &mut StateReader<'_>) -> Result<FrameCounter, StateError> {
    let sequence_cycle = reader.read_u32()?;
    let mode_five_step = reader.read_bool()?;
    let irq_inhibit = reader.read_bool()?;
    let irq_pending = reader.read_bool()?;
    let pending_write = match reader.read_u8()? {
        0 => None,
        1 => Some(PendingFrameCounterWrite {
            apply_cycle: reader.read_u64()?,
            mode_five_step: reader.read_bool()?,
            irq_inhibit: reader.read_bool()?,
        }),
        _ => {
            return Err(StateError::InvalidPayload(
                "invalid APU frame-counter write presence tag",
            ));
        }
    };
    Ok(FrameCounter {
        sequence_cycle,
        mode_five_step,
        irq_inhibit,
        irq_pending,
        pending_write,
    })
}

fn mix_channels(pulse1: u8, pulse2: u8, triangle: u8, noise: u8, dmc: u8) -> u16 {
    PULSE_MIX_TABLE[usize::from(pulse1 + pulse2)]
        + TND_MIX_TABLE[usize::from(3 * triangle + 2 * noise + dmc)]
}

const PULSE_MIX_TABLE: [u16; 31] = build_pulse_mix_table();
const TND_MIX_TABLE: [u16; 203] = build_tnd_mix_table();

const fn build_pulse_mix_table() -> [u16; 31] {
    let mut table = [0; 31];
    let mut index = 1;
    while index < table.len() {
        let numerator = 32_767_u64 * 9_552 * index as u64;
        let denominator = 100_u64 * (8_128 + 100 * index as u64);
        table[index] = (numerator / denominator) as u16;
        index += 1;
    }
    table
}

const fn build_tnd_mix_table() -> [u16; 203] {
    let mut table = [0; 203];
    let mut index = 1;
    while index < table.len() {
        let numerator = 32_767_u64 * 16_367 * index as u64;
        let denominator = 100_u64 * (24_329 + 100 * index as u64);
        table[index] = (numerator / denominator) as u16;
        index += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_envelope_sweep_and_linear_units_follow_frame_clocks() {
        let mut apu = Apu::default();
        apu.write_register(0x4015, 0x0f);
        apu.write_register(0x4000, 0x03);
        apu.write_register(0x4001, 0x89);
        apu.write_register(0x4002, 0xfd);
        apu.write_register(0x4003, 0x08);
        apu.write_register(0x4008, 0x14);
        apu.write_register(0x400b, 0x08);
        apu.write_register(0x400c, 0x03);
        apu.write_register(0x400f, 0x08);

        for _ in 0..7_457 {
            apu.clock();
        }
        let snapshot = apu.snapshot();
        assert_eq!(snapshot.pulse1.envelope.decay_level, 15);
        assert_eq!(snapshot.triangle.linear_counter, 20);
        assert_eq!(snapshot.pulse1.length_counter, 254);

        for _ in 7_457..14_913 {
            apu.clock();
        }
        let snapshot = apu.snapshot();
        assert_eq!(snapshot.pulse1.length_counter, 253);
        assert_eq!(snapshot.triangle.length_counter, 253);
        assert_eq!(snapshot.noise.length_counter, 253);
    }

    #[test]
    fn pulse_timer_and_mixer_generate_audible_fixed_rate_samples() {
        let mut apu = Apu::default();
        apu.write_register(0x4015, 0x01);
        apu.write_register(0x4000, 0xbf);
        apu.write_register(0x4002, 0xfd);
        apu.write_register(0x4003, 0x08);
        apu.begin_frame_output();
        for _ in 0..29_781 {
            apu.clock();
        }

        assert!((798..=799).contains(&apu.frame_samples().len()));
        assert!(apu.frame_samples().iter().any(|sample| *sample != 0));
        assert_eq!(
            apu.snapshot().total_samples,
            apu.frame_samples().len() as u64
        );
    }

    #[test]
    fn disabled_output_keeps_hardware_and_sample_cadence_identical() {
        let mut enabled = Apu::new(AudioOutput::Enabled);
        let mut disabled = Apu::new(AudioOutput::Disabled);
        for apu in [&mut enabled, &mut disabled] {
            apu.write_register(0x4015, 0x0f);
            apu.write_register(0x4000, 0xbf);
            apu.write_register(0x4002, 0xfd);
            apu.write_register(0x4003, 0x08);
        }
        for _ in 0..100_000 {
            enabled.clock();
            disabled.clock();
        }

        let mut enabled_snapshot = enabled.snapshot();
        let mut disabled_snapshot = disabled.snapshot();
        enabled_snapshot.audio_output = AudioOutput::Disabled;
        enabled_snapshot.frame_sample_count = 0;
        enabled_snapshot.mix_accumulator = 0;
        enabled_snapshot.mix_count = 0;
        enabled_snapshot.high_pass_previous_input = 0;
        enabled_snapshot.high_pass_previous_output = 0;
        disabled_snapshot.frame_sample_count = 0;
        assert_eq!(enabled_snapshot, disabled_snapshot);
        assert!(!enabled.frame_samples().is_empty());
        assert!(disabled.frame_samples().is_empty());
    }

    #[test]
    fn status_read_reports_lengths_and_clears_only_frame_irq() {
        let mut apu = Apu::default();
        apu.write_register(0x4015, 0x01);
        apu.write_register(0x4003, 0x08);
        for _ in 0..29_829 {
            apu.clock();
        }
        apu.dmc.irq_pending = true;
        assert_eq!(apu.read_status() & 0xc1, 0xc1);
        assert!(!apu.frame_counter.irq_pending);
        assert!(apu.dmc.irq_pending);
    }

    #[test]
    fn four_step_frame_irq_reasserts_across_its_three_hardware_edges() {
        let mut apu = Apu::default();
        for _ in 0..29_827 {
            apu.clock();
        }
        assert!(!apu.frame_counter.irq_pending);

        for expected_sequence_cycle in [29_828, 29_829] {
            apu.clock();
            assert_eq!(apu.frame_counter.sequence_cycle, expected_sequence_cycle);
            assert_ne!(apu.read_status() & 0x40, 0);
            assert!(!apu.frame_counter.irq_pending);
        }
        apu.clock();
        assert_eq!(apu.frame_counter.sequence_cycle, 0);
        assert_ne!(apu.read_status() & 0x40, 0);
    }

    #[test]
    fn dmc_reader_requests_dma_and_updates_address_length_and_irq() {
        let mut apu = Apu::default();
        apu.write_register(0x4010, 0x8f);
        apu.write_register(0x4012, 0xff);
        apu.write_register(0x4013, 0x00);
        apu.write_register(0x4015, 0x10);
        for _ in 0..4 {
            apu.clock();
        }
        assert_eq!(
            apu.take_dmc_dma_request(),
            Some(DmcDmaRequest {
                address: 0xffc0,
                kind: DmcDmaKind::Load,
            })
        );
        apu.complete_dmc_dma(0b1010_0101);
        let snapshot = apu.snapshot().dmc;
        assert_eq!(snapshot.bytes_remaining, 0);
        assert_eq!(snapshot.current_address, 0xffc1);
        assert_eq!(snapshot.sample_buffer, Some(0b1010_0101));
        assert!(snapshot.irq_pending);
    }

    #[test]
    fn integer_mixer_tables_cover_the_complete_channel_range() {
        assert_eq!(mix_channels(0, 0, 0, 0, 0), 0);
        let loudest = mix_channels(15, 15, 15, 15, 127);
        assert!(loudest > 32_000);
        assert!(loudest <= i16::MAX as u16);
    }

    #[test]
    fn state_round_trip_preserves_hardware_and_output_but_not_host_policy() {
        let mut source = Apu::default();
        source.write_register(0x4015, 0x0f);
        source.write_register(0x4000, 0xbf);
        source.write_register(0x4002, 0xfd);
        source.write_register(0x4003, 0x08);
        source.write_register(0x4008, 0xff);
        source.write_register(0x400a, 0x40);
        source.write_register(0x400b, 0x18);
        source.begin_frame_output();
        for _ in 0..12_345 {
            source.clock();
        }

        let mut bytes = Vec::new();
        source.write_state(&mut bytes, true);
        let mut restored = Apu::new(AudioOutput::Disabled);
        let mut reader = StateReader::new(&bytes);
        restored.read_state(&mut reader, true).unwrap();
        reader.finish().unwrap();

        let mut expected = source.snapshot();
        expected.audio_output = AudioOutput::Disabled;
        assert_eq!(restored.snapshot(), expected);
        assert!(restored.frame_samples().is_empty());

        bytes[0] = 2;
        assert!(matches!(
            Apu::default().read_state(&mut StateReader::new(&bytes), true),
            Err(StateError::InvalidPayload(_))
        ));
        bytes[0] = 1;
        assert!(matches!(
            Apu::default().read_state(&mut StateReader::new(&bytes[..10]), true),
            Err(StateError::Truncated { .. })
        ));
    }
}
