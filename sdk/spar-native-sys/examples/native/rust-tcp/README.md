# Native TCP ABI sample

This Rust module exercises Spar's native ABI 1 resource handles and byte transfer. Build it with `cargo build --release`; the compiler's native TCP integration tests load its shared library directly.

The reusable TCP package is in `SparLibs/spar-tcp` in the Spar libraries workspace. That package owns its Spar wrapper, manifest, and platform build scripts. The HTTP server library that uses it is in `SparLibs/spar-http-server`.
