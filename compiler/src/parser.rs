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
    Enum(EnumDecl),
    Interface(InterfaceDecl),
    Use(Vec<Use>),
}

pub fn parse(toks: Vec<Token>, src: &str, diags: &mut Diags) -> Program {
    let mut p = Parser { toks, pos: 0, src, diags, self_ty: None };
    p.program()
}

struct Parser<'a, 'd> {
    toks: Vec<Token>,
    pos: usize,
    src: &'a str,
    diags: &'d mut Diags,
    /// The struct or enum whose body is being parsed: what `Self` and `self` stand for.
    self_ty: Option<String>,
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
        let mut enums = Vec::new();
        let mut interfaces = Vec::new();
        let mut uses = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), Tok::Eof) {
                break;
            }
            match self.item() {
                Ok(Some(Item::Fn(f))) => fns.push(f),
                Ok(Some(Item::Struct(mut s))) => {
                    fns.append(&mut s.methods);
                    structs.push(s)
                }
                Ok(Some(Item::Enum(mut e))) => {
                    fns.append(&mut e.methods);
                    enums.push(e)
                }
                Ok(Some(Item::Interface(i))) => interfaces.push(i),
                Ok(Some(Item::Use(u))) => uses.extend(u),
                Ok(None) => {}
                Err(()) => self.sync_item(),
            }
        }
        Program { files: Vec::new(), fns, structs, enums, interfaces, uses }
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
                    if self.is_kw("fn")
                        || self.is_kw("pub")
                        || self.is_kw("struct")
                        || self.is_kw("enum")
                        || self.is_kw("interface")
                        || self.is_kw("use")
                    {
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
            return self.use_decl().map(|u| Some(Item::Use(u)));
        }
        let public = self.is_kw("pub");
        if public { self.bump(); }
        if self.is_kw("fn") {
            return self.function(None, true).map(|mut f| { f.public = public; Some(Item::Fn(f)) });
        }
        if self.is_kw("struct") {
            return self.struct_decl().map(|mut s| { s.public = public; Some(Item::Struct(s)) });
        }
        if self.is_kw("enum") {
            return self.enum_decl().map(|mut e| { e.public = public; Some(Item::Enum(e)) });
        }
        if self.is_kw("interface") {
            return self.interface_decl().map(|mut i| { i.public = public; Some(Item::Interface(i)) });
        }
        for kw in ["impl", "const", "type"] {
            if self.is_kw(kw) {
                self.unsupported(start, format!("`{kw}` declarations are not supported in P0"))?;
            }
        }
        let found = describe(self.peek());
        self.err_here(format!("expected `fn`, found {found}"));
        Err(())
    }

    /// `: Drop + Named` after a struct or enum name.
    fn impl_list(&mut self) -> PResult<Vec<(String, Span)>> {
        let mut out = Vec::new();
        if self.eat(&Tok::Colon) {
            loop {
                out.push(self.qualified_type_name("an interface name")?);
                if !self.eat(&Tok::Plus) {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// A method inside a struct or enum body (`pub` is accepted and has no effect yet).
    fn method(&mut self, owner: &str) -> PResult<FnDecl> {
        if self.is_kw("pub") {
            self.bump();
        }
        if !self.is_kw("fn") {
            let found = describe(self.peek());
            self.err_here(format!("expected `fn` after `pub`, found {found}"));
            return Err(());
        }
        let saved = self.self_ty.replace(owner.to_string());
        let f = self.function(Some(owner), true);
        self.self_ty = saved;
        f
    }

    /// `interface Name { fn method(self, ...) -> T ... }`: one signature per line.
    fn interface_decl(&mut self) -> PResult<InterfaceDecl> {
        self.bump();
        let (name, name_span) = self.ident("an interface name")?;
        if matches!(self.peek(), Tok::LBracket) {
            let sp = self.span();
            self.unsupported(sp, "generic interfaces are not supported yet")?;
        }
        if matches!(self.peek(), Tok::Colon) {
            let sp = self.span();
            self.unsupported(sp, "interfaces that extend other interfaces are not supported yet")?;
        }
        self.expect(Tok::LBrace, "`{` to start the interface body")?;
        let mut methods = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Newline | Tok::Comma) {
                self.bump();
            }
            if self.eat(&Tok::RBrace) {
                break;
            }
            if self.is_kw("pub") {
                self.bump();
            }
            if !self.is_kw("fn") {
                let found = describe(self.peek());
                self.err_here(format!("expected a method signature (`fn name(self) -> T`) in the interface, found {found}"));
                return Err(());
            }
            let saved = self.self_ty.replace(name.clone());
            let m = self.function(Some(&name), false);
            self.self_ty = saved;
            methods.push(m?);
            if !matches!(self.peek(), Tok::Newline | Tok::RBrace) {
                let found = describe(self.peek());
                self.err_here(format!("an interface method has no body; expected a newline after the signature, found {found}"));
                return Err(());
            }
        }
        Ok(InterfaceDecl { public: false, name, name_span, methods })
    }

    /// `struct Name { field: Type ... fn method(self) { } }`, one member per line (commas are
    /// also accepted between fields).
    fn struct_decl(&mut self) -> PResult<StructDecl> {
        self.bump();
        let (name, name_span) = self.ident("a struct name")?;
        let type_params = self.type_params()?;
        let impls = self.impl_list()?;
        self.expect(Tok::LBrace, "`{` to start the struct body")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Newline | Tok::Comma) {
                self.bump();
            }
            if self.eat(&Tok::RBrace) {
                break;
            }
            if self.is_kw("fn") || self.is_kw("pub") {
                methods.push(self.method(&name)?);
                continue;
            }
            let (fname, fspan) = self.ident("a field name")?;
            self.expect(Tok::Colon, "`:` followed by the field type")?;
            let ty = self.ty()?;
            let default = if self.eat(&Tok::Assign) { Some(self.expr()?) } else { None };
            fields.push(FieldDecl { name: fname, span: fspan, ty, default });
            if !matches!(self.peek(), Tok::Newline | Tok::Comma | Tok::RBrace) {
                let found = describe(self.peek());
                self.err_here(format!("expected a newline or `,` after the field, found {found}"));
                return Err(());
            }
        }
        Ok(StructDecl { public: false, name, name_span, type_params, fields, impls, methods })
    }

    /// `[T, U: Ord + Copy]` after a function or struct name (none when absent).
    fn qualified_type_name(&mut self, what: &str) -> PResult<(String, Span)> {
        let (mut name, mut span) = self.ident(what)?;
        if self.eat(&Tok::Dot) {
            let (member, end) = self.ident("a type name after the module")?;
            name.push('.'); name.push_str(&member); span = span.to(end);
        }
        Ok((name, span))
    }

    fn type_params(&mut self) -> PResult<Vec<TypeParam>> {
        let mut out = Vec::new();
        if !self.eat(&Tok::LBracket) {
            return Ok(out);
        }
        loop {
            let (name, span) = self.ident("a type parameter name")?;
            let mut bounds = Vec::new();
            if self.eat(&Tok::Colon) {
                loop {
                    bounds.push(self.qualified_type_name("a bound such as `Ord` or `Copy`")?);
                    if !self.eat(&Tok::Plus) {
                        break;
                    }
                }
            }
            out.push(TypeParam { name, span, bounds });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RBracket, "`,` or `]` after the type parameters")?;
        Ok(out)
    }

    /// Whether `[` at the current position opens type arguments: the matching `]` is followed
    /// by `(` (a generic call) or, without a space, by `{` (a generic struct literal).
    fn type_args_ahead(&self, allow_static_method: bool) -> bool {
        let mut depth = 0usize;
        let mut i = 0;
        loop {
            match self.peek_at(i) {
                Tok::LBracket => depth += 1,
                Tok::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        return match self.peek_at(i + 1) {
                            Tok::LParen => true,
                            Tok::Dot if allow_static_method => true,
                            Tok::LBrace => self.toks.get(self.pos + i + 1).is_some_and(|t| t.span.start == self.toks[self.pos + i].span.end),
                            _ => false,
                        };
                    }
                }
                Tok::Eof | Tok::Newline | Tok::LBrace | Tok::RBrace | Tok::Assign => return false,
                _ => {}
            }
            i += 1;
        }
    }

    /// `enum Name { Case, Case(field: Type, ...) }`, one case per line (commas are also accepted).
    fn enum_decl(&mut self) -> PResult<EnumDecl> {
        self.bump();
        let (name, name_span) = self.ident("an enum name")?;
        if matches!(self.peek(), Tok::LBracket) {
            let sp = self.span();
            self.unsupported(sp, "generic enums are not supported yet")?;
        }
        let impls = self.impl_list()?;
        self.expect(Tok::LBrace, "`{` to start the enum body")?;
        let mut variants = Vec::new();
        let mut methods = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Newline | Tok::Comma) {
                self.bump();
            }
            if self.eat(&Tok::RBrace) {
                break;
            }
            if self.is_kw("fn") || self.is_kw("pub") {
                methods.push(self.method(&name)?);
                continue;
            }
            let (vname, vspan) = self.ident("a case name")?;
            let mut fields = Vec::new();
            if self.eat(&Tok::LParen) {
                while !matches!(self.peek(), Tok::RParen) {
                    let (fname, fspan) = self.ident("a payload field name")?;
                    self.expect(Tok::Colon, "`:` followed by the field type (case payloads are named, e.g. `Circle(r: f64)`)")?;
                    let ty = self.ty()?;
                    fields.push(FieldDecl { name: fname, span: fspan, ty, default: None });
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::RParen, "`,` or `)`")?;
            }
            variants.push(VariantDecl { name: vname, span: vspan, fields });
            if !matches!(self.peek(), Tok::Newline | Tok::Comma | Tok::RBrace) {
                let found = describe(self.peek());
                self.err_here(format!("expected a newline or `,` after the case, found {found}"));
                return Err(());
            }
        }
        Ok(EnumDecl { public: false, name, name_span, variants, impls, methods })
    }

    /// `use std/m` or `use std/{a, b}`.
    fn use_decl(&mut self) -> PResult<Vec<Use>> {
        self.bump();
        let (first, first_span) = self.ident("a module path")?;
        let mut parts = vec![first];
        let mut last = first_span;
        let mut out = Vec::new();
        while self.eat(&Tok::Slash) {
            if self.eat(&Tok::LBrace) {
                loop {
                    let (module, span) = self.ident("a module name")?;
                    out.push(Use { path: format!("{}/{}", parts.join("/"), module), module, span });
                    if !self.eat(&Tok::Comma) { break; }
                }
                self.expect(Tok::RBrace, "`,` or `}` in the module list")?;
                break;
            }
            let (part, span) = self.ident("a module path component")?;
            parts.push(part);
            last = span;
        }
        if out.is_empty() {
            out.push(Use { path: parts.join("/"), module: parts.last().unwrap().clone(), span: last });
        }
        if !matches!(self.peek(), Tok::Newline | Tok::Eof) {
            self.err_here("expected a newline after the import");
            return Err(());
        }
        Ok(out)
    }

    /// `fn name(params) -> T of a { }`; for a method (`owner`), the first parameter may be the
    /// receiver `self`, `inout self` or `sink self`.
    /// A function or method; `has_body` is false for an interface method's signature.
    fn function(&mut self, owner: Option<&str>, has_body: bool) -> PResult<FnDecl> {
        self.bump();
        let (name, name_span) = self.ident("a function name")?;
        let type_params = self.type_params()?;
        self.expect(Tok::LParen, "`(`")?;
        let mut params = Vec::new();
        while !matches!(self.peek(), Tok::RParen) {
            let recv_inout = self.is_kw("inout") && matches!(self.peek_at(1), Tok::Ident(s) if s == "self");
            let recv_sink = self.is_kw("sink") && matches!(self.peek_at(1), Tok::Ident(s) if s == "self");
            if recv_inout || recv_sink || self.is_kw("self") {
                let start = self.span();
                if recv_inout || recv_sink {
                    self.bump();
                }
                let sp = self.bump().span;
                let Some(owner) = owner.filter(|_| params.is_empty()) else {
                    self.diags.error(
                        "E0020",
                        start,
                        "`self` can only be the first parameter of a method written inside a struct or enum",
                    );
                    return Err(());
                };
                let ty = TypeExpr { name: owner.to_string(), span: sp, args: Vec::new() };
                params.push(Param { name: "self".into(), span: sp, sink: recv_sink, inout: recv_inout, ty });
                if !self.eat(&Tok::Comma) {
                    break;
                }
                continue;
            }
            for m in ["inout", "sink"] {
                if self.is_kw(m) {
                    let sp = self.span();
                    self.unsupported(sp, format!("`{m}` parameters are not supported in P0"))?;
                }
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
        let body = if has_body { self.block()? } else { Block { stmts: Vec::new(), close: self.span() } };
        let name = match owner {
            Some(o) => format!("{o}.{name}"),
            None => name,
        };
        Ok(FnDecl { public: false, name, name_span, owner: owner.map(str::to_string), type_params, params, ret, ret_of, body })
    }

    /// A type, including `T ! E` (which binds loosest: `?String ! io.Error` is a result whose
    /// success value is optional).
    fn ty(&mut self) -> PResult<TypeExpr> {
        let t = self.ty_atom()?;
        if matches!(self.peek(), Tok::Bang) {
            self.bump();
            let e = self.ty_atom()?;
            let span = t.span.to(e.span);
            return Ok(TypeExpr { name: "!".into(), span, args: vec![t, e] });
        }
        Ok(t)
    }

    fn ty_atom(&mut self) -> PResult<TypeExpr> {
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
                self.bump();
                let inner = self.ty_atom()?;
                return Ok(TypeExpr { name: "&inout".into(), span: amp.to(inner.span), args: vec![inner] });
            }
            let inner = self.ty_atom()?;
            return Ok(TypeExpr { name: "&".into(), span: amp.to(inner.span), args: vec![inner] });
        }
        if matches!(self.peek(), Tok::Question) {
            let q = self.bump().span;
            let inner = self.ty_atom()?;
            return Ok(TypeExpr { name: "?".into(), span: q.to(inner.span), args: vec![inner] });
        }
        // `??T` is lexed as the `??` operator.
        if matches!(self.peek(), Tok::Coalesce) {
            let q = self.bump().span;
            let inner = self.ty_atom()?;
            let inner_span = Span { start: q.start + 1, col: q.col + 1, ..q }.to(inner.span);
            let opt = TypeExpr { name: "?".into(), span: inner_span, args: vec![inner] };
            return Ok(TypeExpr { name: "?".into(), span: q.to(opt.span), args: vec![opt] });
        }
        if matches!(self.peek(), Tok::LParen) && matches!(self.peek_at(1), Tok::RParen) {
            let open = self.bump().span;
            let close = self.bump().span;
            return Ok(TypeExpr { name: "()".into(), span: open.to(close), args: Vec::new() });
        }
        // `[]T`: a slice.
        if matches!(self.peek(), Tok::LBracket) && matches!(self.peek_at(1), Tok::RBracket) {
            let open = self.bump().span;
            self.bump();
            let inner = self.ty_atom()?;
            return Ok(TypeExpr { name: "[]".into(), span: open.to(inner.span), args: vec![inner] });
        }
        // `(A, B)`: a tuple type.
        if matches!(self.peek(), Tok::LParen) {
            let open = self.bump().span;
            let mut args = vec![self.ty()?];
            while self.eat(&Tok::Comma) {
                if matches!(self.peek(), Tok::RParen) {
                    break;
                }
                args.push(self.ty()?);
            }
            let close = self.expect(Tok::RParen, "`,` or `)` in the tuple type")?;
            if args.len() < 2 {
                self.diags.error("E0020", open.to(close), "a tuple type needs at least two elements, e.g. `(int, String)`");
                return Err(());
            }
            return Ok(TypeExpr { name: "(,)".into(), span: open.to(close), args });
        }
        if matches!(self.peek(), Tok::LBracket) {
            let sp = self.span();
            self.unsupported(sp, "array types are not supported yet")?;
        }
        if self.is_kw("Self") {
            let span = self.bump().span;
            return match self.self_ty.clone() {
                Some(name) => Ok(TypeExpr { name, span, args: Vec::new() }),
                None => {
                    self.diags.error("E0020", span, "`Self` can only be used inside a struct, enum or interface");
                    Err(())
                }
            };
        }
        let (name, span) = self.ident("a type")?;
        // `fs.Error`: a type from a standard library module.
        let (name, span) = if matches!(self.peek(), Tok::Dot) {
            self.bump();
            let (inner, isp) = self.ident("a type name after the module")?;
            (format!("{name}.{inner}"), span.to(isp))
        } else {
            (name, span)
        };
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
            // `Named + Send`: an interface type with capabilities (P2), as `name: "+"` with the
            // base type followed by the capability names.
            let mut all = vec![TypeExpr { name, span, args }];
            let mut end = span;
            while self.eat(&Tok::Plus) {
                let (cap, csp) = self.ident("a capability after `+`, e.g. `Send`")?;
                end = csp;
                all.push(TypeExpr { name: cap, span: csp, args: Vec::new() });
            }
            return Ok(TypeExpr { name: "+".into(), span: span.to(end), args: all });
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
                self.bump();
                let mut names = Vec::new();
                while !matches!(self.peek(), Tok::RParen) {
                    names.push(self.ident("a variable name (or `_`)")?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::RParen, "`,` or `)`")?;
                if names.len() < 2 {
                    self.err_here("a tuple pattern needs at least two names, e.g. `let (a, b) = t`");
                    return Err(());
                }
                self.expect(Tok::Assign, "`=` followed by a tuple value")?;
                let init = self.expr()?;
                let span = start.to(init.span);
                return Ok(Stmt { span, kind: StmtKind::LetTuple { mutable, names, init } });
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
                // `while let x = e { body }` is `loop { if let x = e { body } else { break } }`.
                let (name, name_span, value) = self.let_head()?;
                let then = self.block()?;
                let close = then.close;
                let brk = Stmt { kind: StmtKind::Break, span: start };
                let els = Some(Else::Block(Block { stmts: vec![brk], close }));
                let if_let = Stmt { kind: StmtKind::IfLet { name, name_span, value, then, els }, span: start };
                return Ok(Stmt { span: start, kind: StmtKind::Loop { body: Block { stmts: vec![if_let], close } } });
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
            let var2 = if self.eat(&Tok::Comma) { Some(self.ident("a second loop variable")?) } else { None };
            if !self.is_kw("in") {
                let found = describe(self.peek());
                self.err_here(format!("expected `in`, found {found}"));
                return Err(());
            }
            self.bump();
            let range_start = self.expr()?;
            if !self.eat(&Tok::DotDot) {
                let body = self.block()?;
                return Ok(Stmt { span: start, kind: StmtKind::ForIn { var, var_span, var2, iter: range_start, body } });
            }
            if let Some((_, sp)) = var2 {
                self.diags.error("E0020", sp, "a range loop has one variable: `for i in a..b`");
                return Err(());
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
        if self.is_kw("match") {
            return self.match_stmt();
        }
        if self.is_kw("with") {
            self.bump();
            let (name, name_span) = self.ident("a name for the lock guard after `with`")?;
            self.expect(Tok::Assign, "`=` after the guard name, e.g. `with g = m.lock() { ... }`")?;
            let init = self.expr()?;
            let body = self.block()?;
            return Ok(Stmt { span: start, kind: StmtKind::With { name, name_span, init, body } });
        }
        if self.is_kw("scope") {
            self.bump();
            let body = self.block()?;
            return Ok(Stmt { span: start, kind: StmtKind::Scope { body } });
        }
        for kw in ["defer", "unsafe"] {
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
        let head = if self.is_kw("let") { Some(self.let_head()?) } else { None };
        let cond = match head {
            Some(_) => None,
            None => Some(self.expr()?),
        };
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
        let kind = match (head, cond) {
            (Some((name, name_span, value)), _) => StmtKind::IfLet { name, name_span, value, then, els },
            (None, Some(cond)) => StmtKind::If { cond, then, els },
            (None, None) => unreachable!(),
        };
        Ok(Stmt { span: start, kind })
    }

    /// `let name = value` after `if` or `while`.
    fn let_head(&mut self) -> PResult<(String, Span, Expr)> {
        self.bump();
        if matches!(self.peek(), Tok::LParen) {
            let sp = self.span();
            self.unsupported(sp, "tuple destructuring is not supported yet")?;
        }
        let (name, name_span) = self.ident("a variable name")?;
        if matches!(self.peek(), Tok::LParen) {
            let sp = self.span();
            self.unsupported(sp, "patterns after `if let` are not supported; write `if let x = value`")?;
        }
        self.expect(Tok::Assign, "`=` followed by an optional value")?;
        let value = self.expr()?;
        Ok((name, name_span, value))
    }

    /// `match value { pattern => body ... }`, one arm per line (commas are also accepted).
    fn match_stmt(&mut self) -> PResult<Stmt> {
        let start = self.bump().span;
        let value = self.expr()?;
        let open = self.pos;
        self.expect(Tok::LBrace, "`{` to start the match arms")?;
        let arms = self.match_arms(start);
        if arms.is_err() {
            // Resume after the `}` that closes the match, so that an unsupported pattern does
            // not turn the rest of the arms into syntax errors.
            let mut depth = 0i32;
            let mut i = open;
            while i < self.toks.len() && !matches!(self.toks[i].tok, Tok::Eof) {
                match self.toks[i].tok {
                    Tok::LBrace => depth += 1,
                    Tok::RBrace => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            self.pos = i.min(self.toks.len() - 1);
        }
        Ok(Stmt { span: start, kind: StmtKind::Match { value, arms: arms? } })
    }

    fn match_arms(&mut self, start: Span) -> PResult<Vec<Arm>> {
        let mut arms = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Newline | Tok::Comma) {
                self.bump();
            }
            if matches!(self.peek(), Tok::RBrace) {
                break;
            }
            if matches!(self.peek(), Tok::Eof) {
                self.diags.error("E0020", start, "this `match` is never closed");
                return Err(());
            }
            let pstart = self.span();
            let pat = self.pattern()?;
            let pspan = pstart.to(self.toks[self.pos.saturating_sub(1)].span);
            self.expect(Tok::FatArrow, "`=>` after the pattern")?;
            let body = if matches!(self.peek(), Tok::LBrace) {
                self.block()?
            } else {
                let s = self.stmt()?;
                let close = s.span;
                Block { stmts: vec![s], close }
            };
            arms.push(Arm { pat, span: pspan, body });
            if !matches!(self.peek(), Tok::Newline | Tok::Comma | Tok::RBrace) {
                let found = describe(self.peek());
                self.err_here(format!("expected a newline or `,` after the match arm, found {found}"));
                return Err(());
            }
        }
        self.bump();
        Ok(arms)
    }

    /// `_`, a name, `none`, `some(p)`, `ok(p)`, `err(p)`, `.Case`, `.Case(p, ...)`, `Type.Case(...)`.
    fn pattern(&mut self) -> PResult<Pattern> {
        let start = self.span();
        let wrapped = |p: &mut Self, what: &str| -> PResult<Pattern> {
            p.bump();
            p.expect(Tok::LParen, &format!("`(` after `{what}`"))?;
            let inner = p.pattern()?;
            p.expect(Tok::RParen, &format!("`)` to close `{what}(...)`"))?;
            Ok(inner)
        };
        match self.peek().clone() {
            Tok::Ident(s) if s == "_" => {
                self.bump();
                Ok(Pattern::Wild)
            }
            Tok::Ident(s) if s == "none" => {
                self.bump();
                Ok(Pattern::None)
            }
            Tok::Ident(s) if s == "some" && matches!(self.peek_at(1), Tok::LParen) => {
                Ok(Pattern::Some(Box::new(wrapped(self, "some")?)))
            }
            Tok::Ident(s) if s == "ok" && matches!(self.peek_at(1), Tok::LParen) => {
                Ok(Pattern::Ok(Box::new(wrapped(self, "ok")?)))
            }
            Tok::Ident(s) if s == "err" => Ok(Pattern::Err(Box::new(wrapped(self, "err")?))),
            Tok::Dot => {
                self.bump();
                let (name, span) = self.ident("a case name after `.`")?;
                let args = self.case_pattern_args()?;
                Ok(Pattern::Case { ty: None, name, span: start.to(span), args })
            }
            Tok::Ident(s) if s.starts_with(|c: char| c.is_ascii_uppercase()) && matches!(self.peek_at(1), Tok::Dot) => {
                let tspan = self.bump().span;
                self.bump();
                // `fs.Error.Utf8(...)` is written with the module too.
                let (mut tname, mut tspan2) = (s, tspan);
                let (mut name, mut span) = self.ident("a case name")?;
                if matches!(self.peek(), Tok::Dot) {
                    self.bump();
                    tname = format!("{tname}.{name}");
                    tspan2 = tspan2.to(span);
                    (name, span) = self.ident("a case name")?;
                }
                let args = self.case_pattern_args()?;
                Ok(Pattern::Case { ty: Some((tname, tspan2)), name, span: start.to(span), args })
            }
            Tok::Ident(s) if !KEYWORDS.contains(&s.as_str()) && s.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') => {
                let span = self.bump().span;
                if matches!(self.peek(), Tok::Dot) {
                    // `fs.Error.Utf8(...)`: a module-qualified case.
                    self.bump();
                    let (t, tsp) = self.ident("a type name")?;
                    self.expect(Tok::Dot, "`.` and a case name")?;
                    let (name, nsp) = self.ident("a case name")?;
                    let args = self.case_pattern_args()?;
                    return Ok(Pattern::Case { ty: Some((format!("{s}.{t}"), span.to(tsp))), name, span: start.to(nsp), args });
                }
                if matches!(self.peek(), Tok::LParen) {
                    self.unsupported(start, format!("`{s}(...)` is not a pattern; enum cases are written `.Case(...)`"))?;
                }
                Ok(Pattern::Bind { name: s, span })
            }
            _ => {
                self.unsupported(start, "this kind of pattern is not supported yet (only names, `_`, `some`, `none`, `ok`, `err` and enum cases)")?;
                unreachable!()
            }
        }
    }

    fn case_pattern_args(&mut self) -> PResult<Vec<Pattern>> {
        let mut args = Vec::new();
        if self.eat(&Tok::LParen) {
            while !matches!(self.peek(), Tok::RParen) {
                if matches!(self.peek(), Tok::Ident(_)) && matches!(self.peek_at(1), Tok::Colon) {
                    let sp = self.span();
                    self.unsupported(sp, "named fields in case patterns are not supported yet; list the fields in order")?;
                }
                args.push(self.pattern()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(Tok::RParen, "`,` or `)`")?;
        }
        Ok(args)
    }

    /// The body of a `match` arm used as an expression.
    fn arm_value(&mut self) -> PResult<Expr> {
        if matches!(self.peek(), Tok::LBrace) {
            let start = self.span();
            let b = self.block()?;
            return Ok(Expr { span: start.to(b.close), kind: ExprKind::Block(b) });
        }
        if let Some(j) = self.jump()? {
            return Ok(j);
        }
        self.expr()
    }

    /// `return [value]`, `break` or `continue` where an expression is expected.
    fn jump(&mut self) -> PResult<Option<Expr>> {
        if self.is_kw("return") {
            let sp = self.bump().span;
            if matches!(self.peek(), Tok::Newline | Tok::RBrace | Tok::RParen | Tok::Comma | Tok::Eof) {
                return Ok(Some(Expr { span: sp, kind: ExprKind::Return(None) }));
            }
            let v = self.expr()?;
            return Ok(Some(Expr { span: sp.to(v.span), kind: ExprKind::Return(Some(Box::new(v))) }));
        }
        if self.is_kw("break") {
            return Ok(Some(Expr { span: self.bump().span, kind: ExprKind::Break }));
        }
        if self.is_kw("continue") {
            return Ok(Some(Expr { span: self.bump().span, kind: ExprKind::Continue }));
        }
        Ok(None)
    }

    /// `match value { pattern => value ... }` in an expression position.
    fn match_expr(&mut self) -> PResult<Expr> {
        let start = self.bump().span;
        let value = self.expr()?;
        self.expect(Tok::LBrace, "`{` to start the match arms")?;
        let mut arms = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Newline | Tok::Comma) {
                self.bump();
            }
            if matches!(self.peek(), Tok::RBrace) {
                break;
            }
            if matches!(self.peek(), Tok::Eof) {
                self.diags.error("E0020", start, "this `match` is never closed");
                return Err(());
            }
            let pstart = self.span();
            let pat = self.pattern()?;
            let pspan = pstart.to(self.toks[self.pos.saturating_sub(1)].span);
            self.expect(Tok::FatArrow, "`=>` after the pattern")?;
            let body = self.arm_value()?;
            arms.push(ExprArm { pat, span: pspan, body });
            if !matches!(self.peek(), Tok::Newline | Tok::Comma | Tok::RBrace) {
                let found = describe(self.peek());
                self.err_here(format!("expected a newline or `,` after the match arm, found {found}"));
                return Err(());
            }
        }
        let close = self.bump().span;
        Ok(Expr { span: start.to(close), kind: ExprKind::Match { value: Box::new(value), arms } })
    }

    fn expr(&mut self) -> PResult<Expr> {
        let mut e = self.binary(1)?;
        while self.is_kw("catch") {
            self.bump();
            let (name, name_span) = match self.peek().clone() {
                Tok::Ident(s) if !KEYWORDS.contains(&s.as_str()) => (s, self.bump().span),
                other => {
                    self.err_here(format!("expected a name for the error after `catch` (or `_`), found {}", describe(&other)));
                    return Err(());
                }
            };
            let (handler, block) = if matches!(self.peek(), Tok::LBrace) {
                let start = self.span();
                let b = self.block()?;
                (Expr { span: start.to(b.close), kind: ExprKind::Block(b) }, true)
            } else {
                self.expect(Tok::FatArrow, "`=> value` or a `{ ... }` block after the `catch` binding")?;
                let h = match self.jump()? {
                    Some(j) => j,
                    None => self.binary(1)?,
                };
                (h, false)
            };
            let span = e.span.to(handler.span);
            e = Expr {
                span,
                kind: ExprKind::Catch { value: Box::new(e), name, name_span, handler: Box::new(handler), block },
            };
        }
        Ok(e)
    }

    /// True when the `(` at the current position closes into `) =>`, i.e. a closure `(a, b) => expr`.
    /// `(a, b) => value` or `(a: int) => { block }`.
    fn closure(&mut self) -> PResult<Expr> {
        let open = self.bump().span;
        let mut params = Vec::new();
        while !matches!(self.peek(), Tok::RParen) {
            let (name, sp) = self.ident("a closure parameter name")?;
            let ty = if self.eat(&Tok::Colon) { Some(self.ty()?) } else { None };
            params.push((name, sp, ty));
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RParen, "`,` or `)`")?;
        self.expect(Tok::FatArrow, "`=>`")?;
        let body = if matches!(self.peek(), Tok::LBrace) {
            let b = self.block()?;
            Expr { span: b.close, kind: ExprKind::Block(b) }
        } else {
            self.expr()?
        };
        let span = open.to(body.span);
        Ok(Expr { span, kind: ExprKind::Closure { params, body: Box::new(body) } })
    }

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

    /// Precedence, lowest first: `||`, `&&`, comparisons (non-associative), `??` (right
    /// associative), `|`, `^`, `&`, shifts, additive (incl. wrapping), multiplicative (incl. wrapping).
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
            Tok::Pipe => (5, BinOp::BitOr),
            Tok::Caret => (6, BinOp::BitXor),
            Tok::Amp => (7, BinOp::BitAnd),
            Tok::Shl => (8, BinOp::Shl),
            Tok::Shr => (8, BinOp::Shr),
            Tok::Plus => (9, BinOp::Add),
            Tok::Minus => (9, BinOp::Sub),
            Tok::PlusWrap => (9, BinOp::AddWrap),
            Tok::MinusWrap => (9, BinOp::SubWrap),
            Tok::Star => (10, BinOp::Mul),
            Tok::Slash => (10, BinOp::Div),
            Tok::Percent => (10, BinOp::Rem),
            Tok::StarWrap => (10, BinOp::MulWrap),
            _ => return None,
        };
        (l == level).then_some(op)
    }

    fn binary(&mut self, level: u8) -> PResult<Expr> {
        if level > 10 {
            return self.unary();
        }
        if level == 4 {
            return self.coalesce();
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

    /// `a ?? b`, right associative. The right side may also be `return [value]`, `break` or
    /// `continue`.
    fn coalesce(&mut self) -> PResult<Expr> {
        let lhs = self.binary(5)?;
        if !matches!(self.peek(), Tok::Coalesce) {
            return Ok(lhs);
        }
        let op_span = self.bump().span;
        let rhs = if self.is_kw("return") {
            let sp = self.bump().span;
            if matches!(self.peek(), Tok::Newline | Tok::RBrace | Tok::RParen | Tok::Comma | Tok::Eof) {
                Expr { span: sp, kind: ExprKind::Return(None) }
            } else {
                let v = self.expr()?;
                Expr { span: sp.to(v.span), kind: ExprKind::Return(Some(Box::new(v))) }
            }
        } else if self.is_kw("break") {
            Expr { span: self.bump().span, kind: ExprKind::Break }
        } else if self.is_kw("continue") {
            Expr { span: self.bump().span, kind: ExprKind::Continue }
        } else {
            self.coalesce()?
        };
        let span = lhs.span.to(rhs.span);
        Ok(Expr { span, kind: ExprKind::Coalesce { op_span, lhs: Box::new(lhs), rhs: Box::new(rhs) } })
    }

    fn unary(&mut self) -> PResult<Expr> {
        let start = self.span();
        if self.is_kw("spawn") {
            self.bump();
            let inner = self.unary()?;
            return Ok(Expr { span: start.to(inner.span), kind: ExprKind::Spawn(Box::new(inner)) });
        }
        if self.is_kw("try") {
            self.bump();
            let inner = self.unary()?;
            return Ok(Expr { span: start.to(inner.span), kind: ExprKind::Try(Box::new(inner)) });
        }
        let op = match self.peek() {
            Tok::Minus => UnOp::Neg,
            Tok::Bang => UnOp::Not,
            Tok::Amp => {
                self.bump();
                let inner = self.unary()?;
                return Ok(Expr { span: start.to(inner.span), kind: ExprKind::Borrow(Box::new(inner)) });
            }
            Tok::Star => {
                // `*p`: read or (as an assignment target) write the place `p` points to (A6).
                UnOp::Deref
            }
            _ => return self.postfix(),
        };
        self.bump();
        let e = self.unary()?;
        Ok(Expr { span: start.to(e.span), kind: ExprKind::Unary { op, op_span: start, expr: Box::new(e) } })
    }

    fn qualified_expr_name(e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Ident(name) => Some(name.clone()),
            ExprKind::Field { base, name } => Self::qualified_expr_name(base).map(|prefix| format!("{prefix}.{name}")),
            _ => None,
        }
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        loop {
            match self.peek() {
                Tok::LParen => {
                    self.bump();
                    let mut args = Vec::new();
                    while !matches!(self.peek(), Tok::RParen) {
                        // `Case(name: value)`: a named argument of an enum case.
                        if let (Tok::Ident(n), Tok::Colon) = (self.peek().clone(), self.peek_at(1).clone()) {
                            let name_span = self.bump().span;
                            self.bump();
                            let value = self.expr()?;
                            let span = name_span.to(value.span);
                            args.push(Expr { span, kind: ExprKind::Named { name: n, name_span, value: Box::new(value) } });
                        } else {
                            args.push(self.expr()?);
                        }
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
                        Tok::Int(v) => {
                            let sp = self.bump().span;
                            (v.to_string(), sp)
                        }
                        other => {
                            self.err_here(format!("expected a field or method name, found {}", describe(&other)));
                            return Err(());
                        }
                    };
                    e = Expr { span: e.span.to(name_span), kind: ExprKind::Field { base: Box::new(e), name } };
                }
                Tok::LBracket => {
                    // `name[T](...)` / `Name[T]{...}`: type arguments, not an index.
                    if let Some(name) = Self::qualified_expr_name(&e) {
                        if self.type_args_ahead(name == "Heap" || name == "collections.Heap") {
                            self.bump();
                            let mut targs = vec![self.ty()?];
                            while self.eat(&Tok::Comma) {
                                targs.push(self.ty()?);
                            }
                            let close = self.expect(Tok::RBracket, "`,` or `]` after the type arguments")?;
                            if matches!(self.peek(), Tok::LBrace) {
                                e = self.struct_lit_body(name, e.span, targs)?;
                            } else {
                                e = Expr { span: e.span.to(close), kind: ExprKind::Generic { name, targs } };
                            }
                            continue;
                        }
                    }
                    let open = self.bump().span;
                    if matches!(self.peek(), Tok::DotDot) {
                        self.unsupported(open, "open-ended slices (`x[..b]`) are not supported yet; write `x[0..b]`")?;
                    }
                    let index = self.expr()?;
                    if matches!(self.peek(), Tok::DotDot) {
                        // `x[a..b]`: a view of elements (or bytes of text) `a` up to `b`.
                        self.bump();
                        if matches!(self.peek(), Tok::RBracket) {
                            self.unsupported(open, "open-ended slices (`x[a..]`) are not supported yet; write `x[a..x.len()]`")?;
                        }
                        let end = self.expr()?;
                        let close = self.expect(Tok::RBracket, "`]` to close the slice")?;
                        e = Expr {
                            span: e.span.to(close),
                            kind: ExprKind::SliceRange { base: Box::new(e), start: Box::new(index), end: Box::new(end) },
                        };
                        continue;
                    }
                    if matches!(self.peek(), Tok::Comma) {
                        self.unsupported(open, "type arguments in expressions (`Name[A, B]`) are not supported in P0")?;
                    }
                    let close = self.expect(Tok::RBracket, "`]`")?;
                    e = Expr { span: e.span.to(close), kind: ExprKind::Index { base: Box::new(e), index: Box::new(index) } };
                }
                Tok::Question => {
                    let sp = self.span();
                    self.unsupported(sp, "optional chaining (`?`) is not supported in P0")?;
                }
                // `Name{field: value}`: a struct literal. Written without a space, which keeps it
                // apart from `if cond {` blocks.
                Tok::LBrace
                    if Self::qualified_expr_name(&e).is_some_and(|n| n.rsplit('.').next().unwrap().starts_with(|c: char| c.is_ascii_uppercase()))
                        && self.span().start == e.span.end =>
                {
                    let name = Self::qualified_expr_name(&e).unwrap();
                    let name_span = e.span;
                    e = self.struct_lit_body(name, name_span, Vec::new())?;
                }
                _ => return Ok(e),
            }
        }
    }

    /// `{field: value, ...}` after a struct name (and its type arguments).
    fn struct_lit_body(&mut self, name: String, name_span: Span, targs: Vec<TypeExpr>) -> PResult<Expr> {
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
        Ok(Expr { span: name_span.to(close), kind: ExprKind::StructLit { name, name_span, targs, fields } })
    }

    fn primary(&mut self) -> PResult<Expr> {
        let span = self.span();
        match self.peek().clone() {
            Tok::Int(v) => {
                self.bump();
                Ok(Expr { span, kind: ExprKind::Int(v) })
            }
            Tok::Float(v) => {
                self.bump();
                Ok(Expr { span, kind: ExprKind::Float(v) })
            }
            Tok::Byte(v) => {
                self.bump();
                Ok(Expr { span, kind: ExprKind::Byte(v) })
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
                if s == "none" {
                    self.bump();
                    return Ok(Expr { span, kind: ExprKind::None });
                }
                if s == "err" && matches!(self.peek_at(1), Tok::LParen) {
                    self.bump();
                    self.bump();
                    let v = self.expr()?;
                    let close = self.expect(Tok::RParen, "`)` to close `err(...)`")?;
                    return Ok(Expr { span: span.to(close), kind: ExprKind::Err(Box::new(v)) });
                }
                if s == "match" {
                    return self.match_expr();
                }
                if s == "self" {
                    self.bump();
                    return Ok(Expr { span, kind: ExprKind::Ident(s) });
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
                    return self.closure();
                }
                self.bump();
                if matches!(self.peek(), Tok::RParen) {
                    let close = self.bump().span;
                    return Ok(Expr { span: span.to(close), kind: ExprKind::Unit });
                }
                let e = self.expr()?;
                if matches!(self.peek(), Tok::Comma) {
                    let mut items = vec![e];
                    while self.eat(&Tok::Comma) {
                        if matches!(self.peek(), Tok::RParen) {
                            break;
                        }
                        items.push(self.expr()?);
                    }
                    let close = self.expect(Tok::RParen, "`,` or `)` in the tuple")?;
                    return Ok(Expr { span: span.to(close), kind: ExprKind::Tuple(items) });
                }
                let close = self.expect(Tok::RParen, "`)`")?;
                Ok(Expr { span: span.to(close), kind: ExprKind::Paren(Box::new(e)) })
            }
            Tok::LBracket => {
                self.bump();
                if matches!(self.peek(), Tok::Colon) {
                    self.bump();
                    let close = self.expect(Tok::RBracket, "`]` to close `[:]`")?;
                    return Ok(Expr { span: span.to(close), kind: ExprKind::MapLit(Vec::new()) });
                }
                let mut items = Vec::new();
                let mut entries = Vec::new();
                loop {
                    while matches!(self.peek(), Tok::Newline | Tok::Comma) {
                        self.bump();
                    }
                    if matches!(self.peek(), Tok::RBracket) {
                        break;
                    }
                    let item = self.expr()?;
                    // `[k: v, ...]`: the first element decides between a map and a vector.
                    if items.is_empty() && (entries.is_empty() && matches!(self.peek(), Tok::Colon) || !entries.is_empty()) {
                        self.expect(Tok::Colon, "`:` between the key and the value")?;
                        let value = self.expr()?;
                        entries.push((item, value));
                    } else {
                        items.push(item);
                    }
                    if !matches!(self.peek(), Tok::Newline | Tok::Comma | Tok::RBracket) {
                        let found = describe(self.peek());
                        self.err_here(format!("expected `,` or `]` in the collection literal, found {found}"));
                        return Err(());
                    }
                }
                let close = self.bump().span;
                if !entries.is_empty() {
                    return Ok(Expr { span: span.to(close), kind: ExprKind::MapLit(entries) });
                }
                Ok(Expr { span: span.to(close), kind: ExprKind::VecLit(items) })
            }
            Tok::Dot => {
                self.bump();
                let (name, nsp) = self.ident("a case name after `.`")?;
                if matches!(self.peek(), Tok::LParen) && self.span().start == nsp.end {
                    self.bump();
                    let mut args = Vec::new();
                    while !matches!(self.peek(), Tok::RParen) {
                        if let (Tok::Ident(n), Tok::Colon) = (self.peek().clone(), self.peek_at(1).clone()) {
                            let name_span = self.bump().span;
                            self.bump();
                            let value = self.expr()?;
                            let vspan = name_span.to(value.span);
                            args.push(Expr { span: vspan, kind: ExprKind::Named { name: n, name_span, value: Box::new(value) } });
                        } else {
                            args.push(self.expr()?);
                        }
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    let close = self.expect(Tok::RParen, "`,` or `)`")?;
                    return Ok(Expr { span: span.to(close), kind: ExprKind::Case { name, args: Some(args) } });
                }
                Ok(Expr { span: span.to(nsp), kind: ExprKind::Case { name, args: None } })
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
                    let mut sub = Parser { toks, pos: 0, src: self.src, diags: &mut *self.diags, self_ty: self.self_ty.clone() };
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
