# 29 · `agent/ode-stop-condition`：`Ode` 局的**停止判据**（不是末位差，是真几何差）

**优先级 P0。** 这是我在分桶里**亲眼看到差异**、却一直没人查的一块。

## 0. 证据（我实测的，直接用）

文件 `1782764906` 批 #0（**标称模式 `Ode`**，1 发，owner=5377）：

```
回放 end_x = -20.75531942288392
我方 stop  = -24.99281942288328      ← 我方一路走到接近边界 (-25)
回放 pos_y =  2.4950410243592103    ｜ 我方 y 与该值**相同**
回放 pos_x = -20.75531942288392
```

复现：

```bash
cargo run --release --offline -q --example replay_endx_probe -- \
  --dir /home/user/MyProject/gw2/.agent-work/replay-snapshot --file 1782764906 --batch 0 --tail 4
```

⇒ **y 是逐位相同的，x 一路走到边界**。这不是 1 ulp 问题，是**弹道什么时候停**的问题：
原版在 `x = -20.755` 就停了，我们继续走到 `-24.99`。

## 1. 要做什么

1. **先量化**：把 401 份快照里标称 `Ode` 的批**单独统计**（脚本自己写）：
   ```
   Ode 批数 / 其中内容命中 / 其中"我方 x 幅度显著大于回放"的批数
   Normal 批同样统计一次作为对照
   ```
   如果"走到边界"是普遍现象，那就是停止判据整体错，不是个别。
2. **查原版 `Ode` 的停止条件**（这是本任务的核心）：
   - 已知：`Mode::Ode` 走 RK4 积分（我方 `src/graphwar_path_calculator.rs` 已实现）；
   - **要查的是**：积分在什么条件下停？（越界 `|x|>25 / |y|>15`？步数上限？
     `y` 不连续？导数爆炸？还是"撞到东西"？）
   - 锚点：arm64 `execute_results` = `0x4a8558`；采样器 `calculate_function` = x86 `0x3c2530`
     （arm64 `0x3b5374`）；`Ode` 分支在哪、判据是什么，**逐条 objdump**。
3. **落地修正**并量化：Ode 批的内容命中 before → after；**Normal 轴一个都不许退**。

## 2. 已知（别重推）

- 任务 21 已经把 `Mode` 接进内核（`DiffEq 515 批 / 550 发从不可比 → 可比`），但**只接了模式，没验停止条件**；
- 三轴当前值：fire `608/947`、batch `643/1075`、死亡 `366/385`、挖洞 `206/293`
  （`fixtures/guardrails.json`；**注意 fire 轴口径正在变更**，见 `dispatch/README.md` 第五轮补记）；
- 求值器已逐位排除；点数组填充者 = `calculate_function`（真机 frida 确认）；
- 修正常数 = `round(y0-f(x0), 3)`。

## 3. 环境

- 克隆到 `<WS>/.agent-work/odestop/repo`，分支 `agent/ode-stop`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/odestop/target`
- x86 目标 `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`（Ghidra = 虚地址 + `0x100000`）
- arm64 未 strip `<WS>/.reference/steam/android-2.0.60/libgraphwarrust.so`
- **Windows 2.0.60（同 ISA + 939 个真名）** `<WS>/.reference/steam/windows-2.0.60/graphwarrust.dll`
  —— 查 `Ode` 相关调用关系最快（注意 COFF `Value` 是**节内偏移**，要加 `.text` 的 `VirtualAddress`）
- 工具：`tools/addr_probe.py`、`tools/loc_xrefs.py`、`examples/replay_endx_probe.rs`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/ode-stop`
- 逐文件 `git add`，**禁止 `git add -A`**

## 4. 可以改的文件

- `src/graphwar_path_calculator.rs`（Ode 分支的停止判据）
- `examples/`（只加不改）、`tests/`、`tools/`
- `docs/06-verification.md` 追加一节

❌ 不许改 `*_class.rs`、`client/`、`dispatch/`；**不许动 Normal 分支的判据**

## 5. 验收

```text
① Ode vs Normal 的"走到边界"批数（各自的数）
② 原版 Ode 的停止条件（地址 + 指令；找不到就写清缺什么）
③ 修正后：Ode 批内容命中 before→after；**Normal 不许退**；三轴数字
④ cargo test --offline 全绿（基线 1569）；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、①的两组数、②停止条件（地址/缺什么）、
Ode 命中 before→after、Normal 与其它轴有没有退、`cargo`/`--check` 结果。