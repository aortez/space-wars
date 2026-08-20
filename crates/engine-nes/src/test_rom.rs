//! Deterministic, repository-owned ROM construction for tests, examples, and
//! benchmarks. The builders intentionally support only the subsets needed to
//! exercise each mapper.

const HEADER_LEN: usize = 16;
const TRAINER_LEN: usize = 512;
const PRG_BANK_LEN: usize = 16 * 1024;
const CHR_BANK_LEN: usize = 8 * 1024;

#[derive(Clone, Debug)]
pub struct NromBuilder {
    prg: Vec<u8>,
    chr: Option<Vec<u8>>,
    trainer: Option<[u8; TRAINER_LEN]>,
    flags6: u8,
}

impl NromBuilder {
    pub fn new_16k() -> Self {
        Self::new(1)
    }

    pub fn new_32k() -> Self {
        Self::new(2)
    }

    fn new(prg_banks: usize) -> Self {
        Self {
            prg: vec![0xea; prg_banks * PRG_BANK_LEN],
            chr: Some(vec![0; CHR_BANK_LEN]),
            trainer: None,
            flags6: 0,
        }
    }

    pub fn without_chr(mut self) -> Self {
        self.chr = None;
        self
    }

    pub fn set_vertical_mirroring(&mut self, vertical: bool) {
        self.flags6 = (self.flags6 & !1) | u8::from(vertical);
    }

    pub fn set_battery_backed(&mut self, battery_backed: bool) {
        self.flags6 = (self.flags6 & !2) | (u8::from(battery_backed) << 1);
    }

    pub fn set_trainer(&mut self, trainer: [u8; TRAINER_LEN]) {
        self.trainer = Some(trainer);
    }

    pub fn write(&mut self, cpu_address: u16, bytes: &[u8]) {
        assert!(cpu_address >= 0x8000, "PRG writes start at CPU $8000");
        let mut offset = usize::from(cpu_address - 0x8000);
        if self.prg.len() == PRG_BANK_LEN {
            offset %= PRG_BANK_LEN;
        }
        let end = offset.checked_add(bytes.len()).expect("ROM write overflow");
        assert!(end <= self.prg.len(), "ROM write exceeds PRG storage");
        self.prg[offset..end].copy_from_slice(bytes);
    }

    pub fn write_chr(&mut self, ppu_address: u16, bytes: &[u8]) {
        let chr = self
            .chr
            .as_mut()
            .expect("CHR writes require a ROM-backed test cartridge");
        let offset = usize::from(ppu_address);
        let end = offset.checked_add(bytes.len()).expect("CHR write overflow");
        assert!(end <= chr.len(), "CHR write exceeds pattern-table storage");
        chr[offset..end].copy_from_slice(bytes);
    }

    pub fn set_vectors(&mut self, nmi: u16, reset: u16, irq: u16) {
        self.write(0xfffa, &nmi.to_le_bytes());
        self.write(0xfffc, &reset.to_le_bytes());
        self.write(0xfffe, &irq.to_le_bytes());
    }

    pub fn build(&self) -> Vec<u8> {
        let trainer_len = self.trainer.as_ref().map_or(0, |_| TRAINER_LEN);
        let chr_len = self.chr.as_ref().map_or(0, Vec::len);
        let mut image = Vec::with_capacity(HEADER_LEN + trainer_len + self.prg.len() + chr_len);
        let mut flags6 = self.flags6;
        if self.trainer.is_some() {
            flags6 |= 0x04;
        }
        image.extend_from_slice(b"NES\x1a");
        image.push((self.prg.len() / PRG_BANK_LEN) as u8);
        image.push(u8::from(self.chr.is_some()));
        image.push(flags6);
        image.push(0);
        image.extend_from_slice(&[0; 8]);
        if let Some(trainer) = &self.trainer {
            image.extend_from_slice(trainer);
        }
        image.extend_from_slice(&self.prg);
        if let Some(chr) = &self.chr {
            image.extend_from_slice(chr);
        }
        image
    }
}

