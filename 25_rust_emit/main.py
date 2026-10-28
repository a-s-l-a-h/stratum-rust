#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Stage 25: Rust SDK crate from stratum-sys/build/API_<n>/05_resolve/output_patched/ -- sits directly on the C ABI.
Mirrors Stage 21 (cpp_emit):
  * inherited methods are flattened into every class, called with the DECLARING class's class_id
  * object returns are untyped `Object` (use .cast::<T>())
  * every call returns Result<T, JavaException>
Rust has no overloading: per base name the "most natural" overload keeps the plain name (fewest params, then
String > object > array/collection > primitive), the others get a short suffix with only the differing parameter types: setText / setText_int / registerListener_Handler.
Every generated fn is #[inline] so wrappers collapse into a direct call of the C symbol.
See API_INDEX.txt for every generated name. Also verifies templates/ffi.rs against stratum_c_abi.h (drift check).
"""
import argparse, hashlib, json, re, shutil
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
SKIP = {"parse_summary.json", "resolve_summary.json", "manifest.json", "sanitize_summary.json"}
RS_KW = set("""as break const continue crate else enum extern false fn for if impl in let loop match mod move mut pub ref return
self Self static struct super trait true type unsafe use where while async await dyn abstract become box do final macro
override priv typeof unsized virtual yield try gen""".split())
RESERVED = set("""raw release is_null to_string hash_code is_instance_of cast unchecked_cast adopt create wrap null arg handle
as_bool as_i8 as_char as_i16 as_i32 as_i64 as_float as_double as_array as_map clone eq ne hash fmt deref default drop into
from borrow as_ref as_mut try_into try_from to_owned type_id new_""".split())


def meta_hash(d: Path) -> str:        # identical to 20_engine_core/emit.py and 21_cpp_emit/main.py
    h = hashlib.sha256()
    for jf in sorted(d.rglob("*.json")):
        if jf.name in SKIP: continue
        h.update(jf.relative_to(d).as_posix().encode()); h.update(jf.read_bytes())
    return h.hexdigest()[:16]


def check_abi(engine_h: Path):
    """ffi.rs is hand-written: fail loudly when stratum_c_abi.h and ffi.rs drift apart."""
    ffi = (HERE / "templates" / "ffi.rs").read_text(encoding="utf-8")
    if not engine_h.exists():
        print(f"-> WARNING: {engine_h} not found, ABI drift check skipped"); return
    h = engine_h.read_text(encoding="utf-8")
    c_fns = set(re.findall(r"STRATUM_API\s+[\w\s\*]+?\b(stratum_\w+)\s*\(", h))
    rs_fns = set(re.findall(r"pub fn (stratum_\w+)", ffi))
    missing, extra = sorted(c_fns - rs_fns), sorted(rs_fns - c_fns)
    if missing:
        raise SystemExit("ffi.rs is missing C ABI functions (engine changed?):\n  " + "\n  ".join(missing))
    if extra:
        print("-> WARNING: ffi.rs declares functions not in the header: " + ", ".join(extra))
    mv = re.search(r"#define\s+STRATUM_ABI_VERSION\s+(\d+)", h)
    rv = re.search(r"STRATUM_ABI_VERSION:\s*i32\s*=\s*(\d+)", ffi)
    if mv and rv and mv.group(1) != rv.group(1):
        raise SystemExit(f"ABI version mismatch: header={mv.group(1)} ffi.rs={rv.group(1)}")
    print(f"-> ABI check OK ({len(c_fns)} functions)")


def ident(s):
    s = re.sub(r"[^a-zA-Z0-9_]", "_", s) or "_x"
    if s[0].isdigit(): s = "_" + s
    if s == "_": s = "_x"
    return s + "_" if s in RS_KW else s


# ── type tables ───────────────────────────────────────────────────────────────
PRIM = {"Z": ("bool", "__c::arg_i({n} as i64)"), "B": ("i8", "__c::arg_i({n} as i64)"), "C": ("u16", "__c::arg_i({n} as i64)"),
        "S": ("i16", "__c::arg_i({n} as i64)"), "I": ("i32", "__c::arg_i({n} as i64)"), "J": ("i64", "__c::arg_i({n})"),
        "F": ("f32", "__c::arg_f({n})"), "D": ("f64", "__c::arg_d({n})")}
PRIM_NAME = {"Z": "bool", "B": "byte", "C": "char", "S": "short", "I": "int", "J": "long", "F": "float", "D": "double"}
ARR_T = {"[": "u8", "]": "i32", "q": "i64", "f": "f32", "d": "f64", "b": "bool", "c": "u16", "h": "i16"}
BOXED = {"java/lang/Boolean": ("bool", "box_bool"), "java/lang/Byte": ("i8", "box_i8"), "java/lang/Short": ("i16", "box_i16"),
         "java/lang/Character": ("u16", "box_char"), "java/lang/Integer": ("i32", "box_i32"), "java/lang/Long": ("i64", "box_i64"),
         "java/lang/Float": ("f32", "box_f32"), "java/lang/Double": ("f64", "box_f64")}
V = "::std::vec::Vec"
ARR_RET = {"byte[]": (f"{V}<u8>", "__c::arr_vec::<u8>(&mut out)"), "int[]": (f"{V}<i32>", "__c::arr_vec::<i32>(&mut out)"),
           "long[]": (f"{V}<i64>", "__c::arr_vec::<i64>(&mut out)"), "float[]": (f"{V}<f32>", "__c::arr_vec::<f32>(&mut out)"),
           "double[]": (f"{V}<f64>", "__c::arr_vec::<f64>(&mut out)"), "boolean[]": (f"{V}<bool>", "__c::arr_bool(&mut out)"),
           "char[]": (f"{V}<u16>", "__c::arr_vec::<u16>(&mut out)"), "short[]": (f"{V}<i16>", "__c::arr_vec::<i16>(&mut out)"),
           "java.lang.String[]": (f"{V}<::std::string::String>", "__c::arr_strings(&mut out)")}
OBJ_VEC = (f"{V}<__c::Object>", "__c::arr_objs(&mut out)")


def param_spec(p, tag, i):
    n, h = f"a{i}", f"h{i}"
    if tag in PRIM:
        t, a = PRIM[tag]
        return dict(ty=t, hold=None, arg=a.format(n=n), lt=False)
    if tag == "s":
        return dict(ty="impl ::std::convert::Into<__c::JStr<'a>>", hold=f"let {h}: __c::JStr<'a> = {n}.into();", arg=f"{h}.arg()", lt=True)
    if tag in ARR_T:
        t = ARR_T[tag]
        return dict(ty=f"impl ::std::convert::Into<__c::ArrArg<'a, {t}>>", hold=f"let {h}: __c::ArrArg<'a, {t}> = {n}.into();", arg=f"{h}.arg()", lt=True)
    if tag == "T":
        return dict(ty="impl ::std::convert::Into<__c::StrArrArg<'a>>", hold=f"let {h}: __c::StrArrArg<'a> = {n}.into();", arg=f"{h}.arg()", lt=True)
    if tag == "A":
        return dict(ty="impl ::std::convert::Into<__c::ObjArrArg>", hold=f"let {h}: __c::ObjArrArg = {n}.into();", arg=f"{h}.arg()", lt=False)
    if tag == "M":
        return dict(ty="impl ::std::convert::Into<__c::ListArg>", hold=f"let {h}: __c::ListArg = {n}.into();", arg=f"{h}.arg()", lt=False)
    if tag == "N":
        return dict(ty="impl ::std::convert::Into<__c::MapArg>", hold=f"let {h}: __c::MapArg = {n}.into();", arg=f"{h}.arg()", lt=False)
    if tag in ("a", "p"):
        return dict(ty="impl __c::IntoCallback", hold=f"let {h} = {n}.into_callback();", arg=f"{h}.arg()", lt=False)
    if tag == "L" and p.get("is_boxed") and p.get("jni_class") in BOXED:
        t, fn = BOXED[p["jni_class"]]
        return dict(ty=f"::core::option::Option<{t}>", hold=f"let {h} = {n}.map(__c::{fn}).unwrap_or_default();", arg=f"{h}.arg()", lt=False)
    return dict(ty="&__c::Object", hold=None, arg=f"{n}.arg()", lt=False)


def ret_spec(ret_id, ret_java):
    simple = {0: ("()", "v", None), 1: ("bool", "z", "r != 0"), 2: ("i8", "i", "r as i8"), 3: ("u16", "i", "r as u16"),
              4: ("i16", "i", "r as i16"), 5: ("i32", "i", "r as i32"), 6: ("i64", "j", "r"), 7: ("f32", "d", "r as f32"),
              8: ("f64", "d", "r"), 9: ("::std::string::String", "str", "__c::to_std(&r)")}
    if ret_id in simple: return simple[ret_id]
    if ret_id == 11:
        t, e = ARR_RET.get(ret_java, OBJ_VEC); return t, "arr", e
    if ret_id == 12: return OBJ_VEC[0], "list", OBJ_VEC[1]
    return "__c::Object", "o", "__c::Object::adopt(r)"          # 10, 13 (Map)


# ── constants ─────────────────────────────────────────────────────────────────
FLT = r"-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?"
STR_OK = re.compile(r'^"(?:[^"\\]|\\[\\"nrt0\'])*"$')
def _fl(v): return v if re.search(r"[.eE]", v) else v + ".0"

def const_literal(f):
    if not (f.get("is_static") and f.get("is_final")): return None
    v, d = f.get("constant_value"), f.get("descriptor") or f.get("jni_signature", "")
    if v is None: return None
    v = v.strip()
    if d in ("I", "S", "B") and re.fullmatch(r"-?\d+", v): return {"I": "i32", "S": "i16", "B": "i8"}[d], v
    if d == "J" and re.fullmatch(r"-?\d+[lL]?", v):
        n = int(v.rstrip("lL"))
        return ("i64", "i64::MIN") if n == -2**63 else ("i64", str(n))
    if d == "Z" and v in ("true", "false"): return "bool", v
    if d == "F" and re.fullmatch(FLT + r"[fF]?", v): return "f32", _fl(v.rstrip("fF"))
    if d == "D" and re.fullmatch(FLT + r"[dD]?", v): return "f64", _fl(v.rstrip("dD"))
    if d == "Ljava/lang/String;" and STR_OK.match(v): return "&'static str", v
    return None


# ── method collection / naming ────────────────────────────────────────────────
def sigkey(m):
    s = m.get("jni_signature", ""); return s[: s.find(")") + 1] if ")" in s else s

def collect_methods(allc, data):
    """own + ancestors + all interfaces (+ java.lang.Object); child overrides win; statics only from the class itself."""
    seen, out = set(), []
    def add(d, own):
        for m in d.get("methods", []):
            if m.get("is_constructor") or m.get("is_private"): continue
            if m.get("is_static") and not own: continue
            k = (m["name"], sigkey(m), bool(m.get("is_static")))
            if k in seen: continue
            seen.add(k); out.append((m, d["class_id"]))
    add(data, True)
    chain, p, guard = [data], data.get("parent_fqn", ""), set()
    while p and p in allc and p not in guard:
        guard.add(p); chain.append(allc[p]); add(allc[p], False); p = allc[p].get("parent_fqn", "")
    q, seen_i = [i for d in chain for i in d.get("interfaces", [])], set()
    while q:
        i = q.pop(0)
        if i in seen_i or i not in allc: continue
        seen_i.add(i); add(allc[i], False); q += allc[i].get("interfaces", [])
    obj = allc.get("java.lang.Object")
    if obj is not None and obj is not data: add(obj, False)
    return out

def simple_t(p):
    jt = p.get("java_type", "obj")
    dims = len(jt) - len(jt.lstrip("["))
    core = jt[dims:]
    if "[]" in core: dims += core.count("[]"); core = core.replace("[]", "")
    core = PRIM_NAME.get(core) or re.sub(r"[^a-zA-Z0-9]", "", core.split(".")[-1].split("$")[-1]) or "obj"
    return core + "Arr" * dims

def _rank(tag):
    """Which overload deserves the plain name: String < object/callback < array/collection < primitive."""
    if tag == "s": return 0
    if tag in ("L", "a", "p"): return 1
    if tag in PRIM: return 3
    return 2

def mk_entries(items, ctor=False):
    es = []
    for m, mcid in items:
        tags, params = m.get("param_tags", ""), m.get("params", [])
        if len(params) > 32 or len(tags) != len(params): continue
        specs = [param_spec(p, t, i) for i, (t, p) in enumerate(zip(tags, params))]
        es.append(dict(m=m, cid=mcid, params=params, tags=tags, specs=specs, static=bool(m.get("is_static")),
                       name="new" if ctor else m["name"],
                       # deprecated overloads (api-versions.xml `deprecated`, set by stage 04.5) never win the plain name
                       dep=0 if ctor else (1 if m.get("sdk_deprecated") is not None else 0)))
    return es

def _suffix(e, base):
    """Short overload suffix: only the parameter types that differ (by position) from the plain-named overload."""
    bp = [simple_t(p) for p in base["params"]]
    parts = [simple_t(p) for i, p in enumerate(e["params"]) if i >= len(bp) or simple_t(p) != bp[i]]
    if not parts:                                   # e.g. same simple names from different packages
        parts = [simple_t(p) for p in e["params"]] or ["0"]
    return "_".join(parts)


def assign_names(entries, used):
    groups = defaultdict(list)
    for e in entries:
        b = ident(e["name"])
        if b in RESERVED: b += "_"
        groups[b].append(e)
    for b in sorted(groups):
        g = sorted(groups[b], key=lambda e: (e["dep"], len(e["params"]), tuple(_rank(t) for t in e["tags"]),
                                             sigkey(e["m"]), e["static"]))
        for k, e in enumerate(g):
            name = b if k == 0 else b + "_" + _suffix(e, g[0])
            cand, n = name, 2
            while cand in used: cand = f"{name}_{n}"; n += 1
            used.add(cand); e["name"] = cand


def body(L, kind, target, cid, slot, specs, conv, ind="        "):
    for s in specs:
        if s["hold"]: L.append(ind + s["hold"])
    n = len(specs)
    if n: L.append(f"{ind}let args = [{', '.join(s['arg'] for s in specs)}];")
    ap = "args.as_ptr()" if n else "::core::ptr::null()"
    L.append(f"{ind}let mut err: i32 = 0;")
    if kind == "ctor":
        L.append(f"{ind}let r = unsafe {{ __f::stratum_new_instance({cid}, {slot}, {ap}, &mut err) }};")
        L.append(f"{ind}__c::check(err)?;"); L.append(f"{ind}Ok(Self(__c::Object::adopt(r)))"); return
    if kind == "v":
        L.append(f"{ind}unsafe {{ __f::stratum_call_v({target}, {cid}, {slot}, {ap}, &mut err); }}")
        L.append(f"{ind}__c::check(err)"); return
    if kind in ("arr", "list"):
        L.append(f"{ind}let mut out = __c::empty_arr();")
        L.append(f"{ind}unsafe {{ __f::stratum_call_{kind}({target}, {cid}, {slot}, {ap}, &mut out, &mut err); }}")
    else:
        L.append(f"{ind}let r = unsafe {{ __f::stratum_call_{kind}({target}, {cid}, {slot}, {ap}, &mut err) }};")
    L.append(f"{ind}__c::check(err)?;"); L.append(f"{ind}Ok({conv})")


def gen_fn(L, e, rtype, is_self, kind, conv, cid, slot):
    specs = e["specs"]
    lt = "<'a>" if any(s["lt"] for s in specs) else ""
    ps = (["&self"] if is_self else []) + [f"a{i}: {s['ty']}" for i, s in enumerate(specs)]
    L.append("    #[inline]")
    L.append(f"    pub fn {e['name']}{lt}({', '.join(ps)}) -> __c::Result<{rtype}> {{")
    body(L, kind, "self.0.raw()" if is_self else "0", cid, slot, specs, conv)
    L += ["    }", ""]


SETTERS = {1: ("bool", "z", "v as u8"), 2: ("i8", "i", "v as i64"), 3: ("u16", "i", "v as i64"), 4: ("i16", "i", "v as i64"),
           5: ("i32", "i", "v as i64"), 6: ("i64", "j", "v"), 7: ("f32", "d", "v as f64"), 8: ("f64", "d", "v"),
           9: ("STR", "str", None), 10: ("&__c::Object", "o", "v.raw()")}


def emit_class(allc, data, names):
    fqn, cid = data["fqn"], data["class_id"]
    pk, cname = names[fqn]
    no_ctor = bool(data.get("is_abstract") or data.get("is_interface") or data.get("is_annotation") or data.get("is_enum"))
    api = []
    L = [f"/// Java class `{fqn}`.", "#[repr(transparent)]", "#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]",
         f"pub struct {cname}(pub __c::Object);",
         f"impl ::std::ops::Deref for {cname} {{ type Target = __c::Object; #[inline(always)] fn deref(&self) -> &__c::Object {{ &self.0 }} }}",
         f"impl __c::AsRaw for {cname} {{ #[inline(always)] fn handle(&self) -> i64 {{ self.0.raw() }} }}",
         f'impl __c::JavaClass for {cname} {{ const CLASS_ID: u32 = {cid}; const FQN: &\'static str = "{fqn}"; '
         f"#[inline(always)] fn wrap(h: i64) -> Self {{ Self(__c::Object::adopt(h)) }} }}",
         f"impl __c::IntoRet for {cname} {{ #[inline] fn set_ret(self, o: &mut __f::StratumVal) {{ __c::put_obj(o, self.0.raw()); }} }}",
         f"impl {cname} {{"]
    used = set()

    if not no_ctor:
        ces = mk_entries([(m, cid) for m in data.get("methods", []) if m.get("is_constructor") and not m.get("is_private")], True)
        assign_names(ces, used)
        for e in ces:
            gen_fn(L, e, "Self", False, "ctor", None, cid, e["m"]["slot"])
            api.append(f"{fqn}: {e['name']}({', '.join(s['ty'] for s in e['specs'])}) -> Self")

    es = mk_entries(collect_methods(allc, data))
    assign_names(es, used)
    for e in es:
        m = e["m"]
        rtype, kind, conv = ret_spec(m.get("ret_type_id", 10), m.get("return_java_type", ""))
        gen_fn(L, e, rtype, not e["static"], kind, conv, e["cid"], m["slot"])
        api.append(f"{fqn}: {e['name']}({', '.join(s['ty'] for s in e['specs'])}) -> {rtype}")

    allf = [(f, cid) for f in data.get("fields", [])]
    p, g = data.get("parent_fqn", ""), set()
    while p and p in allc and p not in g:
        g.add(p); allf += [(f, allc[p]["class_id"]) for f in allc[p].get("fields", [])]; p = allc[p].get("parent_fqn", "")
    seenf = set()
    for f, fcid in allf:
        if f.get("is_private") or f["name"] in seenf: continue
        seenf.add(f["name"])
        fslot, rid, is_static = f.get("slot", 0), f.get("ret_type_id", 10), bool(f.get("is_static"))
        c = const_literal(f); cn = ident(f["name"])
        if c and cn not in used:
            used.add(cn); L += [f"    pub const {cn}: {c[0]} = {c[1]};", ""]; api.append(f"{fqn}: const {cn}: {c[0]}"); continue
        nm = ident(f["name"]); pf = "sf" if is_static else "f"
        tgt = "0" if is_static else "self.0.raw()"; selfp = "" if is_static else "&self"
        rt, kind, conv = ret_spec(rid, f.get("java_type", ""))
        fk = {"v": None, "z": "z", "i": "i", "j": "j", "d": "d", "str": "str", "o": "o", "arr": "arr", "list": "o"}[kind]
        gname = f"{pf}_get_{nm}"
        if fk and gname not in used:
            used.add(gname)
            if kind == "list": rt, conv = "__c::Object", "__c::Object::adopt(r)"
            L.append("    #[inline]")
            L.append(f"    pub fn {gname}({selfp}) -> __c::Result<{rt}> {{")
            L.append("        let mut err: i32 = 0;")
            if fk == "arr":
                L += ["        let mut out = __c::empty_arr();",
                      f"        unsafe {{ __f::stratum_field_get_arr({tgt}, {fcid}, {fslot}, &mut out, &mut err); }}"]
            else:
                L.append(f"        let r = unsafe {{ __f::stratum_field_get_{fk}({tgt}, {fcid}, {fslot}, &mut err) }};")
            L += ["        __c::check(err)?;", f"        Ok({conv})", "    }", ""]
            api.append(f"{fqn}: {gname}() -> {rt}")
        sname = f"{pf}_set_{nm}"
        st = SETTERS.get(rid)
        if not f.get("is_final") and st and sname not in used:
            used.add(sname)
            sp = (selfp + ", ") if selfp else ""
            L.append("    #[inline]")
            if st[0] == "STR":
                L.append(f"    pub fn {sname}<'a>({sp}v: impl ::std::convert::Into<__c::JStr<'a>>) -> __c::Result<()> {{")
                L += ["        let s: __c::JStr<'a> = v.into();", "        let mut err: i32 = 0;",
                      f"        unsafe {{ __f::stratum_field_set_str({tgt}, {fcid}, {fslot}, s.raw(), &mut err); }}"]
            else:
                L.append(f"    pub fn {sname}({sp}v: {st[0]}) -> __c::Result<()> {{")
                L += ["        let mut err: i32 = 0;",
                      f"        unsafe {{ __f::stratum_field_set_{st[1]}({tgt}, {fcid}, {fslot}, {st[2]}, &mut err); }}"]
            L += ["        __c::check(err)", "    }", ""]
            api.append(f"{fqn}: {sname}(v)")
    L.append("}")
    return "\n".join(L), api


def main():
    ap = argparse.ArgumentParser(description="Stratum Stage 25 - Rust SDK emitter")
    _clean = lambda p: Path(str(p).strip(' "\'')).resolve()
    ap.add_argument("--input", required=True, help="05_resolve/output_patched/ (any location)")
    ap.add_argument("--output", default=None,
                    help="Folder that receives the generated crate (Cargo.toml, src/**, API_INDEX.txt). Default: <this folder>/output/crate")
    ap.add_argument("--engine", default=str(REPO / "stratum-sys" / "20_engine_core"),
                    help="20_engine_core folder (only include/stratum_c_abi.h is read, for the ABI drift check)")
    ap.add_argument("--emit-prefix", default="",
                    help="comma-separated FQN prefixes (empty = everything; large). android.app.Activity is always emitted.")
    a = ap.parse_args()

    inp = _clean(a.input)
    if not inp.exists():
        raise SystemExit(f"--input not found: {inp}")
    crate = _clean(a.output) if a.output else (HERE / "output" / "crate").resolve()
    check_abi(_clean(a.engine) / "include" / "stratum_c_abi.h")

    if crate.exists(): shutil.rmtree(crate)
    (crate / "src" / "api").mkdir(parents=True)
    T = HERE / "templates"
    shutil.copy2(T / "Cargo.toml", crate / "Cargo.toml")                       # no build.rs: Gradle/CMake build the engine
    for n in ("lib.rs", "base.rs", "ffi.rs", "reflect.rs", "app.rs"): shutil.copy2(T / n, crate / "src" / n)
    hx = meta_hash(inp)
    (crate / "src" / "meta_hash.rs").write_text(f'pub const STRATUM_META_HASH: &str = "{hx}";\n', encoding="utf-8")

    allc = {}
    for jf in sorted(inp.rglob("*.json")):
        if jf.name in SKIP: continue
        d = json.loads(jf.read_text(encoding="utf-8"))
        if d.get("fqn") and "class_id" in d and "." in d["fqn"]: allc[d["fqn"]] = d
    if not allc: raise SystemExit("No resolved classes.")
    pre = [p.strip() for p in a.emit_prefix.split(",") if p.strip()]
    emit = {f: d for f, d in allc.items() if not d.get("is_annotation")
            and (not pre or any(f.startswith(p) for p in pre) or f == "android.app.Activity")}
    if "android.app.Activity" not in emit: raise SystemExit("android.app.Activity missing from input")

    by_pkg = defaultdict(list)
    for f in emit: by_pkg[tuple(ident(p) for p in f.rsplit(".", 1)[0].split("."))].append(f)
    children, all_pk = defaultdict(set), set()
    for pk in by_pkg:
        for i in range(len(pk)):
            children[pk[:i]].add(pk[i]); all_pk.add(pk[:i + 1])
    all_pk.add(())

    names = {}
    for pk, fqns in by_pkg.items():
        taken = set(children[pk])
        for f in sorted(fqns):
            c = ident(f.rsplit(".", 1)[1].replace("$", "_"))
            while c in taken: c += "_"
            taken.add(c); names[f] = (pk, c)

    index, n = [], 0
    for pk in sorted(all_pk):
        L = ["// Auto-generated by Stratum Stage 25. DO NOT EDIT.", "use crate::base as __c;", "use crate::ffi as __f;", ""]
        L += [f"pub mod {ch};" for ch in sorted(children[pk])]
        L.append("")
        for f in sorted(by_pkg.get(pk, [])):
            code, api = emit_class(allc, emit[f], names)
            L += [code, ""]; index += api; n += 1
        dest = crate / "src" / "api"
        for part in pk: dest = dest / part
        dest.mkdir(parents=True, exist_ok=True)
        (dest / "mod.rs").write_text("\n".join(L), encoding="utf-8")
    (crate / "API_INDEX.txt").write_text("\n".join(sorted(index)), encoding="utf-8")
    print(f"-> {n:,} classes, {len(index):,} API entries, meta hash {hx} -> {crate}")

if __name__ == "__main__":
    main()