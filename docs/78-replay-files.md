# 78 · Replay 文件：位置、格式、能拿到什么

## 1. 存在哪

二进制里写死了两条路径（`strings libgraphwarrust.so | grep replay` 可见）：

```
user://replays/                    ← 对局录像
user://graphwar_media/level_replay ← 关卡录像（另一个东西）
```

对应 Rust 模块：`src/graphwar_replay_control.rs`、`src/graphwar_replay_player.rs`；
UI 用 `res://replay_lobby.tscn` / `res://replay_entry.tscn`。

Godot 4 里 `user://` 映射到 `$XDG_DATA_HOME/godot/app_userdata/<项目名>/`
（默认 `XDG_DATA_HOME` = `~/.local/share`），所以：

```
~/.local/share/godot/app_userdata/<项目名>/replays/<unix 时间戳>
```

**本机实测路径**：

| 路径 | 来源 |
|---|---|
| `/home/user/.local/share/godot/app_userdata/Graphwar/replays/` | **真实对局**（本机已有 **375 个文件**） |
| `gw2/.godot-data/godot/app_userdata/gw2-live/replays/` | 我们 harness 自己跑的 |

文件名就是 Unix 时间戳，没有扩展名。

## 2. 格式：**gzip + JSONL**

文件头是 `1f 8b 08`（gzip），解压后每行一个 JSON 事件：

```json
{"event":{"AddObstacle":{"pos_x":2.9390,"pos_y":-2.8351,"radius":3.4329}},"tick":21592}
{"event":{"FunctionInfo":{"owner_id":635,"function":"(-0.35*(-11.40+x))","corrected_function":null}},"tick":21593}
{"event":{"FunctionFire":{"soldier_id":637}},"tick":21611}
```

**和网络协议里的事件完全同构**（同一套 `*_event` 名字），只是多了 `tick` 字段。

一局完整对局约 1500 个事件、解压后 160 KB。

## 3. 里面有什么（一局实测统计）

| 事件 | 条数 | 用途 |
|---|---|---|
| `FunctionInfo` | **523** | **函数文本**（含玩家打字过程中的中间态） |
| `GameStateInfo` | 225 | 状态机 |
| `DestroyObstacle` | 81 | 挖洞 |
| `EffectInfo` / `EntityEffectInfo` | 65 / 23 | 特效 |
| `TurnInfo` / `EndOfTurnInfo` / `TurnCountInfo` | 56 / 56 / 57 | 回合 |
| `FunctionFire` / `FunctionPoints` / `FunctionActive` | 各 56 | 开火与轨迹 |
| `LifeInfo` | 39 | 生死 |
| `PosInfo` | 33 | 士兵坐标 |
| `AddObstacle` / `ClearObstacles` | 23 / 1 | **关卡（精确坐标+半径）** |
| `TeamInfo` / `SoldierInfo` / `PlayerInfo` / `NameInfo` | 18 / 11 / 7 / 7 | 实体关系 |
| `Skin/Face/HatGraphicsInfo` | 各 18 | 外观 |

⇒ **一局的所有关键事实都在里面**：关卡、坐标、生死、每回合双方的函数、开火者。

## 4. 工具：`tools/parse_replay.py`

```bash
# 汇总一局
python3 tools/parse_replay.py ~/.local/share/godot/app_userdata/Graphwar/replays/<时间戳>

# 打印真正开火用的函数（自动跳过打字中间态）
python3 tools/parse_replay.py <目录> --functions

# 把所有 replay 的开火函数汇总成 JSON
python3 tools/parse_replay.py <目录> --corpus out.json
```

`fired_functions()` 的取法是：**每个 `FunctionFire` 之前最后一条非空 `FunctionInfo`**
——因为 `FunctionInfo` 会随玩家修改不断广播（`9`→`99`→`999` 这种中间态也在里面）。

## 5. 已抽取的语料（本轮）

375 局 → **1149 次开火、888 个不同函数**。

- token 数分布从 1 一直延伸到 147（与"难度 6 上限 200"相容）；
- 算子频次（未区分人机）：`+`1653、`*`1561、`/`826、`exp`811、`sin`250、
  `^`224、`abs`112、`sqrt`103、`cos`64、`tan`60、`ln`18、`log`1。

> ⚠️ 这份语料**混了人类和机器人的函数**（里面能看到 `y` 变量和超长式子，
> 那是玩家手打的）。要分离需要把 `FunctionInfo.owner_id`（= **soldier_id**）
> 经 `SoldierInfo{soldier_id, player_id}` 映射到玩家，再对上名字。
> **这一步本轮没做完**——`NameInfo.entity_id` 与 `player_id` 不是同一个号段，
> 关联规则还没确定。

## 6. 为什么这比 OCR 强

用户之前提供的方案是录屏 OCR（`docs/68` §6），问题很多：识别错误、只能拿函数文本、
拿不到坐标/生死/时间。

replay 文件是**官方记录**：函数文本零误差，还附带精确坐标、障碍、生死、tick。
⇒ **以后凡是需要"真实对局数据"，优先用 replay，不要再 OCR 视频。**
