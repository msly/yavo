#!/usr/bin/env bash
# Usage: verify.sh yavo|rust. Optional: YAVO, RUSTC, RUST_LINKER,
# YAVO_VERIFY_BUILD_TIMEOUT / YAVO_VERIFY_RUN_TIMEOUT (positive integer seconds).
# Requires Bash and Perl with POSIX (Unix). Uses this checkout's saved sources.
set -u
set -o pipefail
arm=${1:-}
if [ "$#" -ne 1 ] || { [ "$arm" != yavo ] && [ "$arm" != rust ]; }; then
    printf 'usage: %s yavo|rust\n' "$0" >&2
    exit 2
fi
experiment_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd) || exit 2
repo_root=$(CDPATH= cd -- "$experiment_dir/../.." && pwd) || exit 2
sources="$experiment_dir/$arm"
tasks="$experiment_dir/tasks"
build_limit=${YAVO_VERIFY_BUILD_TIMEOUT:-120}
run_limit=${YAVO_VERIFY_RUN_TIMEOUT:-10}
for limit in "$build_limit" "$run_limit"; do
    case "$limit" in ''|*[!0-9]*) printf 'timeouts must be positive integer seconds\n' >&2; exit 2;; esac
    if ! [ "$limit" -ge 1 ] 2>/dev/null || ! [ "$limit" -le 86400 ] 2>/dev/null; then
        printf 'timeouts must be between 1 and 86400 seconds\n' >&2
        exit 2
    fi
done
command -v perl >/dev/null 2>&1 || { printf 'verification requires Perl with POSIX\n' >&2; exit 2; }
work=$(mktemp -d "${TMPDIR:-/tmp}/yavo-coldstart-verify.XXXXXXXX") || exit 2
trap 'rm -rf -- "$work"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
: > "$work/empty"
passed=0
failed=0
skipped=0

# An alarm kills the command's private process group, including descendants.
# Capturing streams to files preserves the actual exit code and cannot deadlock
# on a full pipe. Build/run get separate deadlines; timeout exits with 124.
run_timed() {
    perl -MPOSIX=setsid -e '
        my $seconds = shift @ARGV;
        my $pid = fork();
        if (!defined $pid) { die "cannot fork: $!\n"; }
        if (!$pid) {
            setsid() >= 0 or die "cannot create process group: $!\n";
            exec { $ARGV[0] } @ARGV;
            die "cannot execute $ARGV[0]: $!\n";
        }
        my $stop = sub { kill 9, -$pid; kill 9, $pid; waitpid($pid, 0); };
        $SIG{ALRM} = sub { $stop->(); print STDERR "verification timeout after ${seconds}s\n"; exit 124; };
        $SIG{INT} = sub { $stop->(); exit 130; };
        $SIG{TERM} = sub { $stop->(); exit 143; };
        alarm $seconds;
        waitpid($pid, 0);
        my $status = $?;
        alarm 0;
        kill 9, -$pid;
        exit(($status & 127) ? 128 + ($status & 127) : ($status >> 8));
    ' -- "$@"
}

for n in 01 02 03 04 05 06 07 08 09 10; do
    exe="$work/T$n"
    if [ "$arm" = yavo ]; then
        YAVO_ALLOC_CHECK=1 run_timed "$build_limit" "${YAVO:-$repo_root/compiler/target/release/yavo}" \
            build "$sources/T$n.yv" -o "$exe" > "$work/T$n.build.out" 2> "$work/T$n.build.err"
    else
        rust_args=(--edition 2021 -O)
        if [ -n "${RUST_LINKER:-}" ]; then rust_args+=(-C "linker=$RUST_LINKER"); fi
        run_timed "$build_limit" "${RUSTC:-rustc}" "${rust_args[@]}" "$sources/T$n.rs" -o "$exe" \
            > "$work/T$n.build.out" 2> "$work/T$n.build.err"
    fi
    rc=$?
    if [ "$rc" -ne 0 ] || [ ! -x "$exe" ]; then
        printf 'T%s BUILD FAIL (exit %s)\n' "$n" "$rc"
        cat "$work/T$n.build.out" "$work/T$n.build.err" >&2
        failed=$((failed + 1))
    else
        : > "$work/T$n.built"
    fi
done

check_case() {
    local label=$1 program=$2 input=$3 want_out=$4 want_err=$5 want_rc=$6
    shift 6
    if [ ! -f "$work/$program.built" ]; then
        printf '%s SKIP (build failed)\n' "$label"
        skipped=$((skipped + 1))
        return
    fi
    local needed
    for needed in "$input" "$want_out" "$want_err"; do
        if [ ! -r "$needed" ]; then
            printf '%s FAIL (missing fixture %s)\n' "$label" "$needed"
            failed=$((failed + 1))
            return
        fi
    done
    run_timed "$run_limit" "$work/$program" "$@" < "$input" > "$work/out" 2> "$work/err"
    local rc=$?
    if [ "$rc" -eq "$want_rc" ] && cmp -s "$work/out" "$want_out" && cmp -s "$work/err" "$want_err"; then
        printf '%s PASS\n' "$label"
        passed=$((passed + 1))
    else
        printf '%s FAIL (exit %s, expected %s)\n' "$label" "$rc" "$want_rc"
        if ! cmp -s "$work/out" "$want_out"; then printf '%s stdout differs\n' "$label" >&2; fi
        if ! cmp -s "$work/err" "$want_err"; then
            printf '%s stderr differs:\n' "$label" >&2
            cat "$work/err" >&2
        fi
        failed=$((failed + 1))
    fi
}
empty="$work/empty"
check_case T01 T01 "$tasks/T01.in" "$tasks/T01.out" "$empty" 0
check_case T02 T02 "$tasks/T02.in" "$tasks/T02.out" "$empty" 0
check_case T03 T03 "$tasks/T03.in" "$tasks/T03.out" "$empty" 0
check_case T04a T04 "$tasks/T04a.in" "$tasks/T04a.out" "$empty" 0
check_case T04b T04 "$tasks/T04b.in" "$empty" "$tasks/T04b.err" 1
check_case T04c T04 "$tasks/T04c.in" "$empty" "$tasks/T04c.err" 1
check_case T05a T05 "$tasks/T05a.in" "$tasks/T05a.out" "$empty" 0
check_case T05b T05 "$tasks/T05b.in" "$tasks/T05b.out" "$empty" 1
check_case T06enc T06 "$tasks/T06.in" "$tasks/T06.enc" "$empty" 0 encode
check_case T06dec T06 "$tasks/T06.enc" "$tasks/T06.in" "$empty" 0 decode
check_case T06bad T06 "$tasks/T06bad.in" "$empty" "$tasks/T06bad.err" 1 decode
check_case T07 T07 "$empty" "$tasks/T07.out" "$empty" 0 1994 MCMXCIV 4 IIII 0 3999 abc MMMM XLII
check_case T08 T08 "$tasks/T08.in" "$tasks/T08.out" "$empty" 0
check_case T09 T09 "$tasks/T09.in" "$tasks/T09.out" "$empty" 0
check_case T10 T10 "$tasks/T10.in" "$tasks/T10.out" "$empty" 0 3
printf 'verification: %s passed, %s failed, %s skipped\n' "$passed" "$failed" "$skipped"
if [ "$failed" -ne 0 ] || [ "$skipped" -ne 0 ]; then exit 1; fi
