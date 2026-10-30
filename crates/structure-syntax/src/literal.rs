//! Interpretation of literal token text (2.2, B.1.2).

/// Parses a decimal integer with optional single embedded underlines.
pub fn parse_decimal(text: &str) -> Result<u128, String> {
    check_underlines(text)?;
    let digits: String = text.chars().filter(|c| *c != '_').collect();
    digits.parse::<u128>().map_err(|_| "integer literal is out of range".to_string())
}

fn check_underlines(digits: &str) -> Result<(), String> {
    if digits.starts_with('_') || digits.ends_with('_') || digits.contains("__") {
        return Err("underline characters in numeric literals shall only be single and between digits".into());
    }
    Ok(())
}

/// Parses a based integer `2#...`, `8#...` or `16#...`.
pub fn parse_based(text: &str) -> Result<u128, String> {
    let sep = text.find(['#', '£']).ok_or("missing '#' in based literal")?;
    let base: u32 = text[..sep].parse().map_err(|_| "invalid base")?;
    if !matches!(base, 2 | 8 | 16) {
        return Err(format!("integer literals can only be represented in base 2, 8 or 16 (not {})", base));
    }
    let rest = &text[sep + text[sep..].chars().next().unwrap().len_utf8()..];
    check_underlines(rest)?;
    let mut value: u128 = 0;
    for c in rest.chars() {
        if c == '_' {
            continue;
        }
        let d = c.to_digit(16).ok_or("invalid digit")?;
        if d >= base {
            return Err(format!("digit '{}' is not valid in base {}", c, base));
        }
        value = value
            .checked_mul(base as u128)
            .and_then(|v| v.checked_add(d as u128))
            .ok_or("integer literal is out of range")?;
    }
    Ok(value)
}

/// Normalises a real literal (removes underlines) and checks its form.
pub fn parse_real(text: &str) -> Result<String, String> {
    let (mant, exp) = match text.find(['e', 'E']) {
        Some(i) => (&text[..i], Some(&text[i + 1..])),
        None => (text, None),
    };
    let (int, frac) = mant.split_once('.').ok_or("a real literal shall contain a decimal point")?;
    check_underlines(int)?;
    check_underlines(frac)?;
    let mut s: String = int.chars().filter(|c| *c != '_').collect();
    s.push('.');
    s.extend(frac.chars().filter(|c| *c != '_'));
    if let Some(e) = exp {
        let digits = e.trim_start_matches(['+', '-']);
        check_underlines(digits)?;
        s.push('e');
        s.extend(e.chars().filter(|c| *c != '_'));
    }
    Ok(s)
}

/// A duration literal (2.2.3.1), in nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Duration {
    pub negative: bool,
    pub nanos: u128,
}

fn split_prefix(text: &str) -> &str {
    match text.find(['#', '£']) {
        Some(i) => &text[i + text[i..].chars().next().unwrap().len_utf8()..],
        None => text,
    }
}

/// Parses the full token text of a duration literal, e.g. `T#5d_14h3.5ms`.
pub fn parse_duration(token: &str) -> Result<Duration, String> {
    let body = split_prefix(token);
    let (negative, mut rest) = match body.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, body),
    };
    if rest.is_empty() {
        return Err("empty duration literal".into());
    }
    // (unit name, nanoseconds, rank, limit for non-leading components)
    const UNITS: [(&str, u128, u32, u128); 5] = [
        ("d", 86_400_000_000_000, 0, u128::MAX),
        ("h", 3_600_000_000_000, 1, 24),
        ("m", 60_000_000_000, 2, 60),
        ("s", 1_000_000_000, 3, 60),
        ("ms", 1_000_000, 4, 1000),
    ];
    let mut total: u128 = 0;
    let mut last_rank: Option<u32> = None;
    let mut had_fraction = false;
    while !rest.is_empty() {
        if had_fraction {
            return Err("only the least significant unit of a duration may have a fractional part".into());
        }
        let num_end = rest.find(|c: char| !(c.is_ascii_digit() || c == '_' || c == '.')).unwrap_or(rest.len());
        let num = &rest[..num_end];
        if num.is_empty() {
            return Err(format!("invalid duration literal '{}'", token));
        }
        rest = &rest[num_end..];
        let lower = rest.to_ascii_lowercase();
        let unit = if lower.starts_with("ms") {
            "ms"
        } else if lower.starts_with('d') {
            "d"
        } else if lower.starts_with('h') {
            "h"
        } else if lower.starts_with('m') {
            "m"
        } else if lower.starts_with('s') {
            "s"
        } else {
            return Err(format!("missing time unit in duration literal '{}'", token));
        };
        rest = &rest[unit.len()..];
        if let Some(r) = rest.strip_prefix('_') {
            rest = r;
        }
        let &(_, unit_ns, rank, limit) = UNITS.iter().find(|u| u.0 == unit).unwrap();
        if let Some(lr) = last_rank {
            if rank <= lr {
                return Err("duration units shall appear in decreasing order of significance, each at most once".into());
            }
        }
        let (int_part, frac_part) = match num.split_once('.') {
            Some((i, f)) => {
                had_fraction = true;
                (i, Some(f))
            }
            None => (num, None),
        };
        let iv = parse_decimal(int_part)?;
        if last_rank.is_some() && iv >= limit {
            return Err(format!("value {} of unit '{}' is out of range in a non-leading duration component", iv, unit));
        }
        last_rank = Some(rank);
        let mut v = iv.checked_mul(unit_ns).ok_or("duration literal is out of range")?;
        if let Some(f) = frac_part {
            check_underlines(f)?;
            let digits: String = f.chars().filter(|c| *c != '_').collect();
            let mut num: u128 = 0;
            let mut den: u128 = 1;
            for c in digits.chars().take(30) {
                num = num * 10 + c.to_digit(10).unwrap() as u128;
                den *= 10;
            }
            // round to nearest nanosecond
            v += (num * unit_ns + den / 2) / den;
        }
        total = total.checked_add(v).ok_or("duration literal is out of range")?;
    }
    Ok(Duration { negative, nanos: total })
}

