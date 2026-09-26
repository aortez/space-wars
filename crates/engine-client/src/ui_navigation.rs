//! Backend-neutral menu actions shared by keyboards and gamepads.

use spacewars_control::UiAction;

pub(crate) fn moved_selection(current: i32, item_count: i32, delta: i32) -> i32 {
    if item_count <= 0 {
        return 0;
    }
    (current.clamp(0, item_count - 1) + delta).rem_euclid(item_count)
}

pub(crate) fn moved_launcher_selection(current: i32, action: UiAction) -> i32 {
    // Scenario, Play, [Settings, Controls, App Settings], Quit footer.
    // Keep old control indices stable even though their layout has changed.
    let current = current.clamp(0, 5);
    match action {
        UiAction::Up => [4, 0, 1, 1, 2, 1][current as usize],
        UiAction::Down => [1, 2, 4, 4, 0, 4][current as usize],
        UiAction::Left => [0, 1, 5, 2, 4, 3][current as usize],
        UiAction::Right => [0, 1, 3, 5, 4, 2][current as usize],
        _ => current,
    }
}

/// The same layout adds Play New World beside Play; Quit stays in the footer.
pub(crate) fn moved_match_launcher_selection(current: i32, action: UiAction) -> i32 {
    let current = current.clamp(0, 6) as usize;
    match action {
        UiAction::Up => [4, 0, 1, 1, 2, 6, 0][current],
        UiAction::Down => [1, 2, 4, 4, 0, 4, 5][current],
        UiAction::Left => [0, 6, 5, 2, 4, 3, 1][current],
        UiAction::Right => [0, 6, 3, 5, 4, 2, 1][current],
        _ => current as i32,
    }
}

pub(crate) fn moved_launcher_controls_selection(current: i32, action: UiAction) -> i32 {
    match action {
        UiAction::Up | UiAction::Left => moved_selection(current, 3, -1),
        UiAction::Down | UiAction::Right => moved_selection(current, 3, 1),
        _ => current.clamp(0, 2),
    }
}

/// Two columns, ending with Sound (paired with Benchmark/Clock's extra item).
pub(crate) fn moved_ingame_selection(current: i32, extra_row_item: bool, action: UiAction) -> i32 {
    if extra_row_item {
        let current = current.clamp(0, 5);
        match action {
            UiAction::Up => [4, 5, 0, 1, 2, 3][current as usize],
            UiAction::Down => [2, 3, 4, 5, 0, 1][current as usize],
            UiAction::Left | UiAction::Right => [1, 0, 3, 2, 5, 4][current as usize],
            _ => current,
        }
    } else {
        let current = current.clamp(0, 4);
        match action {
            UiAction::Up => [4, 4, 0, 1, 2][current as usize],
            UiAction::Down => [2, 3, 4, 4, 0][current as usize],
            UiAction::Left | UiAction::Right => [1, 0, 3, 2, 4][current as usize],
            _ => current,
        }
    }
}

