#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Stage 25b: assemble the self-contained Rust SDK folder (SOURCE ONLY, same idea as Stage 21b).

    stratum/
    ├── README.md
    ├── native/            -> app/src/main/native/   (CMakeLists.txt, engine/ = Stage 20 engine source)
    ├── rust/              -> app/src/main/rust/     (Cargo.toml, stratum-rust.gradle.kts, src/lib.rs, stratum/ = generated crate)
    └── stratum-runtime/   -> <project root>/stratum-runtime  (Android library module)

    python 25_rust_emit/package.py --input "25_rust_emit/output/crate" --engine "stratum-sys/20_engine_core" --generated "stratum-sys/build/API_35/20_engine_core/generated" --runtime "stratum-sys/runtime" --adapters "stratum-sys/build/API_35/05_5_abstract/output/java/com/stratum/adapters" --output "25_rust_emit/output/stratum" --api-version 35 --min-sdk 24 --java-version 11
"""
import argparse, re, shutil, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent


def die(msg):
    sys.exit("ERROR: " + msg)


def need(p: Path, what: str):
    if not p.exists():
        die(f"{what} not found: {p}")


def copy_files(src: Path, dst: Path, patterns):
    dst.mkdir(parents=True, exist_ok=True)
    n = 0
    for pat in patterns:
        for f in sorted(src.glob(pat)):
            if f.is_file():
                shutil.copy2(f, dst / f.name); n += 1
    return n


def read_hash(path: Path) -> str:
    t = path.read_text(encoding="utf-8")
    m = (re.search(r'STRATUM_META_HASH\s*:\s*&str\s*=\s*"([0-9a-f]+)"', t)      # meta_hash.rs
         or re.search(r'#define\s+STRATUM_META_HASH\s+"([0-9a-f]+)"', t))       # meta_hash.h
    if not m:
        die(f"no STRATUM_META_HASH in {path}")
    return m.group(1)


def render(src: Path, dst: Path, values: dict):
    text = src.read_text(encoding="utf-8")
    for k, v in values.items():
        text = text.replace(f"@@{k}@@", str(v))
    left = re.findall(r"@@[A-Z_]+@@", text)
    if left:
        die(f"unresolved placeholders {sorted(set(left))} in {src}")
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(text, encoding="utf-8")


def java_const(v: str) -> str:
    v = str(v).strip()
    return "VERSION_1_8" if v in ("8", "1.8") else f"VERSION_{v}"





README = r"""# Stratum Rust SDK (source)

Typed Rust bindings for the Android SDK.
  * Gradle -> CMake builds the engine:      libstratum.so   (Stage 20 C++ engine, source in native/engine)
  * Gradle -> cargo builds your Rust crate: libmyapp.so     (cdylib, links -lstratum)
No cargo-ndk, no prebuilt .so, no stub.

