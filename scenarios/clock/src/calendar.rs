//! Validated Gregorian dates supplied by the host, never sampled from a clock.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockDate {
    year: u16,
    month: u8,
    day: u8,
}

impl ClockDate {
    /// Civil years 1..=9999. No timezone, timestamp, or independent weekday.
    pub fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        let leap = is_leap_year(year);
        let days = match month {
            4 | 6 | 9 | 11 => 30,
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            _ => return None,
        };
        (year > 0 && year <= 9999 && day > 0 && day <= days).then_some(Self { year, month, day })
    }

    pub const fn parts(self) -> [u16; 3] {
        [self.year, self.month as u16, self.day as u16]
    }

    pub(crate) fn ordinal(self) -> i32 {
        let year = i32::from(self.year) - 1;
        let before_month = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
        let leap = is_leap_year(self.year);
        year * 365 + year / 4 - year / 100
            + year / 400
            + before_month[usize::from(self.month - 1)]
            + i32::from(self.month > 2 && leap)
            + i32::from(self.day)
            - 1
    }

    pub fn label(self) -> String {
        let weekdays = [
            "MONDAY",
            "TUESDAY",
            "WEDNESDAY",
            "THURSDAY",
            "FRIDAY",
            "SATURDAY",
            "SUNDAY",
        ];
        let months = [
            "JANUARY",
            "FEBRUARY",
            "MARCH",
            "APRIL",
            "MAY",
            "JUNE",
            "JULY",
            "AUGUST",
            "SEPTEMBER",
            "OCTOBER",
            "NOVEMBER",
            "DECEMBER",
        ];
        let weekday = weekdays[(self.ordinal() % 7) as usize];
        let month = months[usize::from(self.month - 1)];
        format!("{weekday} · {month} {}", self.day)
    }
}

fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}
