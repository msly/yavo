use std::io::{self, Read};
use std::fmt::Write;
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut tokens = input.split_whitespace();
    let k: usize = tokens.next().unwrap().parse().unwrap();
    let values: Vec<i64> = tokens.map(|s| s.parse().unwrap()).collect();
    let mut output = String::new();
    let mut count = 0;
    for (start, window) in values.windows(k).enumerate() {
        let mut ordered = window.to_vec();
        ordered.sort_unstable();
        let sum: i64 = window.iter().sum();
        writeln!(output, "{}: median={} sum={}", start + k, ordered[(k - 1) / 2], sum).unwrap();
        count += 1;
    }
    writeln!(output, "windows: {}", count).unwrap();
    print!("{}", output);
}