#[derive(Clone, Debug)]
pub struct Mmc1Builder {
    prg: Vec<u8>,
    chr: Option<Vec<u8>>,
    flags6: u8,
}

impl Mmc1Builder {
    pub fn with_chr_ram(prg_banks: usize) -> Self {
        Self::new(prg_banks, None)
    }

    pub fn with_chr_rom(prg_banks: usize, chr_banks: usize) -> Self {
        assert!(
            (1..=16).contains(&chr_banks) && chr_banks.is_power_of_two(),
            "MMC1 test images require 1-16 power-of-two CHR ROM banks"
        );
        Self::new(prg_banks, Some(chr_banks))
    }

    fn new(prg_banks: usize, chr_banks: Option<usize>) -> Self {
        assert!(
            (2..=16).contains(&prg_banks) && prg_banks.is_power_of_two(),
            "MMC1 test images require 2-16 power-of-two PRG banks"
        );
        Self {
            prg: vec![0xea; prg_banks * PRG_BANK_LEN],
            chr: chr_banks.map(|banks| vec![0; banks * CHR_BANK_LEN]),
            flags6: 0,
        }
    }

    pub fn prg_bank_count(&self) -> usize {
        self.prg.len() / PRG_BANK_LEN
    }

    pub fn chr_half_bank_count(&self) -> usize {
        self.chr
            .as_ref()
            .map_or(2, |chr| chr.len() / (CHR_BANK_LEN / 2))
    }

    pub fn set_vertical_mirroring(&mut self, vertical: bool) {
        self.flags6 = (self.flags6 & !1) | u8::from(vertical);
    }

    pub fn set_battery_backed(&mut self, battery_backed: bool) {
        self.flags6 = (self.flags6 & !2) | (u8::from(battery_backed) << 1);
    }

    pub fn write_prg_bank(&mut self, bank: usize, offset: usize, bytes: &[u8]) {
        assert!(
            bank < self.prg_bank_count(),
            "MMC1 PRG bank is out of range"
        );
        let start = bank
            .checked_mul(PRG_BANK_LEN)
            .and_then(|start| start.checked_add(offset))
            .expect("ROM write overflow");
        let end = start.checked_add(bytes.len()).expect("ROM write overflow");
        assert!(offset < PRG_BANK_LEN, "ROM write starts outside a PRG bank");
        assert!(
            end <= (bank + 1) * PRG_BANK_LEN,
            "ROM write crosses a PRG bank boundary"
        );
        self.prg[start..end].copy_from_slice(bytes);
    }

    pub fn write_fixed_last(&mut self, cpu_address: u16, bytes: &[u8]) {
        assert!(
            cpu_address >= 0xc000,
            "fixed MMC1 writes start at CPU $C000"
        );
        self.write_prg_bank(
            self.prg_bank_count() - 1,
            usize::from(cpu_address - 0xc000),
            bytes,
        );
    }

    pub fn write_chr_half_bank(&mut self, bank: usize, offset: usize, bytes: &[u8]) {
        let chr = self
            .chr
            .as_mut()
            .expect("CHR writes require a ROM-backed MMC1 test cartridge");
        let half_bank_len = CHR_BANK_LEN / 2;
        let half_bank_count = chr.len() / half_bank_len;
        assert!(bank < half_bank_count, "MMC1 CHR bank is out of range");
        let start = bank
            .checked_mul(half_bank_len)
            .and_then(|start| start.checked_add(offset))
            .expect("ROM write overflow");
        let end = start.checked_add(bytes.len()).expect("ROM write overflow");
        assert!(
            offset < half_bank_len,
            "ROM write starts outside a CHR bank"
        );
        assert!(
            end <= (bank + 1) * half_bank_len,
            "ROM write crosses a CHR bank boundary"
        );
        chr[start..end].copy_from_slice(bytes);
    }

