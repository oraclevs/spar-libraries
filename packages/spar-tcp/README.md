# TCP package

This package provides TCP listeners and clients through Spar native ABI 1. The transport is implemented in Rust and exposed through `src/lib.spar`. Build the artifact for your host with `bash build-package.sh` (or `./build-package.ps1` on Windows MSVC), then add the package with `spar add tcp path:/absolute/path/to/spar-tcp`.

```spar
import pkg { listen, accept, read, writeText, close, closeListener } from "tcp";

fn main() -> int {
    var listener = listen(address: "127.0.0.1:8080");
    var connection = accept(listener: listener);
    var request: Bytes = read(connection: connection, maxBytes: 4096);
    writeText(connection: connection, text: "hello");
    close(connection: connection);
    closeListener(listener: listener);
    return 0;
};
```

For configured listeners, create `socketIpv4()` or `socketIpv6()`, set options such as `socketSetReuseAddress`, `socketSetIpv6Only`, or buffer sizes, call `socketBind`, then `socketListen(handle:, backlog:)`. The simpler listener API has `listen`, `listenTls`, `localAddress`, `accept`, `acceptTimeout`, and `closeListener`. `accept` waits for a connection; `acceptTimeout` fails after the requested number of milliseconds. Closing a listener interrupts a waiting `accept`. A TLS listener takes PEM certificate and private key paths. A failed TLS handshake is skipped so the listener can accept the next client.

The client API has `connect(address:, timeoutMillis:)` and `connectTls(address:, serverName:, caCertPath:, timeoutMillis:)`. TLS clients validate the server certificate against the supplied PEM CA file and check `serverName`. Both client and accepted connections support address queries, read and write timeouts, `TCP_NODELAY`, keepalive, socket buffer sizing, `read`, `readExact`, `peek`, partial `writePartial`, full `writeBytes`/`writeAll`, `writeText`, `shutdownRead`, `shutdownWrite`, owned `splitReader`/`splitWriter` halves, `sendFile`, and `close`. `SocketAddress` provides parsed IP and port values, and `resolveAddresses` returns all resolved addresses. `newBuffer`, `readInto`, and `writeBuffer` allow a reusable buffer on the hot path. `connect` interleaves IPv4 and IPv6 attempts after name resolution. A zero read or write timeout disables that timeout. `read` returns up to 1 MiB as `Bytes`; empty `Bytes` means EOF. `close` is safe after the peer has disconnected.

The package manifest uses camel-case native target keys and names `native/interface.json`. That static file lists opaque types and native function signatures. `spar-ls` reads it without loading the binary; Spar verifies it against the binary when running the package. Keep it in sync when changing the Rust API.

Run `bash build-package.sh && cargo test --offline --test e2e` to test configured clients and listeners, byte-stream reads, and half-close. Set `SPAR_BIN` to choose the Spar executable. The Rust build currently uses the local `spar-native` SDK at `../../../Rust/occ_lang/spar-native`; change that dependency before publishing elsewhere. Native artifacts for macOS and Windows still need builds and host tests.

The [implementation status](TCP_STATUS.md) tracks remaining work against `tcp_req.md`.

## Async sockets

Async sockets have separate handle types. Use them from an `async fn`:

```spar
import pkg { listenAsync, acceptAsync, readAsync, writeAllAsync, closeAsync, closeAsyncListener } from "tcp";

async fn main() -> int {
    var listener = listenAsync(address: "127.0.0.1:8080");
    var connection = await acceptAsync(listener: listener);
    var request = await readAsync(connection: connection, maxBytes: 4096);
    var sent = await writeAllAsync(connection: connection, data: request);
    closeAsync(connection: connection);
    closeAsyncListener(listener: listener);
    return 0;
};
```

`connectAsync(address:, timeoutMillis:)` returns an async connection. `readAsync` returns at most 16 MiB; empty `Bytes` means EOF. `writeAllAsync` writes the entire buffer and returns its byte count. One read may be pending per connection. Writes are serialized and can overlap a read. `cancelAsyncAccept`, `cancelAsyncRead`, and `cancelAsyncWrite` fail pending operations without closing the socket. Closing an async handle cancels its pending work. TLS operations still use the blocking API.

## Distribution

Build and test on each target host before release. On Linux and macOS run `bash build-package.sh && SPAR_BIN=/path/to/spar cargo test --test e2e`. On Windows run `./build-package.ps1`, set `$env:SPAR_BIN` to `spar.exe`, then run `cargo test --test e2e`. The tests use a checked-in localhost certificate.

Collect the six target artifacts listed in the manifest under `native/`. Run `assemble-distribution.sh` or `assemble-distribution.ps1` to make an archive and SHA-256 file. The assembler refuses missing targets. Unpack the archive and add it with `spar add tcp path:/path/to/spar-tcp-0.1.0`. Source builds require the local `spar-native` SDK at `../../../Rust/occ_lang/spar-native`; the assembled binary package does not include that build dependency. macOS and Windows builds and host tests remain pending.

For a package limited to the current host, run `assemble-distribution.sh --host-only` after `build-package.sh`. Windows uses `./assemble-distribution.ps1 -HostOnly` after `./build-package.ps1`. Host archives include only that host's native library. Use the default assembler after all six target artifacts have been collected and tested.
