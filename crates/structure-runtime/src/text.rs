//! External representation of data (2.2) used by conversions between
//! character strings and other types (table 22 footnote e).

use crate::calendar::{civil_from_days, days_from_civil, days_in_month};
use crate::image::TextKind;
use crate::ty::*;

pub fn format_real(x: f64, single: bool) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "INF".into() } else { "-INF".into() };
    }
    let mag = x.abs();
    let s = if single { format!("{}", x as f32) } else { format!("{}", x) };
    if mag != 0.0 && !(1e-5..1e15).contains(&mag) {
        let e = if single { format!("{:e}", x as f32) } else { format!("{:e}", x) };
        // Rust gives e.g. "1.5e20" or "1e-7"; the standard requires a
        // decimal point in the mantissa.
        let (m, exp) = e.split_once('e').unwrap();
        let m = if m.contains('.') { m.to_string() } else { format!("{}.0", m) };
        let exp: i32 = exp.parse().unwrap();
        return format!("{}E{}{}", m, if exp < 0 { "-" } else { "+" }, exp.abs());
    }
    if s.contains('.') {
        s
    } else {
        format!("{}.0", s)
    }
}

pub fn format_time(us: i64) -> String {
    let mut s = String::from("T#");
    if us < 0 {
        s.push('-');
    }
    let mut v = us.unsigned_abs();
    if v == 0 {
        s.push_str("0s");
        return s;
    }
    let days = v / 86_400_000_000;
    v %= 86_400_000_000;
    let hours = v / 3_600_000_000;
    v %= 3_600_000_000;
    let mins = v / 60_000_000;
    v %= 60_000_000;
    let secs = v / 1_000_000;
    v %= 1_000_000;
    if days > 0 {
        s.push_str(&format!("{}d", days));
    }
    if hours > 0 {
        s.push_str(&format!("{}h", hours));
    }
    if mins > 0 {
        s.push_str(&format!("{}m", mins));
    }
    if secs > 0 {
        s.push_str(&format!("{}s", secs));
    }
    if v > 0 {
        let ms = v / 1000;
        let frac = v % 1000;
        if frac == 0 {
            s.push_str(&format!("{}ms", ms));
        } else {
            let f = format!("{:03}", frac);
            s.push_str(&format!("{}.{}ms", ms, f.trim_end_matches('0')));
        }
    }
    s
}

fn format_daytime(us: i64) -> String {
    let secs = us / 1_000_000;
    let frac = us % 1_000_000;
    let mut s = format!("{:02}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60);
    if frac != 0 {
        let f = format!("{:06}", frac);
        s.push('.');
        s.push_str(f.trim_end_matches('0'));
    }
    s
}

pub fn format_date(us: i64) -> String {
    let (y, m, d) = civil_from_days(us.div_euclid(MICROS_PER_DAY));
    format!("D#{:04}-{:02}-{:02}", y, m, d)
}

pub fn format_tod(us: i64) -> String {
    format!("TOD#{}", format_daytime(us))
}

pub fn format_dt(us: i64) -> String {
    let days = us.div_euclid(MICROS_PER_DAY);
    let (y, m, d) = civil_from_days(days);
    format!("DT#{:04}-{:02}-{:02}-{}", y, m, d, format_daytime(us.rem_euclid(MICROS_PER_DAY)))
}

/// Formats a value of the given kind as its external representation.
pub fn format(kind: TextKind, v: u64) -> String {
    match kind {
        TextKind::Bool => if v & 1 != 0 { "TRUE" } else { "FALSE" }.into(),
        TextKind::Int(t) | TextKind::Bits(t) => to_i128(t, v).to_string(),
        TextKind::Real(Ty::F32) => format_real(f32_of(v) as f64, true),
        TextKind::Real(_) => format_real(f64_of(v), false),
        TextKind::Time => format_time(v as i64),
        TextKind::Date => format_date(v as i64),
        TextKind::TimeOfDay => format_tod(v as i64),
        TextKind::DateAndTime => format_dt(v as i64),
    }
}

fn strip_prefix_ci<'a>(s: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    for p in prefixes {
        if s.len() >= p.len() && s[..p.len()].eq_ignore_ascii_case(p) {
            return Some(&s[p.len()..]);
        }
    }
    None
}

fn digits(s: &str, radix: u32) -> Option<u128> {
    if s.is_empty() || s.starts_with('_') || s.ends_with('_') || s.contains("__") {
        return None;
    }
    let mut v: u128 = 0;
    for c in s.chars() {
        if c == '_' {
            continue;
        }
        v = v.checked_mul(radix as u128)?.checked_add(c.to_digit(radix)? as u128)?;
    }
    Some(v)
}

fn parse_int(s: &str) -> Option<i128> {
    let s = s.trim();
    // optional type prefix
    let s = match s.find('#') {
        Some(i) if s[..i].chars().all(|c| c.is_ascii_alphabetic() || c == '_') && !s[..i].is_empty() => &s[i + 1..],
        _ => s,
    };
    if let Some((base, rest)) = s.split_once('#') {
        let radix = match base {
            "2" => 2,
            "8" => 8,
            "16" => 16,
            _ => return None,
        };
        return digits(rest, radix).map(|v| v as i128);
    }
    let (neg, body) = match s.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let v = digits(body, 10)? as i128;
    Some(if neg { -v } else { v })
}

fn parse_real(s: &str) -> Option<f64> {
    let s = s.trim();
    let s = match s.find('#') {
        Some(i) => &s[i + 1..],
        None => s,
    };
    let cleaned: String = s.chars().filter(|c| *c != '_').collect();
    if cleaned.is_empty() || !cleaned.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-')) {
        return None;
    }
    cleaned.parse::<f64>().ok().filter(|f| f.is_finite())
}

