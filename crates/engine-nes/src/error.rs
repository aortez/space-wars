use std::error::Error;
use std::fmt;

use crate::cartridge::CartridgeIdentity;

/// A cartridge could not be represented by the currently supported iNES
/// subset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CartridgeError {
    HeaderTooShort {
        actual: usize,
    },
    InvalidMagic([u8; 4]),
    UnsupportedNes2,
    UnsupportedConsoleType(u8),
    UnsupportedPalTiming,
    UnsupportedMapper(u16),
    UnsupportedFourScreenMirroring(u16),
    UnsupportedPrgRomBanks {
        mapper: u16,
        banks: u8,
    },
    UnsupportedPrgRamBanks {
        mapper: u16,
        banks: u8,
    },
    UnsupportedChrRomBanks {
        mapper: u16,
        banks: u8,
    },
    Truncated {
        expected_at_least: usize,
        actual: usize,
    },
}

impl fmt::Display for CartridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HeaderTooShort { actual } => {
                write!(formatter, "iNES header needs 16 bytes, found {actual}")
            }
            Self::InvalidMagic(actual) => write!(
                formatter,
                "invalid iNES magic {:02x?}; expected [4e, 45, 53, 1a]",
                actual
            ),
            Self::UnsupportedNes2 => formatter.write_str("NES 2.0 cartridges are not supported"),
            Self::UnsupportedConsoleType(kind) => {
                write!(formatter, "unsupported iNES console type {kind}")
            }
            Self::UnsupportedPalTiming => {
                formatter.write_str("PAL iNES cartridges are not supported")
            }
            Self::UnsupportedMapper(mapper) => {
                write!(
                    formatter,
                    "unsupported mapper {mapper}; supported mappers are NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4), and AxROM (7)"
                )
            }
            Self::UnsupportedFourScreenMirroring(1) => formatter
                .write_str("MMC1 (mapper 1) uses mapper-controlled mirroring, not four-screen mirroring"),
            Self::UnsupportedFourScreenMirroring(2) => formatter
                .write_str("UxROM (mapper 2) supports horizontal or vertical mirroring, not four-screen mirroring"),
            Self::UnsupportedFourScreenMirroring(3) => formatter
                .write_str("CNROM (mapper 3) supports horizontal or vertical mirroring, not four-screen mirroring"),
            Self::UnsupportedFourScreenMirroring(7) => formatter
                .write_str("AxROM (mapper 7) uses mapper-controlled one-screen mirroring, not four-screen mirroring"),
            Self::UnsupportedFourScreenMirroring(mapper) => {
                write!(formatter, "mapper {mapper} does not support four-screen mirroring")
            }
            Self::UnsupportedPrgRomBanks { mapper: 0, banks } => write!(
                formatter,
                "NROM (mapper 0) requires one or two 16 KiB PRG ROM banks, found {banks}"
            ),
            Self::UnsupportedPrgRomBanks { mapper: 1, banks } => write!(
                formatter,
                "MMC1 (mapper 1) requires 2-16 power-of-two 16 KiB PRG ROM banks in the supported 256 KiB SxROM subset, found {banks}"
            ),
            Self::UnsupportedPrgRomBanks { mapper: 2, banks } => write!(
                formatter,
                "UxROM (mapper 2) requires 2-128 power-of-two 16 KiB PRG ROM banks, found {banks}"
            ),
            Self::UnsupportedPrgRomBanks { mapper: 3, banks } => write!(
                formatter,
                "CNROM (mapper 3) requires one or two 16 KiB PRG ROM banks, found {banks}"
            ),
            Self::UnsupportedPrgRomBanks { mapper: 4, banks } => write!(
                formatter,
                "MMC3 (mapper 4) requires 1-32 power-of-two 16 KiB PRG ROM banks, found {banks}"
            ),
            Self::UnsupportedPrgRomBanks { mapper: 7, banks } => write!(
                formatter,
                "AxROM (mapper 7) requires 2-16 power-of-two 16 KiB PRG ROM banks in the supported 256 KiB subset, found {banks}"
            ),
            Self::UnsupportedPrgRomBanks { mapper, banks } => write!(
                formatter,
                "mapper {mapper} does not support {banks} 16 KiB PRG ROM banks"
            ),
            Self::UnsupportedPrgRamBanks { mapper: 1, banks } => write!(
                formatter,
                "MMC1 (mapper 1) supports at most one 8 KiB PRG RAM bank, found {banks}"
            ),
            Self::UnsupportedPrgRamBanks { mapper, banks } => write!(
                formatter,
                "mapper {mapper} does not support {banks} 8 KiB PRG RAM banks"
            ),
            Self::UnsupportedChrRomBanks { mapper: 0, banks } => write!(
                formatter,
                "NROM (mapper 0) supports zero or one 8 KiB CHR ROM bank, found {banks}"
            ),
            Self::UnsupportedChrRomBanks { mapper: 1, banks } => write!(
                formatter,
                "MMC1 (mapper 1) supports 8 KiB CHR RAM or 1-16 power-of-two 8 KiB CHR ROM banks, found {banks}"
            ),
            Self::UnsupportedChrRomBanks { mapper: 2, banks } => write!(
                formatter,
                "UxROM (mapper 2) requires 8 KiB CHR RAM and no CHR ROM banks, found {banks}"
            ),
            Self::UnsupportedChrRomBanks { mapper: 3, banks } => write!(
                formatter,
                "CNROM (mapper 3) requires two or four 8 KiB CHR ROM banks, found {banks}"
            ),
            Self::UnsupportedChrRomBanks { mapper: 4, banks } => write!(
                formatter,
                "MMC3 (mapper 4) supports 8 KiB CHR RAM or 1-32 power-of-two 8 KiB CHR ROM banks, found {banks}"
            ),
            Self::UnsupportedChrRomBanks { mapper: 7, banks } => write!(
                formatter,
                "AxROM (mapper 7) requires 8 KiB CHR RAM and no CHR ROM banks, found {banks}"
            ),
            Self::UnsupportedChrRomBanks { mapper, banks } => write!(
                formatter,
                "mapper {mapper} does not support {banks} 8 KiB CHR ROM banks"
            ),
            Self::Truncated {
                expected_at_least,
                actual,
            } => write!(
                formatter,
                "truncated iNES image: expected at least {expected_at_least} bytes, found {actual}"
            ),
        }
    }
}

