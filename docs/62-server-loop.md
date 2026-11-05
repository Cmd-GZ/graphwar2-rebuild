# 62 · 能跑完整一局的服务端

## 1. 装配了什么

`src/serverloop.rs` 把已有零件串成一个真正的服务端：

```text
NewConnectionRequest  -> 状态同步 7 条                     (docs/33)
GameStartRequest      -> 生成关卡 + 士兵/队伍/位置/生命 + WaitingForFunctions
FunctionUpdateRequest -> 记下该玩家的函数
FunctionFireRequest   -> begin_turn -> fire（8 条）-> tick（到期销毁）-> end_turn
                      -> 某队全灭则 EndingGame              (docs/45)
```

关卡由 `levelgen::generate`（`docs/61`）现生成，种子来自 `/dev/urandom`，
随机数用 `rng::ChaCha12`（与游戏同算法，`docs/53`）。

## 2. 验证：真实 TCP 上打完一局

`tests/server_match.rs`（**自动化，`cargo test` 覆盖**）：

1. 服务端在 `127.0.0.1:0` 起监听，测试线程跑 `serve_stream`；
2. 客户端连上后发 `NewConnectionRequest`，断言握手序列**恰好是**那 7 条、顺序一致；
3. 发 `GameStartRequest`，断言：生成了障碍（>0）、4 名士兵、
   首回合 `TurnInfo`、最后一条是 `GameStateInfo`；
4. 按 `TurnInfo` 轮流开火（直线 `0.00`），最多 12 回合，**要求出现 `EndingGame`**。

实测：`test result: ok. 1 passed ... finished in 7.03s`。

手工跑同样的事：

```bash
cargo run --quiet --offline --example serve_match -- 127.0.0.1:39993 &
python3 tools/client_match.py 39993
```

实测输出（节选）：

```
开局: {'AddObstacle': 18, 'SoldierInfo': 4, ..., 'TurnInfo': 1}
第 1 发 (player 229): 15 条 -> [..., 'LifeQueuedDeath', ..., 'QueueDestroyObstacle', ...,
                              'DestroyObstacle', 'TurnCountInfo', 'TurnInfo', 'EndOfTurnInfo', ...]
第 2 发 (player 229): 13 条 -> [..., 'LifeQueuedDeath', ..., 'TurnCountInfo', 'GameStateInfo']
对局结束（EndingGame）
```

服务端日志：`ENDED at turn 2` —— 我方两发直线打掉对方两名士兵，队伍全灭。

## 3. 修掉的两个 bug（都是测试逼出来的）

1. **没校验回合归属**：同一个玩家连发 8 次都被接受。已加 `current` 校验。
2. **取错了士兵**：一个玩家有两名士兵，按 `player_id` 查表会取到**已阵亡**的那个
   （日志里 `SOLDIER_DEAD 232`）。改成跟踪 `(player_id, soldier_id)`。

## 4. 简化之处（明确写出来）

- **颜色分配**：用固定值占位（规则仍未知，`docs/34`）；
- **士兵站位**：两队对称固定（真实关卡是随机的），便于复现；
- **朝向/`flip`**：按队伍固定（Team1 向右、Team2 向左），不是游戏里的真实规则；
- **tick 节拍**：由服务端在每次开火后立即推进到"销毁到期"，没有做真实的时间轮；
- **玩家加入流程**：只接受一个连接，没有 `NewPlayerRequest` 的完整分配逻辑。

这些都写在代码注释里，不是遗漏而是**尚未验证的选择**。
