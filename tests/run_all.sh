#!/usr/bin/env bash
# Runs the spar-test suite and type-checks sources and examples.
cd "$(dirname "$0")/.." || exit 1
status=0
spar exec tests/all.spar || status=1
for file in src/lib.spar src/*/*.spar examples/*.spar; do
    spar check "$file" >/dev/null || { echo "FAIL  check $file"; status=1; }
done
for file in examples/*.spar; do
    spar exec "$file" >/dev/null 2>&1 || { echo "FAIL  run $file"; status=1; }
done
exit $status
