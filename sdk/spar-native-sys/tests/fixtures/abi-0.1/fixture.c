/* Built against the frozen ABI 0.1 header. Parameterised with -D for negative loader tests. */
#include <string.h>
#include "spar_native.h"

#ifndef FX_MAJOR
#define FX_MAJOR SPAR_NATIVE_ABI_MAJOR
#endif
#ifndef FX_MIN_MINOR
#define FX_MIN_MINOR 1
#endif
#ifndef FX_CAPS
#define FX_CAPS SPAR_CAP_STRINGS
#endif
#ifndef FX_TARGET
#define FX_TARGET ""
#endif
#ifndef FX_NAME
#define FX_NAME "oldMod"
#endif

static const SparApiV0 *A;
static spar_status_t SPAR_CALL twice(SparEnv *e, void *u, const SparValue *a, uint64_t n, SparValue *o) {
    (void)u; (void)n; int64_t x;
    if (A->int_get(e, a[0], &x)) return SPAR_E_TYPE;
    *o = spar_int(x * 2);
    return SPAR_OK;
}
static spar_status_t SPAR_CALL init(const SparApiV0 *api, SparModule *m, void **st) {
    if (!api || api->abi_major != 0 || api->abi_minor < 1) return SPAR_E_ABI_MISMATCH;
    A = api; *st = NULL;
    static const SparParamSpec p[] = {{"x", 1, "int", 3}};
    SparFunctionSpec s; memset(&s, 0, sizeof s);
    s.struct_size = sizeof s; s.name = "twice"; s.name_len = 5; s.params = p; s.param_count = 1;
    s.ret_type = "int"; s.ret_type_len = 3; s.invoke = twice;
    return A->module_add_function(m, &s);
}
static const SparModuleDescriptor D = {
    sizeof(SparModuleDescriptor), FX_MAJOR, FX_MIN_MINOR, FX_CAPS, 0,
    FX_NAME, sizeof(FX_NAME) - 1, FX_TARGET, sizeof(FX_TARGET) - 1, 1, 0, 0, 0, init, 0, 0, {0, 0, 0, 0}};
SPAR_EXPORT const SparModuleDescriptor *SPAR_CALL spar_native_module_v0(void) { return &D; }
