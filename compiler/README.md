# Yavo P0 compiler prototype

The prototype compiles a limited `.yv` subset to C17, then uses the pinned
`zig cc` toolchain (or the explicit `YAVO_CC` development override).
This is a Linux x86-64 validated prototype, not a completed three-platform
P0 acceptance or a P1 ownership implementation.

## Build and test

With Rust and its host linker configured:

```sh
cd compiler
cargo build --locked --release
./target/release/yavo toolchain install
cargo test --locked --offline -- --nocapture
cargo test --locked --offline --release -- --nocapture
```

The ordinary suite runs every `tests/p0/*.yv` twice, using generated C at
`-O0` and `-O2`. Cargo's `--release` controls the Rust compiler binary;
it is separate from the generated program's optimization setting.

The ignored `conformance_report` is exploratory: rejected inputs are skipped
without distinguishing unsupported features from false rejections. Do not
use its pass count as full conformance or as an acceptance gate.
It is superseded by `tests/conformance.rs` (below), which makes that distinction.

## Diagnostic code classes

| Class | Meaning |
| --- | --- |
| `E0xxx` | An error about the program, as listed in `yavo-tests/ERRORS.md` (the front-end codes `E0010`–`E0018` and `E0020` were added there on 2026-11-16). |
| `U0001` | Valid v0.5 syntax that this compiler does not implement yet. It says nothing about whether the program is correct. |
| `W0001` | Warning: unused variable. |
| `W0002` | Warning: a `match` arm that can never run (added to `ERRORS.md` on 2026-11-18). |
| `W0003` | Warning: an unused import (added to `ERRORS.md` on 2026-11-19). |

Valid v0.5 syntax must never be reported as a syntax error. `tests/p0/errors_unsupported.yv`
locks this in for non-`std` imports, slice types, `*p`, closures, `try`, slices, `.Case`,
`catch`, tuple literals, `match` patterns other than `some(x)`/`none`/`_` (the parser resumes
after the `match`) and module-qualified types (`fs.Error`), and the conformance test counts any
`E0020` on a spec case as wrong (slices
`x[a..b]`, tuple fields `t.0` and bounds `T + Send` are reported as `U0001`).

## Structs (first P1 feature)

Supported: `struct` declarations whose fields are integers, `bool` or other structs; struct
literals `Name{a: 1, b: 2}` (every field given exactly once, values evaluated in the order
written); reading fields, including nested ones; assigning and compound-assigning fields of a
`var` (`s.to.y += 1`); structs as parameters and return values. Such structs are copied on
assignment and when passed, as v0.5 specifies for structs whose fields are all copyable.

Checked: a `str` field is rejected with `E0301` (a view cannot be stored in a field); a struct
that contains itself by value is `E0017`; missing literal fields are `E0018`; unknown and
duplicate fields are `E0010`/`E0011`; structs cannot be compared (`E0501`) or printed directly.
`tests/p0/run_structs.yv` and `tests/p0/errors_structs.yv` cover these.

Owning fields (plan item 7): fields may be `String`, `Vec[T]`, `Map[K, V]`, `?String`, enums or
structs that own data. Such a struct is not copyable: it moves like a `String`, and is dropped
field by field in reverse declaration order (V10). Literal values are moved in. A field is read
in place (`p.name.len()`, `p.tags[0].trim()`, `for t in p.tags` iterates through a hidden
reference and keeps `p` unchanged for the loop); modifying methods work on field chains
(`p.tags.push(x)`, `p.scores.set(k, v)`, `p.items.pop()`); assigning a field drops the old
value; `take`/`replace`/`swap` move values out. Moving a non-copyable field out by value
(`let s = p.name`, `return p.name`, `p.nick ?? x`) is `E0401` (rule 8).

Not yet: methods, `Drop`, generic structs, `struct X: Interface`, field defaults, `.clone()` on
a struct. A struct literal must be written as an uppercase name immediately followed by `{`
(no space), which keeps it apart from `if c {`. Optional fields default to `none` when the
literal leaves them out. Tests: `tests/p0/run_fields.yv`.

Effect on the spec cases: none changed status. Every case that uses `struct` is also blocked
by generic types, imports or `String`; those are the next features to implement.

## Ownership: `String` and `Vec[T]`

