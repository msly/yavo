use crate::parse::Course;
use std::collections::HashMap;

pub fn plan(list: &[Course], cap: u64) -> Result<Vec<String>, String> {
    let n = list.len();
    let idx: HashMap<&str, usize> = list.iter().enumerate().map(|(i, c)| (c.name.as_str(), i)).collect();
    let pre: Vec<Vec<usize>> = list.iter().map(|c| c.prereqs.iter().map(|p| idx[p.as_str()]).collect()).collect();
    let mut dep: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, ps) in pre.iter().enumerate() {
        for &p in ps {
            dep[p].push(i);
        }
    }
    // Kahn order
    let mut indeg: Vec<usize> = pre.iter().map(|p| p.len()).collect();
    let mut order: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut h = 0;
    while h < order.len() {
        let u = order[h];
        h += 1;
        for &v in &dep[u] {
            indeg[v] -= 1;
            if indeg[v] == 0 {
                order.push(v);
            }
        }
    }
    if order.len() < n {
        let mut done = vec![false; n];
        for &u in &order {
            done[u] = true;
        }
        let mut names: Vec<&str> = (0..n).filter(|&i| !done[i]).map(|i| list[i].name.as_str()).collect();
        names.sort();
        return Err(format!("cycle: {}", names.join(" ")));
    }
    let mut chain = vec![1usize; n];
    for &u in order.iter().rev() {
        for &v in &dep[u] {
            chain[u] = chain[u].max(1 + chain[v]);
        }
    }
    let mut scheduled = vec![false; n];
    let mut left = n;
    let mut out = Vec::new();
    let mut term = 0;
    while left > 0 {
        term += 1;
        let mut cand: Vec<usize> = (0..n).filter(|&i| !scheduled[i] && pre[i].iter().all(|&p| scheduled[p])).collect();
        cand.sort_by(|&a, &b| chain[b].cmp(&chain[a]).then_with(|| list[a].name.as_bytes().cmp(list[b].name.as_bytes())));
        let mut total = 0;
        let mut names = Vec::new();
        let mut chosen = Vec::new();
        for i in cand {
            if total + list[i].credits <= cap {
                total += list[i].credits;
                names.push(list[i].name.clone());
                chosen.push(i);
            }
        }
        for i in chosen {
            scheduled[i] = true;
            left -= 1;
        }
        out.push(format!("term {}: {} ({})", term, names.join(" "), total));
    }
    out.push(format!("terms: {}", term));
    Ok(out)
}
