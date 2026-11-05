# HANDOVER —— 给上下文压缩后的自己

**读这一篇就够了。** 它替代"翻 17 篇文档"的过程。若与本篇冲突，以代码与 `docs/` 原文为准。

最后更新：**Godot 4.7.2 已就位，扩展能在真引擎里加载/注册/被调用；
用还原资源打包的 `.pck` 已能启动**。

### 网络协议已还原（本轮）

抓包方式：hook libc 的 `sendto/recvfrom/send/recv`（`tools/capture_net.js`）。
- LAN 发现：UDP 广播，明文 JSON（`time_mode` 写作 `"Timers1m"`）
- 对局：TCP，**`u16` 大端长度 + JSON**
- 29 种消息已入库为 `fixtures/protocol.jsonl`，`src/protocol.rs` 全部解析通过
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
