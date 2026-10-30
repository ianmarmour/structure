//! Proleptic Gregorian calendar arithmetic. DATE and DATE_AND_TIME values
//! count microseconds since 0001-01-01T00:00:00.

use crate::ty::MICROS_PER_DAY;

/// Days since 0001-01-01 of the given civil date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    // Howard Hinnant's algorithm, shifted from 1970-01-01.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468 + 719162
}

/// Civil date of a day count since 0001-01-01.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days - 719162 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(y) => 29,
        2 => 28,
        _ => 0,
    }
}

/// First and last representable DATE_AND_TIME values (0001-01-01 to
/// 9999-12-31T23:59:59.999999).
pub const DT_MIN: i64 = 0;
pub fn dt_max() -> i64 {
    (days_from_civil(9999, 12, 31) + 1) * MICROS_PER_DAY - 1
}

/// Microseconds since 0001-01-01 of 1970-01-01T00:00:00 (Unix epoch).
pub fn unix_epoch_micros() -> i64 {
    days_from_civil(1970, 1, 1) * MICROS_PER_DAY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        assert_eq!(days_from_civil(1, 1, 1), 0);
        assert_eq!(days_from_civil(1970, 1, 1), 719162);
        for days in [0i64, 1, 365, 719162, 738000, 3_652_058] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(civil_from_days(days_from_civil(1984, 6, 25)), (1984, 6, 25));
    }
}
