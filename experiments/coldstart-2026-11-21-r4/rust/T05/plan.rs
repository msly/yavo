use crate::parse::Course;
use std::collections::{HashMap, HashSet};

pub fn validate(cs: &[Course], cap: u64) -> Result<(), String> {
    let mut seen = HashSet::new();
    for c in cs {
        if !seen.insert(c.name.as_str()) {
            return Err(format!("duplicate {}", c.name));
        }
    }
    for c in cs {
        for p in &c.prereq {
            if !seen.contains(p.as_str()) {
                return Err(format!("unknown {}", p));
            }
        }
    }
    for c in cs {
        if c.credit > cap {
            return Err(format!("too big {}", c.name));
        }
    }
    Ok(())
}

fn chain(i: usize, deps: &Vec<Vec<usize>>, memo: &mut Vec<usize>) -> usize {
    if memo[i] != 0 {
        return memo[i];
    }
    let mut best = 0;
    for &d in &deps[i] {
        best = best.max(chain(d, deps, memo));
    }
    memo[i] = best + 1;
    memo[i]
}

pub fn schedule(cs: &[Course], cap: u64) -> Result<Vec<String>, String> {
    let idx: HashMap<&str, usize> = cs.iter().enumerate().map(|(i, c)| (c.name.as_str(), i)).collect();
    let n = cs.len();
    let pre: Vec<Vec<usize>> = cs.iter().map(|c| c.prereq.iter().map(|p| idx[p.as_str()]).collect()).collect();
    let mut deps = vec![Vec::new(); n];
    for (i, p) in pre.iter().enumerate() {
        for &q in p {
            deps[q].push(i);
        }
    }
    // detect unschedulable courses
    let mut done = vec![false; n];
    loop {
        let mut changed = false;
        for i in 0..n {
            if !done[i] && pre[i].iter().all(|&p| done[p]) {
                done[i] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    if done.iter().any(|d| !d) {
        let mut names: Vec<&str> = (0..n).filter(|&i| !done[i]).map(|i| cs[i].name.as_str()).collect();
        names.sort();
        return Err(format!("cycle: {}", names.join(" ")));
    }
    let mut memo = vec![0usize; n];
    for i in 0..n {
        chain(i, &deps, &mut memo);
    }
    let mut sched = vec![false; n];
    let mut left = n;
    let mut out = Vec::new();
    let mut term = 0;
    while left > 0 {
        term += 1;
        let mut cand: Vec<usize> = (0..n).filter(|&i| !sched[i] && pre[i].iter().all(|&p| sched[p])).collect();
        cand.sort_by(|&a, &b| memo[b].cmp(&memo[a]).then_with(|| cs[a].name.cmp(&cs[b].name)));
        let mut total = 0;
        let mut names = Vec::new();
        let mut picked = Vec::new();
        for i in cand {
            if total + cs[i].credit <= cap {
                total += cs[i].credit;
                names.push(cs[i].name.clone());
                picked.push(i);
            }
        }
        for i in picked {
            sched[i] = true;
            left -= 1;
        }
        out.push(format!("term {}: {} ({})", term, names.join(" "), total));
    }
    out.push(format!("terms: {}", term));
    Ok(out)
}
