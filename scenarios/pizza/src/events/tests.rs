use super::*;

const KIND: PizzaEventKind = PizzaEventKind::DirectionalGravity;

fn tick(events: &mut Events) -> PizzaEventState {
    events.begin_tick();
    let during = events.state();
    events.finish_tick();
    during
}

fn wait(events: &Events) -> u32 {
    let PizzaEventState::Idle {
        automatic_wait_ticks: Some(wait),
        pending_manual: None,
    } = events.state()
    else {
        panic!("expected an automatic quiet gap: {:?}", events.state());
    };
    assert!((MIN_EVENT_GAP_TICKS..=MAX_EVENT_GAP_TICKS).contains(&wait));
    wait
}

fn quiet_gap(events: &mut Events) {
    let initial = wait(events);
    for remaining in (1..=initial).rev() {
        assert_eq!(
            tick(events),
            PizzaEventState::Idle {
                automatic_wait_ticks: Some(remaining),
                pending_manual: None,
            }
        );
    }
}

fn active_visit(events: &mut Events, trigger: PizzaEventTrigger) -> PizzaEventDirection {
    let mut direction = None;
    for elapsed_ticks in 0..DIRECTIONAL_GRAVITY_TICKS {
        let during = tick(events);
        let PizzaEventState::Active {
            direction: chosen, ..
        } = during
        else {
            panic!("expected active update {elapsed_ticks}: {during:?}");
        };
        assert_eq!(*direction.get_or_insert(chosen), chosen);
        assert_eq!(
            during,
            PizzaEventState::Active {
                kind: KIND,
                direction: chosen,
                trigger,
                elapsed_ticks,
                remaining_ticks: DIRECTIONAL_GRAVITY_TICKS - elapsed_ticks,
            }
        );
    }
    assert!(matches!(events.state(), PizzaEventState::Idle { .. }));
    direction.unwrap()
}

#[test]
fn automatic_gaps_surround_exactly_900_active_updates() {
    assert_eq!(DIRECTIONAL_GRAVITY_TICKS, 900);
    for seed in 0..32 {
        let mut events = Events::new(seed, true, false);
        quiet_gap(&mut events);
        active_visit(&mut events, PizzaEventTrigger::Automatic);
        quiet_gap(&mut events);
        active_visit(&mut events, PizzaEventTrigger::Automatic);
        wait(&events);
    }
}

#[test]
fn manual_request_bypasses_wait_and_wins_at_the_automatic_deadline() {
    for due in [false, true] {
        let mut single = Events::new(7, true, false);
        if due {
            quiet_gap(&mut single);
        }
        let mut repeated = single.clone();
        assert!(single.request(KIND));
        assert!(repeated.request(KIND));
        for _ in 0..10 {
            assert!(!repeated.request(KIND));
        }
        for elapsed in 0..DIRECTIONAL_GRAVITY_TICKS {
            if elapsed > 0 {
                assert!(
                    !repeated.request(KIND),
                    "busy requests cannot extend or queue"
                );
            }
            let during = tick(&mut single);
            assert!(matches!(
                during,
                PizzaEventState::Active {
                    trigger: PizzaEventTrigger::Manual,
                    ..
                }
            ));
            assert_eq!(tick(&mut repeated), during);
            assert_eq!(single.state(), repeated.state());
        }
        // Includes the busy request on the final active update: no queued event
        // and identical future choices prove that rejections consumed no RNG.
        let gap = wait(&single);
        assert_eq!(gap, wait(&repeated));
        for _ in 0..gap + DIRECTIONAL_GRAVITY_TICKS {
            assert_eq!(tick(&mut single), tick(&mut repeated));
        }
    }
}

#[test]
fn automatic_off_still_allows_manual_visits_without_catch_up() {
    let mut events = Events::new(19, false, false);
    let idle = PizzaEventState::Idle {
        automatic_wait_ticks: None,
        pending_manual: None,
    };
    for _ in 0..MAX_EVENT_GAP_TICKS * 2 {
        assert_eq!(tick(&mut events), idle);
    }
    assert!(events.request(KIND));
    active_visit(&mut events, PizzaEventTrigger::Manual);
    for _ in 0..MAX_EVENT_GAP_TICKS * 2 {
        assert_eq!(tick(&mut events), idle);
    }
}

#[test]
fn seeded_directions_cover_all_edges_and_remain_fixed_through_each_visit() {
    let mut seen = [false; 4];
    for seed in 0..64 {
        let mut events = Events::new(seed, false, false);
        events.request(KIND);
        let direction = active_visit(&mut events, PizzaEventTrigger::Manual);
        seen[PizzaEventDirection::ALL
            .iter()
            .position(|d| *d == direction)
            .unwrap()] = true;
    }
    assert!(seen.into_iter().all(|edge| edge));
}

#[test]
fn cloned_runtime_replays_future_admissions_and_gaps() {
    let mut events = Events::new(42, true, false);
    events.request(KIND);
    for _ in 0..317 {
        tick(&mut events);
    }
    let mut replay = events.clone();
    for step in 0..25_000 {
        if step % 701 == 0 {
            assert_eq!(events.request(KIND), replay.request(KIND));
        }
        assert_eq!(tick(&mut events), tick(&mut replay));
        assert_eq!(events.state(), replay.state());
    }
}

#[test]
fn typed_action_round_trips_and_rejects_malformed_requests() {
    let trigger = PizzaAction::TriggerEvent(KIND);
    assert_eq!(PizzaAction::decode(&trigger.encode()), Some(trigger));
    for payload in [
        vec![],
        vec![1],
        vec![1, 0],
        vec![1, 0, 1, 0],
        vec![2, 0, 1],
        vec![1, 0, 0],
        vec![1, 0, 2],
    ] {
        assert_eq!(
            PizzaAction::decode(&Action::scenario(PIZZA_ACTION_TRIGGER_EVENT, payload)),
            None
        );
    }
    assert_eq!(
        PizzaAction::decode(&Action::scenario(2, vec![1, 0, 1])),
        None
    );
    assert_eq!(
        PizzaAction::decode(&Action::Pointer(engine_common::PointerAction {
            position: engine_common::RenderPoint::new(0.0, 0.0),
            phase: engine_common::PointerPhase::Press,
        })),
        None
    );
}
