//! Conservative allocation effects for the checked program.
//!
//! Effects are a pair of facts, not a single ordinal: a known allocation must not
//! erase evidence of an opaque call. Direct calls (including implicit Drop and
//! lifted callbacks) are solved to a fixed point. No implementation-set reasoning
//! is used for interface dispatch. Run this on the checker's thread: Ty's
//! interned definitions are thread-local.

use std::collections::{BTreeSet, HashSet};

use serde_json::{json, Value};

use crate::ast;
use crate::check::{
    self, BindMode, Builtin, CatchHandler, HExpr, HExprKind, HProgram,
    HStmt, HStmtKind, IoOp, MapRead, MapWrite, ArenaWrite, Method2, PrintPart,
};
use crate::diag::{self, Span};
use crate::types::{
    box_inner, has_user_drop, map_parts, opt_inner, res_parts, sync_parts,
    tuple_elems, vec_elem, SyncKind, Ty,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Effect {
    may_alloc: bool,
    unknown: bool,
}

impl Effect {
    const NONE: Self = Self { may_alloc: false, unknown: false };
    const MAY: Self = Self { may_alloc: true, unknown: false };
    const UNKNOWN: Self = Self { may_alloc: false, unknown: true };

    fn join(self, other: Self) -> Self {
        Self {
            may_alloc: self.may_alloc || other.may_alloc,
            unknown: self.unknown || other.unknown,
        }
    }

    fn status(self) -> &'static str {
        if self.may_alloc {
            "may_alloc"
        } else if self.unknown {
            "unknown"
        } else {
            "no_alloc"
        }
    }
}

struct Callee {
    direct: Effect,
    reason: &'static str,
    targets: Vec<usize>,
    operations: Vec<Operation>,
}

struct Operation {
    kind: &'static str,
    name: String,
    span: Span,
    parameters: Vec<Operation>,
    callee: Callee,
}

impl Operation {
    fn new(
        kind: &'static str,
        name: impl Into<String>,
        span: Span,
        parameters: Vec<Operation>,
        effect: Effect,
        reason: &'static str,
    ) -> Self {
        Self {
            kind, name: name.into(), span, parameters,
            callee: Callee {
                direct: effect, reason, targets: Vec::new(), operations: Vec::new(),
            },
        }
    }

    fn callee_effect(&self, effects: &[Effect]) -> Effect {
        self.callee.targets.iter().fold(
            self.callee.direct.join(effects_of(&self.callee.operations, effects)),
            |effect, &target| effect.join(effects.get(target).copied().unwrap_or(Effect::UNKNOWN)),
        )
    }

    fn effect(&self, effects: &[Effect]) -> Effect {
        effects_of(&self.parameters, effects).join(self.callee_effect(effects))
    }

    fn collect_targets(&self, targets: &mut BTreeSet<usize>) {
        targets.extend(self.callee.targets.iter().copied());
        for op in self.parameters.iter().chain(&self.callee.operations) {
            op.collect_targets(targets);
        }
    }

    fn can_leave_expression(&self) -> bool {
        self.kind == "control_transfer"
            || self.parameters.iter().chain(&self.callee.operations)
                .any(Operation::can_leave_expression)
    }

    fn json(&self, effects: &[Effect], typed: &HProgram, path: &str) -> Value {
        let parameters = effects_of(&self.parameters, effects);
        let callee = self.callee_effect(effects);
        let overall = parameters.join(callee);
        json!({
            "kind": self.kind,
            "name": self.name,
            "status": overall.status(),
            "has_unknown": overall.unknown,
            "location": {
                "file": path,
                "line": self.span.line,
                "column": self.span.col,
                "start": self.span.start,
                "end": self.span.end,
            },
            "parameters": {
                "status": parameters.status(),
                "has_unknown": parameters.unknown,
                "operations": self.parameters.iter()
                    .map(|op| op.json(effects, typed, path)).collect::<Vec<_>>(),
            },
            "callee": {
                "name": self.name,
                "status": callee.status(),
                "has_unknown": callee.unknown,
                "reason": self.callee.reason,
                "targets": self.callee.targets.iter().map(|&id| {
                    let effect = effects.get(id).copied().unwrap_or(Effect::UNKNOWN);
                    json!({
                        "id": id,
                        "name": typed.fns.get(id).map(|f| f.name.as_str()),
                        "status": effect.status(),
                        "has_unknown": effect.unknown,
                    })
                }).collect::<Vec<_>>(),
                "operations": self.callee.operations.iter()
                    .map(|op| op.json(effects, typed, path)).collect::<Vec<_>>(),
            },
        })
    }
}

fn effects_of(operations: &[Operation], effects: &[Effect]) -> Effect {
    operations.iter().fold(Effect::NONE, |effect, op| effect.join(op.effect(effects)))
}

struct Analyzer<'a> {
    program: &'a ast::Program,
    typed: &'a HProgram,
}

