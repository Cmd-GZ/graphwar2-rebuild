# HANDOVER —— 给上下文压缩后的自己

**读这一篇就够了。** 它替代"翻 17 篇文档"的过程。若与本篇冲突，以代码与 `docs/` 原文为准。

最后更新：**Godot 4.7.2 已就位，扩展能在真引擎里加载/注册/被调用；
用还原资源打包的 `.pck` 已能启动**。

### 网络协议已还原（本轮）

抓包方式：hook libc 的 `sendto/recvfrom/send/recv`（`tools/capture_net.js`）。
- LAN 发现：UDP 广播，明文 JSON（`time_mode` 写作 `"Timers1m"`）
- 对局：TCP，**`u16` 大端长度 + JSON**
- 29 种消息已入库为 `fixtures/protocol.jsonl`，`src/protocol.rs` 全部解析通过
- **一整回合 12 条报文序列**（`docs/46`）：`begin_turn`（TurnInfo/EndOfTurnInfo/
  WaitingForFunctions）+ `fire`（8 条）+ `end_turn`（TurnCountInfo），
  种类顺序有测试断言；新增 `emit_end_of_turn` 并用抓包样本逐字节验证
- **推翻"OS 熵"结论**（`docs/50`）：给每次 getrandom 取调用栈后发现 **915/921 来自 libudev**，
  游戏本体只有 3 次且是 HashMap 的 RandomState 播种。二进制里 `expand 32-byte k`
  出现 6 次 ⇒ 确有 ChaCha；看不见是因为 Rust 走原始 syscall(318)，绕过 libc 函数。
  hook syscall 会让 frida 自断通信，该路不可用
- **服务端循环可跑完整一局**（`docs/62`）：`serverloop::run` + `tests/server_match.rs`
  在真实 TCP 上握手→开局→轮流开火→延迟删除→回合推进→全灭 EndingGame（7 秒，自动化）
- **关卡生成 Rust 实现**（`docs/61`）：`levelgen::generate` 用真实随机流夹具
  复现 13 个障碍，`x/y/r` 的 `to_bits()` 全等
- **生命规则现状**（`docs/63`）：3 局数据挖掘 0 次多杀/0 次队友命中；静态追踪只追到
  `tick_event` 构造器与 Arc 派发，未到判定。下一步应写客户端直接驱动真服务器来制造场景
- **回合顺序判定**（`docs/60`）：按状态指针分 2~4 条流分别记录后重新检验
  （3 局 × 每流 × 3 方法 × 25 偏移）仍全不一致 ⇒ **证伪"来自 ChaCha 流"**。
  最可能来自 **HashMap 迭代顺序**（Rust RandomState 的 SipHash 键由 getrandom 播种，
  与 ChaCha 是两条独立熵流），与全部观测一致。README 该行维持 ⬜ 但理由已升级为
  "已证伪 + 指明方向"
- **回合顺序：固定偏移模型全部落空**（`docs/59`）：6 局 ×（TurnInfo 基准 / 关卡生成结束基准）
  ×（低2位/乘移位/高2位/取模）全部不一致；3 局时的一次命中在 6 局下消失，判定为噪声。
  **关键新发现**：`next_u64` 出现**两个不同状态指针** ⇒ 存在两条独立随机流
  （thread_rng 线程局部）。回合顺序若取自另一条流，主管流里当然找不到。
- **RNG 能力总结**（`docs/58`）：类型/状态/种子来源/重放全部打通，
  关卡生成 13/13 逐位复现。回合顺序与 AI **有了验证手段但未验证**：
  实测士兵创建与首个 TurnInfo 都在 draw 34 之前，洗牌应落在 draw 2..34
  （4 元素 Fisher–Yates 只需 3 次随机数，量级吻合），但具体值未定位
- **种子来源定位**（`docs/57`）：播种是惰性的（首次 `next_u64` 前状态全零），
  事件顺序为 `getrandom×3(16B, GW+0x907c66) → 首次 next_u64` ⇒ 种子来自 **OS 熵**；
  但 key ≠ 种子字节原样，中间有派生（未还原）。这不影响 docs/56 的重放
- **端到端重放成功**（`docs/56`）：从 `.rodata` 导出 ziggurat 表
  （X@0x25b428、F@0x25ac20），用抓到的随机流从索引 3 开始重放，
  **13/13 障碍的坐标与半径逐位一致**。判据 1、2 均达成
