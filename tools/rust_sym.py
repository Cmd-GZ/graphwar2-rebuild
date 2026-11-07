"""统一判据：这条 legacy Rust mangled 符号**属于哪个 crate**。

为什么需要它：`tools/name_bridge.py`、`tools/sym_match.py`、`tools/win_names.py`、
`tools/three_way_match.py` 各自都要回答同一个问题——「这条 arm64/PE 符号是不是
`graphwarrust` 自己的函数」。三份实现三种口径，就会得到三套「名字集合」，
而§37/§38 的每个数字都建立在这个集合上。本模块是**唯一**的实现。

## 判据（结构解析，不是「包含子串」）

rustc 的 legacy mangling（`-C symbol-mangling-version=legacy`，本版二进制用的就是它）
是一条**带长度前缀**的路径编码：

```text
_ZN < elem > < elem > … 17h<16 hex> E
```

每个 `<elem>` 是 `<十进制长度><该长度的字节>`，所以**段的边界是可以证出来的**，
不需要猜。元素的形态只有三类：

| 形态 | 例 | 含义 |
|---|---|---|
| 普通标识符 | `12graphwarrust` | 路径段；`..` = `::`，`$u20$`/`$LT$` … 是转义 |
| impl / 限定路径 | `133_$LT$graphwarrust..X$u20$as$u20$T$GT$` | 整段是一个元素，内容是 `<SELF as TRAIT>` |
| 合成后缀 | `28_$u7b$$u7b$closure$u7d$$u7d$` | 编译器生成的闭包体，不是源码里的路径 |

于是「属于哪个 crate」= **路径首段的 crate 名**：

* **A 直接路径**：首元素是普通标识符 ⇒ crate 就是它。
  `_ZN12graphwarrust14graphwar_event…` ⇒ `graphwarrust`。
  长度前缀让这件事无歧义：`12` 之后的 12 字节恰好是 `graphwarrust`，
  `graphwarrust_extra` 那种名字会因为长度是 `19` 而不是 `12` 直接落选。
* **B impl 自类型**：首元素是 `_$LT$…$GT$`（GDExtension 的
  `__register_methods` / trait 方法全部是这一形态，crate 名**没有长度前缀**）。
  解出 `<SELF as TRAIT>`，取 **SELF** 的路径首段；
  `_ZN133_$LT$graphwarrust..axis_drawer..AxisDrawer$u20$as$u20$godot_core..…$GT$…`
  ⇒ `graphwarrust`。
* **排除**：首元素是别的 crate（`core`/`alloc`/`godot_core`/`godot_ffi`…）的，
  即使正文里出现 `graphwarrust` 也不算。典型是
  `_ZN4core3ptr…drop_in_place$LT$…graphwarrust..X…$GT$…` —— 那是
  「库函数恰好以游戏类型为泛型实参」的编译器胶水，**不是**原版源码里的函数。
  同理 `<X as graphwarrust::Trait>`（自类型不是游戏的）也不算：
  被实现的函数体属于 `X`。

⚠️ `"graphwarrust" in name` 这种粗糙判据会把上面第三类全部算进来（本版 Android 304 条、
Windows 侧同类若干），并且会把**别的 crate 里**恰好同名的模块算成游戏的。
`tools/win_names.py` 的注释里已经记着这条教训；本模块把它变成可复用、可测的实现。

## 用法

```python
import rust_sym
rust_sym.crate_of(mangled)          # → "graphwarrust" / "core" / … / None
rust_sym.is_game_name(mangled)      # → bool（crate == "graphwarrust"）
rust_sym.classify(mangled)          # → ("direct"|"impl-head"|"other-crate"|"unparsable", crate)
rust_sym.unescape(elem)             # → 可读路径（`..`→`::`，`$u20$`→空格 …）
```

纯字符串处理，无外部依赖，可直接单测（`tests/rust_sym.rs` 或 `python -m doctest`）。
"""

import re

DEFAULT_CRATE = "graphwarrust"

# `_ZN` 之后到 `E` 之前；允许被 `.llvm.<n>` / `.part.<n>` 之类的链接器后缀污染，
# 这类后缀在**最后一个 `E` 之后**，解析时先剥掉。
_LLVM_SUFFIX = re.compile(r"\.(?:llvm|part|cold|isra)\.[0-9A-Za-z_.]+$")

# 长度前缀：十进制数字紧跟元素字节。数字段最长就是 20 位（usize 上限足够）。
_LEN = re.compile(r"(\d{1,20})")


