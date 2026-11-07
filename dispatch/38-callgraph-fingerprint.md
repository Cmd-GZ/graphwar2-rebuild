# 38 · `agent/callgraph-fingerprint`：给**无 Location 的 12,736 个函数**做身份指纹

**优先级 P2，但是"天花板之外"唯一还能推进的方向。** 算力活，适合你这台机器。

## 0. 为什么做这个

```
目标 .so 里 13,263 个函数
  527 个有 panic::Location ⇒ 名字/归属已到顶（348 有真名，179 无名）
  12,736 个**没有 Location** ⇒ 任何符号表都定不了名（三份符号表都没它们）
```

⇒ 对这 12,736 个，**名字要不回来**，但可以给出：

```
(a) 跨构建的**同源证据**（这个 Linux 函数 = Windows/Android 的哪个函数）
(b) 结构角色（被谁调 / 调谁 / 引用了哪些字符串与常量）
(c) 是否为编译器产物（drop glue / trampoline / 单态化副本）的可判据
```

**目标不是命名，而是把 12,736 个从"未知"降级为"有结构身份"。**

## 1. 要做什么

1. **建指纹库**：对四份构建（Linux 目标 / Android 2.0.60 / Windows 2.0.60 / Windows early2026）
   各算一遍每个函数的：
   ```
   size、指令直方图（按 mnemonic 归类）、常量
   被调者多重集（含 ICF 归并后的规范化）、调用者多重集、引用字符串集合
   ```
2. **跨构建匹配**（**不要用名字，名字只用来做正对照**）：
   - 同 ISA（Linux ↔ Windows）：size 比 + 直方图余弦 + 常量集；
   - 跨 ISA（Linux ↔ Android）：size 比区间放宽 + 被调者多重集（**别用偏移/立即数**，那跨架构不同）；
   - ⚠️ 已知教训：**"形状相同 = ICF"不成立**（338 条形似孪生的函数逐字节全不同）；
   - ⚠️ 已知教训：**WL 调用图指纹在这批数据上贡献 0**（上一轮实测），别把宝押在它上面。
3. **正对照（必须）**：拿**已定名的 348 个**当测试集，量化你的匹配器
   top-1 / top-5 准确率。**准确率不过 80% 就不许把结果写进文档当结论。**
4. **产出**：`fixtures/callgraph_fingerprint.tsv`（每个函数一行：地址 / size / 直方图 hash /
   被调者数 / 匹配到的构建+地址 / 分数 / 档位）；文档追加一节（`docs/02`）。

## 2. 诚实边界（写进文档）

- 这**不会**给出名字；它给的是"同源候选 + 结构角色"；
- 匹配分数低的一律标 `未知`，**不许为了覆盖率把弱证据写成推断**；
- 12,736 是一大批，**允许分批**（先做 2,000 个，跑通再扩），但要在汇报里说清做了多少。

## 3. 环境

- 克隆到 `<WS>/.agent-work/fp/repo`，分支 `agent/callgraph-fingerprint`
- **私有** `CARGO_HOME=<WS>/.cargo-home`、`CARGO_TARGET_DIR=<WS>/.agent-work/fp/target`
- 四份构建：Linux `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`、
  Android `<WS>/.reference/steam/android-2.0.60/libgraphwarrust.so`、
  Windows `<WS>/.reference/steam/windows-2.0.60/graphwarrust.dll`、
  WinEarly `<WS>/.reference/steam/windows-early2026/graphwarrust.dll`
- 函数起点表：`analysis/funcstarts_ghidra_va.pkl`（**值可能是字符串 hex**）；
  ⚠️ **别用**旧表 `funcstarts.pkl`（已知错，与 Ghidra 只交 2 个）
- 既有工具可复用：`tools/three_way_match.py`、`tools/sym_match.py`、`tools/rust_sym.py`
- python+capstone `<WS>/.venv/bin/python`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/callgraph-fingerprint`
- 逐文件 `git add`，**禁止 `git add -A`**

## 4. 可以改的文件

- `tools/`、`fixtures/`（新增）
- `docs/02-module-map.md` 追加一节
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 指纹库覆盖了多少函数（四份构建各多少）
② 正对照 top-1 / top-5 准确率（348 个已定名样本）
③ 12,736 里做了多少（分批要说清），其中"高置信同源"多少 / "只能未知"多少
④ 新增夹具的列与行数
⑤ cargo test --offline 全绿（基线 1620）；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤10 行）

分支+sha、指纹库规模、**正对照准确率**、做了多少个、高置信/未知各多少、
夹具与文档位置、`cargo` 结果。
