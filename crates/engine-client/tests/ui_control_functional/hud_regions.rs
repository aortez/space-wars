//! Broad semantic regions for the desktop screenshots, independent of the
//! production layout calculation. Responsive/device geometry has separate tests.

fn outside_distance(x: usize, width: usize, player: usize) -> usize {
    if player == 0 { x } else { width - 1 - x }
}

pub(super) fn minimap(x: usize, y: usize, width: usize, height: usize, player: usize) -> bool {
    y >= height.saturating_sub(160) && outside_distance(x, width, player) < 155
}

pub(super) fn vitals(x: usize, y: usize, width: usize, height: usize, player: usize) -> bool {
    y > height.saturating_sub(145)
        && y < height.saturating_sub(25)
        && (155..345).contains(&outside_distance(x, width, player))
}

pub(super) fn prompt(x: usize, y: usize, width: usize, height: usize) -> bool {
    // The single-player landing fixture always has a contextual landing prompt.
    (40..height / 4).contains(&y) && x.abs_diff(width / 2) < 125
}

#[test]
fn minimaps_are_outside_bottom_corners_not_the_old_upper_right_positions() {
    assert!(minimap(80, 640, 1280, 720, 0));
    assert!(minimap(1200, 640, 1280, 720, 1));
    assert!(!minimap(1200, 640, 1280, 720, 0));
    assert!(!minimap(80, 640, 1280, 720, 1));
    assert!(!minimap(1100, 250, 1280, 720, 1));
    // Cyan energy meters must not satisfy the minimap-planet check.
    assert!(!minimap(200, 660, 1280, 720, 0));
    assert!(!minimap(1050, 660, 1280, 720, 1));
}

#[test]
fn vitals_exclude_the_radars_world_center_and_old_top_strips() {
    assert!(vitals(200, 630, 1280, 720, 0));
    assert!(vitals(1050, 630, 1280, 720, 1));
    assert!(!vitals(80, 640, 1280, 720, 0));
    assert!(!vitals(640, 640, 1280, 720, 0));
    assert!(!vitals(1050, 630, 1280, 720, 0));
    assert!(!vitals(200, 70, 1280, 720, 0));
}

#[test]
fn landing_prompt_is_top_center_not_world_or_bottom_instruments() {
    assert!(prompt(640, 60, 1280, 720));
    assert!(!prompt(80, 60, 1280, 720));
    assert!(!prompt(640, 360, 1280, 720));
    assert!(!prompt(640, 650, 1280, 720));
}
