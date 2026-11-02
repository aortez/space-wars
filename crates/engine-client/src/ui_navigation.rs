//! Backend-neutral menu actions shared by keyboards and gamepads.

use spacewars_control::UiAction;

pub(crate) fn moved_selection(current: i32, item_count: i32, delta: i32) -> i32 {
    if item_count <= 0 {
        return 0;
    }
    (current.clamp(0, item_count - 1) + delta).rem_euclid(item_count)
}

pub(crate) fn moved_launcher_selection(current: i32, action: UiAction) -> i32 {
    if let Some(next) = show_all_selection(current, action) {
        return next;
    }
    if let Some(next) = training_footer_selection(current, action) {
        return next;
    }
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
    if let Some(next) = show_all_selection(current, action) {
        return next;
    }
    if let Some(next) = training_footer_selection(current, action) {
        return next;
    }
    let current = current.clamp(0, 6) as usize;
    match action {
        UiAction::Up => [4, 0, 1, 1, 2, 6, 0][current],
        UiAction::Down => [1, 2, 4, 4, 0, 4, 5][current],
        UiAction::Left => [0, 6, 5, 2, 4, 3, 1][current],
        UiAction::Right => [0, 6, 3, 5, 4, 2, 1][current],
        _ => current as i32,
    }
}

// The new control keeps its own index so existing API focus meanings stay stable.
fn show_all_selection(current: i32, action: UiAction) -> Option<i32> {
    match (current, action) {
        (0, UiAction::Down) | (1 | 6, UiAction::Up) => Some(9),
        (9, UiAction::Up) => Some(0),
        (9, UiAction::Down) => Some(1),
        (9, _) => Some(9),
        _ => None,
    }
}

fn training_footer_selection(current: i32, action: UiAction) -> Option<i32> {
    if current == 7 || current == 8 {
        Some(match action {
            UiAction::Up => 5,
            UiAction::Down => 0,
            UiAction::Left => {
                if current == 7 {
                    4
                } else {
                    7
                }
            }
            UiAction::Right => {
                if current == 7 {
                    8
                } else {
                    4
                }
            }
            _ => current,
        })
    } else if current == 4 && matches!(action, UiAction::Left | UiAction::Right) {
        Some(if action == UiAction::Left { 8 } else { 7 })
    } else {
        None
    }
}

pub(crate) fn moved_launcher_controls_selection(current: i32, action: UiAction) -> i32 {
    match action {
        UiAction::Up | UiAction::Left => moved_selection(current, 3, -1),
        UiAction::Down | UiAction::Right => moved_selection(current, 3, 1),
        _ => current.clamp(0, 2),
    }
}

