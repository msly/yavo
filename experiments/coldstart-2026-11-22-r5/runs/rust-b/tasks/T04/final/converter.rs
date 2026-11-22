use std::fmt::Write;
fn unit(s: &str) -> Option<i64> {
    match s { "mm" => Some(1), "cm" => Some(10), "m" => Some(1000), "km" => Some(1000000), _ => None }
}
fn number(s: &str) -> Option<(i64, i64)> {
    if s.is_empty() || s.len() > 64 { return None; }
    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let (whole, fraction) = match body.split_once('.') {
        Some((w, f)) => {
            if f.is_empty() || f.len() > 3 || !f.bytes().all(|b| b.is_ascii_digit()) { return None; }
            (w, f)
        }
        None => (body, ""),
    };
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) { return None; }
    let mut value = 0i64;
    for b in whole.bytes() {
        value = value * 10 + (b - b'0') as i64;
        if value > 1000000 { return None; }
    }
    let mut denominator = 1i64;
    for b in fraction.bytes() { value = value * 10 + (b - b'0') as i64; denominator *= 10; }
    if value > 1000000 * denominator { return None; }
    if negative { value = -value; }
    Some((value, denominator))
}
fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 { let r = a % b; a = b; b = r; }
    a
}
fn convert_line(line: &str) -> Option<String> {
    let p: Vec<&str> = line.split(|c| c == ' ' || c == '\t').filter(|s| !s.is_empty()).collect();
    if p.len() != 3 { return None; }
    let (value, decimal_denominator) = number(p[0])?;
    let from = unit(p[1])?;
    let to = unit(p[2])?;
    let mut numerator = value * from;
    let mut denominator = decimal_denominator * to;
    let g = gcd(numerator.abs(), denominator);
    numerator /= g;
    denominator /= g;
    if denominator == 1 { Some(format!("{} {}", numerator, p[2])) }
    else { Some(format!("{}/{} {}", numerator, denominator, p[2])) }
}
pub fn convert_input(input: &str) -> String {
    let mut output = String::new();
    for (i, line) in input.lines().enumerate() {
        match convert_line(line) {
            Some(result) => writeln!(output, "{}: {}", i + 1, result).unwrap(),
            None => writeln!(output, "{}: invalid", i + 1).unwrap(),
        }
    }
    output
}
