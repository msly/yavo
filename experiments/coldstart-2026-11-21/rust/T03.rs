use std::io::{self, Read};

enum E {
    Num(String, i64),
    Neg(Box<E>),
    Bin(Box<E>, char, Box<E>),
}

struct P {
    toks: Vec<(usize, String)>,
    pos: usize,
}

type PR = Result<E, String>;

impl P {
    fn err(&self) -> String {
        match self.toks.get(self.pos) {
            None => "error: unexpected end".to_string(),
            Some((c, t)) => format!("error at col {}: unexpected '{}'", c, t),
        }
    }
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.pos).map(|t| t.1.as_str())
    }
    fn expr(&mut self) -> PR {
        let mut l = self.term()?;
        while let Some(t) = self.peek() {
            if t == "+" || t == "-" {
                let op = t.chars().next().unwrap();
                self.pos += 1;
                let r = self.term()?;
                l = E::Bin(Box::new(l), op, Box::new(r));
            } else {
                break;
            }
        }
        Ok(l)
    }
    fn term(&mut self) -> PR {
        let mut l = self.unary()?;
        while let Some(t) = self.peek() {
            if t == "*" || t == "/" || t == "%" {
                let op = t.chars().next().unwrap();
                self.pos += 1;
                let r = self.unary()?;
                l = E::Bin(Box::new(l), op, Box::new(r));
            } else {
                break;
            }
        }
        Ok(l)
    }
    fn unary(&mut self) -> PR {
        if self.peek() == Some("-") {
            self.pos += 1;
            let e = self.unary()?;
            return Ok(E::Neg(Box::new(e)));
        }
        self.primary()
    }
    fn primary(&mut self) -> PR {
        match self.peek() {
            None => Err(self.err()),
            Some(t) if t.as_bytes()[0].is_ascii_digit() => {
                let s = t.to_string();
                self.pos += 1;
                let v = s.parse::<i64>().unwrap_or(0);
                Ok(E::Num(s, v))
            }
            Some("(") => {
                self.pos += 1;
                let e = self.expr()?;
                if self.peek() == Some(")") {
                    self.pos += 1;
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

fn eval(e: &E) -> Result<i64, ()> {
    match e {
        E::Num(_, v) => Ok(*v),
        E::Neg(x) => Ok(eval(x)?.wrapping_neg()),
        E::Bin(l, o, r) => {
            let a = eval(l)?;
            let b = eval(r)?;
            Ok(match o {
                '+' => a.wrapping_add(b),
                '-' => a.wrapping_sub(b),
                '*' => a.wrapping_mul(b),
                '/' => {
                    if b == 0 {
                        return Err(());
                    }
                    a.wrapping_div(b)
                }
                _ => {
                    if b == 0 {
                        return Err(());
                    }
                    a.wrapping_rem(b)
                }
            })
        }
    }
}

fn main() {
    let mut inp = String::new();
    io::stdin().read_to_string(&mut inp).unwrap();
    let mut out = String::new();
    for line in inp.lines() {
        let b: Vec<char> = line.chars().collect();
        let mut toks = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if c == ' ' || c == '\t' {
                i += 1;
            } else if c.is_ascii_digit() {
                let s = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                toks.push((s + 1, b[s..i].iter().collect::<String>()));
            } else {
                toks.push((i + 1, c.to_string()));
                i += 1;
            }
        }
        let mut p = P { toks, pos: 0 };
        let r = p.expr().and_then(|e| if p.pos < p.toks.len() { Err(p.err()) } else { Ok(e) });
        match r {
            Err(m) => out.push_str(&format!("{}\n", m)),
            Ok(e) => match eval(&e) {
                Ok(v) => out.push_str(&format!("{} = {}\n", show(&e), v)),
                Err(()) => out.push_str(&format!("{} = error: division by zero\n", show(&e))),
            },
        }
    }
    print!("{}", out);
}
