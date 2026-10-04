//! Safe Rust SDK for Spar native modules (ABI 1.0).
//!
//! ```ignore
//! #[spar_native::module(name = "fastArray", version = "0.1.0")]
//! mod fast_array {
//!     #[spar_native::function]
//!     fn sum(values: &[f64]) -> f64 { values.iter().sum() }
//! }
//! ```
//!
//! Ownership at a glance: everything a native function receives is *call-scoped*. `&str`, `&[T]`
//! and `&mut [T]` borrow runtime memory and are valid until the function returns (the lifetime
//! `'env` encodes this, `Context<'env>` is `!Send`). To keep data, copy it. Results are moved into
//! the runtime; `Buf<T>` hands a `Vec<T>` over without copying.

use core::ffi::c_void;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicPtr, Ordering};
use std::fmt::Display;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub use spar_native_macros::{function, module};
pub use spar_native_sys as sys;
use sys::*;

/// Capabilities every SDK-built module requires.
pub const DEFAULT_REQUIRED_CAPABILITIES: u64 =
    SPAR_CAP_STRINGS | SPAR_CAP_BYTES | SPAR_CAP_TYPED_ARRAYS;

/// A failure reported to Spar. `status` is one of the `SPAR_E_*` codes.
#[derive(Debug, Clone)]
pub struct Error {
    pub status: i32,
    pub message: String,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            status: SPAR_E_ERROR,
            message: message.into(),
        }
    }
    pub fn type_error(message: impl Into<String>) -> Self {
        Self {
            status: SPAR_E_TYPE,
            message: message.into(),
        }
    }
    pub fn range(message: impl Into<String>) -> Self {
        Self {
            status: SPAR_E_RANGE,
            message: message.into(),
        }
    }
    fn status(status: i32, what: &str) -> Self {
        Self {
            status,
            message: format!("native call failed: {what} (status {status})"),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;

static API: AtomicPtr<SparApiV0> = AtomicPtr::new(core::ptr::null_mut());

#[inline]
fn api() -> &'static SparApiV0 {
    // SAFETY: set once in `__private::init` from the runtime's static table before any call.
    unsafe { &*API.load(Ordering::Acquire) }
}

/// Per-call context. Not `Send`/`Sync`: the env is thread-affine.
pub struct Context<'env> {
    env: *mut SparEnv,
    _p: PhantomData<(*mut (), &'env ())>,
}

macro_rules! call {
    ($cx:expr, $f:ident($($a:expr),*)) => {{
        let f = api().$f.ok_or_else(|| Error::status(SPAR_E_UNSUPPORTED, stringify!($f)))?;
        // SAFETY: env is the live call env; out-pointers are valid locals.
        let s = unsafe { f($cx.env, $($a),*) };
        if s != SPAR_OK { return Err(Error::status(s, stringify!($f))); }
    }};
}

impl<'env> Context<'env> {
    /// # Safety
    /// `env` must be the env pointer received by the current native function.
    pub unsafe fn from_raw(env: *mut SparEnv) -> Self {
        Self {
            env,
            _p: PhantomData,
        }
    }

    pub fn raw_env(&self) -> *mut SparEnv {
        self.env
    }

    /// New Spar string (copies, must be UTF-8 which `&str` guarantees).
    pub fn string(&self, s: &str) -> Result<Value<'env>> {
        let mut out = SparValue::void();
        call!(self, string_new(s.as_ptr(), s.len() as u64, &mut out));
        Ok(Value {
            raw: out,
            _p: PhantomData,
        })
    }

    /// Sets the pending error message (the trampoline does this for returned `Err`s).
    pub fn set_error(&self, message: &str) {
        if let Some(f) = api().error_set {
            // SAFETY: env is live; message bytes are valid for the call.
            unsafe {
                f(
                    self.env,
                    SPAR_E_ERROR,
                    message.as_ptr(),
                    message.len() as u64,
                )
            };
        }
    }
}

/// An untyped Spar value valid for the current call.
#[derive(Clone, Copy)]
pub struct Value<'env> {
    pub raw: SparValue,
    _p: PhantomData<&'env ()>,
}

