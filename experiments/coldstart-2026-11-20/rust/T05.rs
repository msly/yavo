use std::io::*;
use std::collections::{BTreeMap, BTreeSet};
fn main() {
    let mut inp = String::new();
    stdin().read_to_string(&mut inp).unwrap();
    let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for l in inp.lines() {
        if l.trim().is_empty() { continue; }
        let (n, d) = match l.split_once(':') { Some(x) => x, None => (l, "") };
        let n = n.trim().to_string();
        deps.entry(n.clone()).or_default();
        for x in d.split_whitespace() {
            deps.entry(x.to_string()).or_default();
            deps.get_mut(&n).unwrap().insert(x.to_string());
        }
    }
    let mut done: BTreeSet<String> = BTreeSet::new();
    loop {
        let next = deps.iter().find(|(k, v)| !done.contains(*k) && v.iter().all(|d| done.contains(d))).map(|(k, _)| k.clone());
        match next { Some(k) => { println!("{}", k); done.insert(k); } None => break }
    }
    let rest: Vec<&String> = deps.keys().filter(|k| !done.contains(*k)).collect();
    if !rest.is_empty() {
        println!("cycle: {}", rest.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "));
        std::process::exit(1);
    }
}