- **关卡生成公式闭合**（`docs/55`）：`FUN_005625b0` 其实是 **ziggurat 标准正态采样器**
  （含 rand 的 R 常量 3.654152885361009），所以
  个数 = `trunc(N(0,1)*7+15)`、半径 = `max(N(0,1)*1.6+2.6, 0.8)`、
  `x=(2r+25)(2u-1)`、`y=(2r+15)(2v-1)`。
  这解释了障碍数 5~21、半径 0.8~5.3，以及 docs/54 里"半径对不上线性关系"
  （正态 + ziggurat 拒绝采样会消耗不定数量随机数）
- **RNG 找到了（ChaCha，可读状态）**（`docs/53`）：Ghidra 反编译出关卡生成函数
  `FUN_004f1e50`（模块 `graphwar_level_generator.rs`）：个数=`trunc(rand*7+15)`，
  每个障碍 `r/x/y` 由 ChaCha 产生，`x=(2r+25)(2u-1)`、`y=(2r+15)(2v-1)`；
  块函数 `FUN_00850f30` 是标准 ChaCha（σ 常量 + quarter-round）；
  hook `0x47f700` 可直接读出 key+计数器。**重放暂未对上**（疑为 BlockRng 缓冲区索引）
- **找 RNG 的现状**（`docs/52`）：调用链已确定；两次内存扫（全内存 / 栈）都命中
  σ 常量，但结构是 **72 字节步长、密钥位是指针或低地址**（每次运行还变），
  与 ChaCha 状态（64 字节、密钥位为随机字节）不符 ⇒ **判定不是活的 PRNG 状态**。
  断点：定位不到 RNG 实例。下一步应用 Ghidra 反编译 0x3f1a50/0x3f1d85 看浮点数来源
- **障碍生成链路已定位**（`docs/51`）：`send` 见到 AddObstacle 时取栈，得到
  `0x8f46c3 → 0x45d404 → 0x3d8d2b → 0x45a60c → 0x3f1d85 → …`（三帧一致）。
  另试过扫内存找 ChaCha 状态：命中 7 处但**密钥位是指针/长度，不是随机数**，判定不是活状态
- **随机源重放试过，不行**（`docs/48`）：hook `getrandom` 抓到 921 次调用，
  采样载荷全是 **16 字节**（更像 Rust `HashMap` 的哈希种子，而非 32 字节 ChaCha 密钥）；
  即便拿到种子，**消费顺序不可观测**，无法定位"关卡那一刻"的流位置。
  ⇒ 随机模块（关卡生成、AI）此路终局；README 的 AI 行降为 ⬜
- **回合出场顺序判定为不可复现**（`docs/47`）：按 `SoldierInfo` 创建顺序重新检验，
  三局创建顺序完全相同却给出不同回合序列；合并 `docs/28` 共 10 局，
  排除所有简单确定性排列 ⇒ **推断为开局随机打乱**（与 OS 熵一致），标记为推断。
  README 该行由 🔶 降为 ⬜。影响：整局逐条回放不可行，只能按"每一发"对拍
- **回合计数与结束判定**（`docs/45`）：`TurnCountInfo` 在回合结束发出、每回合 +1；
  `EndingGame` 紧跟其后且**只在某队全灭时**出现（本局我方两士兵全死）。
  坑：`Match::new` 曾把士兵默认登记为 0 号队 → 误判全灭，已改为不预设
- **障碍延迟删除**（`docs/44`）：`QueueDestroyObstacle` → `DestroyObstacle` 实测延迟
  **165 tick**（三样本 163.9/164.0/167.0），`Match::tick()` 到期发出且不重复
- **一回合的报文顺序对上**（`docs/43`）：`fire()` 现在按抓包顺序产出
  `CalculatingResults → FunctionInfo(修正后) → FunctionPoints → SynchronizingResults →
  [死者] → [销毁] → FunctionActive → ExplosionEffect → ExecutingResults`；
  测试断言种类序列与三个状态名
- **`end_of_turn_tick` 解开**（`docs/42`）：它是**回合截止时刻** =
  `回合开始 tick + 时长`；频率同为 60Hz；**六档全部实测**：1m=3600 / 2m=7200 / 3m=10800 / 5m=18000 / 30s=1800，
  且 `TimerInf` 线上发 **0**（哨兵）；按钮循环顺序 1m→2m→3m→5m→Inf→30s→1m
  （反推的回合开始 tick 与当时 `current_tick` 差 0~60）。其余档位按名字×60 推断
