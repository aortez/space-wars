use std::sync::Arc;

use crate::state_codec::fnv1a64;
use crate::{CartridgeError, StateError, state_codec::StateReader, state_codec::StateSink};

const INES_HEADER_LEN: usize = 16;
const TRAINER_LEN: usize = 512;
const PRG_ROM_BANK_LEN: usize = 16 * 1024;
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
    Horizontal,
    Vertical,
    FourScreen,
}

impl Mirroring {
    /// Maps `$2000-$2fff` (and the `$3000-$3eff` mirror) into physical
    /// nametable storage. Horizontal/vertical layouts use 2 KiB; four-screen
    /// layouts use 4 KiB supplied by the cartridge.
    pub const fn map_nametable_address(self, address: u16) -> usize {
        let offset = (address.wrapping_sub(0x2000) as usize) & 0x0fff;
        let table = offset / 0x0400;
        let within_table = offset & 0x03ff;
        let physical_table = match self {
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
        if mapper != 0 {
            return Err(CartridgeError::UnsupportedMapper(mapper));
        }
        if !matches!(prg_banks, 1 | 2) {
            return Err(CartridgeError::UnsupportedPrgRomBanks(prg_banks));
        }
        if chr_banks > 1 {
            return Err(CartridgeError::UnsupportedChrRomBanks(chr_banks));
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

/// Mutable NROM state belonging to one machine.
#[derive(Clone, Debug)]
pub struct Cartridge {
    image: CartridgeImage,
    prg_ram: Box<[u8; PRG_RAM_LEN]>,
    chr: ChrMemory,
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
        Self {
            image,
            prg_ram,
            chr,
        }
    }

    pub fn image(&self) -> &CartridgeImage {
        &self.image
    }

    pub fn cpu_read(&self, address: u16) -> Option<u8> {
        match address {
            0x6000..=0x7fff => Some(self.prg_ram[usize::from(address - 0x6000)]),
            0x8000..=0xffff => {
                let mut offset = usize::from(address - 0x8000);
                if self.image.prg_rom.len() == PRG_ROM_BANK_LEN {
                    offset %= PRG_ROM_BANK_LEN;
                }
                Some(self.image.prg_rom[offset])
            }
            _ => None,
        }
    }

    pub fn cpu_write(&mut self, address: u16, value: u8) -> bool {
        match address {
            0x6000..=0x7fff => {
                self.prg_ram[usize::from(address - 0x6000)] = value;
                true
            }
            0x8000..=0xffff => true,
            _ => false,
        }
    }

    pub fn ppu_read(&self, address: u16) -> Option<u8> {
        let offset = usize::from(address & 0x1fff);
        match &self.chr {
            ChrMemory::Rom(data) => data.get(offset).copied(),
            ChrMemory::Ram(data) => data.get(offset).copied(),
        }
    }

    pub fn ppu_write(&mut self, address: u16, value: u8) -> bool {
        let offset = usize::from(address & 0x1fff);
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

    pub(crate) fn write_state<S: StateSink>(&self, sink: &mut S) {
        sink.write(&self.prg_ram[..]);
        match &self.chr {
            ChrMemory::Rom(_) => sink.write_u8(0),
            ChrMemory::Ram(data) => {
                sink.write_u8(1);
                sink.write(&data[..]);
            }
        }
    }

    pub(crate) fn read_state(&mut self, reader: &mut StateReader<'_>) -> Result<(), StateError> {
        self.prg_ram
            .copy_from_slice(reader.read_bytes(PRG_RAM_LEN)?);
        match (reader.read_u8()?, &mut self.chr) {
            (0, ChrMemory::Rom(_)) => Ok(()),
            (1, ChrMemory::Ram(data)) => {
                data.copy_from_slice(reader.read_bytes(CHR_ROM_BANK_LEN)?);
                Ok(())
            }
            (0, ChrMemory::Ram(_)) | (1, ChrMemory::Rom(_)) => Err(StateError::InvalidPayload(
                "CHR memory kind does not match the cartridge",
            )),
            _ => Err(StateError::InvalidPayload("invalid CHR memory kind")),
        }
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_rom::NromBuilder;

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
        bytes[6] = 0x10;
        assert_eq!(
            CartridgeImage::parse(&bytes),
            Err(CartridgeError::UnsupportedMapper(1))
        );

        let mut bytes = valid.clone();
        bytes[4] = 0;
        assert_eq!(
            CartridgeImage::parse(&bytes),
            Err(CartridgeError::UnsupportedPrgRomBanks(0))
        );

        let mut bytes = valid;
        bytes[5] = 2;
        assert_eq!(
            CartridgeImage::parse(&bytes),
            Err(CartridgeError::UnsupportedChrRomBanks(2))
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
