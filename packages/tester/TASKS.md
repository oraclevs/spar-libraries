# SPAR Test

Source of truth for progress. A box is ticked only after it ran for real.

## Milestone 1 — Foundation
- [x] inspect SPAR (spar 0.6.4): structs, fn values, imports, errors, time, exit, terminal
- [x] inspect package structure (`spar.package.spar`, `spar add`, `import pkg`)
- [x] inspect argument-parser library (`../argparser`, added as dependency `args`)
- [x] public API + core structs

## Milestone 2 — Assertions
- [x] basic: assert / assertTrue / assertFalse / fail
- [x] equality: int, float, bool, str, int/str/float/bool lists
- [x] comparison: int + float
- [x] collections: contains / empty / length
- [x] strings: contains / startsWith / endsWith
- [x] assertFails (recoverable errors only)
- [ ] assertThrows / panic recovery — BLOCKED: `panic` is not catchable in Spar 0.6.4; error object only exposes `.message`

## Milestone 3 — Runner
- [x] test + suite execution, hooks, skip, filter, fail-fast, timing, results

## Milestone 4 — Reporting
- [x] Reporter boundary, console reporter, verbose/quiet, list

## Milestone 5 — CLI
- [x] runMain using spar-args: --filter --verbose --fail-fast --list --quiet --no-color; exit code

## Milestone 6 — Testing and documentation
- [x] self-tests
- [x] examples
- [x] README
- [x] full verification

## Findings during the build
- Module-level state is private to each importing module → assertion→runner state cannot be shared; failures travel in the caught error's `message` (src/core/context.spar + codec.spar).
- `panic()` is uncatchable; runtime errors (1/0, `expect` on Err) are catchable with try/catch and `e.message` is readable.
- Enum names are global: spar-args' `ValueKind` / `Taken` clashed with ours; renamed to `AssertedKind` / `FieldRead`.
- Importing the same file in two `import` statements, or an args module both directly and via a library, redefines its enums.
- Function values are called with `arg0:, arg1:` names; fields holding functions must be copied to a variable before calling.
- `assert` is reserved → `assertThat`. No struct/list `==`, no generic `==`.
- `exit()` exists; `main` returning int sets the exit status. Time: `std/time` ms resolution.