Supported: `String`; `Vec[T]` for integer, `bool`, `str`, `String`, `Vec` and struct elements;
vector literals `[a, b]` (`[]` needs a type annotation); `s.len()`, `s.clone()`, `s.push(text)`,
`"..".toString()`, `v.len()`, `v.clone()`, `v.push(x)`; `v[i]` for copyable elements and `s[i]`
(a byte); printing and comparing `String` (`==`, `<`, ...); `x: sink T` parameters.

Rules checked:

- Assigning, returning, pushing, or passing to a `sink` parameter moves a `String`/`Vec`; using
  it afterwards is `E0402`. The analysis follows `if`/`else` (a branch that returns, breaks or
  continues does not count), and re-checks loop bodies so a value moved in one iteration cannot
  be used by the next. Assigning a new value makes the variable usable again.
- A non-`sink` parameter is a read-only borrow: moving out of it is `E0401`.
- Arguments are handled left to right: `consume(v, v.len())` is `E0402`; borrowing a variable
  and moving it in the same call is `E0102`; `v.push(v.len())` is `E0403` (no two-phase borrows).

Code generation: moving a variable copies it and zeroes the source; an empty or moved-from value
is all zeros, so drops never need runtime flags. Owned variables are dropped in reverse order at
the end of their scope and before `return`, `break` and `continue`; owned temporaries that are
only borrowed (e.g. `f().len()`) are dropped at the end of the statement. Heap growth checks
capacity arithmetic (`P-CAPACITY`) and allocation failure (`P-OOM`); `v[i]` checks bounds
(`P-INDEX`).

Not yet: `String`/`Vec` struct fields, `v[i] = x`, `Map` (see the references section for
`for x in v`, `names[0]` and `inout`).

Leak and double-drop check: `YAVO_ALLOC_CHECK=1` builds with `-DYV_ALLOC_CHECK`, where the
runtime counts live heap blocks and exits with 102 on a leak or an extra free.
`tests/alloc_check.rs` runs every `run_*.yv` that way in debug and release. A mutation test
confirmed it: deleting one generated drop was reported as a leak, and deleting one move's
zeroing made the program fail (the C library's own double-free check fired first). It does not
detect use-after-free. AddressSanitizer is not available here: zig ships no ASan runtime, and
the machine has no gcc, clang or valgrind; UBSan does work.

## Views: `str` borrowed from a `String`

Supported: passing a `String` to a `str` parameter borrows it for the call (the whitelist's
only automatic borrow); `s.trim()`, `s.fields()`, `s.lines()` return views into `s` (no
copying); functions return views with `-> str of a`, `of (a, b)` or `of static`.

Rules checked (semantic table O1, O2, E4, A3):

- A function returning a borrow-carrying type (`str`, `Vec[str]`) without `of` is `E0302`;
  returning a view of something `of` does not list (another parameter, a local, a temporary)
  is `E0303`; `of` naming a `sink` parameter is `E0304`.
- Each variable holding a view records which variables it points into. Storing a view into a
  variable that outlives its source, or a view of a temporary, is `E0305`.
- Moving or modifying a variable (`push`, assignment) while a view into it is still used later
  is `E0102`. "Later" means the view's last use (a borrow ends there); a use inside a loop
  that began after the view was created counts until the end of the loop, so a view printed
  before a `push` in the loop body is still live for the next iteration.

Implementation: before checking a function, a pre-pass numbers its statements in checking order
and records where each name is used and which loops enclose that use. A call's result points
into the arguments named by the callee's `of`. Tests: `tests/p0/run_views.yv`,
`tests/p0/errors_views.yv` (10 diagnostics, including the loop case and a cloned `Vec[str]`).
Three older P0 fixtures that returned `str` without `of` now say `of static` / `of name`.

## References, iteration and `inout`

Supported:

- `for x in v` over a `Vec[T]`: copyable elements are copied into `x`; other elements are
  borrowed as `x: &T`. Iterating over a temporary (`for x in v.clone()`) keeps it in a hidden
  variable until the loop ends.
- `v[i]` on a non-copyable element is a `&T` into `v`. A `&T` is read through automatically when
  printed, compared, used as a method receiver, indexed, or passed to a read-only parameter;
  functions may return one (`-> &String of names`).
- `x: inout T` parameters, called with `f(&v)`: inside, `x` is the caller's variable (it can be
  assigned and pushed to); in C it is a pointer.

Rules checked: changing or moving `v` while a `for` loop iterates over it is `E0102`; passing a
variable as `inout` together with any other use in the same call (`twice(&v, &v)`) is `E0102`;
an `inout` argument must be a `var` written as `&x` (`E0014`, `E0001`); storing a borrow into an
`inout` parameter is `E0306`; taking a `String` out of a `&String` is `E0404` (use `.clone()`);
moving out of a reference is `E0401`; iterating views of a temporary is `E0305`. A `&T` into
`v` is a borrow of `v`, so `v.push` while it is still used is `E0102`.

