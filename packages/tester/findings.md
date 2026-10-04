
spar-test v1 is built in the tester package, written entirely in SPAR with no Rust changes. bash tests/run_all.sh passes. That covers 43 self-tests, a type check of every source, example and test file, and exit-code and CLI checks through real processes.

Public API

- Structs: Test, TestSuite, RunOptions, Reporter.
- Results: Failure, a typed Value, TestResult, SuiteResult and RunResult.
- Running: runSuites(suites, options, reporter) returns a RunResult and prints nothing itself. runMain(suites) is the CLI entry point and returns the exit status.
- Assertions: the required set, plus float, list and assertFails variants.
  - Assertions are typed: assertEqual is int, and the type is in the name for the others (assertEqualStr, assertEqualFloat, assertEqualInts, assertContainsStr).
  - Spar reserves assert, so the plain condition check is assertThat.
  - assertLength is generic.
- Reporter boundary: a Reporter is a struct of function values (onSuite, onRun, onList). consoleReporter is the default.
- Import path: a project that adds the library with spar add test path:../tester imports it as import pkg { X } from "test/<folder>/<file>". I checked this from a scratch consumer project, then deleted it.

Structure

- src/ has core/ (runner, filter, context and codec), assertions/, result/, reporter/ and cli/.
- tests/ has assertions, runner, reporting and all, plus run_all.sh.
- examples/ has basic, hooks and suites.
- README.md and TASKS.md are at the root.

Verification

- Self-tests: all 43 pass. They cover assertions, structured and typed failures, hooks and their ordering, hook failures, filter, fail-fast, skipped tests, counts, reporter output and CLI parsing.
- Exit codes: 0 on success, 1 on a failing test or hook, 2 on bad arguments.
- Not checked: colour in a real terminal. A test confirms the escape codes appear when colour is on and not when it is off.

Commands

spar tests/all.spar
spar examples/suites.spar --filter parser --verbose
spar examples/suites.spar --fail-fast --quiet
spar examples/suites.spar --list

Deferred or blocked

- assertThrows / panic recovery: blocked. panic can't be caught in 0.6.4.
- assertFails: only sees recoverable errors, such as division by zero or unwrapping none.
- Source locations and stack traces: not available.
- Timeouts, coverage, snapshots, mocking, parallel runs: left out of v1.
- JSON, TAP and JUnit reporters: not written, but the Reporter boundary allows them.

SPAR limitations found

- Shared state: module-level var mut state is private to each importing module, so assertions can't hand a failure to the runner through a global.
- Workaround: a failing assertion raises a catchable runtime error (Result.expect) whose message is the encoded Failure. The runner catches it and decodes it. This lives only in core/context.spar and core/codec.spar.
- Equality: there is no == for structs, lists or generic types, which is why the assertions are typed.
- Enum names: they are global across packages. SPAR-args already used ValueKind and Taken, so I renamed mine to AssertedKind and FieldRead.
- Import statements: importing the same file in two import statements redefines its enums.
- Function values: they are called with arg0:/arg1: argument names, and a function-valued struct field must be copied to a variable before calling it.
- Strings: string interpolation can't contain a nested string literal.
- Reserved names: assert and ok are reserved. Outer-scope helper names can clash too.

SPAR features that would help most

1. A catchable panic, or a real throw.
2. State shared across modules.
3. Struct and list equality, or a generic ==.
4. File and line information at the call site.
5. Sub-millisecond timing.

spar-args weaknesses

- It worked well and needed no fixes.
- Its enum names (ValueKind, Taken) clash with user code, which is a SPAR-wide issue.
- args() includes a leading -- when run as spar exec file -- args, but not as spar file args.

Next steps

- Add a JSON/TAP reporter.
- Add test discovery from files.
- Add assertThrows once SPAR can catch panics.
- Add a spar test task entry in the package manifest.
