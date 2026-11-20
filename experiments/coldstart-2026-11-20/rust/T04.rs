use std::io::*;
fn parse(l: &str) -> Vec<i64> { l.split_whitespace().map(|x| x.parse().unwrap()).collect() }
fn ragged(m: &[Vec<i64>]) -> bool { m.iter().any(|r| r.len() != m[0].len()) }
fn fail(s: String) -> ! { eprintln!("{}", s); std::process::exit(1) }
fn main() {
    let mut inp = String::new();
    stdin().read_to_string(&mut inp).unwrap();
    let mut a = vec![]; let mut b = vec![]; let mut second = false;
    for l in inp.lines() {
        if l.trim().is_empty() { second = true; continue; }
        if second { b.push(parse(l)); } else { a.push(parse(l)); }
    }
    if ragged(&a) || ragged(&b) { fail("error: ragged matrix".into()); }
    let ac = a.first().map_or(0, |r| r.len());
    let bc = b.first().map_or(0, |r| r.len());
    if ac != b.len() { fail(format!("error: cannot multiply {}x{} by {}x{}", a.len(), ac, b.len(), bc)); }
    for i in 0..a.len() {
        let row: Vec<String> = (0..bc).map(|j| (0..ac).map(|k| a[i][k] * b[k][j]).sum::<i64>().to_string()).collect();
        println!("{}", row.join(" "));
    }
}
