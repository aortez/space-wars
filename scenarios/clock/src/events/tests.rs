use super::*;

fn advance(schedule: &mut EventSchedule, ticks: u64) {
    for _ in 0..ticks {
        schedule.advance_tick();
    }
}

#[test]
fn catalog_covers_every_kind_once_and_has_bounded_timing() {
    for (kind, definition) in ClockEventKind::ALL.into_iter().zip(EVENT_CATALOG.iter()) {
        assert_eq!(definition.kind, kind);
        assert_eq!(EVENT_CATALOG[kind as usize].kind, kind);
        assert!(definition.duration_ticks > 0 && definition.duration_ticks <= 12 * 60);
        assert!(definition.cooldown_ticks >= COOLDOWN_TICKS);
    }
}

#[test]
fn one_global_cadence_selects_one_event_and_avoids_eligible_repeats() {
    let mut schedule = EventSchedule::new(ClockEventProfile::Calm, ClockEvents::default(), 42);
    let mut previous = None;
    for _ in 0..6 {
        let wait = schedule.next_event_tick.unwrap() - schedule.tick;
        assert!((45 * 60..=75 * 60).contains(&wait));
        advance(&mut schedule, wait - 1);
        assert_eq!(schedule.due_event(true), None);
        advance(&mut schedule, 1);
        assert_eq!(schedule.due_event(false), None);
        let kind = schedule.due_event(true).unwrap();
        assert_ne!(Some(kind), previous);
        schedule.start(kind);
        assert_eq!(schedule.due_event(true), None);
        advance(&mut schedule, EVENT_CATALOG[kind as usize].duration_ticks);
        schedule.finish(kind);
        assert_eq!(schedule.due_event(true), None);
        advance(&mut schedule, COOLDOWN_TICKS);
        previous = Some(kind);
    }
}

#[test]
fn automatic_selection_respects_enablement_and_each_events_reuse_delay() {
    for kind in ClockEventKind::ALL {
        if EVENT_CATALOG[kind as usize].trigger != ClockEventTrigger::Periodic {
            continue;
        }
        let enabled = ClockEvents {
            falling: kind == ClockEventKind::Falling,
            color_cycle: kind == ClockEventKind::ColorCycle,
            meltdown: kind == ClockEventKind::Meltdown,
            duck: kind == ClockEventKind::Duck,
            marquee: kind == ClockEventKind::Marquee,
            digit_slide: false,
        };
        let mut schedule = EventSchedule::new(ClockEventProfile::Demo, enabled, 2);
        let wait = schedule.next_event_tick.unwrap();
        advance(&mut schedule, wait);
        assert_eq!(schedule.due_event(true), Some(kind));
        schedule.start(kind);
        advance(&mut schedule, EVENT_CATALOG[kind as usize].duration_ticks);
        schedule.finish(kind);
        let ready_at = schedule.ready_at[kind as usize];
        advance(&mut schedule, COOLDOWN_TICKS);
        let wait = schedule.next_event_tick.unwrap() - schedule.tick;
        advance(&mut schedule, wait);
        assert_eq!(schedule.due_event(true), None);
        assert_eq!(schedule.next_event_tick, Some(ready_at));
        let remaining = ready_at - schedule.tick;
        advance(&mut schedule, remaining - 1);
        assert_eq!(schedule.due_event(true), None);
        advance(&mut schedule, 1);
        assert_eq!(schedule.due_event(true), Some(kind));
    }
}

#[test]
fn off_and_an_empty_enabled_set_never_schedule_automatic_events() {
    for (profile, enabled) in [
        (ClockEventProfile::Off, ClockEvents::default()),
        (
            ClockEventProfile::Demo,
            ClockEvents {
                falling: false,
                color_cycle: false,
                meltdown: false,
                duck: false,
                marquee: false,
                digit_slide: false,
            },
        ),
    ] {
        let mut schedule = EventSchedule::new(profile, enabled, 3);
        advance(&mut schedule, 90 * 60);
        assert_eq!(schedule.next_event_tick, None);
        assert_eq!(schedule.due_event(true), None);
        // A preview still finishes cleanly without enabling automatic events.
        schedule.start(ClockEventKind::ColorCycle);
        schedule.finish(ClockEventKind::ColorCycle);
        advance(&mut schedule, COOLDOWN_TICKS);
        assert_eq!(schedule.next_event_tick, None);
    }
}

#[test]
fn time_change_events_preserve_periodic_deadlines_and_rng() {
    let mut a = EventSchedule::new(ClockEventProfile::Calm, ClockEvents::default(), 42);
    let mut b = EventSchedule::new(ClockEventProfile::Calm, ClockEvents::default(), 42);
    let deadline = a.next_event_tick.unwrap();
    assert_eq!(a.time_change_event(), Some(ClockEventKind::DigitSlide));
    a.start(ClockEventKind::DigitSlide);
    assert_eq!(a.time_change_event(), None);
    advance(&mut a, DIGIT_SLIDE_TICKS);
    a.finish(ClockEventKind::DigitSlide);
    advance(&mut a, COOLDOWN_TICKS);
    assert_eq!(a.next_event_tick, Some(deadline));
    let remaining = deadline - a.tick;
    advance(&mut a, remaining);
    advance(&mut b, deadline);
    assert_eq!(a.due_event(true), b.due_event(true));
    assert_ne!(a.due_event(true), Some(ClockEventKind::DigitSlide));
}

#[test]
fn slide_only_configuration_never_schedules_periodic_work() {
    let mut events = ClockEvents {
        falling: false,
        color_cycle: false,
        meltdown: false,
        duck: false,
        marquee: false,
        digit_slide: true,
    };
    let mut schedule = EventSchedule::new(ClockEventProfile::Calm, events, 4);
    assert_eq!(schedule.next_event_tick, None);
    advance(&mut schedule, 36000);
    assert_eq!(schedule.due_event(true), None);
    assert_eq!(
        schedule.time_change_event(),
        Some(ClockEventKind::DigitSlide)
    );
    events.digit_slide = false;
    schedule.configure(ClockEventProfile::Calm, events);
    assert_eq!(schedule.time_change_event(), None);
    schedule.configure(ClockEventProfile::Off, ClockEvents::default());
    assert_eq!(schedule.time_change_event(), None);
}

#[test]
fn a_replacing_slide_preview_restarts_the_periodic_schedule_and_respects_reconfiguration() {
    let mut schedule = EventSchedule::new(ClockEventProfile::Calm, ClockEvents::default(), 4);
    schedule.start(ClockEventKind::Falling);
    schedule.finish(ClockEventKind::Falling);
    schedule.start(ClockEventKind::DigitSlide);
    assert!(schedule.next_event_tick.is_some());
    schedule.configure(ClockEventProfile::Off, ClockEvents::default());
    assert_eq!(schedule.next_event_tick, None);
    schedule.finish(ClockEventKind::DigitSlide);
    advance(&mut schedule, COOLDOWN_TICKS);
    assert_eq!(schedule.next_event_tick, None);
    schedule.configure(ClockEventProfile::Demo, ClockEvents::default());
    assert!(schedule.next_event_tick.is_some());
}