impl<'env> Value<'env> {
    pub fn from_raw(raw: SparValue) -> Self {
        Self {
            raw,
            _p: PhantomData,
        }
    }
}

/// Conversion of a Spar argument into a Rust value. `'env` is the call scope.
pub trait FromSpar<'env>: Sized {
    /// Spar type spelling used in the function signature (`int`, `[float]`, ...).
    const SPAR_TYPE: &'static str;
    fn from_spar(cx: &Context<'env>, v: SparValue) -> Result<Self>;
}

/// Conversion of a Rust return value into a Spar value.
pub trait IntoSpar<'env> {
    const SPAR_TYPE: &'static str;
    fn into_spar(self, cx: &Context<'env>) -> Result<SparValue>;
}

impl<'env> FromSpar<'env> for i64 {
    const SPAR_TYPE: &'static str = "int";
    fn from_spar(cx: &Context<'env>, v: SparValue) -> Result<Self> {
        let mut out = 0i64;
        call!(cx, int_get(v, &mut out));
        Ok(out)
    }
}
impl<'env> IntoSpar<'env> for i64 {
    const SPAR_TYPE: &'static str = "int";
    fn into_spar(self, _cx: &Context<'env>) -> Result<SparValue> {
        Ok(SparValue::int(self))
    }
}
impl<'env> FromSpar<'env> for f64 {
    const SPAR_TYPE: &'static str = "float";
    fn from_spar(cx: &Context<'env>, v: SparValue) -> Result<Self> {
        let mut out = 0f64;
        call!(cx, float_get(v, &mut out));
        Ok(out)
    }
}
impl<'env> IntoSpar<'env> for f64 {
    const SPAR_TYPE: &'static str = "float";
    fn into_spar(self, _cx: &Context<'env>) -> Result<SparValue> {
        Ok(SparValue::float(self))
    }
}
impl<'env> FromSpar<'env> for bool {
    const SPAR_TYPE: &'static str = "bool";
    fn from_spar(cx: &Context<'env>, v: SparValue) -> Result<Self> {
        let mut out = 0u8;
        call!(cx, bool_get(v, &mut out));
        Ok(out != 0)
    }
}
impl<'env> IntoSpar<'env> for bool {
    const SPAR_TYPE: &'static str = "bool";
    fn into_spar(self, _cx: &Context<'env>) -> Result<SparValue> {
        Ok(SparValue::bool(self))
    }
}
impl<'env> IntoSpar<'env> for () {
    const SPAR_TYPE: &'static str = "void";
    fn into_spar(self, _cx: &Context<'env>) -> Result<SparValue> {
        Ok(SparValue::void())
    }
}

impl<'env> FromSpar<'env> for &'env str {
    const SPAR_TYPE: &'static str = "str";
    fn from_spar(cx: &Context<'env>, v: SparValue) -> Result<Self> {
        let mut view = SparStrView {
            ptr: core::ptr::null(),
            len: 0,
        };
        call!(cx, string_view(v, &mut view));
        // SAFETY: the runtime guarantees the bytes are valid UTF-8 (they come from a Spar `str`)
        // and immutable for the rest of this call, which `'env` bounds.
        Ok(unsafe {
            if view.len == 0 {
                ""
            } else {
                core::str::from_utf8_unchecked(core::slice::from_raw_parts(
                    view.ptr,
                    view.len as usize,
                ))
            }
        })
    }
}
impl<'env> FromSpar<'env> for String {
    const SPAR_TYPE: &'static str = "str";
    fn from_spar(cx: &Context<'env>, v: SparValue) -> Result<Self> {
        <&str as FromSpar>::from_spar(cx, v).map(str::to_owned)
    }
}
impl<'env> IntoSpar<'env> for String {
    const SPAR_TYPE: &'static str = "str";
    fn into_spar(self, cx: &Context<'env>) -> Result<SparValue> {
        Ok(cx.string(&self)?.raw)
    }
}
impl<'env> IntoSpar<'env> for &str {
    const SPAR_TYPE: &'static str = "str";
    fn into_spar(self, cx: &Context<'env>) -> Result<SparValue> {
        Ok(cx.string(self)?.raw)
    }
}

