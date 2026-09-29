use super::*;
use crate::events::{EventPhase, duck::DUCK_TICKS};

fn moving_panels(event: &DuckEvent) -> [bool; 2] {
    let mut active = door_panels(Some(event)).map(|panel| panel.is_some());
    if event.direction < 0.0 {
        active.reverse();
    }
    active
}

#[test]
fn entrance_panel_returns_flush_after_closing_and_stays_closed() {
    for aspect in [1024.0 / 768.0, 800.0 / 480.0] {
        for direction in [-1.0, 1.0] {
            let mut event = DuckEvent::new_platforms(Layout::new(aspect), 42);
            event.direction = direction;
            assert_eq!(moving_panels(&event), [false, false], "initially flush");
            event.step();
            assert_eq!(moving_panels(&event), [true, false], "entrance opens alone");
            while event.phase == EventPhase::Opening {
                assert!(!event.step(), "opening must finish within the event");
            }
            let panel = door_panels(Some(&event))
                .into_iter()
                .flatten()
                .next()
                .unwrap();
            let end = panel.transform(Vec2::new(panel.hinge.x, panel.bottom));
            assert!((end.y - panel.top).abs() < 1e-4, "open flap is horizontal");
            assert!(
                (end.x - panel.hinge.x) * direction > 0.0,
                "lifts into the scene"
            );
            while event.door_openness().0 == 1.0 {
                assert!(
                    !event.step(),
                    "entrance must begin closing within the event"
                );
            }
            assert_eq!(
                moving_panels(&event),
                [true, false],
                "keep the closing flap"
            );
            while event.door_openness().0 > 0.0 {
                assert!(
                    !event.step(),
                    "entrance must finish closing within the event"
                );
            }
            assert!(event.position().is_some(), "close the wall, not the duck");
            assert_eq!(
                moving_panels(&event),
                [false, false],
                "closed entrance is flush"
            );

            let mut saw_exit = false;
            while event.tick < DUCK_TICKS {
                let open = event.exit_visible()
                    && event.door_openness().1 > 0.0
                    && event.course_opacity() > 0.0;
                saw_exit |= open;
                assert_eq!(
                    moving_panels(&event),
                    [false, open],
                    "aspect={aspect} direction={direction} tick={}",
                    event.tick
                );
                event.step();
            }
            assert!(saw_exit, "the later exit flap must still open");
            assert_eq!(
                event.diagnostics().outcome,
                Some(engine_common::ClockDuckOutcome::Exited)
            );
            assert_eq!(moving_panels(&event), [false, false]);
        }
    }
}
