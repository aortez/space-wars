/// NTSC color-clock numerator used by the RP2A03/2C02 timing ratios.
pub const NTSC_MASTER_CLOCK_NUMERATOR_HZ: u64 = 236_250_000;
/// The 2C02 PPU clock is the NTSC color source divided by 44.
pub const NTSC_PPU_CLOCK_DENOMINATOR: u64 = 44;

/// Emulated television timing standard.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Region {
    #[default]
    Ntsc,
}

/// Deterministic policy used to initialize the NES's 2 KiB internal RAM.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RamInit {
    #[default]
    Zero,
    Pattern(u8),
}

/// Whether a host wants reusable video output when the PPU is implemented.
/// Disabling output never disables emulated PPU work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VideoOutput {
    #[default]
    Enabled,
    Disabled,
}

/// Whether a host wants reusable mixed samples from the APU.
/// Disabling output never disables emulated APU work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AudioOutput {
    #[default]
    Enabled,
    Disabled,
}

/// Deterministic selection of the otherwise power-on-dependent CPU/APU DMA
/// cadence. The names describe the scheduler slot on which a standalone OAM
/// DMA takes its shorter 513-cycle path.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OamDmaAlignment {
    #[default]
    ShortOnEvenSlot,
    ShortOnOddSlot,
}

impl OamDmaAlignment {
    pub(crate) const fn needs_alignment(self, start_slot: u64) -> bool {
        match self {
            Self::ShortOnEvenSlot => start_slot & 1 != 0,
            Self::ShortOnOddSlot => start_slot & 1 == 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MachineConfig {
    pub region: Region,
    pub ram_init: RamInit,
    pub video: VideoOutput,
    pub audio: AudioOutput,
    pub oam_dma_alignment: OamDmaAlignment,
}
