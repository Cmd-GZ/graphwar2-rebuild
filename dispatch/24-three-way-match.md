# 24 · `agent/three-way-match`：三向符号匹配器（**算力活，规模优先**）

**优先级 P0。** 这是目前**唯一可能大规模提升"可定名函数"数量**的路子，且是纯算力活，适合你这台机器。

## 0. 现在有什么

| 构建 | 架构 | 有名字 | 用途 |
|---|---|---|---|
| `libgraphwarrust.so` 2026-08-28 | **x86-64** | ❌ stripped | **逆向目标**（13,503 个函数） |
| `libgraphwarrust.so` android-2.0.60 | arm64 | ✅ 524 唯一名 | 名字锚（**跨架构**） |
| `graphwarrust-2.0.60.dll`（Win） | **x86-64** | ✅ **939 唯一名** | 同版本 + 同 ISA |
| `graphwarrust-early2026.dll` | x86-64 | ✅ 746 唯一名 | 更早构建，版本差分 |

**Windows 两份 DLL 已打包给你**：`dist/gw2-windows-refs.tar.zst`（14 MB，含两个 DLL + `SHA256SUMS` + 说明）。
拿不到就自己下：Steam 控制台 `download_depot 3066840 3066842`（**下完立刻 `cp` 出来**，同一 depot 会覆盖）。

## 1. 硬约束（先读，别浪费时间）

**能跨构建稳定对齐的键只有 `panic::Location` 三元组 `(file, line, col)`**，目标里共 **2,035 个**。

```
⇒ 名字最多锚住 2,035 个函数，**不是 13,503**
⇒ 剩下 11,468 个（纯数学 / 闭包 / 单态化 / 内联）符号表救不了
```

本任务的目标**不是**"给 13.5k 全覆盖"，而是：

```
(a) 把 2,035 这个天花板里的名字尽量落地；
(b) 对天花板之外的，用**结构指纹**给出"可归属率"的**上限估计**（不是硬凑）
```

## 2. 要做什么

1. **搭通用匹配器**（一份代码，四种构建都能吃）：
   - 输入：ELF/PE，输出 `func_addr → (name?, location?, size, callee 多重集, 常量集, 指令直方图)`
   - 名字解析：ELF `.symtab` / PE COFF symbol table（`objdump -t` 或自己解析）
   - Location 解析：**ELF 在 `.data.rel.ro` + `R_X86_64_RELATIVE`；PE 在 `.rdata`/`.data`，很可能是基于 ImageBase 的绝对地址** —— **自己判断，别照搬**。
2. **Location 三向校验**（同版本的自证）：
   - Windows 2.0.60 的 Location 三元组 与 Linux 目标求交，**应接近 2,035**；
   - 差得多 ⇒ **报告"DLL 与目标不同版本"**，不要硬凑。
3. **名字落地**：`Win 名字 → Location → Linux 地址`；Android 提供第二来源；**冲突逐条列出**（同 Location 不同名）。
4. **天花板之外的上限估计**：对没有 Location 的函数，用（尺寸 + 被调者多重集 + 常量）做**跨 ISA/跨 ABI 归一化**后的最近邻，
   给出"高置信匹配率"。**必须给正对照**：用"已知有 Location 且已定名"的函数验证你的匹配器的准确率。
5. **产出夹具**：`fixtures/name_to_addr_v2.tsv`（或在既有文件上加列，**不要破坏** `name_to_addr.tsv` 的既有格式）。

## 3. 环境

- 本任务在**你自己的机器**上做（算力优先）。仓库：`git clone git@github.com:Cmd-GZ/graphwar2-rebuild.git`，分支 `agent/three-way-match`
- ⚠️ **分支前缀必须是 `agent/`**，`CG/` 推不上去（远端已有 `CG`，git 不允许子引用）
- ⚠️ **私有 `CARGO_TARGET_DIR`**（并行会串味）
- 目标二进制、Android 参考：见工具包 `dist/gw2-toolkit.tar.zst`；Windows 两份见 `dist/gw2-windows-refs.tar.zst`
- 函数起点表：`analysis/funcstarts_ghidra_va.pkl`（**值是字符串 hex**；别用旧的 `funcstarts.pkl`，那是错的）
- 既有工具可复用：`tools/sym_match.py`（1590 行）、`tools/name_bridge.py`、`tools/loc_xrefs.py`、`tools/addr_probe.py`
- python+capstone：工具包里有 `.venv`；没有就自建（`pip install capstone`）

## 4. 可以改的文件

- `tools/`（新增匹配器；可扩展 `sym_match.py`）
- `fixtures/`（新增文件；**不覆盖**既有夹具）
- `docs/02-module-map.md` 追加一节（方法与量化）
- `docs/06-verification.md` 若涉及对拍则追加

❌ 不许改 `src/`、`client/`、`dispatch/`、既有测试

## 5. 验收

```text
① Location 三元组数：Win / Android / Linux 各多少；三方交集多少（同版本应 ≈2,035）
② 名字落地：唯一锚 **326 → ?**（给确切数）；Windows 独有 206 个落了多少
③ 冲突清单（同 Location 不同名）条数 + 例
④ 天花板之外的**上限估计**，且匹配器在"已知真值"上的准确率（正对照）
⑤ cargo test --offline 全绿（基线 **1559**）；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤10 行）

分支+sha、三方 Location 数与交集、**唯一锚 326 → ?**、206 个独有名落地数、
冲突条数、匹配器正对照准确率、有没有"版本不一致"的证据、`cargo` 条数。