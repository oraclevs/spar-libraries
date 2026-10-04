# HTTP server package

This Spar library builds a small HTTP/1.1 text server on the `tcp` native package. The request parser and response writer are written in Spar; the native module only provides TCP sockets. It is a concrete test that an app can import a library, resolve its native transitive dependency, and serve a request without adding HTTP syntax to the language.

Build the TCP artifact and install this package in an app:

```bash
cd ../spar-tcp && bash build-package.sh
cd /path/to/your-app
spar add server path:/absolute/path/to/spar-http-server
```

```spar
import pkg { serve, ServerRequest, ServerResponse } from "server";

fn handle(request: ServerRequest) -> ServerResponse {
    return ServerResponse(body: "Hello from ${request.target}");
};

fn main() -> void {
    serve(address: "127.0.0.1:8080", handler: handle);
};
```

`ServerRequest` contains `method`, `target`, `version`, lowercase `headers`, and a UTF-8 `body`. `ServerResponse` has `status`, `reason`, `contentType`, extra `headers`, and `body`. `serve` accepts connections one at a time. `openListener` and `serveOne` allow a bounded server run, useful in tests. The parser accepts HTTP/1.1 with a UTF-8 body, a body limit of 1 MiB, an 8 KiB header limit, and a five second read timeout. It rejects transfer encoding, duplicate headers, malformed header lines, and unsupported protocol versions. Each response sends `Content-Length` in UTF-8 bytes and closes the connection. TLS listeners are available through `openTlsListener(address:, certPath:, keyPath:)` and `serveTls(address:, certPath:, keyPath:, handler:)`. The certificate and private key must be PEM files. The server still does not provide chunked requests, binary bodies, or concurrent handling.

Build the TCP package, then run `cargo test --offline --manifest-path tests/Cargo.toml` to test plain HTTP and HTTPS with a verified local certificate. The plain HTTP test sends a UTF-8 character in two writes. Set `SPAR_BIN` to a specific Spar binary if needed. Native artifacts for other platforms must be built and tested on those hosts before distribution.
