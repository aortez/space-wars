use crate::{
    Cartridge, StateError, VideoOutput,
    state_codec::{StateReader, StateSink},
};

pub const FRAME_WIDTH: usize = 256;
pub const FRAME_HEIGHT: usize = 240;
pub const FRAME_PIXELS: usize = FRAME_WIDTH * FRAME_HEIGHT;

const DOTS_PER_SCANLINE: u16 = 341;
const SCANLINES_PER_FRAME: u16 = 262;
const VBLANK_SCANLINE: u16 = 241;
const PRE_RENDER_SCANLINE: u16 = 261;

const CTRL_VRAM_INCREMENT: u8 = 1 << 2;
const CTRL_SPRITE_PATTERN: u8 = 1 << 3;
const CTRL_BACKGROUND_PATTERN: u8 = 1 << 4;
const CTRL_TALL_SPRITES: u8 = 1 << 5;
const CTRL_NMI_ENABLE: u8 = 1 << 7;

const MASK_GRAYSCALE: u8 = 1 << 0;
const MASK_BACKGROUND_LEFT: u8 = 1 << 1;
const MASK_SPRITES_LEFT: u8 = 1 << 2;
const MASK_BACKGROUND: u8 = 1 << 3;
const MASK_SPRITES: u8 = 1 << 4;

const STATUS_SPRITE_OVERFLOW: u8 = 1 << 5;
const STATUS_SPRITE_ZERO_HIT: u8 = 1 << 6;
const STATUS_VBLANK: u8 = 1 << 7;

