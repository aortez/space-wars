//! Pure input translation and release-to-bind calibration, independent of UI
//! timing, gilrs handles, settings I/O, and individual scenarios.

use std::time::{Duration, Instant};

use engine_common::{
    ControllerBinding, ControllerControl as Control, ControllerProfile, ControllerSource as Source,
};

use crate::input::GamepadSeatInput;

pub(super) const HOLD_TO_SKIP: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Default)]
pub(super) struct RawState {
    pub buttons: Vec<(u32, f32)>,
    /// Analog button values are separate from press/release hysteresis so a
    /// mapped trigger retains its full range, including partial pressure.
    pub button_values: Vec<(u32, f32)>,
    /// Only centered stick/hat axes, never one-sided trigger axes.
    pub axes: Vec<(u32, f32)>,
}

impl RawState {
    pub fn value(&self, source: Source) -> f32 {
        let (entries, code, sign) = match source {
            Source::Button { code } => (&self.buttons, code, 1.0),
            Source::AxisPositive { code } => (&self.axes, code, 1.0),
            Source::AxisNegative { code } => (&self.axes, code, -1.0),
        };
        entries
            .iter()
            .find(|(candidate, _)| *candidate == code)
            .map_or(0.0, |(_, value)| (value * sign).clamp(0.0, 1.0))
    }

    pub fn neutral(&self) -> bool {
        self.buttons.iter().all(|(_, value)| *value < 0.25)
            && self.button_values.iter().all(|(_, value)| *value < 0.25)
            && self.axes.iter().all(|(_, value)| value.abs() < 0.25)
    }

    fn trigger_value(&self, source: Source) -> f32 {
        if let Source::Button { code } = source
            && let Some((_, value)) = self
                .button_values
                .iter()
                .find(|(candidate, _)| *candidate == code)
        {
            return value.clamp(0.0, 1.0);
        }
        self.value(source)
    }

    fn candidates(&self, direction: bool) -> Vec<Source> {
        let mut candidates = self
            .buttons
            .iter()
            .filter(|(_, value)| *value >= 0.65)
            .map(|(code, _)| Source::Button { code: *code })
            .collect::<Vec<_>>();
        if direction {
            candidates.extend(self.axes.iter().filter_map(|(code, value)| {
                if *value >= 0.65 {
                    Some(Source::AxisPositive { code: *code })
                } else if *value <= -0.65 {
                    Some(Source::AxisNegative { code: *code })
                } else {
                    None
                }
            }));
        }
        candidates
    }
}

pub(super) fn remap(
    original: GamepadSeatInput,
    profile: &ControllerProfile,
    raw: &RawState,
    stick_codes: [Option<u32>; 4],
) -> GamepadSeatInput {
    let axes = [
        original.left_stick_x,
        original.left_stick_y,
        original.right_stick_x,
        original.right_stick_y,
    ];
    // Leave unrelated analog sticks alone. A stick axis used for remapped
    // directions must not also feed its old, potentially reversed direction.
    let axes = std::array::from_fn::<_, 4, _>(|i| {
        if stick_codes[i].is_some_and(|code| {
            profile
                .bindings
                .iter()
                .any(|b| b.source.axis_code() == Some(code))
        }) {
            0.0
        } else {
            axes[i]
        }
    });
    let mut result = GamepadSeatInput {
        connected: original.connected,
        name: original.name,
        left_stick_x: axes[0],
        left_stick_y: axes[1],
        right_stick_x: axes[2],
        right_stick_y: axes[3],
        ..Default::default()
    };
    for binding in &profile.bindings {
        let value = raw.value(binding.source);
        let pressed = value >= 0.5;
        match binding.control {
            Control::Up => result.dpad_up = pressed,
            Control::Down => result.dpad_down = pressed,
            Control::Left => result.dpad_left = pressed,
            Control::Right => result.dpad_right = pressed,
            Control::South => result.south = pressed,
            Control::East => result.east = pressed,
            Control::West => result.west = pressed,
            Control::North => result.north = pressed,
            Control::LeftBumper => result.left_bumper = pressed,
            Control::RightBumper => result.right_bumper = pressed,
            Control::Start => result.start = pressed,
            Control::Select => result.select = pressed,
            Control::LeftTrigger => result.left_trigger = raw.trigger_value(binding.source),
            Control::RightTrigger => result.right_trigger = raw.trigger_value(binding.source),
        }
    }
    result
}

pub(super) fn button_control(profile: &ControllerProfile, code: u32) -> Option<Control> {
    profile
        .bindings
        .iter()
        .find(|b| b.source == Source::Button { code })
        .map(|b| b.control)
}

#[derive(Debug)]
pub(super) struct Capture {
    pub index: usize,
    pub bindings: Vec<ControllerBinding>,
    pub message: String,
    pub cancelled: bool,
    ready: bool,
    pending: Option<(Source, Instant)>,
}

