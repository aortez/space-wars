use engine_common::Action;

/// Continuous axes and held buttons. The scenario owns edge detection so
/// recordings, keyboard input and controllers share the same behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Controls {
    pub aim: f32,
    pub power: f32,
    pub fire: bool,
    pub select: bool,
    pub demo: bool,
    pub shape: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    AimDown,
    AimUp,
    PowerDown,
    PowerUp,
    Fire,
    Select,
    Demo,
    Shape,
    Reset,
}
impl Command {
    pub const ALL: [Self; 9] = [
        Self::AimDown,
        Self::AimUp,
        Self::PowerDown,
        Self::PowerUp,
        Self::Fire,
        Self::Select,
        Self::Demo,
        Self::Shape,
        Self::Reset,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScorchedAction {
    Controls(Controls),
    Command(Command),
}
impl ScorchedAction {
    pub fn encode(self) -> Action {
        match self {
            Self::Controls(c) => Action::scenario(
                1,
                [
                    c.aim.to_le_bytes().as_slice(),
                    c.power.to_le_bytes().as_slice(),
                    &[c.fire as u8, c.select as u8, c.demo as u8, c.shape as u8],
                ]
                .concat(),
            ),
            Self::Command(c) => Action::scenario(2, vec![c as u8]),
        }
    }

    pub fn decode(action: &Action) -> Option<Self> {
        let Action::Scenario { kind, payload } = action else {
            return None;
        };
        match *kind {
            1 if payload.len() == 12 && payload[8..].iter().all(|v| *v <= 1) => {
                let aim = f32::from_le_bytes(payload[0..4].try_into().ok()?);
                let power = f32::from_le_bytes(payload[4..8].try_into().ok()?);
                (aim.is_finite() && power.is_finite()).then_some(Self::Controls(Controls {
                    aim: aim.clamp(-1.0, 1.0),
                    power: power.clamp(-1.0, 1.0),
                    fire: payload[8] != 0,
                    select: payload[9] != 0,
                    demo: payload[10] != 0,
                    shape: payload[11] != 0,
                }))
            }
            2 if payload.len() == 1 => Command::ALL
                .get(payload[0] as usize)
                .copied()
                .map(Self::Command),
            _ => None,
        }
    }
}
