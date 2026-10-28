// Stratum Rust glue.  Apply from app/build.gradle.kts (after the android { } block):
//
//     apply(from = "src/main/rust/stratum-rust.gradle.kts")
//
// What it does:
//   1. AGP's CMake step builds libstratum.so (the engine).
//   2. For every ABI CMake built, run `cargo build` (cdylib -> lib<name>.so), linking against that libstratum.so.
//   3. Copy the result to app/src/main/jniLibs/<abi>/lib<name>.so so AGP always packages it into the APK.
//
// Properties (gradle.properties, all optional):
//   stratum.rustLib = myapp          # MUST equal [package] name AND [lib] name in rust/Cargo.toml
//                                    # and System.loadLibrary("myapp")
//   stratum.minApi  = 24
//   stratum.abis    = arm64-v8a,x86_64   # must equal ndk.abiFilters, and the rust targets must be installed
import org.gradle.internal.os.OperatingSystem

abstract class CargoBuildTask : DefaultTask() {
    @get:InputDirectory
    abstract val rustDir: DirectoryProperty

    @get:Internal
    abstract val ndkDir: DirectoryProperty

    @get:Internal
    abstract val cxxIntermediatesDir: DirectoryProperty

    @get:OutputDirectory
    abstract val outputJniDir: DirectoryProperty

    @get:Internal
    abstract val cargoTargetBaseDir: DirectoryProperty

    @get:Input
    abstract val rustLibName: Property<String>

    @get:Input
    abstract val minApi: Property<String>

    @get:Input
    abstract val abis: ListProperty<String>

    @get:Input
    abstract val release: Property<Boolean>

    @get:Input
    abstract val variantName: Property<String>