impl Error for CartridgeError {}

/// The CPU encountered an instruction which is outside the supported official
/// RP2A03 instruction set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CpuError {
    UnsupportedOpcode { pc: u16, opcode: u8 },
}

impl fmt::Display for CpuError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOpcode { pc, opcode } => {
                write!(formatter, "unsupported opcode ${opcode:02X} at ${pc:04X}")
            }
        }
    }
}

impl Error for CpuError {}

#[derive(Debug)]
pub enum MachineError {
    Cartridge(CartridgeError),
    Cpu(CpuError),
}

impl fmt::Display for MachineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cartridge(error) => error.fmt(formatter),
            Self::Cpu(error) => error.fmt(formatter),
        }
    }
}

impl Error for MachineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Cartridge(error) => Some(error),
            Self::Cpu(error) => Some(error),
        }
    }
}

impl From<CartridgeError> for MachineError {
    fn from(value: CartridgeError) -> Self {
        Self::Cartridge(value)
    }
}

impl From<CpuError> for MachineError {
    fn from(value: CpuError) -> Self {
        Self::Cpu(value)
    }
}

/// A checkpoint or durable savestate is invalid for the target machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateError {
    InvalidMagic([u8; 8]),
    UnsupportedVersion {
        found: u16,
    },
    UnsupportedFlags {
        found: u16,
    },
    Truncated {
        needed: usize,
        actual: usize,
    },
    TooLarge {
        declared: usize,
        maximum: usize,
    },
    LengthMismatch {
        declared: usize,
        actual: usize,
    },
    ChecksumMismatch {
        expected: u64,
        actual: u64,
    },
    CartridgeMismatch {
        expected: CartridgeIdentity,
        actual: CartridgeIdentity,
    },
    InvalidPayload(&'static str),
    TrailingPayload {
        remaining: usize,
    },
}

impl fmt::Display for StateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic(actual) => write!(
                formatter,
                "invalid NES savestate magic {:02x?}; expected SWNESST\\0",
                actual
            ),
            Self::UnsupportedVersion { found } => {
                write!(formatter, "unsupported NES savestate version {found}")
            }
            Self::UnsupportedFlags { found } => {
                write!(formatter, "unsupported NES savestate flags ${found:04x}")
            }
            Self::Truncated { needed, actual } => write!(
                formatter,
                "truncated NES savestate: needed {needed} bytes, found {actual}"
            ),
            Self::TooLarge { declared, maximum } => write!(
                formatter,
                "NES savestate declares {declared} payload bytes; maximum is {maximum}"
            ),
            Self::LengthMismatch { declared, actual } => write!(
                formatter,
                "NES savestate declares {declared} payload bytes, found {actual}"
            ),
            Self::ChecksumMismatch { expected, actual } => write!(
                formatter,
                "NES savestate checksum mismatch: expected {expected:016x}, found {actual:016x}"
            ),
            Self::CartridgeMismatch { expected, actual } => write!(
                formatter,
                "NES savestate cartridge mismatch: expected {expected:?}, found {actual:?}"
            ),
            Self::InvalidPayload(reason) => {
                write!(formatter, "invalid NES savestate payload: {reason}")
            }
            Self::TrailingPayload { remaining } => write!(
                formatter,
                "NES savestate payload has {remaining} unexpected trailing bytes"
            ),
        }
    }
}

impl Error for StateError {}
