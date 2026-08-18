/// Buttons in the order shifted by an NES controller: A, B, Select, Start,
/// Up, Down, Left, Right.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControllerButtons(u8);

impl ControllerButtons {
    pub const NONE: Self = Self(0);
    pub const A: Self = Self(1 << 0);
    pub const B: Self = Self(1 << 1);
    pub const SELECT: Self = Self(1 << 2);
    pub const START: Self = Self(1 << 3);
    pub const UP: Self = Self(1 << 4);
    pub const DOWN: Self = Self(1 << 5);
    pub const LEFT: Self = Self(1 << 6);
    pub const RIGHT: Self = Self(1 << 7);

    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOr for ControllerButtons {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for ControllerButtons {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// One physical controller and its emulated serial latch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControllerPort {
    buttons: ControllerButtons,
    shift_register: u8,
    strobe: bool,
}

impl ControllerPort {
    pub fn buttons(&self) -> ControllerButtons {
        self.buttons
    }

    pub fn set_buttons(&mut self, buttons: ControllerButtons) {
        self.buttons = buttons;
        if self.strobe {
            self.latch();
        }
    }

    pub fn strobe(&self) -> bool {
        self.strobe
    }

    /// Applies the shared `$4016` strobe signal.
    pub fn write_strobe(&mut self, value: u8) {
        self.strobe = value & 1 != 0;
        if self.strobe {
            self.latch();
        }
    }

    /// Reads the next serial button bit. After all eight buttons have shifted,
    /// real standard controllers return one bits.
    pub fn read_serial(&mut self) -> u8 {
        if self.strobe {
            self.latch();
        }

        let value = self.shift_register & 1;
        if !self.strobe {
            self.shift_register = (self.shift_register >> 1) | 0x80;
        }
        value
    }

    pub fn peek_serial(&self) -> u8 {
        if self.strobe {
            self.buttons.bits() & 1
        } else {
            self.shift_register & 1
        }
    }

    fn latch(&mut self) {
        self.shift_register = self.buttons.bits();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shifts_buttons_in_hardware_order_then_returns_one() {
        let mut port = ControllerPort::default();
        port.set_buttons(ControllerButtons::A | ControllerButtons::START | ControllerButtons::LEFT);
        port.write_strobe(1);
        port.write_strobe(0);

        let shifted = std::array::from_fn::<_, 10, _>(|_| port.read_serial());
        assert_eq!(shifted, [1, 0, 0, 1, 0, 0, 1, 0, 1, 1]);
    }

    #[test]
    fn high_strobe_continuously_reports_current_a_button() {
        let mut port = ControllerPort::default();
        port.write_strobe(1);
        assert_eq!(port.read_serial(), 0);
        port.set_buttons(ControllerButtons::A | ControllerButtons::RIGHT);
        assert_eq!(port.read_serial(), 1);
        assert_eq!(port.read_serial(), 1);
    }
}
