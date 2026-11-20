use std::io::*;
use std::collections::BTreeMap;
fn main() {
    let mut inp = String::new();
    stdin().read_to_string(&mut inp).unwrap();
    let mut lv: BTreeMap<String, usize> = BTreeMap::new();
    let mut hours: BTreeMap<String, usize> = BTreeMap::new();
    let mut first: Option<String> = None;
    let mut skipped = 0;
    for l in inp.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() < 2 { skipped += 1; continue; }
        *lv.entry(f[1].to_string()).or_insert(0) += 1;
        let h: String = f[0].chars().take(2).collect();
        *hours.entry(h).or_insert(0) += 1;
        if f[1] == "ERROR" && first.is_none() { first = Some(f[2..].join(" ")); }
    }
    let mut v: Vec<(String, usize)> = lv.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (k, c) in &v { println!("{} {}", k, c); }
    println!("first error: {}", first.unwrap_or("none".to_string()));
    let mut best: Option<(&String, usize)> = None;
    for (h, c) in &hours {
        if best.map_or(true, |b| *c > b.1) { best = Some((h, *c)); }
    }
    if let Some((h, c)) = best { println!("busiest hour: {} ({})", h, c); }
    println!("skipped: {}", skipped);
}
