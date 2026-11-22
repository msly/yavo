use std::io::{self, Read};

struct N {
    count: u64,
    min: u8,
    kids: Option<(usize, usize)>,
}

fn walk(nodes: &[N], i: usize, path: &mut String, codes: &mut Vec<Option<String>>) {
    match nodes[i].kids {
        None => codes[nodes[i].min as usize] = Some(if path.is_empty() { "0".to_string() } else { path.clone() }),
        Some((l, r)) => {
            path.push('0');
            walk(nodes, l, path, codes);
            path.pop();
            path.push('1');
            walk(nodes, r, path, codes);
            path.pop();
        }
    }
}

fn main() {
    let mut data = Vec::new();
    io::stdin().read_to_end(&mut data).unwrap();
    let mut cnt = [0u64; 256];
    for b in &data {
        cnt[*b as usize] += 1;
    }
    let mut nodes: Vec<N> = Vec::new();
    let mut live: Vec<usize> = Vec::new();
    for b in 0..256 {
        if cnt[b] > 0 {
            nodes.push(N { count: cnt[b], min: b as u8, kids: None });
            live.push(nodes.len() - 1);
        }
    }
    if live.is_empty() {
        println!("bits: 0");
        return;
    }
    while live.len() > 1 {
        let key = |i: usize| (nodes[i].count, nodes[i].min);
        let mut a = 0;
        for k in 1..live.len() {
            if key(live[k]) < key(live[a]) {
                a = k;
            }
        }
        let x = live.remove(a);
        let mut b = 0;
        for k in 1..live.len() {
            if key(live[k]) < key(live[b]) {
                b = k;
            }
        }
        let y = live.remove(b);
        nodes.push(N { count: nodes[x].count + nodes[y].count, min: nodes[x].min.min(nodes[y].min), kids: Some((x, y)) });
        live.push(nodes.len() - 1);
    }
    let mut codes: Vec<Option<String>> = vec![None; 256];
    walk(&nodes, live[0], &mut String::new(), &mut codes);
    let mut out = String::new();
    let (mut bits, mut depth) = (0u64, 0usize);
    for b in 0..256usize {
        if let Some(c) = &codes[b] {
            let name = if (33..=126).contains(&b) { (b as u8 as char).to_string() } else { format!("\\x{:02X}", b) };
            out.push_str(&format!("{} {} {}\n", name, cnt[b], c));
            bits += cnt[b] * c.len() as u64;
            depth = depth.max(c.len());
        }
    }
    out.push_str(&format!("bits: {}\n", bits));
    out.push_str(&format!("depth: {}\n", depth));
    print!("{}", out);
}
