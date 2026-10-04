/* Benchmark kernels: plain C ABI functions (baseline) and the same work behind the Spar Native ABI. */
#include <math.h>
#include <stdlib.h>
#include <string.h>
#include "spar_native.h"

/* ---- raw C ABI baselines (no Spar involved) ---- */
SPAR_EXPORT int64_t bench_raw_noop(void) { return 0; }
SPAR_EXPORT int64_t bench_raw_add(int64_t a, int64_t b) { return a + b; }
SPAR_EXPORT double bench_raw_sum_f64(const double *p, size_t n) {
    double s = 0; for (size_t i = 0; i < n; i++) s += p[i]; return s;
}
SPAR_EXPORT uint64_t bench_raw_sum_u8(const uint8_t *p, size_t n) {
    uint64_t s = 0; for (size_t i = 0; i < n; i++) s += p[i]; return s;
}

static const SparApiV0 *A;

static spar_status_t SPAR_CALL b_noop(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)e; (void)u; (void)a; (void)n; *o = spar_void(); return SPAR_OK;
}
static spar_status_t SPAR_CALL b_add(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; int64_t x, y;
    if (A->int_get(e, a[0], &x) || A->int_get(e, a[1], &y)) return SPAR_E_TYPE;
    *o = spar_int(x + y); return SPAR_OK;
}
static spar_status_t SPAR_CALL b_add4(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; int64_t v[4];
    for (int i = 0; i < 4; i++) if (A->int_get(e, a[i], &v[i])) return SPAR_E_TYPE;
    *o = spar_int(v[0] + v[1] + v[2] + v[3]); return SPAR_OK;
}
static spar_status_t SPAR_CALL b_add_f(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; double x, y;
    if (A->float_get(e, a[0], &x) || A->float_get(e, a[1], &y)) return SPAR_E_TYPE;
    *o = spar_float(x + y); return SPAR_OK;
}
static spar_status_t SPAR_CALL b_str_len(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; SparStrView v;
    if (A->string_view(e, a[0], &v)) return SPAR_E_TYPE;
    *o = spar_int((int64_t)v.len); return SPAR_OK;
}
static spar_status_t SPAR_CALL b_str_copy(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; SparStrView v;
    if (A->string_view(e, a[0], &v)) return SPAR_E_TYPE;
    return A->string_new(e, v.ptr, v.len, o);
}
static spar_status_t SPAR_CALL b_sum_bytes(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; SparBufferView b = {0};
    if (A->buffer_borrow(e, a[0], SPAR_DTYPE_U8, SPAR_BUFFER_READ | SPAR_BUFFER_NO_COPY, &b)) return SPAR_E_TYPE;
    *o = spar_int((int64_t)bench_raw_sum_u8((const uint8_t *)b.data, b.len_elements));
    A->buffer_release(e, b.borrow); return SPAR_OK;
}
static spar_status_t SPAR_CALL b_sum_list(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; SparBufferView b = {0};
    if (A->buffer_borrow(e, a[0], SPAR_DTYPE_F64, SPAR_BUFFER_READ, &b)) return SPAR_E_TYPE;
    *o = spar_float(bench_raw_sum_f64((const double *)b.data, b.len_elements));
    A->buffer_release(e, b.borrow); return SPAR_OK;
}
static spar_status_t SPAR_CALL b_sum_buf(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; SparBufferView b = {0};
    if (A->buffer_borrow(e, a[0], SPAR_DTYPE_F64, SPAR_BUFFER_READ | SPAR_BUFFER_NO_COPY, &b)) return SPAR_E_TYPE;
    *o = spar_float(bench_raw_sum_f64((const double *)b.data, b.len_elements));
    A->buffer_release(e, b.borrow); return SPAR_OK;
}
/* makeBuf(n): buffer of n f64 with value i */
static spar_status_t SPAR_CALL b_make_buf(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; int64_t len; SparBufferView v = {0};
    if (A->int_get(e, a[0], &len) || len < 0) return SPAR_E_RANGE;
    spar_status_t s = A->buffer_new(e, SPAR_DTYPE_F64, (uint64_t)len, &v, o);
    if (s) return s;
    double *p = (double *)v.data;
    for (int64_t i = 0; i < len; i++) p[i] = (double)i;
    return A->buffer_release(e, v.borrow);
}
/* scaleBuf(buf, k): in-place mutation through a writable borrow */
static spar_status_t SPAR_CALL b_scale_buf(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; double k; SparBufferView b = {0};
    if (A->float_get(e, a[1], &k)) return SPAR_E_TYPE;
    spar_status_t s = A->buffer_borrow(e, a[0], SPAR_DTYPE_F64, SPAR_BUFFER_READ | SPAR_BUFFER_WRITE | SPAR_BUFFER_NO_COPY, &b);
    if (s) return s;
    double *p = (double *)b.data;
    for (uint64_t i = 0; i < b.len_elements; i++) p[i] *= k;
    *o = spar_void();
    return A->buffer_release(e, b.borrow);
}
/* floatsList(n): list of n floats built through list_push (per-element ABI path, the slow way) */
static spar_status_t SPAR_CALL b_floats_list(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; int64_t len; SparValue l;
    if (A->int_get(e, a[0], &len) || len < 0) return SPAR_E_RANGE;
    if (A->list_new(e, (uint64_t)len, &l)) return SPAR_E_OOM;
    for (int64_t i = 0; i < len; i++) if (A->list_push(e, l, spar_float((double)i))) return SPAR_E_ERROR;
    *o = l; return SPAR_OK;
}
/* allocBytes(n): native allocation returned as bytes (copy) */
static spar_status_t SPAR_CALL b_alloc_bytes(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; int64_t len;
    if (A->int_get(e, a[0], &len) || len < 0) return SPAR_E_RANGE;
    uint8_t *p = (uint8_t *)calloc((size_t)len ? (size_t)len : 1, 1);
    if (!p) return SPAR_E_OOM;
    spar_status_t s = A->bytes_new(e, p, (uint64_t)len, o);
    free(p); return s;
}
/* fieldSum(rec): sum of `x` and `y` fields, cached symbols */
static SparSymbol SX, SY;
static spar_status_t SPAR_CALL b_field_sum(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; SparValue x, y; int64_t xi, yi;
    if (A->record_get(e, a[0], SX, &x) || A->record_get(e, a[0], SY, &y)) return SPAR_E_RANGE;
    if (A->int_get(e, x, &xi) || A->int_get(e, y, &yi)) return SPAR_E_TYPE;
    *o = spar_int(xi + yi); return SPAR_OK;
}
/* resource: counter behind an opaque handle */
#define TAG_COUNTER 0x434e5452u
static void SPAR_CALL counter_free(void *p, void *u) { (void)u; free(p); }
static spar_status_t SPAR_CALL b_counter_new(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)a; (void)n; int64_t *c = (int64_t *)calloc(1, sizeof *c);
    if (!c) return SPAR_E_OOM;
    return A->resource_new(e, TAG_COUNTER, c, counter_free, NULL, o);
}
static spar_status_t SPAR_CALL b_counter_bump(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; void *p;
    spar_status_t s = A->resource_get(e, a[0], TAG_COUNTER, &p);
    if (s) return s;
    *o = spar_int(++*(int64_t *)p); return SPAR_OK;
}

