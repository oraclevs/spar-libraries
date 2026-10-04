# Spar native extension architecture

Status: **ABI 1.0 candidate**, pending macOS and Windows test results. See `abi-v1.md` for the binary contract and
`performance.md` for measured numbers (added when the benchmarks exist).

## Goal

A Spar package can implement hot code in Rust, C, C++, Zig or any language that speaks the C ABI, and
Spar users call it like an ordinary module. The runtime is free to change its internals (value
layout, string storage, memory management, scheduler) without breaking compiled packages.

## Layers

```
  Spar program           fastMath::add(a: 20, b: 22)
        │
  Spar runtime           NativeRegistry -> NativeFunction (existing dispatch, ids resolved at compile time)
        │
  native module loader   dlopen, descriptor check, capability negotiation, registration
        │
  Spar Native ABI 1      one exported symbol + versioned function table + POD structs
        │
  SDKs                   spar-native-sys (raw)  spar-native (safe)  spar_native.h  C++ wrapper  Zig
        │
  extension              Rust | C | C++ | Zig
```

*API* is what an extension author types (SDK). *ABI* is the binary contract: symbols, struct layouts,
constants, ownership rules. The SDKs may improve without touching the ABI.

## What the audit of the existing runtime found

| Area | Fact | Consequence |
|---|---|---|
| Native dispatch | `NativeRegistry` holds `NativeFunction { module, name, params: Vec<(String, SparType)>, ret, execution, private, handler }`. Ids are resolved at compile time and calls go through `registry.call(id, ctx, &[Value])`. Public (non-`private`) natives are callable as `module::name(arg: v)` and are typechecked from the registry signature. | Native modules register into this registry. No parallel callable system. Named arguments are already resolved to positional slots at compile time, so the ABI is positional-only. |
| `Value` | 40-byte enum. `String(String)`, `Bytes(Vec<u8>)`, `List(Shared<Vec<Value>>)`, `Object(Shared<Record>)`, `Resource(ResourceId)`. `Shared` is an `Arc` with copy-on-write. Lists of floats are `Vec<Value::Float>`, not contiguous `f64`. | Values never cross the ABI. Argument slices are borrowed directly (no clone), so strings and bytes are zero-copy for the duration of a call. Float/int lists cannot be borrowed as `&[f64]` without a copy today; the ABI models this honestly (`SPAR_BUFFER_COPIED`, `SPAR_BUFFER_NO_COPY`). A native-owned typed `Buffer` resource gives a true zero-copy typed-array path. |
| Errors | `SparError` with spans; natives return `Result<Value, SparError>`, dummy spans get the call span attached. | Native errors become `SparError::EvalError` with the call span. |
| Resources | `ResourceTable`: `ResourceId -> Box<dyn Any + Send>`, per `RuntimeContext`. Spawned tasks get an empty table. | Native resources are a Rust wrapper type whose `Drop` runs the extension finalizer. Finalizers run when the table drops or the resource is closed, before the module is unloaded (modules are never unloaded). |
| Threads | Runtime is not globally locked; contexts are per task. `Value` is `Send`. No GIL exists. | `SparEnv` is call-scoped and thread-affine. Native kernels may run on their own threads without any runtime lock. No GIL concept in the ABI. |
| Async | `NativeExecutionKind::Async`, promises, scheduler in `runtime/scheduler.rs`. | Async native functions are described in the descriptor; completion is delivered through a state-machine handle. |
| Dynamic loading | None existed. `libloading` is available. | New loader in `spar/src/native_module/`. |
| Packages | `spar.package.spar` manifest, lockfile, store, locator. | Native artifacts are declared in the manifest (`native` section) in a later phase; the loader itself takes an explicit path first. |

## Decisions (short ADRs)

### ADR-1 ABI representation: C ABI, tagged value + generational handles
- *Problem*: cross the boundary without exposing `Value` layout, keep scalars allocation-free.
- *Alternatives*: (a) everything a handle (Node-API style); (b) expose a `repr(C)` mirror of `Value`;
  (c) per-signature generated raw C functions only.
- *Chosen*: `SparValue`, a 16-byte `{u32 tag, u32 flags, u64 payload}`. Null, bool, int and float are
  inline (no runtime call, no allocation). Heap values (string, bytes, list, record, resource, option…)
  are a generational handle `{index:u32, generation:u32}` in the payload. Constructors for inline scalars are
  `static inline` in the header. Raw direct-call signatures (`int64_t(*)(int64_t,int64_t)`) exist as an
  optional fast path.
- *Perf*: scalar arguments never touch the handle table. Measured in `performance.md`.
- *Compat*: tag numbers and the 16-byte layout are ABI. New value kinds get new tags; unknown tags
  must be tolerated.
- *Rejected*: (a) costs a table lookup per scalar; (b) freezes `Value`; (c) unusable for structured data,
  kept only as the optional fast path.

### ADR-2 Handle design: per-call generational arena
- *Problem*: temporary values need cheap creation and use-after-scope detection.
- *Chosen*: a per-call `HandleScope` (Vec of slots). A handle is `(index, generation)`. Closing the
  scope bumps every used slot's generation and recycles the arena (vector is kept, cleared, and reused
  across calls — no allocation on the steady state). Argument slots hold a *borrowed pointer* to the
  argument `Value`, not a clone. Native-created values are owned by the slot. Persistent references
  (`SparRef`) live in a separate table on the runtime with their own generations and are explicit.