impl<'env> FromSpar<'env> for Value<'env> {
    const SPAR_TYPE: &'static str = "Any";
    fn from_spar(_cx: &Context<'env>, v: SparValue) -> Result<Self> {
        Ok(Value::from_raw(v))
    }
}
impl<'env> IntoSpar<'env> for Value<'env> {
    const SPAR_TYPE: &'static str = "Any";
    fn into_spar(self, _cx: &Context<'env>) -> Result<SparValue> {
        Ok(self.raw)
    }
}

/// Element types with a stable dtype.
pub trait Dtype: Copy + 'static {
    const DTYPE: u32;
    /// Spar element type spelling (`int`, `float`, `bool`).
    const ELEM: &'static str;
}
macro_rules! dtype {
    ($($t:ty => $d:expr, $e:expr, $list:expr, $slice:expr);* $(;)?) => {$(
        impl Dtype for $t { const DTYPE: u32 = $d; const ELEM: &'static str = $e; }
        impl<'env> FromSpar<'env> for &'env [$t] {
            const SPAR_TYPE: &'static str = $slice;
            fn from_spar(cx: &Context<'env>, v: SparValue) -> Result<Self> {
                borrow_slice::<$t>(cx, v, SPAR_BUFFER_READ).map(|(p, n)| unsafe { slice_of(p as *const $t, n) })
            }
        }
        impl<'env> FromSpar<'env> for &'env mut [$t] {
            const SPAR_TYPE: &'static str = "Buffer";
            fn from_spar(cx: &Context<'env>, v: SparValue) -> Result<Self> {
                borrow_slice::<$t>(cx, v, SPAR_BUFFER_READ | SPAR_BUFFER_WRITE | SPAR_BUFFER_NO_COPY)
                    .map(|(p, n)| unsafe { slice_of_mut(p as *mut $t, n) })
            }
        }
    )*};
}
dtype! {
    f64 => SPAR_DTYPE_F64, "float", "[float]", "Slice<float>";
    f32 => SPAR_DTYPE_F32, "float", "[float]", "Slice<float>";
    i64 => SPAR_DTYPE_I64, "int", "[int]", "Slice<int>";
    i32 => SPAR_DTYPE_I32, "int", "[int]", "Slice<int>";
    u32 => SPAR_DTYPE_U32, "int", "[int]", "Slice<int>";
    i16 => SPAR_DTYPE_I16, "int", "[int]", "Slice<int>";
    u16 => SPAR_DTYPE_U16, "int", "[int]", "Slice<int>";
    i8 => SPAR_DTYPE_I8, "int", "[int]", "Slice<int>"
}

/// # Safety
/// `p` must be valid for `n` reads of `T` for `'a`.
unsafe fn slice_of<'a, T>(p: *const T, n: usize) -> &'a [T] {
    if n == 0 {
        &[]
    } else {
        core::slice::from_raw_parts(p, n)
    }
}
/// # Safety
/// `p` must be valid for `n` exclusive reads/writes of `T` for `'a`.
unsafe fn slice_of_mut<'a, T>(p: *mut T, n: usize) -> &'a mut [T] {
    if n == 0 {
        &mut []
    } else {
        core::slice::from_raw_parts_mut(p, n)
    }
}

fn borrow_slice<T: Dtype>(cx: &Context<'_>, v: SparValue, flags: u32) -> Result<(*mut u8, usize)> {
    let mut view = SparBufferView::zeroed();
    call!(cx, buffer_borrow(v, T::DTYPE, flags, &mut view));
    // The runtime releases the borrow when the call scope closes; the slice cannot outlive it.
    Ok((view.data as *mut u8, view.len_elements as usize))
}

