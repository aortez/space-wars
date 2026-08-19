use crate::StateError;

const FNV1A64_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A64_PRIME: u64 = 0x0000_0100_0000_01b3;

pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = StateHasher::new();
    hash.write(bytes);
    hash.finish()
}

pub(crate) trait StateSink {
    fn write(&mut self, bytes: &[u8]);

    fn write_u8(&mut self, value: u8) {
        self.write(&[value]);
    }

    fn write_u16(&mut self, value: u16) {
        self.write(&value.to_le_bytes());
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    fn write_optional_u8(&mut self, value: Option<u8>) {
        match value {
            None => self.write_u8(0),
            Some(value) => {
                self.write_u8(1);
                self.write_u8(value);
            }
        }
    }
}

impl StateSink for Vec<u8> {
    fn write(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

pub(crate) struct StateHasher(u64);

impl StateHasher {
    pub(crate) const fn new() -> Self {
        Self(FNV1A64_OFFSET)
    }

    pub(crate) const fn finish(self) -> u64 {
        self.0
    }
}

impl StateSink for StateHasher {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(FNV1A64_PRIME);
        }
    }
}

pub(crate) struct StateReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> StateReader<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(crate) fn read_bytes(&mut self, length: usize) -> Result<&'a [u8], StateError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(StateError::InvalidPayload("field length overflow"))?;
        let Some(bytes) = self.bytes.get(self.offset..end) else {
            return Err(StateError::Truncated {
                needed: end,
                actual: self.bytes.len(),
            });
        };
        self.offset = end;
        Ok(bytes)
    }

    pub(crate) fn read_array<const N: usize>(&mut self) -> Result<[u8; N], StateError> {
        let mut result = [0; N];
        result.copy_from_slice(self.read_bytes(N)?);
        Ok(result)
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, StateError> {
        Ok(self.read_bytes(1)?[0])
    }

    pub(crate) fn read_u16(&mut self) -> Result<u16, StateError> {
        Ok(u16::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_u32(&mut self) -> Result<u32, StateError> {
        Ok(u32::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_u64(&mut self) -> Result<u64, StateError> {
        Ok(u64::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_bool(&mut self) -> Result<bool, StateError> {
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(StateError::InvalidPayload(
                "boolean field is not zero or one",
            )),
        }
    }

    pub(crate) fn read_optional_u8(&mut self) -> Result<Option<u8>, StateError> {
        match self.read_u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.read_u8()?)),
            _ => Err(StateError::InvalidPayload(
                "optional field has an invalid tag",
            )),
        }
    }

    pub(crate) const fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    pub(crate) fn finish(self) -> Result<(), StateError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(StateError::TrailingPayload {
                remaining: self.remaining(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_codec_is_little_endian_and_strict() {
        let mut bytes = Vec::new();
        bytes.write_u8(0x12);
        bytes.write_u16(0x3456);
        bytes.write_u32(0x789a_bcde);
        bytes.write_u64(0x0123_4567_89ab_cdef);
        bytes.write_bool(true);
        bytes.write_optional_u8(Some(0xa5));

        let mut reader = StateReader::new(&bytes);
        assert_eq!(reader.read_u8().unwrap(), 0x12);
        assert_eq!(reader.read_u16().unwrap(), 0x3456);
        assert_eq!(reader.read_u32().unwrap(), 0x789a_bcde);
        assert_eq!(reader.read_u64().unwrap(), 0x0123_4567_89ab_cdef);
        assert!(reader.read_bool().unwrap());
        assert_eq!(reader.read_optional_u8().unwrap(), Some(0xa5));
        reader.finish().unwrap();
    }

    #[test]
    fn reader_rejects_truncation_invalid_tags_and_trailing_bytes() {
        assert!(matches!(
            StateReader::new(&[1]).read_u16(),
            Err(StateError::Truncated { .. })
        ));
        assert!(matches!(
            StateReader::new(&[2]).read_bool(),
            Err(StateError::InvalidPayload(_))
        ));
        assert!(matches!(
            StateReader::new(&[0]).finish(),
            Err(StateError::TrailingPayload { remaining: 1 })
        ));
    }
}
