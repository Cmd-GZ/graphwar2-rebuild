# HANDOVER —— 给上下文压缩后的自己

> 最后更新：**2026-11-08 凌晨**（第六轮合并后）。这一晚的进展全在 `/loop` 之外的一次爆发里，先看 §0.0。

## 0.0 一分钟看懂现状（2026-11-07 深夜）

```
还原进度   **527 / 527 可归属函数 = 100%**（逐位 88 / 观测 439 / 推断 0）｜ 未做 0、未知模块 0
协议       **74 / 74 全字段 + 74 条判别值**（72 条 `cmp` 链 + `63`/`65` 由 arm64 跳转表定死）+ 7 个线上枚举全集（已无未知）
GDExtension 类   12 / 12 全部还原
真名       **348 / 527** 有原版真名（并集 351 个地址；无名 179 = 138 推断 + 41 未知）
           （**审计 2026-11-08**：原记 327，那是 §20 的单表口径；现行三表并集见 `docs/02` §37–§40。
            327 条仍可在 `fixtures/name_to_addr.tsv` 里查到）
cargo      **1585 passed / 0 failed**（`--features gdext-bindings` 的更数见当轮实测）
客户端自检   **709 passed / 0 failed**（前置：`client/assets/` + `examples/serve_match`；
           **审计 2026-11-08**：原记 694，`docs/16` §49.7 已涨到 709（+15）；缺资产的
           591 / 2 / 23 等处是 2026-11-07 当轮读数，本次未复测。两个口径见 README「客户端自检的两个口径」）
```

⚠️ **527 只是"游戏函数"的一部分**：2.0.60 arm64 的 `FUNC` 共 18,793 个、名字含 `graphwar` 的 **1,300** 个
⇒ 527 约为游戏函数的 **40%**；其余 773 个（1,300 − 527；**审计 2026-11-08** 更正原记的 770）
没有 `panic::Location`（纯计算 / 闭包 / 泛型单态化），尺子数不到。
⚠️ 但**名字不一定拿不到**：`tools/sym_match.py` 用「函数顺序（主）+ 体量比 + 字面量」批量对，
**1,300 个 arm64 符号里 762 个给出了唯一候选（63.0% = 681 unique + 81 weak，`docs/02` §40.2）**，
其中就包括采样器（**审计 2026-11-08**：原记 733 / 61%，那是上一轮的读数，已按 `docs/02` §40 更正）
`calculate_function`（arm `0x3b5374` ↔ x86 `0x3c2530`，体量比 0.61）。
`fixtures/sym_match.tsv` 每条都带 `confidence` + 可复核的 `evidence` 字段。

**这一晚真正的突破不是代码量，是三份"白捡的证据源"**（详见 README 同名小节）：

1. **Steam 2024-07-30 的 Linux 版没 strip** ⇒ 13,086 个 FUNC、**78 模块 / 165 类型 / 290 方法名**；
2. **Android APK 里的 `.so` 同样是 2.0.60 且没 strip**（`project.binary` 里 `version = "2.0.60"` 证实）；
3. **`panic::Location` 跨架构不变**：两侧 `(文件,行,列)` 三元组 **2,035 / 交集 2,035 / 差 0**
   ⇒ 把 351 个原版函数名钉到 x86 地址（含 527 内的 348；`docs/02` §20、§21 是当轮的 327 口径，
     现行并集见 §37–§40）。

**方法论一句话**：`strip` 只删元数据，同一程序的**别的构建、别的平台**里那份元数据往往还在。
多找一个平台，比硬啃汇编划算得多。

### 0.0.0.1 能不能真的跑起来（2026-11-07 实测）

**两个必须分清的东西**（混起来会得出错误结论）：

| | 现状 |
|---|---|
| 12 个 GDExtension 类的**逻辑** | ✅ 全部还原在 `src/`（纯 Rust、可测）——但它们是**知识不是节点** |
| 把它们变成**真 Godot 节点** | 🔶 只验证了 1 个（`AxisDrawer`）：`ClassDB.class_exists=true`、`.tscn` 能实例化、`_draw` 被引擎调到（102 条线） |

**绑定层两条路，代价已实测**（`docs/18` §10）：

| | 手写（默认，现状） | 官方 `gdext`（原版用的就是它） |
|---|---|---|
| 依赖 / 冷编译 / `.so` | 1 行 / 7.96 s / 781 KB | 32 行 / 161.6 s / 5.45 MB |
| 一个带 `_draw` 的类 | ~800 行，**且没有虚方法表** | **92 行**，虚方法表由宏生成 |
| 源码保真 | ✗ 我们的发明 | ✅ 原版 `.rodata` 里有 `gdext-…/godot-core/src/registry/class.rs` |

⇒ 骨架已按 **cargo feature `gdext-bindings`** 并入（**默认关闭**：零依赖、1585 绿）。
**12 个类迁移粗估 ~1200 行**，正在做第一个垂直切片（`EntityDrawer`）。
⚠️ 三个坑：不 `--import` 扩展静默不加载；`--script` 的 root 进树有时序；`GDExtensionPropertyInfo` 是 **48** 字节。

### 0.0.0 这一晚解出的"能直接写进攻略"的事实（2026-11-07）

先记这些，因为它们比代码更难得：

