# spar-web

A web framework for [SPAR](https://github.com/oraclevs/spar): typed routing, middleware,
JSON bodies, CORS, cookies, static files and central error handling, written in plain
SPAR on top of `std/json`, `std/fs`, `std/path`, `std/time`, and the official
`spar-log`, `spar-test` and `spar-args` packages. Tested on spar 0.6.4.

> **Read this first.** SPAR's standard library has an HTTP *client* only. There is no
> listener or socket API, so a SPAR program cannot open a port by itself. spar-web is
> transport-independent (`Server.handle(Request) -> Response`) and ships one transport,
> *stdio* (one HTTP/1.1 request on stdin, one response on stdout), plus a development
> TCP front end, `tools/bridge.sh`. See [Running a server](#running-a-server).

## Install

```
spar add web path:../spar-web
```

SPAR has no re-exports; import each name from its file:

```spar
import pkg { App } from "web/app/app";
import pkg { Route } from "web/routing/route";
```

(Inside this repository the examples use relative imports.)

## Hello world

```spar
import { Context } from "../src/http/context.spar";
import { Response } from "../src/http/response.spar";
import { Route } from "../src/routing/route.spar";
import { App } from "../src/app/app.spar";
import { serveStdio } from "../src/transport/stdio.spar";

fn home(ctx: Context) -> Response {
    return Response.text(value: "Hello from SPAR");
};

fn main() -> int {
    var app: App = App(routes: [Route.get(path: "/", handler: home)]);
    return serveStdio(server: app.compile().unwrap());
};
```

```
$ printf 'GET / HTTP/1.1\r\n\r\n' | spar exec examples/hello.spar
HTTP/1.1 200 OK
content-type: text/plain; charset=utf-8
connection: close

Hello from SPAR
```

## App

`App` describes the application. Nothing is validated or built until `compile()`:

```spar
var app: App = App(
    routes: [...],          // List<Route>
    groups: [...],          // List<RouteGroup>
    middleware: [...],      // global, outermost first
    config: AppConfig(...),
    errorHandler: myErrors, // fn(WebError) -> Response
    notFound: my404,        // fn(Context) -> Response
);
var server: Server = app.compile().unwrap();   // Result<Server, WebError>
var response: Response = server.handle(request: makeRequest(method: HttpMethod::Get, target: "/"));
```

`compile()` builds the route table and folds every middleware chain once. It reports all
configuration problems together: duplicate routes, malformed paths, misconfigured CORS.
`Server.handle` never throws and needs no socket, so most tests are plain function calls.

`AppConfig`:

| field | default | effect |
|---|---|---|
| `host`, `port` | `127.0.0.1`, `8080` | informational; used by whatever transport you wire up |
| `requestBodyLimit` | `1048576` | bodies longer than this (characters) get `413` before any handler runs |
| `developmentMode` | `false` | default error body includes `cause` |
| `logErrors`, `logger` | `false` | log 5xx failures through the given spar-log `Logger` |

There are no timeout, keep-alive or trusted-proxy settings: SPAR cannot enforce them, so they
are not offered.

## Routes and handlers

A handler is an ordinary function `fn(Context) -> Response`.

```spar
Route.get(path: "/users", handler: listUsers)
Route.post(path: "/users", handler: createUser)
Route(method: HttpMethod::Delete, path: "/users/:id", handler: removeUser)
```

Helpers exist for `get post put patch delete head options`. Each takes an optional
`middleware:` list that runs for that route only.

Paths: `/users` (exact), `/users/:id` (one segment), `/files/*rest` (one or more trailing
segments, last position only). Repeated and trailing slashes are ignored: `/api/` is `/api`.

**Precedence is independent of registration order.** Exact paths are looked up in a map.
Patterns are sorted once; at the first segment where two patterns differ, static beats
`:param` beats `*wildcard`. So `/users/me` always beats `/users/:id`, and `/a/b/:y` beats
`/a/:x/c` for `/a/b/c`. If the requested method is missing on the best path, matching falls
through to other patterns for that method.

`GET` routes also answer `HEAD` (headers only). `OPTIONS` on a known path with no `OPTIONS`
route answers `204` with `Allow`. Duplicate means same method and same shape
(`GET /u/:id` and `GET /u/:name` clash) and is a compile error.

**404 and 405.** Unknown path: `404`. Known path, wrong method: `405` with `Allow`.

## Context, request, query, headers, cookies

```spar
ctx.param(name: "id")                        // route parameter, percent-decoded
ctx.paramInt(name: "id", default: 0)
ctx.query(name: "q", default: "")            // first value
ctx.queryAll(name: "tag")                    // ?tag=a&tag=b -> ["a", "b"]
ctx.queryInt(name: "page", default: 1)       // default if absent or not an integer
ctx.header(name: "Authorization")            // case-insensitive, "" if absent
ctx.cookie(name: "session")
ctx.form()                                   // application/x-www-form-urlencoded body
ctx.body()   ctx.method()   ctx.path()   ctx.request   ctx.pattern
ctx.local(name: "user")                      // set by middleware, see below
```

`ctx.request` is a `Request` (`method`, `path`, `rawQuery`, `query`, `headers`, `body`,
`remote`). Header names are lower-cased. Percent-decoding handles printable ASCII; bytes
above 0x7f stay encoded because SPAR has no byte-to-string conversion. Multipart forms are
not supported.

## Responses

```spar
Response.text(value: "hi")                 Response.html(value: "<p>hi</p>")
Response.json(value: user, status: 201)    // any value, via std/json
Response.empty()   Response.noContent()
Response.redirect(location: "/login")      // 302; status: 301|302|303|307|308
Response.fail(error: notFound())           // hand an error to the error handler

response.withStatus(code: 202)
response.withHeader(name: "Cache-Control", value: "no-cache")
response.withCookie(cookie: Cookie(name: "sid", value: "abc", maxAge: 3600, secure: true))
response.clearCookie(name: "sid")
```

Status codes are ints with names: `statusOk`, `statusCreated`, `statusNoContent`,
`statusBadRequest`, `statusUnauthorized`, `statusForbidden`, `statusNotFound`,
`statusConflict`, `statusUnprocessableEntity`, `statusInternalServerError`, and more in
`http/status.spar`. (SPAR enums cannot carry values.)

Safety: `withHeader` strips CR, LF and NUL from names and values, so a header cannot be
split. Cookies default to `HttpOnly; SameSite=Lax; Path=/`; `SameSite=None` forces
`Secure`; `;`, `,`, spaces and line breaks are removed from names and values. `Secure` is
off by default so plain-HTTP development works: **turn it on in production**.
`Response.redirect` does not judge the target; for user-supplied targets check
`isLocalPath(location:)` first.

## JSON

```spar
struct NewUser { name: str = ""; age: int = 0; };

fn createUser(ctx: Context) -> Response {
    var parsed: Result<NewUser, WebError> = jsonBody<NewUser>(ctx: ctx);
    if parsed.isErr() {
        return Response.fail(error: parsed.unwrapErr());
    }
    return Response.json(value: parsed.unwrap(), status: 201);
};
```

`jsonBody<T>` decodes straight into a struct (or list, or `Record`) using `std/json`.
Malformed JSON is `400 Invalid JSON body`. A wrong type or missing shape is `422` with the
native message and the field name in `error.metadata`, for example
`Invalid request body: $.age: expected int, found str (type mismatch)`.
`requireJson<T>(ctx: ctx)` returns `T` and stops the handler on bad input; the framework
turns that into the same 400/422 response.

It is a function rather than `ctx.json<T>()` because SPAR cannot call a user method with
explicit type arguments.

Validation: decode first, then check, then `Response.fail(error: validationError(message:
"age must be positive", field: "age"))`. A future spar-validation package can slot in
between.

## Middleware

```spar
fn timing() -> Middleware {
    return middleware(name: "timing", run: fn(ctx: Context, next: fn(Context) -> Response) -> Response {
        var started: int = nowMillis();
        var res: Response = next(arg0: ctx);      // continue
        return res.withHeader(name: "x-ms", value: (nowMillis() - started).toString());
    });
};
```

A middleware can inspect the context, return without calling `next` (short-circuit), pass a
changed context down (`next(arg0: ctx.withLocal(name: "user", value: "ada"))`; read it with
`ctx.local`), and change the response coming back. Contexts are values, so changes only go
downstream.

Order: for `[A, B, C]` the flow is `A before, B before, C before, handler, C after, B after,
A after`. Chains are built once in `compile()`.

Layers, outside in: **global** middleware (`App.middleware`; also runs for 404 and 405),
then **group** middleware (outermost group first), then **route** middleware, then the
handler.

```spar
RouteGroup(prefix: "/admin", middleware: [auth()], routes: [Route.get(path: "/secret", handler: secret)])
```

## Route groups

```spar
RouteGroup(prefix: "/api", groups: [
    RouteGroup(prefix: "/v1", routes: [
        Route.get(path: "/users", handler: listUsers),   // GET /api/v1/users
    ]),
])
```

`RouteGroup` is recursive through `groups`. Prefixes are normalised, so `v1/`, `/v1` and
`/v1/` are the same.

## Errors

`WebError` has `kind` (`WebErrorKind`: `NotFound`, `MethodNotAllowed`, `BadRequest`,
`PayloadTooLarge`, `Validation`, `Unauthorized`, `Forbidden`, `Conflict`, `Application`,
`Internal`, `Config`), `status`, `message`, `cause` and `metadata`. Constructors:
`notFound`, `badRequest`, `validationError`, `unauthorized`, `forbidden`, `conflict`,
`appError(status:, message:)`, `internalError`.

Return `Response.fail(error: ...)` from a handler or middleware. A single `errorHandler`
renders every error, including framework ones (404, 405, 413, bad JSON, runtime failures):

```spar
fn errors(error: WebError) -> Response {
    return Response.text(value: error.status.toString() + " " + error.message, status: error.status);
};
App(errorHandler: errors)
```

The default handler returns `{"error":{"status":404,"kind":"not_found","message":"Not Found","cause":""}}`.
Headers set by outer middleware (CORS, request ids) survive on error responses.

Runtime errors that SPAR lets you catch (division by zero, unwrapping `none`, `expect`) are
caught at the boundary and become `500 Internal Server Error`. The message is hidden unless
`developmentMode` is on. `raise(error:)` stops a handler with a `WebError` from anywhere.
Configuration errors (`WebErrorKind::Config`) come from `compile()` and are never mixed
with request errors.

**Limit:** `panic` cannot be caught in SPAR 0.6.4, so there is no real panic recovery; a
`panic` ends the process.

## CORS

```spar
cors(config: CorsConfig(
    allowedOrigins: ["https://app.example.com"],
    allowedMethods: ["GET", "POST"],
    allowedHeaders: ["content-type"],   // empty = echo the preflight's request headers
    exposedHeaders: ["x-total"],
    allowCredentials: true,
    maxAge: 600,
))
```

- No `Origin` header: untouched. Allowed origin: `Access-Control-Allow-Origin` (the origin,
  or `*` when the list is `["*"]`), `Vary: Origin`, credentials and exposed headers.
- Preflight (`OPTIONS` + `Access-Control-Request-Method`) is answered with `204` without
  reaching a route. A disallowed origin or method gets `403`.
- Disallowed origins get no CORS headers on normal requests.
- `["*"]` with `allowCredentials: true` is rejected by `compile()`.

## Static files

```spar
staticFiles(prefix: "/public", dir: "public")   // a Route: GET /public/*file
```

Text types only (html, css, js, json, txt, md, csv, xml, svg). `..`, encoded `..`, hidden
(dot) files, backslashes, NUL, absolute tails and anything that normalises outside `dir`
return `404`. Binary files return `501`: SPAR has no way to turn bytes into a response body
yet. Symlinks are not resolved (no `realpath`), so do not put links to outside directories
in the public folder. Relative `dir` paths resolve against the directory of the entry
script, not the shell's working directory.

## Logging

```spar
var logger: Logger = stderrLogger();           // spar-log, all levels to stderr
App(middleware: [requestLogger(logger: logger)])
```

prints `INFO web GET /users/42 200 12ms method=GET path=... status=200 ms=12`
(warn for 4xx, error for 5xx). Any spar-log `Logger` works. When serving over stdio, stdout
carries the HTTP response, so log to stderr or a file. The framework writes nothing unless
you give it a logger (`logErrors`) or add `requestLogger`.

## Running a server

spar-web currently uses a development TCP bridge for real HTTP requests. `App.listen()` exists only to
return a clear error. What works today:

```
tools/bridge.sh --port 8080 examples/api.spar -- --verbose
curl http://127.0.0.1:8080/api/v1/users/2
```

`tools/bridge.sh` (Rust bridge, development only) accepts TCP connections and runs
`spar exec <script>` once per request with the raw request on stdin, then relays the
response and adds `Content-Length`. Consequences: one process per request, no in-memory state
shared between requests, HTTP/1.1 with `Connection: close`. It exists to test the framework
over real HTTP. A native listener in SPAR would replace it without changing application
code, because `Server.handle` is the only contract.

Application state: handlers are closures, so pass state by capturing it:
`Route.get(path: "/x", handler: fn(ctx: Context) -> Response { return show(ctx: ctx, db: db); })`.
Closures capture by value in SPAR 0.6.4, so this suits configuration and read-only services.

## CLI flags with spar-args

`examples/api.spar` reads `--verbose` with `std/process.args()`; use spar-args when a server
needs `--host`, `--port` and help text.

## Testing

```
bash tests/run_all.sh            # unit + integration + type checks + examples
spar exec tests/all.spar         # 61 unit tests (spar-test), no network
spar exec tests/integration.spar # 16 tests over real HTTP via tools/bridge.sh
```

Unit tests construct requests with `makeRequest` and call `server.handle`.

## Examples

`hello`, `api` (groups, typed JSON, auth, CORS, logging, error handling), `middleware`,
`routing` (groups, precedence, wildcard), `static_files`. Run any with
`printf 'GET /path HTTP/1.1\r\n\r\n' | spar exec examples/<name>.spar`.

## Known limitations

- No native listener: needs the stdio transport and the bridge above.
- `panic` cannot be recovered; only catchable runtime errors are.
- No multipart, streaming bodies, WebSockets, compression, sessions, templates.
- Static files: text only; no symlink resolution; no caching headers.
- `\r` and `\0` are not string escapes in SPAR; spar-web gets them from the JSON decoder.
- Percent-decoding is ASCII only.
- `Content-Length` is not emitted by stdio mode (no byte length of a string); the bridge adds it.
- Typed application state is by closure capture; no `ctx.state<T>()`.
- Enum names are global across packages; spar-web uses `HttpMethod`, `WebErrorKind`,
  `MatchKind`.