class Elem(object):
    """一个 legacy-mangling 路径元素。"""

    __slots__ = ("raw", "declared_len")

    def __init__(self, raw, declared_len):
        self.raw = raw
        self.declared_len = declared_len

    @property
    def is_impl(self):
        return self.raw.startswith("_$LT$") or self.raw.startswith("$LT$")

    @property
    def is_synthetic(self):
        return self.raw.startswith("_$u") or self.raw.startswith("$u")

    def __repr__(self):
        return "Elem(%r)" % (self.raw,)


def strip_mangling(name):
    """`_ZN…E` → 内部字节串；不是 legacy mangling 返回 None。

    只剥「结构上确定」的两样东西：`_ZN`/`E` 外壳和链接器追加的 `.llvm.N` 后缀。
    哈希尾段 `17h<16 hex>` **不剥**——它是 `elements()` 的最后一个元素，
    剥不剥都不影响「首段的 crate」这个判据。
    """
    if not name or not name.startswith("_ZN"):
        return None
    s = name[3:]
    s = _LLVM_SUFFIX.sub("", s)
    if s.endswith("E"):
        s = s[:-1]
    return s


def elements(name):
    """按长度前缀切段。返回 [Elem]；任何一处长度对不上就返回 None（不可解析）。

    「长度对不上」是**硬失败**而不是尽力而为：长度前缀是这套编码唯一的边界信息，
    一旦它和实际字节不一致，后面的段全是猜的。
    """
    body = strip_mangling(name)
    if body is None:
        return None
    out = []
    i, n = 0, len(body)
    while i < n:
        m = _LEN.match(body, i)
        if not m:
            return None
        ln = int(m.group(1))
        start, end = m.end(), m.end() + ln
        if end > n:
            return None
        out.append(Elem(body[start:end], ln))
        i = end
    return out


# ------------------------------------------------------------------ 转义

_ESC = re.compile(r"\$([A-Za-z0-9]{1,4})\$")


def unescape(s):
    """`$LT$`→`<`、`$u20$`→空格、`..`→`::`；其余保持原样。

    `..` 只在**路径**里是 `::`；本模块只在路径语境（首段、自类型）里调用它，
    且 `$LT$…$GT$` 已被逐字符展开，所以不会破坏 `..=` 之类的运算符（它们不会出现在符号名里）。
    """
    def sub(m):
        tok = m.group(1)
        if tok == "u20":
            return " "
        if tok in ("LT", "GT"):
            return "<" if tok == "LT" else ">"
        if tok.startswith("u"):
            try:
                return chr(int(tok[1:], 16))
            except ValueError:
                pass
        return m.group(0)
    return _ESC.sub(sub, s).replace("..", "::")


def split_top(s, sep):
    """按顶层（不在 `<>`/`()`/`[]` 里）的分隔符切分。`sep` 可以是多字符。"""
    out, depth, cur, i = [], 0, [], 0
    while i < len(s):
        c = s[i]
        if s.startswith(sep, i) and depth == 0:
            out.append("".join(cur))
            cur = []
            i += len(sep)
            continue
        if c in "<([":
            depth += 1
        elif c in ">)]":
            depth -= 1
        cur.append(c)
        i += 1
    out.append("".join(cur))
    return out


_REF_PREFIX = ("&mut ", "&", "*const ", "*mut ", "dyn ", "impl ")


def impl_head_tail(demangled):
    """c++filt 输出 `<SELF as TRAIT>::tail` → (SELF, tail)；不是该形态返回 None。

    只认「**第一个**顶层 `<…>` 正好以 `>::` 结束」这一种形状。别的一律不解析——
    解析器宁可说「不知道怎么切」，也不要切出一个看起来合理的位置。
    """
    if not demangled or not demangled.startswith("<"):
        return None
    depth = 0
    for i, c in enumerate(demangled):
        if c == "<":
            depth += 1
        elif c == ">":
            depth -= 1
            if depth == 0:
                if demangled[i + 1:i + 3] == "::":
                    return demangled[1:i], demangled[i + 3:]
                return None
    return None


