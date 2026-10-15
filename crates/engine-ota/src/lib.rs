//! Shared verification and eligibility rules for device-managed rootfs updates.
//!
//! Linux boot-selector changes require the shared installation lock and a
//! durable recovery journal. The inactive-root writer requires that same live
//! authority; reboot belongs to the device service. A verified candidate still
//! needs revalidation at installation.

#[cfg(target_os = "linux")]
pub mod access_linux;
pub mod boot;
#[cfg(target_os = "linux")]
mod boot_files;
#[cfg(target_os = "linux")]
pub mod boot_linux;
#[cfg(target_os = "linux")]
pub mod bootstrap;
mod contract;
#[cfg(target_os = "linux")]
pub mod coordinator;
#[cfg(target_os = "linux")]
mod file_command;
mod image_identity;
#[cfg(target_os = "linux")]
mod image_tools;
#[cfg(target_os = "linux")]
pub mod install_linux;
#[cfg(target_os = "linux")]
pub mod install_lock;
#[cfg(target_os = "linux")]
mod owned_fs;
#[cfg(target_os = "linux")]
mod watchdog_linux;
#[cfg(target_os = "linux")]
pub use watchdog_linux::{EarlyWatchdog, WatchdogCompletion};
pub mod recovery;
#[cfg(target_os = "linux")]
pub mod recovery_linux;
mod recovery_store;
mod repository;
mod store;

pub use contract::*;
pub use image_identity::*;
pub use recovery_store::RecoveryStore;
pub use repository::*;
pub use store::TrustStore;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Recovery(#[from] recovery::RecoveryError),
    #[error("waiting for usable time")]
    WaitingForTime,
    #[error("usable time moved backwards behind the retained trusted-time floor")]
    TimeMovedBackwards,
    #[error("metadata check exceeded its time limit")]
    Timeout,
    #[error("invalid image descriptor: {0}")]
    Descriptor(String),
    #[error("invalid image identity: {0}")]
    ImageIdentity(String),
    #[error("invalid boot evidence: {0}")]
    Boot(String),
    #[error("persistent SSH access: {0}")]
    Access(String),
    #[error("image installation failed: {0}")]
    Install(String),
    #[error("unsupported feed: {0}")]
    Feed(String),
    #[error("invalid or uninitialized trust store: {0}")]
    Store(String),
    #[error("another update or maintenance operation is already in progress")]
    Busy,
    #[error("no retained verified feed; refresh required")]
    RefreshRequired,
    #[error("candidate changed or is no longer eligible")]
    CandidateChanged,
    #[error("image length or SHA-256 does not match signed metadata")]
    ImageMismatch,
    #[error("TUF verification failed: {0}")]
    Tuf(#[source] Box<tough::error::Error>),
    #[error("storage failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<tough::error::Error> for Error {
    fn from(error: tough::error::Error) -> Self {
        Self::Tuf(Box::new(error))
    }
}
