use std::io::{self, Read};
use std::thread;

#[derive(Default, Clone, Copy)]
struct St {
    lines: u64,
    valid: u64,
    sum: i64,
    primes: u64,
    min: Option<i64>,
    max: Option<i64>,
}

fn is_prime(n: i64) -> bool {
    if n < 2 {
        return false;
    }
    let mut d = 2;
    while d * d <= n {
        if n % d == 0 {
            return false;
        }
        d += 1;
    }
    true
}

fn parse(l: &str) -> Option<i64> {
    let d = l.strip_prefix('-').unwrap_or(l);
    if d.is_empty() || !d.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    l.parse::<i64>().ok()
}

fn work(chunk: &[&str]) -> St {
    let mut s = St::default();
    for l in chunk {
        s.lines += 1;
        if let Some(v) = parse(l) {
            s.valid += 1;
            s.sum += v;
            if is_prime(v) {
                s.primes += 1;
            }
            s.min = Some(s.min.map_or(v, |m| m.min(v)));
            s.max = Some(s.max.map_or(v, |m| m.max(v)));
        }
    }
    s
}

fn main() {
    let mut inp = String::new();
    io::stdin().read_to_string(&mut inp).unwrap();
    let lines: Vec<&str> = inp.lines().collect();
    let n = lines.len();
    let mut chunks: Vec<&[&str]> = Vec::new();
    let mut start = 0;
    for k in 0..4 {
        let len = n / 4 + if k < n % 4 { 1 } else { 0 };
        chunks.push(&lines[start..start + len]);
        start += len;
    }
    let res: Vec<St> = thread::scope(|sc| {
        let hs: Vec<_> = chunks.iter().map(|c| sc.spawn(move || work(c))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut t = St::default();
    let mut out = String::new();
    for (k, s) in res.iter().enumerate() {
        out.push_str(&format!("chunk {}: lines={} valid={} sum={} primes={}\n", k + 1, s.lines, s.valid, s.sum, s.primes));
        t.lines += s.lines;
        t.valid += s.valid;
        t.sum += s.sum;
        t.primes += s.primes;
        if let Some(m) = s.min {
            t.min = Some(t.min.map_or(m, |x| x.min(m)));
        }
        if let Some(m) = s.max {
            t.max = Some(t.max.map_or(m, |x| x.max(m)));
        }
    }
    let f = |o: Option<i64>| o.map_or("none".to_string(), |v| v.to_string());
    out.push_str(&format!(
        "total: lines={} valid={} invalid={} sum={} min={} max={} primes={}\n",
        t.lines, t.valid, t.lines - t.valid, t.sum, f(t.min), f(t.max), t.primes
    ));
    print!("{}", out);
}