Not yet: writing the type `&inout T` or building one with `&x` (A7), `*p`. `for x in &v`,
`for i, x in v` and `getMut` are covered in the Map section below.
Tests: `tests/p0/run_refs.yv`, `tests/p0/errors_refs.yv` (9 diagnostics).

## Optional values

Supported (semantic table C5, C7, V9; stdlib contract §1):

- `?T` for any supported `T`, including `??T`, `?Vec[String]`, `Vec[?String]` and `?str`
  (which carries a borrow and needs `of` when returned); `none`; `some(x)`; a `T` used where a
  `?T` is expected is wrapped automatically (`let a: ?int = 7`, `return i`, `describe(9)`).
- `if let x = opt { } else { }`, `while let x = opt { }` (lowered to
  `loop { if let ... else { break } }`), and `match` statements on optionals with `some(x)`,
  `some(_)`, `none` and `_` arms; an arm body is a block or a single statement.
- `a ?? b`: `b` is evaluated only when `a` is `none`. The right side may be `return [value]`,
  `break` or `continue` when the `??` is a whole `let` initializer or assigned value.
- `take(&p)`, `replace(&p, v)`, `swap(&a, &b)` on variables and their fields; `v.pop()`;
  `opt.isSome()`, `opt.isNone()`. A program's own function named `take` (etc.) wins over the
  prelude one.

Ownership: matching an existing place (a variable, its fields, an element read through `&`)
borrows it. A non-copyable payload is bound as `&T` (so moving it is `E0401`, as in T-G06); a
copyable payload is copied but still counts as a borrow of the place, so changing the place
while the binding is used later is `E0102`. Matching an owned temporary (`v.pop()`,
`take(&opt)`, a call) gives its payload to the binding. `a ?? b` consumes `a` as a whole, like
an assignment would (moving out of a parameter is `E0401`). `some(_)` on a temporary drops
the payload before the arm runs.

Rules checked: a `match` missing `none` or `some` (and without `_`) is `E0602`; an arm after
its case is covered is warning `W0002` (its body is still checked); `take` of a type without a
default (structs, `str`) is `E0405`; `replace(&v, v.clone())` is `E0102` (the place is borrowed
by the first argument); overlapping `swap` places are `E0102`; `take`/`replace`/`swap` need a
writable place (`E0014`); writing through a binding into a read-only place is `E0101`; an
untyped `none` and `??` on a non-optional are `E0001`; storing a borrow into an `inout`
optional through `replace` is `E0306` (T-G24); `?? break` outside a loop is `E0015`.
Assigning to a struct field now also checks for live borrows of the variable.

Not yet (`U0001`): writing through a binding into a writable place (valid v0.5, e.g. T-A3's
`n.size += 1`), `?? return` nested inside another expression, printing or comparing
optionals. (`match` with nested patterns, as an expression, and on enums and results is
described under "Enums, results and `match`".)

Code generation: `?T` is `struct { bool has; T val; }`; all-zero is `none`, so moved-from and
`take`n optionals need no flag. Owned optionals get drop and clone helpers, declared before
any is defined because `Vec[?String]` and `?Vec[String]` call each other's. `if let` on a
temporary holds it in a C variable, moves the payload into the binding and never drops the
holder (it is `none` on the other branch). `??` evaluates its right side in a nested C block,
whose temporaries are dropped there; a jump on the right drops the statement's temporaries and
the scopes it leaves, while the normal path still drops them at the end of the statement.
Tests: `tests/p0/run_optionals.yv` (28 lines of output, also run under the heap-block check;
once also built with UBSan, which was clean) and `tests/p0/errors_optionals.yv`
(18 errors, 1 warning).

## Imports

`use std/m` and `use std/{a, b}` import the modules `io`, `fs`, `os`, `math`, `time`, `rand`,
`collections` and `sync` (stdlib contract §1). An unknown module is `E0010`, a module imported
twice `E0011`, and an import nothing uses is warning `W0003`. Using `m.f(...)` or `m.name`
without importing `m` is `E0010`. Importing anything other than `std/...` is `U0001`.

