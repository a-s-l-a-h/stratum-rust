# Stratum Rust API Reference (for LLMs)

Give this file to an LLM and ask it to write `app/src/main/rust/src/lib.rs`. It contains everything needed.

Stratum lets you write an Android app in **Rust** against the **real Android SDK** (Java classes) with no Java glue except a 6-line `MainActivity`. The Rust crate `stratum` is generated from `android.jar`; every Java class becomes a Rust struct and every method a Rust function.

---

## 1. Minimal app

```rust
use stratum::android::widget::{Button, LinearLayout, TextView};
use stratum::*;

stratum_main!(|| {
    on_create(|act: Activity| {
        let root = LinearLayout::new(&act).unwrap();
        root.setOrientation(1).unwrap();                 // VERTICAL
        let tv = TextView::new(&act).unwrap();
        tv.setText("Hello from Rust").unwrap();
        root.addView(&tv).unwrap();

        let btn = Button::new(&act).unwrap();
        btn.setText("Tap").unwrap();
        let label = tv.clone();
        btn.setOnClickListener(move |_: Args| {
            label.setText("Clicked").unwrap();
        }).unwrap();
        root.addView(&btn).unwrap();

        set_content_view(&root);
    });
});
```

Java side (`MainActivity.java`), nothing else is needed:

```java
public class MainActivity extends com.stratum.runtime.StratumActivity {
    @Override protected boolean onImportMain() { System.loadLibrary("myapp"); return true; }
}
```

`stratum_main!` takes a **non-capturing** closure (it is coerced to `fn()`); it defines `JNI_OnLoad`. Requires Rust edition 2021 and `panic = unwind` (never `abort`).

---

## 2. Naming rules (Java -> Rust)

| Java | Rust |
|---|---|
| package `android.widget` | module `stratum::android::widget` |
| class `TextView` | `stratum::android::widget::TextView` |
| inner class `Build$VERSION` | `Build_VERSION` (`$` -> `_`) |
| keyword segment / name (`type`, `ref`, `match`...) | trailing underscore (`type_`) |
| class name equals a child package name | trailing `_` |
| Java name colliding with a Rust-side helper (`release`, `cast`, `clone`, `hash`, `default`, `wrap`, `create`, `null`, `raw`, `handle`, `into`, `from`, ...) | trailing `_` (`release_`) |
| method `setText` | `setText` (Java camelCase is kept) |
| constructor | `Type::new(...)` |
| static method | associated fn: `Integer::compare(a, b)` |
| instance method | `&self` method (inherited methods are flattened into every class) |
| `static final` compile-time constant | `pub const NAME: type` (e.g. `View::GONE`) |
| other static field | `Type::sf_get_NAME()` / `Type::sf_set_NAME(v)` |
| instance field | `obj.f_get_NAME()` / `obj.f_set_NAME(v)` |

Some names clash with Rust std; alias them:
```rust
use stratum::java::lang::String as JString;
use stratum::java::lang::Integer as JInteger;
```

### Overloads
Rust has no overloading. For each Java name the "most natural" overload keeps the plain name (non-deprecated, fewest params, then `String` < object/callback < array/collection < primitive). Others get a suffix made of **only the parameter types that differ**:

```
setText(CharSequence)   -> setText("..")
setText(int resId)      -> setText_int(5)
registerListener(SensorEventListener, Sensor, int) -> registerListener(handlers, &sensor, 2)
new Integer(int)        -> JInteger::new_int(5)
new String(byte[], String) -> JString::new_byteArr_String(&bytes, "UTF-8")
```
Primitive arrays in suffixes are written `byteArr`, `intArr`, ... Find exact names with:
`grep "TextView: setText" app/src/main/rust/stratum/API_INDEX.txt`
(line format: `android.widget.TextView: setText(impl Into<JStr>) -> ()`).

### Only emitted classes exist
The generator was run with `--emit-prefix`. Classes outside it do not exist as typed wrappers (use `reflect`, section 8, for those). Private and hidden/blocked APIs are never emitted.

---

## 3. Every call returns `Result`

```rust
type JResult<T> = Result<T, JavaException>;   // stratum::JResult
```
Use `?` / `.unwrap()` / `let _ =`. A Java exception becomes `Err(JavaException(msg))`.

---

## 4. Types: Java parameter -> Rust argument

