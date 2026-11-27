# Pure Yavo SHA-256 vectors, batch 50

`selfhost/digest.yv` exports `sha256(text: str) -> String`: the digest is 64
lowercase hexadecimal bytes and covers the original UTF-8 bytes, including NUL
and CRLF. The module uses existing integer operations and borrowed string byte
access. It does not add a runtime primitive or copy the whole source. Working
storage is fixed-size vectors for the 64-word message schedule, 64 round
constants and eight state words; the padded message is read virtually.

`vectors.json` fixes **46 cases** before executing the candidate: standard empty,
`abc`, 56-byte and 112-byte examples, the quick-brown-fox pair, a million `a`
bytes, padding boundaries including 55/56/63/64/65 bytes, longer blocks, all
128 ASCII bytes, UTF-8 scalar boundaries, embedded NUL and CRLF. Expected values
come from Python `hashlib.sha256`; the standard examples additionally match their
known published hexadecimal digests. The input recipes and Yavo unit entry are
bound by `freeze.json`. No expected value is read from the implementation.

The independent runner checks every frozen recipe with `hashlib`, verifies that
the Yavo fixture exactly represents those recipes, and compiles the unchanged
module and fixture with stage0 and each supplied selfhost executable. Each
frontend's generated C is compiled in Debug and Release with `YV_ALLOC_CHECK`.
Every run must have exact expected stdout, empty stderr and exit zero. Receipts
bind source, fixture, oracle, harness, runtime, toolchain lock, compilers,
generated C and executables; bound inputs are checked again after execution.
The private Zig caches are removed even when a validation step fails.

```sh
python3 selfhost/tests/digest50/verify.py \
  --stage0 compiler/target/release/yavo \
  --selfhost /absolute/path/to/stage1/yavo-selfhost \
  --selfhost /absolute/path/to/stage2/yavo-selfhost \
  --zig /absolute/path/to/locked/zig \
  --output compiler/target/ci/digest50
```

The runner records the supplied compiler identities. A caller that claims final
stage2 acceptance must bind that executable to a completed bootstrap receipt;
an arbitrary candidate run does not establish that producer relationship.