// u8 slices are the `Bytes` type (zero-copy, read-only) and mutable Buffers.
impl<'env> FromSpar<'env> for &'env [u8] {
    const SPAR_TYPE: &'static str = "Bytes";
    fn from_spar(cx: &Context<'env>, v: SparValue) -> Result<Self> {
        borrow_slice::<u8>(cx, v, SPAR_BUFFER_READ | SPAR_BUFFER_NO_COPY)
            .map(|(p, n)| unsafe { slice_of(p as *const u8, n) })
    }
}
impl Dtype for u8 {
    const DTYPE: u32 = SPAR_DTYPE_U8;
    const ELEM: &'static str = "int";
}
impl<'env> FromSpar<'env> for &'env mut [u8] {
    const SPAR_TYPE: &'static str = "Buffer";
    fn from_spar(cx: &Context<'env>, v: SparValue) -> Result<Self> {
        borrow_slice::<u8>(
            cx,
            v,
            SPAR_BUFFER_READ | SPAR_BUFFER_WRITE | SPAR_BUFFER_NO_COPY,
        )
        .map(|(p, n)| unsafe { slice_of_mut(p, n) })
    }
}
impl<'env> IntoSpar<'env> for Vec<u8> {
    const SPAR_TYPE: &'static str = "Bytes";
    fn into_spar(self, cx: &Context<'env>) -> Result<SparValue> {
        let mut out = SparValue::void();
        call!(cx, bytes_new(self.as_ptr(), self.len() as u64, &mut out));
        Ok(out)
    }
}

/// Zero-copy return of a typed array: the `Vec` moves into a runtime-owned `Buffer`; its
/// destructor runs when the buffer dies.
pub struct Buf<T: Dtype>(pub Vec<T>);

impl<'env, T: Dtype> IntoSpar<'env> for Buf<T> {
    const SPAR_TYPE: &'static str = "Buffer";
    fn into_spar(self, cx: &Context<'env>) -> Result<SparValue> {
        unsafe extern "C" fn finalize<T>(_data: *mut c_void, userdata: *mut c_void) {
            // SAFETY: userdata is the Box<(ptr,len,cap)> created below.
            let (ptr, len, cap) = *Box::from_raw(userdata as *mut (*mut T, usize, usize));
            drop(Vec::from_raw_parts(ptr, len, cap));
        }
        let mut v = core::mem::ManuallyDrop::new(self.0);
        let (ptr, len, cap) = (v.as_mut_ptr(), v.len(), v.capacity());
        let ud = Box::into_raw(Box::new((ptr, len, cap))) as *mut c_void;
        let mut out = SparValue::void();
        let f = api()
            .buffer_from_external
            .ok_or_else(|| Error::status(SPAR_E_UNSUPPORTED, "buffer_from_external"))?;
        // SAFETY: on success the runtime owns the finalizer; on failure we reclaim the Vec below.
        let s = unsafe {
            f(
                cx.env,
                ptr as *mut c_void,
                T::DTYPE,
                len as u64,
                Some(finalize::<T>),
                ud,
                &mut out,
            )
        };
        if s != SPAR_OK {
            // The runtime did not take ownership.
            unsafe {
                let (p, l, c) = *Box::from_raw(ud as *mut (*mut T, usize, usize));
                drop(Vec::from_raw_parts(p, l, c));
            }
            return Err(Error::status(s, "buffer_from_external"));
        }
        Ok(out)
    }
}

impl<'env, T: Dtype> IntoSpar<'env> for Vec<T>
where
    T: IntoSparElem,
{
    const SPAR_TYPE: &'static str = T::LIST;
    fn into_spar(self, cx: &Context<'env>) -> Result<SparValue> {
        let mut list = SparValue::void();
        call!(cx, list_new(self.len() as u64, &mut list));
        for item in self {
            let raw = item.elem();
            call!(cx, list_push(list, raw));
        }
        Ok(list)
    }
}

