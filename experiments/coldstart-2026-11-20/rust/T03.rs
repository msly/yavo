use std::io::{stdin, Read};
use std::collections::BTreeMap;
fn pos(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) { return None; }
    match s.parse::<i64>() { Ok(n) if n > 0 => Some(n), _ => None }
}
fn run(f: &[&str], acc: &mut BTreeMap<String, i64>) -> Result<(), String> {
    let (nargs, ok) = match f[0] {
        "open" => (2, true), "deposit" | "withdraw" => (3, true), "transfer" => (4, true), _ => (0, false),
    };
    if !ok || f.len() != nargs { return Err("bad command".into()); }
    if f[0] == "open" {
        if acc.contains_key(f[1]) { return Err(format!("{} exists", f[1])); }
        acc.insert(f[1].to_string(), 0);
        return Ok(());
    }
    let amt_s = f[nargs - 1];
    let amt = pos(amt_s).ok_or(format!("bad amount '{}'", amt_s))?;
    for n in &f[1..nargs - 1] {
        if !acc.contains_key(*n) { return Err(format!("no account {}", n)); }
    }
    match f[0] {
        "deposit" => { *acc.get_mut(f[1]).unwrap() += amt; }
        "withdraw" => {
            let b = acc.get_mut(f[1]).unwrap();
            if *b < amt { return Err(format!("insufficient funds in {}", f[1])); }
            *b -= amt;
        }
        _ => {
            if f[1] == f[2] { return Err("same account".into()); }
            if acc[f[1]] < amt { return Err(format!("insufficient funds in {}", f[1])); }
            *acc.get_mut(f[1]).unwrap() -= amt;
            *acc.get_mut(f[2]).unwrap() += amt;
        }
    }
    Ok(())
}
fn main() {
    let mut inp = String::new();
    stdin().read_to_string(&mut inp).unwrap();
    let mut acc = BTreeMap::new();
    for l in inp.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.is_empty() { continue; }
        if let Err(e) = run(&f, &mut acc) { println!("error: {}", e); }
    }
    let mut t = 0;
    for (n, b) in &acc { println!("{} {}", n, b); t += b; }
    println!("total: {}", t);
}
