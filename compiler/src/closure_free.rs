//! Lexical free variables of a closure. Captures are resolved by the caller
//! against its enclosing function; type names, member names, and bindings made
//! inside the closure must not cause an owning capture of a same-named local.

use std::collections::HashSet;

use crate::ast::*;
use crate::diag::Span;

/// Each free value name once, in first-use order, with its actual use span.
/// Descend into nested closures: their free references must still be available
/// when the enclosing closure eventually creates them. Nested parameters and
/// locals remain bound only within their lexical scope.
pub fn free_variables(
    params: &[(String, Span, Option<TypeExpr>)],
    body: &Expr,
) -> Vec<(String, Span)> {
    let mut collector = Collector {
        scopes: vec![HashSet::new()],
        seen: HashSet::new(),
        free: Vec::new(),
    };
    for (name, _, _) in params {
        collector.bind(name);
    }
    collector.expr(body);
    collector.free
}

struct Collector {
    scopes: Vec<HashSet<String>>,
    seen: HashSet<String>,
    free: Vec<(String, Span)>,
}

impl Collector {
    fn bind(&mut self, name: &str) {
        if name != "_" {
            self.scopes.last_mut().unwrap().insert(name.to_string());
        }
    }

    fn reference(&mut self, name: &str, span: Span) {
        if !self.scopes.iter().rev().any(|scope| scope.contains(name))
            && self.seen.insert(name.to_string())
        {
            self.free.push((name.to_string(), span));
        }
    }

    fn block(&mut self, block: &Block) {
        self.scopes.push(HashSet::new());
        for statement in &block.stmts {
            self.statement(statement);
        }
        self.scopes.pop();
    }