| 主题 | 结论 | 出处 |
|---|---|---|
| **队伍怎么分** | `Σteam < Σ(team^1)` ⇒ **进人少的队，平手进 Team1**；无队长/选边/每队容量；整房上限 **10 人** | `docs/10` §13 |
| **颜色怎么分** | 队号选池 → `rand::random::<u64>() % 6` 起点 → `+1` 线性探测跳过已用 → 6 个全占则 `(r%180,g%180,b%180)`。**不是进房顺序、不是 id 取模** | `docs/10` §15 |
| **谁成为 Bot** | **不是掉线托管**。唯一入口是 `bot_added_event`(10)；`level` 只决定强度（阈值 4/11/26/101/251，精英数 = 种群/5，limit 5/5/10/15/25/200）。Bot 名字是 `rand % 10` 从 10 项池随机取，**与难度无数据流** | `docs/13` §25 |
| **回合超时是时钟判据** | `state==WaitingForFunctions ∧ now_tick>=end_of_turn_tick ∧ [+0x11c]!=0` ⇒ 自动 `fire_functions`。**不用玩家点按钮** | `docs/14` |
| **目标"打不全就全部复原"** | 判定点在 `queue_end_turn_trigger::{{closure}}`（arm64 `0x4abc00`）：回合末 ∧ 两队都有人活 ∧ `[+0x11d]==1` ⇒ 逐个 `set_alive(id,true,0)`。**`target_manager` 里没有这条分支**（它只是 `HashSet<u32>`） | `docs/16` §32 |
| **死亡发哪个特效** | **照抄出生登记**：`effect_type = entity_effect_map[entity_id]`，查不到就**一条都不发**。分界在**出生**、不在死亡 —— 这正是整局对拍那 19 例失败的成因 | `docs/16` §32 |
| **大厅房间顺序** | **没有排序**：顺序 = `HashMap` 迭代序 = `add_child` 序 ⇒ **大厅先后不是信息** | `docs/09` §10 |
| **音效 ↔ 事件** | 订阅 `effect_info`(44) / `soldier_info`(15) / `entity_removed`(14) / `game_state_info`(30)；槽 = explosion/squash/spark/pop/unpop，音量 −15/−15/−15/0/−5 dB | `docs/16` §30 |
| **段位不用算** | 只从远端事件进，模块内零换算；`RankInfo`(67) = 三个 dword（标志 / 段位 / player_id） | `docs/10` §14 |
| **客户端只认 IP 字面量** | 本机连 `[::1]:port`（**不是 127.0.0.1**）；远端 `SocketAddr::from_str` ⇒ **没有 DNS，域名连不上**；`connect_timeout(30s)`，**无重连** | `docs/09` §11 |

### 0.0.0.2 对拍现在到底多准（**三条轴，报数必须带轴名**）

同一份 **401 份定格快照**（`.agent-work/replay-snapshot/`，指纹见 `docs/06` §25）。
三条驱动轴的数**完全不同**，混着比就是错的——上一轮就是因此把 1074 记到了 `batch` 上。

| 指标 | `fire` | `batch` | `batch_conv`（收敛层） |
|---|---|---|---|
| 内容命中 | **647 / 1080** | **647 / 1080** | **633 / 1080** |
| 种类序列 | **1075 / 1080** | **1075 / 1080** | **1075 / 1080** |

另有两条延迟段指标（`batch` 轴）：死亡集合 **366 / 385**（漏报 0）、挖洞点 **207 / 294**。

复跑命令（**不给命令的数字一律当未验证**）：

```bash
export CARGO_HOME=<WS>/.cargo-home CARGO_TARGET_DIR=<WS>/.cargo-home/target
cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive fire  --policy default --json f.jsonl
cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive batch --policy default --json b.jsonl
cargo run --release --offline -q --example batch_conv   -- --dir <快照> --policy strict
```

**当前最大的一块残差**：`Y` 桶（爆炸点 `pos_y`）的 154 批真末位差 —— 三个假设（取错 x / 相邻采点 /
常数回代顺序）**已被数据全部否掉**，机制仍未知（`docs/06` §25）。下一轮的第一优先是
用 unicorn 对拍 `evaluate_recursive`（任务书 15）。

### 0.0.0.3 合并纪律（2026-11-07 事故后写死，**强制**）

一句话：**不许只看 `cargo test` 绿就合并。**

事故：下游报「种类序列 1074」，我解完冲突就合了。事后复算 —— 1074 属于 `batch_conv` 轴，
`batch` 轴实为 915；而同一批改动里还夹着一个**真 bug**（函数原文括号不平衡 ⇒ 解析器截断
⇒ 弹道退化成 1 个点 ⇒ 死亡特效整块消失），也是事后才查出来。

完整清单见 `AGENTS.md` 的「合并纪律」一节（A 分支侧 5 条 / B master 侧四件套 /
C 数字必须带轴 / D 明令禁止）。**这是本项目唯一一条针对"人"而不是针对"代码"的硬规矩。**

### 0.0.0.4 本轮被审计推翻/修正的结论（别照旧文档抄）

一份对抗性审计（`docs/152`）独立复算了 8 项已合并的结论：**一致 6 / 不一致 2**，另加 2 处推广错误。

| 原说法 | 实测 |
|---|---|
| `0x3c1b40` 出口是 `setb` | **`setg`**（`0x3c1c1f`，字节 `0f 9f`）；语义等价，结论不变 |
| `execute_results` 体到 `0x4a8e50` | 到 **`0x4a93d8`**（`.symtab` 3712 B；`0x4a8e50` 只是热路径 `ret`） |
| `0x9baa10` 使用者全在 `0x3axxxx` | **267 条 `call *` + 56 条 `mov`，跨 153 个函数**；结论（`=bcmp`）仍成立 |
| 「13 种格式原文」 | 实际只列了 12 种（漏 `7.6e-19` 那一对），已补 |

### 0.0.1 这一晚踩过并已写死的坑（别再犯）

