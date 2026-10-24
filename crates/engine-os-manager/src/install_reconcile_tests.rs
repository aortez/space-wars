use super::*;
use spacewars_control::install::{Aborted, HostInstance, Ticket};
use std::sync::atomic::{AtomicUsize, Ordering};

fn id(n: u8) -> Id {
    Id::from_bytes([n; 32]).unwrap()
}
fn ticket() -> Ticket {
    Ticket {
        service: id(1),
        sequence: 1,
        candidate: id(2),
    }
}
fn request() -> ReconcileRequest {
    ReconcileRequest {
        schema: SCHEMA,
        nonce: 42,
        host: HostInstance {
            pid: std::process::id(),
            start_ticks: 12,
            invocation: "a".repeat(32),
        },
        held: None,
    }
}
fn uid() -> u32 {
    unsafe { libc::geteuid() }
}

#[test]
fn native_decision_mapping_never_clears_an_unknown_held_ticket() {
    let held = ticket();
    assert_eq!(
        decision(&Resolution::Clear, Some(&held)),
        RecoveryDecision::Blocked {}
    );
    assert_eq!(
        decision(&Resolution::Clear, None),
        RecoveryDecision::Clear {}
    );
    let resolution = Resolution::Aborted(Aborted {
        schema: SCHEMA,
        ticket: held.clone(),
    });
    let mut wrong = held.clone();
    wrong.sequence += 1;
    assert_eq!(
        decision(&resolution, Some(&wrong)),
        RecoveryDecision::Blocked {}
    );
    assert!(matches!(
        decision(&resolution, Some(&held)),
        RecoveryDecision::Aborted { .. }
    ));
    for outcome in [Outcome::Committed, Outcome::RolledBack] {
        let resolution = Resolution::Finished {
            ticket: held.clone(),
            transaction: engine_ota::Sha256::digest(b"tx"),
            outcome,
        };
        assert_eq!(
            decision(&resolution, Some(&wrong)),
            RecoveryDecision::Blocked {}
        );
        assert!(matches!(
            decision(&resolution, None),
            RecoveryDecision::Finished { .. }
        ));
    }
}

#[test]
fn socket_correlates_nonce_host_and_epoch_and_rejects_a_claimed_other_pid() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reconcile.sock");
    let calls = Arc::new(AtomicUsize::new(0));
    let invoked = calls.clone();
    let _runtime = Runtime::bind(
        &path,
        uid(),
        id(3),
        Arc::new(move |_| {
            invoked.fetch_add(1, Ordering::SeqCst);
            Ok(RecoveryDecision::Clear {})
        }),
    )
    .unwrap();
    let request = request();
    let reply: ReconcileReply = transport::exchange(
        &path,
        uid(),
        Some(std::process::id()),
        &request,
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(reply.nonce, request.nonce);
    assert_eq!(reply.host, request.host);
    assert_eq!(reply.service, id(3));
    assert_eq!(reply.result, RecoveryDecision::Clear {});
    let mut wrong = request.clone();
    wrong.host.pid += 1;
    let reply: ReconcileReply =
        transport::exchange(&path, uid(), None, &wrong, Duration::from_secs(2)).unwrap();
    assert_eq!(reply.result, RecoveryDecision::Blocked {});
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        transport::exchange::<_, ReconcileReply>(
            &path,
            uid().wrapping_add(1),
            None,
            &request,
            Duration::from_secs(1)
        )
        .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn storage_failure_blocks_and_install_lock_contention_waits_without_clear() {
    for busy in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reconcile.sock");
        let _runtime = Runtime::bind(
            &path,
            uid(),
            id(3),
            Arc::new(move |_| {
                Err(if busy {
                    engine_ota::Error::Busy.into()
                } else {
                    invalid("damaged journal")
                })
            }),
        )
        .unwrap();
        let reply: ReconcileReply =
            transport::exchange(&path, uid(), None, &request(), Duration::from_secs(2)).unwrap();
        assert_eq!(
            reply.result,
            if busy {
                RecoveryDecision::Waiting {}
            } else {
                RecoveryDecision::Blocked {}
            }
        );
    }
}