`os.exit(code)` flushes output and ends the
process without running drops (V11), and counts as never returning, so a path that ends in it
needs no `return` (V8). Every other module function, constant and type (`fs.listDir`,
`math.pi`, `Mutex`, `Heap`, ...) that is not listed under "Standard library" below is `U0001`;
using one still marks the import as used. Tests: `tests/p0/run_imports.yv`; the import diagnostics are in
`tests/p0/errors_maps.yv`.

## Maps and `&inout` references

Supported (stdlib contract §3; semantic table A1, A2, A3, A5, C4, C6):

- `Map[K, V]` with integer, `bool`, `str` or `String` keys (other keys are `E0501`) and any
  value type; `[:]` and `[k: v, ...]` literals (`[:]` needs a type from context).
- `m.get(k)` (`?&V`, borrowing the map), `m.getCopy(k)` (`?V`, `V` must be copyable, else
  `E0404`), `m.has(k)`, `m.len()`, `m.clone()`; on a writable map `m.set(k, v)` (moves key and
  value; an existing key keeps its entry and position, the new key and the old value are
  dropped), `m.remove(k)` (`?V`, owned) and `m.getMut(k)` (`?&inout V`). `String` and `str`
  keys are looked up with a `str`, so `m.get("a")` creates no `String` (C6); inserting needs an
  owned key, `m.set("a", 1)` on a `Map[String, int]` is `E0001`.
- `for k, v in m` in insertion order; `for k, v in &m` gives `v: &inout V`. Keys are read-only
  in both (`E0103`). Removing a key and inserting it again puts it at the end.
- `for i, x in v` and `for x in &v` on vectors.
- `&inout T` values, as produced by `getMut` and `in &c`: not copyable (`let q = p` moves, using
  `p` afterwards is `E0402`), writable through whatever the binding was declared with
  (`c.n = 2`, `p.push(1)`), read automatically where a copyable `T` is expected (C4).

Rules checked: writing through a `&T` (e.g. a binding from `get`) is `E0101`; while an
`&inout` into a variable is used later, any use of that variable is `E0102` (exclusive
access), and inside `for ... in &m` the body cannot use `m` at all; modifying a map while a
`get` result is still used is `E0102`; `m.set(k, m.len())` is `E0403`; storing a borrowed key
or value into an `inout` map is `E0306`; `get` on a temporary map whose result is kept is
`E0305`. `update` needs closures and is `U0001`; so are spelling the type `&inout T`, storing
`&inout` values in collections, and storing borrowed values through an `&inout`.

Code generation: the runtime keeps entries in a vector in insertion order and an
open-addressing table of entry indexes (kept at most half full, so probing ends); a removed
entry stays, not live, until the next rebuild compacts the vector. Hashing is splitmix64 for
integers and FNV-1a over the UTF-8 bytes for `str` and `String` keys, so both kinds of lookup
key agree. Each map type gets generated find/insert/remove/drop/clone functions. `&inout T` is
a `T *` and owns nothing, so it is never dropped (types now distinguish "not copyable" from
"needs a drop"). Tests: `tests/p0/run_maps.yv` (21 lines, including 100 inserts and 50 removals
that force rebuilds; heap-block check in debug and release, and UBSan once, all clean) and
`tests/p0/errors_maps.yv` (22 errors, 1 warning, together with the import checks).

## Enums, results and `match`

Supported (plan item 4; cheat sheet §2–§3; semantic table V3, V8, V12, C7):

- `enum Name { Case, Case(field: T, ...) }`, built as `Name.Case`, `Name.Case(field: value)` or
  positionally `Name.Case(value)`; where the type is known, `.Case(...)` (also inside `?T`
  and as the success type of `T ! E`). Payload values move in. Enums with owning payloads move
  and drop only the current case's payload; enums of copyable payloads are copied.
  `println("${e}")` prints `Case` or `Case(field: value, ...)`.
- `T ! E` results: `return err(e)`; a `T` returned from such a function is wrapped as success
  (section 8 whitelist); `ok(v)` where a result is expected. `() ! E` functions succeed by
  falling off the end or with `return` / `return ()`.
- `try f()` in a function with the same error type returns the error at once; values of the
  statement built so far (struct fields, vector elements, sink arguments) and every scope are
  dropped first (V3, V6).
- `f() catch e => value` and `f() catch e { ... }`; the block form must not reach its closing
  `}` (`E0601`, checked by control flow). `catch _` drops the error before the handler runs,
  `catch _e` keeps it until the handler ends (V12). On a `() ! E` result the handler may be a
  call (`check(xs) catch e => println("${e}")`). `catch e => return/break/continue` works as a
  whole `let` initializer, assigned value or statement.
