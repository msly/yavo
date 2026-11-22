use std::io::{self, Read};
use std::fmt::Write;
enum Token { Star, Any, Byte(u8) }
fn matches(pattern: &str, text: &str) -> Option<bool> {
    let bytes = pattern.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 1;
                if i == bytes.len() { return None; }
                tokens.push(Token::Byte(bytes[i]));
            }
            b'*' => tokens.push(Token::Star),
            b'?' => tokens.push(Token::Any),
            b => tokens.push(Token::Byte(b)),
        }
        i += 1;
    }
    let text = text.as_bytes();
    let mut previous = vec![false; text.len() + 1];
    previous[0] = true;
    for token in tokens {
        let mut current = vec![false; text.len() + 1];
        match token {
            Token::Star => {
                current[0] = previous[0];
                for j in 1..=text.len() { current[j] = previous[j] || current[j - 1]; }
            }
            Token::Any => {
                for j in 1..=text.len() { current[j] = previous[j - 1]; }
            }
            Token::Byte(b) => {
                for j in 1..=text.len() { current[j] = previous[j - 1] && text[j - 1] == b; }
            }
        }
        previous = current;
    }
    Some(previous[text.len()])
}
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut lines = input.lines();
    let mut index = 0;
    let mut output = String::new();
    while let Some(pattern) = lines.next() {
        let text = lines.next().unwrap();
        index += 1;
        let result = match matches(pattern, text) { Some(true) => "yes", Some(false) => "no", None => "invalid" };
        writeln!(output, "{}: {}", index, result).unwrap();
    }
    print!("{}", output);
}
