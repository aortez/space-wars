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

#[test]
fn written_counts_round_trip_and_cannot_exceed_the_total_or_lack_a_ticket() {
    let ticket = request().ticket;
    let mut reply = wire::ServiceReply {
        schema: SCHEMA,
        error: None,
        status: wire::Status {
            phase: wire::Phase::Writing,
            ticket: Some(ticket.clone()),
            written: Some(wire::Written {
                bytes: 300,
                total: 1000,
            }),
        },
        offer: None,
        aborted: None,
        policy: None,
    };
    reply.validate().unwrap();
    let json = serde_json::to_string(&reply).unwrap();
    assert!(json.contains("\"written\":{\"bytes\":300,\"total\":1000}"));
    let decoded: wire::ServiceReply = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.status.written, reply.status.written);
    // A reply without counts still decodes, and none are written back out.
    let plain: wire::ServiceReply =
        serde_json::from_str(&json.replace(",\"written\":{\"bytes\":300,\"total\":1000}", ""))
            .unwrap();
    assert_eq!(plain.status.written, None);
    assert!(!serde_json::to_string(&plain).unwrap().contains("written"));
    reply.status.written = Some(wire::Written {
        bytes: 1001,
        total: 1000,
    });
    assert_eq!(reply.validate(), Err(Error::Invalid));
    reply.status = wire::Status {
        phase: wire::Phase::Idle,
        ticket: None,
        written: Some(wire::Written {
            bytes: 0,
            total: 1000,
        }),
    };
    assert_eq!(reply.validate(), Err(Error::Invalid));
}

#[test]
fn host_view_reports_its_scene_and_idle_time_only_when_it_knows_them() {
    let view = wire::HostView {
        nonce: 7,
        host: request().host,
        revision: 3,
        settings_revision: 2,
        game: true,
        fenced: false,
        available: true,
        scene: Some(wire::Scene::Clock),
        idle_ms: Some(600_000),
    };
    let json = serde_json::to_string(&view).unwrap();
    assert!(json.ends_with(",\"scene\":\"clock\",\"idle_ms\":600000}"));
    let decoded: wire::HostView = serde_json::from_str(&json).unwrap();
    assert_eq!(
        (decoded.scene, decoded.idle_ms),
        (Some(wire::Scene::Clock), Some(600_000))
    );
    for (scene, name) in [
        (wire::Scene::Launcher, "\"launcher\""),
        (wire::Scene::Clock, "\"clock\""),
        (wire::Scene::Game, "\"game\""),
    ] {
        assert_eq!(serde_json::to_string(&scene).unwrap(), name);
    }
    // An app that reports neither still decodes, and neither is written back.
    let plain: wire::HostView =
        serde_json::from_str(&json.replace(",\"scene\":\"clock\",\"idle_ms\":600000", "")).unwrap();
    assert_eq!((plain.scene, plain.idle_ms), (None, None));
    let written = serde_json::to_string(&plain).unwrap();
    assert!(!written.contains("scene") && !written.contains("idle_ms"));
    // A scene this version does not know is refused, not read as idle.
    assert!(
        serde_json::from_str::<wire::HostView>(&json.replace("\"clock\"", "\"screensaver\""))
            .is_err()
    );
}

