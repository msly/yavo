mod parse;
mod plan;

use std::io::{self, Read};

fn run(cap: u64, input: &str) -> Result<Vec<String>, String> {
    let cs = parse::parse(input)?;
    plan::validate(&cs, cap)?;
    plan::schedule(&cs, cap)
}

fn main() {
    let cap: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(u64::MAX);
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    match run(cap, &input) {
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
