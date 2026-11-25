# Batch 32: independent interpolation / format oracle

Baseline: `08f19faf834f1b89ddad39fb4e77f97753cc8641`.
The final Rust reference is the frozen
`compiler/target/ci/batch32-stage0-fixed/yavo`, SHA-256
`05fde861598f401f00e748e09b366a4c3654d1e4b181adfbd3edc0820fbf82ab`.
The initial frozen reference at `compiler/target/ci/batch32-stage0/yavo`,
SHA-256 `2f5e7da56f11898371b0243e1d14cd9f57cd666266e3939f86811991dbac4814`,
is retained to document the deferred-output borrow gap found in this batch.

The batch registers **58 cases: 16 run, 36 reject, four unsupported and two
shared_unsupported**. One is a byte-identical promotion, so the manifest grows
from 490 to **547 cases**: 160 run, 28 panic, 279 reject, 64 unsupported and
16 shared_unsupported. There are **188 executable emissions**.

The tenth promotion record preserves `unsupported_interpolation`, now
`run/interpolation_basic.yv`, with unchanged source SHA-256
`2391757f4bbfef054e7fed3f45bbc645c937b72f67023ceadbc8b16f27028a60`.
The manifest SHA-256 is
`a93dacf80303412bd3816768ec309f72045dde94c21f0a4c2526fda5c397fd95`.
`oracle-index.json` maps every registered standalone source to its raw source
hash and passing independent Rust records; no source prefix changes those bytes.

## Accepted output and cleanup

All **16 final run sources pass Debug and Release: 32 final source/profile
executions**. Each run returns normally with exit 0 and exact stdout/stderr,
using `YAVO_ALLOC_CHECK=1` and `YAVO_NO_CACHE=1`. `format_eprintln` intentionally
writes one fixed stderr line; all other run sources expect empty stderr.
Receipts retain each command, source snapshot, source hash, generated C and
executable hashes, compiler/runtime/locked Zig identities, and final input
rehashing. The index selects records matching the final source bytes.

| Sources | Observable behavior |
| --- | --- |
| `interpolation_basic`, `format_scalar_types` | Original promoted bytes; int/byte/bool/str/String/payload-free enum output; non-interpolated `format(value)` and empty output. |
| `interpolation_literal_bytes`, `format_eprintln` | UTF-8, emoji, NUL, escaped literal `${`, quotes, backslashes, tabs, decimal bytes, stdout and stderr. |
| `format_nested_parentheses` | Nested format, nested string literals containing braces/colons, struct field expressions, Vec indexing, and parentheses around an already owned format result. |
| `format_evaluation_order` | print, println and format operands run once in source order; all operands finish before any outer output; print adds no newline. |
| `interpolation_copy_mutation` | Earlier int/length/byte values remain snapshots when later operands mutate their source; Copy results do not retain owner loans. |
| `format_owned_views`, `format_nested_copy_releases` | Owned locals/temporaries, struct fields, Vec elements and String views stay alive until output. Nested format owns its result and releases the original owner loan. Double slices retain the correct owner, while static double-slice bindings remain usable. |
| `format_owned_containers` | Format results move into Vec elements and struct fields, through a return and into a later assignment. |
| `format_later_failure_cleanup` | A format result in an initialized Vec/struct prefix or an earlier call argument is released when a later argument/element propagates an error. |
| `format_try_output_atomicity` | Failed later operands leave no outer stdout/stderr prefix; earlier owned temporaries are released on try propagation. |
| `format_catch_return_cleanup`, `format_catch_loop_cleanup` | Terminating catch-return, catch-continue and catch-break release earlier owned format temporaries and suppress unfinished outer output. |
| `format_lazy_fallback`, `format_user_shadow` | Untaken Optional fallbacks do not allocate or print; user functions named format override the prelude and keep their declared return type. |

