use super::*;

fn request() -> Prepare {
    Prepare {
        schema: SCHEMA,
        ticket: Ticket {
            service: Id::from_bytes([1; 32]).unwrap(),
            sequence: 1,
            candidate: Id::from_bytes([2; 32]).unwrap(),
        },
        host: HostInstance {
            pid: 42,
            start_ticks: 100,
            invocation: "1".repeat(32),
        },
        host_revision: 10,
        settings_revision: 20,
        expires_ms: 30_100,
    }
}

fn begin(gate: &mut HostGate, request: &Prepare) {
    gate.prepare(request.clone(), &request.host, 10, 20, 100)
        .unwrap();
    assert!(gate.fenced());
}

#[test]
fn recovery_requires_the_held_ticket_and_rebinds_only_after_resolution() {
    let old = request();
    let mut gate = HostGate::default();
    begin(&mut gate, &old);
    gate.saved(&old.ticket, 20, true).unwrap();
    gate.presented(&old.ticket).unwrap();
    gate.acknowledge(&old.ticket, 101).unwrap();
    let next = Id::from_bytes([9; 32]).unwrap();
    assert_eq!(gate.reconciled(&next, None), Err(Error::Stale));
    let mut wrong = old.ticket.clone();
    wrong.sequence += 1;
    assert_eq!(gate.reconciled(&next, Some(&wrong)), Err(Error::Stale));
    gate.expire(u64::MAX);
    assert!(gate.fenced());
    gate.reconciled(&next, Some(&old.ticket)).unwrap();
    assert!(!gate.fenced());
    assert_eq!(
        gate.prepare(old.clone(), &old.host, 10, 20, 102),
        Err(Error::Stale)
    );
    let mut new = old.clone();
    new.ticket.service = next.clone();
    begin(&mut gate, &new);
    gate.reconciled(&next, Some(&new.ticket)).unwrap();
    gate.reconciled(&next, None).unwrap();
    assert_eq!(
        gate.prepare(new.clone(), &new.host, 10, 20, 102),
        Err(Error::Stale)
    );
}

