//! Native owner-only adapter for a selected signed trial, excluded from images.
use crate::bootstrap_activation::{RequestPermit, request_trial};
use engine_ota::{Result, supervised_install::TrialPermit};

impl RequestPermit for TrialPermit<'_, '_> {
    fn begin(&mut self) -> Result<()> {
        self.begin()
    }
    fn authorize(&mut self) -> Result<()> {
        self.authorize()
    }
    fn queued(&mut self) -> Result<()> {
        self.queued()
    }
    fn fail(&mut self) -> Result<()> {
        self.fail()
    }
}

pub fn request_reboot(permit: &mut TrialPermit<'_, '_>) -> Result<()> {
    request_trial(permit)
}
