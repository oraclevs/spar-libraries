/*
 * spar_native.h - Spar Native ABI 0 (EXPERIMENTAL, not stable).
 *
 * This header is the canonical definition of the binary contract. The Rust
 * bindings in spar-native-sys mirror it and are checked against it by the
 * ABI golden tests. See docs/native-api/abi-v0.md.
 */
#ifndef SPAR_NATIVE_H
#define SPAR_NATIVE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SPAR_NATIVE_ABI_EXPERIMENTAL 1
#define SPAR_NATIVE_ABI_MAJOR 0u
#define SPAR_NATIVE_ABI_MINOR 1u

#if defined(_WIN32)
#  define SPAR_EXPORT __declspec(dllexport)
#else
#  define SPAR_EXPORT __attribute__((visibility("default")))
#endif
#define SPAR_CALL /* platform C calling convention */

/* ---- status codes (int32_t; consumers must tolerate unknown values) ---- */
typedef int32_t spar_status_t;
#define SPAR_OK                 ((spar_status_t)0)
#define SPAR_E_ERROR            ((spar_status_t)1)  /* generic; message set via error_set */
#define SPAR_E_TYPE             ((spar_status_t)2)
#define SPAR_E_RANGE            ((spar_status_t)3)
#define SPAR_E_OOM              ((spar_status_t)4)
#define SPAR_E_INVALID_ARGUMENT ((spar_status_t)5)
#define SPAR_E_WRONG_THREAD     ((spar_status_t)6)
#define SPAR_E_UNSUPPORTED      ((spar_status_t)7)
#define SPAR_E_CANCELLED        ((spar_status_t)8)
#define SPAR_E_ABI_MISMATCH     ((spar_status_t)9)
#define SPAR_E_INVALID_HANDLE   ((spar_status_t)10) /* null, out of range or stale generation */
#define SPAR_E_BORROW           ((spar_status_t)11) /* borrow state violation */
#define SPAR_E_UTF8             ((spar_status_t)12)
#define SPAR_E_INVALID_STATE    ((spar_status_t)13) /* bad async/resource transition */
#define SPAR_E_PANIC            ((spar_status_t)14) /* extension panic/exception contained */

/* ---- capabilities (bit set) ---- */
#define SPAR_CAP_STRINGS        (UINT64_C(1) << 0)
#define SPAR_CAP_BYTES          (UINT64_C(1) << 1)
#define SPAR_CAP_TYPED_ARRAYS   (UINT64_C(1) << 2)
#define SPAR_CAP_LISTS          (UINT64_C(1) << 3)
#define SPAR_CAP_RECORDS        (UINT64_C(1) << 4)
#define SPAR_CAP_NATIVE_RESOURCES (UINT64_C(1) << 5)
#define SPAR_CAP_CALLBACKS      (UINT64_C(1) << 6)
#define SPAR_CAP_ASYNC          (UINT64_C(1) << 7)
#define SPAR_CAP_ZERO_COPY_BYTES (UINT64_C(1) << 8)
#define SPAR_CAP_DIRECT_CALLS   (UINT64_C(1) << 9)
#define SPAR_CAP_TABLES         (UINT64_C(1) << 10) /* reserved, not offered yet */
#define SPAR_CAP_ARROW_C_DATA   (UINT64_C(1) << 11) /* reserved, not offered yet */

/* ---- opaque runtime-owned types ---- */
typedef struct SparEnv SparEnv;       /* call-scoped, thread-affine */
typedef struct SparModule SparModule; /* valid during init only */
typedef struct SparAsync SparAsync;   /* async completion object */

/* ---- value ---- */
#define SPAR_TAG_VOID     0u
#define SPAR_TAG_BOOL     1u
#define SPAR_TAG_INT      2u
#define SPAR_TAG_FLOAT    3u
/* tags >= 16 carry a generational handle in the payload */
#define SPAR_TAG_STRING   16u
#define SPAR_TAG_BYTES    17u
#define SPAR_TAG_LIST     18u
#define SPAR_TAG_RECORD   19u
#define SPAR_TAG_OPTION   20u
#define SPAR_TAG_RESOURCE 21u
#define SPAR_TAG_CALLABLE 22u
#define SPAR_TAG_OTHER    31u

