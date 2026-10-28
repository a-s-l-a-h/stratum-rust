# Third-Party Licenses & Notices

This project depends on the open-source software and SDKs listed below. This document is a notice of those dependencies and links to their license terms. It is not legal advice; check each license before redistributing.

## Table of Contents

1. [Build-time tools (not shipped)](#1-build-time-tools-not-shipped)
   - [Android SDK and NDK](#android-sdk-and-ndk)
   - [Java Development Kit (Eclipse Temurin / OpenJDK)](#java-development-kit-eclipse-temurin--openjdk)
   - [Rust toolchain](#rust-toolchain)
2. [Data used by the generator](#2-data-used-by-the-generator)
   - [android.jar and the hidden-API lists](#androidjar-and-the-hidden-api-lists)
3. [Code that ends up in an app built with Stratum](#3-code-that-ends-up-in-an-app-built-with-stratum)
   - [Android NDK runtime (libc++)](#android-ndk-runtime-libc)
   - [Rust standard library](#rust-standard-library)
   - [AndroidX and Material Components (demo app)](#androidx-and-material-components-demo-app)

---

## 1. Build-time tools (not shipped)

These are needed to run the generator or build the demo. They are not distributed in this repository or in the generated SDK.

### Android SDK and NDK
- **License:** Android Software Development Kit License Agreement
- **Provider:** Google LLC
- **Usage:** compiling the Java/Kotlin and native code, `android.jar`, CMake and the NDK clang toolchain.
- **Terms:** https://developer.android.com/studio/terms

### Java Development Kit (Eclipse Temurin / OpenJDK)
- **Distribution used:** Eclipse Temurin (Adoptium) 17, downloaded by the pipeline. Any OpenJDK build works.
- **License:** GNU General Public License v2 with the Classpath Exception (OpenJDK)
- **Usage:** `javap` and other build tools, run during generation only. No JDK code is copied into the output.
- **Source:** https://adoptium.net/ and https://openjdk.org/
- **License details:** https://openjdk.org/legal/gplv2+ce.html

### Rust toolchain
- **License:** MIT or Apache-2.0 (the Rust compiler, Cargo and the standard library)
- **Usage:** compiling the generated Rust crate and your app.
- **Source:** https://www.rust-lang.org/policies/licenses

## 2. Data used by the generator

### android.jar and the hidden-API lists
- **Source:** the Android SDK platform package (`android.jar`) and the hidden-API lists downloaded at generation time.
- **Usage:** read to produce the class metadata table, headers and Rust wrappers. `android.jar` itself is downloaded by the pipeline and is not redistributed by this project.
- **Terms:** Android SDK License Agreement (above). Android platform source is Apache License 2.0 unless noted: https://source.android.com/docs/setup/about/licenses
- Generated output contains names and signatures of public Android APIs. Review the SDK terms if you redistribute generated files.

## 3. Code that ends up in an app built with Stratum

### Android NDK runtime (libc++)
- **License:** Apache License 2.0 with LLVM Exceptions
- **Usage:** statically linked into `libstratum.so` (`-DANDROID_STL=c++_static`).
- **Source:** https://libcxx.llvm.org/ and https://developer.android.com/ndk

### Rust standard library
- **License:** MIT or Apache-2.0
- **Usage:** linked into your Rust `.so`. Any extra crates you add in `Cargo.toml` bring their own licenses; list them in your app's notices (`cargo install cargo-license && cargo license` helps).

### AndroidX and Material Components (demo app)
- **License:** Apache License 2.0
- **Usage:** `appcompat` and `material` dependencies of the demo project only.
- **Source:** https://developer.android.com/jetpack/androidx and https://github.com/material-components/material-components-android

---

Stratum itself is released under the MIT License (see `LICENSE`).