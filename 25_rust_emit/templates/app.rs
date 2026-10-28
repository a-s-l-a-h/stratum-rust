//! App-level API: lifecycle auto-wiring, activity, UI thread, callbacks registry, buffers, custom canvas view.
//! (Rust counterpart of Stage 21's stratum.hpp.)
pub use crate::api::android::app::Activity;
use crate::base::*;
use crate::ffi;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once};

// ── logging ──────────────────────────────────────────────────────────────────
#[cfg(target_os = "android")]
#[link(name = "log")]
extern "C" {
    fn __android_log_write(prio: i32, tag: *const c_char, text: *const c_char) -> i32;
}
#[cfg(target_os = "android")]
fn android_log(prio: i32, msg: &str) {
    let tag = CString::new("Stratum").unwrap();
    let text = CString::new(msg.replace('\0', " ")).unwrap();
    unsafe { __android_log_write(prio, tag.as_ptr(), text.as_ptr()) };
}
#[cfg(not(target_os = "android"))]
fn android_log(_prio: i32, msg: &str) {
    eprintln!("[Stratum] {}", msg);
}
fn fatal(msg: &str) -> ! {
    android_log(7, msg);
    std::process::abort()
}
pub fn set_log_enabled(e: bool) {
    unsafe { ffi::stratum_set_log_enabled(e as u8) }
}

// ── init / lifecycle ─────────────────────────────────────────────────────────
type CreateFn = Arc<dyn Fn(Activity) + Send + Sync>;
type VoidFn = Arc<dyn Fn() + Send + Sync>;
struct Hooks {
    create: Option<CreateFn>,
    resume: Option<VoidFn>,
    pause: Option<VoidFn>,
    stop: Option<VoidFn>,
    destroy: Option<VoidFn>,
}
static HOOKS: Mutex<Hooks> = Mutex::new(Hooks { create: None, resume: None, pause: None, stop: None, destroy: None });

fn hooks() -> std::sync::MutexGuard<'static, Hooks> {
    HOOKS.lock().unwrap_or_else(|e| e.into_inner())
}
pub fn on_create<F: Fn(Activity) + Send + Sync + 'static>(f: F) {
    hooks().create = Some(Arc::new(f));
}
pub fn on_resume<F: Fn() + Send + Sync + 'static>(f: F) {
    hooks().resume = Some(Arc::new(f));
}
pub fn on_pause<F: Fn() + Send + Sync + 'static>(f: F) {
    hooks().pause = Some(Arc::new(f));
}
pub fn on_stop<F: Fn() + Send + Sync + 'static>(f: F) {
    hooks().stop = Some(Arc::new(f));
}
pub fn on_destroy<F: Fn() + Send + Sync + 'static>(f: F) {
    hooks().destroy = Some(Arc::new(f));
}
/// Return true to consume the hardware back button.
pub fn on_back_pressed<F: Fn() -> bool + Send + Sync + 'static>(f: F) {
    register_function("onBackPressed", move |_a: Args| f());
}

unsafe extern "C" fn dispatch(ev: *const c_char, act: i64) -> i32 {
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
        let ev = CStr::from_ptr(ev).to_string_lossy().into_owned();
        let (create, resume, pause, stop, destroy) = {
            let h = hooks();
            (h.create.clone(), h.resume.clone(), h.pause.clone(), h.stop.clone(), h.destroy.clone())
        };
        match ev.as_str() {
            "onCreate" => {
                if let Some(f) = create {
                    f(Activity::wrap(if act != 0 { ffi::stratum_clone_ref(act) } else { 0 }));
                }
            }
            "onResume" => {
                if let Some(f) = resume {
                    f()
                }
            }
            "onPause" => {
                if let Some(f) = pause {
                    f()
                }
            }
            "onStop" => {
                if let Some(f) = stop {
                    f()
                }
            }
            "onDestroy" => {
                if let Some(f) = destroy {
                    f()
                }
            }
            _ => {}
        }
    }));
    0
}

