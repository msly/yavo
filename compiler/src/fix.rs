//! Verified source repairs. Planning never writes a file; publication is an atomic
//! replacement of the same, unchanged input snapshot after a complete recheck.

use crate::ast::Program;
use crate::diag::{self, Diags, Edit, Fix, Severity, Span};
use crate::{check, fix_plan, lexer, parser};
use serde_json::json;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Plan {
    pub automatic: Vec<Fix>,
    pub candidates: Vec<Fix>,
    pub preview: String,
    pub blocked_automatic: bool,
}

pub struct FixError {
    pub exit_code: i32,
    pub message: String,
}

fn failure(exit_code: i32, message: impl Into<String>) -> FixError {
    FixError { exit_code, message: message.into() }
}

/// Validate offsets before touching any text. Duplicate edits are harmless; any
/// other overlap, including two different insertions at one position, is rejected.
fn normalized_edits(src: &str, edits: &[Edit]) -> Result<Vec<Edit>, String> {
    let mut edits = edits.to_vec();
    for e in &edits {
        if e.start > e.end || e.end > src.len()
            || !src.is_char_boundary(e.start) || !src.is_char_boundary(e.end)
        {
            return Err("repair contains an invalid UTF-8 byte range".into());
        }
    }
    edits.retain(|e| src[e.start..e.end] != e.replacement);
    edits.sort_by(|a, b| (a.start, a.end, &a.replacement).cmp(&(b.start, b.end, &b.replacement)));
    edits.dedup();
    for pair in edits.windows(2) {
        if pair[1].start < pair[0].end || pair[1].start == pair[0].start {
            return Err("repair contains overlapping edits".into());
        }
    }
    Ok(edits)
}

pub fn apply_edits(src: &str, edits: &[Edit]) -> Result<String, String> {
    let edits = normalized_edits(src, edits)?;
    let mut result = src.to_string();
    for edit in edits.iter().rev() {
        result.replace_range(edit.start..edit.end, &edit.replacement);
    }
    Ok(result)
}

fn all_edits(fixes: &[Fix]) -> Vec<Edit> {
    fixes.iter().flat_map(|fix| fix.edits.iter().cloned()).collect()
}

fn parse_and_check(src: &str) -> (Program, Diags) {
    let mut diags = Diags::default();
    let tokens = lexer::lex_file(src, &mut diags);
    let program = parser::parse(tokens, src, &mut diags);
    if !diags.has_errors() {
        let _ = check::check(&program, &mut diags);
    }
    diags.sort();
    (program, diags)
}

/// Type IDs live in thread-local intern tables. A repair recheck must not append
/// trial types to the original compilation's tables or affect its generated C.
fn isolated_check(src: &str) -> Result<Diags, String> {
    let owned = src.to_string();
    std::thread::Builder::new().name("yavo-fix-check".into())
        .spawn(move || parse_and_check(&owned).1)
        .map_err(|e| format!("cannot start repair validation: {e}"))?
        .join().map_err(|_| "repair validation failed internally".to_string())
}

/// Attach only independently checkable candidates or the verified automatic set.
/// Automatic repairs are atomic as a group: unresolved errors leave the file as is.
pub fn prepare(src: &str, program: &Program, diags: &mut Diags) -> Result<Plan, String> {
    diags.sort();
    if diags.items.iter().any(|d| d.code == "E0020") {
        return Ok(Plan { automatic: Vec::new(), candidates: Vec::new(), preview: src.to_string(), blocked_automatic: false });
    }
    let mut proposals = fix_plan::suggest(src, program, diags);
    proposals.sort_by(|a, b| (a.diagnostic_start, a.code, &a.fix.id)
        .cmp(&(b.diagnostic_start, b.code, &b.fix.id)));
    proposals.dedup_by(|a, b| a.code == b.code && a.diagnostic_start == b.diagnostic_start && a.fix == b.fix);
    // A malformed proposal is an implementation error, not a source edit to guess at.
    for proposal in &mut proposals {
        proposal.fix.edits = normalized_edits(src, &proposal.fix.edits)?;
        if !matches!(proposal.fix.applicability, "automatic" | "candidate") {
            return Err("repair has an unknown applicability".into());
        }
    }
    proposals.retain(|p| !p.fix.edits.is_empty());
    let mut automatic: Vec<Fix> = proposals.iter()
        .filter(|p| p.fix.applicability == "automatic").map(|p| p.fix.clone()).collect();
    let mut preview = src.to_string();
    let mut blocked_automatic = false;
    if !automatic.is_empty() {
        match apply_edits(src, &all_edits(&automatic)) {
            Ok(edited) if !isolated_check(&edited)?.has_errors() => preview = edited,
            _ => {
                automatic.clear();
                blocked_automatic = true;
            }
        }
    }
    let mut candidates = Vec::new();
    for proposal in &proposals {
        let fix = &proposal.fix;
        let verified = if fix.applicability == "automatic" {
            !blocked_automatic
        } else {
            let edited = apply_edits(src, &fix.edits)?;
            let verified = !isolated_check(&edited)?.has_errors();
            if verified && !candidates.contains(fix) { candidates.push(fix.clone()); }
            verified
        };
        if verified {
            for diagnostic in diags.items.iter_mut()
                .filter(|d| d.code == proposal.code && d.span.start == proposal.diagnostic_start)
            {
                if !diagnostic.fixes.contains(fix) { diagnostic.fixes.push(fix.clone()); }
            }
        }
    }
    Ok(Plan { automatic, candidates, preview, blocked_automatic })
}