pub(crate) fn moved_clock_selection(current: i32, action: UiAction) -> i32 {
    // Keep existing control indices stable: the event row is [2, 3, 7, 8, 11],
    // followed by preview/Crow [4, 14], Marquee toggle/recipe [9, 10], actions [5, 6].
    // Date (13) is beside Rain (12); Up/Down also visit it, since Left/Right
    // on a choice adjusts its value rather than moving focus.
    let current = current.clamp(0, 14) as usize;
    match action {
        UiAction::Up => [5, 0, 12, 12, 2, 9, 10, 12, 12, 14, 14, 13, 13, 1, 4][current],
        UiAction::Down => [1, 13, 4, 4, 14, 0, 0, 4, 4, 5, 6, 4, 2, 12, 9][current],
        UiAction::Left => [0, 1, 11, 2, 4, 6, 5, 3, 7, 10, 10, 8, 12, 13, 4][current],
        UiAction::Right => [0, 1, 3, 7, 4, 6, 5, 8, 11, 10, 10, 2, 12, 13, 4][current],
        _ => current as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_wraps_in_both_directions() {
        assert_eq!(moved_selection(0, 5, -1), 4);
        assert_eq!(moved_selection(4, 5, 1), 0);
        assert_eq!(moved_selection(2, 5, 1), 3);
    }

    #[test]
    fn clock_event_row_reaches_every_switch_without_changing_preview() {
        assert_eq!(moved_clock_selection(2, UiAction::Right), 3);
        assert_eq!(moved_clock_selection(3, UiAction::Right), 7);
        assert_eq!(moved_clock_selection(7, UiAction::Right), 8);
        assert_eq!(moved_clock_selection(8, UiAction::Right), 11);
        assert_eq!(moved_clock_selection(11, UiAction::Right), 2);
        assert_eq!(moved_clock_selection(7, UiAction::Left), 3);
        assert_eq!(moved_clock_selection(2, UiAction::Left), 11);
        assert_eq!(moved_clock_selection(11, UiAction::Left), 8);
        assert_eq!(moved_clock_selection(11, UiAction::Down), 4);
        assert_eq!(moved_clock_selection(11, UiAction::Up), 13);
        assert_eq!(moved_clock_selection(8, UiAction::Left), 7);
        assert_eq!(moved_clock_selection(8, UiAction::Down), 4);
        assert_eq!(moved_clock_selection(8, UiAction::Up), 12);
        assert_eq!(moved_clock_selection(7, UiAction::Down), 4);
        assert_eq!(moved_clock_selection(7, UiAction::Up), 12);
        assert_eq!(moved_clock_selection(1, UiAction::Down), 13);
        assert_eq!(moved_clock_selection(13, UiAction::Down), 12);
        assert_eq!(moved_clock_selection(13, UiAction::Up), 1);
        assert_eq!(moved_clock_selection(12, UiAction::Down), 2);
        assert_eq!(moved_clock_selection(12, UiAction::Up), 13);
        assert_eq!(moved_clock_selection(4, UiAction::Down), 14);
        assert_eq!(moved_clock_selection(14, UiAction::Down), 9);
        assert_eq!(moved_clock_selection(9, UiAction::Up), 14);
        assert_eq!(moved_clock_selection(14, UiAction::Up), 4);
        assert_eq!(moved_clock_selection(14, UiAction::Left), 4);
        assert_eq!(moved_clock_selection(14, UiAction::Right), 4);
        assert_eq!(moved_clock_selection(9, UiAction::Right), 10);
        assert_eq!(moved_clock_selection(10, UiAction::Down), 6);
        assert_eq!(moved_clock_selection(6, UiAction::Up), 10);
    }

    #[test]
    fn action_codes_round_trip() {
        for action in UiAction::ALL {
            assert_eq!(UiAction::from_code(action.code()), Some(action));
        }
        assert_eq!(UiAction::from_code(99), None);
    }

    #[test]
    fn launcher_navigation_matches_the_visible_grid() {
        assert_eq!(moved_launcher_selection(0, UiAction::Down), 1);
        assert_eq!(moved_launcher_selection(1, UiAction::Down), 2);
        assert_eq!(moved_launcher_selection(2, UiAction::Right), 3);
        assert_eq!(moved_launcher_selection(3, UiAction::Right), 5);
        assert_eq!(moved_launcher_selection(5, UiAction::Right), 2);
        assert_eq!(moved_launcher_selection(2, UiAction::Down), 4);
        assert_eq!(moved_launcher_selection(4, UiAction::Down), 0);
        assert_eq!(moved_launcher_selection(5, UiAction::Left), 3);
        assert_eq!(moved_match_launcher_selection(1, UiAction::Right), 6);
        assert_eq!(moved_match_launcher_selection(6, UiAction::Down), 5);
        assert_eq!(moved_match_launcher_selection(5, UiAction::Down), 4);
        assert_eq!(moved_match_launcher_selection(4, UiAction::Up), 2);
        assert_eq!(moved_match_launcher_selection(6, UiAction::Left), 1);
    }

    #[test]
    fn launcher_controls_navigation_reaches_touch_test() {
        assert_eq!(moved_launcher_controls_selection(0, UiAction::Right), 1);
        assert_eq!(moved_launcher_controls_selection(0, UiAction::Down), 1);
        assert_eq!(moved_launcher_controls_selection(1, UiAction::Right), 2);
        assert_eq!(moved_launcher_controls_selection(2, UiAction::Right), 0);
        assert_eq!(moved_launcher_controls_selection(0, UiAction::Left), 2);
    }

    #[test]
    fn pause_navigation_accounts_for_optional_benchmark() {
        assert_eq!(moved_ingame_selection(0, true, UiAction::Down), 2);
        assert_eq!(moved_ingame_selection(2, true, UiAction::Down), 4);
        assert_eq!(moved_ingame_selection(4, true, UiAction::Down), 0);
        assert_eq!(moved_ingame_selection(0, false, UiAction::Down), 2);
        assert_eq!(moved_ingame_selection(2, false, UiAction::Right), 3);
    }
}
