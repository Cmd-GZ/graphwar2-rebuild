# 30 · `agent/unnamed-176`：给"有 Location 但无名"的 176 个函数再想办法

**优先级 P1。** 目前 `527` 个可归属函数里 **351 个有真名（66.6%）**，剩 **176 个**两边的符号表都没有名字。

## 0. 现状（已确证）

```
可归属函数          527        （有 panic::Location 引用，这是名字的绝对天花板）
已有真名            351        （Android 524 ∪ Windows 939，交集 307）
仍无名字            176
符号表还能救吗      不能 —— 三份构建（Android/Win2.0.60/WinEarly）都没有它们的符号
```

天花板说明（别搞错）：`Location` 有 2,035 个，但只被 **527 个函数**引用（大函数平均带 3.9 个）；
`13,263` 是函数总数，其中 12,736 个**没有 Location**，符号表对它们无解。

## 1. 目标：不要名字，先要**归属**

对这 176 个，能给出下列任一结论就是成果：

```
(a) 它属于哪个模块（file 已由 Location 给出 ⇒ 模块基本已知，只差函数名）
(b) 它对应另一个构建里的哪个函数（同名 ≠ 同函数，要靠结构）
(c) 它在原版里的角色（例如"clamp 到边界""写报文"），用行为描述代替名字
```

## 2. 方法（按可靠度，逐条给证据档）

1. **锚点邻域推理**：`Location` 能定位到函数，那么该函数的**调用者/被调者**里
   大概率有已定名的函数 ⇒ 用调用关系 + 常量集 + 字符串引用做**候选排序**，
   输出"最可能的角色"，并**明确标推断**。
2. **跨构建结构匹配**（`tools/three_way_match.py` 已有）：
   对每个无名函数，在 Windows/Android 侧找 `size 比 + 被调者多重集 + 常量集` 最接近的，
   **必须给正对照**（拿已知名字的函数验证你的匹配器准确率，报告准确率）。
3. **字符串/常量交叉**：函数体里引用的字符串（如事件名、路径）常常直接透露角色。
4. **判据不许是"名字像"**：这是上一轮 `prefix-fix` 的教训——判据要**结构可证**。

## 3. 交付形式

新增 `docs/` 里的一份表（追加到既有文件，**不新建文件**），每行：

```
linux_addr | 模块（from Location） | 候选角色 | 证据（调用者/常量/字符串/匹配器分数） | 档位（推断/未知）
```

并给出**汇总**：176 个里，"有合理候选"多少、"只能写未知"多少。

⚠️ **不要**把推断写成结论：`AGENTS.md` §0.3 —— 标 `推断` 的必须写清缺什么证据。

## 4. 环境

- 克隆到 `<WS>/.agent-work/unnamed/repo`，分支 `agent/unnamed-176`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/unnamed/target`
- Linux 目标 `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`
- Android `<WS>/.reference/steam/android-2.0.60/libgraphwarrust.so`
- Windows `<WS>/.reference/steam/windows-2.0.60/graphwarrust.dll`、`windows-early2026/graphwarrust.dll`
- 工具 `tools/three_way_match.py`、`tools/rust_sym.py`、`tools/name_accuracy_check.py`、`tools/loc_xrefs.py`
- 夹具 `fixtures/name_to_addr.tsv`（743 行）、`fixtures/name_to_addr_v2.tsv`（980 行）、
  `fixtures/windows_name_to_addr.tsv`、`fixtures/name_crate_audit.tsv`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/unnamed-176`
- 逐文件 `git add`，**禁止 `git add -A`**

## 5. 可以改的文件

- `tools/`、`fixtures/`（新增，不改既有格式）
- `docs/02-module-map.md` 追加一节
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 6. 验收

```text
① 176 个的清单（模块分布）
② "有合理候选"多少 / "只能未知"多少
③ 匹配器的**正对照准确率**（拿已知名字的样本验）
④ 每条候选的证据（调用者/常量/字符串），档位明确
⑤ cargo test --offline 全绿（1569）；coverage --check OK；check_doc_refs rc=0
```

## 7. 汇报（≤8 行）

分支+sha、176 个的模块分布、有候选/未知各多少、正对照准确率、
新增夹具与文档位置、`cargo`/`--check` 结果。