# Writing Spar native modules

ABI 1.0 is the current contract (see `abi-v1.md`). This guide covers the day-to-day: write, build, package,
load, call, and get performance.

## 1. Rust (recommended)

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib"]
[dependencies]
spar-native = { path = "../spar-native" }
```

```rust
#[spar_native::module(name = "fastArray", version = "0.1.0")]
mod fast_array {
    use spar_native::{Buf, Error, Owned, Resource};

    #[spar_native::function]                       // Spar name: sum
    fn sum(values: &[f64]) -> f64 { values.iter().sum() }

    #[spar_native::function]                       // Spar name: linspace
    fn linspace(n: i64) -> Result<Buf<f64>, Error> {
        if n < 0 { return Err(Error::range("n must be non-negative")); }
        Ok(Buf((0..n).map(|i| i as f64).collect()))   // Vec moves into the runtime, no copy
    }
}
```

* Names: functions and parameters are converted from `snake_case` to `camelCase`
  (`#[function(name = "x")]` overrides).
* Types: `i64` `f64` `bool` `&str` `String` `()` `&[T]` `&mut [T]` `&[u8]` (Bytes) `Vec<u8>`
  `Buf<T>` `Vec<f64|i64>` `Value` `Result<T, E: Display>` and `Owned<R: Resource>` / `&R`.
* `&[f64]` is spelled `Slice<float>` in the signature: it accepts a `[float]` list (one copy) or a
  `Buffer` (zero-copy). `&mut [f64]` is `Buffer` only (lists are immutable values).
* Panics never cross the boundary: the trampoline uses `catch_unwind` and reports
  `"<fn> panicked: ..."`. A crate built with `panic = "abort"` aborts instead.
* Lifetimes: everything borrowed from Spar is valid until the function returns (`'env`). Copy to keep.
* Resources: implement `Resource` (`SPAR_TYPE = "Tally"`), return `Owned(value)`, take `&Tally`.
  The runtime runs `Drop` when the resource dies. State needs interior mutability.

## 2. C

`include/spar_native.h` is the whole contract; see `examples/native/c-fastmath/fastmath.c`.
Export exactly one symbol, `spar_native_module_v1`, returning a static `SparModuleDescriptor`; in
`init` call `module_add_function` for each function. Compile with `-fPIC -fvisibility=hidden
-shared`.

## 3. C++

`include/spar_native.hpp` adds RAII borrows and `spar::guard`, which turns any C++ exception into a
Spar error. Exceptions must never escape a function that Spar calls; see `cpp-textkit`.

## 4. Zig

`examples/native/zig-fastmath/fastmath.zig` uses `@cImport` of the same header. It has not been
compiled in this repository's environment (no Zig toolchain); treat it as unverified.

## 5. Packaging

```spar
struct Native {
    module: str = "fastArray";
    abi: str = "spar-native-1";
    capabilities: str = "typed-arrays,strings";
    interface: str = "native/interface.json";
    linuxX8664Gnu: str = "native/linux-x86_64-gnu/libfastarray.so";
    linuxX8664GnuSha256: str = "<64 hex>";
    macosAarch64: str = "native/macos-aarch64/libfastarray.dylib";
};
```
Artifact keys use camel case, such as `linuxX8664Gnu`, `macosAarch64`, and `windowsX8664Msvc`; the most specific match for the host wins. Older underscore keys remain readable but render in camel case. Artifact paths are relative to the
package root and cannot leave it. The SHA-256 (if present) is verified before `dlopen`. Run with
`SPAR_NO_NATIVE=1` to refuse all native packages. For experiments, `SPAR_NATIVE_MODULES=a.so:b.so`
loads modules into every run, and `SPAR_NATIVE_DEBUG=1` prints what was loaded (name, version, ABI,
capabilities, functions).

For editor type checking, provide a static interface file at the manifest's `interface` path. It is JSON with format version 1, the module name, opaque type names, and function signatures. The editor reads this file without loading a native binary. When Spar loads the binary, it rejects a mismatch in type names or function signatures.

```json
{
  "format_version": 1,
  "module": "fastArray",
  "types": ["BufferHandle"],
  "functions": [
    {"name": "open", "params": [{"name": "size", "ty": "int"}], "ret": "BufferHandle"}
  ]
}
```

Call syntax includes `import module; module.function(name: value)` and `module::function(name: value)`. The module is loaded once per process; per-runtime module state remains open work.

## 6. Performance: what is fast

Measured numbers are in `performance.md`. Rules of thumb:

| Pattern | Cost |
|---|---|
| one call, bulk zero-copy work (`Buffer`, `Bytes`) | boundary overhead vanishes (within noise of direct Rust) |
| `[float]` list argument | one contiguous copy per call: ~3-4x slower than a Buffer at 1M elements |
| scalar call, generic signature | tens of ns of ABI + interpreter call overhead |
| scalar call, direct signature (`"ii>i"`) | near the cost of a Rust closure call |
| record field access, per-cell list access | avoid in loops; build results natively |
| callback per element (`SPAR_FN_CALLS`) | full interpreter call each time: use only for rare hooks, prefer a batch operation |

Keep data native: return `Buf<T>`/Buffers and pass them to the next native call instead of
converting to lists.

## 7. Threads and async

* No global lock. `SparEnv` belongs to the thread that received the call (checked). A native kernel
  may use its own threads over borrowed data as long as it joins them before returning.
* `SPAR_FN_ASYNC` functions return `Promise<T>`: call `async_begin`, start work anywhere, finish
  from any thread with `async_complete(json)` / `async_fail`, then `async_release`. States:
  CREATED → RUNNING → COMPLETED | FAILED | CANCELLED → CONSUMED; illegal transitions return
  `SPAR_E_INVALID_STATE`; completing after runtime shutdown returns `SPAR_E_CANCELLED` and touches
  nothing. Releasing an unfinished operation fails its promise instead of hanging.
* Completion values travel as JSON text so no `SparValue` ever crosses threads.
* Native resource handles may cross spawned Spar tasks in the same execution tree. Their payloads
  and finalizers must support concurrent calls. `resource_get` holds a lease until the current
  native call returns; `resource_close` defers finalization while a call still uses that pointer.

## 8. Security

Native modules are trusted in-process code: they can crash the process, corrupt memory and execute
anything. The loader and API validate versions, capabilities, handles, generations, lengths, types,
thread affinity and borrow state to catch mistakes, not attacks. Registries or sandboxes that must
not run native code should set `SPAR_NO_NATIVE`.

## 9. Versioning policy

ABI 1.0: additive changes only (append table entries and capability bits,
new tags/status codes, new trailing struct fields); minor version bumps when the table grows;
modules declare `min_abi_minor` and required capabilities and are rejected with a diagnostic
otherwise. Compiled modules keep working on newer runtimes of the same major (tested against a frozen
copy of the 0.1 header). SDK and API ergonomics may change without an ABI change.
