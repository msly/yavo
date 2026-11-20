use std::io::{stdin, Read};
use std::collections::BTreeMap;
#[derive(Default)]
struct Node { size: u64, kids: BTreeMap<String, Node>, file: bool }
fn total(n: &Node) -> u64 { if n.file { n.size } else { n.kids.values().map(total).sum() } }
fn show(n: &Node, d: usize) {
    for (k, c) in &n.kids {
        if c.file { println!("{}{} {}", " ".repeat(d * 2), k, c.size); }
        else { println!("{}{}/ {}", " ".repeat(d * 2), k, total(c)); show(c, d + 1); }
    }
}
fn main() {
    let mut inp = String::new();
    stdin().read_to_string(&mut inp).unwrap();
    let mut root = Node::default();
    for l in inp.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() != 2 { continue; }
        let sz: u64 = match f[1].parse() { Ok(v) => v, Err(_) => continue };
        let parts: Vec<&str> = f[0].split('/').filter(|p| !p.is_empty()).collect();
        let mut cur = &mut root;
        for (i, p) in parts.iter().enumerate() {
            cur = cur.kids.entry(p.to_string()).or_default();
            if i == parts.len() - 1 { cur.file = true; cur.size += sz; }
        }
    }
    println!("/ {}", total(&root));
    show(&root, 1);
}
