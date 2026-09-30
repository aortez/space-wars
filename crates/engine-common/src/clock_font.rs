//! Stable names and saved choices for the Clock's bounded font catalog.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[repr(u8)]
pub enum ClockFont {
    #[default]
    Classic,
    Matrix,
    Sans,
    Serif,
}

impl ClockFont {
    pub const ALL: [Self; 4] = [Self::Classic, Self::Matrix, Self::Sans, Self::Serif];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::Matrix => "Matrix",
            Self::Sans => "Sans",
            Self::Serif => "Serif",
        }
    }
    pub const fn source(self) -> &'static str {
        match self {
            Self::Classic => "Seven segment",
            Self::Matrix => "Pixel matrix",
            Self::Sans => "DejaVu Sans",
            Self::Serif => "DejaVu Serif",
        }
    }
}

/// Nonempty subset, serialized as readable font names rather than bit flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<ClockFont>", into = "Vec<ClockFont>")]
pub struct ClockFontPool(u8);

impl Default for ClockFontPool {
    fn default() -> Self {
        Self(15)
    }
}

impl ClockFontPool {
    pub const fn bits(self) -> u8 {
        self.0
    }
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits == 0 || bits & !15 != 0 {
            None
        } else {
            Some(Self(bits))
        }
    }
    pub const fn contains(self, font: ClockFont) -> bool {
        self.0 & (1 << font as u8) != 0
    }
    /// Refuse to remove the final font; cycling always has a valid choice.
    pub fn toggle(&mut self, font: ClockFont) {
        if let Some(next) = Self::from_bits(self.0 ^ (1 << font as u8)) {
            *self = next;
        }
    }
    pub fn include(&mut self, font: ClockFont) {
        self.0 |= 1 << font as u8;
    }
    pub fn fonts(self) -> impl Iterator<Item = ClockFont> {
        ClockFont::ALL
            .into_iter()
            .filter(move |font| self.contains(*font))
    }
}

impl TryFrom<Vec<ClockFont>> for ClockFontPool {
    type Error = &'static str;
    fn try_from(fonts: Vec<ClockFont>) -> Result<Self, Self::Error> {
        Self::from_bits(
            fonts
                .into_iter()
                .fold(0, |bits, font| bits | (1 << font as u8)),
        )
        .ok_or("Choose at least one Clock font")
    }
}
impl From<ClockFontPool> for Vec<ClockFont> {
    fn from(pool: ClockFontPool) -> Self {
        pool.fonts().collect()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockFontSettings {
    pub selected: ClockFont,
    /// Choose another enabled font each minute, after any active effect ends.
    pub rotate: bool,
    pub pool: ClockFontPool,
}
