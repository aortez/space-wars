use engine_common::Action;

/// Continuous axes and held buttons. The scenario owns edge detection so
/// recordings, keyboard input and controllers share the same behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Controls {
    pub drive: f32,
    pub aim: f32,
    pub power: f32,
    pub fire: bool,
    pub select: bool,
    pub demo: bool,
    pub shape: bool,
    pub collapse: bool,
    pub barrage: bool,
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
    Collapse,
    Barrage,
    Bank,
    Strength,
    Drop,
    Hillside,
    Roof,
    DriveLeft,
    DriveRight,
    CpuDrive,
    Suspension,
    Course,
    CpuAim,
}
impl Command {
    pub const ALL: [Self; 22] = [
        Self::AimDown,
        Self::AimUp,
        Self::PowerDown,
        Self::PowerUp,
        Self::Fire,
        Self::Select,
        Self::Demo,
        Self::Shape,
        Self::Reset,
        Self::Collapse,
        Self::Barrage,
        Self::Bank,
        Self::Strength,
        Self::Drop,
        Self::Hillside,
        Self::Roof,
        Self::DriveLeft,
        Self::DriveRight,
        Self::CpuDrive,
        Self::Suspension,
        Self::Course,
        Self::CpuAim,
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
                    &[
                        c.fire as u8,
                        c.select as u8,
                        c.demo as u8,
                        c.shape as u8,
                        c.collapse as u8,
                        c.barrage as u8,
                    ],
                    c.drive.to_le_bytes().as_slice(),
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
            1 if [12, 14, 18].contains(&payload.len())
                && payload[8..payload.len().min(14)].iter().all(|v| *v <= 1) =>
            {
                let aim = f32::from_le_bytes(payload[0..4].try_into().ok()?);
                let power = f32::from_le_bytes(payload[4..8].try_into().ok()?);
                let drive = if payload.len() == 18 {
                    f32::from_le_bytes(payload[14..18].try_into().ok()?)
                } else {
                    0.0
                };
                (aim.is_finite() && power.is_finite() && drive.is_finite()).then_some(
                    Self::Controls(Controls {
                        drive: drive.clamp(-1.0, 1.0),
                        aim: aim.clamp(-1.0, 1.0),
                        power: power.clamp(-1.0, 1.0),
                        fire: payload[8] != 0,
                        select: payload[9] != 0,
                        demo: payload[10] != 0,
                        shape: payload[11] != 0,
                        collapse: payload.get(12) == Some(&1),
                        barrage: payload.get(13) == Some(&1),
                    }),
                )
            }
            2 if payload.len() == 1 => Command::ALL
                .get(payload[0] as usize)
                .copied()
                .map(Self::Command),
            _ => None,
        }
    }
}
