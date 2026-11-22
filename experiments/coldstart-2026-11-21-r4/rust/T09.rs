use std::collections::BTreeMap;
use std::io::{self, Read};

enum Node {
    File(u64),
    Dir(BTreeMap<String, Node>),
}

fn comps(p: &str) -> Option<Vec<&str>> {
    if !p.starts_with('/') {
        return None;
    }
    Some(p.split('/').filter(|s| !s.is_empty()).collect())
}

fn join(c: &[&str]) -> String {
    format!("/{}", c.join("/"))
}

fn get<'a>(root: &'a Node, c: &[&str]) -> Option<&'a Node> {
    let mut cur = root;
    for name in c {
        match cur {
            Node::Dir(m) => cur = m.get(*name)?,
            Node::File(_) => return None,
        }
    }
    Some(cur)
}

fn get_mut<'a>(root: &'a mut Node, c: &[&str]) -> Option<&'a mut Node> {
    let mut cur = root;
    for name in c {
        match cur {
            Node::Dir(m) => cur = m.get_mut(*name)?,
            Node::File(_) => return None,
        }
    }
    Some(cur)
}

fn size(n: &Node) -> u64 {
    match n {
        Node::File(s) => *s,
        Node::Dir(m) => m.values().map(size).sum(),
    }
}

fn is_dir(root: &Node, c: &[&str]) -> bool {
    matches!(get(root, c), Some(Node::Dir(_)))
}

fn dump(n: &Node, depth: usize, out: &mut String) {
    if let Node::Dir(m) = n {
        for (k, v) in m {
            match v {
                Node::File(s) => out.push_str(&format!("{}{} {}\n", "  ".repeat(depth), k, s)),
                Node::Dir(_) => {
                    out.push_str(&format!("{}{}/ {}\n", "  ".repeat(depth), k, size(v)));
                    dump(v, depth + 1, out);
                }
            }
        }
    }
}

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut root = Node::Dir(BTreeMap::new());
    let mut out = String::new();
    for line in input.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        let r = exec(&mut root, &f);
        match r {
            Ok(Some(s)) => {
                out.push_str(&s);
                out.push('\n');
            }
            Ok(None) => {}
            Err(e) => {
                out.push_str("error: ");
                out.push_str(&e);
                out.push('\n');
            }
        }
    }
    out.push_str(&format!("/ {}\n", size(&root)));
    dump(&root, 1, &mut out);
    print!("{}", out);
}

fn exec(root: &mut Node, f: &[&str]) -> Result<Option<String>, String> {
    let bad = || "bad command".to_string();
    let need = match f[0] {
        "mkdir" | "rm" | "du" | "ls" => 2,
        "touch" | "mv" => 3,
        _ => return Err(bad()),
    };
    if f.len() != need {
        return Err(bad());
    }
    let c = comps(f[1]).ok_or_else(bad)?;
    match f[0] {
        "mkdir" => {
            let mut cur = root;
            for (i, name) in c.iter().enumerate() {
                match cur {
                    Node::Dir(m) => {
                        cur = m.entry(name.to_string()).or_insert_with(|| Node::Dir(BTreeMap::new()));
                    }
                    Node::File(_) => unreachable!(),
                }
                if let Node::File(_) = cur {
                    return Err(format!("not a directory {}", join(&c[..=i])));
                }
            }
            Ok(None)
        }
        "touch" => {
            if !f[2].bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            let sz: u64 = f[2].parse().map_err(|_| bad())?;
            if c.is_empty() {
                return Err("is a directory /".to_string());
            }
            let parent = &c[..c.len() - 1];
            match get_mut(root, parent) {
                Some(Node::Dir(m)) => {
                    let name = c[c.len() - 1];
                    if let Some(Node::Dir(_)) = m.get(name) {
                        return Err(format!("is a directory {}", join(&c)));
                    }
                    m.insert(name.to_string(), Node::File(sz));
                    Ok(None)
                }
                _ => Err(format!("no such directory {}", join(parent))),
            }
        }
        "rm" => {
            if get(root, &c).is_none() {
                return Err(format!("no such path {}", join(&c)));
            }
            if c.is_empty() {
                return Err("cannot remove root".to_string());
            }
            if let Some(Node::Dir(m)) = get_mut(root, &c[..c.len() - 1]) {
                m.remove(c[c.len() - 1]);
            }
            Ok(None)
        }
        "du" => match get(root, &c) {
            Some(n) => Ok(Some(format!("{} {}", f[1], size(n)))),
            None => Err(format!("no such path {}", join(&c))),
        },
        "ls" => match get(root, &c) {
            Some(Node::File(_)) => Ok(Some(f[1].to_string())),
            Some(Node::Dir(m)) => {
                if m.is_empty() {
                    Ok(Some(format!("{}: (empty)", f[1])))
                } else {
                    let v: Vec<String> = m
                        .iter()
                        .map(|(k, v)| if let Node::Dir(_) = v { format!("{}/", k) } else { k.clone() })
                        .collect();
                    Ok(Some(format!("{}: {}", f[1], v.join(" "))))
                }
            }
            None => Err(format!("no such path {}", join(&c))),
        },
        _ => {
            // mv
            let d = comps(f[2]).ok_or_else(bad)?;
            if get(root, &c).is_none() {
                return Err(format!("no such path {}", join(&c)));
            }
            if c.is_empty() {
                return Err("cannot move root".to_string());
            }
            let name = c[c.len() - 1];
            let target: Vec<&str> = match get(root, &d) {
                Some(Node::Dir(_)) => {
                    let mut t = d.clone();
                    t.push(name);
                    t
                }
                Some(Node::File(_)) => return Err(format!("exists {}", join(&d))),
                None => {
                    let parent = if d.is_empty() { &d[..] } else { &d[..d.len() - 1] };
                    if !is_dir(root, parent) {
                        return Err(format!("no such directory {}", join(parent)));
                    }
                    d.clone()
                }
            };
            if target.len() >= c.len() && target[..c.len()] == c[..] {
                return Err("cannot move into itself".to_string());
            }
            if get(root, &target).is_some() {
                return Err(format!("exists {}", join(&target)));
            }
            let node = match get_mut(root, &c[..c.len() - 1]) {
                Some(Node::Dir(m)) => m.remove(name).unwrap(),
                _ => unreachable!(),
            };
            match get_mut(root, &target[..target.len() - 1]) {
                Some(Node::Dir(m)) => {
                    m.insert(target[target.len() - 1].to_string(), node);
                }
                _ => unreachable!(),
            }
            Ok(None)
        }
    }
}
