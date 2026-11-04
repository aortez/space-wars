use super::native::exchange;
use super::*;
use clap::Parser;
use spacewars_control::install::{Message, SCHEMA, Ticket, transport, wire::*};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    operation: Command,
}

fn ticket() -> Ticket {
    Ticket {
        service: Id::from_bytes([1; 32]).unwrap(),
        sequence: 7,
        candidate: Id::from_bytes([2; 32]).unwrap(),
    }
}
fn confirm(end_game: bool) -> ServiceRequest {
    let ticket = ticket();
    let mut args = vec![
        "install",
        "confirm",
        "--service",
        ticket.service.as_str(),
        "--sequence",
        "7",
        "--candidate",
        ticket.candidate.as_str(),
        "--restart",
    ];
    if end_game {
        args.push("--end-game");
    }
    Cli::try_parse_from(args)
        .unwrap()
        .operation
        .request()
        .unwrap()
}
fn reply(phase: Phase, ticket: Option<Ticket>) -> ServiceReply {
    ServiceReply {
        schema: SCHEMA,
        error: None,
        status: Status { phase, ticket },
        offer: None,
        aborted: None,
    }
}
fn uid() -> u32 {
    unsafe { libc::geteuid() }
}

#[test]
fn confirmation_requires_the_complete_ticket_and_explicit_restart_before_io() {
    let original = ticket();
    let args = vec![
        "install",
        "confirm",
        "--service",
        original.service.as_str(),
        "--sequence",
        "7",
        "--candidate",
        original.candidate.as_str(),
        "--restart",
    ];
    for omit in [2..4, 4..6, 6..8, 8..9] {
        let mut incomplete = args.clone();
        incomplete.drain(omit);
        assert!(Cli::try_parse_from(incomplete).is_err());
    }
    for (index, value) in [(3, "bad"), (5, "0"), (7, "../candidate"), (7, "nightly")] {
        let mut invalid = args.clone();
        invalid[index] = value;
        assert!(Cli::try_parse_from(invalid).is_err());
    }
    for end_game in [false, true] {
        let ServiceRequest::Confirm { consent, .. } = confirm(end_game) else {
            panic!()
        };
        assert_eq!(consent.ticket, original);
        assert_eq!(consent.end_game, end_game);
    }
    assert!(Cli::try_parse_from(["install", "status", "--socket", "/tmp/override"]).is_err());
}

#[test]
fn socket_confirmation_preserves_candidate_and_returns_service_refusal_without_retry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("install.sock");
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let refusal = Arc::new(Mutex::new(Some(Failure::EndGameRequired)));
    let failure = refusal.clone();
    let _server = transport::Server::bind(
        &path,
        Arc::new(|p| p.uid == uid()),
        Arc::new(move |bytes, _| {
            count.fetch_add(1, Ordering::SeqCst);
            let ServiceRequest::Confirm { consent, .. } = ServiceRequest::from_json(bytes).unwrap()
            else {
                panic!("CLI must not obtain a replacement offer or retry");
            };
            assert_eq!(consent.ticket, ticket());
            let error = *failure.lock().unwrap();
            let mut value = reply(
                if error.is_some() {
                    Phase::Offered
                } else {
                    Phase::Preparing
                },
                Some(ticket()),
            );
            value.error = error;
            serde_json::to_vec(&value).unwrap()
        }),
    )
    .unwrap();
    for error in [
        Some(Failure::EndGameRequired),
        Some(Failure::Stale),
        Some(Failure::Expired),
        None,
    ] {
        *refusal.lock().unwrap() = error;
        let before = calls.load(Ordering::SeqCst);
        let response = exchange(
            &confirm(error != Some(Failure::EndGameRequired)),
            &path,
            uid(),
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(response.error, error);
        assert_eq!(calls.load(Ordering::SeqCst), before + 1);
    }
}

