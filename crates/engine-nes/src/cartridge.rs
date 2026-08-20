use std::sync::Arc;

use crate::state_codec::fnv1a64;
use crate::{CartridgeError, StateError, state_codec::StateReader, state_codec::StateSink};

const INES_HEADER_LEN: usize = 16;
const TRAINER_LEN: usize = 512;
const PRG_ROM_BANK_LEN: usize = 16 * 1024;
const CHR_ROM_HALF_BANK_LEN: usize = 4 * 1024;
pub const CHR_MEMORY_BYTES: usize = 8 * 1024;
pub const PRG_RAM_BYTES: usize = 8 * 1024;

const CHR_ROM_BANK_LEN: usize = CHR_MEMORY_BYTES;
const PRG_RAM_LEN: usize = PRG_RAM_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CartridgeIdentity {
    pub byte_len: u32,
    pub fnv1a64: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mirroring {
    OneScreenLower,
    OneScreenUpper,
    Horizontal,
    Vertical,
    FourScreen,
}

impl Mirroring {
    /// Maps `$2000-$2fff` (and the `$3000-$3eff` mirror) into physical
    /// nametable storage. One-screen and horizontal/vertical layouts use 2
    /// KiB; four-screen layouts use 4 KiB supplied by the cartridge.
    pub const fn map_nametable_address(self, address: u16) -> usize {
        let offset = (address.wrapping_sub(0x2000) as usize) & 0x0fff;
        let table = offset / 0x0400;
        let within_table = offset & 0x03ff;
        let physical_table = match self {
            Self::OneScreenLower => 0,
            Self::OneScreenUpper => 1,
            Self::Horizontal => table / 2,
            Self::Vertical => table & 1,
            Self::FourScreen => table,
        };
        physical_table * 0x0400 + within_table
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CartridgeMetadata {
    pub mapper: u16,
    pub mirroring: Mirroring,
    pub battery_backed: bool,
    pub has_trainer: bool,
    pub prg_rom_len: usize,
    pub chr_rom_len: usize,
    pub chr_is_ram: bool,
}

/// Parsed immutable cartridge data. Clones share ROM storage so several
/// machines can execute the same image without copying it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CartridgeImage {
    identity: CartridgeIdentity,
    metadata: CartridgeMetadata,
    prg_rom: Arc<[u8]>,
    chr_rom: Arc<[u8]>,
    trainer: Option<Arc<[u8]>>,
}

impl CartridgeImage {
    pub fn parse(bytes: &[u8]) -> Result<Self, CartridgeError> {
        if bytes.len() < INES_HEADER_LEN {
            return Err(CartridgeError::HeaderTooShort {
                actual: bytes.len(),
            });
        }

        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if magic != *b"NES\x1a" {
            return Err(CartridgeError::InvalidMagic(magic));
        }

        let prg_banks = bytes[4];
        let chr_banks = bytes[5];
        let flags6 = bytes[6];
        let flags7 = bytes[7];

        if flags7 & 0x0c == 0x08 {
            return Err(CartridgeError::UnsupportedNes2);
        }
        let console_type = flags7 & 0x03;
        if console_type != 0 {
            return Err(CartridgeError::UnsupportedConsoleType(console_type));
        }

        let mapper = u16::from(flags6 >> 4) | u16::from(flags7 & 0xf0);
        match mapper {
            0 => {
                if !matches!(prg_banks, 1 | 2) {
                    return Err(CartridgeError::UnsupportedPrgRomBanks {
                        mapper,
                        banks: prg_banks,
                    });
                }
                if chr_banks > 1 {
                    return Err(CartridgeError::UnsupportedChrRomBanks {
                        mapper,
                        banks: chr_banks,
                    });
                }
            }
            1 => {
                if flags6 & 0x08 != 0 {
                    return Err(CartridgeError::UnsupportedFourScreenMirroring(mapper));
                }
                if !(2..=16).contains(&prg_banks) || !prg_banks.is_power_of_two() {
                    return Err(CartridgeError::UnsupportedPrgRomBanks {
                        mapper,
                        banks: prg_banks,
                    });
                }
                if chr_banks != 0
                    && (!(1..=16).contains(&chr_banks) || !chr_banks.is_power_of_two())
                {
                    return Err(CartridgeError::UnsupportedChrRomBanks {
                        mapper,
                        banks: chr_banks,
                    });
                }
            }
            2 => {
                if flags6 & 0x08 != 0 {
                    return Err(CartridgeError::UnsupportedFourScreenMirroring(mapper));
                }
                if !(2..=128).contains(&prg_banks) || !prg_banks.is_power_of_two() {
                    return Err(CartridgeError::UnsupportedPrgRomBanks {
                        mapper,
                        banks: prg_banks,
                    });
                }
                if chr_banks != 0 {
                    return Err(CartridgeError::UnsupportedChrRomBanks {
                        mapper,
                        banks: chr_banks,
                    });
                }
            }
            3 => {
                if flags6 & 0x08 != 0 {
                    return Err(CartridgeError::UnsupportedFourScreenMirroring(mapper));
                }
                if !matches!(prg_banks, 1 | 2) {
                    return Err(CartridgeError::UnsupportedPrgRomBanks {
                        mapper,
                        banks: prg_banks,
                    });
                }
                if !matches!(chr_banks, 2 | 4) {
                    return Err(CartridgeError::UnsupportedChrRomBanks {
                        mapper,
                        banks: chr_banks,
                    });
                }
            }
            _ => return Err(CartridgeError::UnsupportedMapper(mapper)),
        }

        let has_trainer = flags6 & 0x04 != 0;
        let trainer_len = if has_trainer { TRAINER_LEN } else { 0 };
        let prg_rom_len = usize::from(prg_banks) * PRG_ROM_BANK_LEN;
        let chr_rom_len = usize::from(chr_banks) * CHR_ROM_BANK_LEN;
        let expected_at_least = INES_HEADER_LEN
            .checked_add(trainer_len)
            .and_then(|offset| offset.checked_add(prg_rom_len))
            .and_then(|offset| offset.checked_add(chr_rom_len))
            .expect("iNES bank counts are bounded u8 values");
        if bytes.len() < expected_at_least {
            return Err(CartridgeError::Truncated {
                expected_at_least,
                actual: bytes.len(),
            });
        }

        let mirroring = if flags6 & 0x08 != 0 {
            Mirroring::FourScreen
        } else if flags6 & 0x01 != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };

        let trainer_start = INES_HEADER_LEN;
        let prg_start = trainer_start + trainer_len;
        let chr_start = prg_start + prg_rom_len;
        let trainer = has_trainer.then(|| Arc::from(&bytes[trainer_start..prg_start]));

        Ok(Self {
            identity: CartridgeIdentity {
                byte_len: expected_at_least as u32,
                fnv1a64: fnv1a64(&bytes[..expected_at_least]),
            },
            metadata: CartridgeMetadata {
                mapper,
                mirroring,
                battery_backed: flags6 & 0x02 != 0,
                has_trainer,
                prg_rom_len,
                chr_rom_len,
                chr_is_ram: chr_banks == 0,
            },
            prg_rom: Arc::from(&bytes[prg_start..chr_start]),
            chr_rom: Arc::from(&bytes[chr_start..chr_start + chr_rom_len]),
            trainer,
        })
    }

    pub fn metadata(&self) -> CartridgeMetadata {
        self.metadata
    }

    pub fn identity(&self) -> CartridgeIdentity {
        self.identity
    }

    pub fn prg_rom(&self) -> &[u8] {
        &self.prg_rom
    }

    pub fn chr_rom(&self) -> &[u8] {
        &self.chr_rom
    }

    pub fn trainer(&self) -> Option<&[u8]> {
        self.trainer.as_deref()
    }
}

#[derive(Clone, Debug)]
enum ChrMemory {
    Rom(Arc<[u8]>),
    Ram(Box<[u8; CHR_ROM_BANK_LEN]>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MapperSnapshot {
    Nrom,
    Mmc1 {
        shift_register: u8,
        control: u8,
        chr_bank0: u8,
        chr_bank1: u8,
        prg_bank: u8,
        previous_cpu_access_was_write: bool,
    },
    Uxrom {
        selected_prg_bank: u8,
    },
    Cnrom {
        selected_chr_bank: u8,
    },
}

#[derive(Clone, Debug)]
struct Mmc1State {
    shift_register: u8,
    control: u8,
    chr_bank0: u8,
    chr_bank1: u8,
    prg_bank: u8,
    previous_cpu_access_was_write: bool,
}

impl Mmc1State {
    const fn new() -> Self {
        Self {
            shift_register: 0x10,
            control: 0x0c,
            chr_bank0: 0,
            chr_bank1: 0,
            prg_bank: 0,
            previous_cpu_access_was_write: false,
        }
    }

    fn write(&mut self, address: u16, value: u8, consecutive_cpu_write: bool) {
        if value & 0x80 != 0 {
            self.shift_register = 0x10;
            self.control |= 0x0c;
            return;
        }
        if consecutive_cpu_write {
            return;
        }

        let complete = self.shift_register & 1 != 0;
        self.shift_register = (self.shift_register >> 1) | ((value & 1) << 4);
        if !complete {
            return;
        }

        let data = self.shift_register & 0x1f;
        self.shift_register = 0x10;
        match address {
            0x8000..=0x9fff => self.control = data,
            0xa000..=0xbfff => self.chr_bank0 = data,
            0xc000..=0xdfff => self.chr_bank1 = data,
            0xe000..=0xffff => self.prg_bank = data,
            _ => unreachable!("MMC1 writes are in the cartridge ROM window"),
        }
    }
}

#[derive(Clone, Debug)]
enum MapperState {
    Nrom,
    Mmc1(Mmc1State),
    Uxrom { selected_prg_bank: u8 },
    Cnrom { selected_chr_bank: u8 },
}

impl MapperState {
    fn new(mapper: u16) -> Self {
        match mapper {
            0 => Self::Nrom,
            1 => Self::Mmc1(Mmc1State::new()),
            2 => Self::Uxrom {
                selected_prg_bank: 0,
            },
            3 => Self::Cnrom {
                selected_chr_bank: 0,
            },
            _ => unreachable!("cartridge images reject unsupported mappers"),
        }
    }

    const fn snapshot(&self) -> MapperSnapshot {
        match *self {
            Self::Nrom => MapperSnapshot::Nrom,
            Self::Mmc1(ref state) => MapperSnapshot::Mmc1 {
                shift_register: state.shift_register,
                control: state.control,
                chr_bank0: state.chr_bank0,
                chr_bank1: state.chr_bank1,
                prg_bank: state.prg_bank,
                previous_cpu_access_was_write: state.previous_cpu_access_was_write,
            },
            Self::Uxrom { selected_prg_bank } => MapperSnapshot::Uxrom { selected_prg_bank },
            Self::Cnrom { selected_chr_bank } => MapperSnapshot::Cnrom { selected_chr_bank },
        }
    }
}

/// Mutable cartridge and mapper state belonging to one machine.
#[derive(Clone, Debug)]
pub struct Cartridge {
    image: CartridgeImage,
    prg_ram: Box<[u8; PRG_RAM_LEN]>,
    chr: ChrMemory,
    mapper: MapperState,
}

impl Cartridge {
    pub fn new(image: CartridgeImage) -> Self {
        let mut prg_ram = Box::new([0; PRG_RAM_LEN]);
        if let Some(trainer) = image.trainer() {
            // Trainers are mapped at CPU $7000-$71ff before execution.
            prg_ram[0x1000..0x1200].copy_from_slice(trainer);
        }
        let chr = if image.metadata.chr_is_ram {
            ChrMemory::Ram(Box::new([0; CHR_ROM_BANK_LEN]))
        } else {
            ChrMemory::Rom(Arc::clone(&image.chr_rom))
        };
        let mapper = MapperState::new(image.metadata.mapper);
        Self {
            image,
            prg_ram,
            chr,
            mapper,
        }
    }

    pub fn image(&self) -> &CartridgeImage {
        &self.image
    }

    pub fn cpu_read(&self, address: u16) -> Option<u8> {
        match address {
            0x6000..=0x7fff if self.prg_ram_enabled() => {
                Some(self.prg_ram[usize::from(address - 0x6000)])
            }
            0x6000..=0x7fff => None,
            0x8000..=0xffff => Some(match &self.mapper {
                MapperState::Nrom | MapperState::Cnrom { .. } => {
                    let mut offset = usize::from(address - 0x8000);
                    if self.image.prg_rom.len() == PRG_ROM_BANK_LEN {
                        offset %= PRG_ROM_BANK_LEN;
                    }
                    self.image.prg_rom[offset]
                }
                MapperState::Uxrom { selected_prg_bank } => {
                    let bank = if address < 0xc000 {
                        usize::from(*selected_prg_bank)
                    } else {
                        self.prg_bank_count() - 1
                    };
                    let offset = bank * PRG_ROM_BANK_LEN + usize::from(address & 0x3fff);
                    self.image.prg_rom[offset]
                }
                MapperState::Mmc1(state) => {
                    let bank_count = self.prg_bank_count();
                    let selected = usize::from(state.prg_bank & 0x0f) % bank_count;
                    let bank = match (state.control >> 2) & 3 {
                        0 | 1 => {
                            let first = selected & !1;
                            (first + usize::from(address >= 0xc000)) % bank_count
                        }
                        2 if address < 0xc000 => 0,
                        2 => selected,
                        3 if address < 0xc000 => selected,
                        3 => bank_count - 1,
                        _ => unreachable!("MMC1 PRG mode is two bits"),
                    };
                    let offset = bank * PRG_ROM_BANK_LEN + usize::from(address & 0x3fff);
                    self.image.prg_rom[offset]
                }
            }),
            _ => None,
        }
    }

    pub fn cpu_write(&mut self, address: u16, value: u8) -> bool {
        self.cpu_write_with_timing(address, value, false)
    }

    fn cpu_write_with_timing(
        &mut self,
        address: u16,
        value: u8,
        consecutive_cpu_write: bool,
    ) -> bool {
        match address {
            0x6000..=0x7fff if self.prg_ram_enabled() => {
                self.prg_ram[usize::from(address - 0x6000)] = value;
                true
            }
            0x6000..=0x7fff => false,
            0x8000..=0xffff => {
                let prg_bank_count = self.prg_bank_count() as u8;
                let chr_bank_count = self.chr_bank_count() as u8;
                match &mut self.mapper {
                    MapperState::Nrom => {}
                    MapperState::Mmc1(state) => {
                        state.write(address, value, consecutive_cpu_write);
                    }
                    MapperState::Uxrom { selected_prg_bank } => {
                        *selected_prg_bank = value % prg_bank_count;
                    }
                    MapperState::Cnrom { selected_chr_bank } => {
                        *selected_chr_bank = value % chr_bank_count;
                    }
                }
                true
            }
            _ => false,
        }
    }

    pub fn ppu_read(&self, address: u16) -> Option<u8> {
        let offset = self.chr_offset(address);
        match &self.chr {
            ChrMemory::Rom(data) => data.get(offset).copied(),
            ChrMemory::Ram(data) => data.get(offset).copied(),
        }
    }

    pub fn ppu_write(&mut self, address: u16, value: u8) -> bool {
        let offset = self.chr_offset(address);
        match &mut self.chr {
            ChrMemory::Rom(_) => false,
            ChrMemory::Ram(data) => {
                data[offset] = value;
                true
            }
        }
    }

    pub fn prg_ram(&self) -> &[u8; PRG_RAM_LEN] {
        &self.prg_ram
    }

    pub fn chr_ram(&self) -> Option<&[u8; CHR_ROM_BANK_LEN]> {
        match &self.chr {
            ChrMemory::Rom(_) => None,
            ChrMemory::Ram(data) => Some(data),
        }
    }

    pub const fn mapper_snapshot(&self) -> MapperSnapshot {
        self.mapper.snapshot()
    }

    pub fn mirroring(&self) -> Mirroring {
        match self.mapper {
            MapperState::Mmc1(ref state) => match state.control & 3 {
                0 => Mirroring::OneScreenLower,
                1 => Mirroring::OneScreenUpper,
                2 => Mirroring::Vertical,
                3 => Mirroring::Horizontal,
                _ => unreachable!("MMC1 mirroring mode is two bits"),
            },
            MapperState::Nrom | MapperState::Uxrom { .. } | MapperState::Cnrom { .. } => {
                self.image.metadata.mirroring
            }
        }
    }

    pub(crate) fn note_cpu_read_cycle(&mut self) {
        if let MapperState::Mmc1(state) = &mut self.mapper {
            state.previous_cpu_access_was_write = false;
        }
    }

    pub(crate) fn note_cpu_write_cycle(&mut self) -> bool {
        if let MapperState::Mmc1(state) = &mut self.mapper {
            let previous = state.previous_cpu_access_was_write;
            state.previous_cpu_access_was_write = true;
            previous
        } else {
            false
        }
    }

    pub(crate) fn cpu_write_cycle(
        &mut self,
        address: u16,
        value: u8,
        consecutive_cpu_write: bool,
    ) -> bool {
        self.cpu_write_with_timing(address, value, consecutive_cpu_write)
    }

    fn prg_bank_count(&self) -> usize {
        self.image.prg_rom.len() / PRG_ROM_BANK_LEN
    }

    fn chr_bank_count(&self) -> usize {
        self.image.chr_rom.len() / CHR_ROM_BANK_LEN
    }

    fn chr_half_bank_count(&self) -> usize {
        match &self.chr {
            ChrMemory::Rom(data) => data.len() / CHR_ROM_HALF_BANK_LEN,
            ChrMemory::Ram(data) => data.len() / CHR_ROM_HALF_BANK_LEN,
        }
    }

    fn chr_offset(&self, address: u16) -> usize {
        let address = usize::from(address & 0x1fff);
        match &self.mapper {
            MapperState::Cnrom { selected_chr_bank } => {
                usize::from(*selected_chr_bank) * CHR_ROM_BANK_LEN + address
            }
            MapperState::Mmc1(state) => {
                let half_bank_count = self.chr_half_bank_count();
                if state.control & 0x10 == 0 {
                    let bank = usize::from(state.chr_bank0 & 0x1e) % half_bank_count;
                    bank * CHR_ROM_HALF_BANK_LEN + address
                } else {
                    let register = if address < CHR_ROM_HALF_BANK_LEN {
                        state.chr_bank0
                    } else {
                        state.chr_bank1
                    };
                    let bank = usize::from(register) % half_bank_count;
                    bank * CHR_ROM_HALF_BANK_LEN + (address & (CHR_ROM_HALF_BANK_LEN - 1))
                }
            }
            MapperState::Nrom | MapperState::Uxrom { .. } => address,
        }
    }

    fn prg_ram_enabled(&self) -> bool {
        !matches!(&self.mapper, MapperState::Mmc1(state) if state.prg_bank & 0x10 != 0)
    }

    pub(crate) fn write_state<S: StateSink>(&self, sink: &mut S) {
        sink.write_u16(self.image.metadata.mapper);
        sink.write(&self.prg_ram[..]);
        match &self.chr {
            ChrMemory::Rom(_) => sink.write_u8(0),
            ChrMemory::Ram(data) => {
                sink.write_u8(1);
                sink.write(&data[..]);
            }
        }
        match &self.mapper {
            MapperState::Nrom => {}
            MapperState::Mmc1(state) => {
                sink.write_u8(state.shift_register);
                sink.write_u8(state.control);
                sink.write_u8(state.chr_bank0);
                sink.write_u8(state.chr_bank1);
                sink.write_u8(state.prg_bank);
                sink.write_bool(state.previous_cpu_access_was_write);
            }
            MapperState::Uxrom { selected_prg_bank } => sink.write_u8(*selected_prg_bank),
            MapperState::Cnrom { selected_chr_bank } => sink.write_u8(*selected_chr_bank),
        }
    }

    pub(crate) fn read_state(&mut self, reader: &mut StateReader<'_>) -> Result<(), StateError> {
        if reader.read_u16()? != self.image.metadata.mapper {
            return Err(StateError::InvalidPayload(
                "mapper state does not match the cartridge",
            ));
        }
        self.prg_ram
            .copy_from_slice(reader.read_bytes(PRG_RAM_LEN)?);
        match (reader.read_u8()?, &mut self.chr) {
            (0, ChrMemory::Rom(_)) => {}
            (1, ChrMemory::Ram(data)) => {
                data.copy_from_slice(reader.read_bytes(CHR_ROM_BANK_LEN)?);
            }
            (0, ChrMemory::Ram(_)) | (1, ChrMemory::Rom(_)) => {
                return Err(StateError::InvalidPayload(
                    "CHR memory kind does not match the cartridge",
                ));
            }
            _ => return Err(StateError::InvalidPayload("invalid CHR memory kind")),
        }
        let prg_bank_count = self.prg_bank_count();
        let chr_bank_count = self.chr_bank_count();
        match &mut self.mapper {
            MapperState::Nrom => {}
            MapperState::Mmc1(state) => {
                let shift_register = reader.read_u8()?;
                let control = reader.read_u8()?;
                let chr_bank0 = reader.read_u8()?;
                let chr_bank1 = reader.read_u8()?;
                let prg_bank = reader.read_u8()?;
                if shift_register == 0 || shift_register > 0x1f {
                    return Err(StateError::InvalidPayload(
                        "invalid MMC1 serial shift register",
                    ));
                }
                if [control, chr_bank0, chr_bank1, prg_bank]
                    .into_iter()
                    .any(|value| value > 0x1f)
                {
                    return Err(StateError::InvalidPayload("invalid MMC1 mapper register"));
                }
                state.shift_register = shift_register;
                state.control = control;
                state.chr_bank0 = chr_bank0;
                state.chr_bank1 = chr_bank1;
                state.prg_bank = prg_bank;
                state.previous_cpu_access_was_write = reader.read_bool()?;
            }
            MapperState::Uxrom { selected_prg_bank } => {
                let bank = reader.read_u8()?;
                if usize::from(bank) >= prg_bank_count {
                    return Err(StateError::InvalidPayload(
                        "UxROM PRG bank is outside the cartridge",
                    ));
                }
                *selected_prg_bank = bank;
            }
            MapperState::Cnrom { selected_chr_bank } => {
                let bank = reader.read_u8()?;
                if usize::from(bank) >= chr_bank_count {
                    return Err(StateError::InvalidPayload(
                        "CNROM CHR bank is outside the cartridge",
                    ));
                }
                *selected_chr_bank = bank;
            }
        }
        Ok(())
    }

    pub(crate) fn copy_mutable_state_from(&mut self, source: &Self) {
        debug_assert_eq!(self.image, source.image);
        self.prg_ram.copy_from_slice(&source.prg_ram[..]);
        match (&mut self.chr, &source.chr) {
            (ChrMemory::Ram(target), ChrMemory::Ram(source)) => {
                target.copy_from_slice(&source[..]);
            }
            (ChrMemory::Rom(_), ChrMemory::Rom(_)) => {}
            _ => unreachable!("matching cartridge images have matching CHR memory kinds"),
        }
        self.mapper = source.mapper.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_rom::{CnromBuilder, Mmc1Builder, NromBuilder, UxromBuilder};

    #[test]
    fn parses_nrom_and_mirrors_one_prg_bank() {
        let mut builder = NromBuilder::new_16k();
        builder.write(0x8000, &[0x12]);
        let image = CartridgeImage::parse(&builder.build()).unwrap();
        assert_eq!(image.metadata().prg_rom_len, PRG_ROM_BANK_LEN);
        assert_eq!(image.metadata().chr_rom_len, CHR_ROM_BANK_LEN);

        let cartridge = Cartridge::new(image);
        assert_eq!(cartridge.cpu_read(0x8000), Some(0x12));
        assert_eq!(cartridge.cpu_read(0xc000), Some(0x12));
    }

    #[test]
    fn parses_uxrom_and_rejects_incompatible_layouts() {
        let bytes = UxromBuilder::new(8).build();
        let image = CartridgeImage::parse(&bytes).unwrap();
        assert_eq!(image.metadata().mapper, 2);
        assert_eq!(image.metadata().prg_rom_len, 8 * PRG_ROM_BANK_LEN);
        assert_eq!(image.metadata().chr_rom_len, 0);
        assert!(image.metadata().chr_is_ram);

        let mut invalid_prg = bytes.clone();
        invalid_prg[4] = 3;
        assert_eq!(
            CartridgeImage::parse(&invalid_prg),
            Err(CartridgeError::UnsupportedPrgRomBanks {
                mapper: 2,
                banks: 3,
            })
        );

        let mut invalid_chr = bytes;
        invalid_chr[5] = 1;
        assert_eq!(
            CartridgeImage::parse(&invalid_chr),
            Err(CartridgeError::UnsupportedChrRomBanks {
                mapper: 2,
                banks: 1,
            })
        );

        let mut invalid_mirroring = UxromBuilder::new(8).build();
        invalid_mirroring[6] |= 0x08;
        assert_eq!(
            CartridgeImage::parse(&invalid_mirroring),
            Err(CartridgeError::UnsupportedFourScreenMirroring(2))
        );
    }

    #[test]
    fn parses_mmc1_and_rejects_out_of_scope_layouts() {
        let bytes = Mmc1Builder::with_chr_rom(16, 16).build();
        let image = CartridgeImage::parse(&bytes).unwrap();
        assert_eq!(image.metadata().mapper, 1);
        assert_eq!(image.metadata().prg_rom_len, 16 * PRG_ROM_BANK_LEN);
        assert_eq!(image.metadata().chr_rom_len, 16 * CHR_ROM_BANK_LEN);
        assert!(!image.metadata().chr_is_ram);

        let chr_ram = CartridgeImage::parse(&Mmc1Builder::with_chr_ram(2).build()).unwrap();
        assert!(chr_ram.metadata().chr_is_ram);

        let mut invalid_prg = bytes.clone();
        invalid_prg[4] = 32;
        assert_eq!(
            CartridgeImage::parse(&invalid_prg),
            Err(CartridgeError::UnsupportedPrgRomBanks {
                mapper: 1,
                banks: 32,
            })
        );

        let mut invalid_chr = bytes.clone();
        invalid_chr[5] = 3;
        assert_eq!(
            CartridgeImage::parse(&invalid_chr),
            Err(CartridgeError::UnsupportedChrRomBanks {
                mapper: 1,
                banks: 3,
            })
        );

        let mut invalid_mirroring = bytes;
        invalid_mirroring[6] |= 0x08;
        assert_eq!(
            CartridgeImage::parse(&invalid_mirroring),
            Err(CartridgeError::UnsupportedFourScreenMirroring(1))
        );
    }

    #[test]
    fn parses_cnrom_and_rejects_incompatible_layouts() {
        let bytes = CnromBuilder::new_32k(4).build();
        let image = CartridgeImage::parse(&bytes).unwrap();
        assert_eq!(image.metadata().mapper, 3);
        assert_eq!(image.metadata().prg_rom_len, 2 * PRG_ROM_BANK_LEN);
        assert_eq!(image.metadata().chr_rom_len, 4 * CHR_ROM_BANK_LEN);
        assert!(!image.metadata().chr_is_ram);

        let mut invalid_prg = bytes.clone();
        invalid_prg[4] = 3;
        assert_eq!(
            CartridgeImage::parse(&invalid_prg),
            Err(CartridgeError::UnsupportedPrgRomBanks {
                mapper: 3,
                banks: 3,
            })
        );

        let mut invalid_chr = bytes.clone();
        invalid_chr[5] = 1;
        assert_eq!(
            CartridgeImage::parse(&invalid_chr),
            Err(CartridgeError::UnsupportedChrRomBanks {
                mapper: 3,
                banks: 1,
            })
        );

        let mut invalid_mirroring = bytes;
        invalid_mirroring[6] |= 0x08;
        assert_eq!(
            CartridgeImage::parse(&invalid_mirroring),
            Err(CartridgeError::UnsupportedFourScreenMirroring(3))
        );
    }

    #[test]
    fn cartridge_identity_covers_the_canonical_image_but_not_trailing_bytes() {
        let bytes = NromBuilder::new_16k().build();
        let identity = CartridgeImage::parse(&bytes).unwrap().identity();
        assert_eq!(identity.byte_len as usize, bytes.len());

        let mut with_trailing = bytes.clone();
        with_trailing.extend_from_slice(&[0xa5; 7]);
        assert_eq!(
            CartridgeImage::parse(&with_trailing).unwrap().identity(),
            identity
        );

        let mut changed = bytes;
        *changed.last_mut().unwrap() ^= 1;
        assert_ne!(
            CartridgeImage::parse(&changed).unwrap().identity(),
            identity
        );
    }

    #[test]
    fn rejects_every_truncation_boundary() {
        let bytes = NromBuilder::new_16k().build();
        for len in 0..bytes.len() {
            assert!(CartridgeImage::parse(&bytes[..len]).is_err(), "len={len}");
        }
        assert!(CartridgeImage::parse(&bytes).is_ok());
    }

    #[test]
    fn rejects_unsupported_formats_and_bank_counts() {
        let valid = NromBuilder::new_16k().build();

        let mut bytes = valid.clone();
        bytes[0] = 0;
        assert!(matches!(
            CartridgeImage::parse(&bytes),
            Err(CartridgeError::InvalidMagic(_))
        ));

        let mut bytes = valid.clone();
        bytes[7] = 0x08;
        assert_eq!(
            CartridgeImage::parse(&bytes),
            Err(CartridgeError::UnsupportedNes2)
        );

        let mut bytes = valid.clone();
        bytes[6] = 0x40;
        assert_eq!(
            CartridgeImage::parse(&bytes),
            Err(CartridgeError::UnsupportedMapper(4))
        );

        let mut bytes = valid.clone();
        bytes[4] = 0;
        assert_eq!(
            CartridgeImage::parse(&bytes),
            Err(CartridgeError::UnsupportedPrgRomBanks {
                mapper: 0,
                banks: 0,
            })
        );

        let mut bytes = valid;
        bytes[5] = 2;
        assert_eq!(
            CartridgeImage::parse(&bytes),
            Err(CartridgeError::UnsupportedChrRomBanks {
                mapper: 0,
                banks: 2,
            })
        );
    }

    #[test]
    fn trainer_initializes_its_hardware_prg_ram_window() {
        let mut builder = NromBuilder::new_16k();
        builder.set_trainer(std::array::from_fn(|index| index as u8));
        let image = CartridgeImage::parse(&builder.build()).unwrap();
        let cartridge = Cartridge::new(image);
        assert_eq!(cartridge.cpu_read(0x7000), Some(0));
        assert_eq!(cartridge.cpu_read(0x7101), Some(1));
        assert_eq!(cartridge.cpu_read(0x71ff), Some(255));
    }

    #[test]
    fn chr_ram_is_writable_but_chr_rom_is_not() {
        let ram_image =
            CartridgeImage::parse(&NromBuilder::new_16k().without_chr().build()).unwrap();
        let mut ram_cartridge = Cartridge::new(ram_image);
        assert!(ram_cartridge.ppu_write(0x0123, 0xa5));
        assert_eq!(ram_cartridge.ppu_read(0x0123), Some(0xa5));

        let rom_image = CartridgeImage::parse(&NromBuilder::new_16k().build()).unwrap();
        let mut rom_cartridge = Cartridge::new(rom_image);
        assert!(!rom_cartridge.ppu_write(0x0123, 0xa5));
        assert_eq!(rom_cartridge.ppu_read(0x0123), Some(0));
    }

    #[test]
    fn maps_each_nametable_mirroring_layout() {
        let starts = [0x2000, 0x2400, 0x2800, 0x2c00];
        assert_eq!(
            starts.map(|address| Mirroring::OneScreenLower.map_nametable_address(address)),
            [0x000, 0x000, 0x000, 0x000]
        );
        assert_eq!(
            starts.map(|address| Mirroring::OneScreenUpper.map_nametable_address(address)),
            [0x400, 0x400, 0x400, 0x400]
        );
        assert_eq!(
            starts.map(|address| Mirroring::Horizontal.map_nametable_address(address)),
            [0x000, 0x000, 0x400, 0x400]
        );
        assert_eq!(
            starts.map(|address| Mirroring::Vertical.map_nametable_address(address)),
            [0x000, 0x400, 0x000, 0x400]
        );
        assert_eq!(
            starts.map(|address| Mirroring::FourScreen.map_nametable_address(address)),
            [0x000, 0x400, 0x800, 0xc00]
        );
        assert_eq!(
            Mirroring::Vertical.map_nametable_address(0x3eff),
            Mirroring::Vertical.map_nametable_address(0x2eff)
        );
    }

    #[test]
    fn cloned_images_share_rom_but_machines_keep_independent_mutable_memory() {
        let image = CartridgeImage::parse(&NromBuilder::new_32k().without_chr().build()).unwrap();
        let cloned_image = image.clone();
        assert!(Arc::ptr_eq(&image.prg_rom, &cloned_image.prg_rom));

        let mut first = Cartridge::new(image);
        let second = Cartridge::new(cloned_image);
        assert!(first.cpu_write(0x6000, 0xa5));
        assert!(first.ppu_write(0x0000, 0x5a));
        assert_eq!(first.cpu_read(0x6000), Some(0xa5));
        assert_eq!(first.ppu_read(0x0000), Some(0x5a));
        assert_eq!(second.cpu_read(0x6000), Some(0));
        assert_eq!(second.ppu_read(0x0000), Some(0));
    }

    #[test]
    fn reports_header_metadata_flags() {
        let mut builder = NromBuilder::new_16k().without_chr();
        builder.set_vertical_mirroring(true);
        builder.set_battery_backed(true);
        builder.set_trainer([0x5a; TRAINER_LEN]);
        let image = CartridgeImage::parse(&builder.build()).unwrap();
        assert_eq!(
            image.metadata(),
            CartridgeMetadata {
                mapper: 0,
                mirroring: Mirroring::Vertical,
                battery_backed: true,
                has_trainer: true,
                prg_rom_len: PRG_ROM_BANK_LEN,
                chr_rom_len: 0,
                chr_is_ram: true,
            }
        );
    }
}
