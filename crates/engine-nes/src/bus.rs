use crate::{
    Apu, ApuSnapshot, AudioOutput, CHR_MEMORY_BYTES, Cartridge, ControllerButtons, ControllerPort,
    ControllerSnapshot, DmcDmaRequest, OamDmaAlignment, Ppu, PpuCycle, RamInit, StateError,
    VideoOutput,
    cartridge::PRG_RAM_BYTES,
    state_codec::{StateReader, StateSink},
};

pub const CPU_RAM_BYTES: usize = 0x800;
pub const APU_IO_REGISTER_BYTES: usize = 0x18;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemorySnapshot {
    pub cpu_ram: Box<[u8; CPU_RAM_BYTES]>,
    pub prg_ram: Box<[u8; PRG_RAM_BYTES]>,
    pub chr_ram: Option<Box<[u8; CHR_MEMORY_BYTES]>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusSnapshot {
    pub memory: MemorySnapshot,
    pub apu_io_registers: [u8; APU_IO_REGISTER_BYTES],
    pub apu: ApuSnapshot,
    pub controllers: [ControllerSnapshot; 2],
    pub open_bus: u8,
    pub oam_dma_request: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BusAccessKind {
    Read,
    Write,
    DummyRead,
    DummyWrite,
    DmaRead,
    DmaWrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BusAccess {
    pub kind: BusAccessKind,
    pub address: u16,
    pub value: u8,
}

/// The CPU's complete external dependency. A clock performs at most one call
/// to this interface, making every real and dummy hardware access observable.
pub trait CpuBus {
    fn read(&mut self, address: u16) -> u8;
    fn write(&mut self, address: u16, value: u8);
}

/// CPU-visible NES address space for a mapper-0 machine.
///
/// The bus owns the PPU because CPU register accesses and PPU cartridge reads
/// must observe one mutable machine state. The machine scheduler remains
/// responsible for advancing it three dots per CPU-rate slot.
#[derive(Clone, Debug)]
pub struct NesBus {
    ram: Box<[u8; CPU_RAM_BYTES]>,
    ppu: Ppu,
    apu: Apu,
    apu_io_registers: [u8; APU_IO_REGISTER_BYTES],
    controllers: [ControllerPort; 2],
    cartridge: Cartridge,
    open_bus: u8,
    oam_dma_request: Option<u8>,
}

impl NesBus {
    pub fn new(
        cartridge: Cartridge,
        ram_init: RamInit,
        video_output: VideoOutput,
        audio_output: AudioOutput,
        dma_alignment: OamDmaAlignment,
    ) -> Self {
        let ram_value = match ram_init {
            RamInit::Zero => 0,
            RamInit::Pattern(value) => value,
        };
        let mut apu = Apu::new(audio_output);
        apu.set_dma_alignment(dma_alignment);
        Self {
            ram: Box::new([ram_value; 0x800]),
            ppu: Ppu::new(video_output),
            apu,
            apu_io_registers: [0; 0x18],
            controllers: [ControllerPort::default(); 2],
            cartridge,
            open_bus: 0,
            oam_dma_request: None,
        }
    }

    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    pub fn cartridge_mut(&mut self) -> &mut Cartridge {
        &mut self.cartridge
    }

    pub fn ram(&self) -> &[u8; CPU_RAM_BYTES] {
        &self.ram
    }

    pub fn ram_mut(&mut self) -> &mut [u8; CPU_RAM_BYTES] {
        &mut self.ram
    }

    pub fn memory_snapshot(&self) -> MemorySnapshot {
        MemorySnapshot {
            cpu_ram: self.ram.clone(),
            prg_ram: Box::new(*self.cartridge.prg_ram()),
            chr_ram: self.cartridge.chr_ram().map(|data| Box::new(*data)),
        }
    }

    pub fn snapshot(&self) -> BusSnapshot {
        BusSnapshot {
            memory: self.memory_snapshot(),
            apu_io_registers: self.apu_io_registers,
            apu: self.apu.snapshot(),
            controllers: [
                self.controllers[0].snapshot(),
                self.controllers[1].snapshot(),
            ],
            open_bus: self.open_bus,
            oam_dma_request: self.oam_dma_request,
        }
    }

    pub fn open_bus(&self) -> u8 {
        self.open_bus
    }

    pub fn set_controller_buttons(&mut self, port: usize, buttons: ControllerButtons) {
        if let Some(controller) = self.controllers.get_mut(port) {
            controller.set_buttons(buttons);
        }
    }

    pub fn controller(&self, port: usize) -> Option<&ControllerPort> {
        self.controllers.get(port)
    }

    pub fn ppu(&self) -> &Ppu {
        &self.ppu
    }

    pub fn ppu_mut(&mut self) -> &mut Ppu {
        &mut self.ppu
    }

    pub fn apu(&self) -> &Apu {
        &self.apu
    }

    pub fn apu_mut(&mut self) -> &mut Apu {
        &mut self.apu
    }

    pub(crate) fn write_oam_dma(&mut self, value: u8) {
        self.ppu.write_oam_dma(value);
        self.open_bus = value;
    }

    pub(crate) fn clock_ppu(&mut self) -> PpuCycle {
        self.ppu.clock(&self.cartridge)
    }

    pub(crate) fn clock_apu(&mut self) {
        self.apu.clock();
    }

    pub(crate) fn take_dmc_dma_request(&mut self) -> Option<DmcDmaRequest> {
        self.apu.take_dmc_dma_request()
    }

    pub(crate) fn dmc_dma_requested(&self) -> bool {
        self.apu.dmc_dma_request().is_some()
    }

    pub(crate) fn complete_dmc_dma(&mut self, value: u8) {
        self.apu.complete_dmc_dma(value);
    }

    pub(crate) fn take_ppu_nmi(&mut self) -> bool {
        self.ppu.take_nmi_pending()
    }

    pub fn ppu_memory_peek(&self, address: u16) -> u8 {
        self.ppu.memory_peek(&self.cartridge, address)
    }

    pub fn ppu_memory_write(&mut self, address: u16, value: u8) {
        self.ppu.memory_write(&mut self.cartridge, address, value);
    }

    pub fn take_oam_dma_request(&mut self) -> Option<u8> {
        self.oam_dma_request.take()
    }

    pub(crate) fn oam_dma_requested(&self) -> bool {
        self.oam_dma_request.is_some()
    }

    pub(crate) fn write_state<S: StateSink>(&self, sink: &mut S, include_framebuffer: bool) {
        sink.write(&self.ram[..]);
        self.ppu.write_state(sink, include_framebuffer);
        self.apu.write_state(sink, include_framebuffer);
        sink.write(&self.apu_io_registers);
        for controller in &self.controllers {
            controller.write_state(sink);
        }
        self.cartridge.write_state(sink);
        sink.write_u8(self.open_bus);
        sink.write_optional_u8(self.oam_dma_request);
    }

    pub(crate) fn read_state(
        &mut self,
        reader: &mut StateReader<'_>,
        include_framebuffer: bool,
    ) -> Result<(), StateError> {
        self.ram.copy_from_slice(reader.read_bytes(CPU_RAM_BYTES)?);
        self.ppu.read_state(reader, include_framebuffer)?;
        self.apu.read_state(reader, include_framebuffer)?;
        self.apu_io_registers
            .copy_from_slice(reader.read_bytes(APU_IO_REGISTER_BYTES)?);
        for controller in &mut self.controllers {
            controller.read_state(reader)?;
        }
        self.cartridge.read_state(reader)?;
        self.open_bus = reader.read_u8()?;
        self.oam_dma_request = reader.read_optional_u8()?;
        Ok(())
    }

    pub(crate) fn copy_emulated_state_from(&mut self, source: &Self) {
        debug_assert_eq!(self.cartridge.image(), source.cartridge.image());
        self.ram.copy_from_slice(&source.ram[..]);
        self.ppu.copy_emulated_state_from(&source.ppu);
        self.apu.copy_emulated_state_from(&source.apu);
        self.apu_io_registers = source.apu_io_registers;
        self.controllers = source.controllers;
        self.cartridge.copy_mutable_state_from(&source.cartridge);
        self.open_bus = source.open_bus;
        self.oam_dma_request = source.oam_dma_request;
    }

    /// Side-effect-free debug view of the CPU address space.
    pub fn peek(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x1fff => self.ram[usize::from(address & 0x07ff)],
            0x2000..=0x3fff => self
                .ppu
                .peek_cpu_register(usize::from(address & 7), &self.cartridge),
            0x4000..=0x4014 => self.open_bus,
            0x4015 => (self.open_bus & 0x20) | self.apu.peek_status(),
            0x4016 => (self.open_bus & 0xe0) | self.controllers[0].peek_serial(),
            0x4017 => (self.open_bus & 0xe0) | self.controllers[1].peek_serial(),
            0x4018..=0x5fff => self.open_bus,
            0x6000..=0xffff => self.cartridge.cpu_read(address).unwrap_or(self.open_bus),
        }
    }
}

impl CpuBus for NesBus {
    fn read(&mut self, address: u16) -> u8 {
        // $4015 is unusual: only bit 5 is supplied by open bus, and the read
        // clears the frame IRQ without replacing the bus latch.
        if address == 0x4015 {
            return (self.open_bus & 0x20) | self.apu.read_status();
        }
        let value = match address {
            0x0000..=0x1fff => self.ram[usize::from(address & 0x07ff)],
            0x2000..=0x3fff => self
                .ppu
                .cpu_read_register(usize::from(address & 7), &self.cartridge),
            0x4000..=0x4014 => self.open_bus,
            0x4015 => unreachable!("$4015 returns before the general bus read"),
            0x4016 => (self.open_bus & 0xe0) | self.controllers[0].read_serial(),
            0x4017 => (self.open_bus & 0xe0) | self.controllers[1].read_serial(),
            0x4018..=0x5fff => self.open_bus,
            0x6000..=0xffff => self.cartridge.cpu_read(address).unwrap_or(self.open_bus),
        };
        self.open_bus = value;
        value
    }

    fn write(&mut self, address: u16, value: u8) {
        self.open_bus = value;
        match address {
            0x0000..=0x1fff => self.ram[usize::from(address & 0x07ff)] = value,
            0x2000..=0x3fff => {
                self.ppu
                    .cpu_write_register(usize::from(address & 7), value, &mut self.cartridge)
            }
            0x4000..=0x4013 | 0x4015 | 0x4017 => {
                self.apu_io_registers[usize::from(address - 0x4000)] = value;
                self.apu.write_register(address, value);
            }
            0x4014 => {
                self.apu_io_registers[0x14] = value;
                self.oam_dma_request = Some(value);
            }
            0x4016 => {
                self.apu_io_registers[0x16] = value;
                self.controllers[0].write_strobe(value);
                self.controllers[1].write_strobe(value);
            }
            0x4018..=0x5fff => {}
            0x6000..=0xffff => {
                self.cartridge.cpu_write(address, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CartridgeImage, test_rom::NromBuilder};

    fn bus() -> NesBus {
        let image = CartridgeImage::parse(&NromBuilder::new_16k().build()).unwrap();
        NesBus::new(
            Cartridge::new(image),
            RamInit::Zero,
            VideoOutput::Enabled,
            AudioOutput::Enabled,
            OamDmaAlignment::default(),
        )
    }

    #[test]
    fn mirrors_internal_ram_and_ppu_registers() {
        let mut bus = bus();
        bus.write(0x0003, 0x45);
        assert_eq!(bus.read(0x0803), 0x45);
        assert_eq!(bus.read(0x1803), 0x45);

        bus.write(0x2003, 0xfe);
        bus.write(0x2004, 0x80);
        bus.write(0x2003, 0xfe);
        assert_eq!(bus.read(0x3ffc), 0x80);
    }

    #[test]
    fn controller_reads_keep_open_bus_high_bits() {
        let mut bus = bus();
        bus.set_controller_buttons(0, ControllerButtons::A);
        bus.write(0x0000, 0xa0);
        bus.write(0x4016, 1);
        bus.write(0x4016, 0);
        // The strobe write itself puts zero on the open bus.
        assert_eq!(bus.read(0x4016), 1);
        bus.write(0x0000, 0xa0);
        assert_eq!(bus.read(0x4016), 0xa0);
    }

    #[test]
    fn shared_strobe_latches_and_shifts_both_controller_ports_independently() {
        let mut bus = bus();
        bus.set_controller_buttons(
            0,
            ControllerButtons::A | ControllerButtons::START | ControllerButtons::LEFT,
        );
        bus.set_controller_buttons(
            1,
            ControllerButtons::B | ControllerButtons::SELECT | ControllerButtons::RIGHT,
        );
        bus.write(0x4016, 1);
        bus.write(0x4016, 0);

        let first = std::array::from_fn::<_, 10, _>(|_| bus.read(0x4016) & 1);
        let second = std::array::from_fn::<_, 10, _>(|_| bus.read(0x4017) & 1);
        assert_eq!(first, [1, 0, 0, 1, 0, 0, 1, 0, 1, 1]);
        assert_eq!(second, [0, 1, 1, 0, 0, 0, 0, 1, 1, 1]);
    }

    #[test]
    fn exposes_oam_dma_request_without_performing_an_instant_copy() {
        let mut bus = bus();
        bus.write(0x4014, 0x23);
        assert_eq!(bus.take_oam_dma_request(), Some(0x23));
        assert_eq!(bus.take_oam_dma_request(), None);
    }

    #[test]
    fn apu_status_has_open_bus_bit_and_only_clears_the_frame_irq() {
        let mut bus = bus();
        bus.write(0x4015, 0x01);
        bus.write(0x4003, 0x08);
        for _ in 0..29_828 {
            bus.clock_apu();
        }
        bus.write(0x0000, 0xa0);
        let status = bus.read(0x4015);
        assert_eq!(status & 0x61, 0x61);
        assert_eq!(bus.open_bus(), 0xa0);
        assert_eq!(bus.peek(0x4015) & 0x40, 0);

        bus.write(0x4000, 0x5a);
        bus.write(0x0000, 0x33);
        assert_eq!(bus.read(0x4000), 0x33);
        assert_eq!(bus.snapshot().apu_io_registers[0], 0x5a);
    }

    #[test]
    fn oam_register_preserves_addressed_write_behavior() {
        let mut bus = bus();
        bus.write(0x2003, 0xfe);
        bus.write(0x2004, 0x12);
        bus.write(0x2004, 0x34);
        assert_eq!(bus.ppu().oam()[0xfe], 0x12);
        assert_eq!(bus.ppu().oam()[0xff], 0x34);
        assert_eq!(bus.read(0x2004), 0);
    }
}
