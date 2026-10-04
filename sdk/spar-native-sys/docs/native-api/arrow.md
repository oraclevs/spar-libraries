# Tables, Arrow C Data Interface and DLPack (evaluation)

Status: **design only, not implemented in ABI 0.** `SPAR_CAP_TABLES` and `SPAR_CAP_ARROW_C_DATA`
are reserved bits that the runtime does not advertise.

## What exists today

Spar has `Table` values (`Value::Table(Shared<TableValue>)`, row oriented: a schema plus
`Vec<Record>`-like rows). It has no columnar storage and no typed arrays inside `List`.
What ABI 0 offers for bulk numeric work instead:

| Need | ABI 0 answer |
|---|---|
| contiguous numeric column, zero-copy | native `Buffer` resource (`buffer_new`, `buffer_from_external`, `Buf<T>` in the Rust SDK) borrowed with `buffer_borrow` |
| numeric column from a Spar list | `Slice<T>` parameter: one copy into contiguous scratch (`SPAR_BUFFER_COPIED`), no zero-copy |
| table of rows | accessor API (`list_get`, `record_get`), per-cell, deliberately slow |

## Recommendation

Do **not** invent `SparDataFrameABI`. Add a thin adapter when Spar grows a columnar table:

```
Spar Table (columnar)  --export-->  ArrowSchema + ArrowArray (C Data Interface)
                                    ArrowArrayStream       (C Stream Interface, for batches)
```

* Arrow's C Data Interface is ABI-stable, dependency-free and defines its own release callbacks,
  which is exactly the ownership shape `buffer_from_external` already uses (foreign memory + a
  finalizer that must not re-enter the runtime).
* The integration layer is two API entries (appended, minor bump): `table_export_arrow(env, v,
  ArrowArray*, ArrowSchema*)` and `table_import_arrow(env, ArrowArray*, ArrowSchema*, SparValue*)`,
  gated by `SPAR_CAP_ARROW_C_DATA`. Import takes ownership of the release callback; the resulting
  table keeps it alive, and column buffers are borrowed zero-copy through `buffer_borrow`.
* A streaming variant uses `ArrowArrayStream` so a native package can produce batches lazily
  (`readCsv` returning a stream) without materialising millions of cells as `Value`s.

## Blockers on the Spar side (not the ABI)

1. Spar `Table` is row based; export would materialise columns (one pass, still far cheaper than
   per-cell calls, but not zero-copy). A columnar `TableValue` (or typed columns behind the
   existing `Value::List` API, the "typed collections" item in the roadmap) is the enabler.
2. Nullability and dictionary types need a Spar-side decision before they can be exposed.
3. Strings: Arrow `utf8` needs an offsets buffer; Spar strings are `String` per cell.

## DLPack

For tensors (`ndim > 1`, strides, device memory) `SparBufferView` already carries `dtype`, `ndim`,
`stride_bytes` and reserved space. A `DLManagedTensor` import/export pair is a natural minor
addition once a tensor value exists. It is intentionally not the core representation: it would
make GPU/device pointers part of the ABI before any Spar runtime can use them.

## Verdict for the freeze decision

Tables do **not** block ABI 1: the additive rules (append entries, add capability bits) let
Arrow/DLPack arrive as minor versions. Blocking criteria are the ones in `NATIVE_API_TASKS.md`.
