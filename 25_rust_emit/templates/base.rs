//! Core runtime shared by every generated module: Object, argument views, callbacks, errors, array creation.
//! Generated code refers to this module as `__c`. (Rust counterpart of Stage 21's core.hpp.)
//! Tiny helpers are #[inline(always)] so generated wrappers collapse into a direct call of the C ABI symbol.
use crate::ffi::*;
use std::any::Any;
use std::cell::{RefCell, UnsafeCell};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::os::raw::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::slice;

// ── errors ────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct JavaException(pub String);
impl fmt::Display for JavaException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for JavaException {}
pub type Result<T> = std::result::Result<T, JavaException>;

pub fn last_error() -> String {
    unsafe {
        let p = stratum_last_error();
        if p.is_null() {
            String::new()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
}
#[cold]
#[inline(never)]
fn fail() -> JavaException {
    JavaException(last_error())
}
#[inline(always)]
pub fn check(err: i32) -> Result<()> {
    if err == 0 {
        Ok(())
    } else {
        Err(fail())
    }
}
#[inline]
pub fn to_std(s: &StratumStr) -> String {
    if s.ptr.is_null() || s.len == 0 {
        return String::new();
    }
    unsafe { String::from_utf8_lossy(slice::from_raw_parts(s.ptr as *const u8, s.len)).into_owned() }
}

// ── traits ────────────────────────────────────────────────────────────────────
/// Anything that owns / wraps a JNI global ref.
pub trait AsRaw {
    fn handle(&self) -> i64;
}
/// Implemented by every generated class struct.
pub trait JavaClass: AsRaw + Sized {
    const CLASS_ID: u32;
    const FQN: &'static str;
    /// Adopts a global ref (takes ownership).
    fn wrap(h: i64) -> Self;
}

// ── Object: owns one JNI global ref. clone = new global ref. null = handle 0 ──
#[repr(transparent)]
pub struct Object {
    ptr: i64,
}
impl Object {
    #[inline(always)]
    pub const fn null() -> Self {
        Object { ptr: 0 }
    }
    /// Takes ownership of a global ref (internal; used by generated code).
    #[inline(always)]
    pub fn adopt(h: i64) -> Self {
        Object { ptr: h }
    }
    #[inline(always)]
    pub fn raw(&self) -> i64 {
        self.ptr
    }
    /// Gives up ownership; the caller must eventually delete the global ref.
    #[inline]
    pub fn release(mut self) -> i64 {
        let p = self.ptr;
        self.ptr = 0;
        p
    }
    #[inline(always)]
    pub fn is_null(&self) -> bool {
        self.ptr == 0
    }
    #[inline(always)]
    pub fn arg(&self) -> StratumArg {
        StratumArg { i: self.ptr }
    }
    #[inline]
    pub fn hash_code(&self) -> i32 {
        unsafe { stratum_hash_code(self.ptr) }
    }
    #[inline]
    pub fn is_instance_of<T: JavaClass>(&self) -> bool {
        unsafe { stratum_is_instance_of(self.ptr, T::CLASS_ID) != 0 }
    }
    /// Checked downcast (JNI IsInstanceOf). Null -> null T.
    pub fn cast<T: JavaClass>(&self) -> Result<T> {
        if self.ptr == 0 {
            return Ok(T::wrap(0));
        }
        if !self.is_instance_of::<T>() {
            return Err(JavaException(format!("Stratum: object is not an instance of {}", T::FQN)));
        }
        Ok(T::wrap(unsafe { stratum_clone_ref(self.ptr) }))
    }
    #[inline]
    pub fn unchecked_cast<T: JavaClass>(&self) -> T {
        if self.ptr == 0 {
            T::wrap(0)
        } else {
            T::wrap(unsafe { stratum_clone_ref(self.ptr) })
        }
    }

    fn unbox(&self, kind: u8) -> Result<i64> {
        let mut e = 0i32;
        let r = unsafe { stratum_unbox(self.ptr, kind, &mut e) };
        check(e)?;
        Ok(r)
    }
    pub fn as_bool(&self) -> Result<bool> {
        Ok(self.unbox(b'Z')? != 0)
    }
    pub fn as_i8(&self) -> Result<i8> {
        Ok(self.unbox(b'B')? as i8)
    }
    pub fn as_char(&self) -> Result<u16> {
        Ok(self.unbox(b'C')? as u16)
    }
    pub fn as_i16(&self) -> Result<i16> {
        Ok(self.unbox(b'S')? as i16)
    }
    pub fn as_i32(&self) -> Result<i32> {
        Ok(self.unbox(b'I')? as i32)
    }
    pub fn as_i64(&self) -> Result<i64> {
        self.unbox(b'J')
    }
    pub fn as_float(&self) -> Result<f32> {
        Ok(f32::from_bits(self.unbox(b'F')? as u32))
    }
    pub fn as_double(&self) -> Result<f64> {
        Ok(f64::from_bits(self.unbox(b'D')? as u64))
    }
    /// Java array object -> Vec (u8 i8 i16 u16 i32 i64 f32 f64 bool String Object).
    pub fn as_array<T: FromArr>(&self) -> Result<Vec<T>> {
        let mut out = empty_arr();
        let mut e = 0i32;
        unsafe { stratum_get_array_data(self.ptr, &mut out, &mut e) };
        check(e)?;
        Ok(T::from_arr(&mut out))
    }
    pub fn as_map(&self) -> Result<Vec<(Object, Object)>> {
        map_entries(self)
    }
}
impl Default for Object {
    #[inline(always)]
    fn default() -> Self {
        Object::null()
    }
}
impl Clone for Object {
    #[inline]
    fn clone(&self) -> Self {
        if self.ptr == 0 {
            Object::null()
        } else {
            Object { ptr: unsafe { stratum_clone_ref(self.ptr) } }
        }
    }
}
impl Drop for Object {
    #[inline]
    fn drop(&mut self) {
        if self.ptr != 0 {
            unsafe { stratum_delete_ref(self.ptr) };
        }
    }
}
impl PartialEq for Object {
    fn eq(&self, o: &Object) -> bool {
        unsafe { stratum_is_same_object(self.ptr, o.ptr) != 0 }
    }
}
impl Eq for Object {}
impl Hash for Object {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i32(self.hash_code());
    }
}
impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ptr == 0 {
            return write!(f, "null");
        }
        write!(f, "{}", to_std(&unsafe { stratum_to_string(self.ptr) }))
    }
}
impl fmt::Debug for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Object(0x{:x})", self.ptr)
    }
}
impl AsRaw for Object {
    #[inline(always)]
    fn handle(&self) -> i64 {
        self.ptr
    }
}