struct Snapshot {
    bytes: Vec<u8>,
    metadata: Metadata,
}

impl Snapshot {
    fn read(path: &Path) -> Result<Self, String> {
        let metadata = fs::symlink_metadata(path).map_err(|e| format!("cannot inspect {}: {e}", path.display()))?;
        if !metadata.is_file() {
            return Err("fix requires a regular source file; symbolic links are not replaced".into());
        }
        let bytes = fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let snapshot = Self { bytes, metadata };
        snapshot.ensure_current(path)?;
        Ok(snapshot)
    }

    fn ensure_current(&self, path: &Path) -> Result<(), String> {
        let now = fs::symlink_metadata(path).map_err(|e| format!("cannot inspect source before replacement: {e}"))?;
        let mut same = now.is_file() && now.len() == self.metadata.len()
            && now.modified().ok() == self.metadata.modified().ok();
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            same &= now.dev() == self.metadata.dev() && now.ino() == self.metadata.ino()
                && now.mode() == self.metadata.mode() && now.nlink() == self.metadata.nlink();
        }
        if !same || fs::read(path).map_err(|e| format!("cannot reread source before replacement: {e}"))? != self.bytes {
            return Err("source changed while fixes were being checked; no edits were written".into());
        }
        Ok(())
    }
}

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) { let _ = fs::remove_file(&self.0); }
}

