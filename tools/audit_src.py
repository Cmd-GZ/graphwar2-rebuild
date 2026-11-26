#!/usr/bin/env python3
"""审计面 ②（`src/`）的结构保真度取证器 —— 只读，不改任何代码。

## 为什么有这一份

`docs/coverage-matrix.tsv` 的 `ours_funcs` 是**整个文件的 `fn` 条数**（工具自己在
`docs/02-module-map.md` 里写了这一条），它把 `#[cfg(test)] mod tests` 里的测试也算进去，
所以"`ours_funcs` 比 `orig_funcs` 多 ⇒ 自造 helper"这个读法**会把测试当自造**。
本工具把口径拆开，并给出三条互相独立的判据：

1. `modules`：78 模块的 `orig_funcs` vs **生产函数**（`#[cfg(test)]` 之前）vs 台账口径；
2. `names`：`fixtures/name_to_addr{,_v2}.tsv` + `fixtures/windows_name_to_addr.tsv`
   三份名字夹具并集 ∩ `fixtures/boundary_audit.tsv` 的 527 ⇒ **348**（真名），
   再判每个真名在我方源码里落在哪个文件（本模块 / 仅 `_scaffold/` / 别的文件 / 全无）；
3. `scaffold`：`src/_scaffold/*.rs` 的生产函数清单 + 是否只是转发（调 `crate::<原版模块>`）。

## 口径

* 地址一律 **ELF vaddr（RVA）**（与 `fixtures/boundary_audit.tsv` / `name_to_addr_v2.tsv` 同口径；
  Ghidra/REA = 本口径 + 0x100000）。
* "生产函数" = 文件中第一个 `#[cfg(test)]` **之前**的 `fn` 条数；测试函数另外计数。
* 名字匹配只看 **leaf 名**（`GraphwarX::method` → `method`）。同名不代表实现相同，
  本工具只用来定位"原版有这个名字、我方哪个文件里也有这个名字"。

## 用法

    python3 tools/audit_src.py modules
    python3 tools/audit_src.py names
    python3 tools/audit_src.py names --module graphwar_obstacle_manager
    python3 tools/audit_src.py scaffold
    python3 tools/audit_src.py tsv > fixtures/source_audit_src_machine.tsv   # 机器部分
"""

import argparse
import collections
import glob
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

FN_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
TEST_RE = re.compile(r"^#\[cfg\(test\)\]", re.M)

# 编译器/宏生成的名字（原版源码里**没有对应 fn**，不该要求我方有同名函数）
GENERATED = (
    "__inner_init", "__register_methods", "__impl_", "__gdext", "panic_cold_explicit",
    "drop_in_place", "::{{closure}}", "__vtable", "__drop_glue", "__class_",
)
# 通用 leaf 名：在多个 gdext 类里都出现，单看"哪个文件有这个名字"没有意义
GENERIC = {
    "new", "process", "ready", "draw", "init", "tick", "default", "drop", "fmt",
    "from", "clone", "eq", "hash", "serialize", "deserialize", "pump", "start",
}


def read(path):
    with open(os.path.join(ROOT, path), encoding="utf-8", errors="replace") as fh:
        return fh.read()


