use super::*;
use spacewars_control::workflows::{self, Session, Workflow};

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn attached_workflows_preserve_launcher_running_and_paused_sessions() {
    run_functional_test_with_settings(
        "attached-workflows",
        "winit-software",
        4242,
        Some(engine_common::Settings::default()),
        |h| {
            h.wait_until_ready();
            attached(h, Workflow::Settings, "launcher", None).unwrap();
            let root = h.state();
            let previous =
                Session::capture(&h.client, Instant::now() + TRANSITION_TIMEOUT).unwrap();
            let root = h.activate_until_scenario("clock", root);
            let error = workflows::run(
                &h.client,
                &previous,
                Workflow::Settings,
                Instant::now() + TRANSITION_TIMEOUT,
                |_, _| Ok(()),
            )
            .unwrap_err();
            assert!(error.contains("Scenario/session changed"));
            assert!(
                workflows::restore(
                    &h.client,
                    &previous,
                    Instant::now() + TRANSITION_TIMEOUT,
                    |_, _| Ok(())
                )
                .is_err()
            );
            let current = Session::capture(&h.client, Instant::now() + TRANSITION_TIMEOUT).unwrap();
            let error = workflows::run(
                &h.client,
                &current,
                Workflow::Settings,
                Instant::now(),
                |_, _| Ok(()),
            )
            .unwrap_err();
            assert!(error.contains("deadline"));
            h.activate_guarded("launcher.start", &root);
            let game = h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            attached(h, Workflow::Settings, "running-settings", None).unwrap();
            attached(h, Workflow::ClockPause, "running-clock", None).unwrap();
            let state = h.state();
            assert_eq!(state.scenario_revision, game.scenario_revision);
            h.pause_guarded(&state);
            h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::PauseMain),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            let clock_before = h
                .client
                .clock_state_before(Instant::now() + TRANSITION_TIMEOUT)
                .unwrap();
            attached(h, Workflow::Settings, "paused-settings", None).unwrap();
            attached(h, Workflow::ClockPause, "paused-clock", None).unwrap();
            let clock_after = h
                .client
                .clock_state_before(Instant::now() + TRANSITION_TIMEOUT)
                .unwrap();
            assert_eq!(clock_before.simulation_tick, clock_after.simulation_tick);
            assert_eq!(clock_before.settings, clock_after.settings);
            assert!(clock_after.paused);
        },
    );
}

#[test]
#[ignore = "requires an explicit display; CI runs this test under Xvfb"]
fn attached_workflows_preserve_automatic_clock_and_recover_failed_checks() {
    let mut settings = engine_common::Settings::default();
    settings.launch.renderer = engine_common::RendererSetting::Raster;
    settings.autostart.enabled = true;
    settings.autostart.activity = "clock".into();
    settings.autostart.delay_seconds = 5;
    run_functional_test_with_settings(
        "attached-automatic",
        "winit-software",
        4242,
        Some(settings),
        |h| {
            h.wait_until_ready();
            h.wait_for(
                UiStatePredicate {
                    screen: Some(UiScreen::Gameplay),
                    scenario: Some("clock".into()),
                    ..Default::default()
                },
                TRANSITION_TIMEOUT,
            );
            attached(h, Workflow::Settings, "automatic-settings", None).unwrap();
            attached(h, Workflow::ClockPause, "automatic-clock", None).unwrap();
            let error =
                attached(h, Workflow::Settings, "interrupted", Some("saved-networks")).unwrap_err();
            assert_eq!(error, "injected checkpoint failure");
            assert!(
                h.client
                    .request("status\n")
                    .unwrap()
                    .contains("autostart_session=automatic")
            );
            // Recovery left the same automatic session usable by another run.
            attached(h, Workflow::ClockPause, "after-cleanup", None).unwrap();
        },
    );
}

fn attached(
    h: &mut FunctionalHarness,
    workflow: Workflow,
    label: &str,
    fail: Option<&str>,
) -> Result<(), String> {
    let client = h.client.clone();
    let deadline = Instant::now() + Duration::from_secs(30);
    let before_status = loop {
        let status = client.request_before("status\n", deadline).unwrap();
        if status.contains("settings_save_pending=false") && status.contains("autostart_session=") {
            break status;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(POLL_INTERVAL);
    };
    let settings_path = h.run_path().join("config/settings.toml");
    let before_settings = fs::read(&settings_path).unwrap();
    let session = Session::capture(&client, deadline).unwrap();
    let result = workflows::run(&client, &session, workflow, deadline, |observation, _| {
        h.history.push(json!({"attached_workflow": observation}));
        if let Some(checkpoint) = observation.checkpoint {
            h.capture_screenshot(&format!("{label}-{checkpoint}.png"));
            if fail == Some(checkpoint.as_str()) {
                return Err("injected checkpoint failure".into());
            }
        }
        Ok(())
    });
    let restored = workflows::restore(
        &client,
        &session,
        Instant::now() + TRANSITION_TIMEOUT,
        |observation, _| {
            h.history.push(json!({"attached_cleanup": observation}));
            Ok(())
        },
    )
    .unwrap();
    assert!(session.restored(&restored));
    assert_eq!(fs::read(settings_path).unwrap(), before_settings);
    let after_status = client
        .request_before("status\n", Instant::now() + TRANSITION_TIMEOUT)
        .unwrap();
    let automatic = |s: &str| {
        s.lines()
            .find(|line| line.starts_with("autostart_session="))
            .map(str::to_owned)
    };
    assert_eq!(automatic(&before_status), automatic(&after_status));
    result.map(|_| ())
}
