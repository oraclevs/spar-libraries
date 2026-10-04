# spar-args

A command line argument parser for Spar: flags, options, positionals, nested
subcommands, typed values, validation, and generated help.

```spar
import { Cli } from "src/config/cli.spar";
import { ArgOption } from "src/config/option.spar";
import { Flag } from "src/config/flag.spar";
import { Positional } from "src/config/positional.spar";
import { Matches } from "src/result/matches.spar";
import { parseOrExit } from "src/parser/parser.spar";

fn main() -> int {
    var cli: Cli = Cli(
        name: "greet",
        positionals: [Positional(name: "who", required: true)],
        flags: [Flag(name: "shout", short: "s", long: "shout")],
    );
    var matches: Matches = parseOrExit(cli: cli);
    println(value: matches.getStr(name: "who").unwrap());
    return 0;
};
```

`parseOrExit` prints help, version and errors and exits. To handle the outcome
yourself, call `parse(cli:, argv:)`, which returns `Result<Matches, ParseError>`;
help and version requests come back as errors whose `isFailure()` is false.

## Model

- **Flag**: a switch. `SetTrue`, `SetFalse` or `Count`.
- **ArgOption**: takes a value, as `--name value`, `--name=value`, `-n value` or `-nvalue`.
- **Positional**: matched by position. `trailing` or `maxValues: 0` takes the rest.
- **SubCommand**: kept in one flat list. `path` names the ancestors, so
  `spa package registry add` is `SubCommand(name: "add", path: ["package", "registry"])`.
- **ArgGroup**: required / at-most / at-least rules over named arguments.
- `global: true` on an option or flag makes it available in every subcommand below.
- Every config field has a default, so only `name` is required.

Values are stored as text and converted on read: `getStr`, `getInt`, `getFloat`,
`getBool`, `getList`, `getInts`, `getFloats`, `flag`, `count`. `has` is true for
defaults too; `given` only for what the user typed.

## Spar gotchas this code works around

- `Command` and `Option` are built-in types, so the structs are `SubCommand` and `ArgOption`.
- `command` and `section` are reserved words and cannot be variable names.
- There are no re-exports; import from each file (see `src/lib.spar`).
- Strings cannot be indexed or nested inside `${...}`; `src/internal/text.spar` has the helpers.
- `int()` and `str()` do not work at runtime; numbers are parsed in `src/internal/value.spar`.

## Tests

```
bash tests/run_all.sh
```

Examples: `spar examples/basic.spar --help`, `examples/typed.spar`, `examples/subcommands.spar`.
