//! Compares the C header's layout (compiled with the system C compiler) to the Rust mirror.
use spar_native_sys::*;
use std::mem::{align_of, offset_of, size_of};
use std::process::Command;

fn cc(args: &[&str]) -> std::process::Output {
    let manifest = env!("CARGO_MANIFEST_DIR");
    if cfg!(target_env = "msvc") {
        let source = args
            .iter()
            .find(|arg| arg.ends_with(".c") || arg.ends_with(".cpp"))
            .expect("C/C++ source");
        let out = args
            .iter()
            .position(|arg| *arg == "-o")
            .expect("output flag");
        return Command::new("cl")
            .args(["/nologo", "/W4", "/WX"])
            .arg(format!("/I{manifest}/include"))
            .arg(source)
            .arg(format!("/Fe:{}", args[out + 1]))
            .output()
            .expect("MSVC compiler");
    }
    Command::new(args[0])
        .args(&args[1..])
        .arg(format!("-I{manifest}/include"))
        .output()
        .expect("compiler")
}

fn c_facts() -> std::collections::HashMap<String, String> {
    static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let run = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("spar-abi-{}-{run}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join(if cfg!(windows) {
        "abi_check.exe"
    } else {
        "abi_check"
    });
    let src = format!("{}/tests/abi_check.c", env!("CARGO_MANIFEST_DIR"));
    let out = cc(&[
        "cc",
        "-std=c11",
        "-Wall",
        "-Wextra",
        "-Werror",
        &src,
        "-o",
        exe.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().unwrap();
    String::from_utf8(run.stdout)
        .unwrap()
        .lines()
        .map(|l| {
            let (k, v) = l.split_once(' ').unwrap();
            let (name, val) = v.split_once(' ').unwrap();
            (format!("{k} {name}"), val.to_string())
        })
        .collect()
}

macro_rules! check_size {
    ($f:expr, $t:ty, $n:expr) => {
        assert_eq!(
            $f[&format!("size {}", $n)],
            format!("{} {}", size_of::<$t>(), align_of::<$t>()),
            "{}",
            $n
        );
    };
}
macro_rules! check_off {
    ($f:expr, $t:ty, $field:ident, $n:expr) => {
        assert_eq!(
            $f[&format!("off {}", $n)],
            offset_of!($t, $field).to_string(),
            "{}",
            $n
        );
    };
}

#[test]
fn layouts_match_the_c_header() {
    let f = c_facts();
    check_size!(f, SparValue, "SparValue");
    check_size!(f, SparStrView, "SparStrView");
    check_size!(f, SparBufferView, "SparBufferView");
    check_size!(f, SparParamSpec, "SparParamSpec");
    check_size!(f, SparFunctionSpec, "SparFunctionSpec");
    check_size!(f, SparModuleDescriptor, "SparModuleDescriptor");
    check_size!(f, SparApiV0, "SparApiV0");
    check_off!(f, SparValue, tag, "SparValue.tag");
    check_off!(f, SparValue, payload, "SparValue.payload");
    check_off!(f, SparBufferView, data, "SparBufferView.data");
    check_off!(
        f,
        SparBufferView,
        len_elements,
        "SparBufferView.len_elements"
    );
    check_off!(f, SparBufferView, dtype, "SparBufferView.dtype");
    check_off!(
        f,
        SparBufferView,
        stride_bytes,
        "SparBufferView.stride_bytes"
    );
    check_off!(f, SparBufferView, borrow, "SparBufferView.borrow");
    check_off!(f, SparFunctionSpec, name, "SparFunctionSpec.name");
    check_off!(f, SparFunctionSpec, params, "SparFunctionSpec.params");
    check_off!(f, SparFunctionSpec, ret_type, "SparFunctionSpec.ret_type");
    check_off!(f, SparFunctionSpec, invoke, "SparFunctionSpec.invoke");
    check_off!(f, SparFunctionSpec, userdata, "SparFunctionSpec.userdata");
    check_off!(f, SparFunctionSpec, direct, "SparFunctionSpec.direct");
    check_off!(
        f,
        SparFunctionSpec,
        direct_sig,
        "SparFunctionSpec.direct_sig"
    );
    check_off!(f, SparFunctionSpec, reserved, "SparFunctionSpec.reserved");
    check_off!(
        f,
        SparModuleDescriptor,
        required_capabilities,
        "SparModuleDescriptor.required_capabilities"
    );
    check_off!(
        f,
        SparModuleDescriptor,
        module_name,
        "SparModuleDescriptor.module_name"
    );
    check_off!(
        f,
        SparModuleDescriptor,
        target,
        "SparModuleDescriptor.target"
    );
    check_off!(
        f,
        SparModuleDescriptor,
        version_major,
        "SparModuleDescriptor.version_major"
    );
    check_off!(f, SparModuleDescriptor, init, "SparModuleDescriptor.init");
    check_off!(
        f,
        SparModuleDescriptor,
        quiesce,
        "SparModuleDescriptor.quiesce"
    );
    check_off!(
        f,
        SparModuleDescriptor,
        destroy,
        "SparModuleDescriptor.destroy"
    );
    check_off!(
        f,
        SparModuleDescriptor,
        reserved,
        "SparModuleDescriptor.reserved"
    );
    check_off!(f, SparApiV0, capabilities, "SparApiV0.capabilities");
    check_off!(
        f,
        SparApiV0,
        module_add_function,
        "SparApiV0.module_add_function"
    );
    check_off!(f, SparApiV0, error_set, "SparApiV0.error_set");
    check_off!(f, SparApiV0, int_get, "SparApiV0.int_get");
    check_off!(f, SparApiV0, string_view, "SparApiV0.string_view");
    check_off!(f, SparApiV0, buffer_borrow, "SparApiV0.buffer_borrow");
    check_off!(
        f,
        SparApiV0,
        buffer_from_external,
        "SparApiV0.buffer_from_external"
    );
    check_off!(f, SparApiV0, list_get, "SparApiV0.list_get");
    check_off!(f, SparApiV0, record_get, "SparApiV0.record_get");
    check_off!(f, SparApiV0, ref_new, "SparApiV0.ref_new");
    check_off!(f, SparApiV0, resource_new, "SparApiV0.resource_new");
    check_off!(f, SparApiV0, call, "SparApiV0.call");
    check_off!(f, SparApiV0, async_release, "SparApiV0.async_release");
    check_off!(f, SparApiV0, module_add_type, "SparApiV0.module_add_type");
    // released prefix must never move: minor-1 table ended right after async_release
    assert_eq!(
        offset_of!(SparApiV0, module_add_type),
        offset_of!(SparApiV0, async_release) + 8
    );
}

#[test]
fn constants_match_the_c_header() {
    let f = c_facts();
    let get = |k: &str| f[&format!("const {k}")].clone();
    assert_eq!(get("ABI_MAJOR"), SPAR_NATIVE_ABI_MAJOR.to_string());
    assert_eq!(get("ABI_MINOR"), SPAR_NATIVE_ABI_MINOR.to_string());
    assert_eq!(get("TAG_STRING"), SPAR_TAG_STRING.to_string());
    assert_eq!(get("TAG_OTHER"), SPAR_TAG_OTHER.to_string());
    assert_eq!(get("DTYPE_F64"), SPAR_DTYPE_F64.to_string());
    assert_eq!(get("DTYPE_BOOL"), SPAR_DTYPE_BOOL.to_string());
    assert_eq!(get("E_PANIC"), SPAR_E_PANIC.to_string());
    assert_eq!(get("E_INVALID_HANDLE"), SPAR_E_INVALID_HANDLE.to_string());
    assert_eq!(get("CAP_ARROW"), SPAR_CAP_ARROW_C_DATA.to_string());
    assert_eq!(
        get("SYMBOL"),
        std::str::from_utf8(&SPAR_MODULE_SYMBOL[..SPAR_MODULE_SYMBOL.len() - 1]).unwrap()
    );
}

#[test]
fn fixed_sizes_are_pinned() {
    // Hard-coded so a refactor cannot silently move the ABI (64-bit targets).
    assert_eq!(size_of::<SparValue>(), 16);
    assert_eq!(size_of::<SparStrView>(), 16);
    assert_eq!(size_of::<SparBufferView>(), 64);
    assert_eq!(align_of::<SparValue>(), 8);
}

#[test]
fn header_compiles_as_c_and_cpp() {
    let m = env!("CARGO_MANIFEST_DIR");
    let dir = std::env::temp_dir().join(format!("spar-hdr-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (compiler, std_flag, file) in [
        ("cc", "-std=c11", "header_compile.c"),
        ("c++", "-std=c++17", "header_compile.cpp"),
    ] {
        let src = format!("{m}/tests/{file}");
        let out = cc(&[
            compiler,
            std_flag,
            "-Wall",
            "-Wextra",
            "-Wpedantic",
            "-Werror",
            &src,
            "-o",
            dir.join(if cfg!(windows) {
                format!("{file}.exe")
            } else {
                file.to_string()
            })
            .to_str()
            .unwrap(),
        ]);
        assert!(
            out.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(Command::new(dir.join(file)).status().unwrap().success());
    }
}
