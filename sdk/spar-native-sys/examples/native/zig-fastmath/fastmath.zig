// Zig Spar native module. Build (needs Zig >= 0.13):
//   zig build-lib -dynamic -O ReleaseFast -lc -I ../../../include fastmath.zig
// Status: NOT compiled in the repository's CI environment (no Zig toolchain installed there).
// It targets the same C ABI as the C/C++/Rust modules and uses no Zig-specific ABI.
const c = @cImport(@cInclude("spar_native.h"));

var api: *const c.SparApiV0 = undefined;

fn add(env: ?*c.SparEnv, ud: ?*anyopaque, argv: [*c]const c.SparValue, argc: u64, out: [*c]c.SparValue) callconv(.C) c.spar_status_t {
    _ = ud;
    _ = argc;
    var a: i64 = 0;
    var b: i64 = 0;
    var s = api.int_get.?(env, argv[0], &a);
    if (s != c.SPAR_OK) return s;
    s = api.int_get.?(env, argv[1], &b);
    if (s != c.SPAR_OK) return s;
    out.* = c.spar_int(a +% b);
    return c.SPAR_OK;
}

fn sumF64(env: ?*c.SparEnv, ud: ?*anyopaque, argv: [*c]const c.SparValue, argc: u64, out: [*c]c.SparValue) callconv(.C) c.spar_status_t {
    _ = ud;
    _ = argc;
    var view = std.mem.zeroes(c.SparBufferView);
    view.struct_size = @sizeOf(c.SparBufferView);
    const s = api.buffer_borrow.?(env, argv[0], c.SPAR_DTYPE_F64, c.SPAR_BUFFER_READ, &view);
    if (s != c.SPAR_OK) return s;
    const p: [*]const f64 = @ptrCast(@alignCast(view.data));
    var total: f64 = 0;
    for (p[0..view.len_elements]) |x| total += x;
    _ = api.buffer_release.?(env, view.borrow);
    out.* = c.spar_float(total);
    return c.SPAR_OK;
}

const std = @import("std");

fn reg(m: ?*c.SparModule, name: []const u8, params: []const c.SparParamSpec, ret: []const u8, f: c.SparNativeFn) c.spar_status_t {
    var spec = std.mem.zeroes(c.SparFunctionSpec);
    spec.struct_size = @sizeOf(c.SparFunctionSpec);
    spec.name = name.ptr;
    spec.name_len = name.len;
    spec.params = params.ptr;
    spec.param_count = params.len;
    spec.ret_type = ret.ptr;
    spec.ret_type_len = ret.len;
    spec.invoke = f;
    return api.module_add_function.?(m, &spec);
}

fn param(comptime n: []const u8, comptime t: []const u8) c.SparParamSpec {
    return .{ .name = n.ptr, .name_len = n.len, .type = t.ptr, .type_len = t.len };
}

fn init(a: [*c]const c.SparApiV0, m: ?*c.SparModule, state: [*c]?*anyopaque) callconv(.C) c.spar_status_t {
    api = a;
    state.* = null;
    const ab = [_]c.SparParamSpec{ param("a", "int"), param("b", "int") };
    const vals = [_]c.SparParamSpec{param("values", "Slice<float>")};
    var s = reg(m, "add", &ab, "int", add);
    if (s != c.SPAR_OK) return s;
    s = reg(m, "sumF64", &vals, "float", sumF64);
    return s;
}

var descriptor = std.mem.zeroes(c.SparModuleDescriptor);

export fn spar_native_module_v1() *const c.SparModuleDescriptor {
    descriptor.struct_size = @sizeOf(c.SparModuleDescriptor);
    descriptor.abi_major = c.SPAR_NATIVE_ABI_MAJOR;
    descriptor.min_abi_minor = c.SPAR_NATIVE_ABI_MINOR;
    descriptor.required_capabilities = c.SPAR_CAP_STRINGS | c.SPAR_CAP_TYPED_ARRAYS;
    descriptor.module_name = "zigMath";
    descriptor.module_name_len = 7;
    descriptor.init = init;
    return &descriptor;
}