/// Time of day in nanoseconds since midnight (2.2.3.2).
pub fn parse_daytime(text: &str) -> Result<u64, String> {
    let mut parts = text.split(':');
    let h = parts.next().ok_or("missing hour")?;
    let m = parts.next().ok_or("missing minute in time of day")?;
    let s = parts.next().ok_or("missing second in time of day")?;
    if parts.next().is_some() {
        return Err("invalid time of day".into());
    }
    let h = parse_decimal(h)?;
    let m = parse_decimal(m)?;
    let (si, sf) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s, None),
    };
    let si = parse_decimal(si)?;
    if h > 23 {
        return Err("hour of time of day shall be in the range 0..23".into());
    }
    if m > 59 {
        return Err("minute shall be in the range 0..59".into());
    }
    if si > 59 {
        return Err("second shall be in the range 0..59".into());
    }
    let mut ns = ((h * 60 + m) * 60 + si) * 1_000_000_000;
    if let Some(f) = sf {
        check_underlines(f)?;
        let digits: String = f.chars().filter(|c| *c != '_').collect();
        let mut num: u128 = 0;
        let mut den: u128 = 1;
        for c in digits.chars().take(30) {
            num = num * 10 + c.to_digit(10).unwrap() as u128;
            den *= 10;
        }
        ns += (num * 1_000_000_000 + den / 2) / den;
    }
    Ok(ns as u64)
}

