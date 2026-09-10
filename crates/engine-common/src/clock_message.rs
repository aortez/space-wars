//! Validated, fixed-capacity text shared by Clock settings, actions and controls.

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

pub const MAX_CLOCK_MESSAGE_BYTES: usize = 32;

/// ASCII bitmap-font text. Lowercase is normalized to uppercase at the boundary;
/// spaces are preserved, but an empty or all-space message is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ClockMarqueeMessage {
    bytes: [u8; MAX_CLOCK_MESSAGE_BYTES],
    len: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockMessageError {
    Empty,
    TooLong,
    UnsupportedCharacter,
}

impl fmt::Display for ClockMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "Clock message must contain at least one non-space character",
            Self::TooLong => "Clock message must be at most 32 ASCII characters",
            Self::UnsupportedCharacter => {
                "Clock message supports only ASCII letters, digits, spaces and . , : - ! ? / '"
            }
        })
    }
}

impl std::error::Error for ClockMessageError {}

impl ClockMarqueeMessage {
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("Clock messages contain validated ASCII only")
    }
}

impl FromStr for ClockMarqueeMessage {
    type Err = ClockMessageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() > MAX_CLOCK_MESSAGE_BYTES {
            return Err(ClockMessageError::TooLong);
        }
        if value.is_empty() || value.bytes().all(|b| b == b' ') {
            return Err(ClockMessageError::Empty);
        }
        if !value.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b' ' | b'.' | b',' | b':' | b'-' | b'!' | b'?' | b'/' | b'\''
                )
        }) {
            return Err(ClockMessageError::UnsupportedCharacter);
        }
        let mut bytes = [0; MAX_CLOCK_MESSAGE_BYTES];
        for (out, byte) in bytes.iter_mut().zip(value.bytes()) {
            *out = byte.to_ascii_uppercase();
        }
        Ok(Self {
            bytes,
            len: value.len() as u8,
        })
    }
}

impl TryFrom<String> for ClockMarqueeMessage {
    type Error = ClockMessageError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<ClockMarqueeMessage> for String {
    fn from(value: ClockMarqueeMessage) -> Self {
        value.as_str().into()
    }
}

impl Default for ClockMarqueeMessage {
    fn default() -> Self {
        "SPACE WARS"
            .parse()
            .expect("default Clock message is valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_bounds_and_alphabet_without_truncation() {
        for text in [
            "A",
            "Hi, it's 12:34!",
            "  hi  ",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
            "0123456789 .,:-!?/'",
        ] {
            let message: ClockMarqueeMessage = text.parse().unwrap();
            assert_eq!(message.as_str(), text.to_ascii_uppercase());
        }
        assert!(
            "8".repeat(MAX_CLOCK_MESSAGE_BYTES)
                .parse::<ClockMarqueeMessage>()
                .is_ok()
        );
        for (text, expected) in [
            ("".into(), ClockMessageError::Empty),
            ("   ".into(), ClockMessageError::Empty),
            (
                "A".repeat(MAX_CLOCK_MESSAGE_BYTES + 1),
                ClockMessageError::TooLong,
            ),
            ("A\nB".into(), ClockMessageError::UnsupportedCharacter),
            ("A\0B".into(), ClockMessageError::UnsupportedCharacter),
            ("é".into(), ClockMessageError::UnsupportedCharacter),
            ("A_B".into(), ClockMessageError::UnsupportedCharacter),
        ] {
            assert_eq!(text.parse::<ClockMarqueeMessage>(), Err(expected));
        }
    }
}
