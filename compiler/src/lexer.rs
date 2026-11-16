use crate::diag::{Diags, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    Int(u128),
    Str(Vec<StrPiece>),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusWrap,
    MinusWrap,
    StarWrap,
    Shl,
    Shr,
    Amp,
    Pipe,
    Caret,
    Bang,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Assign,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    DotDot,
    Arrow,
    FatArrow,
    Question,
    Newline,
    Eof,
}

/// A string literal is split into text and `${...}` pieces; the parser lexes each
/// interpolation range again so its tokens keep their real source positions.
#[derive(Clone, Debug, PartialEq)]
pub enum StrPiece {
    Text(String),
    Interp { start: usize, end: usize, line: u32, col: u32, spec: Option<String> },
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

pub fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Ident(s) => format!("`{s}`"),
        Tok::Int(_) => "an integer literal".into(),
        Tok::Str(_) => "a string literal".into(),
        Tok::Newline => "a newline".into(),
        Tok::Eof => "the end of the file".into(),
        other => format!("`{}`", punct_text(other)),
    }
}

fn punct_text(tok: &Tok) -> &'static str {
    match tok {
        Tok::Plus => "+",
        Tok::Minus => "-",
        Tok::Star => "*",
        Tok::Slash => "/",
        Tok::Percent => "%",
        Tok::PlusWrap => "+%",
        Tok::MinusWrap => "-%",
        Tok::StarWrap => "*%",
        Tok::Shl => "<<",
        Tok::Shr => ">>",
        Tok::Amp => "&",
        Tok::Pipe => "|",
        Tok::Caret => "^",
        Tok::Bang => "!",
        Tok::EqEq => "==",
        Tok::NotEq => "!=",
        Tok::Lt => "<",
        Tok::Le => "<=",
        Tok::Gt => ">",
        Tok::Ge => ">=",
        Tok::AndAnd => "&&",
        Tok::OrOr => "||",
        Tok::Assign => "=",
        Tok::PlusEq => "+=",
        Tok::MinusEq => "-=",
        Tok::StarEq => "*=",
        Tok::SlashEq => "/=",
        Tok::PercentEq => "%=",
        Tok::LParen => "(",
        Tok::RParen => ")",
        Tok::LBrace => "{",
        Tok::RBrace => "}",
        Tok::LBracket => "[",
        Tok::RBracket => "]",
        Tok::Comma => ",",
        Tok::Colon => ":",
        Tok::Dot => ".",
        Tok::DotDot => "..",
        Tok::Arrow => "->",
        Tok::FatArrow => "=>",
        Tok::Question => "?",
        _ => "?",
    }
}

pub fn lex_file(src: &str, diags: &mut Diags) -> Vec<Token> {
    lex_range(src, 0, src.len(), 1, 1, true, diags)
}

pub fn lex_range(
    src: &str,
    start: usize,
    end: usize,
    line: u32,
    col: u32,
    newlines: bool,
    diags: &mut Diags,
) -> Vec<Token> {
    let mut lx = Lexer {
        src,
        pos: start,
        end,
        line,
        col,
        newlines,
        depth: 0,
        last_ends_stmt: false,
        toks: Vec::new(),
        diags,
    };
    lx.run();
    lx.toks
}

struct Lexer<'a, 'd> {
    src: &'a str,
    pos: usize,
    end: usize,
    line: u32,
    col: u32,
    newlines: bool,
    depth: u32,
    last_ends_stmt: bool,
    toks: Vec<Token>,
    diags: &'d mut Diags,
}