| 坑 | 真相 |
|---|---|
| 2026 三版 `.so` 尾部那 **793 KB** 像是符号表 | **不是**，是 2024 版 `.strtab` 的下半截死数据（`docs/02` §19） |
| 2024 版 `.pck` 里 313 个 `.svg` / 249 个 `.gd` 路径串 | **不是文件**，是 `.import`/`.remap` 记的源路径 ⇒ 素材源文件确实拿不回来 |
| `download_depot` 下多版本 | 同一 app+depot **永远写同一路径，会覆盖**；必须下完立刻 `cp` 走 |
| `tools/class_probe.py` v1 的函数归属列 | 偏 `+0x1000`。真因**不是** pkl 过期（`xrefs.pkl` 与 `.so` 逐条一致 5301/5301），是把已是虚地址的引用点又过一遍 `off2va()`。已修为 **v2**，并新增 `tools/loc_xrefs.py`（只读 `.so`）。旧文档里由它得出的地址都要按 `docs/16` §27.4 重核 |
| doc 注释里放 shell 命令 | 4 空格缩进会被 rustdoc 当 doctest 编译；**必须用 ```text 围栏** |
| 用"谁持有它"反推"它在哪个模块" | **会错**。`GraphwarTextureLoader` 我按持有者 `root+0x16a0` 推成了 `graphics_manager`，实际是独立模块 `graphwar_texture_loader` |
| Python 里写中文映射 | 中文语境混入 ASCII 双引号会截断字符串（今晚踩了**三次**）；用「」或用三引号 |

**"看起来该是图元的东西，其实是节点"** —— 这一晚撞了三次：
士兵/靶子是 3 个 `Sprite2D` + 1 个 `RichTextLabel`；弹道是 `Line2D`（`add_point` 全二进制只有 1 个调用点）；
爆炸才是真图元（`draw_circle`×4 + `polygon` + `line`×2）。

### 0.0.2 还剩什么（按"是不是复原"排序）

| # | 缺口 | 现状 |
|---|---|---|
| 1 | **客户端控制器迁回 Rust** | 12 个 GDExtension 类的**知识**已还原（在 `src/` 里，是纯逻辑不是节点），但 `client/` 那 15,465 行 GDScript **仍是仿制品**；它们**没注册成 Godot 类**、也不调引擎 API。要真跑起来还差：引擎绑定最小面 + 注册 12 个类 + 补齐 `module_container`/`game_state_manager` 之外的大件 |
| 2 | **剩余 53 个可归属函数** | **39 个是 `weak_caller`**（gdext 绑定胶水，对"理解游戏"价值最低）；其余 14 个是极小模块（`lan_broadcaster_v6` 4 / `server_broker_handler` 3 / `mode_manager` 2 / `logger` 2 / 若干 0~1），两个 agent 正在收 |
| 3 | **整局逐位** | **71/400（17.8%）**。已知两大成因：① A 类熵（出场顺序、广播批内顺序）；② **死亡特效的 `Ash`/`Death` 分界**（今晚定位到"照抄出生登记"，见 §0.0.0，还没改进实现） |
| 4 | **`game.rs::fire` 的 `1e-12` 位置排除** | 二进制里只有按实体 id 排除（`docs/122`） |
| 5 | **`handle_tick` 第③段**（回 Setup） | 只有读点没有写点（`docs/14`） |
| 6 | **跨平台** | Win/Android `.gdextension` 条目已补，**没编译过**；跨架构逐位未验证 |
| 7 | **B 类非确定性** | 25 s 墙钟、多客户端竞态——永远做不到逐位 |
| 8 | **素材源文件** | 149 个 `.svg` / 7 个 `.ttf` **物理上拿不回来**（已用 2024 版 pck 验证过：包里只有 `.import` 的路径串） |

**一条工具债已还**：`class_probe.py` 的 `+0x1000` 已修（v2），并新增只读 `.so` 的 `loc_xrefs.py`；
受影响的旧地址按 `docs/16` §27.4 重核过 4 处。


## 1. 仓库与推送规矩

| 仓库 | 路径 | 远端 | 说明 |
|---|---|---|---|
| 上游 | `..`（`gw2/`） | `Cmd-GZ/graphwar2-analysis`（`master`） | 曲线算法报告；**绝不包含**本子仓库 |
| 本仓库 | `gw2-rebuild/` | `Cmd-GZ/graphwar2-rebuild`（`main`） | 源码复原 + 全部文档 |

- 推送必须用：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push`
- 自检：在上游执行 `git ls-files | grep gw2-rebuild` **必须为空**。
- `rsc/`（游戏资源）、`.pck`、反编译产物一律 gitignore（体积 + 版权）。

## 2. 环境坑（每条都真踩过）

| 症状 | 原因 | 处理 |
|---|---|---|
| 编译报无法访问 registry | `~/.cargo` 只读 | `CARGO_HOME=<工作区>/.cargo-home`，且 **`CARGO_TARGET_DIR` 必须单独一行赋值**（同一行展开会变成 `/target`） |
| 下一次命令看不到上次的日志 | **`/tmp` 是逐命令 tmpfs** | 日志一律写工作区 `.agent-work/`（**审计 2026-11-08**：原写 `.rea-work/`，不存在） |
| `pkill -f Godot` 把自己那条命令也杀了 | 命令行里含该字符串 | `ps -eo pid,args` 过滤后按 PID 杀 |
| `ss -tln \| grep godot` 匹配不到 | 不带 `-p` 就没有进程名 | 用 `ss -tlnp` |
| frida 注入报 binding 缺失 | REA 自带 frida 缓存是坏的 | `cd .rea-runtime && npm install-scripts approve frida && npm rebuild frida --foreground-scripts` |
| Godot 里改的脚本不生效 | `project.binary` 覆盖了 `project.godot` | 删掉 `godot-live/project.binary` |
| harness 里机器人不开火 | 回合时间太长 | 探针按 `TurnTimeButton` 缩短回合时间 |

Godot 统一环境变量：

```bash
XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache XDG_CONFIG_HOME=$PWD/.godot-config
```

## 3. 地址与坐标约定（错了会静默出错）

- **Ghidra 地址 = ELF 虚地址 + `0x100000`**；REA / frida 用虚地址。
- `.rodata` 段里 **文件偏移 == 虚地址**；`.text` 段两者差 `0x1000`
  （换算走 ELF 段表：`vaddr = sec.addr + (off - sec.offset)`）。
- 函数起点用 `analysis/funcstarts_ghidra_va.pkl`（已是 Ghidra 地址空间）。
  **`analysis/funcstarts.pkl` 是错的，永远不要用。**
- 坐标：`px = 20x + 500`、`py = 20y + 300`，画布 1000×600。

## 4. 已完成且有真值验证

| 行 | 关键证据 |
|---|---|
| 函数文法 / 求值 / 采点 | 真二进制对拍；真实对局轨迹 69/69 逐点一致 |
| 坐标 / 掩膜 / 光栅化 | 与 `0x4be3e0`、`0x4f11e0`、`0x4f15b0` 逐字节/随机点一致 |
| 构建 / 加载 / 类注册 / 打包 | 真 Godot 4.7.2 跑通；`.pck` 能从包启动 |
| 资源还原 | 692 文件 md5 全符 |
| 网络协议 + 编解码 | 46 种消息；序列化**逐字节一致** |
| 弹道与命中结算 `graphwar_shot_calculator::settle::resolve` | 整关回放（4 发 / 6 发）误差 < 采样步；致死集合逐发一致 |
| 命中与生命 | 穿过即死、不挡弹道；**无血量 / 有友军误伤 / 按弹道先后**（`15-target-life.md`） |
| 回合状态机 | 6 状态 7 事件，13 次迁移逐个相同 |
| tick 时钟 | 60 Hz；`end_of_turn_tick = 回合开始 tick + 时长`，六档实测 |
| 障碍延迟删除 | `QueueDestroyObstacle → DestroyObstacle` **165 tick** |
| 关卡生成 | 公式闭合 + **Rust 实现逐位复现 13/13**（`11-levelgen.md`） |
| 服务端握手 / `Match` / 一回合 12 条报文 | 与抓包逐字节/逐条一致 |
| **服务端循环（完整一局）** | `tests/server_match.rs` 真实 TCP 跑到 `EndingGame`（`09-network.md`） |
| **同时回合制结算顺序** | 4 局 13 回合 82 发：死亡集合 = 逐发独立并集；61 个被 ≥2 发穿过的士兵里 34 例可判别，**最早到达 34 : 固定顺序 0**（`14-turn.md`）。**修复采样器后在实况 0.01 下重验：82 发并集 152/152、多报 0 漏报 0、时刻判决 38:0**（`14-turn.md` §14） |

