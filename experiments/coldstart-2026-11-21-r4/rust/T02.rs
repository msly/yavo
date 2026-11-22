use std::io::{self, Read};

fn pnum(s: &str) -> Option<usize> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let v: usize = s.parse().unwrap_or(usize::MAX);
    if v == 0 { None } else { Some(v) }
}

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut lines = input.lines();
    let mut text: Vec<u8> = lines.next().unwrap_or("").bytes().collect();
    let mut cur = text.len();
    let mut undo: Vec<(Vec<u8>, usize)> = Vec::new();
    let mut out = String::new();
    for (idx, line) in lines.enumerate() {
        let k = idx + 2;
        let f: Vec<&str> = line.split_whitespace().collect();
        let mut ok = true;
        if line.starts_with("I ") && line.len() > 2 {
            undo.push((text.clone(), cur));
            let ins = &line.as_bytes()[2..];
            for (i, b) in ins.iter().enumerate() {
                text.insert(cur + i, *b);
            }
            cur += ins.len();
        } else if f.len() == 1 && f[0] == "P" && line == "P" {
            let s = String::from_utf8_lossy(&text[..cur]).to_string()
                + "|"
                + &String::from_utf8_lossy(&text[cur..]);
            out.push_str(&s);
            out.push('\n');
        } else if line == "U" {
            match undo.pop() {
                Some((t, c)) => {
                    text = t;
                    cur = c;
                }
                None => out.push_str("nothing to undo\n"),
            }
        } else if f.len() == 2 && matches!(f[0], "L" | "R" | "D" | "B") && pnum(f[1]).is_some() {
            let n = pnum(f[1]).unwrap();
            match f[0] {
                "L" => cur -= n.min(cur),
                "R" => cur += n.min(text.len() - cur),
                "D" => {
                    undo.push((text.clone(), cur));
                    let e = cur.saturating_add(n).min(text.len());
                    text.drain(cur..e);
                }
                _ => {
                    undo.push((text.clone(), cur));
                    let s = cur - n.min(cur);
                    text.drain(s..cur);
                    cur = s;
                }
            }
        } else {
            ok = false;
        }
        if !ok {
            out.push_str(&format!("bad command at line {}\n", k));
        }
    }
    out.push_str(&format!("final: {}\n", String::from_utf8_lossy(&text)));
    out.push_str(&format!("length: {}\n", text.len()));
    print!("{}", out);
}
