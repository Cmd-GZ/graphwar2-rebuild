# 80 · `agent/drive-frame-cadence`：给 `drive_frame` 量一份**节奏真值**

**优先级 P1（Ｄ 档：二进制里有代码，"多久一次"要看现场）。**

## 0. 为什么现在能做

```
43 号已定位 `GraphwarGameClient::process` = x86 **0x518400**，并给出"60 Hz 定步、**先 tick 后泵**、delta 不读"
47 号把 `ProcessLoop` + `KernelBus::drive_frame` 落进内核（`process_loop_check` 49 条）
67 号**故意没把 `drive_frame` 接进播放屏**，理由是"没有真帧可对、会静默改 EntityDrawer 的 V"
```

⇒ 缺的不是代码，是**"原版每秒到底调几次、顺序如何"这个真值**。而它可以**直接量**，
不需要视频帧——真值是**调用序列**，不是画面。

## 1. 要做什么

1. 在真游戏里下钩（frida）：
   ```
   · `0x518400`（process）入口：递增计数 + 记 delta 参数
   · 泵函数（事件派发那个；43 号报告里有地址）：计数 + 记调用顺序相对 process
   · 可选：`GraphwarGameClient` 的其它每帧入口
   ```
2. 跑 **≥60 秒**，给出：
   ```
   · process 调用总数 / 实测频率（Hz，含抖动分布）
   · 每次 process 内泵被调几次（应为 0 或 1；若不是 1，说明"追赶"确实存在）
   · 顺序：tick 在前还是泵在前（用时间戳或调用序判定）
   · delta 的实际取值分布（是否恒定 1/60）
   ```
3. **对照我方**：把 `ProcessLoop` 的 headless 计数换成同一口径，逐项对上（频率/顺序/追赶行为）；
4. 结论里明确：**"真帧"对这件事是不必要的**（画面只用于显示验证，不是节奏真值）。

## 2. 判据

- 每条结论要么指运行时观测，要么指指令；
- **正对照**：先在一个已知会调用的函数上下钩，证明钩子链路有效（避免"0 次调用"是钩错了）；
- 抓不到就如实报"卡在哪"。

## 3. 硬约束

- 样本/日志放 `.agent-work/`，**不进冻结快照**、不入库大文件；
- 数字不许退：`cargo test --offline` 1710、三轴 647/647/637；headless 1827 / 0 failed；
- 若顺手把 `drive_frame` 接进播放屏：**必须**同时说明"会不会静默改 `EntityDrawer` 的 `V`"，
  并给出 headless 断言；没把握就**只做测量、不动接线**；
- 提交信息用 `-F 文件`。

## 4. 环境

```
<WS>/.reference/game/ 三件套（sha 9a538115…）｜ frida 17.26.1（<WS>/.venv）
Xvfb（TCP 监听，docs/06 §35.1 的三个坑）
LAN+Bot 起局配方见 69 号（一条命令能起一局并 attach）
```

- 分支 `agent/drive-frame-cadence`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 工具：`tools/frida_point_array.*`（现成的 frida 骨架）、`examples/process_loop*` 相关
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/drive-frame-cadence`

## 5. 可以改的文件

- `tools/`（frida 脚本）、`docs/06-verification.md`（追加观测节）
- 若做接线：`src/_scaffold/root_wiring.rs` + `client/tools/process_loop_check.gd`
- ❌ 不许改 `src/game.rs`、`client/src/**`、`dispatch/`

## 6. 验收

```text
① 钩子正对照（证明链路有效）
② 实测频率 / 每次 process 内泵的次数 / 顺序 / delta 分布（给数 + 抖动）
③ 与 ProcessLoop 的对齐结果（哪些项一致、哪些不一致）
④ 有没有动接线（动了给断言；没动说明为什么）
⑤ cargo/三轴/headless 是否退
```

## 7. 汇报（≤8 行）

分支+sha、正对照、频率与顺序的实测数、与 ProcessLoop 的对齐、是否动接线、三轴是否退。
