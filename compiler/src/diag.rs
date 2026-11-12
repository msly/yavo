use serde_json::{json, Value};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub col: u32,
}

impl Span {
    pub fn to(self, other: Span) -> Span {
        Span {
            start: self.start,
            end: other.end.max(self.end),
            line: self.line,
            col: self.col,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn note(&mut self, note: impl Into<String>) -> &mut Self {
        self.notes.push(note.into());
        self
    }
}

#[derive(Default)]
pub struct Diags {
    pub items: Vec<Diagnostic>,
}

impl Diags {
    pub fn error(&mut self, code: &'static str, span: Span, msg: impl Into<String>) -> &mut Diagnostic {
        self.push(Severity::Error, code, span, msg.into())
    }

    pub fn warn(&mut self, code: &'static str, span: Span, msg: impl Into<String>) -> &mut Diagnostic {
        self.push(Severity::Warning, code, span, msg.into())
    }

    fn push(&mut self, severity: Severity, code: &'static str, span: Span, message: String) -> &mut Diagnostic {
        self.items.push(Diagnostic { severity, code, message, span, notes: Vec::new() });
        self.items.last_mut().unwrap()
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.items.iter().filter(|d| d.severity == severity).count()
    }

    pub fn sort(&mut self) {
        self.items.sort_by_key(|d| (d.span.start, d.severity == Severity::Warning));
    }
}

fn end_line_col(src: &str, span: Span) -> (u32, u32) {
    let (mut line, mut col) = (span.line, span.col);
    let end = span.end.min(src.len());
    if span.start <= end && src.is_char_boundary(span.start) && src.is_char_boundary(end) {
        for c in src[span.start..end].chars() {
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
    }
    (line, col)
}

fn severity_word(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

pub fn render_human(path: &str, src: &str, diags: &Diags) -> String {
    let mut out = String::new();
    for d in &diags.items {
        let _ = writeln!(out, "{}[{}]: {}", severity_word(d.severity), d.code, d.message);
        let _ = writeln!(out, "  --> {}:{}:{}", path, d.span.line, d.span.col);
        let ln = d.span.line.to_string();
        let pad = " ".repeat(ln.len());
        if let Some(text) = src.lines().nth(d.span.line.saturating_sub(1) as usize) {
            let prefix: String = text
                .chars()
                .take(d.span.col.saturating_sub(1) as usize)
                .map(|c| if c == '\t' { '\t' } else { ' ' })
                .collect();
            let (end_line, end_col) = end_line_col(src, d.span);
            let width = if end_line == d.span.line && end_col > d.span.col {
                (end_col - d.span.col) as usize
            } else {
                1
            };
            let _ = writeln!(out, "{pad} |");
            let _ = writeln!(out, "{ln} | {text}");
            let _ = writeln!(out, "{pad} | {prefix}{}", "^".repeat(width));
        }
        for n in &d.notes {
            let _ = writeln!(out, "{pad} = note: {n}");
        }
        out.push('\n');
    }
    let errors = diags.count(Severity::Error);
    let warnings = diags.count(Severity::Warning);
    if errors > 0 {
        let _ = writeln!(out, "error: {path}: {errors} error(s), {warnings} warning(s)");
    } else if warnings > 0 {
        let _ = writeln!(out, "warning: {path}: {warnings} warning(s)");
    }
    out
}

pub fn render_json(path: &str, src: &str, diags: &Diags) -> String {
    let items: Vec<Value> = diags
        .items
        .iter()
        .map(|d| {
            let (end_line, end_col) = end_line_col(src, d.span);
            json!({
                "severity": match d.severity { Severity::Error => "ERROR", Severity::Warning => "WARN" },
                "code": d.code,
                "message": d.message,
                "file": path,
                "line": d.span.line,
                "column": d.span.col,
                "end_line": end_line,
                "end_column": end_col,
                "notes": d.notes,
            })
        })
        .collect();
    json!({
        "schema": "yavo-diagnostics/p0",
        "file": path,
        "error_count": diags.count(Severity::Error),
        "warning_count": diags.count(Severity::Warning),
        "diagnostics": items,
    })
    .to_string()
}
