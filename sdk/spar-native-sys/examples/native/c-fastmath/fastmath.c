#define _POSIX_C_SOURCE 200809L
/* Example Spar native module written in plain C against spar_native.h only. */
#include <math.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <pthread.h>
#include <time.h>
#endif
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "spar_native.h"

static const SparApiV0 *g_api;

static spar_status_t fail(SparEnv *env, spar_status_t code, const char *msg) {
    g_api->error_set(env, code, (const uint8_t *)msg, strlen(msg));
    return code;
}

static spar_status_t SPAR_CALL fm_add(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc;
    int64_t a, b, r;
    spar_status_t s;
    if ((s = g_api->int_get(env, argv[0], &a)) != SPAR_OK) return s;
    if ((s = g_api->int_get(env, argv[1], &b)) != SPAR_OK) return s;
    if ((b > 0 && a > INT64_MAX - b) || (b < 0 && a < INT64_MIN - b))
        return fail(env, SPAR_E_RANGE, "add overflows int");
    r = a + b;
    *out = spar_int(r);
    return SPAR_OK;
}

static spar_status_t SPAR_CALL fm_hypot(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc;
    double x, y;
    spar_status_t s;
    if ((s = g_api->float_get(env, argv[0], &x)) != SPAR_OK) return s;
    if ((s = g_api->float_get(env, argv[1], &y)) != SPAR_OK) return s;
    *out = spar_float(hypot(x, y));
    return SPAR_OK;
}

static spar_status_t SPAR_CALL fm_str_len(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc;
    SparStrView v;
    spar_status_t s = g_api->string_view(env, argv[0], &v);
    if (s != SPAR_OK) return s;
    *out = spar_int((int64_t)v.len);
    return SPAR_OK;
}

static spar_status_t SPAR_CALL fm_shout(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc;
    SparStrView v;
    spar_status_t s = g_api->string_view(env, argv[0], &v);
    if (s != SPAR_OK) return s;
    uint8_t *tmp = (uint8_t *)malloc(v.len + 1);
    if (!tmp) return SPAR_E_OOM;
    for (uint64_t i = 0; i < v.len; i++) tmp[i] = (v.ptr[i] >= 'a' && v.ptr[i] <= 'z') ? v.ptr[i] - 32 : v.ptr[i];
    tmp[v.len] = '!';
    s = g_api->string_new(env, tmp, v.len + 1, out);
    free(tmp);
    return s;
}

static spar_status_t SPAR_CALL fm_sum_bytes(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc;
    SparBufferView b = {0};
    spar_status_t s = g_api->buffer_borrow(env, argv[0], SPAR_DTYPE_U8, SPAR_BUFFER_READ | SPAR_BUFFER_NO_COPY, &b);
    if (s != SPAR_OK) return s;
    const uint8_t *p = (const uint8_t *)b.data;
    int64_t sum = 0;
    for (uint64_t i = 0; i < b.len_elements; i++) sum += p[i];
    g_api->buffer_release(env, b.borrow);
    *out = spar_int(sum);
    return SPAR_OK;
}

static spar_status_t SPAR_CALL fm_sum_f64(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc;
    SparBufferView b = {0};
    spar_status_t s = g_api->buffer_borrow(env, argv[0], SPAR_DTYPE_F64, SPAR_BUFFER_READ, &b);
    if (s != SPAR_OK) return s;
    const double *p = (const double *)b.data;
    double sum = 0;
    for (uint64_t i = 0; i < b.len_elements; i++) sum += p[i];
    g_api->buffer_release(env, b.borrow);
    *out = spar_float(sum);
    return SPAR_OK;
}