## 5. 核心结论速查（省掉重新推导）

**曲线**：`parse 0x4ed670` / `eval 0x4eb9d0` / `path 0x3c2530` / `correction 0x3c5f70`。
`step` = **`0.01`**（服务端常量 `0x3f847ae147ae147b`；**曾经全线误用 `0.5`，`06-verification.md` 已更正**）、
容差 `0.001`、点数 `(200/step) as i32`、边界 `|x|≤25 |y|≤15`、修正 `c = y0 − f(±x0)`。
二分规则见 §7.7：**复检不通过时 `lo = x` 且 `hi = mid2`**。

**掩膜**：1000×600 字节，`idx = py*1000+px`，`!= 0xFF` 命中，**越界也算命中**。
由 `obstacle_manager` 的 `0x523020` 创建（alloc 600000 + memset 0xFF + Arc）。
**摧毁 = 在命中点挖半径 0.5 的洞**（frida 抓到 `fill=0xFF` 的光栅化调用，`08-plane.md`）。

**协议**：UDP 广播明文 JSON 无长度前缀；TCP `u16` 大端长度 + JSON。
**同一进程里两端都在跑**，按 fd 区分方向：`fd 15` 客户端→服务端，`fd 16` 反向。

**tick**：60 Hz，`current_tick = base + round(ms*60/1000)`。
回合时长（tick）：`Timer30s=1800`、`1m=3600`、`2m=7200`、`3m=10800`、`5m=18000`，
`TimerInf` 线上发 **`0`**（哨兵）。

**关卡生成**（`11-levelgen.md`/`56`）：
`n = trunc(N(0,1)*7+15)`；`r = max(N(0,1)*1.6+2.6, 0.8)`；
`x = (2r+25)(2u−1)`、`y = (2r+15)(2v−1)`，`u,v = (u64>>11)·2⁻⁵³`。
`N(0,1)` 是 **ziggurat** 采样器（表在 `.rodata`：`X@0x25b428`、`F@0x25ac20`）。
生成从随机流**第 3 个**数开始；重放要消耗**不定数量**的随机数（拒绝采样）。

**RNG**（`12-rng.md`/`57`）：ChaCha（块函数 `FUN_00850f30`，虚地址 `0x750f30`）。
hook **`0x47f700`**（取随机 u64）即可读状态：`arg0` 是状态指针，
`[0..8]` = 32 字节 key、`[8..10]` = 计数器。播种**惰性**（首次调用前全零），
种子来自 **OS 熵**（游戏自己的 getrandom 包装 `GW+0x907c66`）。
**不同线程各有独立实例**（一局里能看到 2~4 条流）。

**AI**（`archive/ai/66`/`67`/`68`）：GW1 的进化算法，GW2 改了参数与适应度。

- 入口 `FUN_0061b440`（虚地址 `0x51b440`），签名
  `(ctx, ?, player_id, soldier_id, 代数, a5, ?, ?)`；**实参可直接 hook 出来**
  （`tools/hook_ai_args.js`，System V：rcx/r8/r9/栈）
- **每回合评估 = 代数 × (种群 + 3)**；种群 = `a5 × 5`；每代末尾对前 3 名用步长
  `0.01` 精评（粗评步长 `0.5`，即 `0x3fe0000000000000`）
- 六档参数（**实测到的实参**，不是推断）：

  | 难度 | 机器人 | 代数 | 种群 | 每回合评估 |
  |---|---|---|---|---|
  | 1 | ChatGPT | 3 | 50 | 159 |
  | 2 | Wolfram | 10 | 100 | 1030 |
  | 3 | Alpha Zordon | 25 | 150 | 3825 |
  | 4 | Multivac | 100 | 200 | 20300 |
  | 5 | Deep Blue | 250 | 250 | 63250 |
  | 6 | Deep Thought | 1000 | 250 | 253000 |

- 栈上第一个参数恒为 **5**（精英数，与 GW1 的 5 一致）
- 适应度：`命中×2e6 − minDist²/100 − (步数超限 ? 5e7 : 0)`
  （`2e6` 在 `.rodata 0x19fc00`、`1e6` 在 `0x19fb30`，引用点全在 AI 区域）
- **⚠️ 有两种函数模式**：`NormalFunction`（y=f(x)）与 `DiffEqFunction`（y'=f(x,y)）。
  权重表只在变量槽位上不同：Normal 下 10 个权重全给 `x`；
  **DiffEq 下 `x` 6 个 + `y` 4 个**（实测 6.83% / 4.86%，`archive/ai/71` §5.0）。
  `y'` 两种模式都不产生。Rust 侧已用 `graphwar_function_evolutionator::VarMode` 区分
- **token 权重表已从二进制直读**（`archive/ai/71`，`src/graphwar_function_evolutionator.rs`）：
  `idx = random % 85`，区间权重
  `const 11 | x 10 | + 10 | - 5 | * 11 | / 7 | ^ 2 | sqrt 1 | log 1 | ln 1 | exp 6 | abs 7 | sin 7 | cos 5 | tan 1`，
  常数 = `N(0,1) × 10.2`。**运行时 40 万样本逐项吻合到小数点后第二位**
  （`tools/hook_token_tags.js`）
- **变异算子**（`archive/ai/73`，`FUN_005ee720`）：挑一个值 token →
  `y` 插常数；`const` 一半概率 `*= (1+N(0,1))`（**乘性漂移**）、一半换新；
  `x`/算子则包一层 `(op const v)`（op 按 50/25/25 取 `+`/`*`/`/`，**这条是推断**）