- **回合状态机可运行**（`docs/41`）：6 状态 7 事件，13 次迁移与抓包**逐个相同**。
  **未还原**：各状态持续时间——`end_of_turn_tick`（10886/11184/11457）与我们的
  60Hz 时钟（65/126/426）对不上，说明是另一套时间基准
- **关卡生成不可确定性复现**（`docs/40`）：hook libc 随机源，40 秒内 `getrandom` **921 次**、
  `getentropy` 2 次，`rand/random/srand` **全 0** ⇒ 种子来自 OS 熵，同局无法重现。
  只能观测分布（41 样本：障碍 5~21 个、`x∈[-26.35,25.80]`、`y∈[-20.17,20.58]`、
  `r∈[0.80,5.26]`，0.8 像下限钳位）。这一行因此回落为 ⬜
- **障碍摧毁=挖洞**（`docs/39`，本轮关键发现）：frida hook 光栅化，发现一局 28 次调用里
  有 **8 次 `fill=0xFF`、半径 0.5**，位置正是命中点 ⇒ 不是整块移除，而是打洞。
  `QueueDestroyObstacle.radius` 恒 0.5 就是洞半径。回放验证：#2 从差 23.3 → 差 0.34。
  **误差会累积**（我们在自己的停止点挖洞，而真实游戏在真实命中点挖），
  多发射击回放容差因此放宽到 1.0
- **动态掩膜假设被否定**（`docs/38`）：移除被摧毁的障碍后，#2 应飞到 24.9，
  实际游戏仍停在 1.603；且 #2 的摧毁点与障碍边缘偏差 2.309（#0 只有 0.029）。
  ⇒ "障碍摧毁导致弹道变远"不成立，1.050→1.603 的差异**仍未解释**
- **按回合配对精确回放**（`docs/37`，`tools/replay_extract.py`）：维护函数时间线，
  把每次开火与当时的函数原文绑定 → 4 发终点全对（差 0.006~0.394），
  **致死集合逐发完全一致**（`[]`/`[231]`/`[]`/`[230]`）
- **`Match` 服务端对象**（`docs/36`）：`fire()` 一次产出整串报文
  （FunctionPoints → 生死 → 爆炸 → 销毁），tick 走 60Hz 时钟；死者从场上移除。
  死亡关卡 6 发回放：终点全对、产出全部可解析。
  **教训**：`FunctionInfo` 随回合更新，提取时只留最后一条会丢历史——
  必须保留时间线（本轮因此把"精确致死集合"的断言收窄为"不越界"）
- **tick 时钟**（`docs/35`）：抓包加了时间戳后测出**精确 60 ticks/秒**，
  `current_tick = base + round(ms*60/1000)`，113 个样本 110 个精确命中
  （差异是抓包时间戳抖动）；`TickReply` 序列化逐字节一致。
  **未确定**：客户端发 `TickRequest` 的周期（1/45/955ms 混杂）
- **玩家颜色**（`docs/34`）：观测到 10 个不同取值且有重复 → 不是纯随机 RGB；
  但同一玩家跨局颜色不同 → 也不按 id 绑定。**规则未定**，`server` 里用占位值
- **服务端握手可运行**（`docs/33`）：`server::on_new_connection` 产出抓包里的 7 条状态同步，
  逐字节一致；`examples/serve_handshake.rs` 经**真实 TCP 往返**验证
  （客户端发 `NewConnectionRequest`，收到 7 帧 324 字节）
- **网络编解码实现完成并逐字节验证**（`docs/32`）：`Broadcast::to_json` 与
  `emit_turn_info`/`emit_life_queued_death`/`emit_function_points`/`emit_effect_info`
  重新序列化抓包样本后**完全一致**（含 f64 最短往返格式）。
  示例：`cargo run --example emit_broadcast -- "[::1]:43717"` 与抓包同串
- **对局内消息已抓到**（`fixtures/protocol_match.jsonl`，109 条去重）：
  `AddObstacle{pos_x,pos_y,radius}`（一局 12 个）、`PosInfo{entity_id,pos,radius=0.5}`、
  `LifeInfo{entity_id,alive}`、`FunctionInfo{owner_id,function,corrected_function}`（函数原文！）、
  `VersionInfo`、`TimeModeChangeRequest`、`FunctionUpdateRequest`
