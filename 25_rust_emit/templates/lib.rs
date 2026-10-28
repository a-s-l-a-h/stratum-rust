//! Stratum Rust SDK. Generated Java API lives under `stratum::android::...`, `stratum::java::...`.
#![allow(
    non_snake_case,
    non_camel_case_types,
    non_upper_case_globals,
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_unsafe,
    clippy::all
)]

pub mod ffi;
pub mod base;
pub mod reflect;
pub mod app;
pub mod meta_hash;
pub mod api;

pub use api::*;
pub use app::*; // on_create, set_content_view, register_callback, ..., Activity
pub use base::{
    box_bool, box_char, box_f32, box_f64, box_i16, box_i32, box_i64, box_i8, cb, cb0, java_string, new_array, new_array_2d,
    new_list, new_map, new_object_array, new_object_array_of, new_string_array, Args, AsRaw, Callback, CbArg, Handlers,
    IntoCallback, IntoRet, JChar, JPrim, JStr, JavaClass, JavaException, Object,
};
/// `base::Result` under a non-conflicting name (a glob-exported `Result` would shadow std's).
pub use base::Result as JResult;

/// Entry point for your app cdylib: defines JNI_OnLoad, runs `init()` and your registration closure.
///     stratum_main!(|| { on_create(|act| { ... }); });
/// Requires edition 2021 in the app crate (`#[no_mangle]`).
#[macro_export]
macro_rules! stratum_main {
    ($f:expr) => {
        #[no_mangle]
        pub extern "system" fn JNI_OnLoad(
            _vm: *mut ::std::os::raw::c_void,
            _reserved: *mut ::std::os::raw::c_void,
        ) -> i32 {
            $crate::init();
            let f: fn() = $f;
            f();
            0x0001_0006 // JNI_VERSION_1_6
        }
    };
}