impl Lexer<'_, '_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..self.end].chars().next()
    }

    fn peek2(&self) -> Option<char> {
        let mut it = self.src[self.pos..self.end].chars();
        it.next();
        it.next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn span_from(&self, start: usize, line: u32, col: u32) -> Span {
        Span { start, end: self.pos, line, col }
    }

    fn push(&mut self, tok: Tok, span: Span) {
        // Go-style statement termination: a newline ends a statement only after a
        // token that can end one, and never inside parentheses or brackets.
        self.last_ends_stmt = matches!(
            tok,
            Tok::Ident(_) | Tok::Int(_) | Tok::Str(_) | Tok::RParen | Tok::RBracket | Tok::RBrace
        );
        match tok {
            Tok::LParen | Tok::LBracket => self.depth += 1,
            Tok::RParen | Tok::RBracket => self.depth = self.depth.saturating_sub(1),
            _ => {}
        }
        self.toks.push(Token { tok, span });
    }

    fn run(&mut self) {
        loop {
            while matches!(self.peek(), Some(' ' | '\t' | '\r')) {
                self.bump();
            }
            let (start, line, col) = (self.pos, self.line, self.col);
            let Some(c) = self.peek() else {
                let span = Span { start, end: start, line, col };
                if self.newlines && self.last_ends_stmt {
                    self.toks.push(Token { tok: Tok::Newline, span });
                }
                self.toks.push(Token { tok: Tok::Eof, span });
                return;
            };
            match c {
                '\n' => {
                    self.bump();
                    if self.newlines && self.depth == 0 && self.last_ends_stmt {
                        self.last_ends_stmt = false;
                        let span = Span { start, end: start + 1, line, col };
                        self.toks.push(Token { tok: Tok::Newline, span });
                    }
                }
                '/' if self.peek2() == Some('/') => {
                    while !matches!(self.peek(), None | Some('\n')) {
                        self.bump();
                    }
                }
                '/' if self.peek2() == Some('*') => {
                    self.bump();
                    self.bump();
                    loop {
                        match self.peek() {
                            None => break,
                            Some('*') if self.peek2() == Some('/') => {
                                self.bump();
                                self.bump();
                                break;
                            }
                            _ => {
                                self.bump();
                            }
                        }
                    }
                    let span = self.span_from(start, line, col);
                    self.diags.error("E0020", span, "block comments are not part of Yavo; use `//` on each line");
                }
                c if c.is_ascii_alphabetic() || c == '_' => {
                    while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == '_') {
                        self.bump();
                    }
                    let text = self.src[start..self.pos].to_string();
                    let span = self.span_from(start, line, col);
                    self.push(Tok::Ident(text), span);
                }
                c if c.is_ascii_digit() => self.number(start, line, col),
                '"' => self.string(start, line, col),
                _ => self.punct(c, start, line, col),
            }
        }
    }

    fn number(&mut self, start: usize, line: u32, col: u32) {
        let mut radix = 10u32;
        if self.peek() == Some('0') {
            match self.peek2() {
                Some('x') => radix = 16,
                Some('b') => radix = 2,
                Some('o') => radix = 8,
                _ => {}
            }
            if radix != 10 {
                self.bump();
                self.bump();
            }
        }
        let mut value: Option<u128> = Some(0);
        let mut digits = 0;
        let mut bad = false;
        while let Some(c) = self.peek() {
            if c == '_' {
                self.bump();
                continue;
            }
            if !c.is_ascii_alphanumeric() {
                break;
            }
            self.bump();
            match c.to_digit(radix) {
                Some(d) => {
                    digits += 1;
                    value = value
                        .and_then(|v| v.checked_mul(radix as u128))
                        .and_then(|v| v.checked_add(d as u128));
                }
                None => bad = true,
            }
        }
        if radix == 10 && self.peek() == Some('.') && matches!(self.peek2(), Some(c) if c.is_ascii_digit()) {
            self.bump();
            while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == '_') {
                self.bump();
            }
            let span = self.span_from(start, line, col);
            self.diags.error("U0001", span, "floating-point numbers are not supported in P0 yet");
            self.push(Tok::Int(0), span);
            return;
        }
        let span = self.span_from(start, line, col);
        let text = &self.src[start..self.pos];
        if bad || digits == 0 {
            let msg = format!("invalid integer literal `{text}`");
            self.diags.error("E0020", span, msg);
            self.push(Tok::Int(0), span);
            return;
        }
        match value {
            Some(v) => self.push(Tok::Int(v), span),
            None => {
                self.diags.error("E0003", span, "integer literal is too large for any integer type");
                self.push(Tok::Int(0), span);
            }
        }
    }

    fn string(&mut self, start: usize, line: u32, col: u32) {
        self.bump();
        let mut pieces = Vec::new();
        let mut text = String::new();
        loop {
            let (cs, cl, cc) = (self.pos, self.line, self.col);
            match self.peek() {
                None | Some('\n') => {
                    let span = self.span_from(start, line, col);
                    self.diags.error("E0020", span, "unterminated string literal");
                    break;
                }
                Some('"') => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    self.bump();
                    let esc = self.bump();
                    match esc {
                        Some('n') => text.push('\n'),
                        Some('t') => text.push('\t'),
                        Some('r') => text.push('\r'),
                        Some('0') => text.push('\0'),
                        Some('\\') => text.push('\\'),
                        Some('"') => text.push('"'),
                        Some('\'') => text.push('\''),
                        Some('$') => text.push('$'),
                        Some('u') => self.unicode_escape(&mut text, cs, cl, cc),
                        Some(other) => {
                            let span = Span { start: cs, end: self.pos, line: cl, col: cc };
                            self.diags.error("E0020", span, format!("unknown escape sequence `\\{other}`"));
                        }
                        None => {}
                    }
                }
                Some('$') if self.peek2() == Some('{') => {
                    self.bump();
                    self.bump();
                    let (istart, iline, icol) = (self.pos, self.line, self.col);
                    let mut depth = 0u32;
                    let mut colon: Option<usize> = None;
                    let mut closed = false;
                    let mut nested_quote: Option<Span> = None;
                    let mut in_nested_str = false;
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        if c == '"' {
                            nested_quote.get_or_insert(Span { start: self.pos, end: self.pos + 1, line: self.line, col: self.col });
                            in_nested_str = !in_nested_str;
                            self.bump();
                            continue;
                        }
                        if in_nested_str {
                            if c == '\\' {
                                self.bump();
                            }
                            self.bump();
                            continue;
                        }
                        match c {
                            '{' => depth += 1,
                            '}' if depth == 0 => {
                                closed = true;
                                break;
                            }
                            '}' => depth -= 1,
                            ':' if depth == 0 && colon.is_none() => colon = Some(self.pos),
                            _ => {}
                        }
                        self.bump();
                    }
                    if !closed {
                        let span = Span { start: cs, end: self.pos, line: cl, col: cc };
                        self.diags.error("E0020", span, "unterminated `${` in string; close it with `}`");
                        continue;
                    }
                    let iend = self.pos;
                    self.bump();
                    if let Some(span) = nested_quote {
                        self.diags
                            .error("E0020", span, "string literals cannot appear inside `${...}`")
                            .note("bind the string to a variable first, e.g. `let s = \"...\"`, then write `${s}`");
                        continue;
                    }
                    if !text.is_empty() {
                        pieces.push(StrPiece::Text(std::mem::take(&mut text)));
                    }
                    let (expr_end, spec) = match colon {
                        Some(p) => (p, Some(self.src[p + 1..iend].to_string())),
                        None => (iend, None),
                    };
                    pieces.push(StrPiece::Interp { start: istart, end: expr_end, line: iline, col: icol, spec });
                }
                Some(c) => {
                    self.bump();
                    text.push(c);
                }
            }
        }
        if !text.is_empty() || pieces.is_empty() {
            pieces.push(StrPiece::Text(text));
        }
        let span = self.span_from(start, line, col);
        self.push(Tok::Str(pieces), span);
    }

    fn unicode_escape(&mut self, text: &mut String, cs: usize, cl: u32, cc: u32) {
        let mut ok = self.peek() == Some('{');
        let mut value: u32 = 0;
        if ok {
            self.bump();
            let mut digits = 0;
            while let Some(c) = self.peek() {
                if c == '}' {
                    break;
                }
                match c.to_digit(16) {
                    Some(d) if digits < 6 => {
                        value = value * 16 + d;
                        digits += 1;
                        self.bump();
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            ok = ok && digits > 0 && self.peek() == Some('}');
            if ok {
                self.bump();
            }
        }
        match char::from_u32(value).filter(|_| ok) {
            Some(c) => text.push(c),
            None => {
                let span = Span { start: cs, end: self.pos, line: cl, col: cc };
                self.diags.error("E0020", span, "invalid unicode escape; expected `\\u{XXXX}`");
            }
        }
    }

    fn punct(&mut self, c: char, start: usize, line: u32, col: u32) {
        self.bump();
        let next = self.peek();
        let (tok, two) = match (c, next) {
            ('+', Some('%')) => (Tok::PlusWrap, true),
            ('+', Some('=')) => (Tok::PlusEq, true),
            ('+', _) => (Tok::Plus, false),
            ('-', Some('%')) => (Tok::MinusWrap, true),
            ('-', Some('=')) => (Tok::MinusEq, true),
            ('-', Some('>')) => (Tok::Arrow, true),
            ('-', _) => (Tok::Minus, false),
            ('*', Some('%')) => (Tok::StarWrap, true),
            ('*', Some('=')) => (Tok::StarEq, true),
            ('*', _) => (Tok::Star, false),
            ('/', Some('=')) => (Tok::SlashEq, true),
            ('/', _) => (Tok::Slash, false),
            ('%', Some('=')) => (Tok::PercentEq, true),
            ('%', _) => (Tok::Percent, false),
            ('<', Some('<')) => (Tok::Shl, true),
            ('<', Some('=')) => (Tok::Le, true),
            ('<', _) => (Tok::Lt, false),
            ('>', Some('>')) => (Tok::Shr, true),
            ('>', Some('=')) => (Tok::Ge, true),
            ('>', _) => (Tok::Gt, false),
            ('=', Some('=')) => (Tok::EqEq, true),
            ('=', Some('>')) => (Tok::FatArrow, true),
            ('=', _) => (Tok::Assign, false),
            ('!', Some('=')) => (Tok::NotEq, true),
            ('!', _) => (Tok::Bang, false),
            ('&', Some('&')) => (Tok::AndAnd, true),
            ('&', _) => (Tok::Amp, false),
            ('|', Some('|')) => (Tok::OrOr, true),
            ('|', _) => (Tok::Pipe, false),
            ('^', _) => (Tok::Caret, false),
            ('(', _) => (Tok::LParen, false),
            (')', _) => (Tok::RParen, false),
            ('{', _) => (Tok::LBrace, false),
            ('}', _) => (Tok::RBrace, false),
            ('[', _) => (Tok::LBracket, false),
            (']', _) => (Tok::RBracket, false),
            (',', _) => (Tok::Comma, false),
            (':', _) => (Tok::Colon, false),
            ('.', Some('.')) => (Tok::DotDot, true),
            ('.', _) => (Tok::Dot, false),
            ('?', _) => (Tok::Question, false),
            _ => {
                let span = self.span_from(start, line, col);
                self.diags.error("E0020", span, format!("unexpected character `{c}`"));
                return;
            }
        };
        if two {
            self.bump();
        }
        let span = self.span_from(start, line, col);
        self.push(tok, span);
    }
}