pub(crate) fn moved_pizza_selection(current: i32, action: UiAction) -> i32 {
    match (current, action) {
        (0 | 1, UiAction::Up) | (4 | 5, UiAction::Down) => 6,
        (6, UiAction::Up) => 4,
        (6, UiAction::Down) => 0,
        (6, _) => 6,
        _ => moved_ingame_selection(current, true, action),
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
    // followed by preview/Crow/Explosion [4, 14, 15], Marquee/recipe [9, 10], actions [5, 6].
    // Rain (12), Date (13), Cell Swap (18), Grass (19) and Slot Machine (20)
    // share a row. Up/Down visit each; Left/Right on Rain continues to adjust
    // its amount. Slot Machine sits at the end of that row, so Left reaches
    // Grass and Right wraps to Rain, as Grass itself already did. Spider (21)
    // sits between Aquarium (17) and the recipe (10) in the Marquee row.
    let current = current.clamp(0, 21) as usize;
    match action {
        UiAction::Up => [
            5, 0, 12, 12, 2, 9, 10, 12, 12, 17, 17, 18, 18, 16, 4, 14, 1, 15, 13, 18, 19, 15,
        ][current],
        UiAction::Down => [
            1, 16, 4, 4, 14, 0, 0, 4, 4, 5, 6, 4, 2, 18, 15, 17, 13, 9, 12, 2, 2, 6,
        ][current],
        UiAction::Left => [
            0, 1, 11, 2, 4, 6, 5, 3, 7, 10, 21, 8, 12, 13, 4, 14, 1, 9, 13, 18, 19, 17,
        ][current],
        UiAction::Right => [
            0, 1, 3, 7, 4, 6, 5, 8, 11, 17, 10, 2, 12, 13, 15, 4, 0, 21, 19, 20, 12, 10,
        ][current],
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
        assert_eq!(moved_clock_selection(11, UiAction::Up), 18);
        assert_eq!(moved_clock_selection(8, UiAction::Left), 7);
        assert_eq!(moved_clock_selection(8, UiAction::Down), 4);
        assert_eq!(moved_clock_selection(8, UiAction::Up), 12);
        assert_eq!(moved_clock_selection(7, UiAction::Down), 4);
        assert_eq!(moved_clock_selection(7, UiAction::Up), 12);
        assert_eq!(moved_clock_selection(1, UiAction::Down), 16);
        assert_eq!(moved_clock_selection(16, UiAction::Down), 13);
        assert_eq!(moved_clock_selection(13, UiAction::Down), 18);
        assert_eq!(moved_clock_selection(18, UiAction::Down), 12);
        assert_eq!(moved_clock_selection(18, UiAction::Up), 13);
        assert_eq!(moved_clock_selection(13, UiAction::Up), 16);
        assert_eq!(moved_clock_selection(12, UiAction::Down), 2);
        assert_eq!(moved_clock_selection(2, UiAction::Up), 12);
        assert_eq!(moved_clock_selection(19, UiAction::Right), 20);
        assert_eq!(moved_clock_selection(20, UiAction::Left), 19);
        assert_eq!(moved_clock_selection(20, UiAction::Right), 12);
        assert_eq!(moved_clock_selection(20, UiAction::Up), 19);
        assert_eq!(moved_clock_selection(20, UiAction::Down), 2);
        assert_eq!(moved_clock_selection(12, UiAction::Up), 18);
        assert_eq!(moved_clock_selection(4, UiAction::Down), 14);
        assert_eq!(moved_clock_selection(14, UiAction::Down), 15);
        assert_eq!(moved_clock_selection(15, UiAction::Down), 17);
        assert_eq!(moved_clock_selection(17, UiAction::Down), 9);
        assert_eq!(moved_clock_selection(9, UiAction::Up), 17);
        assert_eq!(moved_clock_selection(17, UiAction::Up), 15);
        assert_eq!(moved_clock_selection(15, UiAction::Up), 14);
        assert_eq!(moved_clock_selection(14, UiAction::Up), 4);
        assert_eq!(moved_clock_selection(14, UiAction::Left), 4);
        assert_eq!(moved_clock_selection(14, UiAction::Right), 15);
        assert_eq!(moved_clock_selection(9, UiAction::Right), 17);
        assert_eq!(moved_clock_selection(17, UiAction::Right), 21);
        assert_eq!(moved_clock_selection(21, UiAction::Right), 10);
        assert_eq!(moved_clock_selection(21, UiAction::Left), 17);
        assert_eq!(moved_clock_selection(10, UiAction::Left), 21);
        assert_eq!(moved_clock_selection(21, UiAction::Up), 15);
        assert_eq!(moved_clock_selection(21, UiAction::Down), 6);
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
        for navigate in [moved_launcher_selection, moved_match_launcher_selection] {
            assert_eq!(navigate(4, UiAction::Right), 7);
            assert_eq!(navigate(7, UiAction::Left), 4);
            assert_eq!(navigate(7, UiAction::Up), 5);
            assert_eq!(navigate(7, UiAction::Down), 0);
        }
        assert_eq!(moved_launcher_selection(0, UiAction::Down), 9);
        assert_eq!(moved_launcher_selection(9, UiAction::Down), 1);
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