- `match` with nested patterns: names, `_`, `some(p)`, `none`, `ok(p)`, `err(p)`, `.Case(p, ...)`
  and `Type.Case(...)` (module enums too: `fs.Error.Utf8(n)`); as a statement or as an
  expression (`let n = match r { ok(v) => v, err(_) => 0 }`); expression arms may also be
  `return`/`break`/`continue` or a block that leaves. Matching an existing place binds `&T` to
  non-copyable parts and borrows the place; matching an owned temporary moves the bound parts
  into the bindings and drops the rest before the arm runs. Exhaustiveness and unreachable arms
  use the usual usefulness check over these patterns (`E0602`, `W0002`).
- `panic(msg)` and `assert(cond)` / `assert(cond, msg)`, with interpolation; categories
  `P-PANIC` and `P-ASSERT` (recorded in `yavo-tests/ERRORS.md`); both count as never returning.
- Byte literals `'a'`, `'\n'` (type `byte`).
- `fs.Error` and `io.Error` are known enum types (`Utf8(offset)`, `Io(code)`, and
  `InvalidPath` for `fs`), usable once the module is imported.

Rules checked: a result used as its success type, or as a statement, without handling is
`E0001`; `try` outside a `T ! E` function or with another error type is `E0001`; an unknown
case is `E0010`; wrong payload counts are `E0012`/`E0018`; `.Case` or `err(...)` without a
known type is `E0001`; `Enum{...}` is `E0001`; enum payloads and error types cannot hold views
(`E0301`). Not yet: literal patterns (`.Num(0)`, `1 =>`), named fields in case patterns, `if
let` with patterns, comparing enums, generic enums, methods and interfaces on enums, result
fields in structs.

Code generation: an enum is `struct { uint32_t tag; union { struct {...} v0; ... } u; }` and
a result `struct { bool ok; T val; E err; }` (no `val` for `()`); all-zero is a valid value
in both (first case, or an error holding zeros), so moved-out slots stay safe to drop. Struct,
enum and result types needing drops get drop/clone helpers; every enum gets a printer. A
`match` is an `if`/`else if` chain over tag tests on the scrutinee; a block inside an
expression (a `catch` handler or `match` arm) keeps the enclosing statement's temporaries
listed, so a jump out of it drops them too. Tests: `tests/p0/run_enums.yv`,
`tests/p0/run_results.yv` (both also under the heap-block check, which exercises the early
exits) and `tests/p0/errors_enums.yv` (16 errors, 1 warning).

## Standard library: text, sequences, I/O and arguments

Supported (cheat sheet §5; stdlib contract §2, §4, §5; semantic table S5, M5):

- Prelude: `format("...")` (a new `String`, same interpolation as `println`), `min`, `max` and
  `abs` on integers (`abs(i64.min)` panics `P-OVERFLOW`), `String{}`.
- Text (`str` and `String`): `contains`, `startsWith`, `endsWith`, `find` (`?int`, byte offset),
  `split(sep)` (`Vec[str]` viewing the receiver; empty pieces kept; an empty separator gives the
  whole text), `toLower`/`toUpper` (new `String`; ASCII letters only, other bytes unchanged),
  `parseInt()` (`?int`: optional sign and decimal digits only, `none` on overflow).
- Sequences: `[]T` slices as a type (parameters, `let`, returns with `of`): a `Vec[T]` is viewed
  as a `[]T` where one is expected (section 8 whitelist); `len()`, indexing and `for x in s`
  work on slices. `v.contains(x)` and `v.join(sep)` on vectors and slices (text elements are
  compared/joined as `str`); `v.sort()` on vectors of integers, `str` or `String` (stable merge
  sort; other element types are `E0501`).
- `io.readAll()` (`String ! io.Error`) and `io.readLine()` (`?String ! io.Error`, without the
  `\n` or `\r\n`); both share one stdin buffer and flush stdout before blocking. Invalid UTF-8
  is `.Utf8(offset)` (offset into the input, or into the line for `readLine`), a system error
  `.Io(code)` (errno / `GetLastError()`).
- `fs.readText(path)` and `fs.writeText(path, text)` (`String ! fs.Error`, `() ! fs.Error`); a
  path containing NUL is `.InvalidPath`; Windows paths go through UTF-16 (`CreateFileW`).