// ── boxing / strings / collections ───────────────────────────────────────────
#[inline]
fn box_(kind: u8, bits: i64) -> Object {
    Object::adopt(unsafe { stratum_box(kind, bits) })
}
#[inline]
pub fn box_bool(v: bool) -> Object {
    box_(b'Z', v as i64)
}
#[inline]
pub fn box_i8(v: i8) -> Object {
    box_(b'B', v as i64)
}
#[inline]
pub fn box_i16(v: i16) -> Object {
    box_(b'S', v as i64)
}
#[inline]
pub fn box_char(v: u16) -> Object {
    box_(b'C', v as i64)
}
#[inline]
pub fn box_i32(v: i32) -> Object {
    box_(b'I', v as i64)
}
#[inline]
pub fn box_i64(v: i64) -> Object {
    box_(b'J', v)
}
#[inline]
pub fn box_f32(v: f32) -> Object {
    box_(b'F', v.to_bits() as i64)
}
#[inline]
pub fn box_f64(v: f64) -> Object {
    box_(b'D', v.to_bits() as i64)
}
#[inline]
pub fn java_string(s: &str) -> Object {
    let x = StratumStr { ptr: s.as_ptr() as *const c_char, len: s.len() };
    Object::adopt(unsafe { stratum_new_string(&x) })
}
pub fn new_list<T: AsRaw>(v: &[T]) -> Object {
    let h: Vec<i64> = v.iter().map(|o| o.handle()).collect();
    Object::adopt(unsafe { stratum_new_list(h.as_ptr(), h.len()) })
}
pub fn new_map<K: AsRaw, V: AsRaw>(v: &[(K, V)]) -> Object {
    let k: Vec<i64> = v.iter().map(|p| p.0.handle()).collect();
    let x: Vec<i64> = v.iter().map(|p| p.1.handle()).collect();
    Object::adopt(unsafe { stratum_new_map(k.as_ptr(), x.as_ptr(), k.len()) })
}

// ── Java array creation (parity with core.hpp new_array / new_object_array / new_string_array) ──
/// Rust element types that map to a Java primitive array. kind = STRATUM_K_* letter.
pub trait JPrim: Copy {
    const KIND: u8;
}
macro_rules! jprim {
    ($(($t:ty, $k:expr)),* $(,)?) => { $( impl JPrim for $t { const KIND: u8 = $k; } )* };
}
jprim!((bool, b'Z'), (i8, b'B'), (u8, b'B'), (u16, b'C'), (i16, b'S'), (i32, b'I'), (i64, b'J'), (f32, b'F'), (f64, b'D'));