impl Analyzer<'_> {
    fn declaration_span(&self, id: usize) -> Span {
        // Initial HIR entries preserve source declaration order. Lifted bodies
        // still carry original expression spans even when they have no FnDecl.
        self.program.fns.get(id).map(|f| f.name_span)
            .or_else(|| self.typed.fns.get(id).and_then(|f| f.body.first().map(|s| s.span)))
            .unwrap_or_default()
    }

    fn is_template(&self, id: usize) -> bool {
        self.program.fns.get(id).is_some_and(|f| {
            !f.type_params.is_empty()
                || f.owner.as_ref().is_some_and(|owner| self.program.structs.iter()
                    .any(|s| s.name == *owner && !s.type_params.is_empty()))
        })
    }

    fn call(&self, kind: &'static str, id: usize, span: Span, parameters: Vec<Operation>) -> Operation {
        let name = self.typed.fns.get(id).map(|f| f.name.clone())
            .unwrap_or_else(|| format!("unresolved function {id}"));
        let mut op = Operation::new(
            kind, name, span, parameters, Effect::NONE,
            "effect of the checked function body, including implicit cleanup",
        );
        op.callee.targets.push(id);
        op
    }

    /// Include all potentially executed destructors. Fields and enum alternatives
    /// are deliberately joined without a value-flow or reference-count proof.
    /// Recursive type edges are visited once; their user Drop functions remain
    /// ordinary call-graph edges and are resolved by the same fixed point.
    fn drop_calls(&self, ty: Ty, span: Span, seen: &mut HashSet<Ty>, ops: &mut Vec<Operation>) {
        if !seen.insert(ty) {
            return;
        }
        match ty {
            Ty::Struct(_) | Ty::Enum(_) => {
                if has_user_drop(ty) {
                    // Resolve the actual checked receiver type, including generic
                    // instances, rather than guessing from the source spelling.
                    let target = self.typed.fns.iter().enumerate().find(|(_, f)| {
                        f.name.rsplit('.').next() == Some("drop")
                            && f.params.first().and_then(|&id| f.locals.get(id))
                                .is_some_and(|receiver| receiver.ty == ty)
                    }).map(|(id, _)| id);
                    if let Some(id) = target {
                        ops.push(self.call("user_drop", id, span, Vec::new()));
                    } else {
                        ops.push(Operation::new(
                            "user_drop", format!("{ty}.drop"), span, Vec::new(),
                            Effect::UNKNOWN, "a user destructor has no resolved checked body",
                        ));
                    }
                }
                if let Some(s) = self.typed.structs.iter().find(|s| s.ty == ty) {
                    for (_, field) in &s.fields {
                        self.drop_calls(*field, span, seen, ops);
                    }
                    for (_, fields) in &s.variants {
                        for (_, field) in fields {
                            self.drop_calls(*field, span, seen, ops);
                        }
                    }
                } else {
                    ops.push(Operation::new(
                        "cleanup", format!("drop {ty}"), span, Vec::new(),
                        Effect::UNKNOWN, "aggregate cleanup definition is unavailable",
                    ));
                }
            }
            Ty::Vec(id) | Ty::Deque(id) | Ty::Arena(id) => self.drop_calls(vec_elem(id), span, seen, ops),
            Ty::Heap(id) => {
                self.drop_calls(vec_elem(id), span, seen, ops);
                ops.push(Operation::new(
                    "dynamic_drop", "drop Heap comparator captures", span, Vec::new(),
                    Effect::UNKNOWN, "the saved comparator environment has an erased destructor",
                ));
            }
            Ty::Map(id) | Ty::Set(id) => {
                let (key, value) = map_parts(id);
                self.drop_calls(key, span, seen, ops);
                self.drop_calls(value, span, seen, ops);
            }
            Ty::Opt(id) => self.drop_calls(opt_inner(id), span, seen, ops),
            Ty::Res(id) => {
                let (ok, err) = res_parts(id);
                self.drop_calls(ok, span, seen, ops);
                self.drop_calls(err, span, seen, ops);
            }
            Ty::Tuple(id) => {
                for elem in tuple_elems(id) {
                    self.drop_calls(elem, span, seen, ops);
                }
            }
            Ty::Box(id) => self.drop_calls(box_inner(id), span, seen, ops),
            Ty::Sync(id) => {
                let (kind, inner) = sync_parts(id);
                match kind {
                    SyncKind::Rc | SyncKind::Arc | SyncKind::Mutex | SyncKind::Channel => {
                        self.drop_calls(inner, span, seen, ops);
                    }
                    // Guards only unlock. Join results discarded by a scope are
                    // recorded at Spawn, where the concrete return type is known.
                    SyncKind::Guard | SyncKind::Join | SyncKind::Scope => {}
                }
            }
            Ty::Dyn(_) => ops.push(Operation::new(
                "dynamic_drop", format!("drop {ty}"), span, Vec::new(),
                Effect::UNKNOWN, "interface payload destructor is dynamically selected",
            )),
            Ty::Error => ops.push(Operation::new(
                "cleanup", "unresolved cleanup type", span, Vec::new(),
                Effect::UNKNOWN, "type checking did not establish the cleanup type",
            )),
            // Deallocation itself does not allocate Yavo heap storage. Views do
            // not own their referents, and scalars have no cleanup.
            Ty::String | Ty::Float(_) | Ty::Int(_) | Ty::Bool | Ty::Str
            | Ty::Ref(_) | Ty::RefMut(_) | Ty::Slice(_) | Ty::Id(_) | Ty::Unit => {}
        }
    }

    fn cleanup(&self, ty: Ty, span: Span, reason: &'static str) -> Option<Operation> {
        if !ty.needs_drop() && ty != Ty::Error {
            return None;
        }
        let mut op = Operation::new(
            "implicit_drop", format!("drop {ty}"), span, Vec::new(), Effect::NONE, reason,
        );
        self.drop_calls(ty, span, &mut HashSet::new(), &mut op.callee.operations);
        Some(op)
    }

    fn function(&self, id: usize) -> Vec<Operation> {
        let span = self.declaration_span(id);
        if self.is_template(id) {
            return vec![Operation::new(
                "generic_template", self.program.fns[id].name.clone(), span, Vec::new(),
                Effect::UNKNOWN,
                "generic templates are not checked for all possible type arguments; query a checked specialization",
            )];
        }
        let function = &self.typed.fns[id];
        let visitor = Visitor { analyzer: self };
        let mut ops = visitor.statements(&function.body);
        // These are potential scope exits, not a claim that every local is
        // dropped on every path. A moved local may be empty on a particular exit.
        for local in &function.locals {
            if local.owned {
                if let Some(mut cleanup) = self.cleanup(
                    local.ty, span, "owned local or sink parameter may be dropped on a scope exit",
                ) {
                    cleanup.name = format!("drop {}: {}", local.name, local.ty);
                    ops.push(cleanup);
                }
            }
        }
        ops
    }
}

