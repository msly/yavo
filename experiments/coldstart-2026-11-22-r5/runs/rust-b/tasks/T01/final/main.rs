use std::collections::BTreeMap;
use std::fmt::Write;
use std::io::{self, Read};

fn transfer(accounts: &mut BTreeMap<String, i64>, from: &str, to: &str, amount: i64) -> Result<(), String> {
    let balance = match accounts.get(from) {
        Some(v) => *v,
        None => return Err(format!("unknown {}", from)),
    };
    if !accounts.contains_key(to) { return Err(format!("unknown {}", to)); }
    if balance < amount { return Err(format!("funds {}", from)); }
    if from != to {
        *accounts.get_mut(from).unwrap() -= amount;
        *accounts.get_mut(to).unwrap() += amount;
    }
    Ok(())
}
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut lines = input.lines();
    let mut accounts = BTreeMap::<String, i64>::new();
    let mut output = String::new();
    while let Some(line) = lines.next() {
        let p: Vec<&str> = line.split_whitespace().collect();
        if p.is_empty() { continue; }
        match p[0] {
            "account" => {
                if accounts.contains_key(p[1]) { writeln!(output, "error: duplicate {}", p[1]).unwrap(); }
                else { accounts.insert(p[1].to_string(), p[2].parse().unwrap()); }
            }
            "pay" => {
                match transfer(&mut accounts, p[1], p[2], p[3].parse().unwrap()) {
                    Ok(()) => writeln!(output, "paid").unwrap(),
                    Err(e) => writeln!(output, "error: {}", e).unwrap(),
                }
            }
            "batch" => {
                let k: usize = p[1].parse().unwrap();
                let mut temp = accounts.clone();
                let mut error = None;
                for j in 1..=k {
                    let row: Vec<&str> = lines.next().unwrap().split_whitespace().collect();
                    if error.is_none() {
                        if let Err(e) = transfer(&mut temp, row[0], row[1], row[2].parse().unwrap()) {
                            error = Some(format!("batch: row {} {}", j, e));
                        }
                    }
                }
                if let Some(e) = error { writeln!(output, "{}", e).unwrap(); }
                else { accounts = temp; writeln!(output, "batch: ok").unwrap(); }
            }
            "balance" => {
                if let Some(v) = accounts.get(p[1]) { writeln!(output, "{}: {}", p[1], v).unwrap(); }
                else { writeln!(output, "error: unknown {}", p[1]).unwrap(); }
            }
            _ => unreachable!(),
        }
    }
    let mut total = 0i64;
    for (name, balance) in accounts { writeln!(output, "{}={}", name, balance).unwrap(); total += balance; }
    writeln!(output, "total: {}", total).unwrap();
    print!("{}", output);
}