- **回合/开火链路也抓到了**（150 秒对局）：`GameStartRequest`、`ClearObstacles`、
  `TurnInfo{player_id,soldier_id}`、`TurnCountInfo{turn_count}`、
  `EndOfTurnInfo{end_of_turn_tick,time_mode}`、`FunctionUpdateRequest{player_id,function}`、
  `FunctionFireRequest{player_id}`、`FunctionFire{soldier_id}`、`LifeInfo`、`EntityEffectInfo`
- 抓包两个坑：`recv` 内部调 `recvfrom` 导致**双计**；入向流里**长度前缀重复**
  → 最后改用"扫描 JSON 对象"提取
- **自我更正**：docs/24 曾写"广播拼 `Timers1m`、TCP 拼 `Timer1m`"，实测 `Timers1m`
  出现 0 次（两次抓包共 385 个 `Timer1m`/`Timer3m`），原文已改并记录教训

### 验收清单（本轮新增）

[`docs/49-verification-checklist.md`](49-verification-checklist.md) 把每条 ✅ 整理成
**可直接照跑的命令 + 实测输出**。本轮逐条执行的结果：

```
cargo test --offline               通过 106，失败 0
cargo build --release              12 个导出符号（含 gdext_rust_init）
emit_broadcast 逐字节比对           一致 ✓
cargo test --test live_hits        12 passed（真实关卡回放 / 致死 / 事件序列）
tools/{capi,mask,mask_api,simulate}_check.py   四条 RESULT: PASS
serve_handshake 真实 TCP 往返       7 帧 / 324 字节
```

### 最终验收（全部实测）

```
cargo test                40 条通过
cargo build --release     492,688 字节, 12 个导出符号, 含 gdext_rust_init
真 Godot 4.7.2            GW2_HAS_CLASS=true GW2_INSTANCE=ok GW2_HAS_PING=true
仓库                      子仓 5e6cef3 / 主仓 28eb9b0；主仓跟踪 gw2-rebuild 文件数 0
进度表                     ✅ 7   🔶 4   ⬜ 2
```

### 当前可复现的三条验收（真引擎）

```bash
export XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache XDG_CONFIG_HOME=$PWD/.godot-config
# ① 扩展加载 + 类注册 + 方法调用
cd godot-probe && ../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --editor --path . --quit
                   ../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path .
#   期望：GW2_HAS_CLASS/INSTANCE/HAS_PING=true，并打印 "ping() called from GDScript"
# ② 用还原资源打包并从 pck 启动
cd godot-dist && ../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --main-pack out.pck
#   期望：GW2_TEX=true size=(1024.0, 1016.0)
# ③ 原始扩展的 API 面（负面结论：只暴露 12 类 25 个 UI 方法）
cd godot-ref && ../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path .
```

**注意**：`~` 只读，必须设三个 XDG 目录；`godot-probe/main.tscn` 必须指向 `main.gd`
（指错会静默退出，什么都不打印）。

### 活体 oracle（本轮新增，能力级突破）

```bash
node tools/frida_live.js modules
node tools/frida_live.js call <rva-hex> <ret-type> [type:value,...]
```

frida 拉起真游戏 → 等扩展加载 → 在**运行中的进程**里调 `libgraphwarrust.so` 的内部函数。
实测 `0x9302b0`（Ghidra `0xa302b0`）：`2.7→2`、`-2.7→-3` ⇒ 是 **floor**。
详见 `docs/20-live-oracle.md`。

前置修复（一次性）：`cd .rea-runtime && npm install-scripts approve frida &&
npm rebuild frida --foreground-scripts`。REA 自带 frida 工具因缓存失效仍不可用，
所以直接走 Node API。**每次 bash 是独立 PID 命名空间**，必须用 spawn 模式。

### 原版游戏已能在我们的 harness 里跑（本轮突破）

```bash
cd godot-live && rm -f project.binary      # 关键
godot --headless --path .                  # → GRAPHWarRootNode 活着，子节点齐全
```

三个卡点与解法见 `docs/22-live-game-harness.md`：占位资源、Ad_holder 桩、
**`project.binary` 覆盖 `project.godot`**（最隐蔽的那个）。
⇒ `docs/21` 里"玩法无真值来源"的结论已作废。

**而且能开一局真实对局了**（按钮链路见 `docs/22` §5.1）：
Battle -> Create Game -> AddBot（弹窗里选难度）-> Start -> `PlayScreen`。

对局中 hook 计数（12s）证明玩法在跑：
`parse 11 / eval 1 / rasterize 38 / point_test 21629 / mask_ctor 2 /
turn 1259+1531 / life_entry 12 / levelgen 1+1`。
工具：`tools/hook_live_game.js`（frida 拉起 harness 并自动开局）。

