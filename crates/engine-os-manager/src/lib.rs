//! Device-owned state and a bounded local status interface, independent of the UI.
pub mod time;

#[cfg(target_os = "linux")]
pub mod kiosk_health;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub mod service;
#[cfg(target_os = "linux")]
pub mod store;

pub const SOCKET_PATH: &str = "/run/spacewars-system/status.sock";
pub const STATE_DIRECTORY: &str = "/data/spacewars-system/time";
pub const RTC_BOOT_PATH: &str = "/run/spacewars-rtc/boot.json";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
