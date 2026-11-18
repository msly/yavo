use crate::ast::*;
use crate::diag::{Diags, Span};
use crate::lexer::{self, describe, StrPiece, Tok, Token};

pub const KEYWORDS: &[&str] = &[
    "fn", "pub", "let", "var", "if", "else", "while", "for", "in", "loop", "break", "continue", "return",
    "true", "false", "use", "struct", "enum", "match", "interface", "impl", "const", "type", "none", "err",
    "try", "catch", "inout", "sink", "of", "with", "scope", "unsafe", "self", "Self", "defer", "spawn",
];

type PResult<T> = Result<T, ()>;

enum Item {
    Fn(FnDecl),
    Struct(StructDecl),
}

pub fn parse(toks: Vec<Token>, src: &str, diags: &mut Diags) -> Program {
    let mut p = Parser { toks, pos: 0, src, diags };
    p.program()
}

struct Parser<'a, 'd> {
    toks: Vec<Token>,
    pos: usize,
    src: &'a str,
    diags: &'d mut Diags,
}

impl Parser<'_, '_> {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn peek_at(&self, n: usize) -> &Tok {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)].tok
    }

    fn span(&self) -> Span {
        self.toks[self.pos].span
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if !matches!(t.tok, Tok::Eof) {
            self.pos += 1;
        }
        t
    }

    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Tok::Ident(s) if s == kw)
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == tok {
            self.bump();
            true
        } else {
            false
        }
    }

    fn err_here(&mut self, msg: impl Into<String>) {
        let sp = self.span();
        self.diags.error("E0020", sp, msg);
    }

    fn unsupported(&mut self, span: Span, msg: impl Into<String>) -> PResult<()> {
        self.diags.error("U0001", span, msg);
        Err(())
    }

    fn expect(&mut self, tok: Tok, what: &str) -> PResult<Span> {
        if *self.peek() == tok {
            Ok(self.bump().span)
        } else {
            let found = describe(self.peek());
            self.err_here(format!("expected {what}, found {found}"));
            Err(())
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
    }

    fn ident(&mut self, what: &str) -> PResult<(String, Span)> {
        match self.peek().clone() {
            Tok::Ident(s) if !KEYWORDS.contains(&s.as_str()) => {
                let sp = self.bump().span;
                Ok((s, sp))
            }
            Tok::Ident(s) => {
                self.err_here(format!("`{s}` is a reserved word and cannot be used as {what}"));
                Err(())
            }
            other => {
                self.err_here(format!("expected {what}, found {}", describe(&other)));
                Err(())
            }
        }
    }

    fn program(&mut self) -> Program {
        let mut fns = Vec::new();
        let mut structs = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), Tok::Eof) {
                break;
            }
            match self.item() {
                Ok(Some(Item::Fn(f))) => fns.push(f),
                Ok(Some(Item::Struct(s))) => structs.push(s),
                Ok(None) => {}
                Err(()) => self.sync_item(),
            }
        }
        Program { fns, structs }
    }

    fn sync_item(&mut self) {
        let mut depth = 0i32;
        loop {
            match self.peek() {
                Tok::Eof => return,
                Tok::LBrace => depth += 1,
                Tok::RBrace => depth = (depth - 1).max(0),
                Tok::Newline if depth == 0 => {
                    self.bump();
                    self.skip_newlines();
                    if self.is_kw("fn") || self.is_kw("pub") || self.is_kw("struct") {
                        return;
                    }
                    continue;
                }
                _ => {}
            }
            self.bump();
        }
    }

    fn item(&mut self) -> PResult<Option<Item>> {
        let start = self.span();
        if self.is_kw("use") {
            self.diags.error("U0001", start, "imports are not supported in P0; there is no standard library yet");
            while !matches!(self.peek(), Tok::Newline | Tok::Eof) {
                self.bump();
            }
            return Ok(None);
        }
        // P0 compiles a single file, so `pub` is accepted and has no effect yet.
        if self.is_kw("pub") {
            self.bump();
        }
        if self.is_kw("fn") {
            return self.function().map(|f| Some(Item::Fn(f)));
        }
        if self.is_kw("struct") {
            return self.struct_decl().map(|s| Some(Item::Struct(s)));
        }
        for kw in ["enum", "interface", "impl", "const", "type"] {
            if self.is_kw(kw) {
                self.unsupported(start, format!("`{kw}` declarations are not supported in P0"))?;
            }
        }
        let found = describe(self.peek());
        self.err_here(format!("expected `fn`, found {found}"));
        Err(())
    }

    /// `struct Name { field: Type ... }`, one field per line (commas are also accepted).
    fn struct_decl(&mut self) -> PResult<StructDecl> {
        self.bump();
        let (name, name_span) = self.ident("a struct name")?;
        if matches!(self.peek(), Tok::LBracket) {
            let sp = self.span();
            self.unsupported(sp, "generic structs are not supported in P0")?;
        }
        if matches!(self.peek(), Tok::Colon) {
            let sp = self.span();
            self.unsupported(sp, "implementing interfaces (`struct X: Named`) is not supported in P0")?;
        }
        self.expect(Tok::LBrace, "`{` to start the struct body")?;
        let mut fields = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Newline | Tok::Comma) {
                self.bump();
            }
            if self.eat(&Tok::RBrace) {
                break;
            }
            if self.is_kw("fn") || self.is_kw("pub") {
                let sp = self.span();
                self.unsupported(sp, "methods are not supported in P0")?;
            }
            let (fname, fspan) = self.ident("a field name")?;
            self.expect(Tok::Colon, "`:` followed by the field type")?;
            let ty = self.ty()?;
            if matches!(self.peek(), Tok::Assign) {
                let sp = self.span();
                self.unsupported(sp, "field default values are not supported in P0")?;
            }
            fields.push(FieldDecl { name: fname, span: fspan, ty });
            if !matches!(self.peek(), Tok::Newline | Tok::Comma | Tok::RBrace) {
                let found = describe(self.peek());
                self.err_here(format!("expected a newline or `,` after the field, found {found}"));
                return Err(());
            }
        }
        Ok(StructDecl { name, name_span, fields })
    }

    fn function(&mut self) -> PResult<FnDecl> {
        self.bump();
        let (name, name_span) = self.ident("a function name")?;
        if matches!(self.peek(), Tok::LBracket) {
            let sp = self.span();
            self.unsupported(sp, "generic functions are not supported in P0")?;
        }
        self.expect(Tok::LParen, "`(`")?;
        let mut params = Vec::new();
        while !matches!(self.peek(), Tok::RParen) {
            for m in ["inout", "sink"] {
                if self.is_kw(m) {
                    let sp = self.span();
                    self.unsupported(sp, format!("`{m}` parameters are not supported in P0"))?;
                }
            }
            if self.is_kw("self") {
                let sp = self.span();
                self.unsupported(sp, "methods are not supported in P0")?;
            }
            let (pname, pspan) = self.ident("a parameter name")?;
            self.expect(Tok::Colon, "`:` followed by the parameter type")?;
            let sink = self.is_kw("sink");
            let inout = self.is_kw("inout");
            if sink || inout {
                self.bump();
            }
            let ty = self.ty()?;
            params.push(Param { name: pname, span: pspan, sink, inout, ty });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RParen, "`,` or `)`")?;
        let mut ret_of = None;
        let ret = if self.eat(&Tok::Arrow) {
            let t = self.ty()?;
            if self.is_kw("of") {
                self.bump();
                let paren = self.eat(&Tok::LParen);
                let mut names = Vec::new();
                loop {
                    match self.peek().clone() {
                        // `static` and `self` are written like names here.
                        Tok::Ident(s) => {
                            let sp = self.bump().span;
                            names.push((s, sp));
                        }
                        _ => {
                            let found = describe(self.peek());
                            self.err_here(format!("expected a parameter name or `static` after `of`, found {found}"));
                            return Err(());
                        }
                    }
                    if !(paren && self.eat(&Tok::Comma)) {
                        break;
                    }
                }
                if paren {
                    self.expect(Tok::RParen, "`)` to close the `of` list")?;
                }
                ret_of = Some(names);
            }
            Some(t)
        } else {
            None
        };
        if matches!(self.peek(), Tok::Bang) {
            let sp = self.span();
            self.unsupported(sp, "error-returning functions (`T ! E`) are not supported in P0")?;
        }
        let body = self.block()?;
        Ok(FnDecl { name, name_span, params, ret, ret_of, body })
    }

    fn ty(&mut self) -> PResult<TypeExpr> {
        // `x: inout T`, `x: sink T` and `&T` are valid v0.5 syntax that P0 does not implement yet:
        // report them as unsupported (U0001), not as syntax errors.
        for m in ["inout", "sink"] {
            if self.is_kw(m) {
                let sp = self.span();
                self.unsupported(sp, format!("`{m}` parameters are not supported in P0"))?;
            }
        }
        if matches!(self.peek(), Tok::Amp) {
            let amp = self.bump().span;
            if self.is_kw("inout") {
                self.unsupported(amp, "`&inout T` values are not supported in P0 (use an `inout` parameter)")?;
            }
            let inner = self.ty()?;
            return Ok(TypeExpr { name: "&".into(), span: amp.to(inner.span), args: vec![inner] });
        }
        if matches!(self.peek(), Tok::Question | Tok::LBracket | Tok::LParen) {
            let sp = self.span();
            self.unsupported(sp, "optional, slice and tuple types are not supported in P0")?;
        }
        let (name, span) = self.ident("a type")?;
        let mut args = Vec::new();
        if self.eat(&Tok::LBracket) {
            loop {
                args.push(self.ty()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(Tok::RBracket, "`]` to close the type arguments")?;
        }
        if matches!(self.peek(), Tok::Plus) {
            let sp = self.span();
            self.unsupported(sp, "capability bounds (`T + Send`) are not supported in P0")?;
        }
        Ok(TypeExpr { name, span, args })
    }

    fn block(&mut self) -> PResult<Block> {
        let open = self.expect(Tok::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                Tok::RBrace => break,
                Tok::Eof => {
                    self.diags.error("E0020", open, "this `{` is never closed");
                    return Err(());
                }
                _ => {}
            }
            match self.stmt() {
                Ok(s) => {
                    stmts.push(s);
                    if !matches!(self.peek(), Tok::Newline | Tok::RBrace) {
                        let found = describe(self.peek());
                        self.err_here(format!("expected a newline after the statement, found {found}"));
                        self.sync_stmt();
                    }
                }
                Err(()) => self.sync_stmt(),
            }
        }
        let close = self.bump().span;
        Ok(Block { stmts, close })
    }

    fn sync_stmt(&mut self) {
        let mut depth = 0i32;
        loop {
            match self.peek() {
                Tok::Eof => return,
                Tok::Newline if depth == 0 => {
                    self.bump();
                    return;
                }
                Tok::RBrace if depth == 0 => return,
                Tok::LBrace => depth += 1,
                Tok::RBrace => depth -= 1,
                _ => {}
            }
            self.bump();
        }
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        let start = self.span();
        if self.is_kw("let") || self.is_kw("var") {
            let mutable = self.is_kw("var");
            self.bump();
            if matches!(self.peek(), Tok::LParen) {
                let sp = self.span();
                self.unsupported(sp, "tuple destructuring is not supported in P0")?;
            }
            let (name, name_span) = self.ident("a variable name")?;
            let ty = if self.eat(&Tok::Colon) { Some(self.ty()?) } else { None };
            self.expect(Tok::Assign, "`=` followed by an initial value")?;
            let init = self.expr()?;
            let span = start.to(init.span);
            return Ok(Stmt { span, kind: StmtKind::Let { mutable, name, name_span, ty, init } });
        }
        if self.is_kw("if") {
            return self.if_stmt();
        }
        if self.is_kw("while") {
            self.bump();
            if self.is_kw("let") {
                let sp = self.span();
                self.unsupported(sp, "`while let` is not supported in P0")?;
            }
            let cond = self.expr()?;
            let body = self.block()?;
            return Ok(Stmt { span: start, kind: StmtKind::While { cond, body } });
        }
        if self.is_kw("loop") {
            self.bump();
            let body = self.block()?;
            return Ok(Stmt { span: start, kind: StmtKind::Loop { body } });
        }
        if self.is_kw("for") {
            self.bump();
            let (var, var_span) = self.ident("a loop variable")?;
            if matches!(self.peek(), Tok::Comma) {
                let sp = self.span();
                self.unsupported(sp, "multiple loop variables (`for k, v in m`) are not supported in P0")?;
            }
            if !self.is_kw("in") {
                let found = describe(self.peek());
                self.err_here(format!("expected `in`, found {found}"));
                return Err(());
            }
            self.bump();
            let range_start = self.expr()?;
            if !self.eat(&Tok::DotDot) {
                let body = self.block()?;
                return Ok(Stmt { span: start, kind: StmtKind::ForIn { var, var_span, iter: range_start, body } });
            }
            let range_end = self.expr()?;
            let body = self.block()?;
            return Ok(Stmt {
                span: start,
                kind: StmtKind::For { var, var_span, start: range_start, end: range_end, body },
            });
        }
        if self.is_kw("return") {
            self.bump();
            if matches!(self.peek(), Tok::Newline | Tok::RBrace) {
                return Ok(Stmt { span: start, kind: StmtKind::Return(None) });
            }
            let e = self.expr()?;
            return Ok(Stmt { span: start.to(e.span), kind: StmtKind::Return(Some(e)) });
        }
        if self.is_kw("break") {
            self.bump();
            return Ok(Stmt { span: start, kind: StmtKind::Break });
        }
        if self.is_kw("continue") {
            self.bump();
            return Ok(Stmt { span: start, kind: StmtKind::Continue });
        }
        for kw in ["match", "scope", "defer", "unsafe", "with"] {
            if self.is_kw(kw) {
                self.unsupported(start, format!("`{kw}` is not supported in P0"))?;
            }
        }
        if matches!(self.peek(), Tok::LBrace) {
            let b = self.block()?;
            return Ok(Stmt { span: start, kind: StmtKind::Block(b) });
        }
        let e = self.expr()?;
        let op = match self.peek() {
            Tok::Assign => Some(None),
            Tok::PlusEq => Some(Some(BinOp::Add)),
            Tok::MinusEq => Some(Some(BinOp::Sub)),
            Tok::StarEq => Some(Some(BinOp::Mul)),
            Tok::SlashEq => Some(Some(BinOp::Div)),
            Tok::PercentEq => Some(Some(BinOp::Rem)),
            _ => None,
        };
        if let Some(op) = op {
            let op_span = self.bump().span;
            let value = self.expr()?;
            let span = start.to(value.span);
            return Ok(Stmt { span, kind: StmtKind::Assign { target: e, op, op_span, value } });
        }
        Ok(Stmt { span: e.span, kind: StmtKind::Expr(e) })
    }

    fn if_stmt(&mut self) -> PResult<Stmt> {
        let start = self.bump().span;
        if self.is_kw("let") {
            let sp = self.span();
            self.unsupported(sp, "`if let` is not supported in P0")?;
        }
        let cond = self.expr()?;
        let then = self.block()?;
        if matches!(self.peek(), Tok::Newline) && matches!(self.peek_at(1), Tok::Ident(s) if s == "else") {
            self.bump();
            self.err_here("`else` must be on the same line as the closing `}` of the `if` block");
        }
        let els = if self.is_kw("else") {
            self.bump();
            if self.is_kw("if") {
                Some(Else::If(Box::new(self.if_stmt()?)))
            } else {
                Some(Else::Block(self.block()?))
            }
        } else {
            None
        };
        Ok(Stmt { span: start, kind: StmtKind::If { cond, then, els } })
    }

    fn expr(&mut self) -> PResult<Expr> {
        let e = self.binary(1)?;
        if self.is_kw("catch") {
            let sp = self.span();
            self.unsupported(sp, "`catch` is not supported in P0")?;
        }
        Ok(e)
    }

    /// True when the `(` at the current position closes into `) =>`, i.e. a closure `(a, b) => expr`.
    fn at_closure(&self) -> bool {
        let mut depth = 0usize;
        let mut n = 0usize;
        loop {
            match self.peek_at(n) {
                Tok::LParen => depth += 1,
                Tok::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(self.peek_at(n + 1), Tok::FatArrow);
                    }
                }
                Tok::Eof | Tok::Newline | Tok::LBrace | Tok::RBrace => return false,
                _ => {}
            }
            n += 1;
        }
    }

    /// Precedence, lowest first: `||`, `&&`, comparisons (non-associative), `|`, `^`,
    /// `&`, shifts, additive (incl. wrapping), multiplicative (incl. wrapping).
    fn binop_at(&self, level: u8) -> Option<BinOp> {
        let (l, op) = match self.peek() {
            Tok::OrOr => (1, BinOp::Or),
            Tok::AndAnd => (2, BinOp::And),
            Tok::EqEq => (3, BinOp::Eq),
            Tok::NotEq => (3, BinOp::Ne),
            Tok::Lt => (3, BinOp::Lt),
            Tok::Le => (3, BinOp::Le),
            Tok::Gt => (3, BinOp::Gt),
            Tok::Ge => (3, BinOp::Ge),
            Tok::Pipe => (4, BinOp::BitOr),
            Tok::Caret => (5, BinOp::BitXor),
            Tok::Amp => (6, BinOp::BitAnd),
            Tok::Shl => (7, BinOp::Shl),
            Tok::Shr => (7, BinOp::Shr),
            Tok::Plus => (8, BinOp::Add),
            Tok::Minus => (8, BinOp::Sub),
            Tok::PlusWrap => (8, BinOp::AddWrap),
            Tok::MinusWrap => (8, BinOp::SubWrap),
            Tok::Star => (9, BinOp::Mul),
            Tok::Slash => (9, BinOp::Div),
            Tok::Percent => (9, BinOp::Rem),
            Tok::StarWrap => (9, BinOp::MulWrap),
            _ => return None,
        };
        (l == level).then_some(op)
    }

    fn binary(&mut self, level: u8) -> PResult<Expr> {
        if level > 9 {
            return self.unary();
        }
        let mut lhs = self.binary(level + 1)?;
        while let Some(op) = self.binop_at(level) {
            let op_span = self.bump().span;
            let rhs = self.binary(level + 1)?;
            let span = lhs.span.to(rhs.span);
            lhs = Expr { span, kind: ExprKind::Binary { op, op_span, lhs: Box::new(lhs), rhs: Box::new(rhs) } };
            if level == 3 && self.binop_at(3).is_some() {
                self.err_here("comparison operators cannot be chained; combine comparisons with `&&`");
                return Err(());
            }
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Expr> {
        let start = self.span();
        let op = match self.peek() {
            Tok::Minus => UnOp::Neg,
            Tok::Bang => UnOp::Not,
            Tok::Amp => {
                self.bump();
                let inner = self.unary()?;
                return Ok(Expr { span: start.to(inner.span), kind: ExprKind::Borrow(Box::new(inner)) });
            }
            Tok::Star => {
                self.unsupported(start, "dereference (`*p`) is not supported in P0")?;
                unreachable!()
            }
            _ => return self.postfix(),
        };
        self.bump();
        let e = self.unary()?;
        Ok(Expr { span: start.to(e.span), kind: ExprKind::Unary { op, op_span: start, expr: Box::new(e) } })
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        loop {
            match self.peek() {
                Tok::LParen => {
                    self.bump();
                    let mut args = Vec::new();
                    while !matches!(self.peek(), Tok::RParen) {
                        args.push(self.expr()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    let close = self.expect(Tok::RParen, "`,` or `)`")?;
                    e = Expr { span: e.span.to(close), kind: ExprKind::Call { callee: Box::new(e), args } };
                }
                Tok::Dot => {
                    self.bump();
                    let (name, name_span) = match self.peek().clone() {
                        Tok::Ident(s) => {
                            let sp = self.bump().span;
                            (s, sp)
                        }
                        Tok::Int(_) => {
                            let sp = self.span();
                            self.unsupported(sp, "tuple fields (`t.0`) are not supported in P0")?;
                            unreachable!()
                        }
                        other => {
                            self.err_here(format!("expected a field or method name, found {}", describe(&other)));
                            return Err(());
                        }
                    };
                    e = Expr { span: e.span.to(name_span), kind: ExprKind::Field { base: Box::new(e), name } };
                }
                Tok::LBracket => {
                    let open = self.bump().span;
                    if matches!(self.peek(), Tok::DotDot) {
                        self.unsupported(open, "slices (`x[a..b]`) are not supported in P0")?;
                    }
                    let index = self.expr()?;
                    if matches!(self.peek(), Tok::DotDot) {
                        self.unsupported(open, "slices (`x[a..b]`) are not supported in P0")?;
                    }
                    if matches!(self.peek(), Tok::Comma) {
                        self.unsupported(open, "type arguments in expressions (`Name[A, B]`) are not supported in P0")?;
                    }
                    let close = self.expect(Tok::RBracket, "`]`")?;
                    e = Expr { span: e.span.to(close), kind: ExprKind::Index { base: Box::new(e), index: Box::new(index) } };
                }
                // `Wrapper[str]{...}`: a generic struct literal.
                Tok::LBrace
                    if matches!(&e.kind, ExprKind::Index { base, .. }
                        if matches!(&base.kind, ExprKind::Ident(n) if n.starts_with(|c: char| c.is_ascii_uppercase())))
                        && self.span().start == e.span.end =>
                {
                    self.unsupported(e.span, "generic struct literals are not supported in P0")?;
                }
                Tok::Question => {
                    let sp = self.span();
                    self.unsupported(sp, "optional chaining (`?`) is not supported in P0")?;
                }
                // `Name{field: value}`: a struct literal. Written without a space, which keeps it
                // apart from `if cond {` blocks.
                Tok::LBrace
                    if matches!(&e.kind, ExprKind::Ident(n) if n.starts_with(|c: char| c.is_ascii_uppercase()))
                        && self.span().start == e.span.end =>
                {
                    let ExprKind::Ident(name) = &e.kind else { unreachable!() };
                    let (name, name_span) = (name.clone(), e.span);
                    self.bump();
                    let mut fields = Vec::new();
                    loop {
                        while matches!(self.peek(), Tok::Newline | Tok::Comma) {
                            self.bump();
                        }
                        if matches!(self.peek(), Tok::RBrace) {
                            break;
                        }
                        let (fname, fspan) = self.ident("a field name")?;
                        self.expect(Tok::Colon, "`:` followed by the field value")?;
                        let value = self.expr()?;
                        fields.push(FieldInit { name: fname, span: fspan, value });
                        if !matches!(self.peek(), Tok::Newline | Tok::Comma | Tok::RBrace) {
                            let found = describe(self.peek());
                            self.err_here(format!("expected `,` or `}}` after the field value, found {found}"));
                            return Err(());
                        }
                    }
                    let close = self.bump().span;
                    e = Expr { span: name_span.to(close), kind: ExprKind::StructLit { name, name_span, fields } };
                }
                _ => return Ok(e),
            }
        }
    }

    fn primary(&mut self) -> PResult<Expr> {
        let span = self.span();
        match self.peek().clone() {
            Tok::Int(v) => {
                self.bump();
                Ok(Expr { span, kind: ExprKind::Int(v) })
            }
            Tok::Str(pieces) => {
                self.bump();
                self.string(pieces, span)
            }
            Tok::Ident(s) if s == "true" || s == "false" => {
                self.bump();
                Ok(Expr { span, kind: ExprKind::Bool(s == "true") })
            }
            Tok::Ident(s) if KEYWORDS.contains(&s.as_str()) => {
                if matches!(s.as_str(), "none" | "err" | "try" | "match" | "self") {
                    self.unsupported(span, format!("`{s}` is not supported in P0"))?;
                }
                self.err_here(format!("expected an expression, found `{s}`"));
                Err(())
            }
            Tok::Ident(s) => {
                self.bump();
                Ok(Expr { span, kind: ExprKind::Ident(s) })
            }
            Tok::LParen => {
                if self.at_closure() {
                    self.unsupported(span, "closures (`(a, b) => expr`) are not supported in P0")?;
                }
                self.bump();
                let e = self.expr()?;
                if matches!(self.peek(), Tok::Comma) {
                    self.unsupported(span, "tuple literals are not supported in P0")?;
                }
                let close = self.expect(Tok::RParen, "`)`")?;
                Ok(Expr { span: span.to(close), kind: ExprKind::Paren(Box::new(e)) })
            }
            Tok::LBracket => {
                self.bump();
                if matches!(self.peek(), Tok::Colon) {
                    self.unsupported(span, "map literals (`[:]`) are not supported in P0")?;
                }
                let mut items = Vec::new();
                loop {
                    while matches!(self.peek(), Tok::Newline | Tok::Comma) {
                        self.bump();
                    }
                    if matches!(self.peek(), Tok::RBracket) {
                        break;
                    }
                    items.push(self.expr()?);
                    if !matches!(self.peek(), Tok::Newline | Tok::Comma | Tok::RBracket) {
                        let found = describe(self.peek());
                        self.err_here(format!("expected `,` or `]` in the vector literal, found {found}"));
                        return Err(());
                    }
                }
                let close = self.bump().span;
                Ok(Expr { span: span.to(close), kind: ExprKind::VecLit(items) })
            }
            Tok::Dot => {
                self.unsupported(span, "enum case shorthand (`.Case`) is not supported in P0")?;
                unreachable!()
            }
            other => {
                self.err_here(format!("expected an expression, found {}", describe(&other)));
                Err(())
            }
        }
    }

    fn string(&mut self, pieces: Vec<StrPiece>, span: Span) -> PResult<Expr> {
        let mut segs = Vec::new();
        for piece in pieces {
            match piece {
                StrPiece::Text(t) => segs.push(StrSeg::Text(t)),
                StrPiece::Interp { start, end, line, col, spec } => {
                    let ispan = Span { start, end, line, col };
                    let toks = lexer::lex_range(self.src, start, end, line, col, false, self.diags);
                    if toks.len() == 1 {
                        self.diags.error("E0020", ispan, "empty `${}` in string literal");
                        continue;
                    }
                    let mut sub = Parser { toks, pos: 0, src: self.src, diags: &mut *self.diags };
                    let Ok(expr) = sub.expr() else { continue };
                    if !matches!(sub.peek(), Tok::Eof) {
                        sub.err_here("only a single expression is allowed inside `${...}`");
                        continue;
                    }
                    segs.push(StrSeg::Expr { expr, spec });
                }
            }
        }
        Ok(Expr { span, kind: ExprKind::Str(segs) })
    }
}