- `os.args()` (`[]str of static`), converted once before `main`; an argument that is not valid
  UTF-8 stops the program before `main` with `YAVO-STARTUP-UTF8` and status 2. On Windows the
  arguments come from `CommandLineToArgvW`.

Not yet: `sortBy`, `update`, `Heap`/`Deque`/`Set` (closures and generics first), `x[a..b]`
slices, `s.lines()` on slices of lines, `math`/`time`/`rand`, `String.fromUtf8`, format
specifiers (`${x:.2}`), Unicode case mapping, `Ordering`.

Code generation: printing and `format` share one output abstraction (`yv_out`: a file
descriptor or a `String` to append to), so enum printers serve both. Slices are `yv_vec`
values that own nothing. Tests: `tests/p0/run_stdlib.yv`, `tests/p0/errors_stdlib.yv`
(11 errors, 1 warning), and `tests/stdlib_io.rs`, which builds programs with the heap-block
check and runs them with stdin, arguments (including a non-UTF-8 one) and a scratch directory.

## Methods

Supported (cheat sheet §2; semantic table O1, O2, V1):

- Methods are written inside a struct or enum body. The receiver is the first parameter:
  `self` (a read-only borrow), `inout self` (needs a writable variable or a field chain rooted
  at one, like `push`; the receiver is exclusively borrowed for the call) or `sink self`
  (moves the receiver). `Self` names the enclosing type.
- A function in the body without `self` is called on the type: `Counter.new("x")`.
- `-> str of self` returns a view of the receiver; the caller treats it like any other `of`
  source, so writing to the receiver while the view is alive is `E0102`. `return self.name`
  in a function returning `str` views the stored `String` (a temporary `String` cannot be
  viewed this way).
- Errors: an unknown method on a struct or enum is `E0010`; calling a method on the type or a
  `self`-less function on a value is `E0001`; `inout self` on a temporary or a `let` is
  `E0014`; moving a field out of `sink self` is still `E0401` (no partial moves, rule 8).

Code generation: a method is an ordinary C function `yv_fn_Type__name` whose first parameter
is the receiver (by value for `self`/`sink self`, a pointer for `inout self`). Tests:
`tests/p0/run_methods.yv`, `tests/p0/errors_methods.yv` (11 errors).

## User destructors (`Drop`)

Supported (cheat sheet §5 "析构"; semantic table V4, V6, V10, V11):

- `struct T: Drop { fn drop(inout self) { ... } }` (also on enums). The compiler calls `drop`
  when a complete value is dropped: at the end of its scope, on overwrite (after the new value
  is computed, V5), for `let _ = x`, for temporaries, for elements of a dropped `Vec`/`Map`/
  optional, and when a `try` leaves early. The user's `drop` runs first, then the fields in
  reverse declaration order. Moved-out values and fields of a struct literal that failed half
  way are not given to the user's `drop` (only the finished fields are dropped).
- A `Drop` type is never copyable, even if all its fields are scalars.
- Errors: `x.drop()` is `E0801`; `T: Drop` without exactly `fn drop(inout self)` (missing,
  extra parameters, a return type such as `() ! E`) is `E0802`, and so is a method named
  `drop` on a type that does not declare `Drop`; inside `drop`, overwriting, moving, `take`,
  `replace` or `swap` of the whole `self` is `E0803` (fields may still be taken or replaced).

Code generation: a `Drop` type has a hidden `uint8_t yv_live` member that literals set to 1.
Moves zero the source as before, so a moved-out value has `yv_live == 0`; its drop helper
returns immediately, otherwise it clears the flag, calls `yv_fn_T__drop` and drops the
fields. Tests: `tests/p0/run_drop.yv` (scope order, sink, `let _`, vectors, optionals, an enum
with `Drop`, a field taken inside `drop`), `tests/p0/errors_drop.yv` (8 errors, 2 warnings);
spec cases T-D1, T-GR1, T-GR2 and T-G10–T-G13 pass.

## Tuples

Supported (cheat sheet §2; semantic table E2, V10, rule 8):

- Tuple types `(A, B, ...)` in signatures, fields, collections and optionals; literals
  `(a, b)`; elements `t.0`, `t.1` (also nested: `t.0.1`); `let (a, b) = t` and
  `var (a, b) = t`, with `_` dropping that element at once.
- A tuple is copyable when all elements are, and needs dropping when any element does
  (elements are dropped in reverse order). Moving a non-copyable element out of a tuple
  variable is `E0401` (no partial moves); destructure with `let (a, b) = t` instead, which
  consumes the whole tuple.