pub fn parse_time_of_day(token: &str) -> Result<u64, String> {
    parse_daytime(split_prefix(token))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
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

fn parse_date_body(text: &str) -> Result<Date, String> {
    let mut parts = text.split('-');
    let y = parse_decimal(parts.next().ok_or("missing year")?)?;
    let m = parse_decimal(parts.next().ok_or("missing month in date")?)?;
    let d = parse_decimal(parts.next().ok_or("missing day in date")?)?;
    if parts.next().is_some() {
        return Err("invalid date".into());
    }
    if y == 0 || y > 9999 {
        return Err("year shall be in the range 1..9999".into());
    }
    if !(1..=12).contains(&m) {
        return Err("month shall be in the range 1..12".into());
    }
    if d < 1 || d > days_in_month(y as i64, m as u32) as u128 {
        return Err("day is out of range for the month".into());
    }
    Ok(Date { year: y as i32, month: m as u32, day: d as u32 })
}

pub fn parse_date(token: &str) -> Result<Date, String> {
    parse_date_body(split_prefix(token))
}

pub fn parse_date_and_time(token: &str) -> Result<(Date, u64), String> {
    let body = split_prefix(token);
    // date_literal '-' daytime: the third '-' separates the time of day.
    let mut idx = None;
    let mut count = 0;
    for (i, c) in body.char_indices() {
        if c == '-' {
            count += 1;
            if count == 3 {
                idx = Some(i);
                break;
            }
        }
    }
    let i = idx.ok_or("date and time literal requires a time of day")?;
    let date = parse_date_body(&body[..i])?;
    let tod = parse_daytime(&body[i + 1..])?;
    Ok((date, tod))
}

/// Decodes a character string literal including its quotes. Returns the
/// character codes. `wide` selects the double-byte interpretation of `$`
/// hexadecimal escapes; the quote character determines which quote escape
/// is valid.
pub fn parse_string(token: &str, wide: bool) -> Result<Vec<u32>, String> {
    let mut chars = token.chars();
    let quote = chars.next().ok_or("empty string token")?;
    let inner: Vec<char> = chars.collect();
    if inner.last() != Some(&quote) {
        return Err("unterminated character string literal".into());
    }
    let inner = &inner[..inner.len() - 1];
    let mut out = Vec::new();
    let mut i = 0;
    while i < inner.len() {
        let c = inner[i];
        if c == '$' || c == '¤' {
            let e = *inner.get(i + 1).ok_or("incomplete '$' escape")?;
            i += 2;
            let code = match e.to_ascii_uppercase() {
                '$' => '$' as u32,
                'L' => 0x0A,
                'N' => 0x0A,
                'P' => 0x0C,
                'R' => 0x0D,
                'T' => 0x09,
                '\'' if quote == '\'' => '\'' as u32,
                '"' if quote == '"' => '"' as u32,
                h if h.is_ascii_hexdigit() => {
                    let n = if wide { 4 } else { 2 };
                    let mut v = h.to_digit(16).unwrap();
                    for k in 1..n {
                        let d = inner
                            .get(i + k - 1)
                            .and_then(|c| c.to_digit(16))
                            .ok_or(if wide {
                                "'$' in a double-byte string shall be followed by four hexadecimal digits"
                            } else {
                                "'$' in a single-byte string shall be followed by two hexadecimal digits"
                            })?;
                        v = v * 16 + d;
                    }
                    i += n - 1;
                    v
                }
                other => return Err(format!("invalid escape sequence '${}'", other)),
            };
            out.push(code);
            continue;
        }
        if c == quote {
            return Err(format!("quote character {} inside string must be written as ${}", quote, quote));
        }
        let code = c as u32;
        if !wide && code > 0xFF {
            return Err(format!("character '{}' is not representable in a single-byte character string", c));
        }
        if wide && code > 0xFFFF {
            return Err(format!("character '{}' is not representable in a double-byte character string", c));
        }
        out.push(code);
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("T#14ms").unwrap().nanos, 14_000_000);
        assert_eq!(parse_duration("t#14.7s").unwrap().nanos, 14_700_000_000);
        assert_eq!(parse_duration("T#25h_15m").unwrap().nanos, (25 * 3600 + 15 * 60) * 1_000_000_000);
        assert_eq!(
            parse_duration("t#5d14h12m18s3.5ms").unwrap().nanos,
            ((((5 * 24 + 14) * 60 + 12) * 60 + 18) * 1000) * 1_000_000 + 3_500_000
        );
        assert!(parse_duration("TIME#-14ms").unwrap().negative);
        assert!(parse_duration("T#1h75m").is_err());
        assert!(parse_duration("T#1.5h3m").is_err());
    }

    #[test]
    fn dates() {
        assert_eq!(parse_date("D#1984-06-25").unwrap(), Date { year: 1984, month: 6, day: 25 });
        let (d, t) = parse_date_and_time("DT#1984-06-25-15:36:55.36").unwrap();
        assert_eq!(d.day, 25);
        assert_eq!(t, (15 * 3600 + 36 * 60 + 55) * 1_000_000_000 + 360_000_000);
        assert!(parse_date("D#1983-02-29").is_err());
    }

    #[test]
    fn strings() {
        assert_eq!(parse_string("'$R$L'", false).unwrap(), vec![13, 10]);
        assert_eq!(parse_string("'$$1.00'", false).unwrap().len(), 5);
        assert_eq!(parse_string("'$C4$CB'", false).unwrap(), vec![0xC4, 0xCB]);
        assert_eq!(parse_string("\"$00C4$00CB\"", true).unwrap(), vec![0xC4, 0xCB]);
        assert_eq!(parse_string("'$''", false).unwrap(), vec![39]);
        assert!(parse_string("\"$'\"", true).is_err());
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_based("16#FF").unwrap(), 255);
        assert_eq!(parse_based("8#340").unwrap(), 224);
        assert_eq!(parse_decimal("123_456").unwrap(), 123456);
        assert!(parse_decimal("1__2").is_err());
        assert!(parse_based("3#12").is_err());
        assert_eq!(parse_real("3.14159_26").unwrap(), "3.1415926");
    }
}
