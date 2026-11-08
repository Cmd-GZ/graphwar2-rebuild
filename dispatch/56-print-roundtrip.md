# 56 · `agent/print-roundtrip`：盯「**打印-回代链**」—— Y(189) + X(91) 桶的正确下一步

**优先级 P0。第十一轮报告 §10 第 3 条 —— 53 号明确指了方向。**

## 0. 为什么是这条（53 号关掉了一整族之后的唯一方向）

```
53 号扫了 29 档（取哪个点 / 精度 / 步长）⇒ **没有一档超过 647，fix 全 0**
并且给了量级论证：433 批失败里 94% 是 ≤0.02（多数 **≤1e-9**）的末位差
而"换个点"差一步 ≈ 0.01、精度 1e-6~1e-3、步长 1e-2 ⇒ **量级不匹配**

⇒ 剩下的方向不是"取哪个值"，而是**值怎么被印出来又读回去**：
   `corrected_function` 里那个 **3 位小数** 的常数 ↔ 内部**全精度**常数
```

## 1. 要做什么

1. **把"打印→回代"这条链完整画出来**（逐环节地址）：
   ```
   内部 c（全精度 f64） → 格式化 "{:.3}" / " + 1.197" → 存进发送缓冲
   → 回放/报文里那一行文本 → 重新 parse 成 token → 采样器用的 c'
   ```
   要回答：**`c'` 与 `c` 差多少**（逐位），以及**哪一处决定了这个差**；
2. **在 Y(189) 与 X(91) 桶上做定量**：对每批算
   ```
   c_全精度 vs c_3位小数 → 采样出来的 end_y / end_x 差多少 bits
   与回放的差是否**完全一致**
   ```
   若一致 ⇒ 这批的差就是"3 位小数"造成的，**不是我们的错**，而且能写成结论；
3. **若发现某一批连"3 位小数解释"都对不上** ⇒ 那就是新的真问题，深挖。

## 2. 判据

- 逐位（`to_bits`），不是"接近"；
- **正对照**：拿内容命中的 647 批（其中也有带修正量的）跑同一套，应当**全部一致**；
- 结论要么"这批的差 = 3 位小数造成的"（并给占比），要么"还有 X 批解释不了"。

## 3. 环境

- 分支 `agent/print-roundtrip`
- **私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 快照 `replay-snapshot/`（401 份）
- 现成：`tools/bit_exact_attrib.py`、`examples/replay_attrib.rs`、`tools/nan_c_probe.py`（unicorn 直调）
- 已知事实：真机用 `round(c,3)`（`graphwar_curve.correction_used()`），不是精确值
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/print-roundtrip`

## 4. 可以改的文件

- `tools/`、`examples/`（只加不改）、`tests/`
- 若定位到我方真错：`src/game.rs` 或 `src/math_function.rs` 的**最小一段**
- `docs/06-verification.md` 追加
- ❌ 不许改 `client/`、`dispatch/`

## 5. 验收

```text
① 打印→回代链的逐环节地址
② Y / X 桶里"能由 3 位小数解释"的批数与占比（逐位判据）
③ 正对照 647 批的结果
④ 解释不了的批数（若有，列清单）
⑤ cargo test 1659；三轴不许退；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、链条地址、Y/X 可解释占比、正对照结果、解释不了的批数、三轴有没有退、`cargo` 结果。
