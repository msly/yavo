# Batch 50 supplemental legacy CLI contract

This eight-case supplement preserves two already valid batch-49 command forms.
It does not change the primary 98-case JSON contract, its frozen oracle, source
fixtures, or expectations. It clarifies the new option parser at the boundary
between legacy positional arguments and the new four-argument JSON form.

1. `check INPUT` with exactly one argument after `check` keeps the input spelling
   verbatim, including leading `-` and `--`. Only the newly reserved exact
   arguments `--json` and `--json-diagnostics` retain their missing-input usage
   error. The frozen successful paths are `--source.yv` and `-legacy.yv`.
2. The existing `emit-c INPUT -o OUTPUT` positional form accepts `--json` and
   `--json-diagnostics` as literal output names. Both successful products must
   equal the frozen batch-49 generated C for the unchanged minimal source.
3. `emit-c` with an additional `--json` or `--json-diagnostics` argument, either
   before the input or after the output, is an argument error: exit 2, empty
   stdout, a newline-terminated usage diagnostic on stderr, and no output file.
4. Successful check/emit commands exit 0 with empty stdout and stderr. Source,
   cache, temporary sentinels and unrelated files remain unchanged. Successful
   emit may create only the specified output, with exact frozen bytes.
5. The eight scenarios apply on Linux, macOS and Windows. They are reported
   separately from the original JSON/text inventories; each actual stage1 and
   actual stage2 compiler must pass all eight.

The source and expected behavior are authored independently of the compatibility
fix. The batch-49 immutable compiler provides the generated-C reference and
baseline behavior. The original JSON candidate was already run and exposed the
leading-dash regression before this supplement was created; this fact is recorded
in `freeze.json`. The corrected implementation is not run before this supplement
is frozen. `preflight.json` records baseline observations, not release acceptance.