typedef struct SparValue {
    uint32_t tag;
    uint32_t flags; /* reserved, must be 0 */
    union {
        int64_t i64;
        uint64_t u64;
        double f64;
        uint64_t handle; /* (generation << 32) | index; 0 is never valid */
    } payload;
} SparValue;

static inline SparValue spar_void(void) { SparValue v; v.tag = SPAR_TAG_VOID; v.flags = 0; v.payload.u64 = 0; return v; }
static inline SparValue spar_bool(int b) { SparValue v; v.tag = SPAR_TAG_BOOL; v.flags = 0; v.payload.u64 = b ? 1u : 0u; return v; }
static inline SparValue spar_int(int64_t i) { SparValue v; v.tag = SPAR_TAG_INT; v.flags = 0; v.payload.i64 = i; return v; }
static inline SparValue spar_float(double f) { SparValue v; v.tag = SPAR_TAG_FLOAT; v.flags = 0; v.payload.f64 = f; return v; }

typedef uint64_t SparRef;    /* persistent reference, 0 invalid */
typedef uint32_t SparSymbol; /* interned field name */
typedef uint64_t SparBorrow; /* borrow token, 0 invalid */

/* ---- views ---- */
typedef struct SparStrView {
    const uint8_t *ptr; /* valid until the native function returns; not NUL terminated */
    uint64_t len;
} SparStrView;

#define SPAR_DTYPE_U8   1u
#define SPAR_DTYPE_I8   2u
#define SPAR_DTYPE_U16  3u
#define SPAR_DTYPE_I16  4u
#define SPAR_DTYPE_U32  5u
#define SPAR_DTYPE_I32  6u
#define SPAR_DTYPE_U64  7u
#define SPAR_DTYPE_I64  8u
#define SPAR_DTYPE_F32  9u
#define SPAR_DTYPE_F64  10u
#define SPAR_DTYPE_BOOL 11u /* one byte per element, 0 or 1 */

#define SPAR_BUFFER_READ       (1u << 0)
#define SPAR_BUFFER_WRITE      (1u << 1)
#define SPAR_BUFFER_NO_COPY    (1u << 3) /* fail with SPAR_E_UNSUPPORTED instead of copying */
#define SPAR_BUFFER_COPIED     (1u << 4) /* output flag: view is a scratch copy */

typedef struct SparBufferView {
    uint32_t struct_size;
    uint32_t flags;         /* SPAR_BUFFER_* (in: request, out: result) */
    void *data;
    uint64_t len_elements;
    uint64_t len_bytes;
    uint32_t dtype;         /* SPAR_DTYPE_* */
    uint32_t ndim;          /* 1 in ABI 0 */
    int64_t stride_bytes;   /* element stride, = element size for contiguous */
    uint64_t reserved;
    SparBorrow borrow;
} SparBufferView;

/* ---- function descriptors ---- */
typedef struct SparParamSpec {
    const char *name; uint64_t name_len;
    const char *type; uint64_t type_len; /* Spar type spelling: "int", "float", "str", "List<float>", "Bytes"... */
} SparParamSpec;

typedef spar_status_t (SPAR_CALL *SparNativeFn)(
    SparEnv *env, void *userdata,
    const SparValue *argv, uint64_t argc,
    SparValue *out_result);

#define SPAR_FN_ASYNC (1u << 0)
/* The function calls back into Spar through api->call. Only functions declared with this flag may. */
#define SPAR_FN_CALLS (1u << 1)