static INIT: Once = Once::new();
/// Idempotent. Verifies libstratum.so matches this crate and installs the lifecycle dispatcher.
/// Called by `stratum_main!`, automatically (feature `auto-init`, best effort), or call it yourself.
pub fn init() {
    INIT.call_once(|| unsafe {
        if ffi::stratum_abi_version() != ffi::STRATUM_ABI_VERSION {
            fatal("libstratum.so ABI version differs from this crate. Rebuild both.");
        }
        #[cfg(not(feature = "skip-hash-check"))]
        {
            let h = CStr::from_ptr(ffi::stratum_meta_hash()).to_string_lossy().into_owned();
            if h != crate::meta_hash::STRATUM_META_HASH {
                fatal(&format!(
                    "libstratum.so ({}) and the Rust SDK ({}) were generated from different inputs. Rebuild both.",
                    h,
                    crate::meta_hash::STRATUM_META_HASH
                ));
            }
        }
        ffi::stratum_set_lifecycle_dispatcher(Some(dispatch));
    });
}

#[cfg(all(feature = "auto-init", target_os = "android"))]
#[used]
#[link_section = ".init_array"]
static AUTO_INIT: extern "C" fn() = {
    extern "C" fn run() {
        init();
    }
    run
};

// ── activity / views ─────────────────────────────────────────────────────────
pub fn get_activity() -> Activity {
    Activity::wrap(unsafe { ffi::stratum_get_activity() })
}
/// UI thread only.
pub fn set_content_view(view: &Object) {
    let a = get_activity();
    unsafe { ffi::stratum_set_content_view(a.raw(), view.raw()) }
}
pub fn set_content_view_on(act: &Object, view: &Object) {
    unsafe { ffi::stratum_set_content_view(act.raw(), view.raw()) }
}
/// com.stratum.runtime.StratumView; register "<key>#onDraw" etc. with register_callback.
pub fn create_view(key: &str) -> Result<Object> {
    let k = CString::new(key).map_err(|e| JavaException(e.to_string()))?;
    let p = unsafe { ffi::stratum_create_view(k.as_ptr()) };
    if p == 0 {
        return Err(JavaException(last_error()));
    }
    Ok(Object::adopt(p))
}

// ── callback registry ────────────────────────────────────────────────────────
/// key: "StratumService#onCreate", "StratumReceiver#onReceive", "app#fn", "<viewkey>#onDraw", ...
pub fn register_callback(key: &str, f: impl IntoCallback) {
    let cb = f.into_callback();
    let k = CString::new(key).expect("callback key contains NUL");
    unsafe { ffi::stratum_register_callback(k.as_ptr(), cb.desc_ptr()) }; // engine takes the closure
}
/// Callable from Java via StratumRuntimeLookup.callStratum(name, ...).
pub fn register_function(name: &str, f: impl IntoCallback) {
    register_callback(&format!("app#{}", name), f);
}
pub fn unregister_prefix(prefix: &str) -> usize {
    match CString::new(prefix) {
        Ok(p) => unsafe { ffi::stratum_remove_callbacks_by_prefix(p.as_ptr()) },
        Err(_) => 0,
    }
}
/// Runs `f` once on the main looper.
pub fn run_on_ui_thread<F: FnOnce() + Send + 'static>(f: F) -> Result<()> {
    let cell = Mutex::new(Some(f));
    let cb = Callback::new(move |_a: Args| {
        let f = cell.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(f) = f {
            f();
        }
    });
    let rc = unsafe { ffi::stratum_post_ui(cb.desc_ptr()) };
    if rc != 0 {
        return Err(JavaException(last_error()));
    }
    Ok(())
}

