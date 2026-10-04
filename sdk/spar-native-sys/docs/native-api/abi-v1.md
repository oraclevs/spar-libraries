# Spar Native ABI 1.0

Canonical definition: `include/spar_native.h`. Rust mirror: `src/lib.rs`. Golden tests:
`tests/abi_golden.rs` + `tests/abi_check.c` compare sizes, alignments, offsets and constants of
both. ABI 1.0 uses the tested ABI 0.2 data and function-table layout with a new
entry symbol and version number. The host continues to load ABI 0.1 and 0.2 modules through
their old symbol and passes them an ABI 0.2 table.

## Rules

1. C calling convention only. No unwinding across the boundary (panics/exceptions are contained by the
   SDK trampoline; a stray unwind is undefined behaviour).
2. Fixed-width integers. Lengths are `uint64_t`. Enums are `int32_t`/`uint32_t` constants; unknown
   values must be tolerated.
3. Extensible structs start with `struct_size`; tables only grow at the end; fields are never reordered.
4. Exactly one linker-visible symbol: `spar_native_module_v1`.

## Loading sequence

`dlopen` (path given explicitly) → resolve symbol → read descriptor → validate `struct_size >=`
known prefix, `abi_major == 1`, `min_abi_minor <= runtime minor`, `required_capabilities ⊆ runtime`,
target string (if non-empty, must equal the host triple), module name (identifier) →
`init(api, module, &state)` → function specs registered into `NativeRegistry` → library stays
mapped until process exit.

## Ownership table

| Item | Ownership | Valid until |
|---|---|---|
| `SparValue` inline (void/bool/int/float) | value, copied | forever |
| `SparValue` handle (tag ≥ 16) | call-scoped, runtime owned | native function returns; then stale (generation mismatch → `SPAR_E_INVALID_HANDLE`) |
| `argv` array | borrowed from runtime | native function returns |
| `SparStrView.ptr` | borrowed | native function returns (string immutable) |
| `SparBufferView.data` | borrowed | `buffer_release` or call end; mutable borrow excludes all others |
| Copied buffer view (`SPAR_BUFFER_COPIED`) | runtime-owned scratch | same |
| `string_new/bytes_new` input pointers | caller keeps ownership; runtime copies | immediately |
| `buffer_from_external` data | transferred to runtime; released via the finalizer | finalizer runs on buffer death |
| `SparRef` | module-owned persistent | `ref_drop` |
| Resource `ptr` | transferred to runtime, finalizer runs on close/table drop | finalizer |
| `SparModule` | init-scoped | `init` returns |
| `SparEnv` | call-scoped, thread-affine | native function returns |
| Descriptor and spec strings | module-owned, static | forever (loader copies what it keeps) |
| Error message bytes | copied by `error_set` | immediately |

## Borrow state machine (per buffer slot)

```
AVAILABLE --shared borrow--> SHARED(n) --release--> ... --> AVAILABLE
AVAILABLE --mut borrow----> MUT ---------release-------------> AVAILABLE
SHARED(n) --mut borrow--> SPAR_E_BORROW      MUT --any borrow--> SPAR_E_BORROW
```
Call end releases everything still open. Releasing an unknown/stale token is `SPAR_E_INVALID_HANDLE`,
double release is `SPAR_E_BORROW`.

## Value kinds accepted by APIs

| API | Accepts |
|---|---|
| `int_get` | int |
| `float_get` | float, int (converted) |
| `bool_get` | bool |
| `string_view` | str |
| `buffer_borrow` | `Bytes` (dtype u8), native `Buffer` resource (its dtype), `List` of int (i64/…), float (f64/f32), bool (`SPAR_DTYPE_BOOL`) via copy |
| `list_*`, `record_*`, `option_*` | list, record, option |
| `call` | closure/function value |

## Signature spelling

Parameter/return types are Spar type text parsed by the compiler's own type parser at registration:
`int float bool str Bytes Record Any [float] Option<int> Result<str, str>` (lists are `[T]`) plus two
native-only spellings: `Buffer` (a native-owned typed array resource) and `Slice<T>` (`T` = `int`,
`float`, `bool`: accepts a `[T]` list or a `Buffer`; a list costs one copy, a Buffer none). Parse failure rejects the module with the offending function named.

## Direct calls

`direct_sig` (`"ii>i"`, `"ff>f"`, `"i>b"`…) lets a pure-scalar function be exported as a plain C
function and skip `SparValue` marshalling. `int`→`i`, `float`→`f`, `bool`→`b`, void return `v`.
The declared `params` types must agree with the signature or the module is rejected.
Overflow/semantic checks are the extension's responsibility.

## Versioning policy

Additive within a major: new table entries at the end, new capability bits, new tags, new status
codes, new fields after `struct_size`. Anything else is a new major (new entry symbol name).
ABI 1.0 freezes the existing field order, widths, calling conventions, status and tag numbers,
and ownership rules. ABI 0 remains experimental and is loaded for compatibility only.

## Module lifetime, state, and trust

The loader calls `init` once per canonical library path in a process. A module may return
`out_state`; the host passes it to `quiesce` and `destroy` on `shutdown_all`, after
runtime contexts and their native resources have been dropped. The library stays mapped.
Modules registered with multiple Spar engines share one process-wide instance. A module that
needs mutable state must synchronize its own userdata and static state. ABI 1.0 does not
offer separate module instances per engine or access to a runtime-owned state slot.

A native artifact executes with the Spar process's privileges. The loader does not sandbox
machine code. Manifests name an artifact for each target and may pin its SHA-256; projects
can disable native loading with `SPAR_NO_NATIVE`. Run untrusted native code in a separate
operating-system process with an appropriate sandbox. The module API's thread and borrow
checks protect host invariants but are not a security boundary against malicious code.

Struct schema introspection and foreign-thread attachment are outside ABI 1.0. New entries
may be appended to the function table in a future 1.x minor version; modules must check
`struct_size` and capabilities before using them.

## Native resources across async tasks

Native resource handles are shared by spawned tasks in one execution tree. A resource payload and its finalizer must be safe under concurrent native calls. `resource_get` keeps the pointer alive until its call returns, including when another task calls `resource_close`. A closed handle cannot be acquired by a later call. Ordinary runtime resources remain task-local.