/// Ricoh 2C02G palette used by the pinned DirtSim reference, encoded as
/// RGB565. Palette indices remain the authoritative framebuffer format; this
/// table is presentation-only.
pub const NES_PALETTE_RGB565: [u16; 64] = [
    25388, 365, 4367, 14511, 24684, 28743, 28801, 22720, 12608, 2464, 480, 482, 456, 0, 0, 0,
    44405, 4886, 17049, 31257, 41397, 47534, 47590, 39488, 27360, 15200, 2976, 967, 911, 0, 0, 0,
    65535, 23967, 36127, 50335, 62526, 64535, 64591, 60617, 48453, 34246, 22058, 15953, 17945,
    19049, 0, 0, 65535, 48895, 52927, 59039, 65151, 65116, 65145, 63158, 59092, 53013, 46902,
    44857, 44860, 46518, 0, 0,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PpuTiming {
    pub frame_id: u64,
    pub scanline: u16,
    pub dot: u16,
    pub odd_frame: bool,
    /// Physical PPU clocks consumed since power-on. The omitted odd-frame
    /// coordinate consumes no clock, so rendered odd frames are one shorter.
    pub clocks: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PpuRegisters {
    pub control: u8,
    pub mask: u8,
    pub status: u8,
    pub oam_address: u8,
    pub vram_address: u16,
    pub temporary_address: u16,
    pub fine_x: u8,
    pub second_write: bool,
    pub data_buffer: u8,
    pub io_bus: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PpuCycle {
    /// Current completed-frame count after this PPU clock. This increments on
    /// the same clock for which `frame_completed` is true.
    pub frame_id: u64,
    pub scanline: u16,
    pub dot: u16,
    pub frame_completed: bool,
    pub nmi_requested: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PpuSnapshot {
    pub timing: PpuTiming,
    pub registers: PpuRegisters,
    pub oam: Box<[u8; 0x100]>,
    pub nametables: Box<[u8; 0x1000]>,
    pub palette_ram: [u8; 0x20],
    pub scanline_sprite_pixels: Box<[u8; FRAME_WIDTH]>,
    pub nmi_output: bool,
    pub nmi_pending: bool,
}

/// Cycle-oriented NTSC Ricoh 2C02 PPU.
///
/// The implementation keeps the rendering fetch pipeline, scrolling
/// registers, and frame timing authoritative even when video output is
/// disabled. A host may therefore change presentation policy without changing
/// emulated machine behavior.
#[derive(Clone, Debug)]
pub struct Ppu {
    control: u8,
    mask: u8,
    status: u8,
    oam_address: u8,
    oam: Box<[u8; 0x100]>,
    nametables: Box<[u8; 0x1000]>,
    palette_ram: [u8; 0x20],
    data_buffer: u8,
    io_bus: u8,

    vram_address: u16,
    temporary_address: u16,
    fine_x: u8,
    second_write: bool,

    next_nametable: u8,
    next_attribute: u8,
    next_pattern_low: u8,
    next_pattern_high: u8,
    pattern_shift_low: u16,
    pattern_shift_high: u16,
    attribute_shift_low: u16,
    attribute_shift_high: u16,

    scanline_sprite_pixels: Box<[u8; FRAME_WIDTH]>,
    framebuffer: Box<[u8; FRAME_PIXELS]>,
    video_output: VideoOutput,

    frame_id: u64,
    scanline: u16,
    dot: u16,
    odd_frame: bool,
    clocks: u64,
    rendering_enabled_previous_dot: bool,
    nmi_output: bool,
    nmi_delay: Option<u8>,
    nmi_pending: bool,
    suppress_vblank: bool,
}

impl Ppu {
    pub fn new(video_output: VideoOutput) -> Self {
        Self {
            control: 0,
            mask: 0,
            status: 0,
            oam_address: 0,
            oam: Box::new([0; 0x100]),
            nametables: Box::new([0; 0x1000]),
            palette_ram: [0; 0x20],
            data_buffer: 0,
            io_bus: 0,
            vram_address: 0,
            temporary_address: 0,
            fine_x: 0,
            second_write: false,
            next_nametable: 0,
            next_attribute: 0,
            next_pattern_low: 0,
            next_pattern_high: 0,
            pattern_shift_low: 0,
            pattern_shift_high: 0,
            attribute_shift_low: 0,
            attribute_shift_high: 0,
            scanline_sprite_pixels: Box::new([0; FRAME_WIDTH]),
            framebuffer: Box::new([0; FRAME_PIXELS]),
            video_output,
            frame_id: 0,
            scanline: 0,
            dot: 0,
            odd_frame: false,
            clocks: 0,
            rendering_enabled_previous_dot: false,
            nmi_output: false,
            nmi_delay: None,
            nmi_pending: false,
            suppress_vblank: false,
        }
    }

    pub fn timing(&self) -> PpuTiming {
        PpuTiming {
            frame_id: self.frame_id,
            scanline: self.scanline,
            dot: self.dot,
            odd_frame: self.odd_frame,
            clocks: self.clocks,
        }
    }

    pub fn registers(&self) -> PpuRegisters {
        PpuRegisters {
            control: self.control,
            mask: self.mask,
            status: self.status,
            oam_address: self.oam_address,
            vram_address: self.vram_address,
            temporary_address: self.temporary_address,
            fine_x: self.fine_x,
            second_write: self.second_write,
            data_buffer: self.data_buffer,
            io_bus: self.io_bus,
        }
    }

    pub fn frame_id(&self) -> u64 {
        self.frame_id
    }

    pub fn framebuffer(&self) -> Option<&[u8; FRAME_PIXELS]> {
        matches!(self.video_output, VideoOutput::Enabled).then_some(&self.framebuffer)
    }

    pub fn oam(&self) -> &[u8; 0x100] {
        &self.oam
    }

    pub fn nametables(&self) -> &[u8; 0x1000] {
        &self.nametables
    }

    pub fn palette_ram(&self) -> &[u8; 0x20] {
        &self.palette_ram
    }

    pub fn snapshot(&self) -> PpuSnapshot {
        PpuSnapshot {
            timing: self.timing(),
            registers: self.registers(),
            oam: self.oam.clone(),
            nametables: self.nametables.clone(),
            palette_ram: self.palette_ram,
            scanline_sprite_pixels: self.scanline_sprite_pixels.clone(),
            nmi_output: self.nmi_output,
            nmi_pending: self.nmi_pending,
        }
    }

    pub(crate) fn copy_emulated_state_from(&mut self, source: &Self) {
        let video_output = self.video_output;
        self.control = source.control;
        self.mask = source.mask;
        self.status = source.status;
        self.oam_address = source.oam_address;
        self.oam.copy_from_slice(&source.oam[..]);
        self.nametables.copy_from_slice(&source.nametables[..]);
        self.palette_ram = source.palette_ram;
        self.data_buffer = source.data_buffer;
        self.io_bus = source.io_bus;
        self.vram_address = source.vram_address;
        self.temporary_address = source.temporary_address;
        self.fine_x = source.fine_x;
        self.second_write = source.second_write;
        self.next_nametable = source.next_nametable;
        self.next_attribute = source.next_attribute;
        self.next_pattern_low = source.next_pattern_low;
        self.next_pattern_high = source.next_pattern_high;
        self.pattern_shift_low = source.pattern_shift_low;
        self.pattern_shift_high = source.pattern_shift_high;
        self.attribute_shift_low = source.attribute_shift_low;
        self.attribute_shift_high = source.attribute_shift_high;
        self.scanline_sprite_pixels
            .copy_from_slice(&source.scanline_sprite_pixels[..]);
        self.framebuffer.copy_from_slice(&source.framebuffer[..]);
        self.frame_id = source.frame_id;
        self.scanline = source.scanline;
        self.dot = source.dot;
        self.odd_frame = source.odd_frame;
        self.clocks = source.clocks;
        self.rendering_enabled_previous_dot = source.rendering_enabled_previous_dot;
        self.nmi_output = source.nmi_output;
        self.nmi_delay = source.nmi_delay;
        self.nmi_pending = source.nmi_pending;
        self.suppress_vblank = source.suppress_vblank;
        self.video_output = video_output;
    }

    pub fn take_nmi_pending(&mut self) -> bool {
        let pending = self.nmi_pending;
        self.nmi_pending = false;
        pending
    }

    pub(crate) fn cpu_read_register(&mut self, register: usize, cartridge: &Cartridge) -> u8 {
        let value = match register & 7 {
            2 => {
                let value = (self.status & 0xe0) | (self.io_bus & 0x1f);
                self.status &= !STATUS_VBLANK;
                self.second_write = false;
                // A status read that overlaps the flag-setting dot prevents
                // both the flag and its NMI edge for this frame.
                if self.scanline == VBLANK_SCANLINE && self.dot == 1 {
                    self.suppress_vblank = true;
                }
                self.update_nmi_output();
                value
            }
            4 => {
                let value = self.oam[usize::from(self.oam_address)];
                if self.oam_address & 3 == 2 {
                    value & 0xe3
                } else {
                    value
                }
            }
            7 => {
                let address = self.vram_address & 0x3fff;
                let memory = self.read_memory(cartridge, address);
                let value = if address >= 0x3f00 {
                    self.data_buffer = self.read_memory(cartridge, address.wrapping_sub(0x1000));
                    (memory & 0x3f) | (self.io_bus & 0xc0)
                } else {
                    let buffered = self.data_buffer;
                    self.data_buffer = memory;
                    buffered
                };
                self.increment_vram_address();
                value
            }
            _ => self.io_bus,
        };
        self.io_bus = value;
        value
    }

    pub(crate) fn peek_cpu_register(&self, register: usize, cartridge: &Cartridge) -> u8 {
        match register & 7 {
            2 => (self.status & 0xe0) | (self.io_bus & 0x1f),
            4 => {
                let value = self.oam[usize::from(self.oam_address)];
                if self.oam_address & 3 == 2 {
                    value & 0xe3
                } else {
                    value
                }
            }
            7 if self.vram_address & 0x3fff >= 0x3f00 => {
                self.read_memory(cartridge, self.vram_address)
            }
            7 => self.data_buffer,
            _ => self.io_bus,
        }
    }

    pub(crate) fn cpu_write_register(
        &mut self,
        register: usize,
        value: u8,
        cartridge: &mut Cartridge,
    ) {
        self.io_bus = value;
        match register & 7 {
            0 => {
                self.control = value;
                self.temporary_address =
                    (self.temporary_address & !0x0c00) | (u16::from(value & 3) << 10);
                self.update_nmi_output();
            }
            1 => self.mask = value,
            3 => self.oam_address = value,
            4 => self.write_oam(value),
            5 if !self.second_write => {
                self.temporary_address = (self.temporary_address & !0x001f) | u16::from(value >> 3);
                self.fine_x = value & 7;
                self.second_write = true;
            }
            5 => {
                self.temporary_address = (self.temporary_address & !0x73e0)
                    | (u16::from(value & 7) << 12)
                    | (u16::from(value & 0xf8) << 2);
                self.second_write = false;
            }
            6 if !self.second_write => {
                self.temporary_address =
                    (self.temporary_address & 0x00ff) | (u16::from(value & 0x3f) << 8);
                self.second_write = true;
            }
            6 => {
                self.temporary_address = (self.temporary_address & 0x7f00) | u16::from(value);
                self.vram_address = self.temporary_address;
                self.second_write = false;
            }
            7 => {
                self.write_memory(cartridge, self.vram_address, value);
                self.increment_vram_address();
            }
            _ => {}
        }
    }

    pub(crate) fn write_oam_dma(&mut self, value: u8) {
        self.io_bus = value;
        self.write_oam(value);
    }

    pub(crate) fn memory_peek(&self, cartridge: &Cartridge, address: u16) -> u8 {
        self.read_memory(cartridge, address)
    }

    pub(crate) fn memory_write(&mut self, cartridge: &mut Cartridge, address: u16, value: u8) {
        self.write_memory(cartridge, address, value);
    }

    pub(crate) fn clock(&mut self, cartridge: &Cartridge) -> PpuCycle {
        let scanline = self.scanline;
        let dot = self.dot;
        let mut frame_completed = false;
        let nmi_was_pending = self.nmi_pending;

        if scanline == PRE_RENDER_SCANLINE && dot == 1 {
            // An NMI edge enabled during the last two dots of vblank has
            // already propagated far enough to reach the CPU even though the
            // PPU lowers its output as the pre-render line begins.
            if self.nmi_output && matches!(self.nmi_delay, Some(0 | 1)) {
                self.nmi_pending = true;
                self.nmi_delay = None;
            }
            self.status &= !(STATUS_VBLANK | STATUS_SPRITE_ZERO_HIT | STATUS_SPRITE_OVERFLOW);
            self.update_nmi_output();
        } else if scanline == VBLANK_SCANLINE && dot == 1 {
            self.frame_id = self.frame_id.wrapping_add(1);
            frame_completed = true;
            if self.suppress_vblank {
                self.suppress_vblank = false;
            } else {
                self.status |= STATUS_VBLANK;
            }
            self.update_nmi_output();
        }

        let visible_scanline = scanline < FRAME_HEIGHT as u16;
        let rendering_scanline = visible_scanline || scanline == PRE_RENDER_SCANLINE;
        let rendering_enabled = self.rendering_enabled();

        if visible_scanline && dot == 0 {
            self.evaluate_scanline_sprites(cartridge, scanline);
        }

        if rendering_enabled && rendering_scanline {
            if (2..=257).contains(&dot) || (322..=337).contains(&dot) {
                self.shift_background_registers();
            }

            if (1..=257).contains(&dot) || (321..=337).contains(&dot) {
                match (dot - 1) & 7 {
                    0 => {
                        self.load_background_registers();
                        self.next_nametable =
                            self.read_memory(cartridge, 0x2000 | (self.vram_address & 0x0fff));
                    }
                    2 => {
                        let address = 0x23c0
                            | (self.vram_address & 0x0c00)
                            | ((self.vram_address >> 4) & 0x38)
                            | ((self.vram_address >> 2) & 0x07);
                        let attribute = self.read_memory(cartridge, address);
                        let shift = ((self.vram_address >> 4) & 4) | (self.vram_address & 2);
                        self.next_attribute = (attribute >> shift) & 3;
                    }
                    4 => {
                        let base = if self.control & CTRL_BACKGROUND_PATTERN != 0 {
                            0x1000
                        } else {
                            0
                        };
                        let address = base
                            | (u16::from(self.next_nametable) << 4)
                            | ((self.vram_address >> 12) & 7);
                        self.next_pattern_low = self.read_memory(cartridge, address);
                    }
                    6 => {
                        let base = if self.control & CTRL_BACKGROUND_PATTERN != 0 {
                            0x1000
                        } else {
                            0
                        };
                        let address = base
                            | (u16::from(self.next_nametable) << 4)
                            | ((self.vram_address >> 12) & 7)
                            | 8;
                        self.next_pattern_high = self.read_memory(cartridge, address);
                    }
                    7 => self.increment_coarse_x(),
                    _ => {}
                }
            }

            if dot == 256 {
                self.increment_y();
            } else if dot == 257 {
                self.copy_horizontal_scroll();
            } else if scanline == PRE_RENDER_SCANLINE && (280..=304).contains(&dot) {
                self.copy_vertical_scroll();
            }

            if dot == 338 || dot == 340 {
                self.next_nametable =
                    self.read_memory(cartridge, 0x2000 | (self.vram_address & 0x0fff));
            }
        }

        // The visible pixel observes the shifters after this dot's rendering
        // pipeline update. Dot 1 uses the values prefetched on the pre-render
        // line; dots 2-256 shift exactly once before sampling.
        if visible_scanline && (1..=256).contains(&dot) {
            self.render_pixel(dot - 1, scanline);
        }

        self.clock_nmi_delay();
        self.advance_timing(rendering_enabled);
        self.clocks = self.clocks.wrapping_add(1);

        PpuCycle {
            frame_id: self.frame_id,
            scanline,
            dot,
            frame_completed,
            nmi_requested: !nmi_was_pending && self.nmi_pending,
        }
    }

    pub(crate) fn write_state<S: StateSink>(&self, sink: &mut S, include_framebuffer: bool) {
        sink.write_u8(self.control);
        sink.write_u8(self.mask);
        sink.write_u8(self.status);
        sink.write_u8(self.oam_address);
        sink.write(&self.oam[..]);
        sink.write(&self.nametables[..]);
        sink.write(&self.palette_ram);
        sink.write_u8(self.data_buffer);
        sink.write_u8(self.io_bus);
        sink.write_u16(self.vram_address);
        sink.write_u16(self.temporary_address);
        sink.write_u8(self.fine_x);
        sink.write_bool(self.second_write);
        sink.write_u8(self.next_nametable);
        sink.write_u8(self.next_attribute);
        sink.write_u8(self.next_pattern_low);
        sink.write_u8(self.next_pattern_high);
        sink.write_u16(self.pattern_shift_low);
        sink.write_u16(self.pattern_shift_high);
        sink.write_u16(self.attribute_shift_low);
        sink.write_u16(self.attribute_shift_high);
        sink.write(&self.scanline_sprite_pixels[..]);
        if include_framebuffer {
            sink.write(&self.framebuffer[..]);
        }
        sink.write_u64(self.frame_id);
        sink.write_u16(self.scanline);
        sink.write_u16(self.dot);
        sink.write_bool(self.odd_frame);
        sink.write_u64(self.clocks);
        sink.write_bool(self.rendering_enabled_previous_dot);
        sink.write_bool(self.nmi_output);
        sink.write_optional_u8(self.nmi_delay);
        sink.write_bool(self.nmi_pending);
        sink.write_bool(self.suppress_vblank);
    }

    pub(crate) fn read_state(
        &mut self,
        reader: &mut StateReader<'_>,
        include_framebuffer: bool,
    ) -> Result<(), StateError> {
        self.control = reader.read_u8()?;
        self.mask = reader.read_u8()?;
        self.status = reader.read_u8()?;
        self.oam_address = reader.read_u8()?;
        self.oam.copy_from_slice(reader.read_bytes(0x100)?);
        self.nametables.copy_from_slice(reader.read_bytes(0x1000)?);
        self.palette_ram.copy_from_slice(reader.read_bytes(0x20)?);
        self.data_buffer = reader.read_u8()?;
        self.io_bus = reader.read_u8()?;
        self.vram_address = reader.read_u16()?;
        self.temporary_address = reader.read_u16()?;
        self.fine_x = reader.read_u8()?;
        self.second_write = reader.read_bool()?;
        self.next_nametable = reader.read_u8()?;
        self.next_attribute = reader.read_u8()?;
        self.next_pattern_low = reader.read_u8()?;
        self.next_pattern_high = reader.read_u8()?;
        self.pattern_shift_low = reader.read_u16()?;
        self.pattern_shift_high = reader.read_u16()?;
        self.attribute_shift_low = reader.read_u16()?;
        self.attribute_shift_high = reader.read_u16()?;
        self.scanline_sprite_pixels
            .copy_from_slice(reader.read_bytes(FRAME_WIDTH)?);
        if include_framebuffer {
            self.framebuffer
                .copy_from_slice(reader.read_bytes(FRAME_PIXELS)?);
        }
        self.frame_id = reader.read_u64()?;
        self.scanline = reader.read_u16()?;
        self.dot = reader.read_u16()?;
        self.odd_frame = reader.read_bool()?;
        self.clocks = reader.read_u64()?;
        self.rendering_enabled_previous_dot = reader.read_bool()?;
        self.nmi_output = reader.read_bool()?;
        self.nmi_delay = reader.read_optional_u8()?;
        self.nmi_pending = reader.read_bool()?;
        self.suppress_vblank = reader.read_bool()?;

        if self.status & !0xe0 != 0 {
            return Err(StateError::InvalidPayload("PPU status has unknown bits"));
        }
        if self.vram_address > 0x7fff || self.temporary_address > 0x7fff {
            return Err(StateError::InvalidPayload(
                "PPU scrolling address exceeds 15 bits",
            ));
        }
        if self.fine_x > 7 || self.next_attribute > 3 {
            return Err(StateError::InvalidPayload(
                "PPU fine scroll or attribute latch is out of range",
            ));
        }
        if self.palette_ram.iter().any(|value| *value > 0x3f) {
            return Err(StateError::InvalidPayload(
                "PPU palette contains an out-of-range color",
            ));
        }
        if self
            .scanline_sprite_pixels
            .iter()
            .any(|value| value & 0xc0 != 0)
        {
            return Err(StateError::InvalidPayload(
                "PPU scanline sprite data has unknown bits",
            ));
        }
        if include_framebuffer && self.framebuffer.iter().any(|value| *value > 0x3f) {
            return Err(StateError::InvalidPayload(
                "PPU framebuffer contains an out-of-range color",
            ));
        }
        if self.scanline >= SCANLINES_PER_FRAME || self.dot >= DOTS_PER_SCANLINE {
            return Err(StateError::InvalidPayload("PPU timing is out of range"));
        }
        if self.nmi_delay.is_some_and(|delay| delay > 2) {
            return Err(StateError::InvalidPayload("PPU NMI delay is out of range"));
        }
        let expected_nmi_output =
            self.status & STATUS_VBLANK != 0 && self.control & CTRL_NMI_ENABLE != 0;
        if self.nmi_output != expected_nmi_output {
            return Err(StateError::InvalidPayload(
                "PPU NMI output disagrees with its control and status lines",
            ));
        }
        Ok(())
    }

    fn rendering_enabled(&self) -> bool {
        self.mask & (MASK_BACKGROUND | MASK_SPRITES) != 0
    }

    fn update_nmi_output(&mut self) {
        let output = self.status & STATUS_VBLANK != 0 && self.control & CTRL_NMI_ENABLE != 0;
        if output && !self.nmi_output {
            // The 2C02 output edge takes two PPU clocks to reach the CPU's
            // edge detector. Keeping this delay explicit is also essential
            // for the documented short-pulse suppression windows.
            self.nmi_delay = Some(2);
        } else if !output {
            self.nmi_delay = None;
        }
        self.nmi_output = output;
    }

    fn clock_nmi_delay(&mut self) {
        let Some(delay) = self.nmi_delay else {
            return;
        };
        if delay == 0 {
            self.nmi_delay = None;
            if self.nmi_output {
                self.nmi_pending = true;
            }
        } else {
            self.nmi_delay = Some(delay - 1);
        }
    }

    fn increment_vram_address(&mut self) {
        let increment = if self.control & CTRL_VRAM_INCREMENT != 0 {
            32
        } else {
            1
        };
        self.vram_address = self.vram_address.wrapping_add(increment) & 0x7fff;
    }

    fn write_oam(&mut self, value: u8) {
        self.oam[usize::from(self.oam_address)] = value;
        self.oam_address = self.oam_address.wrapping_add(1);
    }

    fn read_memory(&self, cartridge: &Cartridge, address: u16) -> u8 {
        let address = address & 0x3fff;
        match address {
            0x0000..=0x1fff => cartridge.ppu_read(address).unwrap_or(0),
            0x2000..=0x3eff => {
                self.nametables[cartridge.mirroring().map_nametable_address(address)]
            }
            0x3f00..=0x3fff => self.palette_ram[palette_index(address)] & 0x3f,
            _ => unreachable!("PPU address is masked to 14 bits"),
        }
    }

    fn write_memory(&mut self, cartridge: &mut Cartridge, address: u16, value: u8) {
        let address = address & 0x3fff;
        match address {
            0x0000..=0x1fff => {
                cartridge.ppu_write(address, value);
            }
            0x2000..=0x3eff => {
                self.nametables[cartridge.mirroring().map_nametable_address(address)] = value;
            }
            0x3f00..=0x3fff => self.palette_ram[palette_index(address)] = value & 0x3f,
            _ => unreachable!("PPU address is masked to 14 bits"),
        }
    }

    fn shift_background_registers(&mut self) {
        self.pattern_shift_low <<= 1;
        self.pattern_shift_high <<= 1;
        self.attribute_shift_low <<= 1;
        self.attribute_shift_high <<= 1;
    }

    fn load_background_registers(&mut self) {
        self.pattern_shift_low =
            (self.pattern_shift_low & 0xff00) | u16::from(self.next_pattern_low);
        self.pattern_shift_high =
            (self.pattern_shift_high & 0xff00) | u16::from(self.next_pattern_high);
        self.attribute_shift_low = (self.attribute_shift_low & 0xff00)
            | if self.next_attribute & 1 != 0 {
                0xff
            } else {
                0
            };
        self.attribute_shift_high = (self.attribute_shift_high & 0xff00)
            | if self.next_attribute & 2 != 0 {
                0xff
            } else {
                0
            };
    }

    fn increment_coarse_x(&mut self) {
        if self.vram_address & 0x001f == 31 {
            self.vram_address &= !0x001f;
            self.vram_address ^= 0x0400;
        } else {
            self.vram_address = self.vram_address.wrapping_add(1);
        }
    }

    fn increment_y(&mut self) {
        if self.vram_address & 0x7000 != 0x7000 {
            self.vram_address = self.vram_address.wrapping_add(0x1000);
            return;
        }

        self.vram_address &= !0x7000;
        let mut coarse_y = (self.vram_address & 0x03e0) >> 5;
        if coarse_y == 29 {
            coarse_y = 0;
            self.vram_address ^= 0x0800;
        } else if coarse_y == 31 {
            coarse_y = 0;
        } else {
            coarse_y += 1;
        }
        self.vram_address = (self.vram_address & !0x03e0) | (coarse_y << 5);
    }

    fn copy_horizontal_scroll(&mut self) {
        self.vram_address = (self.vram_address & !0x041f) | (self.temporary_address & 0x041f);
    }

    fn copy_vertical_scroll(&mut self) {
        self.vram_address = (self.vram_address & !0x7be0) | (self.temporary_address & 0x7be0);
    }

    fn evaluate_scanline_sprites(&mut self, cartridge: &Cartridge, scanline: u16) {
        self.scanline_sprite_pixels.fill(0);
        if !self.rendering_enabled() {
            return;
        }

        let height = if self.control & CTRL_TALL_SPRITES != 0 {
            16
        } else {
            8
        };
        let mut selected = 0;

        for sprite_index in 0..64 {
            let offset = sprite_index * 4;
            let top = i16::from(self.oam[offset]) + 1;
            let row = scanline as i16 - top;
            if row < 0 || row >= height {
                continue;
            }
            if selected == 8 {
                self.status |= STATUS_SPRITE_OVERFLOW;
                break;
            }
            selected += 1;

            let tile = self.oam[offset + 1];
            let attributes = self.oam[offset + 2];
            let start_x = usize::from(self.oam[offset + 3]);
            let mut pattern_row = row as u16;
            if attributes & 0x80 != 0 {
                pattern_row = height as u16 - 1 - pattern_row;
            }

            let pattern_address = if height == 16 {
                let table = u16::from(tile & 1) << 12;
                let tile = u16::from(tile & 0xfe) + pattern_row / 8;
                table | (tile << 4) | (pattern_row & 7)
            } else {
                let table = if self.control & CTRL_SPRITE_PATTERN != 0 {
                    0x1000
                } else {
                    0
                };
                table | (u16::from(tile) << 4) | pattern_row
            };
            let pattern_low = self.read_memory(cartridge, pattern_address);
            let pattern_high = self.read_memory(cartridge, pattern_address | 8);

            for sprite_x in 0..8 {
                let screen_x = start_x + sprite_x;
                if screen_x >= FRAME_WIDTH {
                    break;
                }
                if self.scanline_sprite_pixels[screen_x] != 0 {
                    continue;
                }
                let bit = if attributes & 0x40 != 0 {
                    sprite_x
                } else {
                    7 - sprite_x
                };
                let color = ((pattern_high >> bit) & 1) << 1 | ((pattern_low >> bit) & 1);
                if color == 0 {
                    continue;
                }
                self.scanline_sprite_pixels[screen_x] = color
                    | ((attributes & 3) << 2)
                    | if attributes & 0x20 != 0 { 0x10 } else { 0 }
                    | if sprite_index == 0 { 0x20 } else { 0 };
            }
        }
    }

    fn render_pixel(&mut self, x: u16, y: u16) {
        let show_background =
            self.mask & MASK_BACKGROUND != 0 && (x >= 8 || self.mask & MASK_BACKGROUND_LEFT != 0);
        let show_sprites =
            self.mask & MASK_SPRITES != 0 && (x >= 8 || self.mask & MASK_SPRITES_LEFT != 0);

        let (background_color, background_palette) = if show_background {
            let mux = 0x8000 >> self.fine_x;
            let color = u8::from(self.pattern_shift_low & mux != 0)
                | (u8::from(self.pattern_shift_high & mux != 0) << 1);
            let palette = u8::from(self.attribute_shift_low & mux != 0)
                | (u8::from(self.attribute_shift_high & mux != 0) << 1);
            (color, palette)
        } else {
            (0, 0)
        };

        let sprite = if show_sprites {
            self.scanline_sprite_pixels[usize::from(x)]
        } else {
            0
        };
        let sprite_color = sprite & 3;

        if background_color != 0 && sprite_color != 0 && sprite & 0x20 != 0 && x != 255 {
            self.status |= STATUS_SPRITE_ZERO_HIT;
        }

        let palette_address = match (background_color != 0, sprite_color != 0) {
            (false, false) => 0,
            (true, false) => (background_palette << 2) | background_color,
            (false, true) => 0x10 | (sprite & 0x0f),
            (true, true) if sprite & 0x10 != 0 => (background_palette << 2) | background_color,
            (true, true) => 0x10 | (sprite & 0x0f),
        };
        let mut palette_index =
            self.palette_ram[palette_index(0x3f00 + u16::from(palette_address))];
        if self.mask & MASK_GRAYSCALE != 0 {
            palette_index &= 0x30;
        } else {
            palette_index &= 0x3f;
        }

        if matches!(self.video_output, VideoOutput::Enabled) {
            self.framebuffer[usize::from(y) * FRAME_WIDTH + usize::from(x)] = palette_index;
        }
    }

    fn advance_timing(&mut self, rendering_enabled: bool) {
        if self.scanline == PRE_RENDER_SCANLINE
            && self.dot == 339
            && self.odd_frame
            && self.rendering_enabled_previous_dot
        {
            self.dot = 0;
            self.scanline = 0;
            self.odd_frame = false;
            self.rendering_enabled_previous_dot = rendering_enabled;
            return;
        }

        self.dot += 1;
        if self.dot == DOTS_PER_SCANLINE {
            self.dot = 0;
            self.scanline += 1;
            if self.scanline == SCANLINES_PER_FRAME {
                self.scanline = 0;
                self.odd_frame = !self.odd_frame;
            }
        }
        self.rendering_enabled_previous_dot = rendering_enabled;
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new(VideoOutput::Enabled)
    }
}

pub fn rgb565_to_rgb888(color: u16) -> [u8; 3] {
    let red = u32::from((color >> 11) & 0x1f);
    let green = u32::from((color >> 5) & 0x3f);
    let blue = u32::from(color & 0x1f);
    [
        ((red * 255 + 15) / 31) as u8,
        ((green * 255 + 31) / 63) as u8,
        ((blue * 255 + 15) / 31) as u8,
    ]
}

/// Converts a complete palette-index frame into packed RGB888 without
/// allocating. Returns `false` when either slice has the wrong length.
pub fn write_rgb888(indices: &[u8], output: &mut [u8]) -> bool {
    if indices.len() != FRAME_PIXELS || output.len() != FRAME_PIXELS * 3 {
        return false;
    }
    for (index, pixel) in indices.iter().zip(output.chunks_exact_mut(3)) {
        pixel.copy_from_slice(&rgb565_to_rgb888(
            NES_PALETTE_RGB565[usize::from(index & 0x3f)],
        ));
    }
    true
}

const fn palette_index(address: u16) -> usize {
    let mut index = (address as usize) & 0x1f;
    if index >= 0x10 && index & 3 == 0 {
        index -= 0x10;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CartridgeImage, Mirroring, test_rom::NromBuilder};

    fn cartridge(vertical: bool, chr_ram: bool) -> Cartridge {
        let mut rom = if chr_ram {
            NromBuilder::new_16k().without_chr()
        } else {
            NromBuilder::new_16k()
        };
        rom.set_vertical_mirroring(vertical);
        Cartridge::new(CartridgeImage::parse(&rom.build()).unwrap())
    }

    fn write_address(ppu: &mut Ppu, cartridge: &mut Cartridge, address: u16) {
        ppu.cpu_write_register(6, (address >> 8) as u8, cartridge);
        ppu.cpu_write_register(6, address as u8, cartridge);
    }

    #[test]
    fn mirrors_nametables_and_palette_aliases() {
        let mut horizontal = cartridge(false, false);
        let mut ppu = Ppu::default();
        ppu.memory_write(&mut horizontal, 0x2000, 0x12);
        ppu.memory_write(&mut horizontal, 0x2800, 0x34);
        assert_eq!(ppu.memory_peek(&horizontal, 0x2400), 0x12);
        assert_eq!(ppu.memory_peek(&horizontal, 0x2c00), 0x34);
        assert_eq!(ppu.memory_peek(&horizontal, 0x3000), 0x12);

        let mut vertical = cartridge(true, false);
        let mut ppu = Ppu::default();
        ppu.memory_write(&mut vertical, 0x2000, 0x56);
        ppu.memory_write(&mut vertical, 0x2400, 0x78);
        assert_eq!(ppu.memory_peek(&vertical, 0x2800), 0x56);
        assert_eq!(ppu.memory_peek(&vertical, 0x2c00), 0x78);

        ppu.memory_write(&mut vertical, 0x3f00, 0x21);
        assert_eq!(ppu.memory_peek(&vertical, 0x3f10), 0x21);
        ppu.memory_write(&mut vertical, 0x3f24, 0x3f);
        assert_eq!(ppu.memory_peek(&vertical, 0x3f04), 0x3f);
    }

    #[test]
    fn ppu_data_buffers_non_palette_reads_and_honors_increment_mode() {
        let mut cartridge = cartridge(false, true);
        let mut ppu = Ppu::default();
        ppu.memory_write(&mut cartridge, 0x0010, 0xa5);
        ppu.memory_write(&mut cartridge, 0x0011, 0x5a);
        write_address(&mut ppu, &mut cartridge, 0x0010);
        assert_eq!(ppu.cpu_read_register(7, &cartridge), 0);
        assert_eq!(ppu.cpu_read_register(7, &cartridge), 0xa5);
        assert_eq!(ppu.registers().vram_address, 0x0012);

        ppu.cpu_write_register(0, CTRL_VRAM_INCREMENT, &mut cartridge);
        write_address(&mut ppu, &mut cartridge, 0x2000);
        ppu.cpu_write_register(7, 0x33, &mut cartridge);
        assert_eq!(ppu.registers().vram_address, 0x2020);
        assert_eq!(ppu.memory_peek(&cartridge, 0x2000), 0x33);

        ppu.memory_write(&mut cartridge, 0x3f00, 0x2a);
        write_address(&mut ppu, &mut cartridge, 0x3f00);
        ppu.cpu_write_register(1, 0xc0, &mut cartridge);
        assert_eq!(ppu.cpu_read_register(7, &cartridge), 0xea);
    }

    #[test]
    fn scroll_and_address_writes_share_the_hardware_latch() {
        let mut cartridge = cartridge(false, false);
        let mut ppu = Ppu::default();
        ppu.cpu_write_register(5, 0x2d, &mut cartridge);
        assert_eq!(ppu.registers().fine_x, 5);
        assert!(ppu.registers().second_write);
        ppu.cpu_write_register(5, 0xa6, &mut cartridge);
        assert!(!ppu.registers().second_write);
        assert_eq!(ppu.registers().temporary_address & 0x73ff, 0x6285);

        ppu.cpu_write_register(6, 0x3f, &mut cartridge);
        ppu.cpu_read_register(2, &cartridge);
        assert!(!ppu.registers().second_write);
        ppu.cpu_write_register(6, 0x21, &mut cartridge);
        ppu.cpu_write_register(6, 0x43, &mut cartridge);
        assert_eq!(ppu.registers().vram_address, 0x2143);
    }

    #[test]
    fn vblank_sets_at_241_dot_one_and_nmi_is_edge_triggered() {
        let mut cartridge = cartridge(false, false);
        let mut ppu = Ppu::default();
        ppu.cpu_write_register(0, CTRL_NMI_ENABLE, &mut cartridge);

        for _ in 0..(VBLANK_SCANLINE as usize * DOTS_PER_SCANLINE as usize + 1) {
            assert!(!ppu.clock(&cartridge).frame_completed);
        }
        let cycle = ppu.clock(&cartridge);
        assert_eq!((cycle.scanline, cycle.dot), (VBLANK_SCANLINE, 1));
        assert!(cycle.frame_completed);
        assert_eq!(cycle.frame_id, 1);
        assert!(!cycle.nmi_requested);
        assert_eq!(ppu.frame_id(), 1);
        assert!(!ppu.clock(&cartridge).nmi_requested);
        assert!(ppu.clock(&cartridge).nmi_requested);
        assert!(ppu.take_nmi_pending());
        assert!(!ppu.take_nmi_pending());

        let status = ppu.cpu_read_register(2, &cartridge);
        assert_ne!(status & STATUS_VBLANK, 0);
        assert_eq!(ppu.registers().status & STATUS_VBLANK, 0);
    }

    #[test]
    fn enabling_nmi_during_vblank_requests_a_new_edge() {
        let mut cartridge = cartridge(false, false);
        let mut ppu = Ppu::default();
        for _ in 0..=(VBLANK_SCANLINE as usize * DOTS_PER_SCANLINE as usize + 1) {
            ppu.clock(&cartridge);
        }
        assert!(!ppu.take_nmi_pending());
        ppu.cpu_write_register(0, CTRL_NMI_ENABLE, &mut cartridge);
        for _ in 0..3 {
            ppu.clock(&cartridge);
        }
        assert!(ppu.take_nmi_pending());
        ppu.cpu_write_register(0, 0, &mut cartridge);
        ppu.cpu_write_register(0, CTRL_NMI_ENABLE, &mut cartridge);
        for _ in 0..3 {
            ppu.clock(&cartridge);
        }
        assert!(ppu.take_nmi_pending());
    }

    #[test]
    fn odd_rendering_frame_skips_one_coordinate() {
        let cartridge = cartridge(false, false);
        let mut ppu = Ppu {
            mask: MASK_BACKGROUND,
            ..Ppu::default()
        };

        let first_start = ppu.clocks;
        while ppu.scanline != 0 || ppu.dot != 0 || ppu.clocks == first_start {
            ppu.clock(&cartridge);
        }
        let first = ppu.clocks - first_start;
        let second_start = ppu.clocks;
        while ppu.scanline != 0 || ppu.dot != 0 || ppu.clocks == second_start {
            ppu.clock(&cartridge);
        }
        let second = ppu.clocks - second_start;
        assert_eq!(first, 89_342);
        assert_eq!(second, 89_341);
    }

    #[test]
    fn sprite_evaluation_limits_a_scanline_to_eight_and_sets_overflow() {
        let mut cartridge = cartridge(false, true);
        let mut ppu = Ppu {
            mask: MASK_SPRITES | MASK_SPRITES_LEFT,
            ..Ppu::default()
        };
        ppu.oam.fill(0xff);
        ppu.memory_write(&mut cartridge, 0x0010, 0x80);
        for sprite in 0..9 {
            let offset = sprite * 4;
            ppu.oam[offset] = 0;
            ppu.oam[offset + 1] = 1;
            ppu.oam[offset + 2] = 0;
            ppu.oam[offset + 3] = (sprite * 8) as u8;
        }

        ppu.evaluate_scanline_sprites(&cartridge, 1);
        assert_ne!(ppu.status & STATUS_SPRITE_OVERFLOW, 0);
        for sprite in 0..8 {
            assert_ne!(ppu.scanline_sprite_pixels[sprite * 8], 0);
        }
        assert_eq!(ppu.scanline_sprite_pixels[64], 0);
    }

    #[test]
    fn sprite_priority_and_sprite_zero_hit_are_composited_per_pixel() {
        let mut cartridge = cartridge(false, false);
        let mut ppu = Ppu {
            mask: MASK_BACKGROUND | MASK_SPRITES | MASK_BACKGROUND_LEFT | MASK_SPRITES_LEFT,
            ..Ppu::default()
        };
        ppu.memory_write(&mut cartridge, 0x3f01, 0x21);
        ppu.memory_write(&mut cartridge, 0x3f11, 0x30);
        ppu.pattern_shift_low = 0x8000;
        ppu.scanline_sprite_pixels[10] = 0x20 | 1;

        ppu.render_pixel(10, 0);
        assert_ne!(ppu.status & STATUS_SPRITE_ZERO_HIT, 0);
        assert_eq!(ppu.framebuffer[10], 0x30);

        ppu.status &= !STATUS_SPRITE_ZERO_HIT;
        ppu.scanline_sprite_pixels[11] = 0x20 | 0x10 | 1;
        ppu.render_pixel(11, 0);
        assert_ne!(ppu.status & STATUS_SPRITE_ZERO_HIT, 0);
        assert_eq!(ppu.framebuffer[11], 0x21);
    }

    #[test]
    fn horizontal_and_vertical_sprite_flips_select_the_expected_pattern_bit() {
        let mut cartridge = cartridge(false, true);
        let mut ppu = Ppu {
            mask: MASK_SPRITES | MASK_SPRITES_LEFT,
            ..Ppu::default()
        };
        ppu.oam.fill(0xff);
        // Tile one has a pixel at its upper-left; its last row has a pixel at
        // the upper-right after horizontal and vertical flipping.
        ppu.memory_write(&mut cartridge, 0x0010, 0x80);
        ppu.memory_write(&mut cartridge, 0x0017, 0x80);
        ppu.oam[0..4].copy_from_slice(&[0, 1, 0, 10]);
        ppu.oam[4..8].copy_from_slice(&[0, 1, 0xc0, 20]);

        ppu.evaluate_scanline_sprites(&cartridge, 1);
        assert_ne!(ppu.scanline_sprite_pixels[10], 0);
        assert_ne!(ppu.scanline_sprite_pixels[27], 0);
        assert_eq!(ppu.scanline_sprite_pixels[20], 0);
    }

    #[test]
    fn conversion_expands_the_complete_palette_frame_without_allocating() {
        let mut indices = [0; FRAME_PIXELS];
        indices[0] = 0x20;
        let mut rgb = [0; FRAME_PIXELS * 3];
        assert!(write_rgb888(&indices, &mut rgb));
        assert_eq!(&rgb[0..3], &rgb565_to_rgb888(NES_PALETTE_RGB565[0x20]));
        assert!(!write_rgb888(&indices[..1], &mut rgb));
    }

    #[test]
    fn mirroring_mapper_is_used_for_all_render_memory() {
        assert_eq!(Mirroring::Horizontal.map_nametable_address(0x2c00), 0x400);
    }
}
