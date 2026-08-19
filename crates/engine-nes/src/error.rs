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
    UnsupportedMapper(u16),
    UnsupportedPrgRomBanks(u8),
    UnsupportedChrRomBanks(u8),
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
            Self::UnsupportedMapper(mapper) => {
                write!(
                    formatter,
                    "unsupported mapper {mapper}; only NROM (0) is supported"
                )
            }
            Self::UnsupportedPrgRomBanks(banks) => write!(
                formatter,
                "NROM requires one or two 16 KiB PRG ROM banks, found {banks}"
            ),
            Self::UnsupportedChrRomBanks(banks) => write!(
                formatter,
                "NROM supports zero or one 8 KiB CHR ROM bank, found {banks}"
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