- A tuple holding a view (`(str, int)`) carries a borrow: it may be a local, a parameter or
  a return value with `of`, and its elements keep the tuple's sources.
- Not yet: assigning to an element (`t.0 = x`), tuples as `Map` keys, comparing tuples with
  `==`/`<`, printing a whole tuple, tuple patterns in `match`/`for`.

Code generation: each tuple type is a C struct with members `f0`, `f1`, ... defined before
its first use, with drop and clone helpers when needed. The lexer never reads a number after
`.` as a float, so `t.0.1` is two element accesses. Tests: `tests/p0/run_tuples.yv`,
`tests/p0/errors_tuples.yv` (7 errors).

## Closures for `sortBy` and `update`

Supported (cheat sheet §5; semantic table O5, K3, V8; stdlib contract §3, §4):

- `v.sortBy((a, b) => a.1 > b.1)`: a stable sort; the closure says whether `a` goes before
  `b`. Any element type works. A comparison that breaks its contract (e.g. `a <= b`) gives some
  order but never touches memory outside the vector (K3).
- `m.update(key, initial, (v) => v + 1)`: the closure runs exactly once, on the stored value or
  on `initial`; the key is consumed (a duplicate key is dropped). v1 limits the value type to a
  copyable type without borrows (`E0001` otherwise).
- Closure syntax: `(a, b) => expr` or `(a, b) => { ... return x }` (a block body must return on
  every path, `E0013`); parameters may be typed (`(a: int, b: int) =>`). A block body inside the
  call parentheses keeps its newlines (the lexer now treats newlines directly inside `{` as
  statement ends even within `(...)`).
- Captures: these callbacks never keep the closure (O5 noescape), so it may read any outer
  variable, borrowed in place for the duration of the call. It cannot assign to them (`E0014`),
  move them (`E0401`), capture an `&inout` value (`U0001`), or use the collection being
  sorted/updated (`E0102`).
- Not yet: closures anywhere else (stored in variables or fields, passed to your own
  functions, `Heap.by`) are `U0001`.

Code generation: the checker lifts each closure into a function of its own (`closure.N`,
C `yv_fn_closure__N`) whose first parameters are the captured variables. `update` calls it
inline. `sortBy` goes through a generated adapter `yv_tramp_N(ctx, a, b)` that receives the
captured values as a `void *` array and is passed to the runtime's `yv_sort_ctx` (`yv_sort` is
now a wrapper around it). `total += n` with `n: &inout int` now reads `n` (C4), which T-C7
needed. Tests: `tests/p0/run_closures.yv`, `tests/p0/errors_closures.yv` (11 errors, 3
warnings).

## Spec conformance

```sh
cargo test --locked --offline --test conformance -- --nocapture
```

runs all 129 cases in `yavo-tests/MANIFEST.json`, after checking each fixture against its
`source_sha256`, and classifies every case:

| Status | Meaning |
| --- | --- |
| pass | the compiler does what the spec case expects (exact error codes and lines for check-fail; exact stdout or panic category in every listed build mode for run cases) |
| wrong | the compiler disagrees with the spec case: a bug, or a spec gap |
| unsupported | the compiler reported `U0001` |
| not_run | needs `yavo fix`, `yavo verify`, fixtures, sanitizers or other platforms |

At this commit: pass 63, wrong 0, unsupported 53, not_run 13 (P1: 50 / 0 / 26 / 7;
P2: 13 / 0 / 6 / 2; P3: 0 / 0 / 21 / 4). Closures added T-C7 and tuples T-O5p. User destructors added T-D1, T-GR1, T-GR2 and
T-G10–T-G13. The standard library step added T-G08, T-V4, T-V4p and
T-S2 (the runner now writes `fixture_hex` fixtures into a directory of their own and runs the case
there). Enums, results and owning struct fields added T-C4p, T-C6, T-C6p, T-G14, T-G15 and
T-G16. Optional values added T-C5, T-G05, T-G06, T-G07 and
T-G24; maps added T-A1, T-A1p, T-A4, T-A4p, T-C1, T-C1p, T-C7f, T-C8, T-C8f and T-GR10 (T-C7
waits for closures). T-A3/T-A3f wait for `Box`; the
cases that import `std/sync`, `std/fs` or `std/collections` now stop at those modules' types
and functions. The per-case report is written to `target/tmp/conformance-report.json`.