- **长度公式两种模式共用**（`archive/ai/76` §2.6）：DiffEq 下重测均值 9.27（去掉 0/1 后 9.79
  ≈ Normal 的 9.81）；直接量公式输出可见两种模式都会抽到 0/1/负数
- **`limit` 与模式无关**（`13-ai.md`）：DiffEq 下实测仍是 5/5/…/200；
  低难度会发射超限函数是因为**长度均值 10 ≫ L1 的 limit 5**（P(>5)≈84%），
  而 `V>limit` 只是扣 5e7 的**惩罚不是过滤**
- **随机函数长度**（`archive/ai/76`，已从二进制读死）：**`trunc(N(0,1) × 5.0 + 10.0)`**
  （`0x4ef08e` mulsd 5.0、`0x4ef096` addsd 10.0、`0x4ef0b2` cvttsd2si；
  中间的 `maxsd/minsd` 是 ±2³¹ 的溢出保护，不是语义上下限）。
  已实现为 `graphwar_function_evolutionator::random_length`
- **适应度**（`archive/ai/74`）：`命中×2e6 − 1e6 − V²/100 − (V>limit?5e7:0)`，
  **`V` = 该函数的 token 数**（运行时 12/12 命中栈上 `[rsp+440]` 的向量）；
  **常规对局里没有友军惩罚项**（`X` 是采样步长 0.5/0.01，与它相乘的掩膜累加器恒为 0）
- **每代构成**（`archive/ai/75`，实测 8 代）：子代工厂 `0x4ee3a0` 的入口恒为
  **0.8 × 种群**；工厂内部：随机整数为负（~50%）走变异，否则按第二父代长度
  `< 20` 决定"重生随机函数"还是"交叉+变异"；**剩下 20% 不经过工厂 ⇒ 直接复制**
  （含 5 个精英）。⇒ GW1 的 5/25/20 三分结构**不适用**
- ⚠️ `archive/ai/68` 曾据 149 个样本推断"token 集不含 sqrt/ln/^"，**已被 `archive/ai/71` 推翻**
- 观察到 `1770417.48` 这种超大常数 ⇒ 存在**微调变异漂移**（GW1 的 `mutateFineTune`）

**探针**：`godot-live/probe.gd` 支持 `GW2_BOT_OPT=1..6`（六档）与 `GW2_TT_PRESSES`（回合时间）、
`GW2_START_FRAME`（延迟开局）、`GW2_SCENE_DUMP`（场景节点 dump）。

## 6. 明确未知 / 已证不可复现

| 项 | 状态 |
|---|---|
| 回合出场顺序 | **已证伪来自 ChaCha 流**（6 局 × 2 基准 × 4 方法；3 局 × 每流 × 3 方法 × 25 偏移）。最可能来自 **HashMap 迭代顺序**（SipHash 键由 getrandom 播种）⇒ 不可确定性复现（[`12`](12-rng.md) §6–§7） |
| ~~`V` 的语义~~ | **已确认**（`archive/ai/74` §5）：`V` = 被评分函数的 **token 数**，运行时 12/12 命中栈上 `[rsp+440]` 的 token 向量；`−V²/100` 是复杂度二次惩罚，`V>limit` 的 `−5e7` 是复杂度硬上限 |
| ~~20% 副本里除 5 精英外的选取规则~~ | **已结案**（`13-ai.md`）：不存在"两个父代"——每槽用"上一代第 i 名"当唯一父代，产 1 克隆 + 4 个工厂子代；工厂签名只有 `(out, &mode, &parent)` |
| 变异 `y` 分支会产生"两个值相邻"的非法前缀式 | 未解（可能调用方还有一趟修复，见 `archive/ai/73` §5.1） |
| 关卡生成的其他模式 | **已结案：只有两种**（`11-levelgen.md`）。早期 5 个候选定性见 `archive/ai/72` §5.1：散布 `0x3f1e50` 开局调 1 次（真生成器），`0x465a70`/`0x46f690` 紧跟其后（流程辅助），`0x464d10`/`0x478fc0` 在开局前（与关卡无关）。中心式已实现且有 **4 个真值样本**（`11-levelgen.md`）；所谓"圆盘式"其实不是关卡生成器 |
| ~~玩家颜色分配~~ | **已结案**（`10-campaign.md`）：两张各 6 色的**队色表**（Team1 暖 / Team2 冷），随机起点 + 沿表 +1 线性探测 + 跳过已用；单队 6 色满则兜底随机 RGB。replay 1358/1358 零例外 |
| 玩家加入流程 | 服务端只实现了单连接简化版（`09-network.md` §4） |
| 自建客户端连真服务器 | 端口找到（46868）但握手无应答；**很可能当时探针脚本有解析错误、游戏根本没开房**（`62` §5，需重试） |
| **Godot 客户端界面** | **控件实现 0 个**，但**契约已全**：`16-client.md` 用运行时 dump 枚举了 15/15 界面、283 个按钮、每个控件的文本/图标/作用/实现状态；复现 `bash tools/dump_ui.sh`。仍未观测：`function_buttons{,_no_control}` / `line_2d` 三个场景的实例化路径、回放播放、关卡 Start 之后（`16-client.md` §8） |
| **脚本逻辑（105 个 `.gdc`）** | **无可用反编译器**（离线检索无 gdsdecomp/GDRE/pip 包，且禁止联网）。已用 `tools/gdc_strings.py` 解开 `GDSC`+zstd 容器与**字符串池**（105/105）：51 个非插件脚本里 40 个是字号/间距助手、6 个空壳、5 个有行为（造关卡按钮、切换附加按钮、聊天聚焦、页签标题、最小尺寸），**没有一个是游戏规则** ⇒ "处理器逻辑在 `.gdc` 里"这一前提被推翻（`16-client.md` §2.3、§7） |
| 资源 / 音频 / 本地化 | 资源 692 文件 md5 全符；**音频只有 1 个 `sounds/humm.wav`**（无音乐、无音效库）；包里**无** `.translation`/`.csv` ⇒ 无本地化资源，文本硬编码在场景/脚本里。是否有音频系统**未确认** |
| ~~`AxisMode` / `TimeMode` 是否参与判定~~ | ✅ **已结案：不参与任何判定**（`88` §17：采样器/掩膜/结算/开火链签名里都没有这两个参数，AI 上下文构造也没有）。口述阶段的"把握较低"（§14/§16.3）已被签名证据取代 |
| ~~AI 的时间预算 / "精英 5" 的用途~~ | ✅ **已解**：终止 = 代数到顶 **∨ 25 s 墙钟**（`13-ai.md`，每代开头查一次，截断无标记）；精英 = 每代前 **20%**，字面量 5 = 每槽 1 克隆 + 4 工厂子代（`13-ai.md`） |
| `graphwar_function_evolutionator::evolve` 的 25 s 上限 | **未实现**（只按代数循环 ⇒ 难度 6 多算、结果依机器速度，`13-ai.md` §5.4）。另：`src/graphwar_function_evolutionator.rs` 里 `Params.elite` 这个字段名与注释已与 `13-ai.md` 不符（本任务禁改 `src/`） |