fn replace_snapshot(path: &Path, snapshot: &Snapshot, replacement: &str) -> Result<(), String> {
    snapshot.ensure_current(path)?;
    if snapshot.metadata.permissions().readonly() {
        return Err("source is read-only; no edits were written".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if snapshot.metadata.nlink() != 1 {
            return Err("fix will not replace a source file with multiple hard links".into());
        }
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let (temp, mut file) = loop {
        let name = format!(".yavo-fix-{}-{}.tmp", std::process::id(), NEXT_TEMP.fetch_add(1, Ordering::Relaxed));
        let tmp = parent.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => break (TempFile(tmp), file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cannot create replacement beside source: {e}")),
        }
    };
    file.set_permissions(snapshot.metadata.permissions()).map_err(|e| format!("cannot preserve source permissions: {e}"))?;
    file.write_all(replacement.as_bytes()).map_err(|e| format!("cannot write replacement: {e}"))?;
    file.sync_all().map_err(|e| format!("cannot sync replacement: {e}"))?;
    drop(file);
    snapshot.ensure_current(path)?;
    fs::rename(&temp.0, path).map_err(|e| format!("cannot replace source: {e}"))?;
    // Syncing the directory is best effort (not supported on every platform).
    if let Ok(dir) = File::open(parent) { let _ = dir.sync_all(); }
    Ok(())
}

pub fn run(path: &Path, dry_run: bool, json_output: bool) -> Result<i32, FixError> {
    let display = path.to_string_lossy().replace('\\', "/");
    let snapshot = Snapshot::read(path).map_err(|e| failure(2, e))?;
    let src = match std::str::from_utf8(&snapshot.bytes) {
        Ok(src) => src,
        Err(_) => {
            let mut diags = Diags::default();
            diags.error("E0020", Span { start: 0, end: 0, line: 1, col: 1 }, "source file is not valid UTF-8");
            if json_output {
                // There is no UTF-8 edit coordinate space for this input.
                use sha2::{Digest, Sha256};
                let hash = format!("{:x}", Sha256::digest(&snapshot.bytes));
                let diagnostics = diag::render_json_value(&display, "", &diags)["diagnostics"].clone();
                println!("{}", json!({"schema":"yavo-fix/1", "file":display,
                    "source_sha256":hash, "result_sha256":hash, "preview_sha256":hash,
                    "changed":false, "would_change":false, "dry_run":dry_run,
                    "automatic":[], "applied":[], "candidates":[], "blocked_automatic":false,
                    "original_diagnostics":diagnostics, "diagnostics":diagnostics,
                    "error_count":1, "warning_count":0}));
            } else {
                eprint!("{}", diag::render_human(&display, "", &diags));
            }
            return Ok(1);
        }
    };
    let (program, mut before) = parse_and_check(src);
    let plan = prepare(src, &program, &mut before).map_err(|e| failure(4, e))?;
    let would_change = plan.preview != src;
    let changed = would_change && !dry_run;
    let after = if changed {
        // This was already checked during planning. Retain fresh diagnostics for
        // the exact bytes to publish; warning locations may have moved.
        isolated_check(&plan.preview).map_err(|e| failure(4, e))?
    } else {
        before.clone()
    };
    if changed {
        if after.has_errors() { return Err(failure(4, "verified repair did not pass its final check")); }
        replace_snapshot(path, &snapshot, &plan.preview).map_err(|e| failure(4, e))?;
    }
    let result = if changed { &plan.preview } else { src };
    if json_output {
        let original = diag::render_json_value(&display, src, &before);
        let current = diag::render_json_value(&display, result, &after);
        let automatic: Vec<_> = plan.automatic.iter().map(diag::fix_json).collect();
        println!("{}", json!({
            "schema": "yavo-fix/1", "file": display,
            "source_sha256": diag::source_hash(src), "result_sha256": diag::source_hash(result),
            "preview_sha256": diag::source_hash(&plan.preview),
            "changed": changed, "would_change": would_change, "dry_run": dry_run,
            "blocked_automatic": plan.blocked_automatic,
            "automatic": automatic, "applied": if changed { automatic } else { Vec::new() },
            "candidates": plan.candidates.iter().map(diag::fix_json).collect::<Vec<_>>(),
            "original_diagnostics": original["diagnostics"], "diagnostics": current["diagnostics"],
            "error_count": after.count(Severity::Error), "warning_count": after.count(Severity::Warning),
        }));
    } else {
        if changed {
            eprintln!("fixed {} automatic repair(s) in {display}", plan.automatic.len());
        } else if would_change {
            eprintln!("{} automatic repair(s) available for {display} (dry run)", plan.automatic.len());
        } else if plan.blocked_automatic {
            eprintln!("automatic repairs were held back: the complete edited file still has errors or conflicting edits");
        }
        if !after.items.is_empty() { eprint!("{}", diag::render_human(&display, result, &after)); }
        if !plan.candidates.is_empty() {
            eprintln!("candidate repairs require a choice; use --json for complete source edits");
        }
    }
    Ok(if after.has_errors() { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(start: usize, end: usize, replacement: &str) -> Edit {
        Edit { start, end, replacement: replacement.into() }
    }

    #[test]
    fn edits_use_original_utf8_offsets_and_reject_ambiguous_ranges() {
        assert_eq!(apply_edits("甲abc", &[edit(3, 4, "A"), edit(6, 6, "!")]).unwrap(), "甲Abc!");
        assert_eq!(apply_edits("abc", &[edit(0, 1, "A"), edit(0, 1, "A")]).unwrap(), "Abc");
        assert!(apply_edits("甲abc", &[edit(1, 3, "x")]).is_err());
        assert!(apply_edits("abc", &[edit(2, 1, "x")]).is_err());
        assert!(apply_edits("abc", &[edit(0, 4, "x")]).is_err());
        assert!(apply_edits("abc", &[edit(0, 2, "x"), edit(1, 3, "y")]).is_err());
        assert!(apply_edits("abc", &[edit(0, 0, "x"), edit(0, 0, "y")]).is_err());
        assert!(apply_edits("abc", &[edit(0, 0, "x"), edit(0, 1, "y")]).is_err());
    }

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("yavo-fix-unit-{}-{}", std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

    #[test]
    fn candidate_checks_do_not_pollute_original_compilation_type_tables() {
        let source = "struct A { n: int }\nfn main() { let _a = A{n: 1} }\n";
        let mut diags = Diags::default();
        let tokens = lexer::lex_file(source, &mut diags);
        let program = parser::parse(tokens, source, &mut diags);
        let typed = check::check(&program, &mut diags).unwrap();
        let before = crate::codegen::generate(&typed, "original.yv");
        let trial = "struct B { text: String }\nfn main() { let _v: Vec[B] = [B{text: \"v\".toString()}] }\n";
        assert!(!isolated_check(trial).unwrap().has_errors());
        assert_eq!(crate::codegen::generate(&typed, "original.yv"), before);
    }

    #[test]
    fn replacement_rejects_a_stale_snapshot_and_cleans_its_temporary_file() {
        let dir = Directory::new();
        let path = dir.0.join("source.yv");
        fs::write(&path, "old").unwrap();
        let snapshot = Snapshot::read(&path).unwrap();
        fs::write(&path, "new").unwrap();
        assert!(replace_snapshot(&path, &snapshot, "fix").unwrap_err().contains("source changed"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn replacement_preserves_permissions_and_refuses_links_and_read_only_sources() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = Directory::new();
        let path = dir.0.join("source.yv");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let snapshot = Snapshot::read(&path).unwrap();
        replace_snapshot(&path, &snapshot, "new").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
        let link = dir.0.join("linked.yv");
        symlink(&path, &link).unwrap();
        assert!(Snapshot::read(&link).is_err());
        let hard = dir.0.join("hard.yv");
        fs::hard_link(&path, &hard).unwrap();
        assert!(replace_snapshot(&path, &Snapshot::read(&path).unwrap(), "bad").is_err());
        fs::remove_file(&hard).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(replace_snapshot(&path, &Snapshot::read(&path).unwrap(), "bad").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }
}
