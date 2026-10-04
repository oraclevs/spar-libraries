//! Raw Spar Native ABI 1.0 candidate. Mirrors `include/spar_native.h` exactly.
//! Every item here is `#[repr(C)]` / C calling convention. See `docs/native-api/abi-v0.md`.
#![allow(non_camel_case_types, non_snake_case)]

use core::ffi::c_void;

pub const SPAR_NATIVE_ABI_MAJOR: u16 = 1;
pub const SPAR_NATIVE_ABI_MINOR: u16 = 0;
pub const SPAR_MODULE_SYMBOL: &[u8] = b"spar_native_module_v1\0";
pub const SPAR_MODULE_SYMBOL_V0: &[u8] = b"spar_native_module_v0\0";

pub type spar_status_t = i32;
pub const SPAR_OK: spar_status_t = 0;
pub const SPAR_E_ERROR: spar_status_t = 1;
pub const SPAR_E_TYPE: spar_status_t = 2;
pub const SPAR_E_RANGE: spar_status_t = 3;
pub const SPAR_E_OOM: spar_status_t = 4;
pub const SPAR_E_INVALID_ARGUMENT: spar_status_t = 5;
pub const SPAR_E_WRONG_THREAD: spar_status_t = 6;
pub const SPAR_E_UNSUPPORTED: spar_status_t = 7;
pub const SPAR_E_CANCELLED: spar_status_t = 8;
pub const SPAR_E_ABI_MISMATCH: spar_status_t = 9;
pub const SPAR_E_INVALID_HANDLE: spar_status_t = 10;
pub const SPAR_E_BORROW: spar_status_t = 11;
pub const SPAR_E_UTF8: spar_status_t = 12;
pub const SPAR_E_INVALID_STATE: spar_status_t = 13;
pub const SPAR_E_PANIC: spar_status_t = 14;

pub const SPAR_CAP_STRINGS: u64 = 1 << 0;
pub const SPAR_CAP_BYTES: u64 = 1 << 1;
pub const SPAR_CAP_TYPED_ARRAYS: u64 = 1 << 2;
pub const SPAR_CAP_LISTS: u64 = 1 << 3;
pub const SPAR_CAP_RECORDS: u64 = 1 << 4;
pub const SPAR_CAP_NATIVE_RESOURCES: u64 = 1 << 5;
pub const SPAR_CAP_CALLBACKS: u64 = 1 << 6;
pub const SPAR_CAP_ASYNC: u64 = 1 << 7;
pub const SPAR_CAP_ZERO_COPY_BYTES: u64 = 1 << 8;
pub const SPAR_CAP_DIRECT_CALLS: u64 = 1 << 9;
pub const SPAR_CAP_TABLES: u64 = 1 << 10;
pub const SPAR_CAP_ARROW_C_DATA: u64 = 1 << 11;

pub const SPAR_TAG_VOID: u32 = 0;
pub const SPAR_TAG_BOOL: u32 = 1;
pub const SPAR_TAG_INT: u32 = 2;
pub const SPAR_TAG_FLOAT: u32 = 3;
pub const SPAR_TAG_STRING: u32 = 16;
pub const SPAR_TAG_BYTES: u32 = 17;
pub const SPAR_TAG_LIST: u32 = 18;
pub const SPAR_TAG_RECORD: u32 = 19;
pub const SPAR_TAG_OPTION: u32 = 20;
pub const SPAR_TAG_RESOURCE: u32 = 21;
pub const SPAR_TAG_CALLABLE: u32 = 22;
pub const SPAR_TAG_OTHER: u32 = 31;

pub const SPAR_DTYPE_U8: u32 = 1;
pub const SPAR_DTYPE_I8: u32 = 2;
pub const SPAR_DTYPE_U16: u32 = 3;
pub const SPAR_DTYPE_I16: u32 = 4;
pub const SPAR_DTYPE_U32: u32 = 5;
pub const SPAR_DTYPE_I32: u32 = 6;
pub const SPAR_DTYPE_U64: u32 = 7;
pub const SPAR_DTYPE_I64: u32 = 8;
pub const SPAR_DTYPE_F32: u32 = 9;
pub const SPAR_DTYPE_F64: u32 = 10;
pub const SPAR_DTYPE_BOOL: u32 = 11;

