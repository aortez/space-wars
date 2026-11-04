use super::*;
use spacewars_control::install::HostGate;

fn fixture_context() -> Context {
    Context {
        boot_id: "fixture-boot".into(),
        maintenance_clear: true,
        ready: Some(Ready {
            candidate: Candidate {
                id: Id::from_bytes([2; 32]).unwrap(),
                channel: Channel::Release,
                generation: 11,
                build_id: "rootfs-fixture".into(),
            },
            selection_generation: 4,
        }),
        host: Some(HostObservation {
            instance: HostInstance {
                pid: 42,
                start_ticks: 100,
                invocation: "1".repeat(32),
            },
            revision: 10,
            settings_revision: 20,
            game: true,
            fenced: false,
            observed_ms: 100,
        }),
    }
}

fn coordinator() -> Coordinator {
    Coordinator::new("fixture-boot".into()).unwrap()
}

fn prepare(service: &mut Coordinator, context: &Context) -> (Consent, Prepare) {
    let offer = service.offer(context, 100).unwrap();
    let consent = Consent {
        schema: SCHEMA,
        ticket: offer.ticket,
        end_game: true,
    };
    let request = service.confirm(&consent, context, 101).unwrap();
    (consent, request)
}

fn host_ack(request: &Prepare) -> (HostGate, Acknowledgement) {
    let mut gate = HostGate::default();
    gate.prepare(
        request.clone(),
        &request.host,
        request.host_revision,
        request.settings_revision,
        102,
    )
    .unwrap();
    gate.saved(&request.ticket, request.settings_revision, true)
        .unwrap();
    gate.presented(&request.ticket).unwrap();
    let ack = gate.acknowledge(&request.ticket, 103).unwrap();
    (gate, ack)
}

#[test]
fn consent_save_and_host_acknowledgement_produce_only_one_authorization() {
    let context = fixture_context();
    let mut service = coordinator();
    let offer = service.offer(&context, 100).unwrap();
    assert!(offer.ends_game);
    let mut consent = Consent {
        schema: SCHEMA,
        ticket: offer.ticket.clone(),
        end_game: false,
    };
    assert_eq!(
        service.confirm(&consent, &context, 101),
        Err(Error::EndGameRequired)
    );
    consent.end_game = true;
    let request = service.confirm(&consent, &context, 102).unwrap();
    let (mut host, ack) = host_ack(&request);
    let authorization = service.consume(&ack, &request.host, &context, 104).unwrap();
    assert_eq!(authorization.ticket(), &offer.ticket);
    assert_eq!(authorization.selection_generation(), 4);
    assert!(matches!(
        service.consume(&ack, &request.host, &context, 105),
        Err(Error::TooLate)
    ));
    assert_eq!(service.cancel(&offer.ticket), Err(Error::TooLate));
    assert!(service.offer(&context, 106).is_err());
    host.expire(100_000);
    assert!(
        host.fenced(),
        "a lost write/reboot reply cannot restart gameplay"
    );
}

