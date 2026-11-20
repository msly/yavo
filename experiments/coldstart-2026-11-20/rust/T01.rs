use std::io::*;
use std::collections::HashSet;
fn isnum(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let mut parts = s.splitn(2, '.');
    let a = parts.next().unwrap();
    let b = parts.next();
    if a.is_empty() || !a.bytes().all(|c| c.is_ascii_digit()) { return false; }
    match b { None => true, Some(b) => !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()) }
}
fn main() {
    let mut inp = String::new();
    stdin().read_to_string(&mut inp).unwrap();
    let mut lines = inp.lines();
    let names: Vec<&str> = lines.next().unwrap_or("").split(',').collect();
    let mut cols: Vec<Vec<&str>> = vec![vec![]; names.len()];
    for l in lines {
        for (i, v) in l.split(',').enumerate() {
            if i < cols.len() && !v.is_empty() { cols[i].push(v); }
        }
    }
    for (i, n) in names.iter().enumerate() {
        let c = &cols[i];
        if !c.is_empty() && c.iter().all(|v| isnum(v)) {
            let nums: Vec<f64> = c.iter().map(|v| v.parse().unwrap()).collect();
            let mn = nums.iter().cloned().fold(f64::INFINITY, f64::min);
            let mx = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mean = nums.iter().sum::<f64>() / nums.len() as f64;
            println!("{}: min={:.2} max={:.2} mean={:.2}", n, mn, mx, mean);
        } else {
            let d: HashSet<&&str> = c.iter().collect();
            println!("{}: text ({} distinct)", n, d.len());
        }
    }
}
