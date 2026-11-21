use std::io::{self, Read};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;

fn parse(l: &str) -> Option<u64> {
    if l.is_empty() || l.len() > 12 || !l.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u64 = l.parse().ok()?;
    if (1..=10_000_000).contains(&n) { Some(n) } else { None }
}

fn collatz(mut n: u64) -> (u64, u64) {
    let mut steps = 0;
    let mut peak = n;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
        peak = peak.max(n);
    }
    (steps, peak)
}

fn main() {
    let mut inp = String::new();
    io::stdin().read_to_string(&mut inp).unwrap();
    let lines: Vec<&str> = inp.lines().collect();
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<(u64, u64, u64)>>> = Mutex::new(vec![None; lines.len()]);
    thread::scope(|s| {
        for _ in 0..3 {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= lines.len() {
                    break;
                }
                if let Some(n) = parse(lines[i]) {
                    let (st, pk) = collatz(n);
                    results.lock().unwrap()[i] = Some((n, st, pk));
                }
            });
        }
    });
    let res = results.into_inner().unwrap();
    let mut out = String::new();
    let mut best: Option<(u64, u64)> = None;
    for (i, r) in res.iter().enumerate() {
        match r {
            Some((n, st, pk)) => {
                out.push_str(&format!("{}: steps={} peak={}\n", n, st, pk));
                if best.map_or(true, |b| *st > b.1) {
                    best = Some((*n, *st));
                }
            }
            None => out.push_str(&format!("line {}: invalid\n", i + 1)),
        }
    }
    match best {
        Some((n, s)) => out.push_str(&format!("longest: {} ({} steps)\n", n, s)),
        None => out.push_str("longest: none\n"),
    }
    print!("{}", out);
}
