#!/usr/bin/env bash
# Runs every suite and the examples' type checks. Exit status is non-zero when
# anything fails.
cd "$(dirname "$0")/.." || exit 1
status=0
for suite in tests/options.spar tests/positionals.spar tests/validation.spar \
             tests/subcommands.spar tests/help.spar tests/config.spar; do
    spar "$suite" || status=1
done
for file in src/lib.spar examples/*.spar; do
    spar check "$file" >/dev/null || { echo "FAIL  check $file"; status=1; }
done
exit $status