static spar_status_t SPAR_CALL fm_iota(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc;
    int64_t n;
    spar_status_t s = g_api->int_get(env, argv[0], &n);
    if (s != SPAR_OK) return s;
    if (n < 0) return fail(env, SPAR_E_RANGE, "iota: n must be non-negative");
    SparValue list;
    if ((s = g_api->list_new(env, (uint64_t)n, &list)) != SPAR_OK) return s;
    for (int64_t i = 0; i < n; i++)
        if ((s = g_api->list_push(env, list, spar_int(i))) != SPAR_OK) return s;
    *out = list;
    return SPAR_OK;
}

/* mapInts(values, f): calls the Spar callable once per element (the slow per-element pattern). */
static spar_status_t SPAR_CALL fm_map_ints(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc;
    uint64_t n;
    spar_status_t s = g_api->list_len(env, argv[0], &n);
    if (s != SPAR_OK) return s;
    SparValue result;
    if ((s = g_api->list_new(env, n, &result)) != SPAR_OK) return s;
    for (uint64_t i = 0; i < n; i++) {
        SparValue item, mapped;
        if ((s = g_api->list_get(env, argv[0], i, &item)) != SPAR_OK) return s;
        if ((s = g_api->call(env, argv[1], &item, 1, &mapped)) != SPAR_OK) return s;
        if ((s = g_api->list_push(env, result, mapped)) != SPAR_OK) return s;
    }
    *out = result;
    return SPAR_OK;
}

/* ---- async: completion from a native worker thread ---- */
typedef struct { SparAsync *op; int64_t value; int64_t millis; int mode; } AsyncJob;
static volatile int g_second_status = -1;

static void async_worker_body(AsyncJob *job) {
#ifdef _WIN32
    Sleep((DWORD)(job->millis > 0 ? job->millis : 0));
#else
    struct timespec ts = { job->millis / 1000, (job->millis % 1000) * 1000000L };
    nanosleep(&ts, NULL);
#endif
    char buf[64];
    if (job->mode == 1) { /* fail */
        const char *m = "async failure from C worker";
        g_api->async_fail(job->op, (const uint8_t *)m, strlen(m));
    } else if (job->mode == 2) { /* abandon: release without completing */
    } else {
        int n = snprintf(buf, sizeof buf, "%lld", (long long)job->value);
        g_api->async_complete(job->op, (const uint8_t *)buf, (uint64_t)n);
        /* protocol violation: settling twice must be rejected, not crash */
        g_second_status = g_api->async_complete(job->op, (const uint8_t *)buf, (uint64_t)n);
    }
    g_api->async_release(job->op);
    free(job);
}

#ifdef _WIN32
static DWORD WINAPI async_worker(void *p) {
    async_worker_body((AsyncJob *)p);
    return 0;
}
#else
static void *async_worker(void *p) {
    async_worker_body((AsyncJob *)p);
    return NULL;
}
#endif

static spar_status_t start_async(SparEnv *env, int64_t value, int64_t millis, int mode) {
    SparAsync *op;
    spar_status_t s = g_api->async_begin(env, &op);
    if (s != SPAR_OK) return s;
    AsyncJob *job = (AsyncJob *)malloc(sizeof *job);
    if (!job) { g_api->async_release(op); return SPAR_E_OOM; }
    job->op = op; job->value = value; job->millis = millis; job->mode = mode;
#ifdef _WIN32
    HANDLE thread = CreateThread(NULL, 0, async_worker, job, 0, NULL);
    if (!thread) { g_api->async_release(op); free(job); return SPAR_E_ERROR; }
    CloseHandle(thread);
#else
    pthread_t thread;
    if (pthread_create(&thread, NULL, async_worker, job) != 0) {
        g_api->async_release(op);
        free(job);
        return SPAR_E_ERROR;
    }
    pthread_detach(thread);
#endif
    return SPAR_OK;
}