/// Scalars that can be pushed into a Spar list.
pub trait IntoSparElem {
    const LIST: &'static str;
    fn elem(self) -> SparValue;
}
impl IntoSparElem for f64 {
    const LIST: &'static str = "[float]";
    fn elem(self) -> SparValue {
        SparValue::float(self)
    }
}
impl IntoSparElem for i64 {
    const LIST: &'static str = "[int]";
    fn elem(self) -> SparValue {
        SparValue::int(self)
    }
}

impl<'env, T: IntoSpar<'env>, E: Display> IntoSpar<'env> for core::result::Result<T, E> {
    const SPAR_TYPE: &'static str = T::SPAR_TYPE;
    fn into_spar(self, cx: &Context<'env>) -> Result<SparValue> {
        match self {
            Ok(v) => v.into_spar(cx),
            Err(e) => Err(Error::new(e.to_string())),
        }
    }
}

/// Value that can complete a native promise through ABI 1's JSON completion channel.
pub trait AsyncValue: Send + 'static {
    const PROMISE_TYPE: &'static str;
    fn json(self) -> Vec<u8>;
}
impl AsyncValue for i64 {
    const PROMISE_TYPE: &'static str = "Promise<int>";
    fn json(self) -> Vec<u8> {
        self.to_string().into_bytes()
    }
}
impl AsyncValue for bool {
    const PROMISE_TYPE: &'static str = "Promise<bool>";
    fn json(self) -> Vec<u8> {
        if self {
            b"true".to_vec()
        } else {
            b"false".to_vec()
        }
    }
}

/// Starts work after the native call has created a Spar promise. The callback must schedule work
/// and return promptly; it must never retain a borrowed `SparEnv`, string, slice, or resource.
pub struct AsyncTask<T: AsyncValue> {
    start: Box<dyn FnOnce(AsyncCompleter<T>) + Send + 'static>,
}
impl<T: AsyncValue> AsyncTask<T> {
    pub fn new(start: impl FnOnce(AsyncCompleter<T>) + Send + 'static) -> Self {
        Self {
            start: Box::new(start),
        }
    }
}
impl<'env, T: AsyncValue> IntoSpar<'env> for AsyncTask<T> {
    const SPAR_TYPE: &'static str = T::PROMISE_TYPE;
    fn into_spar(self, cx: &Context<'env>) -> Result<SparValue> {
        let begin = api()
            .async_begin
            .ok_or_else(|| Error::status(SPAR_E_UNSUPPORTED, "async_begin"))?;
        let mut raw = core::ptr::null_mut();
        let status = unsafe { begin(cx.env, &mut raw) };
        if status != SPAR_OK {
            return Err(Error::status(status, "async_begin"));
        }
        (self.start)(AsyncCompleter {
            raw: raw as usize,
            _type: PhantomData,
        });
        Ok(SparValue::void())
    }
}

/// Thread-safe, single-use completion handle. Drop fails an unfinished promise and releases its
/// ABI operation. Completing a cancelled promise is safe and still releases the operation.
pub struct AsyncCompleter<T: AsyncValue> {
    raw: usize,
    _type: PhantomData<T>,
}
impl<T: AsyncValue> AsyncCompleter<T> {
    pub fn is_cancelled(&self) -> bool {
        let Some(check) = api().async_is_cancelled else {
            return false;
        };
        let mut cancelled = 0u8;
        unsafe { check(self.raw as *mut SparAsync, &mut cancelled) == SPAR_OK && cancelled != 0 }
    }
    pub fn complete(mut self, value: T) -> Result<()> {
        let json = value.json();
        let complete = api()
            .async_complete
            .ok_or_else(|| Error::status(SPAR_E_UNSUPPORTED, "async_complete"))?;
        let status =
            unsafe { complete(self.raw as *mut SparAsync, json.as_ptr(), json.len() as u64) };
        self.release();
        if status == SPAR_OK || status == SPAR_E_CANCELLED {
            Ok(())
        } else {
            Err(Error::status(status, "async_complete"))
        }
    }
    pub fn fail(mut self, message: impl AsRef<str>) -> Result<()> {
        let message = message.as_ref();
        let fail = api()
            .async_fail
            .ok_or_else(|| Error::status(SPAR_E_UNSUPPORTED, "async_fail"))?;
        let status = unsafe {
            fail(
                self.raw as *mut SparAsync,
                message.as_ptr(),
                message.len() as u64,
            )
        };
        self.release();
        if status == SPAR_OK || status == SPAR_E_CANCELLED {
            Ok(())
        } else {
            Err(Error::status(status, "async_fail"))
        }
    }
    fn release(&mut self) {
        if self.raw != 0 {
            if let Some(release) = api().async_release {
                unsafe {
                    release(self.raw as *mut SparAsync);
                }
            }
            self.raw = 0;
        }
    }
}
impl<T: AsyncValue> Drop for AsyncCompleter<T> {
    fn drop(&mut self) {
        self.release();
    }
}