// ── buffers / surfaces ───────────────────────────────────────────────────────
pub fn allocate_direct_buffer(n: usize) -> Result<Object> {
    let p = unsafe { ffi::stratum_allocate_direct_buffer(n) };
    if p == 0 {
        return Err(JavaException(last_error()));
    }
    Ok(Object::adopt(p))
}
/// Keep `b` alive while using the pointer.
#[inline]
pub fn direct_buffer_address(b: &Object) -> *mut u8 {
    unsafe { ffi::stratum_get_direct_buffer_address(b.raw()) as *mut u8 }
}
#[inline]
pub fn direct_buffer_capacity(b: &Object) -> usize {
    unsafe { ffi::stratum_get_direct_buffer_capacity(b.raw()) }
}
pub fn bytebuffer_data(b: &Object) -> Result<Vec<u8>> {
    let mut out = empty_arr();
    let mut e = 0i32;
    unsafe { ffi::stratum_get_bytebuffer_data(b.raw(), &mut out, &mut e) };
    check(e)?;
    Ok(arr_vec::<u8>(&mut out))
}
/// ANativeWindow* (release with release_native_window).
#[inline]
pub fn surface_to_native_window(s: &Object) -> *mut std::os::raw::c_void {
    unsafe { ffi::stratum_surface_to_native_window(s.raw()) }
}
#[inline]
pub fn release_native_window(w: *mut std::os::raw::c_void) {
    unsafe { ffi::stratum_release_native_window(w) }
}

// ── StratumReceiver / StratumService bridges ─────────────────────────────────
pub fn on_broadcast<F: Fn(Object, Object) + Send + Sync + 'static>(f: F) {
    register_callback("StratumReceiver#onReceive", move |a: Args| {
        f(a.obj(0), a.obj(1));
    });
}
pub fn on_service_command<F: Fn(Object, i32, i32) -> i32 + Send + Sync + 'static>(f: F) {
    register_callback("StratumService#onStartCommand", move |a: Args| f(a.obj(0), a.i32(1), a.i32(2)));
}

// ── custom 2D drawing view (same role as the C++ CustomCanvasView) ───────────
pub trait CanvasView: Send + Sync + 'static {
    fn on_draw(&self, _canvas: Object) {}
    /// Return [width, height] to call setMeasuredDimension, empty for the default measure.
    fn on_measure(&self, _width_spec: i32, _height_spec: i32) -> Vec<i32> {
        Vec::new()
    }
    fn on_layout(&self, _changed: bool, _l: i32, _t: i32, _r: i32, _b: i32) {}
    fn on_touch_event(&self, _event: Object) -> bool {
        false
    }
}
/// Derefs to the underlying StratumView Object. Dropping it unregisters the callbacks.
pub struct CanvasViewHandle {
    obj: Object,
    key: String,
}
impl std::ops::Deref for CanvasViewHandle {
    type Target = Object;
    fn deref(&self) -> &Object {
        &self.obj
    }
}
impl Drop for CanvasViewHandle {
    fn drop(&mut self) {
        unregister_prefix(&format!("{}#", self.key));
    }
}
pub fn create_canvas_view<V: CanvasView>(view: V) -> Result<CanvasViewHandle> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let key = format!("view_{}", NEXT.fetch_add(1, Ordering::Relaxed) + 1);
    let v = Arc::new(view);
    {
        let v = v.clone();
        register_callback(&format!("{}#onDraw", key), move |a: Args| {
            v.on_draw(a.obj(0));
        });
    }
    {
        let v = v.clone();
        register_callback(&format!("{}#onMeasure", key), move |a: Args| v.on_measure(a.i32(0), a.i32(1)));
    }
    {
        let v = v.clone();
        register_callback(&format!("{}#onLayout", key), move |a: Args| {
            v.on_layout(a.bool(0), a.i32(1), a.i32(2), a.i32(3), a.i32(4));
        });
    }
    {
        let v = v.clone();
        register_callback(&format!("{}#onTouchEvent", key), move |a: Args| v.on_touch_event(a.obj(0)));
    }
    match create_view(&key) {
        Ok(obj) => Ok(CanvasViewHandle { obj, key }),
        Err(e) => {
            unregister_prefix(&format!("{}#", key));
            Err(e)
        }
    }
}