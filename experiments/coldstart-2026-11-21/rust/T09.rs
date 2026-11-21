use std::collections::BTreeMap;
use std::io::{self, Read};

enum Node {
    File(u64),
    Dir(BTreeMap<String, Node>),
}

fn comps(p: &str) -> Vec<&str> {
    p.split('/').filter(|s| !s.is_empty()).collect()
}

fn pstr(c: &[&str]) -> String {
    format!("/{}", c.join("/"))
}

fn get<'a>(root: &'a Node, c: &[&str]) -> Option<&'a Node> {
    let mut cur = root;
    for x in c {
        match cur {
            Node::Dir(m) => cur = m.get(*x)?,
            Node::File(_) => return None,
        }
    }
    Some(cur)
}

fn get_mut<'a>(root: &'a mut Node, c: &[&str]) -> Option<&'a mut Node> {
    let mut cur = root;
    for x in c {
        match cur {
            Node::Dir(m) => cur = m.get_mut(*x)?,
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

fn run(root: &mut Node, f: &[&str]) -> Result<Option<String>, String> {
    let bad = || "error: bad command".to_string();
    let (need, size_arg) = match f[0] {
        "mkdir" | "rm" | "du" | "ls" => (2, false),
        "touch" => (3, true),
        "mv" => (3, false),
        _ => return Err(bad()),
    };
    if f.len() != need {
        return Err(bad());
    }
    if !f[1].starts_with('/') || (f[0] == "mv" && !f[2].starts_with('/')) {
        return Err(bad());
    }
    let sz = if size_arg {
        if f[2].is_empty() || !f[2].bytes().all(|c| c.is_ascii_digit()) {
            return Err(bad());
        }
        f[2].parse::<u64>().map_err(|_| bad())?
    } else {
        0
    };
    let p = comps(f[1]);
    match f[0] {
        "mkdir" => {
            let mut cur = &mut *root;
            for i in 0..p.len() {
                let m = match cur {
                    Node::Dir(m) => m,
                    Node::File(_) => unreachable!(),
                };
                let e = m.entry(p[i].to_string()).or_insert_with(|| Node::Dir(BTreeMap::new()));
                if let Node::File(_) = e {
                    return Err(format!("error: not a directory {}", pstr(&p[..=i])));
                }
                cur = e;
            }
            Ok(None)
        }
        "touch" => {
            if p.is_empty() {
                return Err("error: is a directory /".to_string());
            }
            let par = &p[..p.len() - 1];
            match get_mut(root, par) {
                Some(Node::Dir(m)) => {
                    if let Some(Node::Dir(_)) = m.get(p[p.len() - 1]) {
                        return Err(format!("error: is a directory {}", f[1]));
                    }
                    m.insert(p[p.len() - 1].to_string(), Node::File(sz));
                    Ok(None)
                }
                _ => Err(format!("error: no such directory {}", pstr(par))),
            }
        }
        "rm" => {
            if p.is_empty() {
                return Err("error: cannot remove root".to_string());
            }
            if get(root, &p).is_none() {
                return Err(format!("error: no such path {}", pstr(&p)));
            }
            if let Some(Node::Dir(m)) = get_mut(root, &p[..p.len() - 1]) {
                m.remove(p[p.len() - 1]);
            }
            Ok(None)
        }
        "du" => match get(root, &p) {
            None => Err(format!("error: no such path {}", pstr(&p))),
            Some(n) => Ok(Some(format!("{} {}", pstr(&p), size(n)))),
        },
        "ls" => match get(root, &p) {
            None => Err(format!("error: no such path {}", pstr(&p))),
            Some(Node::File(_)) => Ok(Some(pstr(&p))),
            Some(Node::Dir(m)) => {
                if m.is_empty() {
                    Ok(Some(format!("{}: (empty)", pstr(&p))))
                } else {
                    let v: Vec<String> = m
                        .iter()
                        .map(|(k, v)| if let Node::Dir(_) = v { format!("{}/", k) } else { k.clone() })
                        .collect();
                    Ok(Some(format!("{}: {}", pstr(&p), v.join(" "))))
                }
            }
        },
        _ => {
            // mv
            if get(root, &p).is_none() {
                return Err(format!("error: no such path {}", pstr(&p)));
            }
            if p.is_empty() {
                return Err("error: cannot move root".to_string());
            }
            let d = comps(f[2]);
            let target: Vec<&str> = match get(root, &d) {
                Some(Node::Dir(_)) => {
                    let mut t = d.clone();
                    t.push(p[p.len() - 1]);
                    t
                }
                Some(Node::File(_)) => return Err(format!("error: exists {}", pstr(&d))),
                None => {
                    let par = &d[..d.len() - 1];
                    match get(root, par) {
                        Some(Node::Dir(_)) => d.clone(),
                        _ => return Err(format!("error: no such directory {}", pstr(par))),
                    }
                }
            };
            if target.len() >= p.len() && target[..p.len()] == p[..] {
                return Err("error: cannot move into itself".to_string());
            }
            if get(root, &target).is_some() {
                return Err(format!("error: exists {}", pstr(&target)));
            }
            let node = match get_mut(root, &p[..p.len() - 1]) {
                Some(Node::Dir(m)) => m.remove(p[p.len() - 1]).unwrap(),
                _ => unreachable!(),
            };
            if let Some(Node::Dir(m)) = get_mut(root, &target[..target.len() - 1]) {
                m.insert(target[target.len() - 1].to_string(), node);
            }
            Ok(None)
        }
    }
}

fn main() {
    let mut inp = String::new();
    io::stdin().read_to_string(&mut inp).unwrap();
    let mut root = Node::Dir(BTreeMap::new());
    let mut out = String::new();
    for line in inp.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        match run(&mut root, &f) {
            Ok(Some(s)) => out.push_str(&format!("{}\n", s)),
            Ok(None) => {}
            Err(e) => out.push_str(&format!("{}\n", e)),
        }
    }
    out.push_str(&format!("/ {}\n", size(&root)));
    dump(&root, 1, &mut out);
    print!("{}", out);
}
