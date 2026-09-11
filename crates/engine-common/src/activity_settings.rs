//! Persistent match and unattended-activity preferences.

use serde::{Deserialize, Deserializer, Serialize, de::IgnoredAny};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchSettings {
    /// Zero permits an unlimited match. Positive values are gameplay seconds.
    #[serde(deserialize_with = "match_seconds")]
    pub time_limit_seconds: u32,
}

impl Default for MatchSettings {
    fn default() -> Self {
        Self {
            time_limit_seconds: 600,
        }
    }
}

impl MatchSettings {
    pub fn normalized(self) -> Self {
        Self {
            time_limit_seconds: self.time_limit_seconds.min(3600),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutostartSettings {
    pub enabled: bool,
    /// Stable catalog ID; retain unknown IDs so unavailable activities are visible.
    pub activity: String,
    #[serde(deserialize_with = "idle_seconds")]
    pub delay_seconds: u32,
}

impl Default for AutostartSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            activity: "clock".into(),
            delay_seconds: 30,
        }
    }
}

impl AutostartSettings {
    pub fn normalized(&self) -> Self {
        Self {
            delay_seconds: self.delay_seconds.clamp(5, 600),
            ..self.clone()
        }
    }
}

// Invalid individual timer fields must not discard unrelated saved preferences.
#[derive(Deserialize)]
#[serde(untagged)]
enum Seconds {
    Valid(u32),
    Invalid(IgnoredAny),
}

fn match_seconds<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    Ok(match Seconds::deserialize(d)? {
        Seconds::Valid(n) => n.min(3600),
        Seconds::Invalid(_) => MatchSettings::default().time_limit_seconds,
    })
}

fn idle_seconds<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    Ok(match Seconds::deserialize(d)? {
        Seconds::Valid(n) => n.clamp(5, 600),
        Seconds::Invalid(_) => AutostartSettings::default().delay_seconds,
    })
}