    @TaskAction
    fun run() {
        val ndk = ndkDir.get().asFile
        val cxxDir = cxxIntermediatesDir.get().asFile
        val outJni = outputJniDir.get().asFile
        val targetBase = cargoTargetBaseDir.get().asFile
        val rustDirectory = rustDir.get().asFile
        val libName = rustLibName.get()
        val minApiVal = minApi.get()
        val isRel = release.get()
        val variantCap = variantName.get()

        val isWindows = OperatingSystem.current().isWindows
        val hostTag = when {
            isWindows -> "windows-x86_64"
            OperatingSystem.current().isMacOsX -> "darwin-x86_64"
            else -> "linux-x86_64"
        }

        val cargoExe = run {
            val exe = if (isWindows) "cargo.exe" else "cargo"
            val home = System.getenv("CARGO_HOME")?.let { File(it) } ?: File(System.getProperty("user.home"), ".cargo")
            val c = File(home, "bin/$exe")
            if (c.exists()) c.absolutePath else "cargo"
        }

        // ABI -> (rust target triple, NDK clang prefix)
        val triples = mapOf(
            "arm64-v8a"   to Pair("aarch64-linux-android", "aarch64-linux-android"),
            "armeabi-v7a" to Pair("armv7-linux-androideabi", "armv7a-linux-androideabi"),
            "x86_64"      to Pair("x86_64-linux-android", "x86_64-linux-android"),
            "x86"         to Pair("i686-linux-android", "i686-linux-android")
        )

        // Find all libstratum.so files produced by CMake
        val candidates = if (cxxDir.exists()) {
            cxxDir.walkTopDown()
                .filter { it.isFile && it.name == "libstratum.so" }
                .toList()
        } else emptyList()

        if (candidates.isEmpty()) {
            error("No libstratum.so found under ${cxxDir.path}. Did CMake run?")
        }

        val activeAbis = abis.get().filter { abi ->
            candidates.any { it.parentFile.name == abi }
        }

        if (activeAbis.isEmpty()) {
            error("None of the requested ABIs ${abis.get()} were built by CMake. Found: ${candidates.map { it.parentFile.name }}")
        }

        for (abi in activeAbis) {
            val (triple, clangPrefix) = triples[abi] ?: error("Unsupported ABI: $abi")
            val clang = File(ndk, "toolchains/llvm/prebuilt/$hostTag/bin/$clangPrefix$minApiVal-clang" + if (isWindows) ".cmd" else "")

            val abiCandidates = candidates.filter { it.parentFile.name == abi }
            val mine = abiCandidates.filter { it.path.contains(variantCap, ignoreCase = true) }
            val engine = (if (mine.isNotEmpty()) mine else abiCandidates).maxByOrNull { it.lastModified() }
                ?: error("libstratum.so for $abi not found")

            val targetDir = File(targetBase, abi)
            val cmd = mutableListOf(
                cargoExe, "build",
                "--package", libName,
                "--lib",
                "--target", triple,
                "--manifest-path", File(rustDirectory, "Cargo.toml").absolutePath,
                "--target-dir", targetDir.absolutePath
            )
            if (isRel) cmd.add("--release")

            val pb = ProcessBuilder(cmd).directory(rustDirectory)
            pb.redirectInput(ProcessBuilder.Redirect.from(File(if (isWindows) "NUL" else "/dev/null")))
            pb.redirectErrorStream(true)

            val envKey = triple.uppercase().replace('-', '_')
            pb.environment()["CARGO_TARGET_${envKey}_LINKER"] = clang.absolutePath
            pb.environment()["CARGO_TARGET_DIR"] = targetDir.absolutePath
            pb.environment()["CARGO_ENCODED_RUSTFLAGS"] =
                listOf("-L", "native=${engine.parentFile.absolutePath}", "-C", "link-arg=-Wl,-z,max-page-size=16384").joinToString("\u001f")

            logger.lifecycle("───────────────────────────────────────────────────")
            logger.lifecycle("[Rust] Compiling '$libName' for target: $abi ($triple)")
            logger.lifecycle("[Rust] Linker: ${clang.name}")
            logger.lifecycle("───────────────────────────────────────────────────")

            val process = pb.start()
            process.inputStream.bufferedReader().useLines { lines ->
                lines.forEach { line -> logger.lifecycle("  [cargo] $line") }
            }

            val rc = process.waitFor()
            if (rc != 0) error("cargo build failed for $abi (exit $rc). Run: rustup target add $triple")

            val mode = if (isRel) "release" else "debug"

            val candidatesList = listOf(
                File(targetDir, "$triple/$mode/lib$libName.so"),
                File(targetDir, "$mode/lib$libName.so"),
                File(rustDirectory, "target/$triple/$mode/lib$libName.so"),
                File(rustDirectory, "target/$mode/lib$libName.so")
            )

            val so = candidatesList.firstOrNull { it.exists() }
                ?: targetDir.walkTopDown().filter { it.isFile && it.name == "lib$libName.so" }.maxByOrNull { it.lastModified() }
                ?: File(rustDirectory, "target").walkTopDown().filter { it.isFile && it.name == "lib$libName.so" }.maxByOrNull { it.lastModified() }
                ?: error("Expected library 'lib$libName.so' was not found in ${targetDir.path}")

            logger.lifecycle("[Rust] Found compiled library: ${so.path}")

            val dest = File(outJni, "$abi/lib$libName.so")
            dest.parentFile.mkdirs()
            so.copyTo(dest, overwrite = true)
            logger.lifecycle("[Rust] Successfully installed library to: ${dest.path}")
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Dynamic Variant Registration
// ─────────────────────────────────────────────────────────────────────────────

val rustDirFile = file("src/main/rust")
val rustLibNameProp = (findProperty("stratum.rustLib") as String?) ?: "myapp"          // == [package] name == [lib] name in Cargo.toml
val minApiProp = (findProperty("stratum.minApi") as String?) ?: "24"
val abisProp = ((findProperty("stratum.abis") as String?) ?: "arm64-v8a,x86_64").split(",").map { it.trim() }

// Output directly to src/main/jniLibs so AGP always packages it into the APK
val jniOutputDir = file("src/main/jniLibs")

afterEvaluate {
    // Resolve NDK directory
    val androidExt = extensions.findByName("android")
    val ndkDirFile: File? = run {
        try {
            val getM = androidExt?.javaClass?.getMethod("getNdkDirectory")
            getM?.invoke(androidExt) as? File
        } catch (_: Exception) { null }
    } ?: run {
        val localProps = File(rootDir, "local.properties")
        if (localProps.exists()) {
            val props = java.util.Properties().apply { load(localProps.inputStream()) }
            props.getProperty("ndk.dir")?.let { File(it) }?.takeIf { it.exists() }
                ?: props.getProperty("sdk.dir")?.let { File(it, "ndk") }?.listFiles()?.maxOrNull()
        } else null
    } ?: System.getenv("ANDROID_NDK_HOME")?.let { File(it) }
    ?: System.getenv("ANDROID_NDK_ROOT")?.let { File(it) }

    // Register Cargo build tasks for each variant
    listOf("Debug", "Release").forEach { cap ->
        val isRel = cap.equals("Release", ignoreCase = true)

        val cargoTask = tasks.register<CargoBuildTask>("cargoBuild$cap") {
            group = "stratum"

            // Run right after CMake compiles libstratum.so
            dependsOn("externalNativeBuild$cap")

            rustDir.set(rustDirFile)
            ndkDirFile?.let { ndkDir.set(it) }
            cxxIntermediatesDir.set(layout.buildDirectory.dir("intermediates/cxx"))
            outputJniDir.set(jniOutputDir)
            cargoTargetBaseDir.set(layout.buildDirectory.dir("rust"))
            rustLibName.set(rustLibNameProp)
            minApi.set(minApiProp)
            abis.set(abisProp)
            release.set(isRel)
            variantName.set(cap)
        }

        // Ensure JNI packaging and stripping wait for cargoBuild to complete
        tasks.matching {
            it.name.contains(cap, ignoreCase = false) && (
                    (it.name.startsWith("merge") && it.name.endsWith("JniLibFolders")) ||
                            (it.name.startsWith("strip") && it.name.endsWith("DebugSymbols")) ||
                            (it.name.startsWith("package") && (it.name.endsWith("Bundle") || it.name.endsWith("Apk")))
                    )
        }.configureEach {
            dependsOn(cargoTask)
        }
    }
}

tasks.named<Delete>("clean") {
    delete(file("src/main/rust/target"))
    delete(layout.buildDirectory.dir("rust"))
    delete(file("src/main/jniLibs"))
}