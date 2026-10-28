cmake_minimum_required(VERSION 3.22.1)
project(stratum_native CXX)

# Stage 20 engine -> libstratum.so (Gradle packages it into the APK).
# The Rust cdylib (libmyapp.so) is built by cargo, driven from Gradle (rust/stratum-rust.gradle.kts).
add_subdirectory(engine)