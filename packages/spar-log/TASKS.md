# SPAR Log

Status key: [x] done and verified, [ ] not done, [~] partial / documented limit.

## Phase 1 — Research
- [x] Language: structs, `impl`, `mut self`, enums (`E::V`), fn values (`arg0:`), lists (`append(value:)`), try/catch, Result
- [x] stdlib: `std/fs` (appendText), `std/time` (nowMillis, formatIso8601), `std/io` (eprintln), `std/terminal`, `std/json` (stringify), `std/text`
- [x] Ecosystem: spar-args (`../argparser`), spar-test (`../tester`) conventions
- [x] Gaps recorded in README "Known limitations"

## Phase 2 — Core model
- [x] LogLevel + rank (no string compare)
- [x] LogField (typed: str/int/float/bool)
- [x] LogRecord
- [x] LoggerConfig + validation

## Phase 3 — Filtering
- [x] Minimum level (logger) and per-sink minimum level
- [x] Custom predicate filter (`fn(LogRecord) -> bool`)

## Phase 4 — Formatters
- [x] Pretty, Compact, Structured (JSON via std/json)
- [x] Optional timestamp / level / logger name, colour flag

## Phase 5 — Sinks
- [x] Console (stdout, stderr for warn+)
- [x] File (append)
- [x] Memory (for tests)
- [x] Custom sink (function value)
- [x] Multiple sinks per logger
- [x] Sink failure policy (no recursion)

## Phase 6 — Logger features
- [x] Named loggers
- [x] Context loggers (`withFields`, `named`)
- [x] fatal does not exit
- [~] Global logger: deferred (module `var mut` is private per importer)

## Phase 7 — Testing (spar-test)
- [x] tests/all.spar (25 tests via spar-test), tests/run_all.sh

## Phase 8 — Examples
- [x] basic, filtering, structured, file_logging, multiple_loggers (all run)

## Phase 9 — Docs
- [x] README

## Phase 10 — Final verification
- [x] run_all.sh passes (tests, spar check, examples)

## Findings
- Stale `tester/spar.package.spar` pointed at `../argparser` (dir is `spar-args`), breaking spar-test; fixed the path and re-locked.
- `export` is only for struct/enum/type/var; `fn` is visible to importers without it.
- Params are immutable; mutation needs `mut self` methods, so loggers are `var mut`.
- ANSI escapes cannot be written in string literals; derived from `std/terminal`.
- Missing: process/thread id, source location, stdout/stderr flush, local time zone, shared module state.