    pub fn set_vectors(&mut self, nmi: u16, reset: u16, irq: u16) {
        self.write_fixed_last(0xfffa, &nmi.to_le_bytes());
        self.write_fixed_last(0xfffc, &reset.to_le_bytes());
        self.write_fixed_last(0xfffe, &irq.to_le_bytes());
    }

    pub fn build(&self) -> Vec<u8> {
        let chr_len = self.chr.as_ref().map_or(0, Vec::len);
        let mut image = Vec::with_capacity(HEADER_LEN + self.prg.len() + chr_len);
        image.extend_from_slice(b"NES\x1a");
        image.push(self.prg_bank_count() as u8);
        image.push((chr_len / CHR_BANK_LEN) as u8);
        image.push(self.flags6 | 0x10);
        image.push(0);
        image.extend_from_slice(&[0; 8]);
        image.extend_from_slice(&self.prg);
        if let Some(chr) = &self.chr {
            image.extend_from_slice(chr);
        }
        image
    }
}

#[derive(Clone, Debug)]
pub struct Mmc3Builder {
    prg: Vec<u8>,
    chr: Option<Vec<u8>>,
    flags6: u8,
}

impl Mmc3Builder {
    pub fn with_chr_ram(prg_banks: usize) -> Self {
        Self::new(prg_banks, None)
    }

    pub fn with_chr_rom(prg_banks: usize, chr_banks: usize) -> Self {
        assert!(
            (1..=32).contains(&chr_banks) && chr_banks.is_power_of_two(),
            "MMC3 test images require 1-32 power-of-two CHR ROM banks"
        );
        Self::new(prg_banks, Some(chr_banks))
    }

    fn new(prg_banks: usize, chr_banks: Option<usize>) -> Self {
        assert!(
            (1..=32).contains(&prg_banks) && prg_banks.is_power_of_two(),
            "MMC3 test images require 1-32 power-of-two PRG banks"
        );
        Self {
            prg: vec![0xea; prg_banks * PRG_BANK_LEN],
            chr: chr_banks.map(|banks| vec![0; banks * CHR_BANK_LEN]),
            flags6: 0,
        }
    }

    pub fn prg_half_bank_count(&self) -> usize {
        self.prg.len() / (PRG_BANK_LEN / 2)
    }

    pub fn chr_quarter_bank_count(&self) -> usize {
        self.chr.as_ref().map_or(8, |chr| chr.len() / 1024)
    }

    pub fn set_vertical_mirroring(&mut self, vertical: bool) {
        self.flags6 = (self.flags6 & !1) | u8::from(vertical);
    }

    pub fn set_battery_backed(&mut self, battery_backed: bool) {
        self.flags6 = (self.flags6 & !2) | (u8::from(battery_backed) << 1);
    }

    pub fn set_four_screen(&mut self, four_screen: bool) {
        self.flags6 = (self.flags6 & !0x08) | (u8::from(four_screen) << 3);
    }

    pub fn write_prg_half_bank(&mut self, bank: usize, offset: usize, bytes: &[u8]) {
        let bank_len = PRG_BANK_LEN / 2;
        assert!(
            bank < self.prg_half_bank_count(),
            "MMC3 PRG bank is out of range"
        );
        let start = bank
            .checked_mul(bank_len)
            .and_then(|start| start.checked_add(offset))
            .expect("ROM write overflow");
        let end = start.checked_add(bytes.len()).expect("ROM write overflow");
        assert!(offset < bank_len, "ROM write starts outside a PRG bank");
        assert!(
            end <= (bank + 1) * bank_len,
            "ROM write crosses a PRG bank boundary"
        );
        self.prg[start..end].copy_from_slice(bytes);
    }

    pub fn write_fixed_last(&mut self, cpu_address: u16, bytes: &[u8]) {
        assert!(
            cpu_address >= 0xe000,
            "fixed MMC3 writes start at CPU $E000"
        );
        self.write_prg_half_bank(
            self.prg_half_bank_count() - 1,
            usize::from(cpu_address - 0xe000),
            bytes,
        );
    }

