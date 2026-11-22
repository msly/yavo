use std::io::{self, Read};

enum E {
    Num(String, i64),
    Neg(Box<E>),
    Bin(Box<E>, char, Box<E>),
}

#[derive(Clone)]
struct Tok {
    text: String,
    col: usize,
}

struct P {
    t: Vec<Tok>,
    i: usize,
}

type R<T> = Result<T, String>;

impl P {
    fn err(&self) -> String {
        match self.t.get(self.i) {
            Some(k) => format!("error at col {}: unexpected '{}'", k.col, k.text),
            None => "error: unexpected end".to_string(),
        }
    }
    fn peek(&self) -> Option<&str> {
        self.t.get(self.i).map(|k| k.text.as_str())
    }
    fn expr(&mut self) -> R<E> {
        let mut l = self.term()?;
        while let Some(o) = self.peek() {
            if o == "+" || o == "-" {
                let c = o.chars().next().unwrap();
                self.i += 1;
                let r = self.term()?;
                l = E::Bin(Box::new(l), c, Box::new(r));
            } else {
                break;
            }
        }
        Ok(l)
    }
    fn term(&mut self) -> R<E> {
        let mut l = self.unary()?;
        while let Some(o) = self.peek() {
            if o == "*" || o == "/" || o == "%" {
                let c = o.chars().next().unwrap();
                self.i += 1;
                let r = self.unary()?;
                l = E::Bin(Box::new(l), c, Box::new(r));
            } else {
                break;
            }
        }
        Ok(l)
    }
    fn unary(&mut self) -> R<E> {
        if self.peek() == Some("-") {
            self.i += 1;
            let e = self.unary()?;
            return Ok(E::Neg(Box::new(e)));
        }
        self.primary()
    }
    fn primary(&mut self) -> R<E> {
        match self.peek() {
            Some(s) if s.as_bytes()[0].is_ascii_digit() => {
                let s = s.to_string();
                self.i += 1;
                let v = s.parse::<i64>().unwrap_or(i64::MAX);
                Ok(E::Num(s, v))
            }
            Some("(") => {
                self.i += 1;
                let e = self.expr()?;
                if self.peek() == Some(")") {
                    self.i += 1;
                    Ok(e)
                } else {
                    Err(self.err())
                }
            }
            _ => Err(self.err()),
        }
    }
}

fn show(e: &E) -> String {
    match e {
        E::Num(s, _) => s.clone(),
        E::Neg(x) => format!("(-{})", show(x)),
        E::Bin(l, o, r) => format!("({} {} {})", show(l), o, show(r)),
    }
}

fn eval(e: &E) -> Option<i64> {
    match e {
        E::Num(_, v) => Some(*v),
        E::Neg(x) => Some(eval(x)?.wrapping_neg()),
        E::Bin(l, o, r) => {
            let a = eval(l)?;
            let b = eval(r)?;
            Some(match o {
                '+' => a.wrapping_add(b),
                '-' => a.wrapping_sub(b),
                '*' => a.wrapping_mul(b),
                '/' => {
                    if b == 0 {
                        return None;
                    }
                    a.wrapping_div(b)
                }
                _ => {
                    if b == 0 {
                        return None;
                    }
                    a.wrapping_rem(b)
                }
            })
        }
    }
}

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut out = String::new();
    for line in input.lines() {
        let b = line.as_bytes();
        let mut toks = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if c == b' ' || c == b'\t' || c == b'\r' {
                i += 1;
            } else if c.is_ascii_digit() {
                let s = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                toks.push(Tok { text: line[s..i].to_string(), col: s + 1 });
            } else {
                let ch = line[i..].chars().next().unwrap();
                toks.push(Tok { text: ch.to_string(), col: i + 1 });
                i += ch.len_utf8();
            }
        }
        let mut p = P { t: toks, i: 0 };
        let res = p.expr().and_then(|e| {
            if p.i < p.t.len() { Err(p.err()) } else { Ok(e) }
        });
        match res {
            Ok(e) => match eval(&e) {
                Some(v) => out.push_str(&format!("{} = {}\n", show(&e), v)),
                None => out.push_str(&format!("{} = error: division by zero\n", show(&e))),
            },
            Err(m) => {
                out.push_str(&m);
                out.push('\n');
            }
        }
    }
    print!("{}", out);
}