**开火结算/生命的活体取证**（docs/23 §5）：`shot_calc` 入参 Vec len 3 → 返回后 len 5
（cap 3→8、指针换新，像在追加事件）；`life_entry(ctx, id, 1, 0)`，id ∈ {230,231,233,234}。
**两者仍未复刻**，只是观测。

**真实对局对拍已通过**（`docs/23`）：从对局里抓 `calculate_path` 的入参与输出，
`graphwar_curve.calculate_path` 逐点一致 69/69（step=0.5、flip=True）。
⚠️ 由此发现"游戏一律用 step=0.01"的说法不准确：真实调用点会传别的步长。

**让 Bot 开火的关键**：GameSetup 里先按两次 `TurnTimeButton`、再按 `TurnModeButton`，
然后 Start。否则默认回合时限太长，`shot_calc/calculate_path` 一直是 0。
调快后 60 秒内 `shot_calc 159 / calculate_path 159 / parse_function 66`。

### 玩法 oracle：四条尝试的记录（部分已被 docs/22 推翻）

见 `docs/21-oracle-limits.md`。摘要：
1. GDScript 层面玩法不暴露（12 类 25 个 UI 方法）；
2. headless 主菜单下 12 个玩法函数 hook **全零调用**；
3. 自建项目跑原版场景失败：AdMob 原生插件缺失 + 导出态资源（只有 `.ctex`/`.import`，
   源 `.svg` 不在包内）无法按源码路径加载；
4. 直接实例化 `GraphwarRootNode`：无资源时 panic，有资源时依赖链仍失败。

⇒ 回合/关卡/AI/结算保持 🔶/⬜ 是**缺真值**，不是没做。

### 本轮新增的两个坑

7. `GDExtensionClassMethodInfo` 末尾还有 `default_argument_count` / `default_arguments`，
   少写这两个字段 → 引擎 abort（读到栈外垃圾）。
8. `.pck` **不包含** `.so`：`res://libgw2_rebuild.so` 会 `file not found`，
   必须用裸文件名并把库放在 `.pck` 同目录。

**四条独立验收**（都能重跑）：
```
cargo test                        40 条通过
cargo build --release             产出 libgw2_rebuild.so (ELF, 12 个导出符号)
tools/gdext_probe.c               dlopen + 入口契约 PASS
tools/{capi,mask,mask_api,simulate}_check.py   全部 RESULT: PASS
```

---

## 1. 现在在哪

| 仓库 | 远端 | 状态 |
|---|---|---|
| `gw2-rebuild`（本仓库） | `Cmd-GZ/graphwar2` | Rust 复原 21 条测试全过；`rsc/` 已还原但 gitignore |
| `gw2`（上游） | `Cmd-GZ/graphwar-algorithm-analysis` | 曲线算法报告已更正；与本仓库互不包含 |

两仓库都只允许用 `GIT_SSH_COMMAND="ssh -F /dev/null" git push`（本机 ssh 配置有权限问题）。

## 2. 已完成、有验收的部分

| 部件 | 位置 | 验收 |
|---|---|---|
| 函数文法（词法/解析/求值） | `src/function/` | 37 + 70 条真值夹具 |
| 等弦长采点（含 RK4） | `src/path.rs` | 224 条（flip × ode 全覆盖） |
| 修正常数 + 端到端 + **开火模拟原语** | `src/shot.rs` | 端到端 + 上游 648 例；装配校验 15 组（含 ODE/flip/2 次真实命中）`tools/simulate_check.py` |
| 坐标变换 + 掩膜点测试 + **光栅化** | `src/plane.rs` | 14 条 + 与 oracle 逐字节对拍 |
| 事件枚举形状 + 生命两张表 | `src/events.rs`、`src/life.rs` | 5 条 |
| 游戏资源 | `tools/pck_extract.py` → `rsc/` | 692 文件 md5 全符 |
| **可构建二进制** | `Cargo.toml` cdylib + `src/gdext.rs` | `cargo build --release` 出 .so；`nm` 见 `gdext_rust_init`；`tools/gdext_probe.c` dlopen 实测 PASS |

## 3. 核心结论速查（省掉重新推导）

