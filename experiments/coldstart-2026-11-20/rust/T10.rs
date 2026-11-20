use std::io::{stdin, Read};
fn main() {
    let cap: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut inp = String::new();
    stdin().read_to_string(&mut inp).unwrap();
    let mut c: Vec<(String, String)> = Vec::new(); // front = oldest
    let (mut h, mut m) = (0, 0);
    for l in inp.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() == 2 && f[0] == "get" {
            if let Some(p) = c.iter().position(|e| e.0 == f[1]) {
                let e = c.remove(p);
                println!("{}", e.1);
                c.push(e); h += 1;
            } else { println!("miss"); m += 1; }
        } else if f.len() == 3 && f[0] == "put" {
            if let Some(p) = c.iter().position(|e| e.0 == f[1]) { c.remove(p); }
            c.push((f[1].to_string(), f[2].to_string()));
            if c.len() > cap { let e = c.remove(0); println!("evict {}", e.0); }
        }
    }
    println!("hits: {} misses: {}", h, m);
    let keys: Vec<&str> = c.iter().rev().map(|e| e.0.as_str()).collect();
    println!("order: {}", keys.join(" "));
}