/// A Rust value exposed to Spar as an opaque, typed handle. `SPAR_TYPE` is the Spar type name
/// (capitalised identifier); it is registered automatically. The runtime owns the value and runs
/// its `Drop` when the resource dies (or when closed). Access is `&T`: use interior mutability
/// (`Mutex`, atomics) for state; the value may be used from several threads, hence `Send + Sync`.
pub trait Resource: Send + Sync + 'static {
    const SPAR_TYPE: &'static str;
}

fn resource_tag(name: &str) -> u64 {
    // FNV-1a: stable across builds so a handle made by one module version is recognised by the next.
    let mut h = 0xcbf29ce484222325u64;
    for b in name.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    h
}

/// Moves `T` into the runtime as a resource (return type of a native function).
pub struct Owned<T: Resource>(pub T);

impl<'env, T: Resource> IntoSpar<'env> for Owned<T> {
    const SPAR_TYPE: &'static str = T::SPAR_TYPE;
    fn into_spar(self, cx: &Context<'env>) -> Result<SparValue> {
        unsafe extern "C" fn finalize<T>(data: *mut c_void, _ud: *mut c_void) {
            // SAFETY: `data` is the Box<T> leaked below.
            drop(Box::from_raw(data as *mut T));
        }
        let ptr = Box::into_raw(Box::new(self.0)) as *mut c_void;
        let mut out = SparValue::void();
        let f = api()
            .resource_new
            .ok_or_else(|| Error::status(SPAR_E_UNSUPPORTED, "resource_new"))?;
        // SAFETY: on success the runtime owns `ptr` and will call `finalize::<T>` exactly once.
        let s = unsafe {
            f(
                cx.env,
                resource_tag(T::SPAR_TYPE),
                ptr,
                Some(finalize::<T>),
                core::ptr::null_mut(),
                &mut out,
            )
        };
        if s != SPAR_OK {
            unsafe { drop(Box::from_raw(ptr as *mut T)) };
            return Err(Error::status(s, "resource_new"));
        }
        Ok(out)
    }
}

impl<'env, T: Resource> FromSpar<'env> for &'env T {
    const SPAR_TYPE: &'static str = T::SPAR_TYPE;
    fn from_spar(cx: &Context<'env>, v: SparValue) -> Result<Self> {
        let mut p: *mut c_void = core::ptr::null_mut();
        call!(cx, resource_get(v, resource_tag(T::SPAR_TYPE), &mut p));
        // SAFETY: the tag matched, so `p` is a live Box<T> owned by the runtime for at least this call.
        Ok(unsafe { &*(p as *const T) })
    }
}

const BUILTIN_TYPES: &[&str] = &["Bytes", "Record", "Any", "Buffer"];

fn declared_type_name(spar_type: &str) -> Option<&str> {
    let first = spar_type.chars().next()?;
    (first.is_ascii_uppercase()
        && spar_type
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !BUILTIN_TYPES.contains(&spar_type))
    .then_some(spar_type)
}

#[doc(hidden)]
pub mod __private {
    use super::*;

