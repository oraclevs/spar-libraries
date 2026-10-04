# spar-test

The testing library for Spar. Tests are ordinary Spar functions, grouped into
suites, run by a small CLI built on [spar-args](../argparser). Tested on spar 0.6.4.

## Installing

Add the library as a dependency, or import it by path while it is developed
next to your project:

```
spar add test path:../tester
```

Spar has no re-exports, so import each piece from its own file. As a package dependency (verified):
```spar
import pkg { Test } from "test/core/test";
import pkg { assertEqual } from "test/assertions/equality";
import pkg { runMain } from "test/cli/runner";
```

The examples in this repository import by relative path instead.

## Your first test

```spar
import { Test } from "../src/core/test.spar";
import { TestSuite } from "../src/core/suite.spar";
import { assertEqual } from "../src/assertions/equality.spar";
import { runMain } from "../src/cli/runner.spar";

fn additionWorks() -> void {
    assertEqual(actual: 2 + 2, expected: 4);
};

fn main() -> int {
    var suite: TestSuite = TestSuite(
        name: "Math",
        tests: [Test(name: "addition works", run: additionWorks)],
    );
    return runMain(suites: [suite]);
};
```

```
$ spar math_tests.spar

Math

  ✓ addition works

────────────────────────────

1 passed
1 total

Finished in 0ms
```

A test is any `fn() -> void`. It passes unless an assertion fails or it hits a
runtime error. `main` returns the exit status, so `return runMain(...)` makes
a failing run exit with 1.

A test can be disabled (`Test(name: "later", run: f, enabled: false)`); it is
reported as skipped. `tags: ["slow"]` stores free-form metadata.

## Assertions

Spar cannot compare structs or lists with `==`, and generic code cannot compare
values at all. So assertions are typed, and the type is in the name when it is
not `int`. Every assertion takes an optional `message:`.

| Import from | Assertions |
| --- | --- |
| `assertions/basic.spar` | `assertThat`, `assertTrue`, `assertFalse`, `fail` |
| `assertions/equality.spar` | `assertEqual`, `assertNotEqual` (int); `...Str`, `...Bool` variants; `assertEqualFloat` (optional `tolerance:`); list forms `assertEqualInts`, `assertEqualStrs`, `assertEqualFloats`, `assertEqualBools` |
| `assertions/comparison.spar` | `assertGreater`, `assertGreaterOrEqual`, `assertLess`, `assertLessOrEqual` (int, `bound:`); `...Float` variants |
| `assertions/collections.spar` | `assertContains`, `assertNotContains`, `assertEmpty`, `assertNotEmpty` (`List<int>`); `...Str` variants for `List<str>`; `assertLength` (any list) |
| `assertions/strings.spar` | `assertStringContains`, `assertStartsWith`, `assertEndsWith` |
| `assertions/errors.spar` | `assertFails` (see limitations) |

`assert` is a reserved name in Spar, so the plain condition check is `assertThat`.
Struct equality: compare the fields you care about.

## Suites and lifecycle hooks

```spar
TestSuite(
    name: "Counter",
    tests: [Test(name: "a", run: a), Test(name: "b", run: b)],
    beforeAll: openFixtures,
    beforeEach: reset,
    afterEach: report,
    afterAll: closeFixtures,
)
```

Order: `beforeAll`, then for each test `beforeEach`, the test, `afterEach`,
then `afterAll`. Hooks are functions with no parameters; share fixtures through
module-level `var mut` variables defined in the same file as the hooks.

A failing hook never stops the run:

- `beforeEach` fails: that test fails, its body is skipped, `afterEach` still runs.
- `afterEach` fails: the test is marked failed.
- `beforeAll` fails: the suite's tests are skipped, `afterAll` still runs, the next suite runs.
- `afterAll` fails: reported as a hook failure.

Hook failures make the run exit non-zero.

## Running tests

```
spar tests.spar                      run everything
spar tests.spar --filter parser      only suites or tests whose name contains "parser"
spar tests.spar --verbose            durations and per-suite totals
spar tests.spar --quiet              only failures and the summary
spar tests.spar --fail-fast          stop after the first failing test
spar tests.spar --list               list tests without running them
spar tests.spar --help
```

Short forms: `-f`, `-v`, `-q`, `-l`. `--no-color` turns colours off; colours are
only used when stdout is a terminal.

Filtering is case-insensitive. A match on the suite name runs the whole suite;
otherwise only tests with a matching name run.

Exit status: `0` all passed, `1` a test or hook failed, `2` bad command line.

Several files can be combined by importing their suites and passing them all to
`runMain` (see `tests/all.spar`).

## Handling failures

```
FAIL: rejects malformed integer

Expected:
  42

Received:
  "hello"
```

Each failure carries the assertion name, your message, the expected and actual
values (still typed), and the test name. Spar exposes no file or line
information, so there is no source location yet.

### Using results without the console

`runSuites(suites:, options:, reporter:)` in `core/runner.spar` returns a
`RunResult` (`suites`, `total`, `passed`, `failed`, `skipped`, `duration`),
each `SuiteResult` holds `TestResult`s, each with `status`, `duration` and
`failures`. Pass `Reporter()` (silent) to render nothing, or build a `Reporter`
with your own `onSuite`, `onRun` and `onList` functions for JSON, TAP or CI output.
`runWith(suites:, argv:, reporter:)` in `cli/runner.spar` does the same with
command-line parsing.

## Known limitations

- **`panic` cannot be caught.** A test that calls `panic` ends the whole run.
  Division by zero, unwrapping `none`/`Err` and similar errors are caught and
  reported as failures. `assertFails` only sees recoverable errors, and cannot
  distinguish a panic-free error from a failed assertion inside the function.
- **No `assertThrows` with error inspection.** The caught error is opaque apart
  from its message text.
- **Typed assertions.** No single `assertEqual` for every type; no struct or
  nested list equality.
- **No source locations, stack traces or per-test timeouts.** Not available in Spar.
- **Hook output order.** Output printed by hooks or tests appears before the
  suite's result lines, because a suite is reported when it finishes.
- **Duration is whole milliseconds**, so quick tests show `0ms`.
- Failure data crosses the try/catch boundary encoded in the error message; a
  test whose message contains the text `@@spartest@@` can confuse it.
