use std::io::{self, Read};
use std::fmt::Write;
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut lines = input.lines();
    let base: Vec<&str> = lines.next().unwrap().split('/').filter(|s| !s.is_empty()).collect();
    let mut output = String::new();
    for query in lines {
        let mut components = if query.starts_with('/') { Vec::new() } else { base.clone() };
        for component in query.split('/') {
            match component {
                "" | "." => (),
                ".." => { components.pop(); }
                _ => components.push(component),
            }
        }
        writeln!(output, "/{}", components.join("/")).unwrap();
    }
    print!("{}", output);
}
