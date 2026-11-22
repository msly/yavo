use std::io::{self, Read};
use std::fmt::Write;
fn parse(line: &str) -> Option<Vec<String>> {
    let b = line.as_bytes();
    let mut i = 0;
    let mut fields = Vec::new();
    loop {
        let mut field = Vec::new();
        if i < b.len() && b[i] == b'"' {
            i += 1;
            let mut closed = false;
            while i < b.len() {
                if b[i] == b'"' {
                    if i + 1 < b.len() && b[i + 1] == b'"' {
                        field.push(b'"'); i += 2;
                    } else { i += 1; closed = true; break; }
                } else { field.push(b[i]); i += 1; }
            }
            if !closed || (i < b.len() && b[i] != b',') { return None; }
        } else {
            while i < b.len() && b[i] != b',' {
                if b[i] == b'"' { return None; }
                field.push(b[i]); i += 1;
            }
        }
        fields.push(String::from_utf8(field).unwrap());
        if i == b.len() { break; }
        i += 1;
    }
    Some(fields)
}
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut output = String::new();
    for (i, line) in input.lines().enumerate() {
        if let Some(fields) = parse(line) {
            writeln!(output, "{}: {}", i + 1, fields.len()).unwrap();
            for field in fields { writeln!(output, "{}:{}", field.len(), field).unwrap(); }
        } else { writeln!(output, "{}: invalid", i + 1).unwrap(); }
    }
    print!("{}", output);
}