## 7. 正在进行：AI 与关卡

已达成：算法、适应度、六档参数、机器人名单、**token 权重表**、**变异算子**、
**随机函数长度**、**V 的语义**、**每代构成**，并已落成
**结构还原版 AI**（`src/graphwar_function_evolutionator.rs`：`params`/`random_length`/`mutate`/`crossover`/
`next_generation`/`evolve`/`fitness`/`initial_population`，18 条测试含端到端"6 代学会命中"）。

剩余：

1. ~~变异/交叉的个体数~~ **已完成**（`archive/ai/75`：80% 工厂 + 20% 复制）；
2. ~~`-5e7` 阈值~~ **已完成**（`archive/ai/74`：5/5/10/15/25/200，随难度放宽）；
3. ~~随机函数的长度规则~~ **已完成**（`archive/ai/76`：`trunc(N(0,1)×5+10)`，且两种模式共用）；
   **装配机制也查清了**：没有"修复"过程——`simplify`(`0x4ebbe0`) 是递归下降解析器，
   从头读一个完整表达式、**多余的 token 丢弃**（`archive/ai/76` §4.5）；
4. 关卡模式：**只有两种**（散布 + 中心单障碍），Rust 两种都已实现；`103` 证明不存在第三种
   ——`0x4ea6dc` 是图案分派器 `0x4ea690` 的 case 0，不调 `place_obstacle`、记录里**没有半径字段**、
   真实对局 **0 次调用**。散布生成器在真对局里**被调用过**（`06` §14.4 的 hook 计数 `levelgen 1+1`），
   逐位复现 13/13（`61`）；中心单障碍有 4 个真值样本（`103`）。
5. AI 的终止条件与精英机制**已解**（`13-ai.md`：代数到顶 ∨ 25 s 墙钟；`13-ai.md`：精英 = 前 20%）；
   **仍未做**：把 25 s 上限写进 `graphwar_function_evolutionator::evolve`（`AxisMode`/`TimeMode` 已由 `88` §17 结案，不参与判定）。

**批量提取真实函数文本**已打通：`tools/ocr_functions.py`（用户装了 `tesseract-data-eng`）。

## 7.5 统计验收（`13-ai.md`）

真实机器人函数的一元算子频率与权重表吻合（abs 10.2/10.9、sin 10.2/10.9、cos 7.7/7.8）；
二元算子因**进化选择压力**偏高/缺失，**不能用来验证权重表**。
权重表的可靠证据仍是二进制直读 + 40 万次运行时抽样。

## 7.6 同时回合制的结算顺序（`14-turn.md`）

**结论：几何并行、事件按飞行时间串行。**
每发弹道独立算（互不影响，士兵不挡弹道），弹丸按自己的速度飞，
**飞到谁就把谁标死、并从可命中集合里去掉；一个士兵一回合只死一次，归属=最先飞到他的那颗弹丸。**

判定数据（最新 5 个 replay，其中 4 个 SimultaneousTurns + 1 个 SequentialTurns 当对照）：

| 项目 | 数字 |
|---|---|
| 同时回合制 4 局 / 回合 / 发 | 4 局 / 13 回合 / 82 发（每回合 3~8 发）|
| 观测死亡 vs 我们逐发独立求并集 | 修复前（0.5）**152 vs 151，多报 0 漏报 1**；**修复采样器后（实况 0.01）152 vs 152，多报 0 漏报 0**（`14-turn.md` §14）|
| 被 ≥2 发穿过 | 61 人（2 发 48、3 发 10、4 发 3）|
| 可判别（最早到达 vs `FunctionPoints` 顺序预测差 >5 tick） | **34 例：34 : 0 指向"最早到达"** |
| 预测误差中位 | 最早到达 **1.55 tick**；顺序优先 **105.2 tick** |
| 死亡特效 | **152/152 每人恰好 1 个**；`LifeInfo` 每人 1 或 2 条，**从不 ≥3**，且与命中发数无关 |
| 固定顺序 | 3 个回合的偏序约束**出现环**；6 种候选顺序全部 ≤54% ⇒ **不存在任何固定顺序** |

* 速度：**0.29 单位/tick**（75/82 发能配上爆炸事件，p10~p90 = 0.255~0.300）；
  同一回合所有发的 `active_tick` 完全相同（13/13）。
* **不改物理**：`graphwar_shot_calculator::settle::resolve` 的"每发独立算"就是对的。
  要改的是**事件生成**：多发同时结算时死亡要按到达 tick 排序、每个受害者只报一次。
* 顺带发现的两个缺陷（与结算顺序无关，`14-turn.md` §9）：**9.1 已修复、9.2 已撤回**。
  1. **采样器"提前终止"**（实况步长 **0.01** 下 **19/82（23%）** 的曲线被截断；
     0.5 时 6/82、0.1 时 10/82）。
     最小复现 `(abs(cos((-1.43*x)))*8.65)` 从 (-1.3401, 7.5651) flip=false：
     step=0.5 → 70 点停在 (3.236053, 5.368069)；step=0.01 → 2078 点停在 (1.0963, 4.6614)；
     真游戏飞到 23.213。**注意方向**：该函数是偶函数，flip=true 那一支不早停（361 点）。
     另外真游戏自己也会半空停机（93 发里 6 发），规则未知；
     **→ ✅ 已修复（2026-11）：根因是二分复检不通过时漏了 `hi = mid2`，见 §7.7 与 `04-path-sampling.md`；
     `14-turn.md` §14 已用 0.01 重验：82 发并集 152/152、漏报 0。**
  2. ~~掩膜边界差 1 像素~~ **已撤回**：用实况步长 0.01 复算，我们的停点与游戏爆炸点
     差 1e-6~4e-4；0.5 下看到的 0.04~0.05 只是折线离散化。仅剩 1/82 的边缘像素个案。