def _head_segment(ty):
    """`&graphwarrust::X<T>` → `graphwarrust`；`Option<X>` → `Option`；`<…>` → 递归。

    这一层不「猜」：它只是把类型文本按规定剥掉引用/指针/dyn 前缀，再取顶层第一段。
    """
    t = ty.strip()
    changed = True
    while changed and t:
        changed = False
        for p in _REF_PREFIX:
            if t.startswith(p):
                t = t[len(p):].strip()
                changed = True
                break
    if t.startswith("<"):
        # 嵌套限定路径：< <A as B>::C as D >
        inner, depth, i = None, 0, 0
        while i < len(t):
            if t[i] == "<":
                depth += 1
            elif t[i] == ">":
                depth -= 1
                if depth == 0:
                    inner = t[1:i]
                    break
            i += 1
        if inner is None:
            return None
        parts = split_top(inner, " as ")
        return _head_segment(parts[0])
    if not t or t[0] in "([":          # 元组 / 数组 / 切片：没有 crate 首段
        return None
    first = split_top(t, "::")[0]
    first = first.split("<")[0].strip()
    if not first or not re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", first):
        return None
    return first


# ------------------------------------------------------------------ 判据

def classify(name):
    """→ (kind, crate)。kind ∈ {"direct"[A], "impl-head"[B], "other-crate", "unparsable"}。

    * `direct`     —— 首元素是普通标识符（`_ZN12graphwarrust…`）
    * `impl-head`  —— 首元素是 `<SELF as TRAIT>`，取 SELF 的首段作为 crate
    * `other-crate`—— 首元素属于别的 crate（含 `core::ptr::drop_in_place<graphwarrust::X>`
                      这种「正文里提到游戏类型」的胶水）
    * `unparsable` —— 长度前缀对不上 / 不是 `_ZN` / 首元素是合成后缀

    `crate` 在 `unparsable` 时为 `None`。
    """
    els = elements(name)
    if not els:
        return "unparsable", None
    head = els[0]
    if head.is_synthetic:
        return "unparsable", None
    if not head.is_impl:
        crate = unescape(head.raw)
        if re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", crate):
            return "direct", crate
        return "unparsable", None
    # B 形态：解 <SELF as TRAIT>
    body = head.raw[1:] if head.raw.startswith("_") else head.raw
    body = unescape(body)
    if body.startswith("<") and body.endswith(">"):
        body = body[1:-1]
    parts = split_top(body, " as ")
    crate = _head_segment(parts[0])
    if crate is None:
        return "unparsable", None
    return "impl-head", crate


def crate_of(name):
    """这条符号的 crate 名；判断不出返回 None。"""
    return classify(name)[1]


def form_label(name):
    """provenance 标签（只用于夹具里记录「这个名字从哪种 mangling 形态来」）。

    * `impl-head`   —— `_ZN<len>_$LT$crate..T$u20$as$u20$Trait$GT$…`
      （crate 名**没有长度前缀**；前缀过滤会整类丢掉）
    * `nested-impl` —— `_ZN12crate…<impl Trait for crate::T>…`
      （`#[derive]` 那一类，crate 首段仍是本 crate）
    * `direct`      —— `_ZN12crate…` 直接路径
    * 其它分类原样返回（`other-crate` / `unparsable`）
    """
    kind = classify(name)[0]
    if kind != "direct":
        return kind
    body = strip_mangling(name) or ""
    if "$LT$impl$u20$" in body:
        return "nested-impl"
    return "direct"


def is_crate(name, crate=DEFAULT_CRATE):
    return crate_of(name) == crate


def is_game_name(name, crate=DEFAULT_CRATE):
    """本仓库的口径：`graphwarrust` 自己的函数（A + B）。"""
    return is_crate(name, crate)


def mentions_crate(name, crate=DEFAULT_CRATE):
    """**仅供敏感性对照**：正文里是否出现过 crate 名。

    粗糙判据（旧工具的隐含行为）。保留它只有一个用途：量化「粗糙口径 vs 本判据」
    差了多少名字，并在报告里明确它**不是**判据。绝不要用它筛选名字。
    """
    return bool(name) and crate in name


def path_prefix(name, depth=3):
    """展示用：把 mangled 还原成 `graphwarrust::graphwar_event::Type::method` 的前几段。"""
    els = elements(name)
    if not els:
        return None
    segs = []
    for e in els:
        if e.is_synthetic:
            break
        if e.is_impl:
            body = unescape(e.raw[1:] if e.raw.startswith("_") else e.raw)
            if body.startswith("<") and body.endswith(">"):
                body = body[1:-1]
            self_ty = split_top(body, " as ")[0]
            for s in split_top(self_ty, "::"):
                if s.strip():
                    segs.append(s.strip())
            trait = split_top(body, " as ")
            if len(trait) > 1:
                tname = split_top(trait[1], "::")
                if tname:
                    segs.append("<as %s>" % tname[-1].split("<")[0])
        else:
            segs.extend(split_top(unescape(e.raw), "::"))
        if len(segs) >= depth:
            break
    return "::".join(segs[:depth])
