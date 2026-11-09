# 79 · `agent/localplayer-gates`：G7 的"第五道门" + `Computer` 的 `state 0/1/2`（**要一次 bot 局样本**）

**优先级 P1，需要运行时（这是 Ｃ/Ｄ 档交界：要样本 + 要观测）。**

## 0. 两份遗留（都是上一轮明写的）

```
G7   `GraphwarLocalPlayerManager` 的"第五道门"：key = player_id、entry **120 B** 的语义**未复现**
     ⚠️ 报告原话："**我方行为可能与原版不同**"（74 号把前三道门 + 订阅都钉死了，第五道没有）
Computer `GraphwarComputerPlayerManager` 的 `state` 0/1/2 语义未知
     docs/161 建议"抓一次 bot 局（不阻塞升档）"
```

## 1. 要做什么

1. **先静态把能定的定掉**：把 5 条请求（50/52/54/59/66）各自的门条件从 x86 逐条读出来，
   列出"门 → 条件 → 触发时机"的表；标出**哪一道现在没有任何证据**（那就是第五道）；
2. **再抓一次 bot 局**（现有装置：69 号的 LAN+Bot 一条命令起局 + frida；样本存到
   `.agent-work/replay-*`，**不进冻结快照**），把 G7 的五道门与 `Computer.state` 在运行时打出来：
   ```
   · 每条请求发出时，五道门各自的取值（哪些是真、哪些是假）
   · Computer.state 在每次开火前后的取值（0/1/2 分别对应什么阶段）
   ```
3. **对齐**：我方实现（`src/graphwar_local_player_manager.rs`）的门条件与观测是否一致；
   不一致就改，并说明"改前会让哪条请求多发/少发"。

## 2. 判据

- 每条门的结论要么指指令、要么指运行时观测；
- **负结论要正对照**：说"这道门在样本里没触发"之前，先用已知会触发的请求验证观测链路有效；
- 若抓不到（环境问题）：如实报"卡在哪 + 缺什么"，**不许用静态推测代替**。

## 3. 硬约束

- 样本只放 `.agent-work/replay-*`，**不进冻结快照**（混进去三轴数字全变）；
- 数字不许退：`cargo test --offline` 1710、三轴 647/647/637；headless 1827 条 / 0 failed；
- `selftest` 有 flake（76 号在修）：失败先重跑一次再判；
- 提交信息用 `-F 文件`。

## 4. 环境

```
<WS>/.reference/game/  三件套（sha 9a538115…）｜ frida 17.26.1（<WS>/.venv）
LAN+Bot 起局：照 69 号的配方（Xvfb + Popen+attach，Battle → Create Game → AddBot → Start）
GOT 槽：0x9bacf0（0x3c5f70 的调用者）、0x9bac88/0x9bac98（订阅/送达）
```

- 分支 `agent/localplayer-gates`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/localplayer-gates`

## 5. 可以改的文件

- `src/graphwar_local_player_manager.rs`、`src/graphwar_computer_player_manager.rs`（只改门条件/状态机）
- `src/_scaffold/root_wiring.rs`（档位）、`client/tools/assemble_check.gd`
- `docs/161-manager-gaps.md`、`docs/06-verification.md`（追加观测节）
- ❌ 不许改 `src/game.rs`、`client/src/**`、`dispatch/`

## 6. 验收

```text
① 五道门的静态表（门 → 条件 → 地址）
② 运行时打出的门取值 + `Computer.state` 时间线（几次请求样本）
③ 我方实现与观测是否一致；不一致改了什么
④ 样本位置（确认没进冻结快照）
⑤ cargo/三轴/headless 是否退
```

## 7. 汇报（≤8 行）

分支+sha、五道门静态表、运行时样本次数与关键取值、`state` 语义结论、改了什么、样本位置、三轴是否退。