* 复现：`python3 tools/sim_turns_analyze.py --newest 5` + `examples/sim_turns.rs`。

## 7.7 采样器二分循环：`lo`/`hi` 规则已从汇编读死，**且已改码验证**（`04-path-sampling.md`）

上面那条"采样器提前终止"**已修复并验证**（`04-path-sampling.md` 有逐条指令清单）。

**规则**（`0x3c2530` 内，normal 模式；全部逐位验证 = 读指令）：

```
mid  = (lo + hi) * 0.5
chord= hypot(mid - x, f(mid) - y)          # 用基点 x，不是 lo
if not (0.001 > |chord - step|):           # 0x3c29d9 ucomisd + jbe（严格 <）
    if chord > step or isnan(chord): hi = mid      # 0x3c2a86
    else:                            lo = mid      # 0x3c2950
else:
    mid2  = (x + mid) * 0.5                # 0x3c29ee（用 x）
    chord2= hypot(mid2 - x, f(mid2) - y)
    hi    = mid2                           # ★ 0x3c2a0a，先写后判
    if not (chord > chord2): lo = x; continue  # ★ 0x3c2a47 退回基点，hi 已被砍到 mid2
                                       #   判据是 `!(L > L2)`，不是 `L <= L2`：
                                       #   `ucomisd` 遇 NaN 也置 CF=ZF=1 ⇒ `jbe` 成立 ⇒ 走退回
    else:                accept(mid)
```

* 一句话：**复检不通过时 `lo` 回基点 `x`、`hi` 砍到 `mid2`**。
  （历史）两份实现（`graphwar_curve.py` / `graphwar_path_calculator.rs`）曾都只做了 `lo = x`，**漏了 `hi = mid2`**
  ⇒ 拒绝后区间宽一倍 ⇒ 从第 4 点起走另一条轨迹（`06-verification.md` §26 的三方对照）。**现已双双补上。**
* 计数器：**`ebx`**，`0x3c293f` 置 100，`0x3c2955 dec`，`0x3c2957 je` 直接返回（= 早停）。
  首次进入绕过 `dec`，所以恰好 100 轮。
* ODE 分支**没有**这段复检（`0x3c2840` 直接接受），不要一起改。
* 改法（**已完成**）：`graphwar_curve.py` 第 471–473 行、`graphwar_path_calculator.rs` 第 92–95 行各加一行 `hi = mid2`，
  并把判据写成 `not (ln > ln2)` / `!(chord > chord2)`（`jbe` 遇 NaN 成立）。
* **验证已做完**（`06-verification.md` §32 记录）：
  - `analysis/gwemu.py` oracle 24 个用例，**24/24 整条点序列逐位相等**（原实现 17/24），
    含 `06-verification.md` §11 的 `(401, 24.8098, 9.8719)` 与 `(20001, 23.2128, 6.4176)`，第 4 点 `-1.174402651902936`；
  - 父仓库 `difftest.py` **768 例全过**、`fuzz.py` 120 例 0 失配；
  - 本仓库 `cargo test` 全绿（当时 135 条，现 **150** 条），`live_hits` 12 条无退化；
  - `14-turn.md` §14 用 **0.01** 重跑同时回合制：82 发并集 **152/152**、多报 0 / 漏报 0。
* ✅ `tests/live_hits.rs` **已切到实况 0.01、12 条全过**（`06-verification.md` §33）。当年切 0.01 后的 4 个失败
  **不是物理问题**：`first_hit()` 跳过射手身体的半径固定 0.5，而掩膜里的士兵是像素量化圆盘
  （边界抖 1 px = 0.05），0.01 下有点落在 0.507 处被误判成撞到自己；改 `SHOOTER_SKIP_R = 0.56` 即解。
* ⚠️ **仍未做**：`chord2 = NaN` 那条分支与汇编的等价性**未实测**（`04-path-sampling.md` §9.1）。

## 7.8 用户口述的 GW2 行为（7 条，已落进 `21-guide-manual.md`）

用户逐条回答了"指南里哪些在 GW2 还成立"。**这些是用户口述 + 部分可验证**，引用时标注来源。

| # | 结论 | 可验证性 |
|---|---|---|
| 1 | **没有 `y''` 模式** ⇒ 上下箭头设发射角、发射角轮盘在 GW2 **都不存在** | 与 `AGENTS.md` 的"只有一阶"一致；`YButton` 有控件但两模式都 disabled（`16-client.md`） |
| 2 | **回合超时 = 跳过该玩家** | 未独立验证；影响 `src/graphwar_game_server.rs` |
| 3 | **Normal 模式写 `y` 不判非法，等价于 0** | ✅ 与我们的 parser（永远接受 `y`）+ 反汇编（`y'` 恒 0）一致；用户已实测 |
| 4 | **士兵朝向 = 队伍**：`x<0` 朝右、`x>0` 朝左 | ✅ 与 `10-campaign.md` 的 773 个位置样本一致；朝向本身未在节点树里观测到 |
| 5 | **等待界面与 GW1 完全不同**：**没有倒计时**，房主点 Start **立即开始** | 与 `16-client.md` 的 `lan_lobby`/`GameSetup` 台账一致（Start 就直接进 PlayScreen） |
| 6 | **能聊天**，顶部有聊天按钮 | ✅ `16-client.md` 有 `ChatButtonSetup` / `ChatPopUp` / `ChatBox` |
| 7 | 指南的**公式/技巧**部分源自 GW1 算法研究 | 用途：**对照 GW1 与 GW2 的算法差异**即可判断哪些仍适用 |

### 注意别混淆两件事

- **大厅等待界面**：无倒计时、房主点 Start 立即开始（本轮用户口述）；
- **同时回合制（`SimultaneousTurns`）内每回合的"准备完毕"**：用户此前说过
  "fire 的语义变为准备完毕，所有玩家准备完毕时子弹同时发射"——这是**回合内机制**，
  与大厅等待是两回事，不要合并。

