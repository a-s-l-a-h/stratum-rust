//! stratum::reflect: call ANY Java class by name (your own app classes, third-party SDKs) with no generated wrapper.
//! (Rust counterpart of Stage 21's reflect.hpp.)
//!
//!     use stratum::{rargs, reflect};
//!     let n: i32 = reflect::call_static("java.lang.Integer", "parseInt", rargs!["42"])?;
//! Java arrays as arguments: build them first: `let a = new_array(&[1, 2, 3])?; ... rargs![&a]`.
use crate::base::*;
use crate::ffi::*;
use std::ffi::CString;
use std::os::raw::c_void;

pub enum RArg<'a> {
    Null,
    Bool(bool),
    I8(i8),
    I16(i16),
    Char(u16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Str(&'a str),
    Obj(i64),
}
macro_rules! rarg_from {
    ($($t:ty => $v:ident),* $(,)?) => { $( impl<'a> From<$t> for RArg<'a> { #[inline] fn from(x: $t) -> Self { RArg::$v(x) } } )* };
}
rarg_from!(bool => Bool, i8 => I8, i16 => I16, u16 => Char, i32 => I32, i64 => I64, f32 => F32, f64 => F64, &'a str => Str);
impl<'a> From<&'a String> for RArg<'a> {
    fn from(x: &'a String) -> Self {
        RArg::Str(x.as_str())
    }
}
/// `&Object` and every generated class (`&TextView`, `&Activity` ...).
impl<'a, T: AsRaw> From<&'a T> for RArg<'a> {
    #[inline]
    fn from(x: &'a T) -> Self {
        if x.handle() == 0 {
            RArg::Null
        } else {
            RArg::Obj(x.handle())
        }
    }
}
impl<'a> From<()> for RArg<'a> {
    fn from(_: ()) -> Self {
        RArg::Null
    }
}

/// `rargs![1, "text", &obj]` -> `&[RArg]`
#[macro_export]
macro_rules! rargs {
    () => { &[] as &[$crate::reflect::RArg] };
    ($($a:expr),+ $(,)?) => { &[$($crate::reflect::RArg::from($a)),+] };
}

/// Result types for reflect calls.
pub trait FromReflect: Sized {
    fn from_reflect(v: &StratumVal, obj: Option<Object>) -> Self;
}
impl FromReflect for () {
    fn from_reflect(_v: &StratumVal, _o: Option<Object>) -> Self {}
}
impl FromReflect for bool {
    fn from_reflect(v: &StratumVal, _o: Option<Object>) -> Self {
        unsafe { v.v.i != 0 }
    }
}
fn as_i(v: &StratumVal) -> i64 {
    unsafe {
        if v.tag == STRATUM_V_FLOAT || v.tag == STRATUM_V_DOUBLE {
            v.v.d as i64
        } else {
            v.v.i
        }
    }
}
fn as_d(v: &StratumVal) -> f64 {
    unsafe {
        if v.tag == STRATUM_V_FLOAT || v.tag == STRATUM_V_DOUBLE {
            v.v.d
        } else {
            v.v.i as f64
        }
    }
}
macro_rules! from_int {
    ($($t:ty),*) => { $( impl FromReflect for $t { fn from_reflect(v: &StratumVal, _o: Option<Object>) -> Self { as_i(v) as $t } } )* };
}
from_int!(i8, i16, u16, i32, i64);
impl FromReflect for f32 {
    fn from_reflect(v: &StratumVal, _o: Option<Object>) -> Self {
        as_d(v) as f32
    }
}
impl FromReflect for f64 {
    fn from_reflect(v: &StratumVal, _o: Option<Object>) -> Self {
        as_d(v)
    }
}
impl FromReflect for String {
    fn from_reflect(v: &StratumVal, _o: Option<Object>) -> Self {
        unsafe {
            if v.tag == STRATUM_V_STR && !v.v.s.ptr.is_null() && v.v.s.len > 0 {
                String::from_utf8_lossy(std::slice::from_raw_parts(v.v.s.ptr as *const u8, v.v.s.len)).into_owned()
            } else {
                String::new()
            }
        }
    }
}
impl FromReflect for Object {
    fn from_reflect(_v: &StratumVal, o: Option<Object>) -> Self {
        o.unwrap_or_default()
    }
}

fn pack(args: &[RArg]) -> Vec<StratumVal> {
    args.iter()
        .map(|a| {
            let mut s = StratumVal::zeroed();
            match a {
                RArg::Null => s.tag = STRATUM_V_NULL,
                RArg::Bool(x) => {
                    s.tag = STRATUM_V_BOOL;
                    s.v.i = *x as i64;
                }
                RArg::I8(x) => {
                    s.tag = STRATUM_V_BYTE;
                    s.v.i = *x as i64;
                }
                RArg::I16(x) => {
                    s.tag = STRATUM_V_SHORT;
                    s.v.i = *x as i64;
                }
                RArg::Char(x) => {
                    s.tag = STRATUM_V_CHAR;
                    s.v.i = *x as i64;
                }
                RArg::I32(x) => {
                    s.tag = STRATUM_V_INT;
                    s.v.i = *x as i64;
                }
                RArg::I64(x) => {
                    s.tag = STRATUM_V_LONG;
                    s.v.i = *x;
                }
                RArg::F32(x) => {
                    s.tag = STRATUM_V_FLOAT;
                    s.v.d = *x as f64;
                }
                RArg::F64(x) => {
                    s.tag = STRATUM_V_DOUBLE;
                    s.v.d = *x;
                }
                RArg::Str(x) => {
                    s.tag = STRATUM_V_STR;
                    s.v.s = StratumValStr { ptr: x.as_ptr() as *const c_void, len: x.len() };
                }
                RArg::Obj(h) => {
                    s.tag = STRATUM_V_OBJ;
                    s.v.i = *h;
                }
            }
            s
        })
        .collect()
}

fn call<R: FromReflect>(cls: Option<&str>, target: i64, method: Option<&str>, args: &[RArg]) -> Result<R> {
    let ccls = match cls {
        Some(c) => Some(CString::new(c).map_err(|e| JavaException(e.to_string()))?),
        None => None,
    };
    let cmethod = match method {
        Some(m) => Some(CString::new(m).map_err(|e| JavaException(e.to_string()))?),
        None => None,
    };
    let vals = pack(args);
    let mut out = StratumVal::zeroed();
    let rc = unsafe {
        stratum_reflect_call(
            ccls.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
            target,
            cmethod.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
            vals.as_ptr(),
            vals.len(),
            &mut out,
        )
    };
    if rc != 0 {
        return Err(JavaException(last_error()));
    }
    // OBJ results are new global refs owned by us: always adopt so nothing leaks even if R ignores it.
    let obj = if out.tag == STRATUM_V_OBJ { Some(Object::adopt(unsafe { out.v.i })) } else { None };
    Ok(R::from_reflect(&out, obj))
}

pub fn call_static<R: FromReflect>(cls: &str, method: &str, args: &[RArg]) -> Result<R> {
    call(Some(cls), 0, Some(method), args)
}
pub fn call_method<R: FromReflect>(target: &Object, method: &str, args: &[RArg]) -> Result<R> {
    if target.is_null() {
        return Err(JavaException("reflect: null target".to_string()));
    }
    call(None, target.raw(), Some(method), args)
}
pub fn new_instance(cls: &str, args: &[RArg]) -> Result<Object> {
    call(Some(cls), 0, None, args)
}