struct Visitor<'a, 'b> {
    analyzer: &'a Analyzer<'b>,
}

impl Visitor<'_, '_> {
    fn exprs<'a>(&self, expressions: impl IntoIterator<Item = &'a HExpr>) -> Vec<Operation> {
        expressions.into_iter().flat_map(|e| self.expr(e)).collect()
    }

    fn borrowed_exprs<'a>(&self, expressions: impl IntoIterator<Item = &'a HExpr>) -> Vec<Operation> {
        expressions.into_iter().flat_map(|e| self.borrowed(e)).collect()
    }

    fn parts(&self, parts: &[PrintPart]) -> Vec<Operation> {
        parts.iter().flat_map(|part| match part {
            PrintPart::Text(_) => Vec::new(),
            PrintPart::Value(value) | PrintPart::Fixed(value, _) => self.borrowed(value),
        }).collect()
    }

    fn transfer(&self, name: &'static str, span: Span, parameters: Vec<Operation>) -> Operation {
        Operation::new(
            "control_transfer", name, span, parameters, Effect::NONE,
            "control transfer itself does not allocate; live owned temporaries may require cleanup",
        )
    }

    fn construction<'a>(
        &self, expr: &HExpr, values: impl IntoIterator<Item = &'a HExpr>,
    ) -> Vec<Operation> {
        let values: Vec<_> = values.into_iter().collect();
        let evaluated: Vec<_> = values.iter().map(|value| self.expr(value)).collect();
        let mut cleanup = Vec::new();
        let mut later_can_leave = false;
        for (value, operations) in values.iter().zip(&evaluated).rev() {
            if later_can_leave {
                cleanup.extend(self.analyzer.cleanup(
                    value.ty, value.span,
                    "a later field can leave the expression, dropping this already constructed field",
                ));
            }
            later_can_leave |= operations.iter().any(Operation::can_leave_expression);
        }
        let mut op = Operation::new(
            "construction", format!("construct {}", expr.ty), expr.at,
            evaluated.into_iter().flatten().collect(), Effect::NONE,
            "aggregate storage does not allocate; partial-construction cleanup is analyzed separately",
        );
        op.callee.operations = cleanup;
        vec![op]
    }

    /// Mirrors codegen::borrowed's owned-temporary boundary. A borrowed Local,
    /// field, reference, or interface receiver must not acquire a fictitious Drop.
    fn borrowed(&self, expr: &HExpr) -> Vec<Operation> {
        let mut ops = self.expr(expr);
        let fresh = matches!(
            expr.kind,
            HExprKind::Call { .. } | HExprKind::LetTemp { .. } | HExprKind::VecLit(_) | HExprKind::Move(_)
                | HExprKind::HeapNew { .. } | HExprKind::DequeNew | HExprKind::DequePop { .. }
                | HExprKind::ArenaNew | HExprKind::ArenaWrite { .. }
                | HExprKind::Some(_) | HExprKind::Coalesce { .. } | HExprKind::Take(_)
                | HExprKind::Replace(..) | HExprKind::Pop(_) | HExprKind::MapLit(_)
                | HExprKind::MapWrite { .. } | HExprKind::StructLit { .. }
                | HExprKind::TupleLit(_) | HExprKind::EnumLit { .. } | HExprKind::Ok(_)
                | HExprKind::ErrVal(_) | HExprKind::Try(_) | HExprKind::Catch { .. }
                | HExprKind::Match { .. } | HExprKind::Format(_) | HExprKind::EmptyString
                | HExprKind::BoxNew(_) | HExprKind::SyncNew { .. } | HExprKind::Lock { .. }
                | HExprKind::SyncClone(_) | HExprKind::JoinCall(_) | HExprKind::ChanNew(_)
                | HExprKind::ChanSend { .. } | HExprKind::ChanRecv(_)
                | HExprKind::ToDyn { .. } | HExprKind::DynCall { .. } | HExprKind::Io { .. }
                | HExprKind::FromUtf8(_)
                | HExprKind::Call2 {
                    method: Method2::Split | Method2::ToLower | Method2::ToUpper | Method2::Join, ..
                }
                | HExprKind::Method {
                    method: Builtin::StrToString | Builtin::StringClone | Builtin::VecClone
                        | Builtin::SliceToVec | Builtin::MapClone | Builtin::Fields | Builtin::Lines, ..
                }
        );
        if fresh {
            if let Some(cleanup) = self.analyzer.cleanup(
                expr.ty, expr.span, "a borrowed owned temporary is dropped after its use",
            ) {
                ops.push(cleanup);
            }
        }
        ops
    }

    fn statements(&self, statements: &[HStmt]) -> Vec<Operation> {
        let mut ops = Vec::new();
        for statement in statements {
            match &statement.kind {
                HStmtKind::Let { init, .. } => ops.extend(self.expr(init)),
                HStmtKind::LetTuple { locals, init } => {
                    ops.extend(self.expr(init));
                    if let Ty::Tuple(id) = init.ty {
                        for (local, ty) in locals.iter().zip(tuple_elems(id)) {
                            if local.is_none() {
                                ops.extend(self.analyzer.cleanup(
                                    ty, statement.span, "a discarded tuple binding is dropped immediately",
                                ));
                            }
                        }
                    }
                }
                HStmtKind::Assign { value, .. } => {
                    ops.extend(self.expr(value));
                    ops.extend(self.analyzer.cleanup(
                        value.ty, statement.span, "assignment drops the previous value before replacement",
                    ));
                }
                HStmtKind::Discard(value) | HStmtKind::Expr(value) => {
                    ops.extend(self.borrowed(value));
                }
                HStmtKind::If { cond, then, els } => {
                    ops.extend(self.expr(cond));
                    ops.extend(self.statements(then));
                    ops.extend(self.statements(els));
                }
                HStmtKind::While { cond, body } => {
                    ops.extend(self.expr(cond));
                    ops.extend(self.statements(body));
                }
                HStmtKind::For { start, end, body, .. } => {
                    ops.extend(self.expr(start));
                    ops.extend(self.expr(end));
                    ops.extend(self.statements(body));
                }
                HStmtKind::Loop { body } | HStmtKind::ForEach { body, .. }
                | HStmtKind::ForMap { body, .. } | HStmtKind::Block(body) => {
                    ops.extend(self.statements(body));
                }
                HStmtKind::IfLet { scrut, mode, then, els, .. } => {
                    ops.extend(self.expr(scrut));
                    if *mode == BindMode::Owned {
                        ops.extend(self.analyzer.cleanup(
                            scrut.ty, statement.span, "owned pattern input may have unbound parts to drop",
                        ));
                    }
                    ops.extend(self.statements(then));
                    ops.extend(self.statements(els));
                }
                HStmtKind::Return(value) => {
                    let parameters = value.as_ref().map_or_else(Vec::new, |value| self.expr(value));
                    ops.push(self.transfer("return", statement.span, parameters));
                }
                HStmtKind::Print { fd, newline, parts } => {
                    let name = if *fd == 2 { "eprintln" } else if *newline { "println" } else { "print" };
                    ops.push(Operation::new(
                        "builtin", name, statement.span, self.parts(parts), Effect::NONE,
                        "output itself does not allocate; argument evaluation is separate (M3)",
                    ));
                }
                HStmtKind::Match { scrut, owned, arms } => {
                    ops.extend(self.expr(scrut));
                    if *owned {
                        ops.extend(self.analyzer.cleanup(
                            scrut.ty, statement.span, "owned match input may have unbound parts to drop",
                        ));
                    }
                    for arm in arms {
                        ops.extend(self.statements(&arm.body));
                        if let Some(value) = &arm.value {
                            ops.extend(self.expr(value));
                        }
                    }
                }
                HStmtKind::Break | HStmtKind::Continue => {
                    let name = if matches!(statement.kind, HStmtKind::Break) { "break" } else { "continue" };
                    ops.push(self.transfer(name, statement.span, Vec::new()));
                }
            }
        }
        ops
    }

    fn builtin(
        &self, name: impl Into<String>, expr: &HExpr, parameters: Vec<Operation>,
        effect: Effect, reason: &'static str,
    ) -> Vec<Operation> {
        vec![Operation::new("builtin", name, expr.at, parameters, effect, reason)]
    }

    fn expr(&self, expr: &HExpr) -> Vec<Operation> {
        use HExprKind as E;
        const NO: &str = "checked intrinsic has no heap allocation; parameters and cleanup are separate";
        const ALLOC: &str = "this construction or standard-library operation may allocate heap storage";
        match &expr.kind {
            E::Int(_) | E::Float(_) | E::Bool(_) | E::Str(_) | E::Local(_)
            | E::Move(_) | E::InoutArg(_) | E::InoutPlace(_) | E::None
            | E::EmptyString | E::ScopeNew | E::DequeNew | E::ArenaNew => Vec::new(),
            E::HeapNew { caps, .. } => self.builtin(
                "Heap.by", expr, self.exprs(caps),
                if caps.is_empty() { Effect::NONE } else { Effect::MAY },
                "a saved comparator owns its captures; a nonempty environment allocates",
            ),
            E::LetTemp { init, value, .. } => {
                let mut ops = self.expr(init);
                ops.extend(self.expr(value));
                ops
            }
            E::Break | E::Continue => {
                let name = if matches!(expr.kind, E::Break) { "break" } else { "continue" };
                vec![self.transfer(name, expr.at, Vec::new())]
            }
            E::Error => vec![Operation::new(
                "unresolved", "unresolved expression", expr.at, Vec::new(),
                Effect::UNKNOWN, "expression has no checked implementation",
            )],
            E::Call { func, args } => {
                let target = self.analyzer.typed.fns.get(*func);
                let parameters = args.iter().enumerate().flat_map(|(index, arg)| {
                    if target.and_then(|f| f.sinks.get(index)).copied().unwrap_or(false) {
                        self.expr(arg)
                    } else {
                        self.borrowed(arg)
                    }
                }).collect();
                vec![self.analyzer.call("call", *func, expr.at, parameters)]
            }
            E::DynCall { iface, method, args, .. } => {
                let name = self.analyzer.program.interfaces.get(*iface)
                    .and_then(|i| i.methods.get(*method)).map(|m| m.name.clone())
                    .unwrap_or_else(|| format!("interface[{iface}].method[{method}]"));
                let signature = self.analyzer.typed.ifaces.get(*iface)
                    .and_then(|i| i.methods.get(*method));
                let parameters = args.iter().enumerate().flat_map(|(index, arg)| {
                    if signature.and_then(|s| s.sinks.get(index)).copied().unwrap_or(false) {
                        self.expr(arg)
                    } else {
                        self.borrowed(arg)
                    }
                }).collect();
                vec![Operation::new(
                    "dynamic_call", name, expr.at, parameters, Effect::UNKNOWN,
                    "interface dispatch is opaque; current implementations do not prove an allocation contract",
                )]
            }
            E::Neg(value) | E::Not(value) | E::Convert { expr: value, .. }
            | E::ToFloat(value) | E::FloatToInt { expr: value, .. } | E::Abs(value) => self.expr(value),
            E::Binary { lhs, rhs, .. } | E::SameTag { lhs, rhs, .. } => {
                let mut ops = self.borrowed(lhs);
                ops.extend(self.borrowed(rhs));
                ops
            }
            E::Math { func, args } => self.builtin(
                format!("math.{func}"), expr, self.borrowed_exprs(args), Effect::UNKNOWN,
                "the external C math implementation has no verified allocation contract",
            ),
            E::StructLit { fields } | E::EnumLit { fields, .. } => {
                self.construction(expr, fields.iter().map(|(_, value)| value))
            }
            E::TupleLit(values) => self.construction(expr, values),
            E::VecLit(values) => {
                let parameters = self.exprs(values);
                let can_leave = parameters.iter().any(Operation::can_leave_expression);
                let mut op = Operation::new(
                    "builtin", "Vec literal", expr.at, parameters, Effect::MAY, ALLOC,
                );
                if can_leave {
                    op.callee.operations.extend(self.analyzer.cleanup(
                        expr.ty, expr.at,
                        "an element expression can leave before construction finishes, dropping inserted elements",
                    ));
                }
                vec![op]
            }
            E::MapLit(entries) => {
                let mut op = Operation::new(
                    "builtin", "Map literal", expr.at,
                    self.exprs(entries.iter().flat_map(|(key, value)| [key, value])),
                    Effect::MAY, ALLOC,
                );
                if let Ty::Map(id) | Ty::Set(id) = expr.ty {
                    let (key, value) = map_parts(id);
                    for ty in [key, value] {
                        op.callee.operations.extend(self.analyzer.cleanup(
                            ty, expr.at, "duplicate literal keys may replace and drop earlier entries",
                        ));
                    }
                }
                vec![op]
            }
            E::Field { base, .. } | E::StrView(base) | E::Deref(base) | E::AddrOf(base)
            | E::SliceOf(base) | E::Unbox(base) | E::TagTest { value: base, .. } => self.borrowed(base),
            E::Index { base, index } => {
                let mut ops = self.borrowed(base);
                ops.extend(self.expr(index));
                ops
            }
            E::SliceRange { base, start, end } => {
                let mut ops = self.borrowed(base);
                ops.extend(self.expr(start));
                ops.extend(self.expr(end));
                ops
            }
            E::Method { method, recv } => {
                let (name, effect) = match method {
                    Builtin::StrToString => ("str.toString", Effect::MAY),
                    Builtin::StrLen => ("str.len", Effect::NONE),
                    Builtin::StringLen => ("String.len", Effect::NONE),
                    Builtin::StringClone => ("String.clone", Effect::MAY),
                    Builtin::VecLen => ("Vec.len", Effect::NONE),
                    Builtin::HeapLen => ("Heap.len", Effect::NONE),
                    Builtin::VecClone => ("Vec.clone", Effect::MAY),
                    Builtin::Trim => ("str.trim", Effect::NONE),
                    Builtin::Fields => ("str.fields", Effect::MAY),
                    Builtin::Lines => ("str.lines", Effect::MAY),
                    Builtin::IsSome => ("Option.isSome", Effect::NONE),
                    Builtin::IsNone => ("Option.isNone", Effect::NONE),
                    Builtin::MapLen => ("Map.len", Effect::NONE),
                    Builtin::MapClone => ("Map.clone", Effect::MAY),
                    Builtin::SliceToVec => ("slice.toVec", Effect::MAY),
                };
                self.builtin(name, expr, self.borrowed(recv), effect, if effect.may_alloc { ALLOC } else { NO })
            }
            E::Push { place, value } if matches!(place.ty, Ty::Heap(_)) => self.builtin(
                "Heap.push", expr, self.expr(value), Effect::MAY.join(Effect::UNKNOWN),
                "growing the heap may allocate; its stored comparator has an opaque allocation effect",
            ),
            E::Push { place, value } => self.builtin(
                format!("{}.push", place.ty), expr, self.expr(value), Effect::MAY, ALLOC,
            ),
            E::DequePush { value, front, .. } => self.builtin(
                if *front { "Deque.pushFront" } else { "Deque.pushBack" },
                expr, self.expr(value), Effect::MAY, ALLOC,
            ),
            E::DequePop { front, .. } => self.builtin(
                if *front { "Deque.popFront" } else { "Deque.popBack" }, expr, Vec::new(), Effect::NONE, NO,
            ),
            E::Reserve { place, n } => self.builtin(
                format!("{}.reserve", place.ty), expr, self.expr(n), Effect::MAY, ALLOC,
            ),
            E::SyncNew { kind, value } => self.builtin(
                kind.name(), expr, self.expr(value), Effect::MAY, ALLOC,
            ),
            E::Lock { mutex, try_ } => self.builtin(
                if *try_ { "Mutex.tryLock" } else { "Mutex.lock" },
                expr, self.borrowed(mutex), Effect::NONE, NO,
            ),
            E::SyncClone(value) => self.builtin(
                format!("{}.clone", value.ty), expr, self.borrowed(value), Effect::NONE,
                "cloning this shared handle only updates its reference count",
            ),
            E::ChanNew(cap) => self.builtin(
                "Channel", expr, self.expr(cap), Effect::MAY, ALLOC,
            ),
            E::ChanSend { chan, value } => {
                let mut parameters = self.borrowed(chan);
                parameters.extend(self.expr(value));
                self.builtin("Channel.send", expr, parameters, Effect::NONE,
                    "the channel uses its preallocated queue; a rejected value is returned to the caller")
            }
            E::ChanRecv(chan) | E::ChanClose(chan) => self.builtin(
                if matches!(expr.kind, E::ChanRecv(_)) { "Channel.recv" } else { "Channel.close" },
                expr, self.borrowed(chan), Effect::NONE, NO,
            ),
            E::Spawn { func, args, .. } => {
                let target = self.analyzer.typed.fns.get(*func);
                let parameters = args.iter().enumerate().flat_map(|(index, arg)| {
                    if target.and_then(|f| f.sinks.get(index)).copied().unwrap_or(false) || arg.ty.is_copy() {
                        self.expr(arg)
                    } else {
                        self.borrowed(arg)
                    }
                }).collect();
                let mut op = Operation::new(
                    "spawn", "spawn", expr.at, parameters, Effect::MAY,
                    "thread startup, argument storage, join records and scope bookkeeping may allocate",
                );
                op.callee.operations.push(self.analyzer.call("spawned_call", *func, expr.at, Vec::new()));
                if let Some(function) = self.analyzer.typed.fns.get(*func) {
                    op.callee.operations.extend(self.analyzer.cleanup(
                        function.ret, expr.at, "scope cleanup may drop an unclaimed thread result",
                    ));
                }
                vec![op]
            }
            E::JoinCall(value) => self.builtin(
                "Join.join", expr, self.expr(value), Effect::NONE, NO,
            ),
            E::FromUtf8(value) => self.builtin(
                "String.fromUtf8", expr, self.expr(value), Effect::MAY,
                "stdlib contract permits allocating owned UTF-8 storage even when this backend can reuse it",
            ),
            E::Some(value) | E::ErrVal(value) | E::ToDyn { value, .. } => self.expr(value),
            E::Try(value) => vec![self.transfer("try", expr.at, self.expr(value))],
            E::Coalesce { lhs, rhs } => {
                let mut ops = self.expr(lhs);
                ops.extend(self.expr(rhs));
                ops
            }
            E::Return(value) => {
                let parameters = value.as_ref().map_or_else(Vec::new, |value| self.expr(value));
                vec![self.transfer("return", expr.at, parameters)]
            }
            E::Ok(value) => {
                value.as_ref().map_or_else(Vec::new, |value| self.expr(value))
            }
            E::Take(_) => self.builtin("take", expr, Vec::new(), Effect::NONE, NO),
            E::Replace(_, value) => self.builtin("replace", expr, self.expr(value), Effect::NONE, NO),
            E::Swap(..) => self.builtin("swap", expr, Vec::new(), Effect::NONE, NO),
            E::Pop(place) if matches!(place.ty, Ty::Heap(_)) => self.builtin(
                "Heap.pop", expr, Vec::new(), Effect::UNKNOWN,
                "removing a heap element invokes its stored comparator, whose effect is opaque",
            ),
            E::Pop(_) => self.builtin("Vec.pop", expr, Vec::new(), Effect::NONE, NO),
            E::ArenaRead { op, recv, key } => {
                let name = match op { MapRead::Get => "Arena.get", MapRead::GetCopy => "Arena.getCopy", MapRead::Has => "Arena.has" };
                let mut parameters = self.borrowed(recv);
                parameters.extend(self.expr(key));
                self.builtin(name, expr, parameters, Effect::NONE, NO)
            }
            E::ArenaWrite { op, value, .. } => {
                let (name, effect) = match op {
                    ArenaWrite::Add => ("Arena.add", Effect::MAY),
                    ArenaWrite::Remove => ("Arena.remove", Effect::NONE),
                    ArenaWrite::GetMut => ("Arena.getMut", Effect::NONE),
                };
                self.builtin(name, expr, self.expr(value), effect, if effect.may_alloc { ALLOC } else { NO })
            }
            E::MapRead { op, recv, key } => {
                let name = match op {
                    MapRead::Get => "Map.get",
                    MapRead::GetCopy => "Map.getCopy",
                    MapRead::Has => "Map.has",
                };
                let mut parameters = self.borrowed(recv);
                parameters.extend(self.borrowed(key));
                self.builtin(name, expr, parameters, Effect::NONE, NO)
            }
            E::MapWrite { op, place, args } => {
                let (name, effect) = match op {
                    MapWrite::Set => ("Map.set", Effect::MAY),
                    MapWrite::Remove => ("Map.remove", Effect::NONE),
                    MapWrite::GetMut => ("Map.getMut", Effect::NONE),
                };
                let parameters = if *op == MapWrite::Set { self.exprs(args) } else { self.borrowed_exprs(args) };
                let mut operation = Operation::new(
                    "builtin", name, expr.at, parameters, effect, if effect.may_alloc { ALLOC } else { NO },
                );
                if let Ty::Map(id) | Ty::Set(id) = place.ty {
                    let (key, value) = map_parts(id);
                    if matches!(op, MapWrite::Set | MapWrite::Remove) {
                        operation.callee.operations.extend(self.analyzer.cleanup(
                            key, expr.at, "entry removal or replacement can drop a stored or duplicate key",
                        ));
                    }
                    if *op == MapWrite::Set {
                        operation.callee.operations.extend(self.analyzer.cleanup(
                            value, expr.at, "replacing a map entry drops its previous value",
                        ));
                    }
                }
                vec![operation]
            }
            E::Exit(code) => self.builtin("os.exit", expr, self.expr(code), Effect::NONE, NO),
            E::Catch { value, handler, .. } => {
                let mut ops = self.expr(value);
                if let Ty::Res(id) = value.ty {
                    ops.extend(self.analyzer.cleanup(
                        res_parts(id).1, expr.at, "a handled error may be discarded after the catch arm",
                    ));
                }
                match handler {
                    CatchHandler::Value(value) => ops.extend(self.expr(value)),
                    CatchHandler::Block(statements) => ops.extend(self.statements(statements)),
                }
                ops
            }
            E::Match { scrut, owned, arms } => {
                let mut ops = self.expr(scrut);
                if *owned {
                    ops.extend(self.analyzer.cleanup(
                        scrut.ty, expr.at, "owned match input may have unbound parts to drop",
                    ));
                }
                for arm in arms {
                    ops.extend(self.statements(&arm.body));
                    if let Some(value) = &arm.value {
                        ops.extend(self.expr(value));
                    }
                }
                ops
            }
            E::Panic { cond, parts } => {
                let mut parameters = cond.as_ref().map_or_else(Vec::new, |cond| self.expr(cond));
                parameters.extend(self.parts(parts));
                self.builtin(
                    if cond.is_some() { "assert" } else { "panic" }, expr, parameters, Effect::NONE,
                    "panic output uses the output runtime and terminates without unwinding",
                )
            }
            E::Format(parts) => self.builtin(
                "format", expr, self.parts(parts), Effect::MAY, ALLOC,
            ),
            E::MinMax { a, b, .. } => {
                let mut ops = self.expr(a);
                ops.extend(self.expr(b));
                ops
            }
            E::Call2 { method, recv, args } => {
                let (name, effect) = match method {
                    Method2::Contains => ("str.contains", Effect::NONE),
                    Method2::StartsWith => ("str.startsWith", Effect::NONE),
                    Method2::EndsWith => ("str.endsWith", Effect::NONE),
                    Method2::Find => ("str.find", Effect::NONE),
                    Method2::Split => ("str.split", Effect::MAY),
                    Method2::ToLower => ("str.toLower", Effect::MAY),
                    Method2::ToUpper => ("str.toUpper", Effect::MAY),
                    Method2::ParseInt => ("str.parseInt", Effect::NONE),
                    Method2::ParseFloat => ("str.parseFloat", Effect::UNKNOWN),
                    Method2::TotalCmp => ("float.totalCmp", Effect::NONE),
                    Method2::SeqContains => ("sequence.contains", Effect::NONE),
                    Method2::Join => ("sequence.join", Effect::MAY),
                };
                let mut parameters = self.borrowed(recv);
                parameters.extend(self.borrowed_exprs(args));
                self.builtin(name, expr, parameters, effect, if effect.may_alloc {
                    ALLOC
                } else if effect.unknown {
                    "the external C floating-point parser has no verified allocation contract"
                } else {
                    NO
                })
            }
            E::Sort(_) => self.builtin(
                "Vec.sort", expr, Vec::new(), Effect::MAY,
                "stable sorting may allocate a temporary merge buffer",
            ),
            E::SortBy { tramp, caps, .. } => {
                let mut op = Operation::new(
                    "builtin", "Vec.sortBy", expr.at, self.borrowed_exprs(caps), Effect::MAY,
                    "stable sorting may allocate a temporary merge buffer; comparator effects are separate",
                );
                if let Some(tramp) = self.analyzer.typed.tramps.get(*tramp) {
                    op.callee.operations.push(self.analyzer.call("callback", tramp.func, expr.at, Vec::new()));
                } else {
                    op.callee.operations.push(Operation::new(
                        "callback", "unresolved comparator", expr.at, Vec::new(),
                        Effect::UNKNOWN, "comparison adapter is unavailable",
                    ));
                }
                vec![op]
            }
            E::MapUpdate { place, key, init, func, caps } => {
                let mut parameters = self.expr(key);
                parameters.extend(self.expr(init));
                parameters.extend(self.borrowed_exprs(caps));
                let mut op = Operation::new(
                    "builtin", "Map.update", expr.at, parameters, Effect::MAY, ALLOC,
                );
                op.callee.operations.push(self.analyzer.call("callback", *func, expr.at, Vec::new()));
                if let Ty::Map(id) | Ty::Set(id) = place.ty {
                    let (key, value) = map_parts(id);
                    for ty in [key, value] {
                        op.callee.operations.extend(self.analyzer.cleanup(
                            ty, expr.at, "map update may discard an unused initial value or replace an entry",
                        ));
                    }
                }
                vec![op]
            }
            E::Io { op, args } => {
                let name = match op {
                    IoOp::ReadAll => "io.readAll",
                    IoOp::ReadLine => "io.readLine",
                    IoOp::ReadText => "fs.readText",
                    IoOp::WriteText => "fs.writeText",
                    IoOp::WriteNewText => "fs.writeNewText",
                    IoOp::Env => "os.env",
                    IoOp::Host => "os.host",
                    IoOp::TempDir => "fs.tempDir",
                    IoOp::RemoveFile => "fs.removeFile",
                    IoOp::RemoveDir => "fs.removeDir",
                    IoOp::SameFile => "fs.sameFile",
                    IoOp::ProcessRun => "process.run",
                };
                if *op == IoOp::Host {
                    self.builtin(name, expr, Vec::new(), Effect::NONE, "returns a static compilation-target identifier")
                } else {
                    self.builtin(name, expr, self.borrowed_exprs(args), Effect::MAY,
                        "I/O may allocate buffers or platform path conversions under the stdlib contract")
                }
            }
            E::OsArgs => self.builtin(
                "os.args", expr, Vec::new(), Effect::NONE,
                "returns the startup-owned argument view; startup allocation is outside this function",
            ),
            E::BoxNew(value) => self.builtin(
                "Box", expr, self.expr(value), Effect::MAY, ALLOC,
            ),
        }
    }
}