#[test]
fn candidate_policy_host_and_maintenance_changes_permanently_revoke_consent() {
    for change in [
        |c: &mut Context| {
            c.ready.as_mut().unwrap().candidate.id = Id::from_bytes([3; 32]).unwrap();
        },
        |c: &mut Context| {
            c.ready.as_mut().unwrap().selection_generation += 1;
        },
        |c: &mut Context| {
            c.ready.as_mut().unwrap().candidate.channel = Channel::Nightly;
        },
        |c: &mut Context| {
            c.host.as_mut().unwrap().instance.pid += 1;
        },
        |c: &mut Context| {
            c.host.as_mut().unwrap().instance.start_ticks += 1;
        },
        |c: &mut Context| {
            c.host.as_mut().unwrap().instance.invocation = "2".repeat(32);
        },
        |c: &mut Context| {
            c.host.as_mut().unwrap().revision += 1;
        },
        |c: &mut Context| {
            c.host.as_mut().unwrap().settings_revision += 1;
        },
        |c: &mut Context| {
            c.host.as_mut().unwrap().game = false;
        },
        |c: &mut Context| {
            c.host = None;
        },
        |c: &mut Context| {
            c.ready = None;
        },
        |c: &mut Context| {
            c.maintenance_clear = false;
        },
        |c: &mut Context| {
            c.boot_id = "new-boot".into();
        },
    ] {
        let original = fixture_context();
        let mut changed = original.clone();
        change(&mut changed);
        let mut service = coordinator();
        let (consent, request) = prepare(&mut service, &original);
        let (mut host, ack) = host_ack(&request);
        assert!(service.consume(&ack, &request.host, &changed, 104).is_err());
        assert!(
            service
                .consume(&ack, &request.host, &original, 105)
                .is_err(),
            "restored conditions must not revive consent"
        );
        assert!(service.confirm(&consent, &original, 105).is_err());
        let abort = service.cancel(&consent.ticket).unwrap();
        host.aborted(&abort).unwrap();
        assert!(!host.fenced());
    }
}

#[test]
fn consent_is_bound_at_confirmation_as_well_as_final_consumption() {
    let mut context = fixture_context();
    let mut service = coordinator();
    let offer = service.offer(&context, 100).unwrap();
    context.host.as_mut().unwrap().revision += 1; // A game launch/input wins first.
    let consent = Consent {
        schema: SCHEMA,
        ticket: offer.ticket,
        end_game: true,
    };
    assert_eq!(service.confirm(&consent, &context, 101), Err(Error::Stale));
}

#[test]
fn only_the_authenticated_host_and_exact_saved_revision_can_complete() {
    let context = fixture_context();
    let mut service = coordinator();
    let (_, request) = prepare(&mut service, &context);
    let (_, ack) = host_ack(&request);
    let mut peer = request.host.clone();
    peer.pid += 1; // The caller cannot claim the kiosk PID in acknowledgement JSON.
    assert!(matches!(
        service.consume(&ack, &peer, &context, 104),
        Err(Error::Stale)
    ));
    let mut stale = ack.clone();
    stale.settings_revision -= 1;
    assert!(
        service
            .consume(&stale, &request.host, &context, 104)
            .is_err()
    );
    stale = ack.clone();
    stale.host_revision -= 1;
    assert!(
        service
            .consume(&stale, &request.host, &context, 104)
            .is_err()
    );
    assert!(service.consume(&ack, &request.host, &context, 104).is_ok());
}

#[test]
fn retries_keep_deadlines_and_expiry_is_inclusive() {
    let mut context = fixture_context();
    let mut service = coordinator();
    let (consent, request) = prepare(&mut service, &context);
    context.host.as_mut().unwrap().observed_ms = 1000;
    assert_eq!(service.confirm(&consent, &context, 1000).unwrap(), request);
    let (_, ack) = host_ack(&request);
    context.host.as_mut().unwrap().observed_ms = request.expires_ms;
    assert!(matches!(
        service.consume(&ack, &request.host, &context, request.expires_ms),
        Err(Error::Expired)
    ));
    let abort = service.cancel(&consent.ticket).unwrap();
    assert_eq!(service.cancel(&consent.ticket).unwrap(), abort);

    let mut service = coordinator();
    context.host.as_mut().unwrap().observed_ms = 100;
    let offer = service.offer(&context, 100).unwrap();
    context.host.as_mut().unwrap().observed_ms = offer.expires_ms;
    assert_eq!(
        service.confirm(
            &Consent {
                schema: SCHEMA,
                ticket: offer.ticket,
                end_game: true
            },
            &context,
            offer.expires_ms
        ),
        Err(Error::Expired)
    );
}

