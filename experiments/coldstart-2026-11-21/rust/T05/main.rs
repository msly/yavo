mod parse;
mod plan;

use std::io::{self, Read};

fn main() {
    let cap: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut inp = String::new();
    io::stdin().read_to_string(&mut inp).unwrap();
    let result = parse::parse(&inp, cap).and_then(|l| plan::plan(&l, cap));
    match result {
        Ok(lines) => {
            for l in lines {
                println!("{}", l);
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
