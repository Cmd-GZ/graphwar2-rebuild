# 65 · `agent/settle-order`：**报文顺序 ≠ 结算顺序** —— 落地"按到达序解算 + 挖洞"

**优先级 P0。55 号指出的真机制，但那条改法**没人验证过**。**

## 0. 55 号的发现（先读，这是本任务的依据）

```
重叠假设**被否**（89 发 cross，检测器自测 79/79 能抓到）
两个"大差"的真因：**同批更早爆炸的那几发已经挖了洞**
  ⇒ 把它们的洞加进掩膜后，我方停点**逐位等于**回放
  ⇒ 但按 `FunctionPoints` 的顺序只对得上 **61/63**
⇒ **报文顺序 ≠ 结算顺序**：`fire_functions` 应当**按到达序**逐发解算并挖洞
```

⚠️ 报告 §7 明确写着：这条改法**没有人验证过**，要跑三轴。

## 1. 要做什么

1. **先复现 55 号的结论**：在它给的那批样本上，按"到达序"重算，确认 **63/63**（或给出真实数）；
2. **改 `fire_functions`（或等价处）**：批内逐发按**到达序**解算，且**每解完一发就把它的洞写进掩膜**
   供后续发使用（这正是"逐发结算"的语义）；
3. **全量回归**（这一步是本任务的重头，**必须三轴分开报**）：
   ```
   fire 647/1075 ？ batch 647/1075 ？ batch_conv 637/1075 ？
   死亡 366/385 ？ 挖洞 207/294 ？
   ```
   **任何一项下降就回退并说明为什么**（很可能"到达序"只在同时回合成立，
   顺序回合里 `FunctionPoints` 顺序 == 到达序，所以三种轴表现不同）；
4. 若成立：把"批内顺序语义"写进 `docs/05`（shot pipeline）与 `docs/15`。

## 2. 判据（写死，别事后改口径）

```
成立 = fire/batch 轴**不降**，且同时回合那批样本从 61/63 → ≥62/63
部分 = 只改善同时回合、但某条轴降了 ⇒ 必须给"分类开关"（同时回合按到达序、顺序回合按原序）
失败 = 复现不出 55 号的效果 ⇒ 如实写，并说明与它差在哪
```

## 3. 纪律

- **三轴必须分开跑、分开报**（`AGENTS.md` 合并纪律 C：只给数不给轴等于没报）；
- 改了结算路径就**必须**重跑三轴 + headless + `cargo test`；
- 提交信息用 `-F 文件`。

## 4. 环境

- 分支 `agent/settle-order`
- **私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 快照：`<WS>/.agent-work/replay-snapshot/`（**401 份冻结**，别加文件）
- 现成：`examples/replay_attrib.rs`、`tools/bit_exact_attrib.py --check-baseline --axis {fire,batch}`
- 参考：`docs/06` §34（55 号那节）、`docs/05-shot-pipeline.md`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/settle-order`

## 5. 可以改的文件

- `src/game.rs`（`fire_functions` / 批内解算那一段）、`src/graphwar_shot_calculator.rs`（挖洞掩膜接口）
- `docs/05` / `docs/06` / `docs/15`
- ❌ 不许改 `client/`、`dispatch/`

## 6. 验收

```text
① 复现 55 号：同一批样本按到达序后 61/63 → ?
② 改了什么（一段话）
③ **三轴各自的 before/after**（fire / batch / batch_conv）+ 死亡/挖洞
④ headless 与 cargo test（1671）有没有退
⑤ 判决：成立 / 部分 / 失败
```

## 7. 汇报（≤10 行）

分支+sha、复现结果、改法、三轴 before/after、死亡与挖洞、判决、`cargo` 结果。