#[test]
fn status_offer_and_cancel_send_only_the_requested_operation() {
    use spacewars_control::install::{Aborted, Candidate, Channel, Offer};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("install.sock");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let _server = transport::Server::bind(
        &path,
        Arc::new(|p| p.uid == uid()),
        Arc::new(move |bytes, _| {
            let request = ServiceRequest::from_json(bytes).unwrap();
            let mut value = reply(Phase::Idle, None);
            match &request {
                ServiceRequest::Status { .. } => {}
                ServiceRequest::Offer { .. } => {
                    value = reply(Phase::Offered, Some(ticket()));
                    value.offer = Some(Offer {
                        schema: SCHEMA,
                        ticket: ticket(),
                        candidate: Candidate {
                            id: ticket().candidate,
                            channel: Channel::Release,
                            generation: 2,
                            build_id: "fixture".into(),
                        },
                        ends_game: false,
                        expires_ms: transport::boot_ms().unwrap() + 300_000,
                    });
                }
                ServiceRequest::Cancel {
                    ticket: selected, ..
                } => {
                    assert_eq!(selected, &ticket());
                    value = reply(Phase::Aborted, Some(ticket()));
                    value.aborted = Some(Aborted {
                        schema: SCHEMA,
                        ticket: ticket(),
                    });
                }
                _ => panic!("read/cancel commands cannot confirm an installation"),
            }
            observed.lock().unwrap().push(request);
            serde_json::to_vec(&value).unwrap()
        }),
    )
    .unwrap();
    let selected = ticket();
    for args in [
        vec!["install", "status"],
        vec!["install", "offer"],
        vec![
            "install",
            "cancel",
            "--service",
            selected.service.as_str(),
            "--sequence",
            "7",
            "--candidate",
            selected.candidate.as_str(),
        ],
    ] {
        let request = Cli::try_parse_from(args)
            .unwrap()
            .operation
            .request()
            .unwrap();
        exchange(&request, &path, uid(), Duration::from_secs(1)).unwrap();
    }
    assert_eq!(calls.lock().unwrap().len(), 3);
}

#[test]
fn unknown_reply_wrong_peer_and_different_ticket_never_trigger_another_request() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("install.sock");
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let _server = transport::Server::bind(
        &path,
        Arc::new(|p| p.uid == uid()),
        Arc::new(move |_, _| {
            let number = count.fetch_add(1, Ordering::SeqCst);
            if number == 0 {
                return b"{}".to_vec();
            }
            let mut other = ticket();
            other.sequence += 1;
            serde_json::to_vec(&reply(Phase::Preparing, Some(other))).unwrap()
        }),
    )
    .unwrap();
    assert!(exchange(&confirm(true), &path, uid() + 1, Duration::from_secs(1)).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    for expected in 1..=2 {
        assert!(exchange(&confirm(true), &path, uid(), Duration::from_secs(1)).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), expected);
    }
}

#[test]
fn a_lost_confirmation_reply_has_a_bounded_deadline_and_is_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("install.sock");
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let (entered, receipt) = std::sync::mpsc::sync_channel(1);
    let (release, blocked) = std::sync::mpsc::sync_channel(1);
    let blocked = Mutex::new(blocked);
    let _server = transport::Server::bind(
        &path,
        Arc::new(|p| p.uid == uid()),
        Arc::new(move |bytes, _| {
            assert!(matches!(
                ServiceRequest::from_json(bytes).unwrap(),
                ServiceRequest::Confirm { .. }
            ));
            count.fetch_add(1, Ordering::SeqCst);
            entered.send(()).unwrap();
            let _ = blocked.lock().unwrap().recv_timeout(Duration::from_secs(3));
            serde_json::to_vec(&reply(Phase::Preparing, Some(ticket()))).unwrap()
        }),
    )
    .unwrap();
    let started = std::time::Instant::now();
    let result = exchange(&confirm(true), &path, uid(), Duration::from_millis(200));
    let elapsed = started.elapsed();
    receipt.recv_timeout(Duration::from_secs(1)).unwrap();
    release.send(()).unwrap();
    let error = result.unwrap_err();
    assert!(elapsed < Duration::from_secs(2));
    assert!(error.to_string().contains("use install status"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
