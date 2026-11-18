#!/bin/bash
# Regenerates the `// expect-error:` / `// expect-warning:` header of a `tests/p0/errors_*.yv` file.
#
#   tools/expect_header.sh compiler/tests/p0/errors_x.yv
#
# Strips the existing header, runs `yavo check` on the body, and writes the header back with every
# line number shifted by the header's own length. Review the diff: the header records whatever the
# compiler reports now, right or wrong.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
yavo=${YAVO:-$root/compiler/target/debug/yavo}
file=$1
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
grep -v '^// expect-\(error\|warning\):' "$file" > "$tmp/body.yv" || true
"$yavo" check "$tmp/body.yv" > "$tmp/out.txt" 2>&1 || true
# Each diagnostic is `error[CODE]: message` (or `warning[...]`) followed by `  --> file:line:col`.
n=$(grep -cE '^(error|warning)\[' "$tmp/out.txt" || true)
grep -E '^(error|warning)\[|^ *--> ' "$tmp/out.txt" | paste - - \
  | sed -E 's/^(error|warning)\[([A-Z0-9]+)\].*:([0-9]+):([0-9]+)$/\1 \2 \3 \4/' \
  | awk -v n="$n" 'NF == 4 {print "// expect-" $1 ": " $2 " " ($3 + n) ":" $4}' > "$tmp/header" || true
if [ "$(wc -l < "$tmp/header")" -ne "$n" ]; then
  echo "expect_header.sh: could not parse every diagnostic; see $tmp/out.txt" >&2
  trap - EXIT
  exit 1
fi
cat "$tmp/header" "$tmp/body.yv" > "$file"
echo "$file: $n diagnostic(s)"
