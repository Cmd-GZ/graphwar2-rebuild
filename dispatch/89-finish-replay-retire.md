# 89 · `agent/replay-retire-finish`：改掉 `play_screen.gd` 那 **4 行**，把 `replay_player.gd` **整份删掉**

**优先级 P1（收尾活，一天不动就多一天的双份局面机）。**

## 0. 现状

```
84 号把旧局面机从 **426 行删到 66 行**（只剩 4 个静态查表函数：axis_index / function_index / turn_index / time_index）
**没能整份删**，因为 `client/src/screens/play_screen.gd:2170–2173` 还有 4 处活调用
   `GwReplayPlayer.axis_index(...)` 等（那是把回放里的模式字符串翻成 `App.state.*` 档位下标）
当时 `client/src/screens/**` 不在 84 号的允许清单里 ⇒ 留着类名
```

## 1. 要做什么

1. **把那 4 处查表搬走**（任选其一，但要写清理由）：
   ```
   A 搬进 Rust：`GwReplayDriver` 加 `axis_index_of(s)` 等四个方法（推荐：与"流程在 Rust"一致）
   B 搬进视图层的一个小工具类（承认它是纯视图映射）
   ```
2. **删掉 `client/src/play/replay_player.gd`**，并 `grep` 全仓确认 `GwReplayPlayer` 零引用
   （包括 `.tscn`、`project.godot`、`client/tools/*.gd`）；
3. **加一条 headless 断言**："旧类名已不存在"（字符串扫全仓），防止又加回来；
4. 同步 `docs/16-client.md` §57（退役记录）、`docs/162`（若涉及档位）与本轮变更。

## 2. 硬约束

- `cargo test --offline` **1753**；三轴 `647 / 647 / 637`；headless **1923 → 只许多不许少**；
  `selftest` **716**（`63x/7` 是既有 flake，重跑再判）；
- **不许**把查表逻辑抄回 `play_screen.gd` 的别处；也不许引入新的 GDScript 局面机；
- headless 只跑 `extends SceneTree` 的脚本；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/replay-retire-finish`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 相关：`client/src/screens/play_screen.gd:2170–2173`、`client/src/play/replay_player.gd`、
  `src/_scaffold/replay_driver*.rs`、`client/tools/play_flow_check.gd`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/replay-retire-finish`

## 4. 可以改的文件

- `client/src/screens/play_screen.gd`（只改那 4 处 + 相关注释）
- `client/src/play/**`（**删除** `replay_player.gd`）
- `src/_scaffold/replay_driver*.rs`、`client/tools/play_flow_check.gd`
- `fixtures/`（若要补期望值）、`docs/16-client.md`
- ❌ 不许改 `src/game.rs`、`dispatch/`

## 5. 验收

```text
① 4 处搬去哪了（A 还是 B，理由）
② `grep -r GwReplayPlayer` 的结果（必须为空）
③ 新增的"旧类名不存在"断言
④ `wc -l client/src/play/replay_player.gd` → 文件不存在
⑤ cargo/三轴/headless/selftest 是否退
```

## 6. 汇报（≤8 行）

分支+sha、4 处搬到哪、grep 结果、断言、文件已删、三轴是否退。