static spar_status_t SPAR_CALL fm_delayed_add(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc; (void)out;
    int64_t a, b, ms;
    if (g_api->int_get(env, argv[0], &a) || g_api->int_get(env, argv[1], &b) || g_api->int_get(env, argv[2], &ms)) return SPAR_E_TYPE;
    return start_async(env, a + b, ms, 0);
}
static spar_status_t SPAR_CALL fm_delayed_fail(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc; (void)out; int64_t ms;
    if (g_api->int_get(env, argv[0], &ms)) return SPAR_E_TYPE;
    return start_async(env, 0, ms, 1);
}
static spar_status_t SPAR_CALL fm_abandoned(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc; (void)out; (void)argv;
    return start_async(env, 0, 10, 2);
}
static spar_status_t SPAR_CALL fm_second_status(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)env; (void)ud; (void)argc; (void)argv;
    *out = spar_int(g_second_status);
    return SPAR_OK;
}

static spar_status_t SPAR_CALL fm_fail(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argv; (void)argc; (void)out;
    return fail(env, SPAR_E_ERROR, "deliberate failure from C");
}

static spar_status_t SPAR_CALL fm_pair(SparEnv *env, void *ud, const SparValue *argv, uint64_t argc, SparValue *out) {
    (void)ud; (void)argc;
    SparSymbol sum_f, prod_f;
    SparValue rec;
    int64_t a, b;
    spar_status_t s;
    if ((s = g_api->int_get(env, argv[0], &a)) != SPAR_OK) return s;
    if ((s = g_api->int_get(env, argv[1], &b)) != SPAR_OK) return s;
    if ((s = g_api->symbol_intern(env, (const uint8_t *)"sum", 3, &sum_f)) != SPAR_OK) return s;
    if ((s = g_api->symbol_intern(env, (const uint8_t *)"product", 7, &prod_f)) != SPAR_OK) return s;
    if ((s = g_api->record_new(env, 2, &rec)) != SPAR_OK) return s;
    if ((s = g_api->record_set(env, rec, sum_f, spar_int(a + b))) != SPAR_OK) return s;
    if ((s = g_api->record_set(env, rec, prod_f, spar_int(a * b))) != SPAR_OK) return s;
    *out = rec;
    return SPAR_OK;
}

static int64_t fm_direct_mul(int64_t a, int64_t b) { return a * b; }
static double fm_direct_scale(double x, double k, uint8_t neg) { return neg ? -x * k : x * k; }

#define STR(x) x, sizeof(x) - 1
static uint32_t g_flags;
static spar_status_t add_fn(SparModule *m, const char *name, const SparParamSpec *params, uint64_t n,
                            const char *ret, SparNativeFn fn) {
    SparFunctionSpec spec;
    memset(&spec, 0, sizeof spec);
    spec.flags = g_flags;
    spec.struct_size = sizeof spec;
    spec.name = name; spec.name_len = strlen(name);
    spec.params = params; spec.param_count = n;
    spec.ret_type = ret; spec.ret_type_len = strlen(ret);
    spec.invoke = fn;
    return g_api->module_add_function(m, &spec);
}

