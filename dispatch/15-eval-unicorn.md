# 15 · `agent/eval-unicorn`：用 unicorn 对拍 `evaluate_recursive`，解掉最后 1 ulp

**优先级 P0。第二轮报告 §8 的第 1 条**，也是唯一"工具现成、只差执行"的缺口。

## 为什么是它

第二轮把内容命中从 21 推到 **324/1080**，剩下的残差几乎全是**同一个 1 ulp 问题**：

```text
09（爆炸点 y）：154 批真末位差，≤4 ulp 占 96.1%
11（end_x）  ：负结果 —— 不是"差一个采样点"，仍在点序列的最后一轮舍入
⇒ 两者同源：**求值器 `evaluate_recursive` 的最后一次舍入与真二进制不同**
```

上游有现成工具：`analysis/emu.py`（unicorn）能**逐条执行真二进制的指令**。
拿它当 oracle，把 `f64` 的输入→输出对照我们的实现，能一次定位到**哪一步舍入不同**。

## 已知（别重推）

- 求值器：`0x4eb9d0`（x86，**零 Location 锚**）；解析器 `0x4ed670`
- 已排除的解释（都有数据）：取错 x（±2000 ulp 内任何 x 都算不出 obs）、
  相邻采点（0/154）、常数回代顺序（IEEE 下四种结合逐位相同，且 Sterbenz 反证）
- **16 批是纯序列化格式差**（`0` vs `0.0`、`2.638231208698597e-62`）：回放是 serde_json/ryu 语义，
  我方 `emit_*` 用 `format!("{}")`。**这 16 批与本任务无关，但可以顺手一起修**（见"顺带"）

## 要做什么

1. **搭那个对照**：给定 `(函数文本, x)`，一边跑我们的 `math_function` 求值，
   一边用 `analysis/emu.py` 在真二进制上跑同一个求值，**逐位比较返回值**；
2. 从 154 批里挑 **≥5 个能复现 1 ulp 差的最小输入**（例如 `1782927404` 的 `fn="x"` 那类），
   逐步缩小到"哪一次乘法/加法/取整不同"；
3. 定位后落地修正 `src/math_function/**`，并**回归全部既有逐位测试**；
4. **顺带**（可另提交）：把 `src/_scaffold/protocol.rs` 的 `emit_*` f64 打印改成 **ryu 语义**
   —— 那 16 批会直接归零，且与会话几何无关。

## 环境

- 克隆到 `<WS>/.agent-work/eval-unicorn/repo`，分支 `agent/eval-unicorn`
- **私有** `export CARGO_TARGET_DIR=<WS>/.agent-work/eval-unicorn/target`（并行会串味）
- `CARGO_HOME=<WS>/.cargo-home`；python+unicorn：`<WS>/.venv/bin/python`
- 上游工具：`<WS>/analysis/emu.py`、`difftest.py`（老仓库 `Cmd-GZ/graphwar2-analysis`）
- 快照：`<WS>/.agent-work/replay-snapshot/`（401 份，**定格**）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/eval-unicorn`

## 可以改的文件

- `src/math_function.rs` 与 `src/math_function/**`
- `src/_scaffold/protocol.rs`（**只**改 f64 打印）
- `tests/`、`examples/`、`tools/`
- `docs/06-verification.md` 追加一节

❌ 不许碰 `*_class.rs`、`client/`、`src/game.rs` 的几何。

## 验收

```text
最小复现：给出 ≥1 个 (表达式, x) → 我方 y 与真二进制 y 的 16 进制位对照（修前/修后）
同一份快照：内容命中 324 → ？（给出新数）；种类序列不许退
16 批序列化格式差：修前/修后各多少
cargo test --offline 全绿（基线 1519）
```

## 汇报（≤8 行）

分支+sha、最小复现是什么、**哪一步舍入不同（地址级）**、改了什么、
内容命中 before/after、16 批格式差是否归零、`cargo` 条数。
