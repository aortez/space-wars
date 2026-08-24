//! Backend-neutral menu actions shared by keyboards and gamepads.

use spacewars_control::UiAction;

pub(crate) fn moved_selection(current: i32, item_count: i32, delta: i32) -> i32 {
    if item_count <= 0 {
        return 0;
    }
    (current.clamp(0, item_count - 1) + delta).rem_euclid(item_count)
}

pub(crate) fn moved_launcher_selection(current: i32, action: UiAction) -> i32 {
    let current = current.clamp(0, 4);
    match action {
        UiAction::Up => [3, 0, 0, 1, 2][current as usize],
        UiAction::Down => [1, 3, 4, 0, 0][current as usize],
        UiAction::Left | UiAction::Right => [0, 2, 1, 4, 3][current as usize],
        _ => current,
    }
}

pub(crate) fn moved_launcher_controls_selection(current: i32, action: UiAction) -> i32 {
    match action {
        UiAction::Up | UiAction::Left => moved_selection(current, 3, -1),
        UiAction::Down | UiAction::Right => moved_selection(current, 3, 1),
        _ => current.clamp(0, 2),
    }
}

pub(crate) fn moved_ingame_selection(
    current: i32,
    benchmark_available: bool,
    action: UiAction,
) -> i32 {
    if benchmark_available {
        let current = current.clamp(0, 4);
        match action {
            UiAction::Up => [4, 4, 0, 1, 2][current as usize],
            UiAction::Down => [2, 3, 4, 4, 0][current as usize],
            UiAction::Left | UiAction::Right => [1, 0, 3, 2, 4][current as usize],
            _ => current,
        }
    } else {
        let current = current.clamp(0, 3);
        match action {
            UiAction::Up | UiAction::Down => [2, 3, 0, 1][current as usize],
            UiAction::Left | UiAction::Right => [1, 0, 3, 2][current as usize],
            _ => current,
        }
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
    fn action_codes_round_trip() {
        for action in UiAction::ALL {
            assert_eq!(UiAction::from_code(action.code()), Some(action));
        }
        assert_eq!(UiAction::from_code(99), None);
    }

    #[test]
    fn launcher_navigation_matches_the_visible_grid() {
        assert_eq!(moved_launcher_selection(0, UiAction::Down), 1);
        assert_eq!(moved_launcher_selection(1, UiAction::Right), 2);
        assert_eq!(moved_launcher_selection(2, UiAction::Down), 4);
        assert_eq!(moved_launcher_selection(4, UiAction::Left), 3);
        assert_eq!(moved_launcher_selection(3, UiAction::Down), 0);
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