#[test]
fn a_countdown_names_its_generation_and_its_end() {
    let request = wire::HostRequest::Countdown {
        schema: SCHEMA,
        nonce: 7,
        generation: 27,
        ends_ms: 61_000,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(
        json,
        r#"{"command":"countdown","schema":1,"nonce":7,"generation":27,"ends_ms":61000}"#
    );
    assert!(wire::HostRequest::from_json(json.as_bytes()).is_ok());
    for invalid in [
        json.replace("\"generation\":27", "\"generation\":0"),
        json.replace("\"ends_ms\":61000", "\"ends_ms\":0"),
        json.replace("\"schema\":1", "\"schema\":2"),
        json.replace(",\"ends_ms\":61000", ""),
        json.replace("}", ",\"build_id\":\"x\"}"),
    ] {
        assert_eq!(
            wire::HostRequest::from_json(invalid.as_bytes()).err(),
            Some(Error::Invalid),
            "{invalid}"
        );
    }
}

#[test]
fn the_panel_reads_the_setting_and_saves_only_what_it_changes() {
    let read = r#"{"command":"policy","schema":1}"#;
    assert!(wire::ServiceRequest::from_json(read.as_bytes()).is_ok());
    let save = wire::ServiceRequest::SavePolicy {
        schema: SCHEMA,
        change: wire::PolicyChange {
            mode: Some(wire::InstallMode::Automatic),
            window: None,
        },
    };
    let json = serde_json::to_string(&save).unwrap();
    assert_eq!(
        json,
        r#"{"command":"save-policy","schema":1,"change":{"mode":"automatic"}}"#
    );
    let window = r#"{"command":"save-policy","schema":1,"change":{"window":{"start_minute":780,"end_minute":900}}}"#;
    for valid in [json.as_str(), window] {
        assert!(
            wire::ServiceRequest::from_json(valid.as_bytes()).is_ok(),
            "{valid}"
        );
    }
    for invalid in [
        r#"{"command":"save-policy","schema":1,"change":{}}"#.to_owned(),
        window.replace("900", "780"),
        window.replace("900", "1440"),
        json.replace("automatic", "sometimes"),
        json.replace("}}", ",\"countdown\":true}}"),
        json.replace("\"schema\":1", "\"schema\":2"),
        read.replace("}", ",\"mode\":\"ask\"}"),
    ] {
        assert_eq!(
            wire::ServiceRequest::from_json(invalid.as_bytes()).err(),
            Some(Error::Invalid),
            "{invalid}"
        );
    }
}

#[test]
fn a_reply_carries_the_setting_only_without_an_error_or_an_offer() {
    let mut reply = wire::ServiceReply {
        schema: SCHEMA,
        error: None,
        status: wire::Status {
            phase: wire::Phase::Idle,
            ticket: None,
            written: None,
        },
        offer: None,
        aborted: None,
        policy: None,
    };
    let plain = serde_json::to_string(&reply).unwrap();
    assert!(!plain.contains("policy"));
    reply.policy = Some(wire::InstallPolicy {
        mode: wire::InstallMode::Ask,
        window: wire::InstallWindow {
            start_minute: 180,
            end_minute: 300,
        },
        local_minute: Some(1334),
    });
    assert_eq!(reply.validate(), Ok(()));
    let json = serde_json::to_string(&reply).unwrap();
    assert!(json.ends_with(
        r#","policy":{"mode":"ask","window":{"start_minute":180,"end_minute":300},"local_minute":1334}}"#
    ));
    let decoded: wire::ServiceReply = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.policy, reply.policy);
    // A device without a usable time says so by leaving the minute out.
    let unset: wire::ServiceReply =
        serde_json::from_str(&json.replace(",\"local_minute\":1334", "")).unwrap();
    assert_eq!(unset.policy.unwrap().local_minute, None);
    for broken in [
        |r: &mut wire::ServiceReply| r.error = Some(wire::Failure::SaveFailed),
        |r: &mut wire::ServiceReply| r.policy.as_mut().unwrap().local_minute = Some(1440),
        |r: &mut wire::ServiceReply| r.policy.as_mut().unwrap().window.end_minute = 180,
    ] {
        let mut changed = reply.clone();
        broken(&mut changed);
        assert_eq!(changed.validate(), Err(Error::Invalid));
    }
}

#[test]
fn a_window_moves_by_whole_hours_and_wraps_at_midnight() {
    let window = |start_minute, end_minute| wire::InstallWindow {
        start_minute,
        end_minute,
    };
    assert_eq!(window(180, 300).shifted(1), window(240, 360));
    assert_eq!(window(180, 300).shifted(-3), window(0, 120));
    assert_eq!(window(0, 120).shifted(-1), window(1380, 60));
    assert_eq!(window(1410, 60).shifted(1), window(30, 120));
    assert_eq!(window(810, 855).shifted(24), window(810, 855));
    assert!(window(1380, 60).valid() && !window(60, 60).valid() && !window(0, 1440).valid());
}
