# stratum-rust

Try the demo : https://github.com/a-s-l-a-h/stratumrustdemo


Write Android apps in **Rust** against the real Android SDK. `stratum-rust` is the Rust target built on [**stratum-sys**](https://github.com/a-s-l-a-h/stratum-sys) (the base Stratum project, included here as the `stratum-sys/` submodule). It turns the output of stratum-sys (class metadata + C engine) into

- a typed Rust crate (`stratum`) with a wrapper for every Android/Java class, and
- a ready-to-copy SDK folder (engine source, Rust crate, Android runtime module, Gradle glue).

```
Android SDK jar -> stratum-sys pipeline (stages 00..05, 20) -> 25_rust_emit -> output/stratum/ -> your Android Studio project
```

## Repository layout

```
stratum-rust/
├── build_all.py            one command: pipeline + generate + package
├── 25_rust_emit/
│   ├── main.py             Stage 25: generates the Rust crate (typed API + API_INDEX.txt)
│   ├── package.py          Stage 25b: assembles output/stratum/
│   └── templates/          hand-written crate parts (ffi.rs, base.rs, app.rs, reflect.rs, lib.rs)
│       └── app/            files copied into the user's project (CMakeLists, Cargo.toml, lib.rs, stratum-rust.gradle.kts)
├── stratum-sys/            git submodule: pipeline, C++ engine, Java runtime
├── STRATUM_RUST_API.md     API reference to hand to an LLM
└── note                    copy-paste commands
```

## Requirements

- Python 3.10+, Git (submodule), internet on the first run (downloads android.jar, JDK 17, hidden-API list)
- Android Studio with NDK + CMake 3.22.1
- Rust (https://rustup.rs) and the Android targets you ship:
  `rustup target add aarch64-linux-android x86_64-linux-android` (add `armv7-linux-androideabi i686-linux-android` for 32-bit)

## Generate the SDK

```bash
git clone --recursive <this repo>
cd stratum-rust
python build_all.py --api 35 --emit-prefix ""          # "" = every class (large, safest for demos)
```
Useful variants:
```bash
python build_all.py --api 35 --emit-prefix android.app.,android.widget.,android.view.,android.content.,android.os.,android.util.,android.hardware.,android.graphics.,java.lang.,java.util.
python build_all.py --skip-pipeline                    # pipeline outputs already exist
python build_all.py --pipeline-args "--force-download"
```
> `--emit-prefix` decides which classes become typed Rust wrappers. The default (`android.app.,android.widget.,android.view.`) is **too small** for the demo (it uses `android.os`, `android.hardware`, `android.util`, `android.content.res`, `java.lang`). Classes you leave out are still reachable through `stratum::reflect`.

Manual (same as `note`):
```bash
python 25_rust_emit/main.py --input "stratum-sys/build/API_35/05_resolve/output_patched" --output "25_rust_emit/output/crate" --engine "stratum-sys/20_engine_core"
python 25_rust_emit/package.py --input "25_rust_emit/output/crate" --engine "stratum-sys/20_engine_core" \
  --generated "stratum-sys/build/API_35/20_engine_core/generated" --runtime "stratum-sys/runtime" \
  --adapters "stratum-sys/build/API_35/05_5_abstract/output/java/com/stratum/adapters" \
  --output "25_rust_emit/output/stratum" --api-version 35 --min-sdk 24 --java-version 11
```
Stage 25 also checks `templates/ffi.rs` against `stratum_c_abi.h` and fails loudly if the C ABI drifted.

## Output: `25_rust_emit/output/stratum/`

```
stratum/
├── README.md
├── native/            -> app/src/main/native/    CMakeLists.txt + engine/ (builds libstratum.so)
├── rust/              -> app/src/main/rust/      Cargo.toml, stratum-rust.gradle.kts, src/lib.rs, stratum/ (generated crate)
└── stratum-runtime/   -> <project root>/stratum-runtime   (Android library module)
```

## Use it in a project

1. Copy the three folders as shown above.
2. `settings.gradle.kts`: `include(":app")` and `include(":stratum-runtime")`.
3. Root `build.gradle.kts`: add `com.android.library` plugin `apply false`.
4. `app/build.gradle.kts`:
   ```kotlin
   android {
       defaultConfig {
           minSdk = 24
           ndk { abiFilters += listOf("arm64-v8a", "x86_64") }     // == stratum.abis
           externalNativeBuild { cmake { arguments += listOf("-DANDROID_STL=c++_static") } }
       }
       externalNativeBuild { cmake { path = file("src/main/native/CMakeLists.txt"); version = "3.22.1" } }
   }
   dependencies { implementation(project(":stratum-runtime")) }
   apply(from = "src/main/rust/stratum-rust.gradle.kts")       // after the android { } block
   ```
5. `MainActivity` extends `com.stratum.runtime.StratumActivity` and calls `System.loadLibrary("myapp")` in `onImportMain()`. Use an AppCompat theme.
6. Write your app in `app/src/main/rust/src/lib.rs`.

`gradle.properties` (all optional): `stratum.rustLib=myapp`, `stratum.abis=arm64-v8a,x86_64`, `stratum.minApi=24`.
The library name must be identical in `Cargo.toml` (`[package] name` **and** `[lib] name`), `System.loadLibrary`, and `stratum.rustLib`.

How the build works: Gradle runs CMake -> `libstratum.so`; `stratum-rust.gradle.kts` then runs `cargo build --package <lib> --lib --target <triple>` linking against it and copies `lib<name>.so` into `app/src/main/jniLibs/<abi>/`. Cargo output appears in the `cargoBuildDebug` / `cargoBuildRelease` Gradle tasks.

IDE support on your PC: `cd app/src/main/rust && cargo check` (nothing is linked off-Android).

## Writing code

Read [`STRATUM_RUST_API.md`](STRATUM_RUST_API.md) (naming rules, type mapping, callbacks, reflection, pitfalls). Give that file to an LLM to have it write app code. Every generated name is listed in `rust/stratum/API_INDEX.txt`.

## Example / try it without setup

[**stratumrustdemo**](https://github.com/a-s-l-a-h/stratumrustdemo) is a 10-screen app written entirely in Rust (tasks, calculator, sketchpad, level/motion sensors, stopwatch, notes, bridge benchmark, Java arrays, settings).

To test without running the generator, download `stratumrustdemo-full.zip` from its [Releases page](https://github.com/a-s-l-a-h/stratumrustdemo/releases). It already contains the generated engine and Rust crate. Unzip, open in Android Studio, run.

## Status

Pre-1.0. Built on stratum-sys, which enables only API level 35 by default.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `No libstratum.so found ... Did CMake run?` | Build from Android Studio / Gradle so `externalNativeBuild` runs first; check `abiFilters` |
| `cargo build failed ... rustup target add ...` | Install the Rust target named in the message |
| `package ID specification ... did not match` | `[package] name` in `Cargo.toml` != `stratum.rustLib` |
| `lib<name>.so was not found` | `[lib] name` differs from `stratum.rustLib` |
| `UnsatisfiedLinkError ... libmyapp.so` | `System.loadLibrary` name differs from the Rust lib name |
| `libstratum.so (...) and the Rust SDK (...) were generated from different inputs` | Re-copy all three folders from one `build_all.py` run |
| `unresolved import stratum::android::...` | Class not emitted: widen `--emit-prefix` or use `reflect` |
| Callback never fires / type inference error | Annotate the closure `|_: Args|` or wrap with `cb(...)` |
| 16 KB page-size warnings | Already handled: the glue passes `-Wl,-z,max-page-size=16384` |

## License

MIT. Third-party notices for the pipeline are in `stratum-sys/THIRD-PARTY-LICENSES.md`.




# stratum-rust

Write Android apps in **Rust** against the real Android SDK. `stratum-rust` is the Rust target of the Stratum pipeline: it turns the output of [`stratum-sys`](stratum-sys/) (class metadata + C engine) into

- a typed Rust crate (`stratum`) with a wrapper for every Android/Java class, and
- a ready-to-copy SDK folder (engine source, Rust crate, Android runtime module, Gradle glue).

```
Android SDK jar -> stratum-sys pipeline (stages 00..05, 20) -> 25_rust_emit -> output/stratum/ -> your Android Studio project
```

## Repository layout

```
stratum-rust/
├── build_all.py            one command: pipeline + generate + package
├── 25_rust_emit/
│   ├── main.py             Stage 25: generates the Rust crate (typed API + API_INDEX.txt)
│   ├── package.py          Stage 25b: assembles output/stratum/
│   └── templates/          hand-written crate parts (ffi.rs, base.rs, app.rs, reflect.rs, lib.rs)
│       └── app/            files copied into the user's project (CMakeLists, Cargo.toml, lib.rs, stratum-rust.gradle.kts)
├── stratum-sys/            git submodule: pipeline, C++ engine, Java runtime
├── STRATUM_RUST_API.md     API reference to hand to an LLM
└── note                    copy-paste commands
```

## Requirements

- Python 3.10+, Git (submodule), internet on the first run (downloads android.jar, JDK 17, hidden-API list)
- Android Studio with NDK + CMake 3.22.1
- Rust (https://rustup.rs) and the Android targets you ship:
  `rustup target add aarch64-linux-android x86_64-linux-android` (add `armv7-linux-androideabi i686-linux-android` for 32-bit)

## Generate the SDK

```bash
git clone --recursive <this repo>
cd stratum-rust
python build_all.py --api 35 --emit-prefix ""          # "" = every class (large, safest for demos)
```
Useful variants:
```bash
python build_all.py --api 35 --emit-prefix android.app.,android.widget.,android.view.,android.content.,android.os.,android.util.,android.hardware.,android.graphics.,java.lang.,java.util.
python build_all.py --skip-pipeline                    # pipeline outputs already exist
python build_all.py --pipeline-args "--force-download"
```
> `--emit-prefix` decides which classes become typed Rust wrappers. The default (`android.app.,android.widget.,android.view.`) is **too small** for the demo (it uses `android.os`, `android.hardware`, `android.util`, `android.content.res`, `java.lang`). Classes you leave out are still reachable through `stratum::reflect`.

Manual (same as `note`):
```bash
python 25_rust_emit/main.py --input "stratum-sys/build/API_35/05_resolve/output_patched" --output "25_rust_emit/output/crate" --engine "stratum-sys/20_engine_core"
python 25_rust_emit/package.py --input "25_rust_emit/output/crate" --engine "stratum-sys/20_engine_core" \
  --generated "stratum-sys/build/API_35/20_engine_core/generated" --runtime "stratum-sys/runtime" \
  --adapters "stratum-sys/build/API_35/05_5_abstract/output/java/com/stratum/adapters" \
  --output "25_rust_emit/output/stratum" --api-version 35 --min-sdk 24 --java-version 11
```
Stage 25 also checks `templates/ffi.rs` against `stratum_c_abi.h` and fails loudly if the C ABI drifted.

## Output: `25_rust_emit/output/stratum/`

```
stratum/
├── README.md
├── native/            -> app/src/main/native/    CMakeLists.txt + engine/ (builds libstratum.so)
├── rust/              -> app/src/main/rust/      Cargo.toml, stratum-rust.gradle.kts, src/lib.rs, stratum/ (generated crate)
└── stratum-runtime/   -> <project root>/stratum-runtime   (Android library module)
```

## Use it in a project

1. Copy the three folders as shown above.
2. `settings.gradle.kts`: `include(":app")` and `include(":stratum-runtime")`.
3. Root `build.gradle.kts`: add `com.android.library` plugin `apply false`.
4. `app/build.gradle.kts`:
   ```kotlin
   android {
       defaultConfig {
           minSdk = 24
           ndk { abiFilters += listOf("arm64-v8a", "x86_64") }     // == stratum.abis
           externalNativeBuild { cmake { arguments += listOf("-DANDROID_STL=c++_static") } }
       }
       externalNativeBuild { cmake { path = file("src/main/native/CMakeLists.txt"); version = "3.22.1" } }
   }
   dependencies { implementation(project(":stratum-runtime")) }
   apply(from = "src/main/rust/stratum-rust.gradle.kts")       // after the android { } block
   ```
5. `MainActivity` extends `com.stratum.runtime.StratumActivity` and calls `System.loadLibrary("myapp")` in `onImportMain()`. Use an AppCompat theme.
6. Write your app in `app/src/main/rust/src/lib.rs`.

`gradle.properties` (all optional): `stratum.rustLib=myapp`, `stratum.abis=arm64-v8a,x86_64`, `stratum.minApi=24`.
The library name must be identical in `Cargo.toml` (`[package] name` **and** `[lib] name`), `System.loadLibrary`, and `stratum.rustLib`.

How the build works: Gradle runs CMake -> `libstratum.so`; `stratum-rust.gradle.kts` then runs `cargo build --package <lib> --lib --target <triple>` linking against it and copies `lib<name>.so` into `app/src/main/jniLibs/<abi>/`. Cargo output appears in the `cargoBuildDebug` / `cargoBuildRelease` Gradle tasks.

IDE support on your PC: `cd app/src/main/rust && cargo check` (nothing is linked off-Android).

## Writing code

Read [`STRATUM_RUST_API.md`](STRATUM_RUST_API.md) (naming rules, type mapping, callbacks, reflection, pitfalls). Give that file to an LLM to have it write app code. Every generated name is listed in `rust/stratum/API_INDEX.txt`.

## Example

See the **stratumrustdemo** project: a 10-screen app (tasks, calculator, sketchpad, level/motion sensors, stopwatch, notes, bridge benchmark, Java arrays, settings).

## Troubleshooting

| Symptom | Fix |
|---|---|
| `No libstratum.so found ... Did CMake run?` | Build from Android Studio / Gradle so `externalNativeBuild` runs first; check `abiFilters` |
| `cargo build failed ... rustup target add ...` | Install the Rust target named in the message |
| `package ID specification ... did not match` | `[package] name` in `Cargo.toml` != `stratum.rustLib` |
| `lib<name>.so was not found` | `[lib] name` differs from `stratum.rustLib` |
| `UnsatisfiedLinkError ... libmyapp.so` | `System.loadLibrary` name differs from the Rust lib name |
| `libstratum.so (...) and the Rust SDK (...) were generated from different inputs` | Re-copy all three folders from one `build_all.py` run |
| `unresolved import stratum::android::...` | Class not emitted: widen `--emit-prefix` or use `reflect` |
| Callback never fires / type inference error | Annotate the closure `|_: Args|` or wrap with `cb(...)` |
| 16 KB page-size warnings | Already handled: the glue passes `-Wl,-z,max-page-size=16384` |

## License

See `stratum-sys/LICENSE` and `stratum-sys/THIRD-PARTY-LICENSES.md`.