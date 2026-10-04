# spar-web

Source of truth for progress. [x] = done and verified by running it.

## Phase 1 — Research
- [x] Native HTTP: client only (std/http). No server/listener, no sockets. Blocking gap.
- [x] JSON: parse<T> into structs (typed errors with field path), stringify
- [x] Callables: fn values, closures capture by value, named args (arg0 for fn-typed values)
- [x] Errors: try/catch catches runtime errors; panic is not catchable; Result.expect is the throw trick
- [x] Strings: no char indexing; substring/indexOf/split/replace; no int parse (use json)
- [x] Generics: explicit type args work on free fns, not on user methods
- [x] spar-test, spar-log, spar-args inspected and added as path deps

## Phase 2 — Core types
- [x] HttpMethod, Request, Response, Status, WebError, Context
- [x] Route, RouteGroup, Middleware, App/AppConfig

## Phase 3 — Routing
- [x] Static routes (exact map)
- [x] Route params, wildcard
- [x] Precedence (static > param > wildcard, order independent)
- [x] Query params (repeated values)
- [x] 404 / 405 + Allow
- [x] Duplicate route detection at compile

## Phase 4 — Middleware
- [x] Chain composed once, global + group + route
- [x] Request logging via spar-log
- [x] CORS

## Phase 5 — Body
- [x] JSON (Result + raising variants), typed decode
- [x] urlencoded form
- [ ] multipart: deferred (no native support, no byte/str conversion)

## Phase 6 — Errors
- [x] WebError model, central error handler, recovery of runtime errors

## Phase 7 — Cookies, static files
- [x] Cookies
- [x] Static files (text types, traversal-safe)

## Phase 8 — Transport
- [x] stdio HTTP/1.1 transport (native listener missing)
- [x] dev TCP bridge (tools/bridge.sh) for integration tests

## Phase 9 — Testing / docs
- [x] Unit tests (spar-test)
- [x] Integration tests via bridge
- [x] Examples
- [x] README
- [x] Final verification (bash tests/run_all.sh)

## Deferred / blocked
- Native TCP listener: blocked (std/http is client-only). Bridge + stdio transport instead.
- Panic recovery: blocked (panic is not catchable).
- Graceful shutdown / server.stop(): blocked (no listener, no signals API).
- Binary static files: blocked (no str<->Bytes conversion).
- Typed app state via ctx.state<T>(): not possible (no downcast from Any; no explicit type args on methods). Closure capture instead.
- Request timeout, keep-alive, trusted proxies: not offered (not enforceable).
