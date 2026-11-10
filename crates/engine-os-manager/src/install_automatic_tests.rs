use super::*;
use crate::install_handoff::Ready;
use spacewars_control::install::{Candidate, Channel, HostInstance, Id};

const IMAGE: &str = "rootfs-fixture";
const DAY_MS: u64 = 24 * 60 * 60_000;

/// An unfenced app, observed at 100 ms, as its scene implies for a
/// confirmation: everything but the launcher is a game.
fn context(scene: Option<Scene>, idle_ms: Option<u64>) -> Context {
    Context {
        boot_id: "fixture-boot".into(),
        maintenance_clear: true,
        ready: Some(Ready {
            candidate: Candidate {
                id: Id::from_bytes([2; 32]).unwrap(),
                channel: Channel::Release,
                generation: 11,
                build_id: IMAGE.into(),
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
            game: scene != Some(Scene::Launcher),
            fenced: false,
            observed_ms: 100,
            scene,
            idle_ms,
        }),
    }
}
fn idle_launcher() -> Context {
    context(Some(Scene::Launcher), Some(IDLE_MS))
}
fn coordinator() -> Coordinator {
    Coordinator::new("fixture-boot".into()).unwrap()
}
fn host(context: &Context) -> &HostObservation {
    context.host.as_ref().unwrap()
}

#[test]
fn only_an_idle_launcher_or_clock_lets_an_attempt_go_on() {
    for (scene, idle_ms, expected) in [
        (Some(Scene::Launcher), Some(IDLE_MS), Ok(())),
        (Some(Scene::Clock), Some(IDLE_MS), Ok(())),
        (Some(Scene::Clock), Some(DAY_MS), Ok(())),
        // A game is never idle, however long nobody has touched it.
        (Some(Scene::Game), Some(DAY_MS), Err(Hold::Game)),
        // An app that does not say what it shows has a game on.
        (None, Some(DAY_MS), Err(Hold::Game)),
        (Some(Scene::Launcher), Some(IDLE_MS - 1), Err(Hold::InUse)),
        (Some(Scene::Clock), Some(0), Err(Hold::InUse)),
        // An app that cannot tell when it was last used is in use.
        (Some(Scene::Launcher), None, Err(Hold::InUse)),
        (None, None, Err(Hold::Game)),
    ] {
        let context = context(scene, idle_ms);
        assert_eq!(idle(host(&context)), expected, "{scene:?} {idle_ms:?}");
    }
    // An app that another handoff holds is not asked again.
    let mut held = idle_launcher();
    held.host.as_mut().unwrap().fenced = true;
    assert_eq!(idle(host(&held)), Err(Hold::AppUnavailable));
}

#[test]
fn an_attempt_needs_the_judged_image_an_app_and_a_free_coordinator() {
    let context = idle_launcher();
    let mut state = coordinator();
    assert_eq!(admit(&state, &context, IMAGE), Ok(()));
    // The gate judged another image than the one an offer would name now.
    assert_eq!(admit(&state, &context, "rootfs-other"), Err(Hold::Refused));
    let mut unready = idle_launcher();
    unready.ready = None;
    assert_eq!(admit(&state, &unready, IMAGE), Err(Hold::Refused));
    let mut absent = idle_launcher();
    absent.host = None;
    assert_eq!(admit(&state, &absent, IMAGE), Err(Hold::AppUnavailable));
    // The app's own evidence is read from the worker's context as well.
    assert_eq!(
        admit(
            &state,
            &self::context(Some(Scene::Game), Some(DAY_MS)),
            IMAGE
        ),
        Err(Hold::Game)
    );

    // A standing offer is no obstacle, and neither is one that was withdrawn.
    let offer = state.offer(&context, 100).unwrap();
    assert_eq!(admit(&state, &context, IMAGE), Ok(()));
    state.cancel(&offer.ticket).unwrap();
    assert_eq!(admit(&state, &context, IMAGE), Ok(()));

    // A handoff that somebody confirmed is: the worker is installing.
    let offer = state.offer(&context, 101).unwrap();
    let consent = Consent {
        schema: SCHEMA,
        ticket: offer.ticket,
        end_game: false,
    };
    state.confirm(&consent, &context, 102).unwrap();
    assert_eq!(admit(&state, &context, IMAGE), Err(Hold::Refused));
}

#[test]
fn its_own_confirmation_ends_the_clock_and_never_a_game() {
    // From the launcher nothing ends.
    let mut state = coordinator();
    let prepare = confirm(&mut state, &idle_launcher(), IMAGE, 101).unwrap();
    assert_eq!(state.status().phase, Handoff::Preparing);
    assert_eq!(state.status().ticket, Some(prepare.ticket));

    // The Clock is a game to a confirmation, and this one ends it.
    let clock = context(Some(Scene::Clock), Some(IDLE_MS));
    assert!(host(&clock).game);
    let mut state = coordinator();
    confirm(&mut state, &clock, IMAGE, 101).unwrap();
    assert_eq!(state.status().phase, Handoff::Preparing);

    // A game gets no offer at all.
    let mut state = coordinator();
    assert_eq!(
        confirm(
            &mut state,
            &context(Some(Scene::Game), Some(DAY_MS)),
            IMAGE,
            101
        )
        .unwrap_err(),
        Hold::Game
    );
    assert_eq!(state.status().phase, Handoff::Idle);

    // An app that calls its launcher a game is not believed either way: the
    // attempt does not say it ends a game, and the coordinator refuses one.
    let mut confused = idle_launcher();
    confused.host.as_mut().unwrap().game = true;
    let mut state = coordinator();
    assert_eq!(
        confirm(&mut state, &confused, IMAGE, 101).unwrap_err(),
        Hold::Refused
    );
    assert_ne!(state.status().phase, Handoff::Preparing);

    // An observation that went stale while the worker waited is refused.
    let mut state = coordinator();
    assert_eq!(
        confirm(&mut state, &idle_launcher(), IMAGE, 100 + 2_001).unwrap_err(),
        Hold::Refused
    );
}

#[test]
fn a_standing_offer_is_the_one_the_attempt_confirms() {
    let context = idle_launcher();
    let mut state = coordinator();
    let offer = state.offer(&context, 100).unwrap();
    let prepare = confirm(&mut state, &context, IMAGE, 101).unwrap();
    assert_eq!(prepare.ticket, offer.ticket);
    // Its preparation keeps the offer's own deadline or the handoff's, never
    // a later one.
    assert!(prepare.expires_ms <= offer.expires_ms);
}

#[test]
fn status_names_are_stable() {
    let status = Status {
        phase: Phase::Countdown,
        hold: None,
        countdown_ends_ms: Some(61_000),
    };
    assert_eq!(
        serde_json::to_value(status).unwrap(),
        serde_json::json!({"phase": "countdown", "hold": null, "countdown_ends_ms": 61_000})
    );
    for (phase, name) in [
        (Phase::Waiting, "waiting"),
        (Phase::Watching, "watching"),
        (Phase::Countdown, "countdown"),
        (Phase::Postponed, "postponed"),
        (Phase::Started, "started"),
    ] {
        assert_eq!(serde_json::to_value(phase).unwrap(), name);
    }
    for (hold, name) in [
        (Hold::AppUnavailable, "app-unavailable"),
        (Hold::Game, "game"),
        (Hold::InUse, "in-use"),
        (Hold::NotAutomatic, "not-automatic"),
        (Hold::Refused, "refused"),
    ] {
        assert_eq!(serde_json::to_value(hold).unwrap(), name);
    }
}