## 7.9 用户补充的事实

### 7.9.1 用户口述的残留项现状

| 项 | 用户说明 |
|---|---|
| **音频** | `.pck` 里**有一个文件是损坏的**；其余未细看 |
| **多语言** | **GW2 没有多语言支持** |
| **字体** | **需要处理**（不是可选项） |
| **广告 / IAP** | Linux 端**应该没有**；可去 **APKMirror 找 GW2 的 APK** 探索 Android 侧 |
| **房间列表来源** | 用户也不清楚；**服务端与房间服务器代码拿不到** |
| **第 0 代初始种群** | GW1 做法 = **随机凑表达式 → 用某函数裁剪成合法表达式**；GW2 是否相同**未知** |

### 7.9.2 给后来者的提醒

- 客户端素材：`.agent-work/tools/recovered/`（**不进 git**：31 `.tscn` + 105 `.gd` + `.tres` + 字体 + svg）；
- 内核两条缺口是**纯实现**，结论已在 `13-ai.md`，不需要再逆向；
- **临时产物禁止进 git**：`.rea-work/`、`.sim-work/`、`.agent-work/` 都已 gitignore
  （本轮曾误传 49 个文件，已 `git rm --cached` 清理，见提交 `e03c8e6`）。

---

## 8. 我犯过的错（别再犯）

**地址 / 反汇编类**

1. 把虚地址当 Ghidra 地址传工具 → `not_found`（差 `0x100000`）。
2. 把文件偏移当虚地址 → 掩膜归属算错（`.text` 差 `0x1000`）。
3. grep 反编译文件里的浮点常量 → 被文件头版本号 `12.1` 污染。
4. 相信 `analysis/funcstarts.pkl` → 与 Ghidra 交集仅 2/13430。
5. 从局部伪代码推取整方式 → 把"截断"说成"四舍五入"；必须用输入输出对拍。

**结论 / 证据类**

6. 只看系统调用次数、没看**调用栈** → 把 libudev 的 921 次 `getrandom` 当成游戏的，
   得出"OS 熵 ⇒ 关卡不可复现"的错误结论（`11-levelgen.md` 已被 `12-rng.md` 推翻）。
7. 用 3 局样本得出"命中"就下结论 → 扩到 6 局立刻消失，是统计噪声。
   **小样本一律先算假阳性期望**。
8. 凭函数内部特征猜用途 → 把 `0x567530` 猜成"AI 选弹道"，实际是关卡通视检查。
9. 把"障碍被整块移除"当成机制写进文档 → 被数据否定后才改成"挖洞"（`08-plane.md`→`08-plane.md`）。
   **推断必须标明；能用实验否定就要去否定。**

**工具 / 脚本类**

10. 一次 `str.replace` 改文档没核对 → README 索引只更新一半、某行没改成功（静默失败）。
    **每次替换后都要 grep 验证**。
11. `/tmp` 当持久目录 → 下一条命令就没了。
12. `pkill -f <名字>` → 连自己那条命令一起杀。
13. GDScript 的 `elif` 顺序写错（`frames >= 600` 先匹配，`frames == 720` 永不执行）。
14. 探针脚本有解析错误 → 游戏照常"运行"，但**整局根本没开始**；我却在对着空环境调网络。
    **先用一条 print 确认脚本真的加载成功。**
15. 用 hook 计数做性能/预算结论 → 难度 6 要拦 25 万次评分，**instrumentation 把游戏拖慢**
    导致测到的数是被自己截断的（`archive/ai/67` §2）。**先确认 hook 不改变被测行为。**
16. AI 参数靠"评估数整除"反推（虽然后来对上了），正解是**直接 hook 实参**
    （rcx/r8/r9/栈），一步到位。**能读输入就别猜输出。**
17. 用户录屏里右下角数字被我当成回合计时，实际是**视频总时长**。**图上读数不可靠，
    要问具体数值。**
18. 用 149 个 OCR 样本里"出现 0 次"推断"算子不存在" → 被二进制打脸（`archive/ai/68` 被 `archive/ai/71` 推翻）。
    **低频事件在小样本里为 0 是常态，不能当否定证据。**
19. 直接拿函数起点表里的地址去 `Interceptor.attach` → 可能不是指令边界，**attach 会抛异常**，
    且**异常会中断同一段脚本里后续的 attach**（我用一个 try/catch 都没包的脚本踩到）。
    **每个 attach 都要单独 try/catch。**
20. 把 Ghidra 地址当虚地址去 hook → 命中了另一个每帧都调用的函数（3514 次），
    差点得出"散布生成器每帧都在跑"的相反结论。**这是第二次踩同一个坑（第 2 条也是）。**
    现在工具脚本里一律写注释标明"虚地址 = Ghidra − 0x100000"。

## 9. 文件地图与常用命令

```
src/math_function/   文法与求值        src/_scaffold/protocol.rs  46 种消息 + 序列化
src/graphwar_path_calculator.rs     采点              src/graphwar_shot_calculator/settle.rs    命中与致死结算
src/plane.rs    坐标/掩膜/光栅化   src/game.rs      Match（一回合报文序列）
src/graphwar_shot_calculator.rs     修正+开火原语      src/turnstate.rs 回合状态机
src/graphwar_tick_handler.rs     60Hz + 回合时长    src/graphwar_level_generator.rs  关卡生成（含 ziggurat）
src/_scaffold/rng.rs      ChaCha12          src/graphwar_game_server.rs 服务端循环
src/server.rs   握手/状态同步      src/gdext*.rs    Godot 扩展
tests/          events / evidence / golden / live_hits / plane / server_match
tools/          pck_extract / mask_check / simulate_check / hook_* / capture_* / predict_obstacles
docs/           00 状态总览、16 复现方法、49 验收清单、62 服务端、AI.md（AI 权威）、88 覆盖面、91 二分循环
```

```bash
export CARGO_HOME=$PWD/../.cargo-home
export CARGO_TARGET_DIR=$PWD/../.cargo-home/target
cargo test --offline                      # 150 条，无需游戏二进制
cargo run --example serve_match -- 127.0.0.1:39993 &
python3 tools/client_match.py 39993       # 打完整一局

node tools/capture_rand_values.js         # 抓随机流 + 真实报文
python3 tools/predict_obstacles.py        # 关卡重放 → 应输出 "完全一致 13/13"
```
