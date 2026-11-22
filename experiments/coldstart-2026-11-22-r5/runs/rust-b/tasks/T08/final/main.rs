use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::fmt::Write;
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut documents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut output = String::new();
    for line in input.lines() {
        let p: Vec<&str> = line.split_whitespace().collect();
        if p.is_empty() { continue; }
        match p[0] {
            "set" => { documents.insert(p[1].to_string(), p[2..].iter().map(|s| s.to_string()).collect()); }
            "add" => { documents.entry(p[1].to_string()).or_default().insert(p[2].to_string()); }
            "remove" => { if let Some(tags) = documents.get_mut(p[1]) { tags.remove(p[2]); } }
            "drop" => { documents.remove(p[1]); }
            "tags" => {
                match documents.get(p[1]) {
                    None => writeln!(output, "{}: missing", p[1]).unwrap(),
                    Some(tags) if tags.is_empty() => writeln!(output, "{}: none", p[1]).unwrap(),
                    Some(tags) => writeln!(output, "{}: {}", p[1], tags.iter().cloned().collect::<Vec<_>>().join(" ")).unwrap(),
                }
            }
            "query" => {
                let matches: Vec<&str> = documents.iter().filter(|(_, tags)| p[1..].iter().all(|tag| tags.contains(*tag))).map(|(name, _)| name.as_str()).collect();
                writeln!(output, "docs: {}", if matches.is_empty() { "none".to_string() } else { matches.join(" ") }).unwrap();
            }
            _ => unreachable!(),
        }
    }
    let distinct: BTreeSet<&str> = documents.values().flat_map(|tags| tags.iter().map(|s| s.as_str())).collect();
    writeln!(output, "total: docs={} tags={}", documents.len(), distinct.len()).unwrap();
    print!("{}", output);
}
