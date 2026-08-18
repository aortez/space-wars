use crate::{Cartridge, ControllerButtons, ControllerPort, Ppu, PpuCycle, RamInit, VideoOutput};

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
    ram: Box<[u8; 0x800]>,
    ppu: Ppu,
    apu_io_registers: [u8; 0x18],
    controllers: [ControllerPort; 2],
    cartridge: Cartridge,
    open_bus: u8,
    oam_dma_request: Option<u8>,
}

impl NesBus {
    pub fn new(cartridge: Cartridge, ram_init: RamInit, video_output: VideoOutput) -> Self {
        let ram_value = match ram_init {
            RamInit::Zero => 0,
            RamInit::Pattern(value) => value,
        };
        Self {
            ram: Box::new([ram_value; 0x800]),
            ppu: Ppu::new(video_output),
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

    pub fn ram(&self) -> &[u8; 0x800] {
        &self.ram
    }

    pub fn ram_mut(&mut self) -> &mut [u8; 0x800] {
        &mut self.ram
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

    pub(crate) fn write_oam_dma(&mut self, value: u8) {
        self.ppu.write_oam_dma(value);
        self.open_bus = value;
    }

    pub(crate) fn clock_ppu(&mut self) -> PpuCycle {
        self.ppu.clock(&self.cartridge)
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

    /// Side-effect-free debug view of the CPU address space.
    pub fn peek(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x1fff => self.ram[usize::from(address & 0x07ff)],
            0x2000..=0x3fff => self
                .ppu
                .peek_cpu_register(usize::from(address & 7), &self.cartridge),
            0x4000..=0x4015 => self.apu_io_registers[usize::from(address - 0x4000)],
            0x4016 => (self.open_bus & 0xe0) | self.controllers[0].peek_serial(),
            0x4017 => (self.open_bus & 0xe0) | self.controllers[1].peek_serial(),
            0x4018..=0x5fff => self.open_bus,
            0x6000..=0xffff => self.cartridge.cpu_read(address).unwrap_or(self.open_bus),
        }
    }
}

impl CpuBus for NesBus {
    fn read(&mut self, address: u16) -> u8 {
        let value = match address {
            0x0000..=0x1fff => self.ram[usize::from(address & 0x07ff)],
            0x2000..=0x3fff => self
                .ppu
                .cpu_read_register(usize::from(address & 7), &self.cartridge),
            0x4000..=0x4015 => self.apu_io_registers[usize::from(address - 0x4000)],
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
        NesBus::new(Cartridge::new(image), RamInit::Zero, VideoOutput::Enabled)
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
    fn exposes_oam_dma_request_without_performing_an_instant_copy() {
        let mut bus = bus();
        bus.write(0x4014, 0x23);
        assert_eq!(bus.take_oam_dma_request(), Some(0x23));
        assert_eq!(bus.take_oam_dma_request(), None);
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