    /// Runs a native function body with arity check, panic containment and error reporting.
    ///
    /// # Safety
    /// Called only from generated trampolines with the runtime's arguments.
    pub unsafe fn run<F>(
        name: &str,
        env: *mut SparEnv,
        argv: *const SparValue,
        argc: u64,
        expected: usize,
        out: *mut SparValue,
        f: F,
    ) -> i32
    where
        F: for<'env> FnOnce(Context<'env>, &[SparValue]) -> Result<SparValue>,
    {
        let cx = Context::from_raw(env);
        if argc as usize != expected {
            cx.set_error(&format!("{name} expects {expected} arguments, got {argc}"));
            return SPAR_E_INVALID_ARGUMENT;
        }
        let args: &[SparValue] = if expected == 0 {
            &[]
        } else {
            core::slice::from_raw_parts(argv, expected)
        };
        match catch_unwind(AssertUnwindSafe(|| f(Context::from_raw(env), args))) {
            Ok(Ok(v)) => {
                out.write(v);
                SPAR_OK
            }
            Ok(Err(e)) => {
                if let Some(set) = api().error_set {
                    set(env, e.status, e.message.as_ptr(), e.message.len() as u64);
                }
                e.status
            }
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".into());
                cx.set_error(&format!("{name} panicked: {msg}"));
                SPAR_E_PANIC
            }
        }
    }

    /// Module init wrapper: stores the API table, contains panics.
    ///
    /// # Safety
    /// Called only from the generated init trampoline.
    pub unsafe fn init(
        api_ptr: *const SparApiV0,
        state: *mut *mut c_void,
        f: impl FnOnce() -> Result<()>,
    ) -> i32 {
        if api_ptr.is_null()
            || ((*api_ptr).struct_size as usize) < core::mem::size_of::<SparApiV0>()
        {
            // The SDK needs the ABI 1.0 table prefix; a shorter table is unsafe.
            return SPAR_E_ABI_MISMATCH;
        }
        API.store(api_ptr as *mut SparApiV0, Ordering::Release);
        if !state.is_null() {
            *state = core::ptr::null_mut();
        }
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(Ok(())) => SPAR_OK,
            Ok(Err(e)) => e.status,
            Err(_) => SPAR_E_PANIC,
        }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)] // internal, called only from generated init code
    pub fn register(
        _api: *const SparApiV0,
        module: *mut SparModule,
        name: &str,
        params: &[(&str, &str)],
        ret: &str,
        invoke: SparNativeFn,
        flags: u32,
    ) -> Result<()> {
        let specs: Vec<SparParamSpec> = params
            .iter()
            .map(|(n, t)| SparParamSpec {
                name: n.as_ptr(),
                name_len: n.len() as u64,
                type_: t.as_ptr(),
                type_len: t.len() as u64,
            })
            .collect();
        let spec = SparFunctionSpec {
            struct_size: core::mem::size_of::<SparFunctionSpec>() as u32,
            flags,
            name: name.as_ptr(),
            name_len: name.len() as u64,
            params: specs.as_ptr(),
            param_count: specs.len() as u64,
            ret_type: ret.as_ptr(),
            ret_type_len: ret.len() as u64,
            invoke: Some(invoke),
            userdata: core::ptr::null_mut(),
            direct: core::ptr::null(),
            direct_sig: core::ptr::null(),
            direct_sig_len: 0,
            reserved: [0; 2],
        };
        if let Some(add_type) = api().module_add_type {
            // Newer runtimes only: opaque resource type names are declared before use.
            for t in params.iter().map(|(_, t)| *t).chain(core::iter::once(ret)) {
                if let Some(name) = declared_type_name(t) {
                    // SAFETY: init-time call with the module pointer we were given.
                    let s = unsafe { add_type(module, name.as_ptr(), name.len() as u64) };
                    if s != SPAR_OK {
                        return Err(Error::status(s, name));
                    }
                }
            }
        }
        let f = api()
            .module_add_function
            .ok_or_else(|| Error::status(SPAR_E_UNSUPPORTED, "module_add_function"))?;
        // SAFETY: spec and param arrays live for the call; the host copies them.
        let s = unsafe { f(module, &spec) };
        if s != SPAR_OK {
            return Err(Error::status(s, name));
        }
        Ok(())
    }
}