/* Direct signatures: the extension exports a plain C function, no SparValue.
 * signature is a string of arg kinds then '>' then the return kind, using
 * 'i' (int64_t), 'f' (double), 'b' (uint8_t bool), 'v' (void return only).
 * Example: "ii>i" is int64_t (*)(int64_t, int64_t). Up to 4 args. */
typedef struct SparFunctionSpec {
    uint32_t struct_size;
    uint32_t flags;
    const char *name; uint64_t name_len;
    const SparParamSpec *params; uint64_t param_count;
    const char *ret_type; uint64_t ret_type_len;
    SparNativeFn invoke;    /* generic entry; may be NULL when direct is set */
    void *userdata;
    const void *direct;     /* optional direct function pointer */
    const char *direct_sig; uint64_t direct_sig_len;
    uint64_t reserved[2];
} SparFunctionSpec;

/* ---- finalizers ---- */
typedef void (SPAR_CALL *SparFinalizer)(void *data, void *userdata);

typedef struct SparApiV0 {
    uint32_t struct_size;
    uint16_t abi_major;
    uint16_t abi_minor;
    uint64_t capabilities;

    /* registration (init only) */
    spar_status_t (SPAR_CALL *module_add_function)(SparModule *module, const SparFunctionSpec *spec);

    /* errors */
    spar_status_t (SPAR_CALL *error_set)(SparEnv *env, int32_t kind, const uint8_t *msg, uint64_t len);

    /* scalars (handles or inline) */
    spar_status_t (SPAR_CALL *int_get)(SparEnv *env, SparValue v, int64_t *out);
    spar_status_t (SPAR_CALL *float_get)(SparEnv *env, SparValue v, double *out); /* accepts int */
    spar_status_t (SPAR_CALL *bool_get)(SparEnv *env, SparValue v, uint8_t *out);

    /* strings */
    spar_status_t (SPAR_CALL *string_new)(SparEnv *env, const uint8_t *ptr, uint64_t len, SparValue *out); /* UTF-8 validated */
    spar_status_t (SPAR_CALL *string_view)(SparEnv *env, SparValue v, SparStrView *out);

    /* bytes and typed buffers */
    spar_status_t (SPAR_CALL *bytes_new)(SparEnv *env, const uint8_t *ptr, uint64_t len, SparValue *out); /* copies */
    spar_status_t (SPAR_CALL *buffer_borrow)(SparEnv *env, SparValue v, uint32_t dtype, uint32_t flags, SparBufferView *out);
    spar_status_t (SPAR_CALL *buffer_release)(SparEnv *env, SparBorrow borrow);
    /* creates a runtime-owned zero-initialised buffer; view is a writable borrow, released by the call scope or buffer_release */
    spar_status_t (SPAR_CALL *buffer_new)(SparEnv *env, uint32_t dtype, uint64_t len_elements, SparBufferView *view, SparValue *out);
    /* adopts native memory; finalizer runs when the buffer dies (never inside a Spar call into the runtime) */
    spar_status_t (SPAR_CALL *buffer_from_external)(SparEnv *env, void *data, uint32_t dtype, uint64_t len_elements,
                                                    SparFinalizer finalize, void *userdata, SparValue *out);
    /* converts a buffer into a list of ints/floats/bools (copies) */
    spar_status_t (SPAR_CALL *buffer_to_list)(SparEnv *env, SparValue buffer, SparValue *out);

    /* lists */
    spar_status_t (SPAR_CALL *list_len)(SparEnv *env, SparValue v, uint64_t *out);
    spar_status_t (SPAR_CALL *list_get)(SparEnv *env, SparValue v, uint64_t index, SparValue *out);
    spar_status_t (SPAR_CALL *list_new)(SparEnv *env, uint64_t capacity, SparValue *out);
    spar_status_t (SPAR_CALL *list_push)(SparEnv *env, SparValue list, SparValue item); /* only lists created in this call */

    /* records */
    /* env may be NULL: interning is process-global and thread-safe (usable in init). */
    spar_status_t (SPAR_CALL *symbol_intern)(SparEnv *env, const uint8_t *name, uint64_t len, SparSymbol *out);
    spar_status_t (SPAR_CALL *record_new)(SparEnv *env, uint64_t capacity, SparValue *out);
    spar_status_t (SPAR_CALL *record_set)(SparEnv *env, SparValue rec, SparSymbol field, SparValue item);
    spar_status_t (SPAR_CALL *record_get)(SparEnv *env, SparValue rec, SparSymbol field, SparValue *out);
    spar_status_t (SPAR_CALL *record_len)(SparEnv *env, SparValue rec, uint64_t *out);

    /* options */
    spar_status_t (SPAR_CALL *option_some)(SparEnv *env, SparValue item, SparValue *out);
    spar_status_t (SPAR_CALL *option_none)(SparEnv *env, SparValue *out);
    spar_status_t (SPAR_CALL *option_get)(SparEnv *env, SparValue opt, uint8_t *is_some, SparValue *out);

    /* persistent references */
    spar_status_t (SPAR_CALL *ref_new)(SparEnv *env, SparValue v, SparRef *out);
    spar_status_t (SPAR_CALL *ref_get)(SparEnv *env, SparRef ref, SparValue *out);
    spar_status_t (SPAR_CALL *ref_drop)(SparRef ref);

    /* native resources */
    spar_status_t (SPAR_CALL *resource_new)(SparEnv *env, uint64_t type_tag, void *ptr,
                                            SparFinalizer finalize, void *userdata, SparValue *out);
    spar_status_t (SPAR_CALL *resource_get)(SparEnv *env, SparValue v, uint64_t type_tag, void **out);
    spar_status_t (SPAR_CALL *resource_close)(SparEnv *env, SparValue v, uint64_t type_tag);

    /* callbacks (Spar callable -> value) */
    spar_status_t (SPAR_CALL *call)(SparEnv *env, SparValue callable, const SparValue *argv, uint64_t argc, SparValue *out);

    /* async: create a completion object for the current call */
    spar_status_t (SPAR_CALL *async_begin)(SparEnv *env, SparAsync **out);
    spar_status_t (SPAR_CALL *async_complete)(SparAsync *a, const uint8_t *json_utf8, uint64_t len); /* completes with a JSON-decoded value */
    spar_status_t (SPAR_CALL *async_fail)(SparAsync *a, const uint8_t *msg, uint64_t len);
    spar_status_t (SPAR_CALL *async_is_cancelled)(SparAsync *a, uint8_t *out);
    spar_status_t (SPAR_CALL *async_release)(SparAsync *a);

    /* appended in later minors ONLY; check struct_size before use */
} SparApiV0;