/// `new_array(&[1, 2, 3])` -> int[]   (u8/i8 -> byte[], u16 -> char[], bool -> boolean[] ...)
pub fn new_array<T: JPrim>(v: &[T]) -> Result<Object> {
    let h = unsafe { stratum_new_array(T::KIND, v.as_ptr() as *const c_void, v.len()) };
    if h == 0 {
        return Err(fail());
    }
    Ok(Object::adopt(h))
}
/// `elem_class`: "java/lang/String", "android.view.Surface", or an array descriptor for multi-dim:
/// "[I" -> int[][], "[[I" -> int[][][], "[Landroid/view/Surface;" -> Surface[][]. Items may be null Objects.
pub fn new_object_array<T: AsRaw>(elem_class: &str, items: &[T]) -> Result<Object> {
    let c = CString::new(elem_class).map_err(|e| JavaException(e.to_string()))?;
    let h: Vec<i64> = items.iter().map(|o| o.handle()).collect();
    let r = unsafe { stratum_new_object_array(c.as_ptr(), h.as_ptr(), h.len()) };
    if r == 0 {
        return Err(fail());
    }
    Ok(Object::adopt(r))
}
/// Array whose element class is the generated class: `new_object_array_of(&surfaces)` -> Surface[].
pub fn new_object_array_of<T: JavaClass>(items: &[T]) -> Result<Object> {
    new_object_array(T::FQN, items)
}
pub fn new_string_array<S: AsRef<str>>(items: &[S]) -> Result<Object> {
    let objs: Vec<Object> = items.iter().map(|s| java_string(s.as_ref())).collect();
    new_object_array("java/lang/String", &objs)
}
/// `new_array_2d(&[vec![1, 2], vec![3]])` -> int[][]
pub fn new_array_2d<T: JPrim>(rows: &[Vec<T>]) -> Result<Object> {
    let objs = rows.iter().map(|r| new_array(r)).collect::<Result<Vec<Object>>>()?;
    new_object_array(&format!("[{}", T::KIND as char), &objs)
}

// ── argument converters ──────────────────────────────────────────────────────
#[inline(always)]
pub fn arg_i(v: i64) -> StratumArg {
    StratumArg { i: v }
}
#[inline(always)]
pub fn arg_d(v: f64) -> StratumArg {
    StratumArg { d: v }
}
#[inline(always)]
pub fn arg_f(v: f32) -> StratumArg {
    let mut a = StratumArg { i: 0 };
    a.f = v;
    a
}

/// UTF-8 string argument; null = Java null. Borrows from &str / &String / Option<&str>, or OWNS a String
/// (so `tv.setText(format!("{}", x))` works). The bytes stay valid for as long as the JStr lives.
pub struct JStr<'a> {
    s: StratumStr,
    _own: Option<String>, // keeps an owned String alive (its heap buffer does not move when JStr moves)
    _m: PhantomData<&'a str>,
}
impl<'a> JStr<'a> {
    #[inline(always)]
    pub fn null() -> Self {
        JStr { s: StratumStr { ptr: std::ptr::null(), len: 0 }, _own: None, _m: PhantomData }
    }
    #[inline(always)]
    pub fn new(v: &'a str) -> Self {
        JStr { s: StratumStr { ptr: v.as_ptr() as *const c_char, len: v.len() }, _own: None, _m: PhantomData }
    }
    #[inline]
    pub fn owned(v: String) -> Self {
        let s = StratumStr { ptr: v.as_ptr() as *const c_char, len: v.len() };
        JStr { s, _own: Some(v), _m: PhantomData }
    }
    #[inline(always)]
    pub fn raw(&self) -> &StratumStr {
        &self.s
    }
    #[inline(always)]
    pub fn arg(&self) -> StratumArg {
        let mut a = StratumArg { i: 0 };
        a.p = &self.s as *const StratumStr as *const c_void;
        a
    }
}
impl<'a> From<&'a str> for JStr<'a> {
    #[inline(always)]
    fn from(v: &'a str) -> Self {
        JStr::new(v)
    }
}
impl<'a> From<&'a String> for JStr<'a> {
    #[inline(always)]
    fn from(v: &'a String) -> Self {
        JStr::new(v.as_str())
    }
}
impl<'a> From<String> for JStr<'a> {
    #[inline]
    fn from(v: String) -> Self {
        JStr::owned(v)
    }
}
impl<'a> From<Option<&'a str>> for JStr<'a> {
    #[inline]
    fn from(v: Option<&'a str>) -> Self {
        match v {
            Some(s) => JStr::new(s),
            None => JStr::null(),
        }
    }
}

/// Primitive array view (byte[] = u8, int[] = i32, ... boolean[] = bool). `ArrArg::null()` = Java null; empty slice = empty array.
pub struct ArrArg<'a, T> {
    s: StratumSlice,
    null: bool,
    _m: PhantomData<&'a [T]>,
}
impl<'a, T> ArrArg<'a, T> {
    #[inline(always)]
    pub fn null() -> Self {
        ArrArg { s: StratumSlice { ptr: std::ptr::null(), len: 0 }, null: true, _m: PhantomData }
    }
    #[inline(always)]
    pub fn new(v: &'a [T]) -> Self {
        ArrArg { s: StratumSlice { ptr: v.as_ptr() as *const c_void, len: v.len() }, null: false, _m: PhantomData }
    }
    #[inline(always)]
    pub fn arg(&self) -> StratumArg {
        let mut a = StratumArg { i: 0 };
        a.p = if self.null { std::ptr::null() } else { &self.s as *const StratumSlice as *const c_void };
        a
    }
}
impl<'a, T> From<&'a [T]> for ArrArg<'a, T> {
    #[inline(always)]
    fn from(v: &'a [T]) -> Self {
        ArrArg::new(v)
    }
}
impl<'a, T> From<&'a Vec<T>> for ArrArg<'a, T> {
    #[inline(always)]
    fn from(v: &'a Vec<T>) -> Self {
        ArrArg::new(v.as_slice())
    }
}
impl<'a, T, const N: usize> From<&'a [T; N]> for ArrArg<'a, T> {
    #[inline(always)]
    fn from(v: &'a [T; N]) -> Self {
        ArrArg::new(&v[..])
    }
}
impl<'a, T> From<Option<&'a [T]>> for ArrArg<'a, T> {
    #[inline]
    fn from(v: Option<&'a [T]>) -> Self {
        match v {
            Some(s) => ArrArg::new(s),
            None => ArrArg::null(),
        }
    }
}