impl Capture {
    pub fn new() -> Self {
        Self {
            index: 0,
            bindings: Vec::new(),
            message: String::new(),
            cancelled: false,
            ready: false,
            pending: None,
        }
    }

    pub fn target(&self) -> Option<Control> {
        Control::ALL.get(self.index).copied()
    }

    pub fn skip(&mut self) {
        if self.target().is_some_and(|target| !target.required()) {
            self.advance(None);
        }
    }

    fn advance(&mut self, source: Option<Source>) {
        if let (Some(control), Some(source)) = (self.target(), source) {
            self.bindings.push(ControllerBinding { control, source });
        }
        self.index += 1;
        self.ready = false;
        self.pending = None;
        self.message.clear();
    }

    pub fn update(&mut self, raw: &RawState, now: Instant) {
        let Some(target) = self.target() else { return };
        if !self.ready {
            self.ready = raw.neutral();
            return;
        }
        if let Some((source, since)) = self.pending {
            if raw
                .candidates(target.direction())
                .iter()
                .any(|candidate| *candidate != source)
            {
                self.pending = None;
                self.ready = false;
                self.message = "One input at a time; release everything and try again.".into();
                return;
            }
            if now.duration_since(since) >= HOLD_TO_SKIP && raw.value(source) >= 0.5 {
                if target.required() {
                    self.cancelled = true;
                } else {
                    self.skip();
                }
            } else if raw.neutral() {
                if self.bindings.iter().any(|binding| binding.source == source) {
                    self.pending = None;
                    self.message =
                        "Already assigned. Hold 2 seconds to skip, or choose another button."
                            .into();
                } else {
                    self.advance(Some(source));
                }
            }
            return;
        }
        let candidates = raw.candidates(target.direction());
        if candidates.len() > 1 {
            self.message = "One input at a time; release everything and try again.".into();
            self.ready = false;
        } else if let Some(source) = candidates.first().copied() {
            if self.bindings.iter().any(|binding| binding.source == source) {
                // Holding an already assigned button is still a way to skip
                // absent optional controls on small arcade / NES controllers.
                if !target.required() {
                    self.pending = Some((source, now));
                    self.message = "Already assigned. Hold 2 seconds to skip, then release.".into();
                } else {
                    self.message =
                        "Already assigned; choose another input, or hold 2s to cancel.".into();
                    self.pending = Some((source, now));
                }
            } else {
                self.pending = Some((source, now));
                self.message = "Release to bind. Hold 2 seconds to skip optional buttons.".into();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(code: u32) -> RawState {
        RawState {
            buttons: vec![(code, 1.0)],
            ..Default::default()
        }
    }

    #[test]
    fn capture_waits_for_neutral_and_release_and_rejects_duplicates() {
        let now = Instant::now();
        let mut capture = Capture::new();
        capture.update(&raw(10), now); // The press that opened the wizard.
        capture.update(&RawState::default(), now);
        assert_eq!(capture.index, 0);
        capture.update(&raw(10), now);
        assert_eq!(capture.index, 0);
        capture.update(&RawState::default(), now);
        assert_eq!(capture.index, 1);
        capture.update(&RawState::default(), now);
        capture.update(&raw(10), now);
        capture.update(&RawState::default(), now);
        assert_eq!(capture.index, 1);
    }

    #[test]
    fn both_cabinet_permutations_produce_same_edges_and_holds() {
        // These are observed evdev button codes (gilrs codes are opaque in
        // production); both cabinets can assign the same physical positions.
        for codes in [
            [308, 305, 307, 304, 310, 311],
            [304, 305, 308, 307, 310, 311],
        ] {
            let targets = [
                Control::South,
                Control::East,
                Control::West,
                Control::North,
                Control::LeftBumper,
                Control::RightBumper,
            ];
            let profile = ControllerProfile {
                device_key: "test".into(),
                name: "test".into(),
                bindings: targets
                    .into_iter()
                    .zip(codes)
                    .map(|(control, code)| ControllerBinding {
                        control,
                        source: Source::Button { code },
                    })
                    .collect(),
            };
            for (index, code) in codes.into_iter().enumerate() {
                assert_eq!(button_control(&profile, code), Some(targets[index]));
                let mapped = remap(GamepadSeatInput::default(), &profile, &raw(code), [None; 4]);
                let states = [
                    mapped.south,
                    mapped.east,
                    mapped.west,
                    mapped.north,
                    mapped.left_bumper,
                    mapped.right_bumper,
                ];
                assert_eq!(states.into_iter().filter(|v| *v).count(), 1);
                assert!(states[index]);
                assert_eq!(
                    remap(
                        GamepadSeatInput::default(),
                        &profile,
                        &RawState::default(),
                        [None; 4]
                    ),
                    GamepadSeatInput::default()
                );
            }
        }
    }

    #[test]
    fn signed_axes_remap_without_leaking_the_old_stick_direction() {
        let profile = ControllerProfile {
            device_key: "test".into(),
            name: "test".into(),
            bindings: vec![
                ControllerBinding {
                    control: Control::Left,
                    source: Source::AxisPositive { code: 1 },
                },
                ControllerBinding {
                    control: Control::Right,
                    source: Source::AxisNegative { code: 1 },
                },
            ],
        };
        for (value, left) in [(1.0, true), (-1.0, false)] {
            let raw = RawState {
                axes: vec![(1, value)],
                ..Default::default()
            };
            let original = GamepadSeatInput {
                left_stick_x: value,
                right_stick_y: 0.8,
                ..Default::default()
            };
            let mapped = remap(
                original,
                &profile,
                &raw,
                [Some(1), Some(2), Some(3), Some(4)],
            );
            assert_eq!(mapped.dpad_left, left);
            assert_eq!(mapped.dpad_right, !left);
            assert_eq!(mapped.left_stick_x, 0.0);
            assert_eq!(mapped.right_stick_y, 0.8);
        }
    }

    #[test]
    fn short_duplicate_press_never_binds_twice_and_long_hold_skips_optional() {
        let now = Instant::now();
        let mut capture = Capture::new();
        capture.index = 6;
        capture.bindings.push(ControllerBinding {
            control: Control::South,
            source: Source::Button { code: 1 },
        });
        capture.update(&RawState::default(), now);
        capture.update(&raw(1), now);
        capture.update(&RawState::default(), now + Duration::from_millis(100));
        assert_eq!(capture.index, 6);
        assert_eq!(capture.bindings.len(), 1);
        capture.update(&raw(1), now);
        capture.update(&raw(1), now + HOLD_TO_SKIP);
        assert_eq!(capture.index, 7);
        capture.update(&raw(1), now + HOLD_TO_SKIP * 3);
        assert_eq!(capture.index, 7, "hold cannot skip multiple prompts");
        assert_eq!(capture.bindings.len(), 1);
    }

    #[test]
    fn required_input_long_hold_cancels_without_committing() {
        let now = Instant::now();
        let mut capture = Capture::new();
        capture.update(&RawState::default(), now);
        capture.update(&raw(1), now);
        capture.update(&raw(1), now + HOLD_TO_SKIP);
        assert!(capture.cancelled);
        assert!(capture.bindings.is_empty());
    }

    #[test]
    fn diagonal_and_multiple_buttons_are_not_arbitrarily_assigned() {
        let now = Instant::now();
        let mut capture = Capture::new();
        capture.update(&RawState::default(), now);
        capture.update(
            &RawState {
                axes: vec![(1, 1.0), (2, -1.0)],
                ..Default::default()
            },
            now,
        );
        capture.update(
            &RawState {
                axes: vec![(1, 1.0)],
                ..Default::default()
            },
            now,
        );
        capture.update(&RawState::default(), now);
        assert_eq!(capture.index, 0);
        // Real event streams deliver these presses sequentially, not as one
        // conveniently simultaneous snapshot. That must reject the chord too.
        capture.update(&raw(1), now);
        capture.update(
            &RawState {
                buttons: vec![(1, 1.0), (2, 1.0)],
                ..Default::default()
            },
            now,
        );
        capture.update(&RawState::default(), now);
        assert_eq!(capture.index, 0);
        capture.update(
            &RawState {
                buttons: vec![(1, 1.0), (2, 1.0)],
                ..Default::default()
            },
            now,
        );
        capture.update(&RawState::default(), now);
        assert_eq!(capture.index, 0);
    }

    #[test]
    fn skipped_buttons_are_disabled_and_unmapped_analog_axes_are_preserved() {
        let original = GamepadSeatInput {
            south: true,
            east: true,
            left_stick_x: 0.75,
            ..Default::default()
        };
        let profile = ControllerProfile {
            device_key: "test".into(),
            name: "test".into(),
            bindings: vec![],
        };
        let mapped = remap(original, &profile, &raw(1), [None; 4]);
        assert!(!mapped.south && !mapped.east);
        assert_eq!(mapped.left_stick_x, 0.75);
        assert_eq!(button_control(&profile, 1), None);
    }

    #[test]
    fn analog_trigger_pressure_is_preserved_below_its_digital_press_threshold() {
        let profile = ControllerProfile {
            device_key: "test".into(),
            name: "test".into(),
            bindings: vec![ControllerBinding {
                control: Control::RightTrigger,
                source: Source::Button { code: 1 },
            }],
        };
        let raw = RawState {
            buttons: vec![(1, 0.0)],
            button_values: vec![(1, 0.3)],
            ..Default::default()
        };
        let mapped = remap(GamepadSeatInput::default(), &profile, &raw, [None; 4]);
        assert_eq!(mapped.right_trigger, 0.3);
    }
}