    pub fn write_chr_quarter_bank(&mut self, bank: usize, offset: usize, bytes: &[u8]) {
        let chr = self
            .chr
            .as_mut()
            .expect("CHR writes require a ROM-backed MMC3 test cartridge");
        let bank_len = 1024;
        let bank_count = chr.len() / bank_len;
        assert!(bank < bank_count, "MMC3 CHR bank is out of range");
        let start = bank
            .checked_mul(bank_len)
            .and_then(|start| start.checked_add(offset))
            .expect("ROM write overflow");
        let end = start.checked_add(bytes.len()).expect("ROM write overflow");
        assert!(offset < bank_len, "ROM write starts outside a CHR bank");
        assert!(
            end <= (bank + 1) * bank_len,
            "ROM write crosses a CHR bank boundary"
        );
        chr[start..end].copy_from_slice(bytes);
    }

    pub fn set_vectors(&mut self, nmi: u16, reset: u16, irq: u16) {
        self.write_fixed_last(0xfffa, &nmi.to_le_bytes());
        self.write_fixed_last(0xfffc, &reset.to_le_bytes());
        self.write_fixed_last(0xfffe, &irq.to_le_bytes());
    }

    pub fn build(&self) -> Vec<u8> {
        let chr_len = self.chr.as_ref().map_or(0, Vec::len);
        let mut image = Vec::with_capacity(HEADER_LEN + self.prg.len() + chr_len);
        image.extend_from_slice(b"NES\x1a");
        image.push((self.prg.len() / PRG_BANK_LEN) as u8);
        image.push((chr_len / CHR_BANK_LEN) as u8);
        image.push(self.flags6 | 0x40);
        image.push(0);
        image.extend_from_slice(&[0; 8]);
        image.extend_from_slice(&self.prg);
        if let Some(chr) = &self.chr {
            image.extend_from_slice(chr);
        }
        image
    }
}

#[derive(Clone, Debug)]
pub struct AxromBuilder {
    prg: Vec<u8>,
}

impl AxromBuilder {
    pub fn new(prg_banks: usize) -> Self {
        assert!(
            (1..=8).contains(&prg_banks) && prg_banks.is_power_of_two(),
            "AxROM test images require 1-8 power-of-two 32 KiB PRG banks"
        );
        Self {
            prg: vec![0xea; prg_banks * 2 * PRG_BANK_LEN],
        }
    }

    pub fn prg_bank_count(&self) -> usize {
        self.prg.len() / (2 * PRG_BANK_LEN)
    }

    pub fn write_bank(&mut self, bank: usize, offset: usize, bytes: &[u8]) {
        let bank_len = 2 * PRG_BANK_LEN;
        assert!(
            bank < self.prg_bank_count(),
            "AxROM PRG bank is out of range"
        );
        let start = bank
            .checked_mul(bank_len)
            .and_then(|start| start.checked_add(offset))
            .expect("ROM write overflow");
        let end = start.checked_add(bytes.len()).expect("ROM write overflow");
        assert!(offset < bank_len, "ROM write starts outside a PRG bank");
        assert!(
            end <= (bank + 1) * bank_len,
            "ROM write crosses a PRG bank boundary"
        );
        self.prg[start..end].copy_from_slice(bytes);
    }

    pub fn set_vectors(&mut self, nmi: u16, reset: u16, irq: u16) {
        let vectors = [nmi.to_le_bytes(), reset.to_le_bytes(), irq.to_le_bytes()];
        for bank in 0..self.prg_bank_count() {
            let mut offset = 0x7ffa;
            for vector in vectors {
                self.write_bank(bank, offset, &vector);
                offset += 2;
            }
        }
    }

