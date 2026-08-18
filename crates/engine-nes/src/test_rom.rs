//! Deterministic, repository-owned mapper-0 ROM construction for tests,
//! examples, and benchmarks. It intentionally supports only the subset needed
//! to exercise the engine.

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
}