#[test]
fn recovery_wire_rejects_ambiguous_or_uncorrelated_shapes() {
    use super::wire::*;
    let request = ReconcileRequest {
        schema: SCHEMA,
        nonce: 1,
        host: request().host,
        held: None,
    };
    let value = serde_json::to_value(&request).unwrap();
    for (field, replacement) in [
        ("nonce", serde_json::json!(0)),
        ("schema", serde_json::json!(2)),
        ("path", serde_json::json!("/data/other")),
    ] {
        let mut bad = value.clone();
        bad[field] = replacement;
        assert!(ReconcileRequest::from_json(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    let reply = ReconcileReply {
        schema: SCHEMA,
        nonce: 1,
        host: request.host,
        service: Id::from_bytes([3; 32]).unwrap(),
        result: RecoveryDecision::Clear {},
    };
    let mut bad = serde_json::to_value(&reply).unwrap();
    bad["result"]["ticket"] = serde_json::json!(null);
    assert!(ReconcileReply::from_json(&serde_json::to_vec(&bad).unwrap()).is_err());
    assert!(ReconcileReply::from_json(&vec![b' '; MAX_MESSAGE_BYTES + 1]).is_err());
}

#[test]
fn protocol_rejects_extra_fields_versions_bad_identity_and_oversized_messages() {
    let request = request();
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(
        Prepare::from_json(&serde_json::to_vec(&value).unwrap()).unwrap(),
        request
    );
    for change in [
        |v: &mut serde_json::Value| {
            v["path"] = "/dev/sda2".into();
        },
        |v: &mut serde_json::Value| {
            v["schema"] = 2.into();
        },
        |v: &mut serde_json::Value| {
            v["ticket"]["sequence"] = 0.into();
        },
        |v: &mut serde_json::Value| {
            v["ticket"]["candidate"] = "0".repeat(64).into();
        },
        |v: &mut serde_json::Value| {
            v["host"]["pid"] = 1.into();
        },
        |v: &mut serde_json::Value| {
            v["host"]["invocation"] = "x".repeat(32).into();
        },
        |v: &mut serde_json::Value| {
            v["ticket"]["url"] = "https://example.test".into();
        },
    ] {
        let mut changed = value.clone();
        change(&mut changed);
        assert!(Prepare::from_json(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
    assert!(Prepare::from_json(&vec![b' '; MAX_MESSAGE_BYTES + 1]).is_err());
    assert!(Id::try_from("A".repeat(64)).is_err());
}

#[test]
fn saved_settings_and_presented_updater_are_both_required() {
    let request = request();
    let ticket = &request.ticket;
    let mut gate = HostGate::default();
    begin(&mut gate, &request);
    assert_eq!(gate.acknowledge(ticket, 101), Err(Error::NotReady));
    assert_eq!(gate.saved(ticket, 19, true), Err(Error::Stale));
    gate.presented(ticket).unwrap();
    assert_eq!(gate.acknowledge(ticket, 102), Err(Error::NotReady));
    gate.saved(ticket, 20, true).unwrap();
    let ack = gate.acknowledge(ticket, 103).unwrap();
    assert_eq!(ack.settings_revision, 20);
    assert_eq!(
        Acknowledgement::from_json(&serde_json::to_vec(&ack).unwrap()).unwrap(),
        ack
    );

    let mut gate = HostGate::default();
    begin(&mut gate, &request);
    gate.saved(ticket, 20, true).unwrap();
    assert_eq!(gate.acknowledge(ticket, 104), Err(Error::NotReady));
}

#[test]
fn failed_save_or_input_before_acknowledgement_releases_the_fence() {
    let mut request = request();
    let mut gate = HostGate::default();
    begin(&mut gate, &request);
    assert_eq!(
        gate.saved(&request.ticket, 20, false),
        Err(Error::SaveFailed)
    );
    assert!(!gate.fenced());
    assert_eq!(gate.acknowledge(&request.ticket, 102), Err(Error::Stale));
    request.ticket.sequence += 1;
    begin(&mut gate, &request);
    gate.saved(&request.ticket, 20, true).unwrap();
    gate.interrupt(&request.ticket).unwrap();
    // Input advances the host revision: the queued old Prepare cannot refence it.
    assert_eq!(
        gate.prepare(request.clone(), &request.host, 11, 20, 103),
        Err(Error::Stale)
    );
    assert!(!gate.fenced());
}

#[test]
fn retries_do_not_reset_progress_or_extend_the_deadline() {
    let request = request();
    let mut gate = HostGate::default();
    begin(&mut gate, &request);
    gate.saved(&request.ticket, 20, true).unwrap();
    gate.prepare(request.clone(), &request.host, 10, 20, 200)
        .unwrap();
    let mut extended = request.clone();
    extended.expires_ms += 1;
    assert_eq!(
        gate.prepare(extended, &request.host, 10, 20, 200),
        Err(Error::Busy)
    );
    gate.presented(&request.ticket).unwrap();
    assert_eq!(
        gate.acknowledge(&request.ticket, request.expires_ms),
        Err(Error::Expired)
    );
    assert!(!gate.fenced());
}

#[test]
fn lost_ack_or_commit_response_cannot_unfence_the_app() {
    let request = request();
    let mut gate = HostGate::default();
    begin(&mut gate, &request);
    gate.saved(&request.ticket, 20, true).unwrap();
    gate.presented(&request.ticket).unwrap();
    let ack = gate.acknowledge(&request.ticket, 101).unwrap();
    gate.expire(100_000);
    assert!(gate.fenced());
    assert_eq!(gate.interrupt(&request.ticket), Err(Error::TooLate));
    assert_eq!(gate.acknowledge(&request.ticket, 100_000).unwrap(), ack);
    let mut wrong = request.ticket.clone();
    wrong.sequence += 1;
    assert_eq!(
        gate.aborted(&Aborted {
            schema: SCHEMA,
            ticket: wrong
        }),
        Err(Error::Stale)
    );
    assert!(gate.fenced());
    gate.aborted(&Aborted {
        schema: SCHEMA,
        ticket: request.ticket,
    })
    .unwrap();
    assert!(!gate.fenced());
}

#[test]
fn changed_host_and_settings_are_not_acknowledged_and_unacked_fences_expire() {
    let mut request = request();
    let mut gate = HostGate::default();
    let mut replaced = request.host.clone();
    replaced.start_ticks += 1;
    assert_eq!(
        gate.prepare(request.clone(), &replaced, 10, 20, 100),
        Err(Error::Stale)
    );
    assert_eq!(
        gate.prepare(request.clone(), &request.host, 10, 21, 100),
        Err(Error::Stale)
    );
    // Rejected old-host requests must not bind an empty gate to their service.
    request.ticket.service = Id::from_bytes([3; 32]).unwrap();
    begin(&mut gate, &request);
    gate.expire(request.expires_ms);
    assert!(!gate.fenced());
}

#[test]
fn delayed_prepare_cannot_revive_an_aborted_or_expired_ticket() {
    let request = request();
    let mut gate = HostGate::default();
    begin(&mut gate, &request);
    let abort = Aborted {
        schema: SCHEMA,
        ticket: request.ticket.clone(),
    };
    gate.aborted(&abort).unwrap();
    gate.aborted(&abort).unwrap(); // Lost abort response is safe to retry.
    assert_eq!(
        gate.prepare(request.clone(), &request.host, 10, 20, 104),
        Err(Error::Stale)
    );
    assert!(!gate.fenced());
    let mut next = request.clone();
    next.ticket.sequence += 1;
    begin(&mut gate, &next);
    assert_eq!(gate.aborted(&abort), Err(Error::Stale));
    assert!(gate.fenced());
    let mut gate = HostGate::default();
    gate.aborted(&abort).unwrap();
    assert_eq!(
        gate.prepare(request.clone(), &request.host, 10, 20, 104),
        Err(Error::Stale)
    );
}

#[test]
fn service_incarnations_cannot_overwrite_retired_ticket_history() {
    let request = request();
    let mut gate = HostGate::default();
    begin(&mut gate, &request);
    gate.interrupt(&request.ticket).unwrap();
    let mut other = request.clone();
    other.ticket.service = Id::from_bytes([9; 32]).unwrap();
    assert_eq!(
        gate.aborted(&Aborted {
            schema: SCHEMA,
            ticket: other.ticket.clone()
        }),
        Err(Error::Stale)
    );
    assert_eq!(
        gate.prepare(other, &request.host, 10, 20, 104),
        Err(Error::Stale)
    );
    assert_eq!(
        gate.prepare(request.clone(), &request.host, 10, 20, 104),
        Err(Error::Stale)
    );
    assert!(!gate.fenced());
}
