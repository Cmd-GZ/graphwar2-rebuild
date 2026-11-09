# 83 · `agent/addrprobe-tags`：修 `addr_probe.py tags` 的**第四形态**并**全量复扫**

**优先级 P0 —— 它影响所有基于"tag 计数"的历史结论（作者自己判定应高于新功能）。**

## 0. 问题

```
第十六轮报告 §9 第 1 条：`addr_probe.py tags` 少认了**第四种**形态（已登记，未修）
原因：修工具要动 `tools/`，当时不在若干任务的允许目录里
影响：凡是引用过"某 tag 出现在 N 处"的结论，都要按新口径复核
```

## 1. 要做什么

1. **先复现盲区**：找一个**已知属于第四形态**的站点（第四形态具体是什么，先自己从
   `tools/addr_probe.py` 的现实现与 `docs/` 里的相关结论反推，**并在报告里写清定义**），
   证明当前版本**扫不到它**（这就是正对照）；
2. **修**：把第四形态纳入，且**不许把前三形态的数量改掉**（给出修前/修后每个形态的计数差）；
3. **全量复扫**：对四份构建重跑 tag 普查，输出对比表
   ```
   形态 | 修前计数 | 修后计数 | 差 | 差落在哪些函数（Top 10）
   ```
4. **复核历史结论**：`grep` 出所有引用"tag 计数"的文档断言（例如"X 全仓唯一""共 N 处"），
   逐条判：仍然成立 / 要改（给出新数）/ 不再成立。**列成表，逐条给结论。**
5. **给出新口径的一行说明**，写进 `docs/`（供以后引用）。

## 2. 硬约束

- 工具改正后，**既有结论不许无声变动** —— 每条变动都要在上表里点名；
- 数字不许退：`cargo test --offline` **1753**、三轴 `fire 647 / batch 647 / batch_conv 637`；
- headless **只跑 `extends SceneTree` 的脚本**（14 个；`assemble 241`、`play_flow 183`），
  拿 helper 喂 `--script` 会弹 ALERT 并留实例；
- `selftest` 有两种已知降级（见 `docs/18` §10.3.3）：`629/7` 是"服务端没就绪"，先重跑再判；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/addrprobe-tags`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 四份构建：`<WS>/.reference/steam/{2026-08-28, android-2.0.60, windows-2.0.60, windows-early2026}`
- 工具：`tools/addr_probe.py`、`tools/rela_slot.py`、`tools/ptr_slots.py`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/addrprobe-tags`

## 4. 可以改的文件

- `tools/addr_probe.py`、`fixtures/`（重扫结果）
- 受影响文档（`docs/07`、`docs/02` 等，**只改数字与"是否仍成立"，不重写结论**）
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 第四形态的定义 + 复现（修前扫不到的正对照）
② 修前/修后每个形态的计数差
③ 四份构建的对比表
④ 历史结论复核表（仍成立 / 改数 / 不成立，逐条）
⑤ 新口径说明写进 docs/ 的位置
⑥ cargo 1753、三轴、headless 是否退
```

## 6. 汇报（≤10 行）

分支+sha、第四形态是什么、复现证据、修前后计数差、历史结论里**改了几条**（列出最要命的 2 条）、`cargo` 结果。
