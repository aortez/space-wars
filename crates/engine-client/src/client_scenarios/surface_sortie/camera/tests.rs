use super::*;

fn target(x: f32, y: f32, height: f32) -> CameraTarget {
    CameraTarget {
        camera: Camera2::new(RenderPoint::new(x, y), height),
        anchor: RenderPoint::ZERO,
        focus: CameraFocus::Vehicle,
        framed_opponent: false,
    }
}

fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.002, "{a} != {b}");
}

#[test]
fn camera_zoom_and_focus_ease_without_overshoot_in_both_directions() {
    for (from, to) in [
        (target(0.0, 0.0, 260.0), target(3.0, 6.0, 44.0)),
        (target(3.0, 6.0, 44.0), target(0.0, 0.0, 260.0)),
    ] {
        let mut camera = PlayerCamera::new(from);
        let mut previous = camera.displayed(1.0);
        let dt = Duration::from_secs_f64(1.0 / 60.0);
        camera.advance(to, dt);
        assert!(camera.height > 44.0 && camera.height < 260.0);
        assert!(camera.offset.y > 0.0 && camera.offset.y < 6.0);
        for _ in 0..180 {
            let next = camera.displayed(1.0);
            assert!(
                (next.height - to.camera.height).abs()
                    <= (previous.height - to.camera.height).abs()
            );
            assert!(
                (next.center.y - to.camera.center.y).abs()
                    <= (previous.center.y - to.camera.center.y).abs()
            );
            previous = next;
            camera.advance(to, dt);
        }
        near(camera.height, to.camera.height);
        near(camera.offset.y, to.camera.center.y);
    }
}

#[test]
fn camera_easing_is_time_based_at_30_60_and_120_updates() {
    let initial = target(0.0, 0.0, 260.0);
    let desired = target(16.0, 7.0, 44.0);
    let samples = [30, 60, 120].map(|hz| {
        let mut camera = PlayerCamera::new(initial);
        for _ in 0..hz {
            camera.advance(desired, Duration::from_secs_f64(1.0 / hz as f64));
        }
        camera.displayed(1.0)
    });
    for sample in &samples[1..] {
        near(sample.center.x, samples[0].center.x);
        near(sample.center.y, samples[0].center.y);
        near(sample.height, samples[0].height);
    }
}

#[test]
fn camera_carries_active_actor_motion_without_follow_lag() {
    let mut desired = target(20.0, -4.0, 260.0);
    let mut camera = PlayerCamera::new(desired);
    for _ in 0..600 {
        desired.anchor.x += 4.0;
        desired.anchor.y -= 2.0;
        desired.camera.center.x += 4.0;
        desired.camera.center.y -= 2.0;
        camera.advance(desired, Duration::from_secs_f64(1.0 / 60.0));
        assert_eq!(camera.displayed(1.0), desired.camera);
    }
}

#[test]
fn camera_boarding_changes_anchor_without_an_instant_focus_jump() {
    let aboard = target(0.0, 0.0, 44.0);
    let mut on_foot = target(8.0, 7.0, 44.0);
    on_foot.anchor = RenderPoint::new(8.0, 0.0);
    on_foot.focus = CameraFocus::Spaceling;
    let dt = Duration::from_secs_f64(1.0 / 60.0);
    let mut camera = PlayerCamera::new(aboard);
    camera.advance(on_foot, dt);
    let first = camera.displayed(1.0);
    assert!(first.center.x > 0.0 && first.center.x < 1.0, "{first:?}");
    assert!(first.center.y > 0.0 && first.center.y < 1.0);
    for _ in 0..120 {
        camera.advance(on_foot, dt);
    }
    let before_board = camera.displayed(1.0);
    camera.advance(aboard, dt);
    let after_board = camera.displayed(1.0);
    assert!(after_board.center.x < before_board.center.x);
    assert!(after_board.center.x > before_board.center.x - 1.0);
}

#[test]
fn camera_pause_freezes_and_restart_or_teleport_snaps() {
    let initial = target(10.0, 20.0, 100.0);
    let mut camera = PlayerCamera::new(initial);
    let mut distant = target(5010.0, 1020.0, 44.0);
    distant.anchor = RenderPoint::new(5000.0, 1000.0);
    camera.advance(distant, Duration::ZERO);
    assert_eq!(camera, PlayerCamera::new(initial));
    camera.advance(distant, Duration::from_secs_f64(1.0 / 60.0));
    assert_eq!(camera, PlayerCamera::new(distant));
    assert_eq!(PlayerCamera::new(initial).displayed(1.0), initial.camera);
}

#[test]
fn camera_keeps_active_actor_visible_while_zooming_in_on_narrow_panes() {
    let mut initial = target(125.0, -80.0, 440.0);
    initial.framed_opponent = true;
    let mut camera = PlayerCamera::new(initial);
    for _ in 0..120 {
        camera.advance(target(0.0, 0.0, 44.0), Duration::from_secs_f64(1.0 / 60.0));
        for aspect in [0.18, 512.0 / 768.0, 400.0 / 480.0, 1.0] {
            let view = camera.displayed(aspect);
            let subject = view.world_to_viewport(RenderPoint::ZERO, aspect);
            assert!((0.0999..=0.9001).contains(&subject.x), "{subject:?}");
            assert!((0.0999..=0.9001).contains(&subject.y), "{subject:?}");
        }
    }
}