#define S(x) x, sizeof(x) - 1
static spar_status_t reg(SparModule *m, const char *name, const SparParamSpec *p, uint64_t n, const char *ret, SparNativeFn fn) {
    SparFunctionSpec s; memset(&s, 0, sizeof s);
    s.struct_size = sizeof s; s.name = name; s.name_len = strlen(name);
    s.params = p; s.param_count = n; s.ret_type = ret; s.ret_type_len = strlen(ret); s.invoke = fn;
    return A->module_add_function(m, &s);
}
/* direct signature: plain C function, no SparValue */
static spar_status_t reg_direct(SparModule *m, const char *name, const SparParamSpec *p, uint64_t n, const char *ret,
                                const void *fn, const char *sig) {
    SparFunctionSpec s; memset(&s, 0, sizeof s);
    s.struct_size = sizeof s; s.name = name; s.name_len = strlen(name);
    s.params = p; s.param_count = n; s.ret_type = ret; s.ret_type_len = strlen(ret);
    s.direct = fn; s.direct_sig = sig; s.direct_sig_len = strlen(sig);
    return A->module_add_function(m, &s);
}
SPAR_EXPORT int64_t bench_raw_add4(int64_t a, int64_t b, int64_t c, int64_t d) { return a + b + c + d; }
SPAR_EXPORT double bench_raw_add_f(double a, double b) { return a + b; }
static spar_status_t SPAR_CALL init(const SparApiV0 *api, SparModule *m, void **st) {
    A = api; *st = NULL;
    static const SparParamSpec ab[] = {{S("a"), S("int")}, {S("b"), S("int")}};
    static const SparParamSpec abcd[] = {{S("a"), S("int")}, {S("b"), S("int")}, {S("c"), S("int")}, {S("d"), S("int")}};
    static const SparParamSpec xy[] = {{S("x"), S("float")}, {S("y"), S("float")}};
    static const SparParamSpec text[] = {{S("text"), S("str")}};
    static const SparParamSpec data[] = {{S("data"), S("Bytes")}};
    static const SparParamSpec vals[] = {{S("values"), S("[float]")}};
    static const SparParamSpec buf[] = {{S("buf"), S("Buffer")}};
    static const SparParamSpec bufk[] = {{S("buf"), S("Buffer")}, {S("k"), S("float")}};
    static const SparParamSpec nn[] = {{S("n"), S("int")}};
    static const SparParamSpec rec[] = {{S("rec"), S("Record")}};
    static const SparParamSpec ctr[] = {{S("counter"), S("Counter")}};
    if (A->symbol_intern(NULL, (const uint8_t *)"x", 1, &SX) || A->symbol_intern(NULL, (const uint8_t *)"y", 1, &SY))
        return SPAR_E_ERROR; /* symbol_intern is process-global: env may be NULL (init time) */
    if (A->struct_size >= sizeof(SparApiV0) && A->module_add_type(m, "Counter", 7)) return SPAR_E_ERROR;
    if (reg(m, "noop", NULL, 0, "void", b_noop)) return SPAR_E_ERROR;
    if (reg_direct(m, "noopD", NULL, 0, "int", (const void *)bench_raw_noop, ">i")) return SPAR_E_ERROR;
    if (reg_direct(m, "addD", ab, 2, "int", (const void *)bench_raw_add, "ii>i")) return SPAR_E_ERROR;
    if (reg_direct(m, "add4D", abcd, 4, "int", (const void *)bench_raw_add4, "iiii>i")) return SPAR_E_ERROR;
    if (reg_direct(m, "addFD", xy, 2, "float", (const void *)bench_raw_add_f, "ff>f")) return SPAR_E_ERROR;
    if (reg(m, "add", ab, 2, "int", b_add)) return SPAR_E_ERROR;
    if (reg(m, "add4", abcd, 4, "int", b_add4)) return SPAR_E_ERROR;
    if (reg(m, "addF", xy, 2, "float", b_add_f)) return SPAR_E_ERROR;
    if (reg(m, "strLen", text, 1, "int", b_str_len)) return SPAR_E_ERROR;
    if (reg(m, "strCopy", text, 1, "str", b_str_copy)) return SPAR_E_ERROR;
    if (reg(m, "sumBytes", data, 1, "int", b_sum_bytes)) return SPAR_E_ERROR;
    if (reg(m, "sumList", vals, 1, "float", b_sum_list)) return SPAR_E_ERROR;
    if (reg(m, "sumBuf", buf, 1, "float", b_sum_buf)) return SPAR_E_ERROR;
    if (reg(m, "makeBuf", nn, 1, "Buffer", b_make_buf)) return SPAR_E_ERROR;
    if (reg(m, "scaleBuf", bufk, 2, "void", b_scale_buf)) return SPAR_E_ERROR;
    if (reg(m, "floatsList", nn, 1, "[float]", b_floats_list)) return SPAR_E_ERROR;
    if (reg(m, "allocBytes", nn, 1, "Bytes", b_alloc_bytes)) return SPAR_E_ERROR;
    if (reg(m, "fieldSum", rec, 1, "int", b_field_sum)) return SPAR_E_ERROR;
    if (reg(m, "counterNew", NULL, 0, "Counter", b_counter_new)) return SPAR_E_ERROR;
    if (reg(m, "counterBump", ctr, 1, "int", b_counter_bump)) return SPAR_E_ERROR;
    return SPAR_OK;
}
static const SparModuleDescriptor D = {
    .struct_size = sizeof(SparModuleDescriptor), .abi_major = SPAR_NATIVE_ABI_MAJOR, .min_abi_minor = SPAR_NATIVE_ABI_MINOR,
    .required_capabilities = SPAR_CAP_STRINGS | SPAR_CAP_BYTES | SPAR_CAP_TYPED_ARRAYS | SPAR_CAP_RECORDS | SPAR_CAP_NATIVE_RESOURCES | SPAR_CAP_DIRECT_CALLS,
    .module_name = "benchkit", .module_name_len = 8, .version_minor = 1, .init = init,
};
SPAR_EXPORT const SparModuleDescriptor *SPAR_CALL spar_native_module_v1(void) { return &D; }
