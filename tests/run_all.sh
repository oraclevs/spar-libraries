#!/usr/bin/env bash
# Runs the self-tests, type checks every source file and example, and checks the
# runner's exit status end to end. Exit status is non-zero when anything fails.
cd "$(dirname "$0")/.." || exit 1
status=0

spar tests/all.spar || status=1

for file in src/lib.spar src/*/*.spar examples/*.spar tests/*.spar; do
    spar check "$file" >/dev/null || { echo "FAIL  check $file"; status=1; }
done

# End-to-end: exit codes and CLI options through a real process.
spar examples/basic.spar >/dev/null; [ $? -eq 0 ] || { echo "FAIL  basic example should exit 0"; status=1; }
spar examples/suites.spar >/dev/null; [ $? -eq 1 ] || { echo "FAIL  suites example should exit 1"; status=1; }
spar examples/suites.spar -f math >/dev/null; [ $? -eq 1 ] || { echo "FAIL  filter math should exit 1"; status=1; }
spar examples/suites.spar -f string-nothing >/dev/null; [ $? -eq 0 ] || { echo "FAIL  empty filter should exit 0"; status=1; }
spar examples/suites.spar --bogus >/dev/null 2>&1; [ $? -eq 2 ] || { echo "FAIL  bad option should exit 2"; status=1; }
out=$(spar examples/suites.spar --list -f parser)
[ "$out" = $'Parser\n  parses long option\n  length\n  rejects malformed integer' ] || { echo "FAIL  --list output"; status=1; }
out=$(spar examples/suites.spar --fail-fast -q | grep -c '^FAIL:')
[ "$out" -eq 1 ] || { echo "FAIL  --fail-fast should report exactly one failure"; status=1; }

[ $status -eq 0 ] && echo "all checks passed"
exit $status