    fn pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Bind { name, .. } => self.bind(name),
            Pattern::Some(inner) | Pattern::Ok(inner) | Pattern::Err(inner) => self.pattern(inner),
            Pattern::Case { args, .. } => {
                for arg in args {
                    self.pattern(arg);
                }
            }
            Pattern::None | Pattern::Wild => {}
        }
    }

    fn otherwise(&mut self, otherwise: &Option<Else>) {
        match otherwise {
            Some(Else::Block(block)) => self.block(block),
            Some(Else::If(statement)) => self.statement(statement),
            None => {}
        }
    }

    fn statement(&mut self, statement: &Stmt) {
        match &statement.kind {
            StmtKind::Let { name, init, .. } => {
                // `let x = x + 1` reads the old x, then introduces the new x.
                self.expr(init);
                self.bind(name);
            }
            StmtKind::LetTuple { names, init, .. } => {
                self.expr(init);
                for (name, _) in names {
                    self.bind(name);
                }
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => self.expr(expr),
            StmtKind::If { cond, then, els } => {
                self.expr(cond);
                self.block(then);
                self.otherwise(els);
            }
            StmtKind::IfLet { name, value, then, els, .. } => {
                self.expr(value);
                self.scopes.push(HashSet::new());
                self.bind(name);
                self.block(then);
                self.scopes.pop();
                // The successful-match binding is not in the else branch.
                self.otherwise(els);
            }
            StmtKind::Match { value, arms } => {
                self.expr(value);
                for arm in arms {
                    self.scopes.push(HashSet::new());
                    self.pattern(&arm.pat);
                    self.block(&arm.body);
                    self.scopes.pop();
                }
            }
            StmtKind::While { cond, body } => {
                self.expr(cond);
                self.block(body);
            }
            StmtKind::Loop { body } | StmtKind::Block(body) | StmtKind::Scope { body } => self.block(body),
            StmtKind::For { var, start, end, body, .. } => {
                self.expr(start);
                self.expr(end);
                self.scopes.push(HashSet::new());
                self.bind(var);
                self.block(body);
                self.scopes.pop();
            }
            StmtKind::ForIn { var, var2, iter, body, .. } => {
                self.expr(iter);
                self.scopes.push(HashSet::new());
                self.bind(var);
                if let Some((name, _)) = var2 {
                    self.bind(name);
                }
                self.block(body);
                self.scopes.pop();
            }
            StmtKind::With { name, init, body, .. } => {
                self.expr(init);
                self.scopes.push(HashSet::new());
                self.bind(name);
                self.block(body);
                self.scopes.pop();
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
        }
    }

    fn expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Ident(name) => self.reference(name, expr.span),
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Byte(_) | ExprKind::Bool(_)
            | ExprKind::None | ExprKind::Break | ExprKind::Continue | ExprKind::Unit
            | ExprKind::Generic { .. } => {}
            ExprKind::Str(segments) => {
                for segment in segments {
                    if let StrSeg::Expr { expr, .. } = segment {
                        self.expr(expr);
                    }
                }
            }
            ExprKind::Call { callee, args } => {
                self.expr(callee);
                for arg in args {
                    self.expr(arg);
                }
            }
            ExprKind::Field { base, .. } => self.expr(base),
            ExprKind::StructLit { fields, .. } => {
                for field in fields {
                    self.expr(&field.value);
                }
            }
            ExprKind::VecLit(items) | ExprKind::Tuple(items) => {
                for item in items {
                    self.expr(item);
                }
            }
            ExprKind::MapLit(entries) => {
                for (key, value) in entries {
                    self.expr(key);
                    self.expr(value);
                }
            }
            ExprKind::Index { base, index } => {
                self.expr(base);
                self.expr(index);
            }
            ExprKind::SliceRange { base, start, end } => {
                self.expr(base);
                self.expr(start);
                self.expr(end);
            }
            ExprKind::Borrow(inner) | ExprKind::Paren(inner) | ExprKind::Err(inner)
            | ExprKind::Try(inner) | ExprKind::Spawn(inner)
            | ExprKind::Unary { expr: inner, .. } | ExprKind::Named { value: inner, .. } => self.expr(inner),
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::Coalesce { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            ExprKind::Closure { params, body } => {
                self.scopes.push(HashSet::new());
                for (name, _, _) in params {
                    self.bind(name);
                }
                self.expr(body);
                self.scopes.pop();
            }
            ExprKind::Case { args, .. } => {
                for arg in args.iter().flatten() {
                    self.expr(arg);
                }
            }
            ExprKind::Catch { value, name, handler, .. } => {
                self.expr(value);
                self.scopes.push(HashSet::new());
                self.bind(name);
                self.expr(handler);
                self.scopes.pop();
            }
            ExprKind::Match { value, arms } => {
                self.expr(value);
                for arm in arms {
                    self.scopes.push(HashSet::new());
                    self.pattern(&arm.pat);
                    self.expr(&arm.body);
                    self.scopes.pop();
                }
            }
            ExprKind::Block(block) => self.block(block),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{diag::Diags, lexer, parser};

    fn captures(body: &str) -> Vec<String> {
        let source = format!("fn main() {{\nlet f = (a, b) => {body}\n}}\n");
        let mut diags = Diags::default();
        let tokens = lexer::lex_file(&source, &mut diags);
        let program = parser::parse(tokens, &source, &mut diags);
        assert!(!diags.has_errors(), "{:?}", diags.items);
        let StmtKind::Let { init, .. } = &program.fns[0].body.stmts[0].kind else {
            panic!("expected closure binding");
        };
        let ExprKind::Closure { params, body } = &init.kind else {
            panic!("expected closure expression");
        };
        let free = free_variables(params, body);
        for (name, span) in &free {
            assert_eq!(&source[span.start..span.end], name);
        }
        free.into_iter().map(|(name, _)| name).collect()
    }

    #[test]
    fn local_shadow_does_not_capture_outer_resource() {
        assert!(captures("{\nlet resource = 1\nreturn a + resource < b\n}").is_empty());
        assert_eq!(captures("a + offset + offset < b + bias"), ["offset", "bias"]);
    }

    #[test]
    fn initializer_reads_outer_binding_before_local_exists() {
        assert_eq!(captures("{\nlet x = x + 1\nlet (y, z) = (seed, x)\nreturn a + y + z < b\n}"), ["x", "seed"]);
        assert_eq!(captures("{\n{ let hidden = 1 }\nreturn a + hidden < b\n}"), ["hidden"]);
    }

    #[test]
    fn nested_closures_propagate_only_lexically_free_names() {
        assert_eq!(captures("{\nlet local = 2\nlet inner = (token) => token + local + a + outer\nreturn inner(b) < a\n}"), ["outer"]);
    }

    #[test]
    fn branch_loop_pattern_and_handler_bindings_have_limited_scope() {
        assert_eq!(captures(r#"{
if let token = source {
    let value = token
} else {
    let value = token
}
for key, value in items { let pair = (key, value) }
for i in lower..upper { let value = i }
match choice {
    .Some(token) => { let value = token }
    _ => { let value = fallback }
}
let result = fallible catch error => error + a
with guard = lock { let value = guard }
return a < b
}"#), ["source", "token", "items", "lower", "upper", "choice", "fallback", "fallible", "lock"]);
    }

    #[test]
    fn only_value_references_are_captures() {
        assert_eq!(captures(r#"{
let value = Container[int]{field: payload}
let text = "${value.field} ${external}"
let result = match choice {
    some(inner) => inner
    none => fallback
}
return a + result < b + generic[int](argument)
}"#), ["payload", "external", "choice", "fallback", "argument"]);
    }
}
