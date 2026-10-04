// C++ Spar native module: strings, buffers and exception conversion.
#include <algorithm>
#include <cstring>
#include <numeric>
#include <string>
#include <vector>
#include "spar_native.hpp"

static const SparApiV0 *g_api;

static spar_status_t SPAR_CALL tk_upper(SparEnv *env, void *, const SparValue *argv, uint64_t, SparValue *out) {
    return spar::guard(g_api, env, out, [&](spar::Env &e) {
        std::string s(e.get_str(argv[0]));
        std::transform(s.begin(), s.end(), s.begin(), [](unsigned char c) { return std::toupper(c); });
        return e.new_str(s);
    });
}

static spar_status_t SPAR_CALL tk_mean(SparEnv *env, void *, const SparValue *argv, uint64_t, SparValue *out) {
    return spar::guard(g_api, env, out, [&](spar::Env &e) {
        spar::F64Borrow v(e, argv[0], SPAR_BUFFER_READ);
        if (v.size() == 0) throw spar::Error("mean of empty input", SPAR_E_RANGE);
        return spar_float(std::accumulate(v.begin(), v.end(), 0.0) / static_cast<double>(v.size()));
    });
}

static spar_status_t SPAR_CALL tk_throws(SparEnv *env, void *, const SparValue *, uint64_t, SparValue *out) {
    return spar::guard(g_api, env, out, [&](spar::Env &) -> SparValue {
        std::vector<int> v;
        return spar_int(v.at(3)); // std::out_of_range, converted to a Spar error
    });
}

static spar_status_t SPAR_CALL tk_checksum(SparEnv *env, void *, const SparValue *argv, uint64_t, SparValue *out) {
    return spar::guard(g_api, env, out, [&](spar::Env &e) {
        spar::BytesBorrow b(e, argv[0], SPAR_BUFFER_READ | SPAR_BUFFER_NO_COPY);
        uint32_t h = 2166136261u;
        for (auto c : b) h = (h ^ c) * 16777619u;
        return spar_int(static_cast<int64_t>(h & 0x7fff));
    });
}

static spar_status_t add(SparModule *m, const char *name, const SparParamSpec *p, uint64_t n, const char *ret, SparNativeFn fn) {
    SparFunctionSpec s;
    std::memset(&s, 0, sizeof s);
    s.struct_size = sizeof s;
    s.name = name; s.name_len = std::strlen(name);
    s.params = p; s.param_count = n;
    s.ret_type = ret; s.ret_type_len = std::strlen(ret);
    s.invoke = fn;
    return g_api->module_add_function(m, &s);
}

#define P(n, t) {n, sizeof(n) - 1, t, sizeof(t) - 1}
static spar_status_t SPAR_CALL init(const SparApiV0 *api, SparModule *m, void **state) {
    g_api = api;
    *state = nullptr;
    static const SparParamSpec text[] = {P("text", "str")};
    static const SparParamSpec vals[] = {P("values", "Slice<float>")};
    static const SparParamSpec data[] = {P("data", "Bytes")};
    spar_status_t s;
    if ((s = add(m, "upper", text, 1, "str", tk_upper))) return s;
    if ((s = add(m, "mean", vals, 1, "float", tk_mean))) return s;
    if ((s = add(m, "throws", nullptr, 0, "int", tk_throws))) return s;
    if ((s = add(m, "checksum", data, 1, "int", tk_checksum))) return s;
    return SPAR_OK;
}

static const SparModuleDescriptor D = {
    sizeof(SparModuleDescriptor), SPAR_NATIVE_ABI_MAJOR, SPAR_NATIVE_ABI_MINOR,
    SPAR_CAP_STRINGS | SPAR_CAP_BYTES | SPAR_CAP_TYPED_ARRAYS, 0,
    "textKit", 7, "", 0, 0, 1, 0, 0, init, nullptr, nullptr, {0, 0, 0, 0}};

extern "C" SPAR_EXPORT const SparModuleDescriptor *SPAR_CALL spar_native_module_v1(void) { return &D; }
