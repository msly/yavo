use std::io::{self, Read};

fn num(s: &str) -> Option<usize> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: usize = s.parse().ok()?;
    if n == 0 { None } else { Some(n) }
}

fn main() {
    let mut inp = String::new();
    io::stdin().read_to_string(&mut inp).unwrap();
    let mut lines = inp.lines();
    let mut text: Vec<u8> = lines.next().unwrap_or("").bytes().collect();
    let mut cur = text.len();
    let mut undo: Vec<(Vec<u8>, usize)> = Vec::new();
    let mut out = String::new();
    let mut k = 1;
    for line in lines {
        k += 1;
        let f: Vec<&str> = line.split_whitespace().collect();
        let mut ok = true;
        if line.starts_with("I ") && line.len() > 2 {
            undo.push((text.clone(), cur));
            let ins = &line.as_bytes()[2..];
            for (i, &c) in ins.iter().enumerate() {
                text.insert(cur + i, c);
            }
            cur += ins.len();
        } else if f.len() == 1 && f[0] == "P" && line == "P" {
            let mut s = String::from_utf8_lossy(&text[..cur]).to_string();
            s.push('|');
            s.push_str(&String::from_utf8_lossy(&text[cur..]));
            out.push_str(&s);
            out.push('\n');
        } else if f.len() == 1 && f[0] == "U" {
            if let Some((t, c)) = undo.pop() {
                text = t;
                cur = c;
            } else {
                out.push_str("nothing to undo\n");
            }
        } else if f.len() == 2 && matches!(f[0], "L" | "R" | "D" | "B") && num(f[1]).is_some() {
            let n = num(f[1]).unwrap();
            match f[0] {
                "L" => cur = cur.saturating_sub(n),
                "R" => cur = (cur + n).min(text.len()),
                "D" => {
                    undo.push((text.clone(), cur));
                    let e = (cur + n).min(text.len());
                    text.drain(cur..e);
                }
                _ => {
                    undo.push((text.clone(), cur));
                    let s = cur.saturating_sub(n);
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
    out.push_str(&format!("final: {}\nlength: {}\n", String::from_utf8_lossy(&text), text.len()));
    print!("{}", out);
}
