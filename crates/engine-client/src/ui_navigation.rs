//! Backend-neutral menu actions shared by keyboards and gamepads.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub(crate) enum UiAction {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
    Confirm = 4,
    Back = 5,
    Start = 6,
    Controls = 7,
}

impl UiAction {
    pub(crate) const fn code(self) -> i32 {
        self as i32
    }

    pub(crate) const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Up),
            1 => Some(Self::Down),
            2 => Some(Self::Left),
            3 => Some(Self::Right),
            4 => Some(Self::Confirm),
            5 => Some(Self::Back),
            6 => Some(Self::Start),
            7 => Some(Self::Controls),
            _ => None,
        }
    }
}

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
        for action in [
            UiAction::Up,
            UiAction::Down,
            UiAction::Left,
            UiAction::Right,
            UiAction::Confirm,
            UiAction::Back,
            UiAction::Start,
            UiAction::Controls,
        ] {
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
    fn pause_navigation_accounts_for_optional_benchmark() {
        assert_eq!(moved_ingame_selection(0, true, UiAction::Down), 2);
        assert_eq!(moved_ingame_selection(2, true, UiAction::Down), 4);
        assert_eq!(moved_ingame_selection(4, true, UiAction::Down), 0);
        assert_eq!(moved_ingame_selection(0, false, UiAction::Down), 2);
        assert_eq!(moved_ingame_selection(2, false, UiAction::Right), 3);
    }
}
