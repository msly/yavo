use std::io::{stdin, stdout, Read, Write};
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let mut inp = Vec::new();
    stdin().read_to_end(&mut inp).unwrap();
    let mut out: Vec<u8> = Vec::new();
    if mode == "encode" {
        let mut i = 0;
        while i < inp.len() {
            let mut j = i;
            while j < inp.len() && inp[j] == inp[i] { j += 1; }
            out.extend_from_slice((j - i).to_string().as_bytes());
            out.push(inp[i]);
            i = j;
        }
    } else {
        let mut start: Option<usize> = None;
        let mut cnt: u64 = 0;
        for (i, &b) in inp.iter().enumerate() {
            if b.is_ascii_digit() {
                if start.is_none() { start = Some(i); cnt = 0; }
                cnt = cnt.saturating_mul(10).saturating_add((b - b'0') as u64);
            } else {
                match start {
                    None => { eprintln!("invalid input at byte {}", i); std::process::exit(1); }
                    Some(s) => {
                        if cnt == 0 { eprintln!("invalid input at byte {}", s); std::process::exit(1); }
                        for _ in 0..cnt { out.push(b); }
                        start = None;
                    }
                }
            }
        }
        if let Some(s) = start { eprintln!("invalid input at byte {}", s); std::process::exit(1); }
    }
    stdout().write_all(&out).unwrap();
}