The test is a ratchet, not a gate: it fails only when a case listed in
`tests/conformance_baseline.txt` stops passing. After real progress, refresh the baseline with
`YAVO_UPDATE_BASELINE=1 cargo test --locked --offline --test conformance`.

## Cross-compilation

```sh
yavo build hello.yv --release --target x86_64-windows-gnu -o hello.exe
```

| Target | Output |
| --- | --- |
| `x86_64-linux-musl`, `aarch64-linux-musl` | static ELF (no dynamic loader, no host libc dependency) |
| `x86_64-windows-gnu`, `aarch64-windows-gnu` | PE `.exe` (the default output name gets `.exe`) |
| `x86_64-macos`, `aarch64-macos` | Mach-O |

Cross-compilation always uses the pinned zig; with `YAVO_CC` set, `--target` exits with 3.
`yavo run --target` only accepts a target this machine can execute; otherwise it is a usage
error (exit 2). Unknown targets are a usage error that lists the supported ones.

Verified on Linux x86-64 (`tests/targets.rs`, plus a one-off build of all 14 `run_*`/`panic_*`
programs for all six targets, 84 builds, no failures):

- every target produces the right executable format and architecture (ELF / Mach-O / PE header);
- Linux outputs are statically linked;
- the `x86_64-linux-musl` output runs here and matches the expected stdout.

Not verified: running the Windows, macOS and aarch64 outputs (no such machines or emulators
here).

Release builds are stripped (`-O2 -s`, decided 2026-11-16): `run_arith.yv` for
`x86_64-linux-musl` went from 1.3 MB to 12 KB, for `x86_64-windows-gnu` from 222 KB to 76 KB,
and Windows targets no longer leave a `.pdb` next to the output. Debug builds keep debug info
(`-O0 -g`); a Windows debug build still writes a `.pdb`.

## Four behavior fixes in this prototype commit

| Area | Behavior | Regression evidence |
| --- | --- | --- |
| Discard | `_` never enters name lookup. `let _ = expression` evaluates and discards without a binding, including unit-returning calls. `_name` remains a real binding. | `errors_discard_read`, `errors_discard_parameter`, `errors_discard_loop`, `run_discard` |
| Interpolation | All operands are evaluated left-to-right into C temporaries before the outer print writes anything. No formatted heap string is created. | `run_interpolation_order`, `panic_interpolation_no_prefix` |
| C diagnostics | Both captured C compiler streams are forwarded to stderr on success and failure; stdout remains frontend JSON in diagnostic mode. | `c_compiler_diagnostics_are_visible_on_success_and_failure` |
| Output failure | Failed writes terminate with exit 101 and a best-effort `P-SYSTEM` diagnostic. POSIX EINTR is retried; partial writes are completed; zero writes fail. Error reporting does not recurse through the failing buffered writer. | Buffered/direct stdout, flush-before-stderr, read-only stderr, both streams unusable, `/dev/full`, injected EINTR/partial/zero-write tests |

Output locations are recorded at generated print statements. When a buffered
flush fails, the report uses the most recent write location in that buffer.
If stderr is also unavailable, termination still occurs without recursive
error reporting. Normal OS signal termination such as SIGPIPE remains nonzero.

The new system-level regression runner has a 60-second child-process limit
and attempts process-tree cleanup on timeout. The earlier fixture runner is
otherwise unchanged apart from testing both C optimization modes.

## Validation record

- Before implementation changes, the added tests failed on the original
  compiler: all six new fixture files and all seven system-level test functions
  detected the corresponding behavior or missing discard handling.
- After the fixes, 22 P0 fixture files pass in both generated-C modes.
- Seven system-level regression test functions pass on Linux x86-64.
- Both Rust Debug and Release test builds pass. The validation used the
  installed Rust 1.99.1 and Zig 0.17.1; no toolchain was downloaded or changed
  during this repair.
- The seven system-level functions cover successful/failed C compiler
  diagnostics plus six runtime I/O scenarios. Runtime I/O scenarios run with
  both C optimization settings. `/dev/full` is Linux-only; the injected
  POSIX-write test is Unix-only. Windows and macOS were not run.
- No P1 ownership, Drop, or complete 129-case conformance claim is made.

On the reviewed server, Rust linking used the existing
`CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=$HOME/.yavo/bin/rust-linker`.
That machine-local wrapper was not changed by this repair and is not required
on machines with a working Rust host linker.

Historical documents/tags and the toolchain lock are unchanged. Target
selection, nested string literals in interpolation, the diagnostic-code table,
and full machine-interface alignment remain outside these four fixes.