    pub fn build(&self) -> Vec<u8> {
        let mut image = Vec::with_capacity(HEADER_LEN + self.prg.len());
        image.extend_from_slice(b"NES\x1a");
        image.push((self.prg.len() / PRG_BANK_LEN) as u8);
        image.push(0);
        image.push(0x70);
        image.push(0);
        image.extend_from_slice(&[0; 8]);
        image.extend_from_slice(&self.prg);
        image
    }
}

#[derive(Clone, Debug)]
pub struct UxromBuilder {
    prg: Vec<u8>,
    flags6: u8,
}

impl UxromBuilder {
    pub fn new(prg_banks: usize) -> Self {
        assert!(
            (2..=128).contains(&prg_banks) && prg_banks.is_power_of_two(),
            "UxROM test images require 2-128 power-of-two PRG banks"
        );
        Self {
            prg: vec![0xea; prg_banks * PRG_BANK_LEN],
            flags6: 0,
        }
    }

    pub fn prg_bank_count(&self) -> usize {
        self.prg.len() / PRG_BANK_LEN
    }

    pub fn set_vertical_mirroring(&mut self, vertical: bool) {
        self.flags6 = (self.flags6 & !1) | u8::from(vertical);
    }

    pub fn write_bank(&mut self, bank: usize, offset: usize, bytes: &[u8]) {
        assert!(
            bank < self.prg_bank_count(),
            "UxROM PRG bank is out of range"
        );
        let start = bank
            .checked_mul(PRG_BANK_LEN)
            .and_then(|start| start.checked_add(offset))
            .expect("ROM write overflow");
        let end = start.checked_add(bytes.len()).expect("ROM write overflow");
        assert!(offset < PRG_BANK_LEN, "ROM write starts outside a PRG bank");
        assert!(
            end <= (bank + 1) * PRG_BANK_LEN,
            "ROM write crosses a PRG bank boundary"
        );
        self.prg[start..end].copy_from_slice(bytes);
    }

    pub fn write_fixed(&mut self, cpu_address: u16, bytes: &[u8]) {
        assert!(
            cpu_address >= 0xc000,
            "fixed UxROM writes start at CPU $C000"
        );
        self.write_bank(
            self.prg_bank_count() - 1,
            usize::from(cpu_address - 0xc000),
            bytes,
        );
    }

    pub fn set_vectors(&mut self, nmi: u16, reset: u16, irq: u16) {
        self.write_fixed(0xfffa, &nmi.to_le_bytes());
        self.write_fixed(0xfffc, &reset.to_le_bytes());
        self.write_fixed(0xfffe, &irq.to_le_bytes());
    }

    pub fn build(&self) -> Vec<u8> {
        let mut image = Vec::with_capacity(HEADER_LEN + self.prg.len());
        image.extend_from_slice(b"NES\x1a");
        image.push(self.prg_bank_count() as u8);
        image.push(0);
        image.push(self.flags6 | 0x20);
        image.push(0);
        image.extend_from_slice(&[0; 8]);
        image.extend_from_slice(&self.prg);
        image
    }
}

#[derive(Clone, Debug)]
pub struct CnromBuilder {
    prg: Vec<u8>,
    chr: Vec<u8>,
    flags6: u8,
}

impl CnromBuilder {
    pub fn new_16k(chr_banks: usize) -> Self {
        Self::new(1, chr_banks)
    }

    pub fn new_32k(chr_banks: usize) -> Self {
        Self::new(2, chr_banks)
    }

    fn new(prg_banks: usize, chr_banks: usize) -> Self {
        assert!(
            matches!(chr_banks, 2 | 4),
            "CNROM test images require two or four CHR banks"
        );
        Self {
            prg: vec![0xea; prg_banks * PRG_BANK_LEN],
            chr: vec![0; chr_banks * CHR_BANK_LEN],
            flags6: 0,
        }
    }

    pub fn chr_bank_count(&self) -> usize {
        self.chr.len() / CHR_BANK_LEN
    }

