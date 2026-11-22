#!/usr/bin/env bash
# Round-4 independent verification. Usage: verify.sh yavo|rust [SOURCE_DIR]
# SOURCE_DIR defaults to this experiment's yavo/ or rust/ directory.
# Optional env: YAVO, RUSTC, RUST_LINKER, YAVO_VERIFY_BUILD_TIMEOUT, YAVO_VERIFY_RUN_TIMEOUT.
set -u
set -o pipefail
arm=${1:-}
if [ "$arm" != yavo ] && [ "$arm" != rust ]; then
    printf 'usage: %s yavo|rust [SOURCE_DIR]\n' "$0" >&2
    exit 2
fi
experiment_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd) || exit 2
repo_root=$(CDPATH= cd -- "$experiment_dir/../.." && pwd) || exit 2
sources=${2:-$experiment_dir/$arm}
tasks="$experiment_dir/tasks"
build_limit=${YAVO_VERIFY_BUILD_TIMEOUT:-120}
run_limit=${YAVO_VERIFY_RUN_TIMEOUT:-20}
command -v perl >/dev/null 2>&1 || { printf 'verification requires Perl\n' >&2; exit 2; }
work=$(mktemp -d "${TMPDIR:-/tmp}/yavo-r4-verify.XXXXXXXX") || exit 2
trap 'rm -rf -- "$work"' EXIT
: > "$work/empty"
passed=0; failed=0; skipped=0; req_fail=0

run_timed() {
    perl -MPOSIX=setsid -e '
        my $seconds = shift @ARGV;
        my $pid = fork();
        if (!defined $pid) { die "cannot fork: $!\n"; }
        if (!$pid) { setsid(); exec { $ARGV[0] } @ARGV; die "cannot execute $ARGV[0]: $!\n"; }
        $SIG{ALRM} = sub { kill 9, -$pid; kill 9, $pid; waitpid($pid, 0); print STDERR "timeout after ${seconds}s\n"; exit 124; };
        alarm $seconds; waitpid($pid, 0); my $status = $?; alarm 0; kill 9, -$pid;
        exit(($status & 127) ? 128 + ($status & 127) : ($status >> 8));
    ' -- "$@"
}

ext=yv; [ "$arm" = rust ] && ext=rs
for n in 01 02 03 04 05 06 07 08 09 10 11 12; do
    if [ -d "$sources/T$n" ]; then entry="$sources/T$n/main.$ext"; else entry="$sources/T$n.$ext"; fi
    exe="$work/T$n"
    if [ ! -f "$entry" ]; then printf 'T%s MISSING %s\n' "$n" "$entry"; failed=$((failed + 1)); continue; fi
    if [ "$arm" = yavo ]; then
        YAVO_ALLOC_CHECK=1 run_timed "$build_limit" "${YAVO:-$repo_root/compiler/target/release/yavo}" \
            build "$entry" -o "$exe" > "$work/T$n.b.out" 2> "$work/T$n.b.err"
    else
        rust_args=(--edition 2021 -O)
        [ -n "${RUST_LINKER:-}" ] && rust_args+=(-C "linker=$RUST_LINKER")
        run_timed "$build_limit" "${RUSTC:-rustc}" "${rust_args[@]}" "$entry" -o "$exe" > "$work/T$n.b.out" 2> "$work/T$n.b.err"
    fi
    rc=$?
    if [ "$rc" -ne 0 ] || [ ! -x "$exe" ]; then
        printf 'T%s BUILD FAIL (exit %s)\n' "$n" "$rc"; cat "$work/T$n.b.out" "$work/T$n.b.err" >&2
        failed=$((failed + 1))
    else
        : > "$work/T$n.built"
    fi
done

# Code requirements stated in TASKS.md (structure only; behaviour is checked below).
count_files() { find "$sources/$1" -name "*.$ext" -type f 2>/dev/null | wc -l; }
req() { if eval "$2"; then printf 'REQ %s ok\n' "$1"; else printf 'REQ %s FAIL\n' "$1"; req_fail=$((req_fail + 1)); fi; }
req "T04 >=2 files" '[ "$(count_files T04)" -ge 2 ]'
req "T05 >=3 files" '[ "$(count_files T05)" -ge 3 ]'
if [ "$arm" = yavo ]; then thread_re='spawn '; else thread_re='thread::(spawn|scope)|\.spawn\('; fi
req "T06 threads" 'grep -Eq "$thread_re" "$sources/T06.$ext"'
req "T10 threads" 'grep -Eq "$thread_re" "$sources/T10.$ext"'

check() {
    local label=$1 prog=$2 input=$3 want_out=$4 want_err=$5 want_rc=$6; shift 6
    if [ ! -f "$work/$prog.built" ]; then printf '%s SKIP\n' "$label"; skipped=$((skipped + 1)); return; fi
    (cd "$experiment_dir" && run_timed "$run_limit" "$work/$prog" "$@" < "$input" > "$work/out" 2> "$work/err")
    local rc=$?
    if [ "$rc" -eq "$want_rc" ] && cmp -s "$work/out" "$want_out" && cmp -s "$work/err" "$want_err"; then
        printf '%s PASS\n' "$label"; passed=$((passed + 1))
    else
        printf '%s FAIL (exit %s, expected %s)\n' "$label" "$rc" "$want_rc"
        cmp -s "$work/out" "$want_out" || { printf '%s stdout differs:\n' "$label" >&2; diff "$want_out" "$work/out" | head -8 >&2; }
        cmp -s "$work/err" "$want_err" || { printf '%s stderr differs:\n' "$label" >&2; head -5 "$work/err" >&2; }
        failed=$((failed + 1))
    fi
}
t=$tasks; z=$work/empty
printf 'error: cannot read tasks/nosuch.txt\n' > "$work/T11x.err"
check T01 T01 $t/T01.in $t/T01.out $z 0
check T02 T02 $t/T02.in $t/T02.out $z 0
check T03 T03 $t/T03.in $t/T03.out $z 0
check T04 T04 $t/T04.in $t/T04.out $z 0 server.port db.url name db.dsn loop.a db.missing nosuch.key bad.ref
check T04bad T04 $t/T04bad.in $z $t/T04bad.err 2 x
check T05a T05 $t/T05a.in $t/T05a.out $z 0 10
check T05b T05 $t/T05b.in $z $t/T05b.err 1 10
check T05c T05 $t/T05c.in $z $t/T05c.err 1 10
check T05d T05 $t/T05d.in $z $t/T05d.err 1 6
check T06 T06 $t/T06.in $t/T06.out $z 0
check T07 T07 $t/T07.in $t/T07.out $t/T07.err 0
check T08 T08 $t/T08.in $t/T08.out $z 0
check T09 T09 $t/T09.in $t/T09.out $z 0
check T10 T10 $t/T10.in $t/T10.out $z 0
check T11ab T11 $z $t/T11ab.out $z 1 tasks/T11a.txt tasks/T11b.txt
check T11aa T11 $z $t/T11aa.out $z 0 tasks/T11a.txt tasks/T11a.txt
check T11x T11 $z $z $work/T11x.err 2 tasks/nosuch.txt tasks/T11a.txt
check T12 T12 $t/T12.in $t/T12.out $z 0
printf 'verification: %s passed, %s failed, %s skipped; requirement failures: %s\n' "$passed" "$failed" "$skipped" "$req_fail"
[ "$failed" -eq 0 ] && [ "$skipped" -eq 0 ] && [ "$req_fail" -eq 0 ]
