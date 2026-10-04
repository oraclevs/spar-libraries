#!/usr/bin/env bash
# Unit tests, integration tests (uses the Rust TCP bridge), type-check
# of every source/example, and a run of each example. Run from anywhere.
cd "$(dirname "$0")/.." || exit 1
status=0
spar exec tests/all.spar || status=1
spar exec tests/integration.spar || status=1
for file in src/*/*.spar examples/*.spar tests/*.spar; do
    spar check "$file" >/dev/null || { echo "FAIL  check $file"; status=1; }
done
run() { # example, path, expected status line fragment
    out=$(printf "GET $2 HTTP/1.1\r\n\r\n" | spar exec "examples/$1.spar" 2>/dev/null)
    case "$out" in *"$3"*) ;; *) echo "FAIL  run $1 $2 (wanted $3)"; status=1;; esac
}
run hello / "Hello from SPAR"
run api /api/v1/users "Obi"
run middleware /hello "hello world"
run routing /api/v1/users/me "the current user"
run routing /api/v1/users/7 "user 7"
run routing /assets/a/b.css "asset a/b.css"
run static_files /public/index.html "spar-web static"
run static_files /public/../tests/all.spar "404 Not Found"
exit $status