| Java parameter | Rust argument |
|---|---|
| boolean / byte / char / short / int / long / float / double | `bool` / `i8` / `u16` / `i16` / `i32` / `i64` / `f32` / `f64` |
| `String`, `CharSequence` | `&str`, `&String`, `String` (owned, e.g. `format!(..)`), `Option<&str>` (None = null) |
| `byte[]` | `&[u8]` / `&Vec<u8>` / `&[u8; N]` / `Option<&[u8]>` |
| `int[]` `long[]` `float[]` `double[]` `boolean[]` `char[]` `short[]` | same with `i32` `i64` `f32` `f64` `bool` `u16` `i16` |
| `String[]` | `&[&str]`, `&Vec<&str>`, `&[&str; N]`, `&[String]`, `&Vec<String>` |
| `Object[]`, `View[]`, any object array | `&[T]`, `&Vec<T>`, `&[T; N]` where `T` is `Object` or a generated class |
| `List`, `Collection`, `Iterable` | `&existing_java_obj`, `&[T]`, `&Vec<T>` (builds an `ArrayList`) |
| `Map`, `HashMap` | `&existing_java_obj`, `&[(K, V)]`, `&Vec<(K, V)>` (builds a `HashMap`; K, V are Object-like) |
| listener / callback / abstract class | closure `\|_: Args\| ...`, `Handlers`, `Callback` (section 6) |
| `Integer`, `Long`, `Boolean`, ... (boxed) | `Option<i32>`, `Option<i64>`, `Option<bool>`, ... |
| any other object | `&Object` or `&AnyGeneratedClass` (deref coercion) |

## 5. Types: Java return -> Rust value

| Java return | Rust |
|---|---|
| void | `()` |
| boolean/byte/char/short/int/long/float/double | `bool/i8/u16/i16/i32/i64/f32/f64` |
| `String`, `CharSequence` | `String` (null -> empty string) |
| `byte[]` `int[]` ... | `Vec<u8>` `Vec<i32>` ... (`boolean[]` -> `Vec<bool>`, `String[]` -> `Vec<String>`) |
| other arrays, `List`, `Collection` | `Vec<Object>` |
| `Map` | `Object` -> `.as_map()?` gives `Vec<(Object, Object)>` |
| boxed (`Integer`...) | `Object` -> `.as_i32()?` / `.as_i64()?` / `.as_bool()?` / `.as_float()?` / `.as_double()?` ... |
| any other object | `Object` (untyped). Convert: `obj.cast::<TextView>()?` (checked) or `obj.unchecked_cast::<TextView>()` |

