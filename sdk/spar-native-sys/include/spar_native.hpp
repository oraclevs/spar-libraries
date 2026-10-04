// spar_native.hpp - thin C++17 RAII layer over spar_native.h (ABI 0, experimental).
// Nothing here is ABI: it only calls the C table. Exceptions never cross the boundary.
#ifndef SPAR_NATIVE_HPP
#define SPAR_NATIVE_HPP

#include <cstdint>
#include <exception>
#include <stdexcept>
#include <string>
#include <string_view>
#include "spar_native.h"

namespace spar {

struct Error : std::runtime_error {
    spar_status_t status;
    explicit Error(const std::string &m, spar_status_t s = SPAR_E_ERROR) : std::runtime_error(m), status(s) {}
};

class Env {
public:
    Env(const SparApiV0 *api, SparEnv *env) : api_(api), env_(env) {}
    const SparApiV0 *api() const { return api_; }
    SparEnv *raw() const { return env_; }

    void check(spar_status_t s, const char *what) const {
        if (s != SPAR_OK) throw Error(std::string(what) + " failed", s);
    }
    int64_t get_int(SparValue v) const { int64_t x; check(api_->int_get(env_, v, &x), "int_get"); return x; }
    double get_float(SparValue v) const { double x; check(api_->float_get(env_, v, &x), "float_get"); return x; }
    // Valid until the native function returns.
    std::string_view get_str(SparValue v) const {
        SparStrView sv; check(api_->string_view(env_, v, &sv), "string_view");
        return {reinterpret_cast<const char *>(sv.ptr), static_cast<size_t>(sv.len)};
    }
    SparValue new_str(std::string_view s) const {
        SparValue out; check(api_->string_new(env_, reinterpret_cast<const uint8_t *>(s.data()), s.size(), &out), "string_new");
        return out;
    }
    void set_error(const std::string &m, spar_status_t kind = SPAR_E_ERROR) const {
        api_->error_set(env_, kind, reinterpret_cast<const uint8_t *>(m.data()), m.size());
    }

private:
    const SparApiV0 *api_;
    SparEnv *env_;
};

// RAII buffer borrow: released in the destructor.
template <typename T, uint32_t Dtype>
class Borrow {
public:
    Borrow(const Env &env, SparValue v, uint32_t flags) : env_(env) {
        view_.struct_size = sizeof(view_);
        env.check(env.api()->buffer_borrow(env.raw(), v, Dtype, flags, &view_), "buffer_borrow");
        live_ = true;
    }
    ~Borrow() { if (live_) env_.api()->buffer_release(env_.raw(), view_.borrow); }
    Borrow(const Borrow &) = delete;
    Borrow &operator=(const Borrow &) = delete;
    T *data() const { return static_cast<T *>(view_.data); }
    size_t size() const { return static_cast<size_t>(view_.len_elements); }
    T *begin() const { return data(); }
    T *end() const { return data() + size(); }

private:
    const Env &env_;
    SparBufferView view_{};
    bool live_ = false;
};

using F64Borrow = Borrow<double, SPAR_DTYPE_F64>;
using BytesBorrow = Borrow<const uint8_t, SPAR_DTYPE_U8>;

// Runs `f(Env&)` -> SparValue, converting any C++ exception into a Spar error status.
template <typename F>
spar_status_t guard(const SparApiV0 *api, SparEnv *env, SparValue *out, F &&f) noexcept {
    Env e(api, env);
    try {
        *out = f(e);
        return SPAR_OK;
    } catch (const Error &err) {
        e.set_error(err.what(), err.status);
        return err.status;
    } catch (const std::exception &ex) {
        e.set_error(std::string("C++ exception: ") + ex.what());
        return SPAR_E_PANIC;
    } catch (...) {
        e.set_error("unknown C++ exception");
        return SPAR_E_PANIC;
    }
}

} // namespace spar
#endif