**曲线**（逐位验证）
`parse @0x4ed670` / `eval @0x4eb9d0` / `path @0x3c2530`。
`step=0.01`、容差 `0.001`、最多 100 次二分、点数 `(200/step) as i32`、
边界 `|x|≤25 |y|≤15`、修正常数 `c = y0 − f(±x0)`。

**坐标**（观测，`0x4be3e0`）
`px = 20x + 500`、`py = 20y + 300`，画布 1000×600。
三处（`0x4be3e0`/`0x4f11e0`/`0x4f15b0`）同构，`.so` 内**无 y 翻转**。

**命中**（观测，`0x4f15b0`）
1000×600 字节掩膜，`idx = py*1000+px`，`!= 0xFF` 即命中，**越界算命中**。
掩膜由 `obstacle_manager` 的 `0x523020` 创建：`alloc(600000)` + `memset 0xFF`
+ 引用计数容器；同模块负责填充（`0x4f11e0`）与查询（`0x4f15b0`）。
**三个 API 都已与真二进制对拍通过**（`tools/mask_api_check.py`，随机用例 0 差异）。

**资源**（实测）
PCK v4：索引在包尾（头部 `0x20` 处 `directory_offset`），
偏移基准是 `file_base`（**不是**包起点，flags=2 会骗人）。

**控制流**（观测）
`calculate_path` ← `shot_calculator`、`target_manager`；
`shot_calculator` ← `game_state_manager`、`function_evolutionator`（均经函数指针）。

## 4. 明确未解决（别假装做了）

| 块 | 卡点 |
|---|---|
| ~~光栅化~~ | **已完成**：`src/plane.rs::rasterize` 与真二进制逐字节一致（`tools/mask_check.py`，5 组 0 差异） |
| 回合 `turn_manager` | 14 个函数全是 HashMap 探测（`0x28` 步长）；无浮点常量 ⇒ 时限是数据驱动 |
| 目标选择 `target_manager` | SIMD 字节扫描，反编译不可读 |
| AI `function_evolutionator` | 跳转表 Ghidra 未建模；"批量试射"仍是推断 |
| 二进制功能 | 入口已可加载，但**未注册任何类**；且本机只有导出模板、无编辑器，未在真 Godot 里跑过（`docs/18` §6） |

## 5. 决策记录

- **上游报告不拆分**：`docs/09` 有判据（744 行 < 1200 阈值；新增内容仍属主线）。
- **`rsc/` 不入库**：二进制衍生素材 + 公开仓库涉及再分发；只提交 `MANIFEST.tsv`。
  用户若确认有权，再改 `.gitignore`。

## 6. 我犯过的错（别再犯）

1. 把**虚地址**当 Ghidra 地址传工具 → `not_found`（差 `0x100000`）。
2. 直接 grep 反编译文件的浮点常量 → 被文件头 `Ghidra 12.1.4` 的 `12.1` 污染。
3. 凭函数内部特征猜用途 → 把 `0x567530` 猜成"AI 选弹道"，查调用者后是**关卡**通视检查。
4. 用一次 `str.replace` 改文档却没核对结果 → README 索引**只更新了一半**（静默失败）。
5. 相信 `funcstarts.pkl` → 与 Ghidra 交集仅 2/13430。
6. 按 Godot 源码字面理解 `PACK_REL_FILEBASE` → 692 个文件 md5 全错，实测要用 `file_base`。
7. 把**文件偏移当虚地址**用 → 掩膜归属算错（`.text` 段 offset 与 vaddr 差 `0x1000`）。
   换算必须走 ELF 段表：`vaddr = sec.addr + (off - sec.offset)`。
8. 从局部伪代码推取整函数 → 把"截断"说成"四舍五入"；必须用输入输出对拍。

## 7. 下一步优先级

1. **让重建结果能构建成二进制**：`crate-type = ["cdylib"]` + 导出 `gdext_rust_init`；
   先用 `readelf -h` / `nm -D` 证明产物是 ELF 且带入口符号，再接真实 godot-rust。
2. 光栅化三条缺口（补齐即可实现 `rasterize`）。
3. 回合语义：先解 `0x28` 步长的那张表。

## 8. 文件地图

```
docs/00  状态总览          docs/08  坐标/掩膜      docs/14 目标选择
docs/01  架构              docs/09  报告拆分决策   docs/15 端到端（建议先读）
docs/02  模块地图          docs/10  生命死亡       docs/16 如何验证
docs/03  函数语言          docs/11  间接派发       docs/17 资源还原
docs/04  采点算法          docs/12  AI
docs/05  开火链路          docs/13  回合
```