/// Query a source function or a concrete checked specialization. Unknown effects
/// are a successful report; a missing subject is a usage error for the CLI.
pub fn analyze(
    program: &ast::Program,
    typed: &check::HProgram,
    subject: &str,
    path: &str,
    source: &str,
) -> Result<Value, String> {
    let root = program.fns.iter().position(|f| f.name == subject)
        .or_else(|| typed.fns.iter().position(|f| f.name == subject))
        .filter(|&id| id < typed.fns.len())
        .ok_or_else(|| format!("function '{subject}' was not found in '{path}'"))?;
    let analyzer = Analyzer { program, typed };
    let functions: Vec<_> = (0..typed.fns.len()).map(|id| analyzer.function(id)).collect();
    let mut effects = vec![Effect::NONE; functions.len()];
    // The finite lattice has two independent bits per function. Each iteration
    // only sets bits, including along recursive call and destructor cycles.
    loop {
        let next: Vec<_> = functions.iter().map(|ops| effects_of(ops, &effects)).collect();
        if next == effects {
            break;
        }
        effects = next;
    }

    let mut reachable = BTreeSet::from([root]);
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        let Some(ops) = functions.get(id) else { continue };
        let mut targets = BTreeSet::new();
        for op in ops {
            op.collect_targets(&mut targets);
        }
        for target in targets {
            if target < functions.len() && reachable.insert(target) {
                pending.push(target);
            }
        }
    }
    let effect = effects[root];
    Ok(json!({
        "schema": "yavo-allocations/1",
        "subject": subject,
        "function_id": root,
        "status": effect.status(),
        "has_unknown": effect.unknown,
        "source_sha256": diag::source_hash(source),
        "file": path,
        "analysis": {
            "mode": "conservative",
            "path_sensitive": false,
            "scope": "checked function, reachable calls, callbacks, spawned work and potential implicit cleanup",
            "notes": [
                "may_alloc takes precedence over unknown; has_unknown preserves opaque-path evidence",
                "all branches and potential owned-value drops are joined without proving reachability or moved-value emptiness",
                "startup allocation and allocator-internal bookkeeping are outside the per-function query",
                "standard-output and synchronization runtime contracts are trusted; libc and OS internals have not been allocation-audited",
                "a template-name query remains unknown; concrete specializations and their callers use checked instance bodies",
                "generic templates and dynamic interface dispatch have no inferred closed-world allocation guarantee"
            ],
        },
        "operations": functions[root].iter()
            .map(|op| op.json(&effects, typed, path)).collect::<Vec<_>>(),
        "functions": reachable.iter().filter(|&&id| id != root).map(|&id| {
            json!({
                "id": id,
                "name": typed.fns[id].name,
                "status": effects[id].status(),
                "has_unknown": effects[id].unknown,
                "operations": functions[id].iter()
                    .map(|op| op.json(&effects, typed, path)).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::Effect;

    #[test]
    fn known_allocation_keeps_unknown_evidence() {
        let combined = Effect::MAY.join(Effect::UNKNOWN);
        assert_eq!(combined.status(), "may_alloc");
        assert!(combined.unknown);
        assert_eq!(Effect::NONE.join(Effect::UNKNOWN).status(), "unknown");
        assert_eq!(Effect::NONE.join(Effect::NONE).status(), "no_alloc");
    }
}
