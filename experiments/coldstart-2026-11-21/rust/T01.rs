use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io::{self, Read};

fn main() {
    let mut inp = String::new();
    io::stdin().read_to_string(&mut inp).unwrap();
    let mut g: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut out = String::new();
    for line in inp.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        let need = match f[0] {
            "add" | "remove" | "friends" => 2,
            "link" | "unlink" | "path" => 3,
            _ => 0,
        };
        if need == 0 || f.len() != need {
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
        for x in &f[1..] {
            if !g.contains_key(*x) {
                out.push_str(&format!("error: no user {}\n", x));
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
                if !g[f[1]].contains(f[2]) {
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
                if a == b {
                    out.push_str(&format!("path: {}\n", a));
                    continue;
                }
                let mut par: HashMap<&str, &str> = HashMap::new();
                let mut q: VecDeque<&str> = VecDeque::new();
                par.insert(a, a);
                q.push_back(a);
                let mut found = false;
                'bfs: while let Some(u) = q.pop_front() {
                    for v in &g[u] {
                        if !par.contains_key(v.as_str()) {
                            par.insert(v.as_str(), u);
                            if v == b {
                                found = true;
                                break 'bfs;
                            }
                            q.push_back(v.as_str());
                        }
                    }
                }
                if !found {
                    out.push_str("path: none\n");
                } else {
                    let mut chain = vec![b];
                    let mut cur = b;
                    while cur != a {
                        cur = par[cur];
                        chain.push(cur);
                    }
                    chain.reverse();
                    out.push_str(&format!("path: {}\n", chain.join(" -> ")));
                }
            }
            _ => {}
        }
    }
    let links: usize = g.values().map(|s| s.len()).sum::<usize>() / 2;
    out.push_str(&format!("users: {} links: {}\n", g.len(), links));
    print!("{}", out);
}
