# 53 · `agent/corpus-sweep`：把"假设空间"整片扫一遍（**本该吃满 16 核的活**）

**优先级 P1，但它是目前**最能吃满 CPU** 的一类任务： embarrassingly parallel（按回放文件分片）。**

## 0. 为什么派这个（也是对"下游 CPU 只跑 20%"的回应）

现在的活大多是**单线程**：一次 `cargo run --example replay_attrib` 走完 1080 批、
一个 frida 进程挂一局、一个 unicorn worker 跑一条式子。
⇒ 再重的任务也只占 1–2 个核，**16C32T 的机器只用到 20% 是正常的**。

这类活天然可并行：**每个回放文件独立**。把 401 份切成 N 片、N 个进程同时跑，
就能真正吃满。

## 1. 要做什么

对 **401 份回放**（`<WS>/.agent-work/replay-snapshot/`）整片扫**一个假设空间**，
回答："**有没有任何一种解释，能把那 433 批失败里的某类变成命中？**"

候选假设（每个都要独立实现成可切换的开关，不要写死）：

```
H1  `end_x` = pts[len-1] / pts[len-2] / 首个越界点 / 插值到边界
H2  `pos`   = 同 H1 的四种，且**与 end_x 独立选择**（16 种组合）
H3  修正常数 = 3 位小数（现行）/ 全精度 / 截断
H4  采样终止 = 越界即停 / 越界后再走一步 / 停在最后一个 inside 点
H5  `DiffEq` 的积分步长 = 0.01 / 0.005 / 自适应
```

跑法：

```
① 401 份按文件分片（例如 32 片）→ 每片一个进程 → 32 个进程并行
② 每片内对每个假设组合跑一遍（组合数 = 4×4×3×3×3 太多 ⇒ 先做单变量：每次只换一个假设）
③ 汇总：每种假设下"内容命中数"（0..1080）与"相对 647 的增减"
```

**判据**：任何假设只要让命中数**稳定超过 647**，就是新线索 → 立刻深挖；
若全部 ≤647，**如实写"假设空间已扫，没有更好的解释"** —— 这也是硬结论。

## 2. 硬约束

- **必须并行**：任务书要求 **墙钟时间 ≤ 单进程串行的 1/8**，并在汇报里给出 `time` 实测与 `nproc`；
- **不许改 `src/`**（这是"试试别的解释"，不是"改实现"）；用 `examples/` 里加开关或独立脚本；
- 数字不许退：`cargo test --offline` **1638**、三轴 `fire 647/1075`、`batch 647/1075`；
- 提交信息用 `-F 文件`；**别把大 JSONL 入库**（只入汇总表）。

## 3. 环境

- 分支 `agent/corpus-sweep`
- **私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 快照：`<WS>/.agent-work/replay-snapshot/`（401 份，只读）
- 现成：`examples/replay_attrib.rs`（`--drive fire|batch|conv`、`--policy default|strict`）、
  `tools/bit_exact_attrib.py`（分桶）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/corpus-sweep`

## 4. 可以改的文件

- `examples/`（只加不改：可加 `--hypothesis` 开关）、`tools/`、`fixtures/`（**只入汇总表**）
- `docs/06-verification.md` 追加一节
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 并行方案（几片、几个进程）、墙钟时间 vs 单进程串行（给数）
② 每种假设的命中数（单变量轮换）
③ 有没有超过 647 的（有→哪一条；没有→如实写"扫过，没有更好解释"）
④ 汇总表入库位置
⑤ cargo test 1638；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、并行方案与墙钟/加速比、各假设命中数、有无超过 647、汇总表位置、`cargo` 结果。
