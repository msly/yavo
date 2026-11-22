use std::io::{self, Read};
use std::thread;

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

#[derive(Default)]
struct Stat {
    lines: usize,
    valid: usize,
    sum: i64,
    min: Option<i64>,
    max: Option<i64>,
    primes: usize,
}

fn work(lines: &[&str]) -> Stat {
    let mut s = Stat { lines: lines.len(), ..Default::default() };
    for l in lines {
        let digits = l.strip_prefix('-').unwrap_or(l);
        if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let v: i64 = match l.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        s.valid += 1;
        s.sum += v;
        s.min = Some(s.min.map_or(v, |m| m.min(v)));
        s.max = Some(s.max.map_or(v, |m| m.max(v)));
        if is_prime(v) {
            s.primes += 1;
        }
    }
    s
}

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let lines: Vec<&str> = input.lines().collect();
    let n = lines.len();
    let (q, r) = (n / 4, n % 4);
    let mut parts: Vec<&[&str]> = Vec::new();
    let mut start = 0;
    for k in 0..4 {
        let len = q + if k < r { 1 } else { 0 };
        parts.push(&lines[start..start + len]);
        start += len;
    }
    let stats: Vec<Stat> = thread::scope(|sc| {
        let hs: Vec<_> = parts.iter().map(|p| sc.spawn(move || work(p))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut out = String::new();
    let mut t = Stat::default();
    for (k, s) in stats.iter().enumerate() {
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
