use std::io::{self, Read};
use std::fmt::Write;
use std::thread;
fn encode(segment: Vec<u8>) -> Vec<(u8, usize)> {
    let mut runs: Vec<(u8, usize)> = Vec::new();
    for byte in segment {
        if let Some(last) = runs.last_mut() {
            if last.0 == byte { last.1 += 1; continue; }
        }
        runs.push((byte, 1));
    }
    runs
}
fn main() {
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input).unwrap();
    input.pop();
    let n = input.len();
    let mut workers = Vec::new();
    let mut start = 0;
    for part in 0..3 {
        let len = n / 3 + if part < n % 3 { 1 } else { 0 };
        let segment = input[start..start + len].to_vec();
        start += len;
        workers.push(thread::spawn(move || encode(segment)));
    }
    let mut merged: Vec<(u8, usize)> = Vec::new();
    for worker in workers {
        for (byte, count) in worker.join().unwrap() {
            if let Some(last) = merged.last_mut() {
                if last.0 == byte { last.1 += count; continue; }
            }
            merged.push((byte, count));
        }
    }
    let mut output = String::new();
    for &(byte, count) in &merged { writeln!(output, "{} {}", byte, count).unwrap(); }
    writeln!(output, "runs: {} chars: {}", merged.len(), n).unwrap();
    print!("{}", output);
}
