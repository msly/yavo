use std::io::{self, Read};

struct J {
    name: String,
    arr: u64,
    dur: u64,
    pri: u64,
}

fn num(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut jobs: Vec<J> = Vec::new();
    for (i, line) in input.lines().enumerate() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let j = if f.len() == 4 {
            match (num(f[1]), num(f[2]), num(f[3])) {
                (Some(a), Some(d), Some(p)) if d > 0 => Some(J { name: f[0].to_string(), arr: a, dur: d, pri: p }),
                _ => None,
            }
        } else {
            None
        };
        match j {
            Some(j) => jobs.push(j),
            None => eprintln!("line {}: invalid", i + 1),
        }
    }
    let n = jobs.len();
    let mut done = vec![false; n];
    let mut t: u64 = 0;
    let mut idle: u64 = 0;
    let mut total_wait: u64 = 0;
    for _ in 0..n {
        let mut best: Option<usize> = None;
        for i in 0..n {
            if done[i] || jobs[i].arr > t {
                continue;
            }
            best = match best {
                None => Some(i),
                Some(b) => {
                    let (x, y) = (&jobs[i], &jobs[b]);
                    let better = x.pri > y.pri
                        || (x.pri == y.pri && (x.arr < y.arr || (x.arr == y.arr && x.name < y.name)));
                    if better { Some(i) } else { Some(b) }
                }
            };
        }
        let b = match best {
            Some(b) => b,
            None => {
                let next = (0..n).filter(|&i| !done[i]).map(|i| jobs[i].arr).min().unwrap();
                idle += next - t;
                t = next;
                continue_pick(&jobs, &done, t)
            }
        };
        let j = &jobs[b];
        println!("t={} {} (waited {})", t, j.name, t - j.arr);
        total_wait += t - j.arr;
        t += j.dur;
        done[b] = true;
    }
    let avg = if n == 0 { 0.0 } else { total_wait as f64 / n as f64 };
    println!("avg wait: {:.2}", avg);
    println!("idle: {}", idle);
    println!("end: {}", t);
}

fn continue_pick(jobs: &[J], done: &[bool], t: u64) -> usize {
    let mut best: Option<usize> = None;
    for i in 0..jobs.len() {
        if done[i] || jobs[i].arr > t {
            continue;
        }
        best = match best {
            None => Some(i),
            Some(b) => {
                let (x, y) = (&jobs[i], &jobs[b]);
                let better = x.pri > y.pri
                    || (x.pri == y.pri && (x.arr < y.arr || (x.arr == y.arr && x.name < y.name)));
                if better { Some(i) } else { Some(b) }
            }
        };
    }
    best.unwrap()
}
