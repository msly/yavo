use std::io::{self, Read};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

fn collatz(n: u64) -> (u64, u64) {
    let (mut x, mut steps, mut peak) = (n, 0u64, n);
    while x != 1 {
        x = if x % 2 == 0 { x / 2 } else { 3 * x + 1 };
        steps += 1;
        if x > peak {
            peak = x;
        }
    }
    (steps, peak)
}

fn parse(l: &str) -> Option<u64> {
    if l.is_empty() || !l.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let v: u64 = l.parse().ok()?;
    if (1..=10_000_000).contains(&v) { Some(v) } else { None }
}

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let lines: Vec<&str> = input.lines().collect();
    let next = AtomicUsize::new(0);
    let parts: Vec<Vec<(usize, Option<(u64, u64, u64)>)>> = thread::scope(|sc| {
        let hs: Vec<_> = (0..3)
            .map(|_| {
                sc.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        if i >= lines.len() {
                            break;
                        }
                        let r = parse(lines[i]).map(|n| {
                            let (s, p) = collatz(n);
                            (n, s, p)
                        });
                        mine.push((i, r));
                    }
                    mine
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut res: Vec<Option<(u64, u64, u64)>> = vec![None; lines.len()];
    for p in parts {
        for (i, r) in p {
            res[i] = r;
        }
    }
    let mut out = String::new();
    let mut best: Option<(u64, u64)> = None;
    for (i, r) in res.iter().enumerate() {
        match r {
            Some((n, s, p)) => {
                out.push_str(&format!("{}: steps={} peak={}\n", n, s, p));
                if best.map_or(true, |(_, bs)| *s > bs) {
                    best = Some((*n, *s));
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
