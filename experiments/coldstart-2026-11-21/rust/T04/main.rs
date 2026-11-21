mod config;

use config::{Config, Fail};
use std::io::{self, Read};

fn main() {
    let mut inp = String::new();
    io::stdin().read_to_string(&mut inp).unwrap();
    let cfg = match Config::parse(&inp) {
        Ok(c) => c,
        Err(n) => {
            eprintln!("line {}: syntax error", n);
            std::process::exit(2);
        }
    };
    let mut out = String::new();
    for q in std::env::args().skip(1) {
        if !cfg.has(&q) {
            out.push_str(&format!("{}: not found\n", q));
            continue;
        }
        match cfg.expand(&q) {
            Ok(v) => out.push_str(&format!("{} = {}\n", q, v)),
            Err(Fail::Missing(r)) => out.push_str(&format!("{}: missing {}\n", q, r)),
            Err(Fail::Cycle) => out.push_str(&format!("{}: cycle\n", q)),
        }
    }
    print!("{}", out);
}