    pub fn set_vertical_mirroring(&mut self, vertical: bool) {
        self.flags6 = (self.flags6 & !1) | u8::from(vertical);
    }

    pub fn write_prg(&mut self, cpu_address: u16, bytes: &[u8]) {
        assert!(cpu_address >= 0x8000, "PRG writes start at CPU $8000");
        let mut offset = usize::from(cpu_address - 0x8000);
        if self.prg.len() == PRG_BANK_LEN {
            offset %= PRG_BANK_LEN;
        }
        let end = offset.checked_add(bytes.len()).expect("ROM write overflow");
        assert!(end <= self.prg.len(), "ROM write exceeds PRG storage");
        self.prg[offset..end].copy_from_slice(bytes);
    }

    pub fn write_chr_bank(&mut self, bank: usize, offset: usize, bytes: &[u8]) {
        assert!(
            bank < self.chr_bank_count(),
            "CNROM CHR bank is out of range"
        );
        let start = bank
            .checked_mul(CHR_BANK_LEN)
            .and_then(|start| start.checked_add(offset))
            .expect("ROM write overflow");
        let end = start.checked_add(bytes.len()).expect("ROM write overflow");
        assert!(offset < CHR_BANK_LEN, "ROM write starts outside a CHR bank");
        assert!(
            end <= (bank + 1) * CHR_BANK_LEN,
            "ROM write crosses a CHR bank boundary"
        );
        self.chr[start..end].copy_from_slice(bytes);
    }

    pub fn set_vectors(&mut self, nmi: u16, reset: u16, irq: u16) {
        self.write_prg(0xfffa, &nmi.to_le_bytes());
        self.write_prg(0xfffc, &reset.to_le_bytes());
        self.write_prg(0xfffe, &irq.to_le_bytes());
    }

