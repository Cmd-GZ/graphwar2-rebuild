# 88 · `agent/functionpoints-rescan`：用**修好的** tag 工具复扫 `FunctionPoints` 构造点（**依赖关系**）

**优先级 P0 —— 87 号的负结论是拿旧工具得出的，83 修好第四形态后可能翻转。**

## 0. 为什么这是第一件事

```
87 号用（当时的）三形态扫 `FunctionPoints` 的构造点 ⇒ **0 处**
但 83 号证明：第四形态（materialize）以前全被记进 tag 0，修正后**换桶 129 处**
⇒ 那 0 处**很可能**有若干个其实是第四形态的站点
⇒ **87 的"构造点扫不到"这条负结论，必须用新工具复扫后才算数**
```

## 1. 要做什么

1. **复扫构造点**：用修好的 `tools/addr_probe.py tags`（第四形态已认），把 `FunctionPoints` 的
   构造/写入点重新扫一遍（四份构建都给数）；
2. **对每一处新命中**：判断它是不是"真的构造点"（不是 ref 也不是 materialize 的中间值），
   给出**指令级证据**（哪条 `mov`/`lea` 往哪个偏移写多少个字段）；
3. **明确写结论**：
   ```
   · 若扫到了 ⇒ 87 的负结论**翻转**，并写清"原结论错在哪、新证据是什么"
   · 若仍为 0 ⇒ 87 的负结论**升级为硬结论**（"用修正后的工具仍扫不到"）
   ```
4. 两种结果都要回写：`docs/06` §47（87 那节）加一段"83 修复后复扫结论"，
   并把旧结论**标成"基于旧工具"**（不许直接删，否则看不出是被推翻的）。

## 2. 硬约束

- 复扫**只改该节结论**，不许批量改别处 tag 数（那是 83 号的范围）；
- 数字不许退：`cargo test --offline` **1753**、三轴 `fire 647 / batch 647 / batch_conv 637`、
  headless **1923**（14 个 SceneTree 脚本）、`selftest` **716**（`63x/7` 是既有 flake，重跑再判）；
- headless 只跑 `extends SceneTree` 的脚本；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/functionpoints-rescan`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 四份构建：`<WS>/.reference/steam/{2026-08-28, android-2.0.60, windows-2.0.60, windows-early2026}`
- 工具：`tools/addr_probe.py`（83 修好的那份）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/functionpoints-rescan`

## 4. 验收

```text
① 四份构建各自的构造点命中数（修前 0 / 修后 ?）
② 每处新命中的指令级证据
③ 结论：翻转 / 升级为硬结论（二选一，写清）
④ docs/06 §47 的补充段（旧结论要标"基于旧工具"）
⑤ cargo/三轴/headless/selftest 是否退
```

## 5. 汇报（≤8 行）

分支+sha、四份构建命中数、最硬的一处证据、结论（翻转/硬化）、文档改在哪、三轴是否退。
