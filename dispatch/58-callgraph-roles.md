# 58 · `agent/callgraph-roles`：用**调用图**给无名字函数反推"角色"

**优先级 P1。这是"源码还原"分子还能往上走的第二条路。**

## 0. 想法

```
12,736 个无 Location 函数拿不到名字
但它们**有调用关系**：被谁调、调了谁、被哪些已定名函数调用
⇒ 用"调用图位置"反推**角色**（不是名字），例如：
   "只被 GraphwarRootNode::ready 调用、且内部调 draw_arc 三次" ⇒ "ready 里的绘制闭包"
```

## 1. 要做什么

1. **建调用图**：用 `analysis/funcstarts_ghidra_va.pkl`（**别用**错的 `funcstarts.pkl`）
   + 直接调用（`call rel32`）+ **函数指针派发**（`tools/ptr_slots.py` 的槽机制；
   ⚠️ 只看 `rel32` 会漏掉大量调用）；
2. **给每个无名函数算"角色特征"**：
   ```
   被哪些**已定名**函数调用（多重集）、调用哪些**已定名**函数、
   引用了哪些字符串（真内容，不是文件名）、调了哪些引擎 API（draw_*/set_* 等）
   ```
3. **输出角色标签**（不是名字）：例如
   `角色 = 「被 X::ready 调用的闭包，含 3 次 draw_arc」`，并标**档位**（观测/推断/未知）；
4. **正对照**：拿 348 个已定名函数验证"角色描述是否与真名相符"（例如真名含 `ready`
   的函数，其角色描述里应当出现"被 ready 调用"或"在 ready 里"）。

## 2. 硬约束

- **不许把角色写成名字**（"这是 `X::ready` 的闭包"可以；"这个函数叫 `X::ready::{{closure}}`"不行 —— 那需要证据）；
- 每条角色给证据（哪个调用者、哪个常量、哪次派发）；
- 数字不许退：`cargo test --offline` 1659、三轴 647/1075；`check_doc_refs` rc=0；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/callgraph-roles`
- 四份构建 + 函数起点表 + `tools/{ptr_slots,callers,addr_probe}.py`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/callgraph-roles`

## 4. 可以改的文件

- `tools/`、`fixtures/`（角色表，**文本**）
- `docs/02-module-map.md` 追加一节
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 调用图规模（节点/边；直接调用 vs 指针派发各多少）
② 给了多少个无名函数"角色描述"（分档：观测/推断/未知）
③ 正对照：348 个已定名函数里，角色描述与真名相符的占比
④ 产物位置与列
⑤ cargo test 1659；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、图规模、角色覆盖数、正对照相符率、产物位置、`cargo` 结果。