#[test]
fn restart_old_ticket_and_old_cancel_cannot_authorize_or_cancel_new_work() {
    let context = fixture_context();
    let mut old = coordinator();
    let (consent, request) = prepare(&mut old, &context);
    let (_, ack) = host_ack(&request);
    let mut restarted = coordinator();
    let offer = restarted.offer(&context, 100).unwrap();
    assert_ne!(offer.ticket.service, consent.ticket.service);
    assert!(
        restarted
            .consume(&ack, &request.host, &context, 104)
            .is_err()
    );
    assert!(restarted.cancel(&consent.ticket).is_err());
    let next = Consent {
        schema: SCHEMA,
        ticket: offer.ticket,
        end_game: true,
    };
    assert!(restarted.confirm(&next, &context, 105).is_ok());

    old.cancel(&consent.ticket).unwrap();
    let later = old.offer(&context, 104).unwrap();
    assert_ne!(later.ticket.sequence, consent.ticket.sequence);
    assert!(old.cancel(&consent.ticket).is_err());
}

#[test]
fn stale_future_missing_and_backward_observations_do_not_admit() {
    let mut context = fixture_context();
    for now in [99, 2101] {
        assert!(coordinator().offer(&context, now).is_err());
    }
    context.host = None;
    assert!(coordinator().offer(&context, 100).is_err());
    let context = fixture_context();
    let mut service = coordinator();
    let (_, request) = prepare(&mut service, &context);
    let (_, ack) = host_ack(&request);
    assert!(service.consume(&ack, &request.host, &context, 100).is_err());
    assert!(service.consume(&ack, &request.host, &context, 104).is_err());
}

#[test]
fn a_failed_save_never_produces_an_acknowledgement_or_consumes_consent() {
    let context = fixture_context();
    let mut service = coordinator();
    let (consent, request) = prepare(&mut service, &context);
    let mut host = HostGate::default();
    host.prepare(
        request.clone(),
        &request.host,
        request.host_revision,
        request.settings_revision,
        102,
    )
    .unwrap();
    assert_eq!(
        host.saved(&request.ticket, request.settings_revision, false),
        Err(Error::SaveFailed)
    );
    assert_eq!(host.acknowledge(&request.ticket, 103), Err(Error::Stale));
    assert!(service.cancel(&consent.ticket).is_ok());
}

#[test]
fn lost_offer_reply_is_idempotent_and_abandoned_offers_have_a_finite_lifetime() {
    let mut context = fixture_context();
    let mut service = coordinator();
    let offer = service.offer(&context, 100).unwrap();
    assert_eq!(service.offer(&context, 101).unwrap(), offer);
    context.host.as_mut().unwrap().observed_ms = offer.expires_ms;
    let next = service.offer(&context, offer.expires_ms).unwrap();
    assert_ne!(next.ticket, offer.ticket);
    assert!(
        service
            .confirm(
                &Consent {
                    schema: SCHEMA,
                    ticket: offer.ticket,
                    end_game: true
                },
                &context,
                next.expires_ms - OFFER_MS
            )
            .is_err()
    );
}

#[test]
fn lost_abort_reply_remains_recoverable_while_the_host_is_fenced() {
    let mut context = fixture_context();
    let mut service = coordinator();
    let (consent, request) = prepare(&mut service, &context);
    let (mut host, _) = host_ack(&request);
    context.host.as_mut().unwrap().fenced = true;
    let abort = service.cancel(&consent.ticket).unwrap();
    assert_eq!(service.offer(&context, 104), Err(Error::Busy));
    assert_eq!(service.cancel(&consent.ticket).unwrap(), abort);
    host.aborted(&abort).unwrap();
    context.host.as_mut().unwrap().fenced = false;
    assert!(service.offer(&context, 105).is_ok());
}

#[test]
fn queued_wrong_ticket_cannot_consume_the_current_handoff() {
    let context = fixture_context();
    let mut service = coordinator();
    let (_, request) = prepare(&mut service, &context);
    let (_, ack) = host_ack(&request);
    let mut wrong = ack.clone();
    wrong.ticket.sequence += 1;
    assert!(
        service
            .consume(&wrong, &request.host, &context, 104)
            .is_err()
    );
    assert!(service.consume(&ack, &request.host, &context, 105).is_ok());
}
