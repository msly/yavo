use std::io::{self, Read};
use std::fmt::Write;
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut intervals: Vec<(i64, i64)> = Vec::new();
    let mut output = String::new();
    for line in input.lines() {
        let p: Vec<&str> = line.split_whitespace().collect();
        if p.is_empty() { continue; }
        match p[0] {
            "add" => {
                intervals.push((p[1].parse().unwrap(), p[2].parse().unwrap()));
                intervals.sort_unstable();
                let mut merged: Vec<(i64, i64)> = Vec::new();
                for (l, r) in intervals {
                    if let Some(last) = merged.last_mut() {
                        if l <= last.1 + 1 { last.1 = last.1.max(r); continue; }
                    }
                    merged.push((l, r));
                }
                intervals = merged;
            }
            "del" => {
                let l: i64 = p[1].parse().unwrap();
                let r: i64 = p[2].parse().unwrap();
                let mut next = Vec::new();
                for (a, b) in intervals {
                    if b < l || a > r { next.push((a, b)); }
                    else {
                        if a < l { next.push((a, l - 1)); }
                        if b > r { next.push((r + 1, b)); }
                    }
                }
                intervals = next;
            }
            "has" => {
                let x: i64 = p[1].parse().unwrap();
                writeln!(output, "{}", if intervals.iter().any(|&(l, r)| l <= x && x <= r) { "yes" } else { "no" }).unwrap();
            }
            "show" => {
                if intervals.is_empty() { writeln!(output, "empty").unwrap(); }
                else {
                    let s: Vec<String> = intervals.iter().map(|(l, r)| format!("{}:{}", l, r)).collect();
                    writeln!(output, "{}", s.join(" ")).unwrap();
                }
            }
            _ => unreachable!(),
        }
    }
    let count: i64 = intervals.iter().map(|&(l, r)| r - l + 1).sum();
    writeln!(output, "count: {}", count).unwrap();
    print!("{}", output);
}
