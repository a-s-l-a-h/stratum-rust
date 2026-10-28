#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
One command for the whole Rust target:

  1. cd stratum-sys/  -> run run_pipeline_helper.py exactly as normal -> cd back
  2. python 25_rust_emit/main.py     (generate the Rust crate)
  3. python 25_rust_emit/package.py  (assemble the final output/stratum/ folder; you copy it into your project yourself)

    python build_all.py                                  # API 35, stages 00..05 + 20 (no C++ SDK stages)
    python build_all.py --api 35 --emit-prefix android.app.,android.widget.,android.view.
    python build_all.py --skip-pipeline                  # pipeline outputs already exist
    python build_all.py --full-pipeline                  # also run 21_cpp_emit / package / zip
    python build_all.py --pipeline-args "--force-download"
"""
import argparse, os, shlex, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SYS = ROOT / "stratum-sys"
RUST = ROOT / "25_rust_emit"
# Rust needs: setup..resolve + 05.5 adapters + 20 (metadata_table / meta_hash). Stage 21 (C++ headers) is not needed.
RUST_STAGES = "00,01,02,03,04,05,20"


def run(cmd, cwd=None):
    cmd = [str(c) for c in cmd]
    print("$ " + " ".join(cmd), flush=True)
    rc = subprocess.run(cmd, cwd=cwd).returncode
    if rc != 0:
        sys.exit(f"ERROR: command failed with exit code {rc}")


def ensure_submodule():
    if (SYS / "run_pipeline_helper.py").exists():
        return
    print("-> stratum-sys is empty, initialising the submodule ...")
    run(["git", "submodule", "update", "--init", "--recursive"], cwd=ROOT)
    if not (SYS / "run_pipeline_helper.py").exists():
        sys.exit("ERROR: stratum-sys/run_pipeline_helper.py not found. Clone with --recursive.")


def pipeline(a):
    cmd = [sys.executable, "run_pipeline_helper.py", "--job", a.api]
    if not a.full_pipeline:
        cmd += ["--only", RUST_STAGES]
    cmd += shlex.split(a.pipeline_args)
    old = os.getcwd()
    os.chdir(SYS)                       # cd into stratum-sys
    try:
        run(cmd)                        # runs as if you were inside stratum-sys
    finally:
        os.chdir(old)                   # cd out


def main():
    ap = argparse.ArgumentParser(description="Stratum Rust target: pipeline + rust emit + package")
    ap.add_argument("--api", default="35", help="job name or api_version in stratum-sys/pipeline.json")
    ap.add_argument("--emit-prefix", default="android.app.,android.widget.,android.view.",
                    help="comma-separated FQN prefixes ('' = everything; huge)")
    ap.add_argument("--out-root", default=str(RUST / "output"), help="receives crate/ and stratum/")
    ap.add_argument("--api-version", default=None, help="compileSdk of the runtime module (default: --api)")
    ap.add_argument("--min-sdk", default="24")
    ap.add_argument("--java-version", default="11")
    ap.add_argument("--skip-pipeline", action="store_true")
    ap.add_argument("--full-pipeline", action="store_true", help="run ALL pipeline stages (incl. C++ SDK + zip)")
    ap.add_argument("--pipeline-args", default="", help='extra args for run_pipeline_helper.py, e.g. "--force-download"')

    a = ap.parse_args()

    ensure_submodule()
    if not a.skip_pipeline:
        print("=" * 70 + "\n STEP 1: stratum-sys pipeline\n" + "=" * 70)
        pipeline(a)

    api_dir = SYS / "build" / f"API_{a.api}"
    engine = SYS / "20_engine_core"
    resolved = api_dir / "05_resolve" / "output_patched"
    generated = api_dir / "20_engine_core" / "generated"
    adapters = api_dir / "05_5_abstract" / "output" / "java" / "com" / "stratum" / "adapters"
    out_root = Path(a.out_root).resolve()
    crate_out, sdk_out = out_root / "crate", out_root / "stratum"

    print("=" * 70 + "\n STEP 2: Rust emit (25_rust_emit/main.py)\n" + "=" * 70)
    run([sys.executable, RUST / "main.py",
         "--input", resolved, "--output", crate_out, "--engine", engine,
         f"--emit-prefix={a.emit_prefix}"], cwd=ROOT)

    print("=" * 70 + "\n STEP 3: package (25_rust_emit/package.py)\n" + "=" * 70)
    cmd = [sys.executable, RUST / "package.py",
           "--input", crate_out, "--engine", engine, "--generated", generated,
           "--runtime", SYS / "runtime", "--adapters", adapters, "--output", sdk_out,
           "--api-version", a.api_version or a.api, "--min-sdk", a.min_sdk, "--java-version", a.java_version]

    run(cmd, cwd=ROOT)
    print(f"\nDONE. Final SDK folder: {sdk_out}\nCopy its folders into your project (see {sdk_out / 'README.md'}).")


if __name__ == "__main__":
    main()