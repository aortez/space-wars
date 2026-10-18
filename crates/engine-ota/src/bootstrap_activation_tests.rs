use super::*;
use crate::bootstrap::TrialFailure;
use crate::bootstrap::trial::{TrialAction, TrialRecord};
use crate::bootstrap::trial_store::tests::Fixture;
use std::cell::Cell;

const REQUEST_BOOT: &str = "cc606400-dc75-446e-b26b-385592b0f562";

fn ready(fixture: &Fixture) -> TrialStore {
    let mut store = fixture.open();
    store
        .advance(|record| {
            *record = TrialRecord::staging(fixture.inputs.clone())?;
            record.phase = TrialPhase::TrialReady;
            Ok(())
        })
        .unwrap();
    store
}

#[test]
fn activation_persists_request_before_authority_and_never_replays_after_reopen() {
    let fixture = Fixture::new();
    let mut store = ready(&fixture);
    let calls = Cell::new(0);
    let report = run(
        &mut store,
        REQUEST_BOOT,
        &mut || {
            calls.set(calls.get() + 1);
            Ok(())
        },
        |permit| {
            assert!(permit.authorize_reboot().is_err());
            assert!(permit.reboot_queued().is_err());
            permit.begin()?;
            assert_eq!(permit.store.durable()?.phase, TrialPhase::TrialRequested);
            assert!(permit.begin().is_err());
            permit.authorize_reboot()?;
            permit.reboot_queued()?;
            assert!(permit.authorize_reboot().is_err());
            assert!(permit.reboot_queued().is_err());
            Ok(())
        },
    )
    .unwrap();
    assert!(report.reboot_job_accepted);
    assert!(!report.automatic_updates_enabled);
    assert_eq!(report.request_boot_id, REQUEST_BOOT);
    assert_eq!(calls.get(), 2);
    drop(store);
    let mut reopened = fixture.open();
    assert!(
        run(&mut reopened, REQUEST_BOOT, &mut || Ok(()), |_| panic!(
            "replayed request"
        ))
        .is_err()
    );
    let mut boot = fixture.boot();
    boot.boot_id = REQUEST_BOOT.into();
    assert!(reopened.advance(|record| record.reconcile(&boot)).is_err());
    boot.boot_id.clone_from(&fixture.inputs.origin_boot_id);
    assert!(reopened.advance(|record| record.reconcile(&boot)).is_err());
    assert_eq!(
        reopened
            .advance(|record| record.reconcile(&fixture.boot()))
            .unwrap(),
        TrialAction::Observe
    );
}

#[test]
fn activation_guards_and_failures_cannot_restore_a_consumed_attempt() {
    for case in [
        "initial-guard",
        "final-guard",
        "reply-lost",
        "no-request",
        "no-job",
    ] {
        let fixture = Fixture::new();
        let mut store = ready(&fixture);
        let calls = Cell::new(0);
        let result = run(
            &mut store,
            REQUEST_BOOT,
            &mut || {
                calls.set(calls.get() + 1);
                if (case == "initial-guard" && calls.get() == 1)
                    || (case == "final-guard" && calls.get() == 2)
                {
                    Err(invalid("device changed"))
                } else {
                    Ok(())
                }
            },
            |permit| {
                if case == "no-request" {
                    return Ok(());
                }
                permit.begin()?;
                permit.authorize_reboot()?;
                if case == "reply-lost" {
                    return Err(invalid("lost job reply"));
                }
                Ok(())
            },
        );
        assert!(result.is_err(), "{case}");
        drop(store);
        let mut store = fixture.open();
        if matches!(case, "initial-guard" | "no-request") {
            assert_eq!(store.phase(), TrialPhase::TrialReady);
        } else {
            assert_eq!(store.failure(), Some(TrialFailure::ActivationRequestFailed));
            assert!(
                run(&mut store, REQUEST_BOOT, &mut || Ok(()), |_| panic!(
                    "retried"
                ))
                .is_err()
            );
            assert_eq!(
                store
                    .advance(|record| record.reconcile(&fixture.boot()))
                    .unwrap(),
                TrialAction::RebootLegacy
            );
        }
    }
}

#[test]
fn interrupted_request_flush_never_releases_reboot_authority() {
    for point in ["write", "file-sync", "rename", "directory-sync"] {
        let fixture = Fixture::new();
        let mut store = ready(&fixture);
        store.fault = Some(point);
        let authorized = Cell::new(false);
        assert!(
            run(&mut store, REQUEST_BOOT, &mut || Ok(()), |permit| {
                permit.begin()?;
                permit.authorize_reboot()?;
                authorized.set(true);
                permit.reboot_queued()
            })
            .is_err()
        );
        assert!(!authorized.get(), "{point}");
        assert!(store.durable().is_err());
        drop(store);
        let mut store = fixture.open();
        if matches!(point, "rename" | "directory-sync") {
            assert_eq!(store.phase(), TrialPhase::TrialRequested);
            assert!(
                run(&mut store, REQUEST_BOOT, &mut || Ok(()), |_| panic!(
                    "retried"
                ))
                .is_err()
            );
        } else {
            assert_eq!(store.phase(), TrialPhase::TrialReady);
        }
    }
}

#[test]
fn process_loss_after_consumption_leaves_an_attempt_that_cannot_be_resent() {
    let fixture = Fixture::new();
    let mut store = ready(&fixture);
    let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = run(&mut store, REQUEST_BOOT, &mut || Ok(()), |permit| {
            permit.begin()?;
            panic!("simulated process loss before job reply")
        });
    }));
    assert!(interrupted.is_err());
    drop(store);
    let mut store = fixture.open();
    assert_eq!(store.phase(), TrialPhase::TrialRequested);
    assert!(
        run(&mut store, REQUEST_BOOT, &mut || Ok(()), |_| panic!(
            "retried"
        ))
        .is_err()
    );
}

#[test]
fn old_or_malformed_request_boot_evidence_cannot_authorize_a_trial() {
    let fixture = Fixture::new();
    let store = fixture.open();
    for case in [
        "old-schema",
        "missing-request",
        "invalid-request",
        "unrequested-ready",
    ] {
        let mut value = serde_json::to_value(store.durable().unwrap()).unwrap();
        match case {
            "old-schema" => value["schema"] = 1.into(),
            "missing-request" => value["request_boot_id"] = serde_json::Value::Null,
            "invalid-request" => value["request_boot_id"] = "not-a-boot".into(),
            "unrequested-ready" => value["phase"] = "trial-ready".into(),
            _ => unreachable!(),
        }
        let record: TrialRecord = serde_json::from_value(value).unwrap();
        assert!(record.validate().is_err(), "{case}");
    }
}
