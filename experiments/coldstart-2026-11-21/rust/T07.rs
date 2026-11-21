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
    let mut inp = String::new();
    io::stdin().read_to_string(&mut inp).unwrap();
    let mut jobs: Vec<J> = Vec::new();
    for (i, line) in inp.lines().enumerate() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        let ok = if f.len() == 4 {
            match (num(f[1]), num(f[2]), num(f[3])) {
                (Some(a), Some(d), Some(p)) if d > 0 => {
                    jobs.push(J { name: f[0].to_string(), arr: a, dur: d, pri: p });
                    true
                }
                _ => false,
            }
        } else {
            false
        };
        if !ok {
            eprintln!("line {}: invalid", i + 1);
        }
    }
    let n = jobs.len();
    let mut done = vec![false; n];
    let mut t = 0u64;
    let mut idle = 0u64;
    let mut total_wait = 0u64;
    let mut out = String::new();
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
                        || (x.pri == y.pri && (x.arr < y.arr || (x.arr == y.arr && x.name.as_bytes() < y.name.as_bytes())));
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
                // pick again at new time
                let mut bb = 0;
                let mut first = true;
                for i in 0..n {
                    if done[i] || jobs[i].arr > t {
                        continue;
                    }
                    if first {
                        bb = i;
                        first = false;
                    } else {
                        let (x, y) = (&jobs[i], &jobs[bb]);
                        if x.pri > y.pri
                            || (x.pri == y.pri && (x.arr < y.arr || (x.arr == y.arr && x.name.as_bytes() < y.name.as_bytes())))
                        {
                            bb = i;
                        }
                    }
                }
                bb
            }
        };
        let w = t - jobs[b].arr;
        total_wait += w;
        out.push_str(&format!("t={} {} (waited {})\n", t, jobs[b].name, w));
        t += jobs[b].dur;
        done[b] = true;
    }
    let avg = if n == 0 { 0.0 } else { total_wait as f64 / n as f64 };
    out.push_str(&format!("avg wait: {:.2}\nidle: {}\nend: {}\n", avg, idle, t));
    print!("{}", out);
}
