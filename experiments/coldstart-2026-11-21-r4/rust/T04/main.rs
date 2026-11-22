mod config;

use std::io::{self, Read};

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let queries: Vec<String> = std::env::args().skip(1).collect();
    match config::Config::parse(&input) {
        Ok(c) => {
            let mut out = String::new();
            for q in &queries {
                out.push_str(&c.query(q));
                out.push('\n');
            }
            print!("{}", out);
        }
        Err(n) => {
            eprintln!("line {}: syntax error", n);
            std::process::exit(2);
        }
    }
}