/// String[] argument.
pub struct StrArrArg<'a> {
    items: Vec<StratumStr>,
    s: StratumSlice,
    null: bool,
    _m: PhantomData<&'a str>,
}
impl<'a> StrArrArg<'a> {
    pub fn null() -> Self {
        StrArrArg { items: Vec::new(), s: StratumSlice { ptr: std::ptr::null(), len: 0 }, null: true, _m: PhantomData }
    }
    fn build<I: Iterator<Item = &'a str>>(it: I) -> Self {
        let items: Vec<StratumStr> = it.map(|x| StratumStr { ptr: x.as_ptr() as *const c_char, len: x.len() }).collect();
        let s = StratumSlice { ptr: items.as_ptr() as *const c_void, len: items.len() };
        StrArrArg { items, s, null: false, _m: PhantomData }
    }
    #[inline]
    pub fn arg(&self) -> StratumArg {
        let mut a = StratumArg { i: 0 };
        a.p = if self.null { std::ptr::null() } else { &self.s as *const StratumSlice as *const c_void };
        a
    }
}
impl<'a> From<&'a [&'a str]> for StrArrArg<'a> {
    fn from(v: &'a [&'a str]) -> Self {
        StrArrArg::build(v.iter().copied())
    }
}
impl<'a> From<&'a Vec<&'a str>> for StrArrArg<'a> {
    fn from(v: &'a Vec<&'a str>) -> Self {
        StrArrArg::build(v.iter().copied())
    }
}
impl<'a, const N: usize> From<&'a [&'a str; N]> for StrArrArg<'a> {
    fn from(v: &'a [&'a str; N]) -> Self {
        StrArrArg::build(v.iter().copied())
    }
}
impl<'a> From<&'a [String]> for StrArrArg<'a> {
    fn from(v: &'a [String]) -> Self {
        StrArrArg::build(v.iter().map(|s| s.as_str()))
    }
}
impl<'a> From<&'a Vec<String>> for StrArrArg<'a> {
    fn from(v: &'a Vec<String>) -> Self {
        StrArrArg::build(v.iter().map(|s| s.as_str()))
    }
}

/// Object[] / View[] / int[][] ... argument (element class comes from the real method descriptor in the engine).
pub struct ObjArrArg {
    h: Vec<i64>,
    s: StratumSlice,
    null: bool,
}
impl ObjArrArg {
    pub fn null() -> Self {
        ObjArrArg { h: Vec::new(), s: StratumSlice { ptr: std::ptr::null(), len: 0 }, null: true }
    }
    fn build(h: Vec<i64>) -> Self {
        let s = StratumSlice { ptr: h.as_ptr() as *const c_void, len: h.len() };
        ObjArrArg { h, s, null: false }
    }
    #[inline]
    pub fn arg(&self) -> StratumArg {
        let mut a = StratumArg { i: 0 };
        a.p = if self.null { std::ptr::null() } else { &self.s as *const StratumSlice as *const c_void };
        a
    }
}
impl<'a, T: AsRaw> From<&'a [T]> for ObjArrArg {
    fn from(v: &'a [T]) -> Self {
        ObjArrArg::build(v.iter().map(|o| o.handle()).collect())
    }
}
impl<'a, T: AsRaw> From<&'a Vec<T>> for ObjArrArg {
    fn from(v: &'a Vec<T>) -> Self {
        ObjArrArg::build(v.iter().map(|o| o.handle()).collect())
    }
}
impl<'a, T: AsRaw, const N: usize> From<&'a [T; N]> for ObjArrArg {
    fn from(v: &'a [T; N]) -> Self {
        ObjArrArg::build(v.iter().map(|o| o.handle()).collect())
    }
}

