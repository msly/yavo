use std::io::{self, Read};
use std::fmt::Write;
struct Entry { key: String, value: i64, expires: i64 }
fn show(entries: &[Entry], output: &mut String) {
    if entries.is_empty() { writeln!(output, "keys: none").unwrap(); }
    else { writeln!(output, "keys: {}", entries.iter().map(|e| e.key.as_str()).collect::<Vec<_>>().join(" ")).unwrap(); }
}
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut lines = input.lines();
    let capacity: usize = lines.next().unwrap().trim().parse().unwrap();
    let mut entries: Vec<Entry> = Vec::new();
    let mut output = String::new();
    for line in lines {
        let p: Vec<&str> = line.split_whitespace().collect();
        if p.is_empty() { continue; }
        let time: i64 = p[0].parse().unwrap();
        entries.retain(|e| e.expires > time);
        match p[1] {
            "put" => {
                let key = p[2];
                let value: i64 = p[3].parse().unwrap();
                let ttl: i64 = p[4].parse().unwrap();
                entries.retain(|e| e.key != key);
                if ttl > 0 && capacity > 0 {
                    entries.insert(0, Entry { key: key.to_string(), value, expires: time + ttl });
                    entries.truncate(capacity);
                }
            }
            "get" => {
                let key = p[2];
                if let Some(index) = entries.iter().position(|e| e.key == key) {
                    let entry = entries.remove(index);
                    writeln!(output, "{}: {}", key, entry.value).unwrap();
                    entries.insert(0, entry);
                } else { writeln!(output, "{}: missing", key).unwrap(); }
            }
            "keys" => show(&entries, &mut output),
            _ => unreachable!(),
        }
    }
    show(&entries, &mut output);
    print!("{}", output);
}