- *Alternatives*: Box per handle (allocation per temp, rejected); raw pointers to `Value` (unsafe after
  any Vec growth or COW, rejected); one global table with locking (contention, rejected).
- *Compat*: handles are opaque `u64`; representation is not ABI beyond "0 is invalid".

### ADR-3 Module entrypoint and negotiation
- One exported symbol for new modules: `spar_native_module_v1` returning a
  `SparModuleDescriptor*`. The loader also accepts the older `spar_native_module_v0`
  symbol and gives ABI 0 modules the ABI 0.2 table. Everything else is registered through the API table, not linker symbols.
- The descriptor starts with `struct_size`. The loader validates `struct_size`, ABI major/minor,
  required capabilities against the runtime's capability mask, target triple string, non-empty module
  name, and only then calls `init`. Failures are `SparError` diagnostics naming the library path.
- The runtime passes `SparApiV0*` with `struct_size` and `capabilities`. New functions are only ever
  appended; extensions must check `struct_size` before touching an entry they compiled against
  a newer header.

### ADR-4 Call frame
- `invoke(env, userdata, argv, argc, out)` — positional, contiguous, no maps or vectors built per call.
  The runtime builds the `argv` array in a reusable scratch buffer stored in the per-thread call state.
- Optional direct signatures for pure-scalar functions skip `SparValue` entirely. Whether they earn
  their place is decided by the benchmark, not assumed (`performance.md`).

### ADR-5 Errors and panics
- `int32_t` status. `SPAR_OK` = success. On failure the extension calls `error_set` (or the runtime
  synthesises a message from the status). Nothing is allocated on the success path.
- No unwinding across the ABI. Rust SDK trampolines use `catch_unwind`; C++ wrapper catches `...`.
  A `panic = "abort"` extension aborts the process (documented).

### ADR-6 Strings and borrowed data
- A string view (`ptr`, `len`) is valid until the native function returns. Strings are immutable so no
  lease object is required. Retaining data means copying it or taking a persistent reference.
- Bytes/typed buffers use explicit borrows with a state machine (`AVAILABLE → SHARED(n) | MUT → RELEASED`)
  because mutation is possible for native-owned buffers. All borrows are auto-released when the
  call scope closes; early `buffer_release` is allowed and required before re-borrowing mutably.

### ADR-7 Typed buffers and bulk data
- `buffer_borrow(v, dtype, flags)` accepts `Bytes` (u8, zero-copy), native `Buffer` resources
  (any dtype, zero-copy) and `List<int|float|bool>` (copy into contiguous scratch, `SPAR_BUFFER_COPIED`;
  fails if `SPAR_BUFFER_NO_COPY`). Writable copied views are not offered for lists: results are
  returned as new buffers/lists.
- Arrow C Data Interface is an integration layer for tables and is evaluated in `arrow.md`; DLPack is
  noted for tensors. Neither becomes the core value representation.

### ADR-8 Threading
- No GIL. `SparEnv` belongs to the thread that received the call. Using it elsewhere returns
  `SPAR_E_WRONG_THREAD` (checked in debug and release, it is one integer compare).
- Native code may start threads and process borrowed buffers on them, provided the borrow outlives
  those threads (join before return). Worker → Spar interaction goes through the executor
  (`executor_post`) and completion handles, not through `SparEnv`.
- `SPAR_CAP_THREAD_ATTACH` (attach foreign threads) is deliberately not offered in v0.

### ADR-9 Async
- Async functions get an `SparAsync` completion object. Lifecycle: `CREATED → RUNNING → COMPLETED |
  FAILED | CANCELLED → CONSUMED`. Every transition is validated. Runtime shutdown cancels outstanding
  operations and later completion attempts return `SPAR_E_CANCELLED` and never touch the runtime.

### ADR-10 Resources and shutdown order
- Resources are `Value::Resource(id)` backed by a wrapper that owns `(ptr, type_tag, finalizer,
  userdata)`. Finalizer contract: may free memory and release OS handles; must not call back into Spar.
- Shutdown: context drop → resource table drop → finalizers run → module `quiesce` → module `destroy`.
  Libraries stay mapped until process exit (no `dlclose`) so finalizers, threads and callbacks can
  never point into unmapped code.

### ADR-11 Struct/record access
- Accessor API only (`symbol_intern`, `record_get/new/set`, `record_len`, `record_key_at`). Field
  symbols are interned once at init. No layout promises.

### ADR-12 Stability tiers
- ABI 1.0 preserves the 0.2 layout under a new symbol. Platform validation remains pending after
  the criteria in `NATIVE_API_TASKS.md` ("Freeze criteria") are satisfied.

## Security boundary

Native modules are trusted in-process code. They can crash the process, corrupt memory and run any
native instruction. The ABI checks handles, generations, types, lengths, thread affinity, borrow
state and version compatibility to catch *accidents*, not malice. The loader never searches the
working directory; it loads an explicit path (or a manifest-selected artifact with a recorded
SHA-256). A sandboxed WASM extension mode is a possible future class that would reuse the same
descriptor and function shapes; nothing here depends on it.

## Performance model

Cross the boundary once per bulk operation, not once per element. The measured cost of the bridge
is in `performance.md`. Callbacks per element are supported but are documented as the slow pattern.