/// java.util.List / Collection / Iterable argument: an existing Java object, or a Rust slice built into an ArrayList.
pub struct ListArg {
    h: Vec<i64>,
    r: StratumListArg,
    null: bool,
}
impl ListArg {
    pub fn null() -> Self {
        ListArg { h: Vec::new(), r: StratumListArg { existing: 0, ptrs: std::ptr::null(), len: 0 }, null: true }
    }
    fn build(h: Vec<i64>) -> Self {
        let r = StratumListArg { existing: 0, ptrs: h.as_ptr(), len: h.len() };
        ListArg { h, r, null: false }
    }
    #[inline]
    pub fn arg(&self) -> StratumArg {
        let mut a = StratumArg { i: 0 };
        a.p = if self.null { std::ptr::null() } else { &self.r as *const StratumListArg as *const c_void };
        a
    }
}
impl<'a, T: AsRaw> From<&'a T> for ListArg {
    fn from(existing: &'a T) -> Self {
        ListArg { h: Vec::new(), r: StratumListArg { existing: existing.handle(), ptrs: std::ptr::null(), len: 0 }, null: false }
    }
}
impl<'a, T: AsRaw> From<&'a [T]> for ListArg {
    fn from(v: &'a [T]) -> Self {
        ListArg::build(v.iter().map(|o| o.handle()).collect())
    }
}
impl<'a, T: AsRaw> From<&'a Vec<T>> for ListArg {
    fn from(v: &'a Vec<T>) -> Self {
        ListArg::build(v.iter().map(|o| o.handle()).collect())
    }
}
impl<'a, T: AsRaw, const N: usize> From<&'a [T; N]> for ListArg {
    fn from(v: &'a [T; N]) -> Self {
        ListArg::build(v.iter().map(|o| o.handle()).collect())
    }
}

/// java.util.Map / HashMap argument.
pub struct MapArg {
    k: Vec<i64>,
    v: Vec<i64>,
    r: StratumMapArg,
    null: bool,
}
impl MapArg {
    pub fn null() -> Self {
        MapArg { k: Vec::new(), v: Vec::new(), r: StratumMapArg { existing: 0, keys: std::ptr::null(), vals: std::ptr::null(), len: 0 }, null: true }
    }
    fn build(k: Vec<i64>, v: Vec<i64>) -> Self {
        let r = StratumMapArg { existing: 0, keys: k.as_ptr(), vals: v.as_ptr(), len: k.len() };
        MapArg { k, v, r, null: false }
    }
    #[inline]
    pub fn arg(&self) -> StratumArg {
        let mut a = StratumArg { i: 0 };
        a.p = if self.null { std::ptr::null() } else { &self.r as *const StratumMapArg as *const c_void };
        a
    }
}
impl<'a, T: AsRaw> From<&'a T> for MapArg {
    fn from(existing: &'a T) -> Self {
        MapArg {
            k: Vec::new(),
            v: Vec::new(),
            r: StratumMapArg { existing: existing.handle(), keys: std::ptr::null(), vals: std::ptr::null(), len: 0 },
            null: false,
        }
    }
}
impl<'a, K: AsRaw, V: AsRaw> From<&'a [(K, V)]> for MapArg {
    fn from(v: &'a [(K, V)]) -> Self {
        MapArg::build(v.iter().map(|p| p.0.handle()).collect(), v.iter().map(|p| p.1.handle()).collect())
    }
}
impl<'a, K: AsRaw, V: AsRaw> From<&'a Vec<(K, V)>> for MapArg {
    fn from(v: &'a Vec<(K, V)>) -> Self {
        MapArg::build(v.iter().map(|p| p.0.handle()).collect(), v.iter().map(|p| p.1.handle()).collect())
    }
}

// ── returned arrays ──────────────────────────────────────────────────────────
#[inline(always)]
pub fn empty_arr() -> StratumArr {
    StratumArr::empty()
}
pub fn arr_vec<T: Copy>(a: &mut StratumArr) -> Vec<T> {
    let v = if a.len > 0 && !a.data.is_null() {
        unsafe { slice::from_raw_parts(a.data as *const T, a.len).to_vec() }
    } else {
        Vec::new()
    };
    unsafe { stratum_arr_free(a) };
    v
}
fn arr_vec_k<T: Copy>(a: &mut StratumArr, kind: u8) -> Vec<T> {
    if a.kind != kind {
        unsafe { stratum_arr_free(a) };
        return Vec::new();
    }
    arr_vec::<T>(a)
}
pub fn arr_bool(a: &mut StratumArr) -> Vec<bool> {
    arr_vec::<u8>(a).into_iter().map(|b| b != 0).collect()
}
pub fn arr_strings(a: &mut StratumArr) -> Vec<String> {
    let mut v = Vec::new();
    if a.kind == b'T' && !a.data.is_null() {
        let it = unsafe { slice::from_raw_parts(a.data as *const StratumStr, a.len) };
        for s in it {
            v.push(to_std(s));
        }
    }
    unsafe { stratum_arr_free(a) };
    v
}
/// Adopts the handles (kind 'O'); a String[] returned for an Object[] declaration is converted to java Strings.
pub fn arr_objs(a: &mut StratumArr) -> Vec<Object> {
    let mut v = Vec::new();
    unsafe {
        if a.kind == b'O' && !a.data.is_null() {
            let h = slice::from_raw_parts_mut(a.data as *mut i64, a.len);
            for x in h.iter_mut() {
                v.push(Object::adopt(*x));
                *x = 0; // ownership moved: stratum_arr_free skips null handles, only frees the buffer
            }
        } else if a.kind == b'T' && !a.data.is_null() {
            let it = slice::from_raw_parts(a.data as *const StratumStr, a.len);
            for s in it {
                v.push(if s.ptr.is_null() { Object::null() } else { java_string(&to_std(s)) });
            }
        }
        stratum_arr_free(a);
    }
    v
}
pub fn map_entries(m: &Object) -> Result<Vec<(Object, Object)>> {
    let (mut k, mut v) = (empty_arr(), empty_arr());
    let mut e = 0i32;
    unsafe { stratum_get_map_entries(m.raw(), &mut k, &mut v, &mut e) };
    check(e)?;
    Ok(arr_objs(&mut k).into_iter().zip(arr_objs(&mut v).into_iter()).collect())
}