static spar_status_t SPAR_CALL module_init(const SparApiV0 *api, SparModule *m, void **state) {
    g_api = api;
    *state = NULL;
    static const SparParamSpec ab_int[] = {{STR("a"), STR("int")}, {STR("b"), STR("int")}};
    static const SparParamSpec xy[] = {{STR("x"), STR("float")}, {STR("y"), STR("float")}};
    static const SparParamSpec text[] = {{STR("text"), STR("str")}};
    static const SparParamSpec data[] = {{STR("data"), STR("Bytes")}};
    static const SparParamSpec values[] = {{STR("values"), STR("Slice<float>")}};
    static const SparParamSpec n_int[] = {{STR("n"), STR("int")}};
    spar_status_t s;
    if ((s = add_fn(m, "add", ab_int, 2, "int", fm_add)) != SPAR_OK) return s;
    if ((s = add_fn(m, "hypot", xy, 2, "float", fm_hypot)) != SPAR_OK) return s;
    if ((s = add_fn(m, "strLen", text, 1, "int", fm_str_len)) != SPAR_OK) return s;
    if ((s = add_fn(m, "shout", text, 1, "str", fm_shout)) != SPAR_OK) return s;
    if ((s = add_fn(m, "sumBytes", data, 1, "int", fm_sum_bytes)) != SPAR_OK) return s;
    if ((s = add_fn(m, "sumF64", values, 1, "float", fm_sum_f64)) != SPAR_OK) return s;
    if ((s = add_fn(m, "iota", n_int, 1, "[int]", fm_iota)) != SPAR_OK) return s;
    if ((s = add_fn(m, "pair", ab_int, 2, "Record", fm_pair)) != SPAR_OK) return s;
    if ((s = add_fn(m, "fail", NULL, 0, "int", fm_fail)) != SPAR_OK) return s;
    static const SparParamSpec map_params[] = {{STR("values"), STR("[int]")}, {STR("f"), STR("fn(value: int) -> int")}};
    static const SparParamSpec da[] = {{STR("a"), STR("int")}, {STR("b"), STR("int")}, {STR("millis"), STR("int")}};
    static const SparParamSpec ms[] = {{STR("millis"), STR("int")}};
    {
        static const SparParamSpec scl[] = {{STR("x"), STR("float")}, {STR("k"), STR("float")}, {STR("negate"), STR("bool")}};
        SparFunctionSpec d;
        memset(&d, 0, sizeof d);
        d.struct_size = sizeof d;
        d.name = "mulD"; d.name_len = 4; d.params = ab_int; d.param_count = 2; d.ret_type = "int"; d.ret_type_len = 3;
        d.direct = (const void *)fm_direct_mul; d.direct_sig = "ii>i"; d.direct_sig_len = 4;
        if ((s = g_api->module_add_function(m, &d)) != SPAR_OK) return s;
        memset(&d, 0, sizeof d);
        d.struct_size = sizeof d;
        d.name = "scaleD"; d.name_len = 6; d.params = scl; d.param_count = 3; d.ret_type = "float"; d.ret_type_len = 5;
        d.direct = (const void *)fm_direct_scale; d.direct_sig = "ffb>f"; d.direct_sig_len = 5;
        if ((s = g_api->module_add_function(m, &d)) != SPAR_OK) return s;
    }
    g_flags = SPAR_FN_ASYNC;
    if ((s = add_fn(m, "delayedAdd", da, 3, "Promise<int>", fm_delayed_add)) != SPAR_OK) return s;
    if ((s = add_fn(m, "delayedFail", ms, 1, "Promise<int>", fm_delayed_fail)) != SPAR_OK) return s;
    if ((s = add_fn(m, "abandoned", NULL, 0, "Promise<int>", fm_abandoned)) != SPAR_OK) return s;
    g_flags = 0;
    if ((s = add_fn(m, "secondStatus", NULL, 0, "int", fm_second_status)) != SPAR_OK) return s;
    g_flags = SPAR_FN_CALLS;
    s = add_fn(m, "mapInts", map_params, 2, "[int]", fm_map_ints);
    g_flags = 0;
    if (s != SPAR_OK) return s;
    return SPAR_OK;
}

static const SparModuleDescriptor DESCRIPTOR = {
    .struct_size = sizeof(SparModuleDescriptor),
    .abi_major = SPAR_NATIVE_ABI_MAJOR,
    .min_abi_minor = SPAR_NATIVE_ABI_MINOR,
    .required_capabilities = SPAR_CAP_STRINGS | SPAR_CAP_BYTES | SPAR_CAP_LISTS | SPAR_CAP_RECORDS | SPAR_CAP_CALLBACKS | SPAR_CAP_ASYNC,
    .module_name = "fastMath", .module_name_len = 8,
    .target = "", .target_len = 0,
    .version_major = 0, .version_minor = 1, .version_patch = 0,
    .init = module_init,
};

SPAR_EXPORT const SparModuleDescriptor *SPAR_CALL spar_native_module_v1(void) { return &DESCRIPTOR; }
