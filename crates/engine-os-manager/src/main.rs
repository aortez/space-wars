//! Spacewars os-manager: Pi-only privileged helper.
//!
//! Handles runtime WiFi config, A/B update application, power management, and
//! other operations that require root. Game processes ask via a Unix domain
//! socket rather than calling `sudo` directly.
//!
//! Stub for M3. Feature-gating moves in once the Pi image build exists.

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("engine-os-manager starting (stub).");
}