/// Element types accepted by `Object::as_array`.
pub trait FromArr: Sized {
    fn from_arr(a: &mut StratumArr) -> Vec<Self>;
}
macro_rules! arr_prim {
    ($(($t:ty, $k:expr)),* $(,)?) => { $( impl FromArr for $t { fn from_arr(a: &mut StratumArr) -> Vec<$t> { arr_vec_k::<$t>(a, $k) } } )* };
}
arr_prim!((u8, b'B'), (i8, b'B'), (i16, b'S'), (u16, b'C'), (i32, b'I'), (i64, b'J'), (f32, b'F'), (f64, b'D'));
impl FromArr for bool {
    fn from_arr(a: &mut StratumArr) -> Vec<bool> {
        if a.kind != b'Z' {
            unsafe { stratum_arr_free(a) };
            return Vec::new();
        }
        arr_bool(a)
    }
}
impl FromArr for String {
    fn from_arr(a: &mut StratumArr) -> Vec<String> {
        arr_strings(a)
    }
}
impl FromArr for Object {
    fn from_arr(a: &mut StratumArr) -> Vec<Object> {
        arr_objs(a)
    }
}

// ── callbacks (Java -> Rust) ─────────────────────────────────────────────────
/// The arguments of one callback invocation. Lifetime-free on purpose (keeps `|a: &Args|` closures simple);
/// it is only ever handed out as `&Args` and cannot outlive the callback.
#[derive(Clone, Copy)]
pub struct Args {
    p: *const StratumVal,
    n: usize,
}
impl Args {
    #[inline(always)]
    pub unsafe fn from_raw(p: *const StratumVal, n: usize) -> Args {
        Args { p, n }
    }
    #[inline]
    fn slice(&self) -> &[StratumVal] {
        if self.p.is_null() || self.n == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.p, self.n) }
        }
    }
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.n
    }
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
    #[inline]
    pub fn get(&self, i: usize) -> CbArg<'_> {
        CbArg { v: self.slice().get(i) }
    }
    #[inline]
    pub fn is_null(&self, i: usize) -> bool {
        self.get(i).is_null()
    }
    #[inline]
    pub fn obj(&self, i: usize) -> Object {
        self.get(i).obj()
    }
    #[inline]
    pub fn i32(&self, i: usize) -> i32 {
        self.get(i).i() as i32
    }
    #[inline]
    pub fn i64(&self, i: usize) -> i64 {
        self.get(i).i()
    }
    #[inline]
    pub fn f32(&self, i: usize) -> f32 {
        self.get(i).d() as f32
    }
    #[inline]
    pub fn f64(&self, i: usize) -> f64 {
        self.get(i).d()
    }
    #[inline]
    pub fn bool(&self, i: usize) -> bool {
        self.get(i).i() != 0
    }
    pub fn str(&self, i: usize) -> String {
        self.get(i).str()
    }
    pub fn cast<T: JavaClass>(&self, i: usize) -> Result<T> {
        self.obj(i).cast::<T>()
    }
}
#[derive(Clone, Copy)]
pub struct CbArg<'a> {
    v: Option<&'a StratumVal>,
}
impl<'a> CbArg<'a> {
    #[inline]
    pub fn is_null(&self) -> bool {
        match self.v {
            None => true,
            Some(v) => v.tag == STRATUM_V_NULL,
        }
    }
    /// A cloned global ref: safe to keep after the callback returns.
    pub fn obj(&self) -> Object {
        match self.v {
            Some(v) if v.tag == STRATUM_V_OBJ => Object::adopt(unsafe { stratum_clone_ref(v.v.i) }),
            _ => Object::null(),
        }
    }
    #[inline]
    pub fn i(&self) -> i64 {
        match self.v {
            None => 0,
            Some(v) => unsafe {
                if v.tag == STRATUM_V_FLOAT || v.tag == STRATUM_V_DOUBLE {
                    v.v.d as i64
                } else {
                    v.v.i
                }
            },
        }
    }
    #[inline]
    pub fn d(&self) -> f64 {
        match self.v {
            None => 0.0,
            Some(v) => unsafe {
                if v.tag == STRATUM_V_FLOAT || v.tag == STRATUM_V_DOUBLE {
                    v.v.d
                } else {
                    v.v.i as f64
                }
            },
        }
    }
    pub fn str(&self) -> String {
        match self.v {
            None => String::new(),
            Some(v) => unsafe {
                if v.tag == STRATUM_V_STR {
                    if v.v.s.ptr.is_null() || v.v.s.len == 0 {
                        String::new()
                    } else {
                        String::from_utf8_lossy(slice::from_raw_parts(v.v.s.ptr as *const u8, v.v.s.len)).into_owned()
                    }
                } else if v.tag == STRATUM_V_OBJ {
                    Object::adopt(stratum_clone_ref(v.v.i)).to_string()
                } else {
                    String::new()
                }
            },
        }
    }
}

