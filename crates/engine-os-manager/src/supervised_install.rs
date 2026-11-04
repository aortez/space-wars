//! Native owner-only adapter for a selected signed trial, excluded from images.
use crate::bootstrap_activation::request_trial;
use engine_ota::{Result, supervised_install::TrialPermit};

pub fn request_reboot(permit: &mut TrialPermit<'_, '_>) -> Result<()> {
    request_trial(permit)
}