/* ---- module ---- */
typedef spar_status_t (SPAR_CALL *SparModuleInitFn)(const SparApiV0 *api, SparModule *module, void **out_state);
typedef void (SPAR_CALL *SparModuleQuiesceFn)(void *state);
typedef void (SPAR_CALL *SparModuleDestroyFn)(void *state);

typedef struct SparModuleDescriptor {
    uint32_t struct_size;
    uint16_t abi_major;
    uint16_t min_abi_minor;
    uint64_t required_capabilities;
    uint64_t optional_capabilities;
    const char *module_name; uint64_t module_name_len;
    const char *target; uint64_t target_len; /* e.g. "x86_64-unknown-linux-gnu"; may be empty */
    uint32_t version_major, version_minor, version_patch, reserved0;
    SparModuleInitFn init;
    SparModuleQuiesceFn quiesce; /* may be NULL */
    SparModuleDestroyFn destroy; /* may be NULL */
    uint64_t reserved[4];
} SparModuleDescriptor;

/* The single symbol a module exports. */
SPAR_EXPORT const SparModuleDescriptor *SPAR_CALL spar_native_module_v0(void);
#define SPAR_MODULE_SYMBOL "spar_native_module_v0"

#ifdef __cplusplus
}
#endif
#endif /* SPAR_NATIVE_H */