## Requirements (once)
- Android Studio with NDK + CMake 3.22.1 (SDK Manager)
- Rust (https://rustup.rs), then:  rustup target add aarch64-linux-android
  (+ armv7-linux-androideabi, x86_64-linux-android, i686-linux-android for other ABIs)
- cargo is searched in $CARGO_HOME/bin, ~/.cargo/bin (Windows: %USERPROFILE%\.cargo\bin), then PATH

## Project layout
    RustDemo/
    ├── settings.gradle.kts
    ├── build.gradle.kts
    ├── gradle.properties
    ├── stratum-runtime/                 <- from this SDK (Java runtime + adapters, Android library module)
    └── app/
        ├── build.gradle.kts
        └── src/main/
            ├── AndroidManifest.xml
            ├── java/com/example/rustdemo/MainActivity.java
            ├── native/                  <- from this SDK: CMakeLists.txt + engine/
            └── rust/                    <- from this SDK: Cargo.toml, stratum-rust.gradle.kts, src/lib.rs, stratum/

Shortcut: python build_all.py --api 35 --install path/to/RustDemo

## settings.gradle.kts
    include(":app")
    include(":stratum-runtime")

## root build.gradle.kts
    plugins {
        id("com.android.application") version "<your AGP>" apply false
        id("com.android.library") version "<your AGP>" apply false     // for :stratum-runtime
    }

## app/build.gradle.kts
    android {
        defaultConfig {
            minSdk = 24
            ndk { abiFilters += listOf("arm64-v8a") }                  // keep == stratum.abis (below)
            externalNativeBuild {
                cmake {
                    arguments += listOf("-DANDROID_STL=c++_static")
                    // arguments += "-DSTRATUM_LOG_ENABLED=1"          // engine tracing
                }
            }
        }
        externalNativeBuild { cmake { path = file("src/main/native/CMakeLists.txt"); version = "3.22.1" } }
    }
    dependencies { implementation(project(":stratum-runtime")) }
    apply(from = "src/main/rust/stratum-rust.gradle.kts")              // AFTER the android { } block

## gradle.properties (optional)
    stratum.abis=arm64-v8a            # comma list, must equal abiFilters, rust targets must be installed
    stratum.rustLib=myapp             # [lib] name in rust/Cargo.toml == System.loadLibrary("myapp")
    stratum.minApi=24

## MainActivity.java
    package com.example.rustdemo;
    public class MainActivity extends com.stratum.runtime.StratumActivity {
        // StratumActivity already loaded libstratum.so; this loads libmyapp.so (your Rust code)
        @Override protected boolean onImportMain() { System.loadLibrary("myapp"); return true; }
    }
Use an AppCompat theme in the manifest (StratumActivity extends AppCompatActivity). The library module's
manifest (StratumService / StratumReceiver) is merged automatically.

## Rust (app/src/main/rust/src/lib.rs)
    use stratum::*;
    use stratum::android::widget::TextView;
    stratum_main!(|| {
        on_create(|act: Activity| {
            let tv = TextView::new(&act).unwrap();
            tv.setText("hello from Rust").unwrap();
            set_content_view(&tv);
        });
    });

- every call returns Result<T, JavaException>; object returns are `Object` -> `.cast::<T>()?`
- generated classes deref to `Object`, so `&text_view` works where `&Object` is expected
- overloads: the most natural one keeps the plain name, the rest get a type suffix (setText / setText_int);
  all names are in rust/stratum/API_INDEX.txt
- callbacks: view.setOnClickListener(cb(|a| { ... }))   (a.obj(0), a.i32(1), a.str(2), ...)
- multi-method listeners / abstract classes: Handlers::new().on("onPageFinished", |a| ...).on(...)
- Java arrays: new_array(&[1,2,3])?, new_array_2d(&rows)?, new_string_array(&["a","b"])?,
  new_object_array("android.view.Surface", &items)?, new_object_array_of(&items)?
- any other Java class by name: stratum::reflect (call_static / call_method / new_instance, rargs![...])
- keep edition 2021 (`#[no_mangle]` in stratum_main!) and panic = unwind (callbacks use catch_unwind)

## Notes
- IDE support on your PC: cd app/src/main/rust && cargo check   (no Android target: nothing is linked)
- Rust errors appear in the Gradle output of the `cargoBuild<Variant>` task.
- The APK contains lib/<abi>/libstratum.so and lib/<abi>/libmyapp.so.
"""


def main():
    ap = argparse.ArgumentParser(description="Stratum Stage 25b - package Rust SDK (source only)")
    _clean = lambda p: Path(str(p).strip(' "\'')).resolve()
    ap.add_argument("--engine", default=str(REPO / "stratum-sys" / "20_engine_core"),
                    help="20_engine_core folder (CMakeLists.txt, src/, include/)")
    ap.add_argument("--generated", default=None,
                    help="metadata_table.cpp/.h + meta_hash.h folder (default: <engine>/generated)")
    ap.add_argument("--rust-out", "--input", dest="rust_out", default=None,
                    help="Stage 25 main.py output (the crate). Default: <this folder>/output/crate")
    ap.add_argument("--runtime", default=str(REPO / "stratum-sys" / "runtime"),
                    help="runtime/ folder containing stratum-runtime/")
    ap.add_argument("--adapters", default=None,
                    help="Stage 05.5 adapters folder (com/stratum/adapters). Optional; skipped if missing")
    ap.add_argument("--output", default=None, help="Packaged SDK folder. Default: <this folder>/output/stratum")
    ap.add_argument("--api-version", required=True, help="Android API the SDK was generated from (-> compileSdk)")
    ap.add_argument("--min-sdk", default="24", help="minSdk of the runtime module")
    ap.add_argument("--java-version", default="11", help="Java level of the runtime module (1.8, 11, 17)")

    a = ap.parse_args()

    engine = _clean(a.engine)
    generated = _clean(a.generated) if a.generated else engine / "generated"
    crate = _clean(a.rust_out) if a.rust_out else (HERE / "output" / "crate").resolve()
    runtime = _clean(a.runtime)
    rt_mod = runtime / "stratum-runtime"
    adapters = _clean(a.adapters) if a.adapters else None
    out = _clean(a.output) if a.output else (HERE / "output" / "stratum").resolve()
    tpl = HERE / "templates" / "app"

    need(crate / "Cargo.toml", "Rust crate (run 25_rust_emit/main.py)")
    need(crate / "src" / "meta_hash.rs", "meta_hash.rs")
    need(engine / "CMakeLists.txt", "engine CMakeLists.txt")
    need(engine / "src", "engine src/")
    need(generated / "metadata_table.cpp", "generated/metadata_table.cpp (pipeline stage 20)")
    need(generated / "meta_hash.h", "generated/meta_hash.h")
    need(engine / "include" / "stratum_c_abi.h", "stratum_c_abi.h")
    need(rt_mod / "src" / "main" / "java", "runtime/stratum-runtime/src/main/java")
    need(rt_mod / "src" / "main" / "AndroidManifest.xml", "runtime AndroidManifest.xml")
    need(rt_mod / "consumer-rules.pro", "runtime consumer-rules.pro")
    need(rt_mod / "build.gradle.template", "runtime build.gradle.template")
    for t in ("CMakeLists.txt", "Cargo.toml", "lib.rs", "stratum-rust.gradle.kts"):
        need(tpl / t, f"templates/app/{t}")

    h_engine = read_hash(generated / "meta_hash.h")
    h_crate = read_hash(crate / "src" / "meta_hash.rs")
    if h_engine != h_crate:
        die(f"meta hash mismatch: engine={h_engine} crate={h_crate}. Re-run stage 20 and main.py from the SAME input.")

    # never delete a folder we are reading from
    for _src in (engine, crate, runtime, generated) + ((adapters,) if adapters else ()):
        if out == _src or out in _src.parents or _src in out.parents:
            die(f"--output ({out}) must not equal, contain or live inside a source folder: {_src}")
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)

    # ── native/ : CMake glue + engine source (-> libstratum.so) ─────────────
    nat = out / "native"
    eng = nat / "engine"
    eng.mkdir(parents=True)
    shutil.copy2(tpl / "CMakeLists.txt", nat / "CMakeLists.txt")
    shutil.copy2(engine / "CMakeLists.txt", eng / "CMakeLists.txt")
    n_src = copy_files(engine / "src", eng / "src", ["*.cpp", "*.h", "*.hpp"])
    n_gen = copy_files(generated, eng / "generated", ["*.cpp", "*.h"])
    (eng / "include").mkdir()
    shutil.copy2(engine / "include" / "stratum_c_abi.h", eng / "include" / "stratum_c_abi.h")

    # ── rust/ : app crate (cdylib) + generated crate + gradle glue ──────────
    rs = out / "rust"
    (rs / "src").mkdir(parents=True)
    shutil.copy2(tpl / "Cargo.toml", rs / "Cargo.toml")
    shutil.copy2(tpl / "stratum-rust.gradle.kts", rs / "stratum-rust.gradle.kts")
    shutil.copy2(tpl / "lib.rs", rs / "src" / "lib.rs")
    shutil.copytree(crate, rs / "stratum")

    # ── stratum-runtime/ : Android library module ───────────────────────────
    mod = out / "stratum-runtime"
    shutil.copytree(rt_mod, mod, ignore=shutil.ignore_patterns(
        "__pycache__", "build", "*.iml", "build.gradle.template", "build.gradle"))
    values = dict(COMPILE_SDK=a.api_version, MIN_SDK=a.min_sdk,
                  JAVA_VERSION=java_const(a.java_version), META_HASH=h_engine)
    render(rt_mod / "build.gradle.template", mod / "build.gradle", values)
    n_ad = 0
    if adapters and adapters.exists():
        n_ad = copy_files(adapters, mod / "src" / "main" / "java" / "com" / "stratum" / "adapters", ["*.java"])
    else:
        print(f"-> note: no adapters dir ({adapters}) (Stage 05.5 off?) - skipped")

    (out / "README.md").write_text(README, encoding="utf-8")

    n_rs = len(list((rs / "stratum" / "src").rglob("*.rs")))
    print("=" * 60)
    print(f" STRATUM RUST SDK PACKAGED  (meta hash {h_engine}, compileSdk {a.api_version})")
    print("=" * 60)
    print(f"-> rust files     : {n_rs}")
    print(f"-> engine sources : {n_src} files, generated: {n_gen} files")
    print(f"-> java adapters  : {n_ad}")
    print(f"-> output         : {out}")
    print("   native/ -> app/src/main/native/   rust/ -> app/src/main/rust/   stratum-runtime/ -> <project root>")




if __name__ == "__main__":
    main()