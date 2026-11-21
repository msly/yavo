use std::collections::HashMap;
use std::io::Read;

struct Entry {
    name: String,
    score: i64,
}

fn longer<'a>(a: &'a str, b: &'a str) -> &'a str {
    if b.len() > a.len() {
        return b;
    }
    a
}

fn parse(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() != 2 {
            continue;
        }
        let s: i64 = match f[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        out.push(Entry { name: f[0].to_string(), score: s });
    }
    out
}

fn main() {
    let mut text = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut text) {
        eprintln!("read error: {}", e);
        std::process::exit(1);
    }
    let mut entries = parse(&text);
    if entries.is_empty() {
        println!("no entries");
        return;
    }
    let mut names: Vec<String> = Vec::new();
    for e in &entries {
        names.push(e.name.clone());
    }
    let saved = names.clone();
    let first: &str = names[0].trim();
    println!("first: {}", first);
    names.push("zz-sentinel".to_string());
    println!("count: {}", names.len() - 1);
    let mut best = "";
    for n in &saved {
        best = longer(best, n);
    }
    println!("longest: {}", best);
    entries.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(&b.name)));
    let top = entries[0].name.clone();
    println!("top: {}", top);
    let mut groups: HashMap<String, i64> = HashMap::new();
    for e in &entries {
        *groups.entry(e.name[0..1].to_string()).or_insert(0) += 1;
    }
    let mut singles: Vec<String> = Vec::new();
    for (k, v) in &groups {
        if *v == 1 {
            singles.push(k.clone());
        }
    }
    for k in singles {
        groups.insert(format!("{}*", k), 0);
    }
    let mut keys: Vec<String> = groups.keys().cloned().collect();
    keys.sort();
    for k in keys {
        println!("{}: {}", k, groups.get(&k).copied().unwrap_or(0));
    }
}
