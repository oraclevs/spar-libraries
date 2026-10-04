# TCP implementation status

The package has blocking TCP and reactor-backed async TCP. All twelve package integration tests pass on Linux. Cross-platform host tests and the remaining items below are required before a stable release.

| Area | Available now | Still required |
| --- | --- | --- |
| Addresses and DNS | IPv4/IPv6 addresses, typed `SocketAddress`, all-address DNS lookup, interleaved IPv4/IPv6 blocking connection attempts | Parallel DNS lookup, cancellable resolution, full RFC 8305 tuning |
| Server and client | Configurable sockets, backlog, blocking accept and connect, reactor-backed async accept and connect, explicit cancellation | Graceful listener drain; macOS and Windows host validation |
| Byte stream | Read, exact read, reusable buffers, partial and full write, peek, EOF, split halves, file copy, async read and full write | Vectored I/O, zero-copy file transfer, stream-to-stream copy, async TLS |
| Options | Reuse address/port, IPv6-only, buffer sizes, no-delay, keepalive timing, linger, abortive close, pending error | Keepalive probe count, native handle adoption, platform-specific options |
| Control and errors | Blocking read/write timeouts, connect/accept timeout, explicit async cancellation; errors include OS kind and code in messages | Persistent deadlines and typed network errors |
| Validation | Spar integration tests for TCP, TLS, HTTP, and HTTPS on Linux; portable TCP test fixture; release archive scripts | Stress and benchmark suites, macOS and Windows builds and host tests |

Native ABI 1 supports external promise completion. The Rust SDK exposes async tasks, and Spar shares native handles safely between async tasks. Tokio handles socket readiness using the host's reactor. `sendFile` currently uses a portable file copy and makes no zero-copy guarantee. The blocking TLS implementation remains for compatibility with `spar-http-server`.
