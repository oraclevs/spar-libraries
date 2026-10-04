/* Prints layout facts of the public header; compared with the Rust mirror by abi_golden.rs. */
#include <stdio.h>
#include <stddef.h>
#include "spar_native.h"

#define SIZE(T) printf("size %s %zu %zu\n", #T, sizeof(T), _Alignof(T))
#define OFF(T, F) printf("off %s.%s %zu\n", #T, #F, offsetof(T, F))

int main(void) {
    SIZE(SparValue); SIZE(SparStrView); SIZE(SparBufferView); SIZE(SparParamSpec);
    SIZE(SparFunctionSpec); SIZE(SparModuleDescriptor); SIZE(SparApiV0);
    OFF(SparValue, tag); OFF(SparValue, flags); OFF(SparValue, payload);
    OFF(SparBufferView, struct_size); OFF(SparBufferView, flags); OFF(SparBufferView, data);
    OFF(SparBufferView, len_elements); OFF(SparBufferView, len_bytes); OFF(SparBufferView, dtype);
    OFF(SparBufferView, ndim); OFF(SparBufferView, stride_bytes); OFF(SparBufferView, borrow);
    OFF(SparFunctionSpec, name); OFF(SparFunctionSpec, params); OFF(SparFunctionSpec, ret_type);
    OFF(SparFunctionSpec, invoke); OFF(SparFunctionSpec, userdata); OFF(SparFunctionSpec, direct);
    OFF(SparFunctionSpec, direct_sig); OFF(SparFunctionSpec, reserved);
    OFF(SparModuleDescriptor, required_capabilities); OFF(SparModuleDescriptor, module_name);
    OFF(SparModuleDescriptor, target); OFF(SparModuleDescriptor, version_major);
    OFF(SparModuleDescriptor, init); OFF(SparModuleDescriptor, quiesce);
    OFF(SparModuleDescriptor, destroy); OFF(SparModuleDescriptor, reserved);
    OFF(SparApiV0, capabilities); OFF(SparApiV0, module_add_function); OFF(SparApiV0, error_set);
    OFF(SparApiV0, int_get); OFF(SparApiV0, string_view); OFF(SparApiV0, buffer_borrow);
    OFF(SparApiV0, buffer_from_external); OFF(SparApiV0, list_get); OFF(SparApiV0, record_get);
    OFF(SparApiV0, ref_new); OFF(SparApiV0, resource_new); OFF(SparApiV0, call);
    OFF(SparApiV0, async_release); OFF(SparApiV0, module_add_type);
    printf("const ABI_MAJOR %u\nconst ABI_MINOR %u\n", SPAR_NATIVE_ABI_MAJOR, SPAR_NATIVE_ABI_MINOR);
    printf("const TAG_STRING %u\nconst TAG_OTHER %u\nconst DTYPE_F64 %u\nconst DTYPE_BOOL %u\n",
           SPAR_TAG_STRING, SPAR_TAG_OTHER, SPAR_DTYPE_F64, SPAR_DTYPE_BOOL);
    printf("const E_PANIC %d\nconst E_INVALID_HANDLE %d\nconst CAP_ARROW %llu\n",
           SPAR_E_PANIC, SPAR_E_INVALID_HANDLE, (unsigned long long)SPAR_CAP_ARROW_C_DATA);
    printf("const SYMBOL %s\n", SPAR_MODULE_SYMBOL);
    return 0;
}
