//! Device-owned state and a bounded local status interface, independent of the UI.
#[cfg(target_os = "linux")]
pub mod bootstrap_activation;
#[cfg(target_os = "linux")]
pub mod bootstrap_canonical;
#[cfg(target_os = "linux")]
mod bootstrap_recovery;
#[cfg(all(target_os = "linux", feature = "recovery-qualification"))]
pub mod supervised_install;
#[cfg(all(target_os = "linux", feature = "recovery-qualification"))]
pub mod supervised_kiosk_crash;
#[cfg(all(target_os = "linux", feature = "recovery-qualification"))]
pub mod supervised_kiosk_hang;
#[cfg(all(target_os = "linux", feature = "recovery-qualification"))]
pub mod supervised_trial_reset;
#[cfg(all(test, target_os = "linux"))]
#[allow(dead_code)]
#[path = "../../engine-ota/tests/support/mod.rs"]
mod test_feed;
pub mod time;
#[cfg(target_os = "linux")]
pub mod time_query;
#[cfg(target_os = "linux")]
pub mod time_runtime;
#[cfg(target_os = "linux")]
pub mod time_schedule;

#[cfg(target_os = "linux")]
pub mod install_handoff;
#[cfg(target_os = "linux")]
pub mod kiosk_health;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub mod maintenance;
#[cfg(target_os = "linux")]
mod network_access;
#[cfg(target_os = "linux")]
pub mod network_controller;
#[cfg(target_os = "linux")]
pub mod network_deployment;
#[cfg(target_os = "linux")]
pub mod network_monitor;
#[cfg(target_os = "linux")]
pub mod network_observation;
#[cfg(target_os = "linux")]
pub mod network_policy;
#[cfg(target_os = "linux")]
pub mod network_radio;
#[cfg(target_os = "linux")]
mod network_radio_state;
#[cfg(target_os = "linux")]
pub mod network_runtime;
#[cfg(target_os = "linux")]
pub mod network_service;
#[cfg(target_os = "linux")]
pub mod network_sessions;
#[cfg(target_os = "linux")]
pub mod network_ssh;
#[cfg(target_os = "linux")]
pub mod network_watch;
#[cfg(target_os = "linux")]
pub mod recovery_service;
#[cfg(target_os = "linux")]
pub mod recovery_status;
#[cfg(target_os = "linux")]
mod schedule_store;
#[cfg(target_os = "linux")]
pub mod service;
#[cfg(target_os = "linux")]
pub mod store;
#[cfg(target_os = "linux")]
pub mod update_runtime;
#[cfg(target_os = "linux")]
pub mod update_schedule;

pub const SOCKET_PATH: &str = "/run/spacewars-system/status.sock";
pub const STATE_DIRECTORY: &str = "/data/spacewars-system/time";
pub const RTC_BOOT_PATH: &str = "/run/spacewars-rtc/boot.json";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    Ota(#[from] engine_ota::Error),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