pub const SPAR_BUFFER_READ: u32 = 1 << 0;
pub const SPAR_BUFFER_WRITE: u32 = 1 << 1;
pub const SPAR_BUFFER_NO_COPY: u32 = 1 << 3;
pub const SPAR_BUFFER_COPIED: u32 = 1 << 4;

pub const SPAR_FN_ASYNC: u32 = 1 << 0;
pub const SPAR_FN_CALLS: u32 = 1 << 1;

#[repr(C)]
pub struct SparEnv {
    _private: [u8; 0],
}
#[repr(C)]
pub struct SparModule {
    _private: [u8; 0],
}
#[repr(C)]
pub struct SparAsync {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union SparPayload {
    pub i64_: i64,
    pub u64_: u64,
    pub f64_: f64,
    pub handle: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SparValue {
    pub tag: u32,
    pub flags: u32,
    pub payload: SparPayload,
}

impl SparValue {
    #[inline]
    pub const fn void() -> Self {
        Self {
            tag: SPAR_TAG_VOID,
            flags: 0,
            payload: SparPayload { u64_: 0 },
        }
    }
    #[inline]
    pub const fn bool(b: bool) -> Self {
        Self {
            tag: SPAR_TAG_BOOL,
            flags: 0,
            payload: SparPayload { u64_: b as u64 },
        }
    }
    #[inline]
    pub const fn int(i: i64) -> Self {
        Self {
            tag: SPAR_TAG_INT,
            flags: 0,
            payload: SparPayload { i64_: i },
        }
    }
    #[inline]
    pub const fn float(f: f64) -> Self {
        Self {
            tag: SPAR_TAG_FLOAT,
            flags: 0,
            payload: SparPayload { f64_: f },
        }
    }
}

pub type SparRef = u64;
pub type SparSymbol = u32;
pub type SparBorrow = u64;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SparStrView {
    pub ptr: *const u8,
    pub len: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SparBufferView {
    pub struct_size: u32,
    pub flags: u32,
    pub data: *mut c_void,
    pub len_elements: u64,
    pub len_bytes: u64,
    pub dtype: u32,
    pub ndim: u32,
    pub stride_bytes: i64,
    pub reserved: u64,
    pub borrow: SparBorrow,
}

impl SparBufferView {
    pub const fn zeroed() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            flags: 0,
            data: core::ptr::null_mut(),
            len_elements: 0,
            len_bytes: 0,
            dtype: 0,
            ndim: 0,
            stride_bytes: 0,
            reserved: 0,
            borrow: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SparParamSpec {
    pub name: *const u8,
    pub name_len: u64,
    pub type_: *const u8,
    pub type_len: u64,
}

pub type SparNativeFn = unsafe extern "C" fn(
    env: *mut SparEnv,
    userdata: *mut c_void,
    argv: *const SparValue,
    argc: u64,
    out_result: *mut SparValue,
) -> spar_status_t;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SparFunctionSpec {
    pub struct_size: u32,
    pub flags: u32,
    pub name: *const u8,
    pub name_len: u64,
    pub params: *const SparParamSpec,
    pub param_count: u64,
    pub ret_type: *const u8,
    pub ret_type_len: u64,
    pub invoke: Option<SparNativeFn>,
    pub userdata: *mut c_void,
    pub direct: *const c_void,
    pub direct_sig: *const u8,
    pub direct_sig_len: u64,
    pub reserved: [u64; 2],
}

pub type SparFinalizer = unsafe extern "C" fn(data: *mut c_void, userdata: *mut c_void);

macro_rules! api_fn {
    ($($t:tt)*) => { Option<unsafe extern "C" fn($($t)*) -> spar_status_t> };
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SparApiV0 {
    pub struct_size: u32,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub capabilities: u64,

    pub module_add_function: api_fn!(*mut SparModule, *const SparFunctionSpec),
    pub error_set: api_fn!(*mut SparEnv, i32, *const u8, u64),
    pub int_get: api_fn!(*mut SparEnv, SparValue, *mut i64),
    pub float_get: api_fn!(*mut SparEnv, SparValue, *mut f64),
    pub bool_get: api_fn!(*mut SparEnv, SparValue, *mut u8),
    pub string_new: api_fn!(*mut SparEnv, *const u8, u64, *mut SparValue),
    pub string_view: api_fn!(*mut SparEnv, SparValue, *mut SparStrView),
    pub bytes_new: api_fn!(*mut SparEnv, *const u8, u64, *mut SparValue),
    pub buffer_borrow: api_fn!(*mut SparEnv, SparValue, u32, u32, *mut SparBufferView),
    pub buffer_release: api_fn!(*mut SparEnv, SparBorrow),
    pub buffer_new: api_fn!(*mut SparEnv, u32, u64, *mut SparBufferView, *mut SparValue),
    pub buffer_from_external: api_fn!(
        *mut SparEnv,
        *mut c_void,
        u32,
        u64,
        Option<SparFinalizer>,
        *mut c_void,
        *mut SparValue
    ),
    pub buffer_to_list: api_fn!(*mut SparEnv, SparValue, *mut SparValue),
    pub list_len: api_fn!(*mut SparEnv, SparValue, *mut u64),
    pub list_get: api_fn!(*mut SparEnv, SparValue, u64, *mut SparValue),
    pub list_new: api_fn!(*mut SparEnv, u64, *mut SparValue),
    pub list_push: api_fn!(*mut SparEnv, SparValue, SparValue),
    pub symbol_intern: api_fn!(*mut SparEnv, *const u8, u64, *mut SparSymbol),
    pub record_new: api_fn!(*mut SparEnv, u64, *mut SparValue),
    pub record_set: api_fn!(*mut SparEnv, SparValue, SparSymbol, SparValue),
    pub record_get: api_fn!(*mut SparEnv, SparValue, SparSymbol, *mut SparValue),
    pub record_len: api_fn!(*mut SparEnv, SparValue, *mut u64),
    pub option_some: api_fn!(*mut SparEnv, SparValue, *mut SparValue),
    pub option_none: api_fn!(*mut SparEnv, *mut SparValue),
    pub option_get: api_fn!(*mut SparEnv, SparValue, *mut u8, *mut SparValue),
    pub ref_new: api_fn!(*mut SparEnv, SparValue, *mut SparRef),
    pub ref_get: api_fn!(*mut SparEnv, SparRef, *mut SparValue),
    pub ref_drop: api_fn!(SparRef),
    pub resource_new: api_fn!(
        *mut SparEnv,
        u64,
        *mut c_void,
        Option<SparFinalizer>,
        *mut c_void,
        *mut SparValue
    ),
    pub resource_get: api_fn!(*mut SparEnv, SparValue, u64, *mut *mut c_void),
    pub resource_close: api_fn!(*mut SparEnv, SparValue, u64),
    pub call: api_fn!(
        *mut SparEnv,
        SparValue,
        *const SparValue,
        u64,
        *mut SparValue
    ),
    pub async_begin: api_fn!(*mut SparEnv, *mut *mut SparAsync),
    pub async_complete: api_fn!(*mut SparAsync, *const u8, u64),
    pub async_fail: api_fn!(*mut SparAsync, *const u8, u64),
    pub async_is_cancelled: api_fn!(*mut SparAsync, *mut u8),
    pub async_release: api_fn!(*mut SparAsync),
    // minor 2
    pub module_add_type: api_fn!(*mut SparModule, *const u8, u64),
}

pub type SparApiV1 = SparApiV0;

pub type SparModuleInitFn = unsafe extern "C" fn(
    api: *const SparApiV0,
    module: *mut SparModule,
    out_state: *mut *mut c_void,
) -> spar_status_t;
pub type SparModuleQuiesceFn = unsafe extern "C" fn(state: *mut c_void);
pub type SparModuleDestroyFn = unsafe extern "C" fn(state: *mut c_void);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SparModuleDescriptor {
    pub struct_size: u32,
    pub abi_major: u16,
    pub min_abi_minor: u16,
    pub required_capabilities: u64,
    pub optional_capabilities: u64,
    pub module_name: *const u8,
    pub module_name_len: u64,
    pub target: *const u8,
    pub target_len: u64,
    pub version_major: u32,
    pub version_minor: u32,
    pub version_patch: u32,
    pub reserved0: u32,
    pub init: Option<SparModuleInitFn>,
    pub quiesce: Option<SparModuleQuiesceFn>,
    pub destroy: Option<SparModuleDestroyFn>,
    pub reserved: [u64; 4],
}

// Descriptors are static, immutable data owned by the module.
unsafe impl Sync for SparModuleDescriptor {}
unsafe impl Sync for SparParamSpec {}
unsafe impl Sync for SparFunctionSpec {}