    pub fn build(&self) -> Vec<u8> {
        let mut image = Vec::with_capacity(HEADER_LEN + self.prg.len() + self.chr.len());
        image.extend_from_slice(b"NES\x1a");
        image.push((self.prg.len() / PRG_BANK_LEN) as u8);
        image.push(self.chr_bank_count() as u8);
        image.push(self.flags6 | 0x30);
        image.push(0);
        image.extend_from_slice(&[0; 8]);
        image.extend_from_slice(&self.prg);
        image.extend_from_slice(&self.chr);
        image
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_vectors_into_both_16k_cpu_windows() {
        let mut builder = NromBuilder::new_16k();
        builder.set_vectors(0x8123, 0x8456, 0x8789);
        let image = builder.build();
        let prg = &image[HEADER_LEN..HEADER_LEN + PRG_BANK_LEN];
        assert_eq!(&prg[0x3ffa..0x4000], &[0x23, 0x81, 0x56, 0x84, 0x89, 0x87]);
    }

    #[test]
    fn writes_chr_fixture_bytes() {
        let mut builder = NromBuilder::new_16k();
        builder.write_chr(0x123, &[0xa5, 0x5a]);
        let image = builder.build();
        let chr_start = HEADER_LEN + PRG_BANK_LEN;
        assert_eq!(&image[chr_start + 0x123..chr_start + 0x125], &[0xa5, 0x5a]);
    }

    #[test]
    fn builds_mapper_two_with_chr_ram_and_fixed_vectors() {
        let mut builder = UxromBuilder::new(8);
        builder.set_vertical_mirroring(true);
        builder.set_vectors(0xc123, 0xc456, 0xc789);
        let image = builder.build();
        assert_eq!(image[4], 8);
        assert_eq!(image[5], 0);
        assert_eq!(image[6], 0x21);
        let vectors = HEADER_LEN + 8 * PRG_BANK_LEN - 6;
        assert_eq!(&image[vectors..], &[0x23, 0xc1, 0x56, 0xc4, 0x89, 0xc7]);
    }

    #[test]
    fn builds_mapper_one_with_serial_banks_and_fixed_vectors() {
        let mut builder = Mmc1Builder::with_chr_rom(8, 4);
        builder.set_vertical_mirroring(true);
        builder.set_battery_backed(true);
        builder.write_prg_bank(3, 0x123, &[0xa5, 0x5a]);
        builder.write_chr_half_bank(5, 0x321, &[0x42]);
        builder.set_vectors(0xc123, 0xc456, 0xc789);
        let image = builder.build();
        assert_eq!(image[4], 8);
        assert_eq!(image[5], 4);
        assert_eq!(image[6], 0x13);
        assert_eq!(image[HEADER_LEN + 3 * PRG_BANK_LEN + 0x123], 0xa5);
        let chr_start = HEADER_LEN + 8 * PRG_BANK_LEN;
        assert_eq!(image[chr_start + 5 * (CHR_BANK_LEN / 2) + 0x321], 0x42);
        let vectors = HEADER_LEN + 8 * PRG_BANK_LEN - 6;
        assert_eq!(
            &image[vectors..chr_start],
            &[0x23, 0xc1, 0x56, 0xc4, 0x89, 0xc7]
        );
    }

    #[test]
    fn builds_mapper_three_with_switchable_chr_rom() {
        let mut builder = CnromBuilder::new_32k(4);
        builder.set_vertical_mirroring(true);
        builder.write_chr_bank(2, 0x123, &[0xa5, 0x5a]);
        builder.set_vectors(0x8123, 0x8456, 0x8789);
        let image = builder.build();
        assert_eq!(image[4], 2);
        assert_eq!(image[5], 4);
        assert_eq!(image[6], 0x31);
        let chr_start = HEADER_LEN + 2 * PRG_BANK_LEN;
        assert_eq!(
            &image[chr_start + 2 * CHR_BANK_LEN + 0x123..][..2],
            &[0xa5, 0x5a]
        );
    }

    #[test]
    fn builds_mapper_four_with_irq_banks_and_fixed_vectors() {
        let mut builder = Mmc3Builder::with_chr_rom(16, 16);
        builder.set_vertical_mirroring(true);
        builder.set_battery_backed(true);
        builder.write_prg_half_bank(7, 0x123, &[0xa5, 0x5a]);
        builder.write_chr_quarter_bank(11, 0x321, &[0x42]);
        builder.set_vectors(0xe123, 0xe456, 0xe789);
        let image = builder.build();
        assert_eq!(image[4], 16);
        assert_eq!(image[5], 16);
        assert_eq!(image[6], 0x43);
        let prg_half_len = PRG_BANK_LEN / 2;
        assert_eq!(image[HEADER_LEN + 7 * prg_half_len + 0x123], 0xa5);
        let chr_start = HEADER_LEN + 16 * PRG_BANK_LEN;
        assert_eq!(image[chr_start + 11 * 1024 + 0x321], 0x42);
        let vectors = HEADER_LEN + 16 * PRG_BANK_LEN - 6;
        assert_eq!(
            &image[vectors..chr_start],
            &[0x23, 0xe1, 0x56, 0xe4, 0x89, 0xe7]
        );
    }

    #[test]
    fn builds_mapper_seven_with_vectors_in_every_switchable_bank() {
        let mut builder = AxromBuilder::new(8);
        builder.write_bank(5, 0x123, &[0xa5, 0x5a]);
        builder.set_vectors(0x8123, 0x8456, 0x8789);
        let image = builder.build();
        assert_eq!(image[4], 16);
        assert_eq!(image[5], 0);
        assert_eq!(image[6], 0x70);
        let bank_len = 2 * PRG_BANK_LEN;
        assert_eq!(image[HEADER_LEN + 5 * bank_len + 0x123], 0xa5);
        for bank in 0..8 {
            let vectors = HEADER_LEN + bank * bank_len + 0x7ffa;
            assert_eq!(
                &image[vectors..vectors + 6],
                &[0x23, 0x81, 0x56, 0x84, 0x89, 0x87]
            );
        }
    }
}
