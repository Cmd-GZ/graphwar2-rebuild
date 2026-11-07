# 20 · `agent/point-array-filler`：追点数组的**填充者**（两条悬案的共同入口）

**优先级 P0。** 第三轮报告 §8 第 1 条；它是 **1 ulp 残差**与 **"同坐标不同序号"** 两条悬案的共同入口。

## 为什么是它

第三轮把求值器逐位排除了（`eval_differ = 0`，1287/1287 相同），
所以那 154 批 1 ulp 的差**不在求值**，而很可能在**"谁把 y 写进点数组"**这一步：

```text
docs/06 §32.5 指出：点数组在 [x19+0x30 .. x19+0x40]
上游 arm64 execute_results（0x4a8558）里只找到 mem::take 式的"清空写"，
**没有找到填充者**
```

同时"一次挖洞发 2 条、坐标相同但序号不同"也指向同一处（点表 + 序号）。

## 要做什么

1. **定位填充者**：谁往那个 `Vec<(f64,f64)>` 里 `push`/`extend`？
   建议路径（按可靠度）：
   - arm64 `0x4a8558` 附近，找对 `[x19+0x30/+0x38/+0x40]`（ptr/len/cap 三连）的写入；
   - 从采样器 `0x3c2530`（arm64 `0x3b5374`）的**输出**反查：它的返回点序列被谁接走；
   - x86 侧对应函数用同址映射 + 指令形态确认。
2. **回答两件事**（这是交付的核心）：
   - **1 ulp**：写进点数组的 `y` 与"采样器返回值"是否**逐位相同**？若不同，差在哪一步（取整/回代/类型转换）？
   - **同坐标不同序号**：一次挖洞两条报文的点坐标相同但序号不同 —— 序号是什么、为什么同点两号。
3. 若能据此修正实现，落地并量化；不能则**写清缺什么证据**。

## 已知（别重推）

- 求值器：`0x4eb9d0`（x86）/ arm64 对应；**已逐位排除**（`eval_differ = 0`）
- 采样器：`0x3c2530`（x86）= `GraphwarShotCalculator::calculate_function`（arm `0x3b5374`），
  **零 `call rel32`**，唯一入口是函数指针槽 `0x9bac98`（2 个使用者：`0x3c30c1`、`0x3e2d35`）
- `execute_results`：arm64 **`0x4a8558`**，体 **3712 B** ⇒ 到 `0x4a93d8`
- 两处 `add_effect`：`0x4a89ec`（恒 ExplosionEffect，`pos` 取 **点数组末元素** `0x4a891c ldp`）、
  `0x4a8cb0`（`pos` 来自 `0x3b7f58` 返回的 `Option<(f64,f64)>`）

## 环境

- 克隆到 `<WS>/.agent-work/pointfill/repo`，分支 `agent/point-array-filler`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/pointfill/target`
- x86 目标 `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`
- arm64 未 strip `<WS>/.reference/steam/android-2.0.60/libgraphwarrust.so`
- 快照 `<WS>/.agent-work/replay-snapshot/`
- 工具：`tools/addr_probe.py`（fde/dis/callers/locs/jt，带缓存）、`tools/loc_xrefs.py`、
  `tools/posy_attrib.py`（1 ulp 分布）、`examples/posy_probe.rs`
- python+capstone：`<WS>/.venv/bin/python`
- ⚠️ **函数指针三种 materialize 形态**（上一轮踩过）：`movabs` 直写 / `i64::MAX+add` / `i64::MAX+inc`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/point-array-filler`

## 可以改的文件

- `tools/`、`examples/`（只加不改）、`tests/`
- `src/game.rs` 或 `src/_scaffold/diffconv.rs`（**只在证据指向时**，改动最小）
- `docs/06-verification.md` 追加一节

❌ 不许碰 `*_class.rs`、`client/`、`dispatch/`

## 验收

```text
① 填充者是谁（函数 + 地址 + 指令），或明确的"没找到 + 缺什么"
② 1 ulp 的差发生在哪一步（逐位对照表），或"不在这一步"的反证
③ "同坐标不同序号"的成因（一句 + 证据）
若改了实现：三轴护栏一个都不许退（见 dispatch/README.md 的表）
cargo test --offline 全绿；coverage --check OK
```

## 汇报（≤8 行）

分支+sha、填充者（有/无 + 地址）、①②③ 各一句、有没有改实现、
三轴数字 before/after、`cargo`/`--check` 结果。
