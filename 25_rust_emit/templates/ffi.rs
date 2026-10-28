//! Raw FFI mirror of 20_engine_core/include/stratum_c_abi.h (ABI version 1). Hand-written;
//! `25_rust_emit/main.py` fails if a function or the ABI version drifts from the header.
use std::os::raw::{c_char, c_void};

pub const STRATUM_ABI_VERSION: i32 = 1;

#[repr(C)]
#[derive(Copy, Clone)]
pub union StratumArg {
    pub i: i64,
    pub d: f64,
    pub f: f32,
    pub p: *const c_void,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct StratumStr {
    pub ptr: *const c_char,
    pub len: usize,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct StratumSlice {
    pub ptr: *const c_void,
    pub len: usize,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct StratumListArg {
    pub existing: i64,
    pub ptrs: *const i64,
    pub len: usize,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct StratumMapArg {
    pub existing: i64,
    pub keys: *const i64,
    pub vals: *const i64,
    pub len: usize,
}
/// kind: B I J F D Z C S (primitive), T (StratumStr[]), O (int64 handles). Free with stratum_arr_free.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct StratumArr {
    pub kind: u8,
    pub len: usize,
    pub data: *mut c_void,
}
impl StratumArr {
    #[inline(always)]
    pub const fn empty() -> Self {
        StratumArr { kind: 0, len: 0, data: std::ptr::null_mut() }
    }
}

pub const STRATUM_V_NULL: u8 = 0;
pub const STRATUM_V_BOOL: u8 = 1;
pub const STRATUM_V_BYTE: u8 = 2;
pub const STRATUM_V_CHAR: u8 = 3;
pub const STRATUM_V_SHORT: u8 = 4;
pub const STRATUM_V_INT: u8 = 5;
pub const STRATUM_V_LONG: u8 = 6;
pub const STRATUM_V_FLOAT: u8 = 7;
pub const STRATUM_V_DOUBLE: u8 = 8;
pub const STRATUM_V_STR: u8 = 9;
pub const STRATUM_V_OBJ: u8 = 10;
pub const STRATUM_V_ERR: u8 = 11;
pub const STRATUM_V_I32ARR: u8 = 12;

// primitive array kinds for stratum_new_array
pub const STRATUM_K_BYTE: u8 = b'B';
pub const STRATUM_K_SHORT: u8 = b'S';
pub const STRATUM_K_CHAR: u8 = b'C';
pub const STRATUM_K_INT: u8 = b'I';
pub const STRATUM_K_LONG: u8 = b'J';
pub const STRATUM_K_FLOAT: u8 = b'F';
pub const STRATUM_K_DOUBLE: u8 = b'D';
pub const STRATUM_K_BOOL: u8 = b'Z';

#[repr(C)]
#[derive(Copy, Clone)]
pub struct StratumValStr {
    pub ptr: *const c_void,
    pub len: usize,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub union StratumValPayload {
    pub i: i64,
    pub d: f64,
    pub s: StratumValStr,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct StratumVal {
    pub tag: u8,
    pub v: StratumValPayload,
}
impl StratumVal {
    #[inline(always)]
    pub fn zeroed() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

pub type StratumTramp =
    unsafe extern "C" fn(userdata: *mut c_void, method: *const c_char, args: *const StratumVal, argc: usize, out: *mut StratumVal);
pub type StratumDropFn = unsafe extern "C" fn(userdata: *mut c_void);
pub type StratumLifecycleFn = unsafe extern "C" fn(event: *const c_char, act: i64) -> i32;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct StratumCbDesc {
    pub userdata: *mut c_void,
    pub fn_: Option<StratumTramp>,
    pub drop: Option<StratumDropFn>,
    pub existing: i64,
}

// On Android the final cdylib links -lstratum, so libmyapp.so records libstratum.so as NEEDED
// (Gradle/CMake build libstratum.so, Gradle passes -L to cargo). On a PC target (cargo check) nothing is linked.
#[cfg_attr(target_os = "android", link(name = "stratum"))]
extern "C" {
    pub fn stratum_core_init(java_vm: *mut c_void) -> i32;
    pub fn stratum_abi_version() -> i32;
    pub fn stratum_meta_hash() -> *const c_char;
    pub fn stratum_last_error() -> *const c_char;
    pub fn stratum_get_env() -> *mut c_void;

    pub fn stratum_new_instance(cid: u32, slot: u32, a: *const StratumArg, err: *mut i32) -> i64;
    pub fn stratum_call_v(p: i64, cid: u32, slot: u32, a: *const StratumArg, err: *mut i32);
    pub fn stratum_call_z(p: i64, cid: u32, slot: u32, a: *const StratumArg, err: *mut i32) -> u8;
    pub fn stratum_call_i(p: i64, cid: u32, slot: u32, a: *const StratumArg, err: *mut i32) -> i64;
    pub fn stratum_call_j(p: i64, cid: u32, slot: u32, a: *const StratumArg, err: *mut i32) -> i64;
    pub fn stratum_call_d(p: i64, cid: u32, slot: u32, a: *const StratumArg, err: *mut i32) -> f64;
    pub fn stratum_call_str(p: i64, cid: u32, slot: u32, a: *const StratumArg, err: *mut i32) -> StratumStr;
    pub fn stratum_call_o(p: i64, cid: u32, slot: u32, a: *const StratumArg, err: *mut i32) -> i64;
    pub fn stratum_call_arr(p: i64, cid: u32, slot: u32, a: *const StratumArg, out: *mut StratumArr, err: *mut i32);
    pub fn stratum_call_list(p: i64, cid: u32, slot: u32, a: *const StratumArg, out: *mut StratumArr, err: *mut i32);

    pub fn stratum_field_get_i(p: i64, cid: u32, slot: u32, err: *mut i32) -> i64;
    pub fn stratum_field_get_z(p: i64, cid: u32, slot: u32, err: *mut i32) -> u8;
    pub fn stratum_field_get_j(p: i64, cid: u32, slot: u32, err: *mut i32) -> i64;
    pub fn stratum_field_get_d(p: i64, cid: u32, slot: u32, err: *mut i32) -> f64;
    pub fn stratum_field_get_str(p: i64, cid: u32, slot: u32, err: *mut i32) -> StratumStr;
    pub fn stratum_field_get_o(p: i64, cid: u32, slot: u32, err: *mut i32) -> i64;
    pub fn stratum_field_get_arr(p: i64, cid: u32, slot: u32, out: *mut StratumArr, err: *mut i32);
    pub fn stratum_field_set_i(p: i64, cid: u32, slot: u32, v: i64, err: *mut i32);
    pub fn stratum_field_set_z(p: i64, cid: u32, slot: u32, v: u8, err: *mut i32);
    pub fn stratum_field_set_j(p: i64, cid: u32, slot: u32, v: i64, err: *mut i32);
    pub fn stratum_field_set_d(p: i64, cid: u32, slot: u32, v: f64, err: *mut i32);
    pub fn stratum_field_set_str(p: i64, cid: u32, slot: u32, v: *const StratumStr, err: *mut i32);
    pub fn stratum_field_set_o(p: i64, cid: u32, slot: u32, v: i64, err: *mut i32);

    pub fn stratum_arr_free(a: *mut StratumArr);
    pub fn stratum_get_array_data(arr_handle: i64, out: *mut StratumArr, err: *mut i32);
    pub fn stratum_delete_ref(p: i64);
    pub fn stratum_clone_ref(p: i64) -> i64;
    pub fn stratum_is_instance_of(p: i64, class_id: u32) -> u8;
    pub fn stratum_is_same_object(a: i64, b: i64) -> u8;
    pub fn stratum_new_string(s: *const StratumStr) -> i64;
    pub fn stratum_to_string(p: i64) -> StratumStr;
    pub fn stratum_hash_code(p: i64) -> i32;
    pub fn stratum_box(kind: u8, bits: i64) -> i64;
    pub fn stratum_unbox(obj_handle: i64, kind: u8, err: *mut i32) -> i64;
    pub fn stratum_new_list(items: *const i64, n: usize) -> i64;
    pub fn stratum_new_map(keys: *const i64, vals: *const i64, n: usize) -> i64;
    pub fn stratum_new_array(kind: u8, data: *const c_void, len: usize) -> i64;
    pub fn stratum_new_object_array(elem_class: *const c_char, items: *const i64, n: usize) -> i64;
    pub fn stratum_get_map_entries(map_handle: i64, keys_out: *mut StratumArr, vals_out: *mut StratumArr, err: *mut i32);

    pub fn stratum_register_callback(key: *const c_char, d: *const StratumCbDesc);
    pub fn stratum_remove_callback(key: *const c_char);
    pub fn stratum_remove_callbacks_by_prefix(prefix: *const c_char) -> usize;
    pub fn stratum_callback_count() -> usize;
    pub fn stratum_set_lifecycle_dispatcher(f: Option<StratumLifecycleFn>);
    pub fn stratum_post_ui(d: *const StratumCbDesc) -> i32;

    pub fn stratum_get_activity() -> i64;
    pub fn stratum_create_view(key: *const c_char) -> i64;
    pub fn stratum_set_content_view(act: i64, view: i64);

    pub fn stratum_get_direct_buffer_address(bb: i64) -> *mut c_void;
    pub fn stratum_get_direct_buffer_capacity(bb: i64) -> usize;
    pub fn stratum_get_bytebuffer_data(bb: i64, out: *mut StratumArr, err: *mut i32);
    pub fn stratum_allocate_direct_buffer(capacity: usize) -> i64;
    pub fn stratum_surface_to_native_window(surface: i64) -> *mut c_void;
    pub fn stratum_release_native_window(window: *mut c_void);

    pub fn stratum_reflect_call(
        class_name: *const c_char,
        target: i64,
        method: *const c_char,
        args: *const StratumVal,
        argc: usize,
        out: *mut StratumVal,
    ) -> i32;

    pub fn stratum_set_log_enabled(enabled: u8);
    pub fn stratum_is_log_enabled() -> u8;
    pub fn stratum_log_msg(msg: *const c_char);
}