#!/usr/bin/env bash
# Post-submission boundary audit. Original tasks and submitted sources are unchanged.
# Run with YAVO pointing at the archived 1af1f69 compiler; outputs are written here.
set -eu
audit_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
experiment_dir=$(CDPATH= cd -- "$audit_dir/.." && pwd)
: "${YAVO:?set YAVO to the archived 1af1f69 compiler}"
expected_compiler=0b1fbfc83be0bbe8b8d03a0fa7291bc6875dff9c75edf427139eebf8ec39d40c
actual_compiler=$(sha256sum "$YAVO" | cut -d ' ' -f 1)
[ "$actual_compiler" = "$expected_compiler" ] || { printf 'wrong Yavo compiler snapshot\n' >&2; exit 2; }
work=$(mktemp -d "${TMPDIR:-/tmp}/yavo-r4-boundary.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT
YAVO_ALLOC_CHECK=1 "$YAVO" build "$experiment_dir/yavo/T10.yv" -o "$work/yavo-T10" \
    > "$audit_dir/t10-leading-zero.yavo.build.stdout" 2> "$audit_dir/t10-leading-zero.yavo.build.stderr"
"${RUSTC:-$HOME/.cargo/bin/rustc}" --edition 2021 -O \
    -C "linker=${RUST_LINKER:-$HOME/.yavo/bin/rust-linker}" \
    "$experiment_dir/rust/T10.rs" -o "$work/rust-T10" \
    > "$audit_dir/t10-leading-zero.rust.build.stdout" 2> "$audit_dir/t10-leading-zero.rust.build.stderr"
for arm in yavo rust; do
    set +e
    "$work/$arm-T10" < "$audit_dir/t10-leading-zero.in" \
        > "$audit_dir/t10-leading-zero.$arm.stdout" 2> "$audit_dir/t10-leading-zero.$arm.stderr"
    run_rc=$?
    set -e
    printf '%s\n' "$run_rc" > "$audit_dir/t10-leading-zero.$arm.exit"
    if [ "$run_rc" -eq 0 ] && [ ! -s "$audit_dir/t10-leading-zero.$arm.stderr" ] && \
        cmp -s "$audit_dir/t10-leading-zero.$arm.stdout" "$audit_dir/t10-leading-zero.expected.stdout"; then
        printf '%s boundary: PASS\n' "$arm"
    else
        printf '%s boundary: FAIL (exit %s)\n' "$arm" "$run_rc"
    fi
done
