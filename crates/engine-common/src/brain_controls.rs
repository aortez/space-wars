//! Versioned control and observation contracts carried by a saved brain.
use serde::{Deserialize, Serialize};

pub const CREATURE_PROFILE: &str = "creature_controls_v1";
pub const CREATURE_SENSES: &str = "creature_polar_v1";
pub const CREATURE_SLOTS: [&str; 4] = ["move", "jump", "run", "dive"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrainBinding {
    pub profile: String,
    pub senses: String,
    pub required_controls: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct BodyProfile<'a> {
    pub profile: &'a str,
    pub senses: &'a str,
    pub controls: &'a [&'a str],
}

impl BodyProfile<'static> {
    pub const fn duck() -> Self {
        Self {
            profile: CREATURE_PROFILE,
            senses: CREATURE_SENSES,
            controls: &CREATURE_SLOTS,
        }
    }
}

impl BrainBinding {
    pub fn creature() -> Self {
        Self {
            profile: CREATURE_PROFILE.into(),
            senses: CREATURE_SENSES.into(),
            required_controls: vec!["move".into(), "jump".into()],
        }
    }

    pub fn bind(&self, body: &BodyProfile<'_>) -> Result<(), String> {
        if self.profile != body.profile {
            return Err(format!("Body needs control profile {}", self.profile));
        }
        if self.senses != body.senses {
            return Err(format!("Body needs observation schema {}", self.senses));
        }
        for slot in &self.required_controls {
            if !body.controls.contains(&slot.as_str()) {
                return Err(format!("Body lacks required control {slot}"));
            }
        }
        Ok(())
    }
}

/// Held movement along local +x (right on the Clock) and three held buttons.
/// Each body preserves its ordinary press and release behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureControls {
    pub move_milli: i16,
    pub jump: bool,
    pub run: bool,
    pub dive: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CreatureDecision {
    pub controls: CreatureControls,
    pub invalid: bool,
}

impl CreatureControls {
    /// The four signed outputs are post-activation. Zero releases a button.
    /// Reject the entire decision on a malformed slot, including optional ones.
    pub fn decode(output: &[f32], body: &BodyProfile<'_>) -> CreatureDecision {
        if body.profile != CREATURE_PROFILE
            || output.len() != CREATURE_SLOTS.len()
            || output.iter().any(|value| !(-1.0..=1.0).contains(value))
        {
            return CreatureDecision {
                invalid: true,
                ..Default::default()
            };
        }
        CreatureDecision {
            controls: Self {
                move_milli: (output[0] * 1000.0).round() as i16,
                jump: output[1] > 0.0,
                run: output[2] > 0.0 && body.controls.contains(&"run"),
                dive: output[3] > 0.0 && body.controls.contains(&"dive"),
            },
            invalid: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_validates_all_slots_and_ignores_absent_optional_buttons() {
        let duck = BodyProfile::duck();
        assert_eq!(
            CreatureControls::decode(&[0.; 4], &duck),
            CreatureDecision::default()
        );
        let controls = CreatureControls::decode(&[-1., 1., -1., 1.], &duck);
        assert_eq!(
            controls.controls,
            CreatureControls {
                move_milli: -1000,
                jump: true,
                run: false,
                dive: true,
            }
        );
        assert!(!controls.invalid);
        for invalid in [
            &[0., 1., f32::NAN, 1.][..],
            &[0., 1., 1., f32::INFINITY],
            &[1.1, 1., 1., 1.],
            &[1., 1., 1.],
        ] {
            let decision = CreatureControls::decode(invalid, &duck);
            assert!(decision.invalid);
            assert_eq!(decision.controls, CreatureControls::default());
        }
        let walker = BodyProfile {
            controls: &["move", "jump"],
            ..duck
        };
        let decoded = CreatureControls::decode(&[1., 1., 1., 1.], &walker);
        assert_eq!(
            decoded.controls,
            CreatureControls {
                move_milli: 1000,
                jump: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn transfer_checks_profile_senses_and_required_slots() {
        let duck = BodyProfile::duck();
        let walker = BodyProfile {
            controls: &["jump", "move"],
            ..duck
        };
        let mut binding = BrainBinding::creature();
        assert!(binding.bind(&walker).is_ok());
        assert!(
            binding
                .bind(&BodyProfile {
                    profile: "ship",
                    ..duck
                })
                .is_err()
        );
        assert!(
            binding
                .bind(&BodyProfile {
                    senses: "pixels",
                    ..duck
                })
                .is_err()
        );
        binding.required_controls.push("run".into());
        assert_eq!(
            binding.bind(&walker).unwrap_err(),
            "Body lacks required control run"
        );
        assert!(binding.bind(&duck).is_ok());
    }
}