### `Object`
Owns one JNI global ref. `clone()` makes a new global ref, `drop` frees it (there are ~51k global refs, don't leak thousands). Null is a valid value.
```
Object::null()  is_null()  raw()  hash_code()  is_instance_of::<T>()  cast::<T>()?  unchecked_cast::<T>()
as_bool/as_i8/as_char/as_i16/as_i32/as_i64/as_float/as_double ()?   as_array::<T>()?   as_map()?
to_string()  (Display = Java toString())      == (Java IsSameObject)      Default = null
```
`as_array::<T>()` supports `u8 i8 i16 u16 i32 i64 f32 f64 bool String Object`.
All generated classes `Deref` to `Object`, are `Clone`, `Default`(null), `Send + Sync` (so they can live in `static OnceLock<TextView>`).

### Helpers
```rust
box_bool box_i8 box_i16 box_char box_i32 box_i64 box_f32 box_f64   // -> Object (java.lang.Integer ...)
java_string("..") -> Object
new_list(&[obj, ...]) -> Object      new_map(&[(k, v), ...]) -> Object
```

### Java arrays you create yourself
```rust
new_array(&[1i32, 2, 3])?              // int[]   (u8/i8->byte[], u16->char[], i16, i64, f32, f64, bool; NOT usize)
new_array_2d(&[vec![1.5f32, 2.5], vec![3.5]])?   // float[][]
new_string_array(&["a", "b"])?         // String[]
new_object_array("android.view.Surface", &items)?   // Surface[]
new_object_array("[I", &[row1, row2])?              // int[][]   ("[[I" -> int[][][], "[Ljava/lang/String;" -> String[][])
new_object_array_of(&surfaces)?        // element class taken from the generated type
```
Wrong element type gives a clean `Err` (ArrayStoreException), not a crash.

---

## 6. Callbacks (Java -> Rust)

Anything the Java API wants as a listener / callback / abstract adapter class takes a Rust closure.

```rust
btn.setOnClickListener(move |_: Args| { ... }).unwrap();            // type annotation `: Args` is required
box.setOnCheckedChangeListener(|a: Args| { let checked = a.bool(1); });
```
Closure bounds: `Fn(Args) -> R + Send + Sync + 'static` where `R: IntoRet`. It is `Fn`, not `FnMut`: keep mutable state in `Mutex` / `Atomic*` / `static`.

`Args` (Copy): `len() is_empty() is_null(i) obj(i)->Object (cloned, safe to keep) i32(i) i64(i) f32(i) f64(i) bool(i) str(i) cast::<T>(i)? get(i)->CbArg`.
Argument index = Java parameter index (for `OnCheckedChangeListener.onCheckedChanged(button, isChecked)`: `a.obj(0)`, `a.bool(1)`).

Return value (`IntoRet`): `()`, `bool`, `i8..i64`, `u8..u64`, `f32`, `f64`, `&str`, `String`, `Object`, `JChar(u16)`, `Vec<i32>` (int[]), `Option<T>`, `Result<T, E: Display>` (`Err` becomes a Java `RuntimeException`). Example: `|_: Args| true` for `onLongClick`.

**Multi-method listeners and abstract classes** (SensorEventListener, TextWatcher, SeekBar listener, WebViewClient, CameraDevice.StateCallback ...):
```rust
let h = Handlers::new()
    .on("onSensorChanged", |a| { let ev = a.obj(0); ... })
    .on("onAccuracyChanged", |_| {});
sm.registerListener(h, &sensor, 2)?;
```
The name is the Java method name. Methods with no handler do nothing (primitive returns default to 0/false).

Pass an already existing Java listener object: `Callback::existing(&obj)`. `cb(|a| ...)` builds a `Callback` explicitly (helps type inference); `cb0(|| ...)` for no-arg closures.

**Panics**: a panic inside a callback is caught and turned into a Java `RuntimeException`. Recommended pattern: wrap bodies in a `guard(|| { ... })` that catches the panic and shows a toast. Never use `panic = "abort"`.

**Threads**: callbacks run on the thread Java calls them from (usually the UI thread). Any Rust thread may call the API (it attaches to the JVM automatically), but **UI work must run on the UI thread**:
```rust
std::thread::spawn(|| {
    let _ = run_on_ui_thread(move || { label.setText("done").unwrap(); });   // FnOnce + Send + 'static
});
```

---

## 7. App-level API (`use stratum::*;`)

```rust
stratum_main!(|| { ... });                     // entry; register hooks inside
on_create(|act: Activity| {...});  on_resume(|| {...});  on_pause(|| {...});  on_stop(|| {...});  on_destroy(|| {...});
on_back_pressed(|| -> bool {...});             // return true to consume the back button

get_activity() -> Activity                     // is_null() before onCreate
set_content_view(&view)                        // UI thread;  set_content_view_on(&act, &view)
run_on_ui_thread(FnOnce) -> JResult<()>

register_callback("key#method", closure)       // low level
register_function("name", closure)             // Java: StratumRuntimeLookup.callStratum("name", args...)
unregister_prefix("key#") -> usize
on_broadcast(|ctx: Object, intent: Object| {...})            // StratumReceiver (BOOT_COMPLETED etc.)
on_service_command(|intent: Object, flags: i32, start_id: i32| -> i32 {...})   // StratumService

allocate_direct_buffer(n)? direct_buffer_address(&b) direct_buffer_capacity(&b) bytebuffer_data(&b)?
surface_to_native_window(&s) release_native_window(w)
set_log_enabled(bool)                          // only if the engine was built with -DSTRATUM_LOG_ENABLED=1
```

### Custom 2D drawing view
```rust
struct MyView { /* shared state: Arc<...> */ }
impl CanvasView for MyView {
    fn on_draw(&self, canvas: Object) { /* reflect::call_method::<()>(&canvas, "drawColor", rargs![0xFF000000u32 as i32]) */ }
    fn on_touch_event(&self, event: Object) -> bool { true }
    // optional: on_measure(&self, wspec, hspec) -> Vec<i32> ([w,h] or empty), on_layout(...)
}
let handle = create_canvas_view(MyView { .. })?;     // CanvasViewHandle, derefs to Object
parent.addView(&handle)?;                            // KEEP `handle` alive: dropping it unregisters the callbacks
reflect::call_method::<()>(&handle, "invalidate", rargs![])?;   // request redraw
```
Touch events: `getAction()`=0 down, 2 move, 1 up; `getX()`, `getY()`. Drawing is done via reflection on `Canvas` / `Paint`.

---

## 8. `reflect`: call ANY Java class by name

For classes that were not emitted, your own app classes, or third-party SDKs.
```rust
use stratum::{rargs, reflect};
let n: i32      = reflect::call_static("java.lang.Integer", "parseInt", rargs!["42"])?;
let s: String   = reflect::call_method(&obj, "toString", rargs![])?;
let o: Object   = reflect::call_method(&act, "getSystemService", rargs!["sensor"])?;
let paint       = reflect::new_instance("android.graphics.Paint", rargs![])?;
reflect::call_method::<()>(&paint, "setStrokeWidth", rargs![3.0f32])?;
```
- Result type `R`: `()`, `bool`, `i8 i16 u16 i32 i64`, `f32 f64`, `String`, `Object`.
- `rargs![..]` accepts: `bool i8 i16 u16 i32 i64 f32 f64 &str &String &Object &AnyGeneratedClass ()`(=null). **No** `u8/u32/usize`. **Suffix literals** (`3.0f32`, `8i32`).
- Overload matching is by runtime argument types: exact first, then widening (i32 -> long/float/double; f32 -> double). A `f64` will NOT match a `float` parameter: pass `f32`.
- Only **public** methods are found. Static vs instance is decided by the target (`call_static` vs `call_method`).
- Java arrays as arguments: build them first (`new_array`) and pass `&arr`.

---

## 9. Recipes

**Dp to px**: read `getResources().getDisplayMetrics().f_get_density()` once, `px = (dp * density) as i32`.

**Persist data**: `let dir = reflect::call_method::<Object>(&get_activity(), "getFilesDir", rargs![])?.to_string();` then use `std::fs` on `{dir}/file.txt`.

**Toast**:
```rust
let t = reflect::call_static::<Object>("android.widget.Toast", "makeText", rargs![&act, "hi", 0i32])?;
reflect::call_method::<()>(&t, "show", rargs![])?;
```

**Sensor**:
```rust
let sm = reflect::call_method::<Object>(&act, "getSystemService", rargs!["sensor"])?;
let acc = reflect::call_method::<Object>(&sm, "getDefaultSensor", rargs![1i32])?;
let h = Handlers::new().on("onSensorChanged", |a| {
    let vals: Vec<f32> = a.obj(0).unchecked_cast::<SensorEvent>().f_get_values().unwrap_or_default();
}).on("onAccuracyChanged", |_| {});
sm.unchecked_cast::<SensorManager>().registerListener(h, &acc, 2)?;
```

**Global UI state**: store views in `static X: OnceLock<TextView>` / `Mutex<Vec<..>>`; closures must be `Send + Sync + 'static`, so no borrowing locals (clone views into `move` closures).

**Layout params**: `LinearLayout$LayoutParams` via `reflect::new_instance("android.widget.LinearLayout$LayoutParams", rargs![w, h, weight])`, then `parent.addView(&child, &params)`. (-1 = MATCH_PARENT, -2 = WRAP_CONTENT.)

**Colors**: ARGB as `u32` then `as i32` (`0xFF00BFA5u32 as i32`).

---

## 10. Rules / pitfalls checklist

1. `use stratum::*;` plus the specific `stratum::android::...` classes you need; alias `java::lang::String`.
2. Every call is `Result`; never ignore errors silently in logic that matters.
3. Closure passed to a generated function: annotate `|_: Args|` (or use `cb(...)`). Inside `stratum_main!` closures cannot capture variables.
4. Callback state: `Mutex` / `Atomic` / `OnceLock`; no `FnMut`.
5. UI mutation only on the UI thread (`run_on_ui_thread` from other threads).
6. Object returns are untyped: `cast::<T>()` before calling typed methods.
7. Keep `CanvasViewHandle` alive.
8. Don't hold a Mutex guard while calling into Java if the callback may re-enter and lock the same Mutex (deadlock). Drop guards first.
9. A class missing from `stratum::...` was not emitted: use `reflect`, or regenerate with a wider `--emit-prefix`.
10. `float` Java params need `f32`; `long` needs `i64`; `char` needs `u16`; `byte` needs `i8` (but `byte[]` is `&[u8]`).
11. Lib name must match in three places: `Cargo.toml` `[package] name` and `[lib] name`, `System.loadLibrary("..")`, and `stratum.rustLib` (gradle property / default in `stratum-rust.gradle.kts`).
12. Java side may call Rust with `StratumRuntimeLookup.callStratum("name", args)` after `register_function("name", ...)`.
13. Rust edition 2021, `crate-type = ["cdylib"]`, `[workspace]` table present in the app `Cargo.toml`, release profile `lto = "fat"`, never `panic = "abort"`.

## 11. What Stratum adds beyond plain JNI

- Whole Android SDK as typed Rust (thousands of classes), generated from `android.jar`, hidden/blocked/private APIs stripped.
- Precomputed slot table: no JNI signature parsing at runtime; wrappers are `#[inline]` direct C ABI calls.
- Automatic conversions: Rust strings/slices/Vec <-> Java String / primitive arrays / `ArrayList` / `HashMap`, boxed primitives, multi-dimensional arrays.
- Callbacks for interfaces (dynamic proxy) and **abstract classes** (generated Java adapters) from plain Rust closures; return values flow back to Java.
- Lifecycle hooks, UI-thread posting, custom Canvas view, broadcast receiver and service bridges, direct ByteBuffers and Surface -> `ANativeWindow`.
- Reflection for anything not generated.
- Safety: panic -> `RuntimeException`, Java exception -> `Err`, ABI version + metadata hash check at startup (aborts with a clear log if `libstratum.so` and the crate come from different generator runs).