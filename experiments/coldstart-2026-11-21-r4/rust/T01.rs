use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{self, Read};

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut g: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut out = String::new();
    for line in input.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        let need = match f[0] {
            "add" | "remove" | "friends" => 2,
            "link" | "unlink" | "path" => 3,
            _ => {
                out.push_str("error: bad command\n");
                continue;
            }
        };
        if f.len() != need {
            out.push_str("error: bad command\n");
            continue;
        }
        if f[0] == "add" {
            if g.contains_key(f[1]) {
                out.push_str(&format!("error: {} exists\n", f[1]));
            } else {
                g.insert(f[1].to_string(), BTreeSet::new());
            }
            continue;
        }
        let mut missing = false;
        for a in &f[1..] {
            if !g.contains_key(*a) {
                out.push_str(&format!("error: no user {}\n", a));
                missing = true;
                break;
            }
        }
        if missing {
            continue;
        }
        match f[0] {
            "link" => {
                if f[1] == f[2] {
                    out.push_str("error: self link\n");
                } else {
                    g.get_mut(f[1]).unwrap().insert(f[2].to_string());
                    g.get_mut(f[2]).unwrap().insert(f[1].to_string());
                }
            }
            "unlink" => {
                if f[1] == f[2] || !g[f[1]].contains(f[2]) {
                    out.push_str("error: not linked\n");
                } else {
                    g.get_mut(f[1]).unwrap().remove(f[2]);
                    g.get_mut(f[2]).unwrap().remove(f[1]);
                }
            }
            "remove" => {
                let fr = g.remove(f[1]).unwrap();
                for x in fr {
                    g.get_mut(&x).unwrap().remove(f[1]);
                }
            }
            "friends" => {
                let s = &g[f[1]];
                if s.is_empty() {
                    out.push_str(&format!("{}: (none)\n", f[1]));
                } else {
                    let v: Vec<&str> = s.iter().map(|x| x.as_str()).collect();
                    out.push_str(&format!("{}: {}\n", f[1], v.join(" ")));
                }
            }
            "path" => {
                let (a, b) = (f[1], f[2]);
                let mut par: BTreeMap<&str, &str> = BTreeMap::new();
                let mut seen: BTreeSet<&str> = BTreeSet::new();
                seen.insert(a);
                let mut q = VecDeque::new();
                q.push_back(a);
                while let Some(u) = q.pop_front() {
                    if u == b {
                        break;
                    }
                    for v in &g[u] {
                        if seen.insert(v.as_str()) {
                            par.insert(v.as_str(), u);
                            q.push_back(v.as_str());
                        }
                    }
                }
                if seen.contains(b) {
                    let mut p = vec![b];
                    let mut c = b;
                    while c != a {
                        c = par[c];
                        p.push(c);
                    }
                    p.reverse();
                    out.push_str(&format!("path: {}\n", p.join(" -> ")));
                } else {
                    out.push_str("path: none\n");
                }
            }
            _ => {}
        }
    }
    let m: usize = g.values().map(|s| s.len()).sum::<usize>() / 2;
    out.push_str(&format!("users: {} links: {}\n", g.len(), m));
    print!("{}", out);
}