def rows_of(path):
    out = []
    with open(os.path.join(ROOT, path), encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("#"):
                continue
            out.append(line.rstrip("\n").split("\t"))
    return out


def _strip_test_blocks(text):
    """去掉 **`#[cfg(test)]` 包住的项**（`mod tests {…}` 与 `#[cfg(test)] use …;`）。

    ⚠️ 不能简单取"第一个 `#[cfg(test)]` 之前"：`src/graphwar_play_screen_control.rs`
    第 324 行有一条 **`#[cfg(test)] use`**（只作用于一个 use），后面还有 800 行生产代码。
    早期版本就是踩了这个坑（把 prod=27 数成 prod=5）。所以这里按括号匹配整块挖掉。
    """
    out, i, n = [], 0, len(text)
    pat = re.compile(r"#\[cfg\(test\)\]")
    while True:
        m = pat.search(text, i)
        if not m:
            out.append(text[i:])
            break
        out.append(text[i:m.start()])
        j = m.end()
        mod_head = re.compile(r"\s*(?:pub\s+)?mod\s+\w+\s*\{").match(text, j)
        if mod_head:
            k, depth = mod_head.end() - 1, 0
            while k < n:
                if text[k] == "{":
                    depth += 1
                elif text[k] == "}":
                    depth -= 1
                    if depth == 0:
                        k += 1
                        break
                k += 1
            i = k
        else:
            use_head = re.compile(r"\s*(?:pub\s+)?use\s[^;]*;").match(text, j)
            i = use_head.end() if use_head else j
    return "".join(out)


def split_fns(text):
    """(生产函数名, 测试函数名)。测试 = 被 `#[cfg(test)]` 包住的那几块里的 fn。"""
    prod = _strip_test_blocks(text)
    return FN_RE.findall(prod), FN_RE.findall(text)[len(FN_RE.findall(prod)):]


def src_files():
    out = []
    base = os.path.join(ROOT, "src")
    for dirpath, _dirs, files in os.walk(base):
        for name in sorted(files):
            if name.endswith(".rs"):
                out.append(os.path.relpath(os.path.join(dirpath, name), ROOT).replace(os.sep, "/"))
    return sorted(out)


def module_rows():
    return [r for r in rows_of("docs/coverage-matrix.tsv") if r[0] != "module"]


def inventory():
    inv = {}
    for rel in src_files():
        p, t = split_fns(read(rel))
        inv[rel] = {"prod": p, "test": t}
    return inv


def ledger_map():
    """module -> [我方文件]（台账 ours_file 列）。"""
    out = {}
    for r in module_rows():
        out[r[0]] = [f for f in r[5].split("+") if f]
    # 模块名 ↔ 我方文件名：coverage-matrix 的 module 列就是原版文件名
    return out


def name_rows():
    """348 = 三份名字夹具的地址并集 ∩ 527；返回 addr -> (module, type, method, src)。"""
    b527 = set()
    for r in rows_of("fixtures/boundary_audit.tsv"):
        if r[0] == "addr":
            continue
        b527.add(r[0].lower())
    best = {}

    def put(addr, pri, module, typ, method):
        addr = addr.lower()
        if addr and (addr not in best or best[addr][0] < pri):
            best[addr] = (pri, module, typ, method)

    for r in rows_of("fixtures/name_to_addr.tsv")[1:]:
        put(r[3], 1, r[0], r[1], r[2])
    for r in rows_of("fixtures/name_to_addr_v2.tsv")[1:]:
        put(r[3], 3, r[0], r[1], r[2])
    for r in rows_of("fixtures/windows_name_to_addr.tsv")[1:]:
        if r[5].startswith("0x"):
            put(r[5], 2, r[1], r[2], r[3])
    named = {a: v for a, v in best.items() if a in b527}
    return b527, best, named


def is_generated(method):
    return any(g in method for g in GENERATED) or "::<" in method


# ---------------------------------------------------------------------------
# 子命令
# ---------------------------------------------------------------------------

def cmd_modules(_args):
    inv = inventory()
    rows = module_rows()
    tot = collections.Counter()
    per = []
    for r in rows:
        module, orig, files, ledger = r[0], int(r[2]), [f for f in r[5].split("+") if f], int(r[6])
        prod = sum(len(inv[f]["prod"]) for f in files if f in inv)
        test = sum(len(inv[f]["test"]) for f in files if f in inv)
        per.append((module, orig, prod, test, ledger, files))
        tot["orig"] += orig
        tot["prod"] += prod
        tot["test"] += test
        tot["ledger"] += ledger
    mapped = {f for _m, _o, _p, _t, _l, fs in per for f in fs}
    all_prod = sum(len(v["prod"]) for v in inv.values())
    all_test = sum(len(v["test"]) for v in inv.values())
    scaf_prod = sum(len(v["prod"]) for k, v in inv.items() if k not in mapped)

    print("== 口径 ==")
    print("  台账口径 sum(ours_funcs)      = %d（含 #[cfg(test)] 里的 fn）" % tot["ledger"])
    print("  同 78 行的生产函数 sum(prod)  = %d；测试函数 sum(test) = %d" % (tot["prod"], tot["test"]))
    print("  sum(orig_funcs)               = %d" % tot["orig"])
    print("  全 src/ 生产函数 = %d，测试函数 = %d（测试占 %.0f%%）"
          % (all_prod, all_test, 100.0 * all_test / (all_prod + all_test)))
    print("  台账没覆盖到的文件（= _scaffold/ 等）生产函数 = %d，占全部生产函数 %.1f%%"
          % (scaf_prod, 100.0 * scaf_prod / all_prod))
    print()
    print("== 分布（生产函数 / orig_funcs）==")
    buckets = collections.Counter()
    for module, orig, prod, test, ledger, files in per:
        if orig == 0:
            buckets["orig=0"] += 1
        elif prod == 0:
            buckets["prod=0"] += 1
        else:
            r = prod / orig
            key = ("<0.6x" if r < 0.6 else "0.6-1.0x" if r < 1.0 else "1.0-1.5x" if r < 1.5
                   else "1.5-2.5x" if r < 2.5 else "2.5-4x" if r < 4 else ">4x")
            buckets[key] += 1
    for k in ("<0.6x", "0.6-1.0x", "1.0-1.5x", "1.5-2.5x", "2.5-4x", ">4x", "prod=0", "orig=0"):
        print("  %8s %3d" % (k, buckets[k]))
    print()
    print("== 明细 ==")
    print("%-44s %5s %5s %5s %6s %6s" % ("module", "orig", "prod", "test", "ledger", "r"))
    for module, orig, prod, test, ledger, _files in per:
        ratio = ("%.2f" % (prod / orig)) if orig else "—"
        print("%-44s %5d %5d %5d %6d %6s" % (module, orig, prod, test, ledger, ratio))
    print()
    print("== 台账文件里生产函数 < orig_funcs 的模块（文件名叫对了但函数不在）==")
    for module, orig, prod, test, ledger, files in per:
        if orig >= 2 and prod < orig:
            print("  %-42s orig=%-3d prod=%-3d test=%-3d %s" % (module, orig, prod, test, files))
    return 0


def cmd_names(args):
    b527, _best, named = name_rows()
    inv = inventory()
    modfiles = ledger_map()
    name2files = collections.defaultdict(list)
    for rel, v in inv.items():
        for n in set(v["prod"]):
            name2files[n].append(rel)

    real = {a: v for a, v in named.items() if not is_generated(v[3]) and not (v[2] == "_" and v[3] == "_")}
    print("527 个可归属函数里：有真名 %d（= 真名 fixture 并集 ∩ 527；与 docs/00 §… 的 348 一致），"
          "其中编译器/宏生成名 %d，可要求的真名 %d"
          % (len(named), len(named) - len(real), len(real)))
    stat = collections.Counter()
    out = []
    for addr, (_pri, module, typ, method) in sorted(real.items()):
        if args.module and module != args.module:
            continue
        modfiles_set = set(modfiles.get(module + ".rs", []))
        where = name2files.get(method, [])
        in_mod = [f for f in where if f in modfiles_set]
        in_scaf = [f for f in where if "/_scaffold/" in f]
        if in_mod:
            kind = "本模块文件"
        elif in_scaf:
            kind = "仅scaffold"
        elif where:
            kind = "别的src文件"
        else:
            kind = "全无"
        stat[kind] += 1
        out.append((addr, module, typ, method, kind,
                    in_mod[0] if in_mod else (",".join(in_scaf[:3]) if in_scaf else
                                              (",".join(where[:3]) if where else ""))))
    print("  分布：", dict(stat))
    print()
    for kind in ("仅scaffold", "别的src文件", "全无"):
        sel = [o for o in out if o[4] == kind]
        if not sel:
            continue
        print("== %s（%d）==" % (kind, len(sel)))
        for addr, module, typ, method, _k, where in sel:
            print("  %s %-34s %-32s %s" % (addr, "%s::%s" % (typ or module, method), where,
                                           "" if method not in GENERIC else "(通用名)"))
        print()
    return 0


def cmd_scaffold(_args):
    inv = inventory()
    print("%-46s %5s %5s %8s %8s %s" % ("file", "prod", "test", "origcall", "ruleish", "备注"))
    for rel in sorted(k for k in inv if "/_scaffold/" in k):
        text = read(rel)
        prod = inv[rel]["prod"]
        orig_calls = len(re.findall(r"crate::(graphwar_\w+|entity_drawer|math_function|plane|"
                                    r"turnstate|game|events|server|axis_drawer|effect_drawer|"
                                    r"function_\w+|player_showcase|weak_caller|obstacle_drawer|"
                                    r"polish_notation_converter|backtracer|button_manager)", text))
        ruleish = len([n for n in prod if re.search(
            r"(fire|shoot|hit|kill|damage|life|alive|dead|death|score|fitness|mutat|crossover|"
            r"evolv|turn|round|winner|verdict|team|target|soldier|obstacle|tick|aim|path|"
            r"settle|gate|rule|level|campaign|rank|bot|spawn|relayout|look)", n, re.I)])
        print("%-46s %5d %5d %8d %8d" % (rel, len(prod), len(inv[rel]["test"]), orig_calls, ruleish))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("modules", help="78 模块 orig vs prod vs 台账口径")
    p.set_defaults(func=cmd_modules)
    p = sub.add_parser("names", help="348 真名落在我方哪个文件")
    p.add_argument("--module", help="只看某个原版模块（如 graphwar_obstacle_manager）")
    p.set_defaults(func=cmd_names)
    p = sub.add_parser("scaffold", help="_scaffold 逐文件生产函数/转发/规则味")
    p.set_defaults(func=cmd_scaffold)
    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