The **36 rejects** comprise 15 E0001, eight E0102, eight E0020, three E0012,
one E0010 and one E0601. First-error code, line and column are recorded from
the independent Rust JSON diagnostics. The cases include ordinary interpolation
bindings/returns/function arguments, parentheses around raw interpolation,
arity, unprintable values, constant-only field defaults, format specifiers,
malformed fragments, original Unicode positions, and catch fallthrough.

The four `unsupported` sources are valid in Rust but remain subset boundaries:
zero-argument println/eprintln, float formatting, and a binding of a double
slice into owned String storage. Unknown format specifiers and direct Optional
formatting are U0001 in both compilers and are `shared_unsupported`.

## Reference borrow correction

Rust evaluates every message operand before producing the outer output. Its
original checker did not retain earlier String/view loans across later
interpolation operands. A later inout assignment could therefore release the
allocation still referenced by an earlier output snapshot. Initial check-only
probes admitted named String, String slice, owned Vec element and nested-format
outer-prefix examples. **These unsafe programs were never built or executed.**

The corrected `print_parts` holds the existing shared/mutable argument-borrow
window across the parts list and restores only its own marks. Eight registered
programs now receive E0102 for later owner replacement or take, including field,
nested-format and chained-slice roots. Independent positive runs verify that
Copy snapshots and a completed nested format are not over-locked.

`reference-correction.json` identifies the old and corrected binaries and the
observed old acceptances. Separate targeted Rust P0 receipts are copied byte
for byte as `rust-borrow-before.json` and `rust-borrow-after.json`; their source
and output snapshots are embedded in `rust-borrow-snapshots.json`. The P0
negative source initially reported only one of eleven intended E0102 errors;
the corrected checker reports all eleven at the expected positions. Its safe
positive source passes Debug and Release with allocation checking. The P0
coverage also includes Rust-only forms and does not expand the selfhost subset.

## Attempt history and diagnostic boundaries

| Evidence | Preserved result |
| --- | --- |
| `stage0-core-initial-checks.json` | 20 initial check-only probes, including four unsafe accepted owner mutations, raw/parenthesized contexts and specifier distinctions. |
| `stage0-syntax-initial-checks.json` | 20 check-only probes for scanner limits, original locations, defaults, printable types, Copy controls and nested borrow windows. |
| `stage0-diagnostic-priority.json` | Two complete diagnostic streams showing inner-fragment and outer-source error ordering. |
| `stage0-initial-safe-debug-1.json` | 14/16 initial runtime candidates passed. One function accidentally used reserved name `use`; one conditional catch handler received E0601 from the existing checker. Both failed before execution. |
| `stage0-initial-safe-debug-2.json` | The helper was renamed `accept`; separate direct catch-continue and catch-break loops preserve both intended cleanup checks. Both corrected sources pass. |
| `stage0-fixed-debug-1.json`, `stage0-fixed-release-1.json` | All 58 candidate checks and 16 native run programs pass with the corrected reference. |
| `stage0-fixed-order-debug-1.json`, `stage0-fixed-order-release-1.json` | The final order source additionally covers direct print and its missing newline; the final source passes both modes. |

Candidate and run-candidate snapshots retain the original bytes. The history
records subsequent resource-control additions and the reviewed double-slice
coverage; no failed attempt was replaced by its successful successor.

Parentheses around raw interpolation intentionally produce E0001:
`println(("${1}"))` and `format(("${1}"))` are not direct string arguments in
Rust. By contrast, `println((format("${1}")))` is legal. A user-defined format
function also receives an ordinary argument and does not inherit interpolation
privileges. `int:.2` and `:.18` produce E0001, while an unknown `:x` produces
U0001. The existing Rust scanner's `${'}'}` and `${':'}` byte-literal limits
remain E0020; this batch does not silently broaden that scanner grammar.

Rust diagnostic ordering is retained explicitly: `${1 2 @}` first reports the
extra expression at `2`; `${1 + @}` first reports `@` before its secondary EOF
error; an earlier `${@}` is reported before a later file-level `@`. The full
diagnostic streams are kept alongside the selected first-error observation.
The independent index establishes reference behavior; final selfhost checks,
fixed-point equality and generated-program gates are recorded separately.