fn parse_fixed(s: &str) -> Option<(u128, u128, u128)> {
    // returns (integer, fraction numerator, denominator)
    let (i, f) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s, None),
    };
    let iv = digits(i, 10)?;
    match f {
        None => Some((iv, 0, 1)),
        Some(f) => {
            let mut num = 0u128;
            let mut den = 1u128;
            for c in f.chars().filter(|c| *c != '_').take(30) {
                num = num * 10 + c.to_digit(10)? as u128;
                den *= 10;
            }
            Some((iv, num, den))
        }
    }
}

pub fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim();
    let body = strip_prefix_ci(s, &["TIME#", "T#"]).unwrap_or(s);
    let (neg, mut rest) = match body.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, body),
    };
    if rest.is_empty() {
        return None;
    }
    let mut total: i128 = 0;
    let mut last_rank = -1i32;
    let mut frac_seen = false;
    while !rest.is_empty() {
        if frac_seen {
            return None;
        }
        let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '_' || c == '.'))?;
        let num = &rest[..end];
        rest = &rest[end..];
        let lower = rest.to_ascii_lowercase();
        let (unit, us, rank): (usize, i128, i32) = if lower.starts_with("ms") {
            (2, 1_000, 4)
        } else if lower.starts_with('d') {
            (1, 86_400_000_000, 0)
        } else if lower.starts_with('h') {
            (1, 3_600_000_000, 1)
        } else if lower.starts_with('m') {
            (1, 60_000_000, 2)
        } else if lower.starts_with('s') {
            (1, 1_000_000, 3)
        } else {
            return None;
        };
        if rank <= last_rank {
            return None;
        }
        last_rank = rank;
        rest = &rest[unit..];
        rest = rest.strip_prefix('_').unwrap_or(rest);
        let (iv, num, den) = parse_fixed(num)?;
        frac_seen = den > 1;
        total += iv as i128 * us + (num as i128 * us + den as i128 / 2) / den as i128;
    }
    let total = if neg { -total } else { total };
    i64::try_from(total).ok()
}

fn parse_daytime(s: &str) -> Option<i64> {
    let mut parts = s.split(':');
    let h = digits(parts.next()?, 10)?;
    let m = digits(parts.next()?, 10)?;
    let (si, num, den) = parse_fixed(parts.next()?)?;
    if parts.next().is_some() || h > 23 || m > 59 || si > 59 {
        return None;
    }
    Some((((h * 60 + m) * 60 + si) * 1_000_000 + (num * 1_000_000 + den / 2) / den) as i64)
}

fn parse_date_body(s: &str) -> Option<i64> {
    let mut parts = s.split('-');
    let y = digits(parts.next()?, 10)? as i64;
    let m = digits(parts.next()?, 10)? as u32;
    let d = digits(parts.next()?, 10)? as u32;
    if parts.next().is_some() || !(1..=9999).contains(&y) || !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    Some(days_from_civil(y, m, d) * MICROS_PER_DAY)
}

pub fn parse_text(kind: TextKind, s: &str) -> Option<u64> {
    let t = s.trim();
    match kind {
        TextKind::Bool => {
            let b = strip_prefix_ci(t, &["BOOL#"]).unwrap_or(t);
            if b.eq_ignore_ascii_case("TRUE") || b == "1" {
                Some(1)
            } else if b.eq_ignore_ascii_case("FALSE") || b == "0" {
                Some(0)
            } else {
                None
            }
        }
        TextKind::Int(ty) | TextKind::Bits(ty) => from_i128(ty, parse_int(t)?),
        TextKind::Real(Ty::F32) => {
            let f = parse_real(t)? as f32;
            if f.is_finite() {
                Some(of_f32(f))
            } else {
                None
            }
        }
        TextKind::Real(_) => Some(of_f64(parse_real(t)?)),
        TextKind::Time => parse_duration(t).map(|v| v as u64),
        TextKind::Date => parse_date_body(strip_prefix_ci(t, &["DATE#", "D#"]).unwrap_or(t)).map(|v| v as u64),
        TextKind::TimeOfDay => parse_daytime(strip_prefix_ci(t, &["TIME_OF_DAY#", "TOD#"]).unwrap_or(t)).map(|v| v as u64),
        TextKind::DateAndTime => {
            let b = strip_prefix_ci(t, &["DATE_AND_TIME#", "DT#"]).unwrap_or(t);
            let mut count = 0;
            let idx = b.char_indices().find(|(_, c)| {
                if *c == '-' {
                    count += 1;
                }
                count == 3
            })?;
            let d = parse_date_body(&b[..idx.0])?;
            let tod = parse_daytime(&b[idx.0 + 1..])?;
            Some((d + tod) as u64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(format_real(1.0, false), "1.0");
        assert_eq!(format_real(1.5e20, false), "1.5E+20");
        assert_eq!(format_time(0), "T#0s");
        assert_eq!(format_time(90_061_503_500), "T#1d1h1m1s503.5ms");
        assert_eq!(parse_duration("T#1h_30m"), Some(5_400_000_000));
        assert_eq!(parse_text(TextKind::Int(Ty::I16), "16#FF"), Some(255));
        assert_eq!(parse_text(TextKind::Int(Ty::I8), "200"), None);
        let dt = parse_text(TextKind::DateAndTime, "DT#1984-06-25-15:36:55.36").unwrap();
        assert_eq!(format_dt(dt as i64), "DT#1984-06-25-15:36:55.36");
    }
}