thread_local! {
    static RET_BUF: RefCell<String> = RefCell::new(String::new());
    static RET_INTS: RefCell<Vec<i32>> = RefCell::new(Vec::new());
}
fn put_str(o: &mut StratumVal, s: &str) {
    RET_BUF.with(|b| {
        let mut b = b.borrow_mut();
        b.clear();
        b.push_str(s);
        o.tag = STRATUM_V_STR;
        o.v.s = StratumValStr { ptr: b.as_ptr() as *const c_void, len: b.len() };
    });
}
pub fn set_err(o: &mut StratumVal, msg: &str) {
    put_str(o, msg);
    o.tag = STRATUM_V_ERR;
}
#[inline]
pub fn put_obj(o: &mut StratumVal, h: i64) {
    if h != 0 {
        o.tag = STRATUM_V_OBJ;
        o.v.i = unsafe { stratum_clone_ref(h) };
    } else {
        o.tag = STRATUM_V_NULL;
    }
}

/// What a callback closure may return.
pub trait IntoRet {
    fn set_ret(self, out: &mut StratumVal);
}
impl IntoRet for () {
    #[inline(always)]
    fn set_ret(self, _o: &mut StratumVal) {}
}
impl IntoRet for bool {
    #[inline]
    fn set_ret(self, o: &mut StratumVal) {
        o.tag = STRATUM_V_BOOL;
        o.v.i = self as i64;
    }
}
macro_rules! ret_int {
    ($($t:ty => $tag:expr),* $(,)?) => { $( impl IntoRet for $t { #[inline] fn set_ret(self, o: &mut StratumVal) { o.tag = $tag; o.v.i = self as i64; } } )* };
}
ret_int!(i8 => STRATUM_V_INT, i16 => STRATUM_V_INT, i32 => STRATUM_V_INT, u8 => STRATUM_V_INT, u16 => STRATUM_V_INT,
         u32 => STRATUM_V_LONG, i64 => STRATUM_V_LONG, u64 => STRATUM_V_LONG, isize => STRATUM_V_LONG, usize => STRATUM_V_LONG);
/// Return a Java `char`.
pub struct JChar(pub u16);
impl IntoRet for JChar {
    #[inline]
    fn set_ret(self, o: &mut StratumVal) {
        o.tag = STRATUM_V_CHAR;
        o.v.i = self.0 as i64;
    }
}
impl IntoRet for f32 {
    #[inline]
    fn set_ret(self, o: &mut StratumVal) {
        o.tag = STRATUM_V_FLOAT;
        o.v.d = self as f64;
    }
}
impl IntoRet for f64 {
    #[inline]
    fn set_ret(self, o: &mut StratumVal) {
        o.tag = STRATUM_V_DOUBLE;
        o.v.d = self;
    }
}
impl IntoRet for &str {
    fn set_ret(self, o: &mut StratumVal) {
        put_str(o, self);
    }
}
impl IntoRet for String {
    fn set_ret(self, o: &mut StratumVal) {
        put_str(o, &self);
    }
}
impl IntoRet for Object {
    #[inline]
    fn set_ret(self, o: &mut StratumVal) {
        put_obj(o, self.raw());
    }
}
impl IntoRet for Vec<i32> {
    fn set_ret(self, o: &mut StratumVal) {
        RET_INTS.with(|b| {
            let mut b = b.borrow_mut();
            *b = self;
            o.tag = STRATUM_V_I32ARR;
            o.v.s = StratumValStr { ptr: b.as_ptr() as *const c_void, len: b.len() };
        });
    }
}
impl<T: IntoRet> IntoRet for Option<T> {
    #[inline]
    fn set_ret(self, o: &mut StratumVal) {
        if let Some(x) = self {
            x.set_ret(o);
        }
    }
}
/// `Err(e)` becomes a RuntimeException on the Java side, so `?` works inside callbacks.
impl<T: IntoRet, E: fmt::Display> IntoRet for std::result::Result<T, E> {
    fn set_ret(self, o: &mut StratumVal) {
        match self {
            Ok(x) => x.set_ret(o),
            Err(e) => set_err(o, &e.to_string()),
        }
    }
}

fn panic_msg(p: Box<dyn Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        format!("Rust panic in callback: {}", s)
    } else if let Some(s) = p.downcast_ref::<String>() {
        format!("Rust panic in callback: {}", s)
    } else {
        "Rust panic in callback".to_string()
    }
}

unsafe extern "C" fn tramp<F, R>(ud: *mut c_void, _m: *const c_char, a: *const StratumVal, n: usize, out: *mut StratumVal)
where
    F: Fn(Args) -> R,
    R: IntoRet,
{
    let f = &*(ud as *const F);
    let args = Args::from_raw(a, n);
    let out = &mut *out;
    match catch_unwind(AssertUnwindSafe(|| f(args))) {
        Ok(r) => r.set_ret(out),
        Err(p) => set_err(out, &panic_msg(p)),
    }
}
unsafe extern "C" fn drop_box<T>(p: *mut c_void) {
    drop(Box::from_raw(p as *mut T));
}

/// Owns one C-ABI callback descriptor until the engine takes it over (then userdata is NULL and drop is a no-op).
/// The engine nulls `userdata` THROUGH the pointer we hand out, hence the UnsafeCell.
pub struct Callback {
    desc: UnsafeCell<StratumCbDesc>,
}
impl Callback {
    /// `Fn` (not FnMut): callbacks may be re-entered or run on any thread. Use Cell/Mutex for state.
    pub fn new<F, R>(f: F) -> Callback
    where
        F: Fn(Args) -> R + Send + Sync + 'static,
        R: IntoRet,
    {
        let ud = Box::into_raw(Box::new(f)) as *mut c_void;
        Callback {
            desc: UnsafeCell::new(StratumCbDesc {
                userdata: ud,
                fn_: Some(tramp::<F, R> as StratumTramp),
                drop: Some(drop_box::<F> as StratumDropFn),
                existing: 0,
            }),
        }
    }
    /// Pass an existing Java listener object as-is (no adapter/proxy is built).
    pub fn existing(o: &Object) -> Callback {
        Callback { desc: UnsafeCell::new(StratumCbDesc { userdata: std::ptr::null_mut(), fn_: None, drop: None, existing: o.raw() }) }
    }
    #[inline]
    pub fn arg(&self) -> StratumArg {
        let mut a = StratumArg { i: 0 };
        a.p = self.desc.get() as *const c_void;
        a
    }
    #[inline]
    pub fn desc_ptr(&self) -> *const StratumCbDesc {
        self.desc.get() as *const StratumCbDesc
    }
}
impl Drop for Callback {
    fn drop(&mut self) {
        let d = unsafe { *self.desc.get() };
        if !d.userdata.is_null() {
            if let Some(f) = d.drop {
                unsafe { f(d.userdata) };
            }
        }
    }
}

/// Multi-method listeners / abstract classes (WebViewClient, TextWatcher ...):
///     Handlers::new().on("onPageFinished", |a| { ... }).on("shouldOverrideUrlLoading", |a| false)
pub struct Handlers {
    map: HashMap<String, Box<dyn Fn(Args, &mut StratumVal) + Send + Sync>>,
}
impl Handlers {
    pub fn new() -> Handlers {
        Handlers { map: HashMap::new() }
    }
    pub fn on<F, R>(mut self, name: &str, f: F) -> Handlers
    where
        F: Fn(Args) -> R + Send + Sync + 'static,
        R: IntoRet,
    {
        self.map.insert(name.to_string(), Box::new(move |a: Args, o: &mut StratumVal| f(a).set_ret(o)));
        self
    }
}
impl Default for Handlers {
    fn default() -> Self {
        Handlers::new()
    }
}
unsafe extern "C" fn tramp_handlers(ud: *mut c_void, m: *const c_char, a: *const StratumVal, n: usize, out: *mut StratumVal) {
    let h = &*(ud as *const Handlers);
    let name = if m.is_null() { String::new() } else { CStr::from_ptr(m).to_string_lossy().into_owned() };
    if let Some(f) = h.map.get(&name) {
        let args = Args::from_raw(a, n);
        let out = &mut *out;
        if let Err(p) = catch_unwind(AssertUnwindSafe(|| f(args, out))) {
            set_err(out, &panic_msg(p));
        }
    }
}

/// Anything accepted where the Java API wants a listener/callback/abstract-class adapter.
pub trait IntoCallback {
    fn into_callback(self) -> Callback;
}
impl IntoCallback for Callback {
    #[inline(always)]
    fn into_callback(self) -> Callback {
        self
    }
}
impl IntoCallback for Handlers {
    fn into_callback(self) -> Callback {
        let ud = Box::into_raw(Box::new(self)) as *mut c_void;
        Callback {
            desc: UnsafeCell::new(StratumCbDesc {
                userdata: ud,
                fn_: Some(tramp_handlers as StratumTramp),
                drop: Some(drop_box::<Handlers> as StratumDropFn),
                existing: 0,
            }),
        }
    }
}
/// Plain closures: `view.setOnClickListener(|_: Args| { ... })`. `cb(|a| ...)` still works and lets type inference work without explicit type.
impl<F, R> IntoCallback for F
where
    F: Fn(Args) -> R + Send + Sync + 'static,
    R: IntoRet,
{
    #[inline]
    fn into_callback(self) -> Callback {
        Callback::new(self)
    }
}
/// `view.setOnClickListener(cb(|a| { ...; }))`
#[inline]
pub fn cb<F, R>(f: F) -> Callback
where
    F: Fn(Args) -> R + Send + Sync + 'static,
    R: IntoRet,
{
    Callback::new(f)
}
/// Closure that takes no arguments: `cb0(|| ...)`.
#[inline]
pub fn cb0<F, R>(f: F) -> Callback
where
    F: Fn() -> R + Send + Sync + 'static,
    R: IntoRet,
{
    Callback::new(move |_a: Args| f())
}