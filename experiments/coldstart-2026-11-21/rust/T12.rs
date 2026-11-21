use std::io::{self, Read};

enum T {
    Leaf(u8),
    Node(Box<T>, Box<T>),
}

fn walk(t: &T, pre: &mut String, codes: &mut Vec<String>) {
    match t {
        T::Leaf(b) => codes[*b as usize] = if pre.is_empty() { "0".to_string() } else { pre.clone() },
        T::Node(l, r) => {
            pre.push('0');
            walk(l, pre, codes);
            pre.pop();
            pre.push('1');
            walk(r, pre, codes);
            pre.pop();
        }
    }
}

fn main() {
    let mut inp = Vec::new();
    io::stdin().read_to_end(&mut inp).unwrap();
    let mut cnt = [0u64; 256];
    for &b in &inp {
        cnt[b as usize] += 1;
    }
    let mut nodes: Vec<(u64, u8, T)> = (0..256).filter(|&b| cnt[b] > 0).map(|b| (cnt[b], b as u8, T::Leaf(b as u8))).collect();
    if nodes.is_empty() {
        println!("bits: 0");
        return;
    }
    while nodes.len() > 1 {
        let mut take = |nodes: &mut Vec<(u64, u8, T)>| {
            let mut bi = 0;
            for i in 1..nodes.len() {
                if (nodes[i].0, nodes[i].1) < (nodes[bi].0, nodes[bi].1) {
                    bi = i;
                }
            }
            nodes.swap_remove(bi)
        };
        let x = take(&mut nodes);
        let y = take(&mut nodes);
        nodes.push((x.0 + y.0, x.1.min(y.1), T::Node(Box::new(x.2), Box::new(y.2))));
    }
    let root = nodes.pop().unwrap();
    let mut codes = vec![String::new(); 256];
    walk(&root.2, &mut String::new(), &mut codes);
    let mut out = String::new();
    let mut bits = 0;
    let mut depth = 0;
    for b in 0..256usize {
        if cnt[b] == 0 {
            continue;
        }
        let name = if (33..=126).contains(&b) { (b as u8 as char).to_string() } else { format!("\\x{:02X}", b) };
        out.push_str(&format!("{} {} {}\n", name, cnt[b], codes[b]));
        bits += cnt[b] * codes[b].len() as u64;
        depth = depth.max(codes[b].len());
    }
    out.push_str(&format!("bits: {}\ndepth: {}\n", bits, depth));
    print!("{}", out);
}
