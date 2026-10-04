# spar-log

Structured logging for Spar: levels, typed fields, named and context loggers,
pluggable formatters and sinks. Pure Spar, no runtime changes.

## Install and import

```
spar add log path:../spar-log
```

Spar has no re-exports, so import each name from its file:

```spar
import pkg { Logger } from "log/core/logger";
import pkg { LoggerConfig } from "log/core/config";
import pkg { LogLevel } from "log/core/level";
```

## Quick start

```spar
var mut logger: Logger = Logger(name: "compiler");   // info and above, pretty, console
logger.info(message: "Application started");
logger.warn(message: "Configuration file not found");
```

Loggers are used as `var mut` because they track memory-sink output and sink errors.

```
14:18:23 INFO  compiler Application started
14:18:23 WARN  compiler Configuration file not found
```

`warn` and above go to stderr, the rest to stdout.

## Levels

`LogLevel::Trace, Debug, Info, Warn, Error, Fatal`, ranked by `levelRank`.
`LoggerConfig(level: LogLevel::Warn)` drops everything below warn before any
record is built. `fatal` is only a severity; it never exits your program.
Call `logger.log(level: ..., message: ...)` for a dynamic level.

## Structured fields

```spar
logger.info(
    message: "Build completed",
    fields: [
        strField(name: "target", value: "android"),
        intField(name: "port", value: 8080),
        floatField(name: "duration", value: 1.4),
        boolField(name: "cached", value: true)
    ],
);
// ... Build completed target=android port=8080 duration=1.4 cached=true
```

Spar has no mixed-type lists, so a `LogField` stores its value as text plus a
kind. The JSON formatter uses the kind to write numbers and booleans unquoted.

## Named and context loggers

```spar
var mut parser: Logger = logger.named(name: "parser");
var mut req: Logger = logger.withFields(fields: [strField(name: "requestId", value: "abc123")]);
```

Children copy the config; they do not share Memory-sink output or error counters.

## Formatters

| Formatter | Output |
|---|---|
| `prettyFormatter()` | `14:20:07 INFO  build Build started k=v` |
| `compactFormatter()` | `INFO build: Build started k=v` |
| `verboseFormatter()` | Rust-diagnostic style: `error: msg`, `  --> logger at time`, `   = key: value` |
| `structuredFormatter()` | `{"time":..,"level":"info","logger":..,"message":..,"fields":{..}}` |

JSON strings are escaped by `std/json`. Customize with `Formatter(showTimestamp:,
showLevel:, showLoggerName:, color:)`, or set `useCustom: true` and `custom:`
to a `fn(LogRecord) -> str`. `consoleSink()` colors automatically when stdout and stderr are terminals and stays plain when piped; `coloredConsoleSink()` always colors. File sinks are never colored (validation rejects it). Levels, logger names and field keys are colored. Pretty
timestamps are `HH:MM:SS` in UTC.

## Sinks

Each `Sink` has its own formatter and `minLevel`; a logger can have several.

```spar
LoggerConfig(sinks: [consoleSink(), fileSink(path: "./app.log")])
```

- `consoleSink()`: stdout, or stderr from `stderrFrom` (default warn).
- `fileSink(path:)`: appends one line per record, never truncates.
- `memorySink()`: records land in `logger.records` and `logger.lines`; for tests.
- `customSink(write:)`: calls your `fn(str) -> void`.

Customize with `Sink(kind: SinkKind::File, path: "x.log", formatter: compactFormatter(), minLevel: LogLevel::Warn)`.

A sink that throws (bad path, disk error) does not stop other sinks or the
program. The failure is counted in `logger.sinkErrors`, kept in
`logger.lastError`, and printed once to stderr unless `reportFailures: false`.
It never goes back through the logger, so there is no recursion.

## Configuration and validation

`LoggerConfig(level, sinks, useFilter, filter, clock, reportFailures)`. `filter`
is an extra `fn(LogRecord) -> bool`; `clock` is the timestamp source.

`validateConfig(config:)` returns a list of problems: no sinks, empty file path,
duplicate file path, colour on a file sink. `checkedLogger(name:, config:)`
returns `Result<Logger, str>` and fails on the first problem.

## Examples and tests

```
spar exec examples/basic.spar
spar exec examples/filtering.spar
spar exec examples/structured.spar
spar exec examples/file_logging.spar
spar exec examples/multiple_loggers.spar
spar exec examples/verbose.spar
bash tests/run_all.sh        # spar-test suite, type checks, runs examples
```

Tests use the official `spar-test` (`../tester`).

## Known limitations

- No global logger: module-level `var mut` is private to each importing module.
- No source location, process or thread id: Spar exposes none.
- Timestamps are UTC millisecond clock; no local time zone.
- Console sink picks stdout/stderr by level; there is no flush control.
- Context/child loggers do not share Memory-sink output with the parent.
- Not implemented: rotation, network/syslog, async, sampling.
