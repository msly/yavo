use std::cmp::Ordering;
use std::fmt::Write;
use std::io::{self, Read};
struct Version<'a> { original: &'a str, core: [i64; 3], pre: Option<Vec<&'a str>> }
fn identifiers(s: &str) -> Option<Vec<&str>> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.iter().all(|part| !part.is_empty() && part.len() <= 32 && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')) {
        Some(parts)
    } else { None }
}
fn core_number(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) { return None; }
    let mut value = 0i64;
    for b in s.bytes() {
        value = value * 10 + (b - b'0') as i64;
        if value > 1000000000 { return None; }
    }
    Some(value)
}
fn parse(s: &str) -> Option<Version<'_>> {
    let body = match s.split_once('+') {
        Some((body, build)) => { identifiers(build)?; body }
        None => s,
    };
    let (core_text, pre) = match body.split_once('-') {
        Some((core, pre)) => (core, Some(identifiers(pre)?)),
        None => (body, None),
    };
    let parts: Vec<&str> = core_text.split('.').collect();
    if parts.len() != 3 { return None; }
    let core = [core_number(parts[0])?, core_number(parts[1])?, core_number(parts[2])?];
    Some(Version { original: s, core, pre })
}
fn compare_identifier(a: &str, b: &str) -> Ordering {
    let an = a.bytes().all(|c| c.is_ascii_digit());
    let bn = b.bytes().all(|c| c.is_ascii_digit());
    match (an, bn) {
        (true, true) => {
            let a = a.trim_start_matches('0');
            let b = b.trim_start_matches('0');
            a.len().cmp(&b.len()).then_with(|| a.cmp(b))
        }
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.cmp(b),
    }
}
fn compare(a: &Version<'_>, b: &Version<'_>) -> Ordering {
    let core = a.core.cmp(&b.core);
    if core != Ordering::Equal { return core; }
    match (&a.pre, &b.pre) {
        (None, None) => Ordering::Equal,
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (Some(ap), Some(bp)) => {
            for (a, b) in ap.iter().zip(bp.iter()) {
                let order = compare_identifier(a, b);
                if order != Ordering::Equal { return order; }
            }
            ap.len().cmp(&bp.len())
        }
    }
}
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut versions = Vec::new();
    let mut invalid = 0;
    let mut output = String::new();
    for (index, line) in input.lines().enumerate() {
        match parse(line) {
            Some(version) => versions.push(version),
            None => { invalid += 1; writeln!(output, "line {}: invalid", index + 1).unwrap(); }
        }
    }
    versions.sort_by(compare);
    for version in &versions { writeln!(output, "{}", version.original).unwrap(); }
    writeln!(output, "valid: {} invalid: {}", versions.len(), invalid).unwrap();
    print!("{}", output);
}
