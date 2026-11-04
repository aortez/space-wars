//! Native manual installation runs on the dedicated install worker. Its stack
//! retains every lock/lease through admission, writes and the reboot handoff.
use super::service::{Admission, Backend, host_exchange};
use super::*;
use crate::update_runtime::{Handle, InstallCandidate};
use engine_ota::{
    RecoveryStore, boot_linux::BootController, install_lock::InstallLock, native_install,
};
use spacewars_control::install::{transport, wire::*};
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) struct Native {
    boot_id: String,
    updates: Handle,
}
impl Native {
    pub fn new(boot_id: String, updates: Handle) -> Self {
        Self { boot_id, updates }
    }

    fn context_for(&self, selected: &InstallCandidate) -> Result<Context, Failure> {
        let observed_ms = transport::boot_ms().map_err(|_| Failure::NotReady)?;
        let nonce = observed_ms.max(1);
        let (reply, peer) = host_exchange(
            &HostRequest::Observe {
                schema: SCHEMA,
                nonce,
            },
            Duration::from_secs(1),
        )?;
        let view = reply.view.ok_or(Failure::NotReady)?;
        if view.nonce != nonce || view.host != peer || !view.available {
            return Err(Failure::NotReady);
        }
        Ok(Context {
            boot_id: self.boot_id.clone(),
            ready: Some(
                Ready::from_verified(&selected.candidate, selected.generation)
                    .map_err(Failure::from)?,
            ),
            maintenance_clear: true,
            host: Some(HostObservation {
                instance: peer,
                revision: view.revision,
                settings_revision: view.settings_revision,
                game: view.game,
                fenced: view.fenced,
                observed_ms,
            }),
        })
    }

    fn run(&self, prepare: &Prepare, admission: &Admission) -> crate::Result<()> {
        let expected = self.updates.install_candidate()?;
        if expected.candidate.id().to_string() != prepare.ticket.candidate.as_str() {
            return Err(invalid("candidate changed before admission"));
        }
        let selected = self.updates.claim_install(&expected)?;
        let lock = InstallLock::acquire()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let mut time = || self.updates.install_time();
        let mut session = runtime.block_on(native_install::Session::open(
            &lock,
            &expected.candidate,
            &mut time,
        ))?;
        admission.pending().map_err(failed)?;
        let mut journal = journal::Journal::open_native()?;
        let token = crate::network_policy::HoldToken::generate()?;
        journal.prepare_with_hold(
            &lock,
            prepare,
            &self.boot_id,
            expected.generation,
            session.recovery(),
            token.clone(),
        )?;
        // Persist the token before sending admission: a lost response can leave
        // a durable hold, which startup renewal/cleanup must still identify.
        super::hold::admit(&token)?;
        admission.pending().map_err(failed)?;
        let (ack, peer) = self.prepare(prepare).map_err(failed)?;
        let plan = runtime.block_on(session.plan(&mut time))?;
        super::hold::renew(&token)?;
        selected.check()?;
        let context = self.context_for(&expected).map_err(failed)?;
        let authorization = admission.consume(&ack, &peer, &context).map_err(failed)?;
        journal.begin(&lock, session.recovery_mut(), authorization, plan)?;
        let mut last_renewal = Instant::now();
        let hold_failed = std::cell::Cell::new(false);
        runtime.block_on(session.write_and_reboot(
            &mut time,
            |progress| {
                use engine_ota::install_linux::WriteStage;
                admission.phase(match progress.stage {
                    WriteStage::CheckingPayload => Phase::CheckingPayload,
                    WriteStage::Writing => Phase::Writing,
                    WriteStage::Flushing => Phase::Flushing,
                    WriteStage::ReadingBack => Phase::ReadingBack,
                    WriteStage::CheckingFilesystem => Phase::CheckingFilesystem,
                    WriteStage::Prepared => Phase::CheckingFilesystem,
                });
                if last_renewal.elapsed() >= Duration::from_secs(30) {
                    last_renewal = Instant::now();
                    if super::hold::renew(&token).is_err() {
                        hold_failed.set(true);
                    }
                }
            },
            |permit| {
                // Failure after writing never turns into a second write/reboot.
                if hold_failed.get()
                    || selected.check().is_err()
                    || super::hold::renew(&token).is_err()
                {
                    return Err(engine_ota::Error::Install(
                        "installation maintenance admission changed".into(),
                    ));
                }
                crate::bootstrap_activation::request_trial(permit)
            },
        ))?;
        admission.phase(Phase::RebootRequested);
        // Keep the lock until PID 1 stops this service for reboot. Otherwise a
        // concurrent reconciliation could mistake a queued reboot for a crash.
        while !admission.stopped() {
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
}

impl Backend for Native {
    fn context(&self) -> Result<Context, Failure> {
        let selected = self
            .updates
            .install_candidate()
            .map_err(|_| Failure::NotReady)?;
        let result = (|| -> crate::Result<()> {
            let lock = InstallLock::acquire()?;
            let boot = BootController::open(&lock)?;
            let recovery =
                RecoveryStore::open(Path::new(crate::recovery_service::RECOVERY_DIRECTORY))?;
            journal::Journal::open_native()?.require_clear(&lock, &recovery)?;
            boot.plan_install(&recovery, &selected.candidate)?;
            Ok(())
        })();
        result.map_err(|_| Failure::Unavailable)?;
        self.context_for(&selected)
    }
    fn prepare(&self, prepare: &Prepare) -> Result<(Acknowledgement, HostInstance), Failure> {
        let (reply, peer) = host_exchange(
            &HostRequest::Prepare {
                schema: SCHEMA,
                prepare: prepare.clone(),
            },
            Duration::from_secs(31),
        )?;
        if let Some(error) = reply.error {
            return Err(error);
        }
        Ok((reply.acknowledgement.ok_or(Failure::Invalid)?, peer))
    }
    fn abort(&self, aborted: &Aborted) {
        let _ = host_exchange(
            &HostRequest::Abort {
                schema: SCHEMA,
                aborted: aborted.clone(),
            },
            Duration::from_secs(2),
        );
    }
    fn accept(&self, _: &Admission, _: &Acknowledgement, _: &HostInstance) -> Result<(), Failure> {
        Err(Failure::Unavailable)
    }
    fn execute(&self, prepare: &Prepare, admission: &Admission) -> Result<(), Failure> {
        self.run(prepare, admission).map_err(|error| {
            tracing::warn!(%error, "manual installation stopped; durable outcome requires reconciliation");
            Failure::NotReady
        })
    }
    fn settled(&self) -> bool {
        true
    } // context proved both journals clear under InstallLock.
}
impl crate::bootstrap_activation::RequestPermit for native_install::TrialPermit<'_, '_> {
    fn begin(&mut self) -> engine_ota::Result<()> {
        self.begin()
    }
    fn authorize(&mut self) -> engine_ota::Result<()> {
        self.authorize()
    }
    fn queued(&mut self) -> engine_ota::Result<()> {
        self.queued()
    }
    fn fail(&mut self) -> engine_ota::Result<()> {
        self.fail()
    }
}
fn invalid(message: &str) -> crate::Error {
    crate::Error::Invalid(message.into())
}
fn failed(error: Failure) -> crate::Error {
    invalid(&format!("installation handoff refused: {error:?}"))
}
