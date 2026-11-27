# HANDOVER —— 给上下文压缩后的自己

> **这是一份 cache，不是历史。** 目的只有一个：让被压缩掉上下文的"我"能在五分钟内
> 恢复判断力，不重复已经走过的死路。**只放"现在必须知道"的东西**；过程与逐例证据在
> `docs/06`（验证）、`docs/02`（模块）、`docs/00`（状态总览）、`dispatch/reports/`（每轮报告）。

---

## 0. 一句话任务（改不了）

**还原 Graphwar II 的源代码**（不是造克隆）。原版是 Rust + Godot 4.7 GDExtension，
`libgraphwarrust.so`（sha256 `9a538115…`，stripped）。**行为一致只是第一步，源码级还原才是终点。**
判据见 `AGENTS.md` §0（六条硬标准）——尤其：**`cargo test` / `selftest` 的条数是回归保护，不是复原证据。**

我的角色（2026-11-26 起）：**主线 `master` 的负责人** —— 定位缺口、派 teammate、复核数字、合并、写文档。
派活走 **agent teams**（共享任务板 + 各自 worktree/分支），合并仍按 `AGENTS.md` 合并纪律 A/B/C/D。
⚠️ **`dispatch/` 那套任务书/报告流程已退役**（它只在旧的 CG 下游分支上；master 里一份都没有），规格/交付直接写在工作区 `.agent-work/specs/` 与文档里。

---

## 0.0 **优先级（用户 2026-11-26 明确纠正过，别再漂）**

**终点 = 源码级还原**（`AGENTS.md` §0）：文件 / 模块 / **函数边界** / **数据结构**能对上。
'行为一致'、'像素 0 px' 都只是**第一等证据**，不是目标本身。派活/报进度时按这个顺序说话：

```
P1  控制器迁回 Rust（§0.4 明写）—— client/src/screens/*.gd 现有 ~7,050 行**仿制品**，
    原版这些控制器在 Rust 里（play_screen_control / entity_drawer / player_screen_control …），
    我们已经把这些模块还原了一大半却还在跑 GDScript 仿制流程 ⇒ 这是**最大的源码级欠账**
P2  数据结构还原 —— 结构体字段集合/顺序/偏移/大小 与真二进制逐项对上（例：T28 查出评分 sret 是
    24 字节 4 字段 {score@0, dist@8, hits@0x10, esi@0x14}，我方只有 3 个）
P3  模块/函数边界 —— 10 条 mismatch 已按 Location 切出；`multi` 86 条待数据流判定；
    19 个模块还没有同名文件；`ours_funcs vs orig_funcs` 的差异要逐个解释
P4  命名 —— **实数 400/527 具名（T87 用 Android 未剥离 .symtab 补了 52 条）、127 无名**（105 多名待定 + 1 无引用 + 21 判为非游戏 crate）
     ⚠️ **三个数（52 / 47 / 400）以前对不齐，现在统一了**：`52` = T87 定名条数；`47` = 其中**以前实际能进计分**的
     （5 条被 T82 的 `na` 台账压住）；`400` = `348（v1∪v2∪windows_name_to_addr 三份名字夹具并集）+ 52`。
     T89（`docs/06` §137）把那 5 条改判成 `real-missing` 后，工具也报「叠加 52 条」⇒ 三处一致，`348∪52 = 400` 实测成立。
     ⚠️ 另外两个口径**不要混用**：`fixtures/boundary_audit.tsv` 的 name 列只认 v2+hybrid（v2 真名 312 地址 + 预测 177 = 489/38）；
     348/179 是"三份夹具并集"口径；400 是"并集 ∪ T87"。报数必须说清是哪一种。
     ⚠️ 同址多名已裁决（`docs/06` §137）：`fixtures/name_to_addr_v2.tsv` 现在**一个地址只有一条 `role=outer`**，
     被内联掉的别名降为 `inline_alias` 备注；`boundary_audit.py` 以前「文件序最后一行赢」挑错 7 处名字，已修。
     ⚠️ **但要看穿**：那 52 条新名字只对应 **7 个不同源码函数**（`get_module_wk` 一条占 20 个机器码副本）⇒ 机器码 +52 / **源码级 +7** / 严格 1:1 **+0（R6 244→242 反而降）**。
     原理性上限：两份构建函数数 12 607（android）vs 14 812（linux），**单态化副本的份数/顺序不保证相同** ⇒ 族内实例级 1:1 没有可用不变量，105 条多名是"分不开"而不是"没挖"（`docs/06` §136）。
     **全局缺口（T86 清算，`docs/06` §134）**：原版 `.text` 可分析函数 **13 263**；其中 **na 9 104 = 68.6%**（std 单态化 1 494 + gdext 宏产物 1 456 + **na:other 6 154**）、
     `real-missing` **542**（抽 10% 复核，假阳性 17.0% ⇒ 纠偏 ≈450）、`unknown` **3 208**（信息天花板）、`implemented` 409。
     ⇒ **真正还差 ≈555 个函数（区间 500–600）**——游戏本 crate 在 Android 符号表里 **1 244** 条 = 797 具名 fn + 167 闭包 + **280 编译器 drop glue** ⇒ 需手写 ≈964、已实现 409 ⇒ 差 555。
     **不是 53/65**（那只覆盖 527 内部）、**也不是 12 736**（其中 ~90% 是 std/三方/gdext/C++）。最集中 5 模块：`module_container` / `game_state_manager` / `setup_control` / `root_node` / `event`。
     ⚠️ **537→885→1 085 的关系**：`885` 是 `docs/02` §10 的**历史读数**（441 观测 + 444 推断），差额 358 = 444 条只有推断无锚 − 之后新找到的 86 条锚；同规则用今天的 527 当种子重算 = **1 085** ⇒ 885 既不是"看过 885 个"也不是上限。
     🔑 **钥匙**：Android 2.0.60 的 `.so` **`.symtab` 未剥离**（实测 18 793 个 FUNC 符号）⇒ 12 609 条 `.text` 函数有真名，可用于给那 179 条无名函数定名
     （T87 **已做完**：**52** 条定名 + 21 条判非游戏 crate + 106 条判不了 = 179，见 `docs/06` §136；T89 收口见 §137）。
——  证据类工作（像素对拍 / 逐位对拍）按"它为哪条源码级问题提供了判据"来排序，不单独追指标
```

**别再把"像素全 0"当主目标**：像素对拍的最大价值是**抓源码级复原错误**（已抓到 `update_sprite` 的 `scale.x` 漏 `k`、
`name_label_layout` 实参语义错、`draw_set_transform` 棋盘单位、底栏最小高 96 vs 98 等）——用它的产出反推源码，而不是为了让截图一样。

### 0.0.0 **官服抓包已完成**（2026-11-27，用户 2026-11-26 授权）

真客户端 2.0.60 + 真 `.so` 连真 broker `69.164.213.144:42970`，三会话 **1717 条真帧 / 56 种事件**（本地完整一局、官服房间
`US Room 1 @69.164.213.144:37933` 完整一局到结局、broker 大堂静置）。产物 `fixtures/protocol_live_frames.jsonl`（1718 行）
+ `protocol_live_expected.tsv` + `protocol_zero_frame_events.tsv` + `protocol_enum_domains.tsv`；`docs/06` §110 / `docs/07` §7.8。
- **升档 6 类 → `观测`**：`LockInfo`(2) · `LockRequest`(2) · `ChatMessage`(1) · `ChatMessageRequest`(1) · `GameRemoved`(1) · `EndingGame`(1)；
  **与 `WIRE_SPEC` 逐项差异 0 处**（键序/字段名/类型三判据机判）。
- **两条口径修正**：`EndingGame` **不是线上事件**（线上是 `GameStateInfo{"game_state":"EndingGame"}`）；
  `ClearBans`/`BanConnection` 线上名带 **`Request` 后缀**（tag 73/72）。
- **证伪 `TurnLimitInfo` 零帧之说**：T46 当年在 **gzip 压缩字节**上 grep ⇒ 假阴性；解压重扫 403 份得 **21 帧**。
- **仍抓不到 3 类（写清缺什么）**：`GameHeartbeat`（1717 帧全无，含官服完整一局 ⇒ 推断由客户端之外的组件构造；
  客户端写方向挂 `write` 会自噬递归）、`ClearBansRequest`/`BanConnectionRequest`（要"房主+加入者两实例+被踢者"那一局，或本版客户端根本没有 UI 入口）。
- 新增硬闸测试 `captured_samples_are_the_live_wire_bytes`（把真帧字节编译进测试，改 1 字节即红）。

### 0.0.1 今天（2026-11-26）这一程干了什么（下一条会话先读这段）

**用户纠正了两次方向，两次都对，别再犯**：
1. "这是源码级还原项目，不是行为重建" ⇒ 已写进 §0.0 优先级梯子。
2. "既然你都能在 `AGENTS.md` 白纸黑字下犯错，别的 agent 也可能" ⇒ **派出 5 个独立审计面**，
   产出 **[`docs/AUDIT-2026-11-26-source-vs-behaviour.md`](AUDIT-2026-11-26-source-vs-behaviour.md)**（人可读、含"哪些真奔源码、哪些把行为当终点"）。

**审计的判决（要记住结论，别重跑）**：
- `client/` = **原版视图脚本 + 我们的控制器**；`client/src/` 生产 14 000 行里 **A 353 / B 8 532 / C 4 941**，**游戏规则达 A 档 = 0 行**；原版 **31 份 `.tscn` 当时一份没进**（现已 **16/16 全进**）。
- `src/` = **原版文件名 + 我们的内部结构**；生产函数 2 388 里 **48.7% 在 `_scaffold`**（T49 已搬回 4 处、1191→1150）。
- 测试：1772 条里 **341 条纯自洽**，**真正逐值对拍仅 ~9.6%**；`client/tools/*_check.gd` 那 1 961 条**从没审过**（T50 已审，修 24 条 M 镜像 → 1978）。
- 文档：抽样 122 条，**27.9% 档位与物证不匹配**；最不可信的恰是最常读的三篇（`docs/00`/`docs/16`/`HANDOVER`）——已按清单改正。
             **函数级**：A `312/527 = 59.2 %`（含 na，分母 527）／B `312/438 = 71.2 %`（剔 na，⚠️ 招牌数）／B′ `312/445 = 70.1 %`（只剔强 na）
             （2026-11-27 T89 收口：F1 312 / F2 71 / F3 144；`docs/06` §137 逐项写明 309→310→313→312 每一步的来源）
             —— **A/B 必须同时报**（`python3 tools/function_coverage.py --stats` 一次打全）；`docs/06` §129 是这套口径的审计：F3 的 153 条里只有 **53 条是 real-missing**，88 条不该我们手写，
             **7 个模块的 F3 是 100% 的 na** ⇒ 按"条数"排工作量会一半白干。台账 `fixtures/f3_triage.tsv`，判定可 `tools/f3_triage.py --control` 复算。
             ⚠️ **别再用 `312/439 = 71.1 %` 那个数**（T85 之前这里写的就是它）：分子含 33 条 na 假抬、分母却把同一批 na 剔了 ⇒ **双向占便宜**，虚高 7.5 pt。
             T85（`docs/06` §131）加了 P2 负向排除：判 `na:*` 的地址出现在我方注释里**不再算 F1**（实测 33 条假抬）⇒ A 从 `312/527` 降到 `279/527`。

**今天新增的工具（别重复造）**：`tools/docsec.py`（按节号合并 docs，**同号拒跑**，为此丢过一节正文）、
`tools/headless_audit.py`（headless 断言 M/C/G 分档）、`tools/function_coverage.py`、`tools/struct_audit.py`、
`tools/audit_src.py`、`tools/audit_client.py`、`tools/audit_tests_scan.py`、`tools/file_structure.py`、`tools/ai_gen_parity.py`。

**今天最大的教训（我自己的）**：`--flow replay` 不加 `--seed-replays` ⇒ 两侧都拍空回放列表 ⇒ **假 0 px**。
我据此写了"回放两屏归零"，被 t47 的独立发现推翻。已给 `frame_capture.py` 加硬闸、写进 `AGENTS.md` 的坑清单，
并把"**任何两侧都停在同一个错误画面的采集都会假装通过**"作为通则记下。**凡是"突然变 0 / 突然变完美"的读数，先怀疑采样**。

**收尾前新增的一条已解决结论（T51，`docs/06` §101）**：回放屏 **2.710 % 是真回归**（正对照证明 T21 的 0.1858 % 不是空画面）。
bisect 把窗口收窄到 **(a3ed8c7, 5f74a29]**；**60 367 px 里 98.6 % 是右三键列整体下移 48 px**
（`RightControls` 的 `save_screenshot`/`save_summary`/`save_clip`：宽 337、高 96、步距 100 都与原版逐项相同，
只有 y 差 48；病根指到 `_set_replay_layout()` 的 `_fire_row.custom_minimum_size` 补丁，**推断**无指令证据）。
**下一会话第一件事**：从 `564d558`(T44) 起验（窗口内唯一整体替换 `play_screen` 节点树的 merge），
用 selftest dump `RightControls` 5 个节点的 `rect` 定案后修到 **≤1 714 px / L2 0**。
⚠️ **新坑**：共享 `CARGO_TARGET_DIR` 跨 worktree 会让 `cargo build` **静默不重建**（7 棵树拿到同一个 `.so`）⇒ 会测出 59.99 % 的假巨大差异；每点必须 `touch` + 核对 `.so` md5，或每树独立 target dir。

**收尾时仍在跑的两条（下一条会话先看它们的通知或分支）**：
- `agent/replay-regress`（t51）：回放屏 2.710 % 的 **bisect** —— 先判"记录的 0.186 % 是不是也是空画面"，
  再定位 2.7 % 属于哪一层。`fixtures/replay_regression_bisect.tsv`
- `agent/f3-client-handler`（t52，**已合并**）：`client_connection_handler` F3 **7/8→0/8**（7 条按 vaddr 落码）；
  仍剩 `setup_control` 9 条 + `game_client` 2 条（台账 `fixtures/client_handler_anchors.tsv`，仍在板上）。

---

## 1. 现在的数（报数**必须带轴名/前置条件**，否则等于没报）

```
可归属函数   **模块级** 527/527（每模块都有文件+档位；逐位 89 / 观测 435 / 推断 3 是模块档位加权，`coverage_matrix --check` 打印的就是这一组）
             **函数级**：A `312/527 = 59.2 %`（口径 A：分母含 na）｜B `312/438 = 71.2 %`（口径 B：剔 `na:*`，⚠️ 招牌数）｜B′ `312/445 = 70.1 %`（2026-11-27 T89 实测：F1 312 / F2 71 / F3 144；`python3 tools/function_coverage.py --stats` 一次打全。历史：`docs/06` §87 首次 131/527 → §132 T85 279 → §136 T87 309/439 → §137 T89 312/438）
             ⚠️ **A/B 必须同时报**：`docs/06` §129 审出 F3 的 153 条里只有 **53 条 real-missing**、88 条不该我们手写（12 std 单态化 + 69 gdext 宏产物 + 7 同体泛型），**7 个模块的 F3 是 100% 的 na** ⇒ 只看口径 A 会虚低。台账 `fixtures/f3_triage.tsv`，判定可 `tools/f3_triage.py --control` 复算。
             ⚠️ 别再用 `312/439 = 71.1 %`（T85 前这里写的就是它）：分子含 33 条 na 假抬 + 分母剔同一批 na = **双向占便宜**。T85 §131 的 P2 负向排除把 A 从 `312/527` 拉到 `279/527`；旧口径要复跑就加 `--no-na`（逐字节复现，见 §131.3）。
             （2026-11-26：`polish_notation_converter` 唯一可归属函数 `0x4f5450` 对拍成 18/18 逐位 ⇒ 88→89）
             **439 里本质可隔离执行的只有 129（29.4%）**，其余 310 要 Godot/分配器 ⇒ 这是「观测」分母的可及上限（`docs/06` §64）
             但这只是 13,263 个 Ghidra 函数里的 4.0% —— 见 §3「信息天花板」
真名         348 / 527（三份符号表并集）；无名 179 = 138 族级 + 41 未知
名字候选     混合定名器（T1，2026-11-24，`docs/02` §35.12c）：13,263 行输出 mode=order 31 / fp 1,558 / none 11,674
             （fp/order 都是**推断级候选名，逐条待核**）；近孪生族留出 56.8%→**95.5%**、可部署域 57.8%→93.3%、族外 −1 行（已定位）
协议         74 / 74 全字段 + 74 条判别值 + 7 个线上枚举全集（无未知）
GDExtension  **原版 12 类**（`docs/02` §16.2）+ 我方 3 = **15 个，全部节点化**；headless 14 个 SceneTree 脚本 / **1933** 条 / 0 failed（2026-11-26 本机实跑；play_flow 233）
cargo        **1520** / 0 / 1 ignored（`--features gdext-bindings` **1650** / 0 / 1）；2026-11-27 收尾实测。
             ⚠️ **计数口径变更（T66）**：旧读数 1853/1983 里含 **356 条 `#[path]` 重复实例**（同一 src 单元测试被多个测试二进制重复编译）；
             T66 把 14 个 `#[path]` 取源改成真 crate 引用后，每个测试二进制只跑自己顶层 `#[test]`（17/17 实测相等）⇒ **以后基线用 1520/1650**，别再与旧数比大小。
selftest     **717 / 0 / 1 skip**（2026-11-26 本轮实测；判据 = `missing=0` 且 `silent=0`）
headless     14 个 `client/tools/*_check.gd`（`extends SceneTree`）合计 **1978 / 0**（T50 去镜像后 1961→1978）
             **四个前置**：① `client/assets/` 齐 ② `cargo build --example serve_match` ③ 可写 `HOME`
             ④ 真回放在 `$HOME/.local/share/godot/app_userdata/Graphwar/replays/`
             缺 ④ = `654 / 0 / 3 skip`（missing=63 全 degraded / silent 0）；缺 ① 的三档历史读数是 591/667/618。
             **别预设 `XDG_*`**（预设到工作区外 ⇒ `user://` 不可写 ⇒ 632/7）。命令与判据见 `docs/18` §10.3.4

客户端像素   **15 屏有真值**（`docs/06` §53–§101）。⚠️ **2026-11-26 更正：此前「八屏 0 px / 回放两屏归零」是假通过** —— `--flow replay` 不加 `--seed-replays` 时两侧都拍成空回放列表（亮像素 0.0 % vs 播种后 55.6 %），空 vs 空 = 「0 px」。
             **带 seed 的真实读数（2026-11-27 起 = 按 tick 停帧口径，见 §120）**：`menu` **0.000 %**、`replay_lobby` **0.000 %**、**`replay_play` 856 px = 0.0384 %（L2 0，三轮逐 px 相同）**、**`replay_end` 856 px = 0.0384 %**、**`campaign` 0.172 %（L3）**、**`level_popup` 0.101 %（L3）**、`player_skins` **0.000 %**。
             **对局屏** `match_ready` **0.0919 %（L2 0）** / `match_play` **0.26 %（L2 1）** ｜ 结算框面板区 0 px。
             ⚠️ **`replay_play`/`replay_end` 旧读数 2.710 %/2.723 % 是 T54 修掉右三键列 48 px 平移之前的数**（现在 0.0384 %）；`match_ready` 的 0.081 %/0.094 % 是**旧客户端**（play_screen 还没换原版 `.tscn`）那轮的读数，现在换 tick 停帧后是 0.0919 % 且**逐轮可复现**。
             **原版 `.tscn` 已装 16/16**（逐字节）；`screens/*.gd` 手抄树累计删 ≈1 900 行。
             ⚠️ **采集六条前置**：①`--flow replay` 必须 `--seed-replays`（否则假 0；`frame_capture.py` 已加硬闸）②UDP 42968 可能被泄漏 scope 占（→ 私有 netns）③新树缺 `client/.godot` ⇒ 空白帧（`godot --headless --path client --import`）④改 `src/` 必须重建覆盖 `.so` ⑤两侧**串行** ⑥**停帧按 tick 预注册**（`fixtures/frame_capture_freeze_plan.txt` + `frame_capture.py --stop-plan`，见 `docs/06` §120）—— 不按 tick 停帧时 `replay_play`/`replay_end` 会在 856 / 1126 px 之间跳（差的那 270 px 是我方回放屏多画的闪烁光标，**不是**渲染差异）。

**两张最容易混的口径**：`527 vs 13,263`（可归属 vs 全部函数）、`1075 vs 647`（种类 vs 内容）。

---

## 2. 已经**关掉**的路（别再花时间，这是本文件最值钱的部分）

每条都做过实验、都有数。**引用任何"假设"之前先看这里。**

| 方向 | 判决 | 关键证据 |
|---|---|---|
| "老版本 replay 打的所以对不上" | **否** | 用户新录**当前版本**的 bot-vs-bot，一样差 1 ulp；unicorn 直调真 `.so` 给出的是**我方那个值** |
| "回放自身不自洽"（407 批） | **成立但被收窄** | 只有 **211 批**是值级互斥（任何忠实实现都产不出）；其余是机制（两次读、同批挖洞） |
| "换个点 / 换精度 / 换步长" | **否，一整族** | 29 档假设全扫：**没有一档超过 647，fix 全 0**，且量级不匹配（差 1e-9 vs 换点差 0.01） |
| "打印-回代链丢了精度" | **否** | 167/280 批是 DiffEq，`corrected_function == function` **184/184 相同** ⇒ DiffEq 局根本没有常数 |
| "修正常数 token 差 1 ulp" | **否** | 三样同抓：常数 token vs 真机印字 **21/21、0 ulp**；数组未被改 20/20 |
| "1 ulp 出在 AI 开的那一发" | **仍未复现** | LAN+AI 局造出来了、AI 真开火；**直接量**"喂进 `0x3c5f70` 的 `&Point`" = `start_x`，**3/3、0 ulp** |
| "补间接调用图能提准确率" | **否（边际 0）** | 判别力 −0.126 → **+0.696**，但叠到 REF：78.2 → 77.0（p=0.5） |
| "加特征 / 调权重能再涨" | **否** | 24 特征里 **7 个拿掉后 top-1 反而涨**；3,000 组配置只比 24 组多 3 个样本；边际只在 `gram`（−16.63 点）与 `scal`（−2.49），`size`/`consts` **纯冗余** |
| "扩样本能压窄置信区间" | **部分** | 348 → 975、CI ±6.36 → ±3.02，但新增 **622/629 是依赖 crate**；**游戏代码仍 n≈169、±6.4** ⇒ 瓶颈是**锚**不是名字 |
| "重叠/串发导致整批崩" | **否** | 89 发交叉命中 0/178；真因是**同批更早爆炸的洞已进掩膜** + `end_x` 早一个采样点 |
| "角色推断能给无名函数定角色" | **否** | 正对照 40.5% **< 平凡基线 44.3%** |
| "顺序轴（跨构建函数顺序+体量）能给 12,736 定名" | **全局否 / 族内是** | 全库 **20.9%**（tier A 82.6%、B 20.5%、C 6.0%）；但**近孪生族 84%**（tier A **28/28**）—— 结构指纹在该族只有 51.9% ⇒ **两轴盲区互补**（`docs/02` §35.12b） |
| "视觉三屏差异是引擎版本/部署形态造成的" | **否（2×2 因子实验）** | C↔D（只换引擎）与 B↔D（只换 `--path`↔pck）在静态屏都 **0 像素**；跨侧两组数字**逐位相同** ⇒ **是我方 UI 本身**（`docs/06` §54） |
| "换证据类型就能再涨"（crash 栈 / campaign 表 / 真机帧） | **三条都不通** | crash 栈：做匹配的那份**无 `.symtab`/`.debug_*`**、仓库 0 份崩溃产物 ⇒ 新锚 0；真机帧：仓库 0 帧、能给的量已被 `.rodata`/`client/`/campaign 覆盖；campaign 表：**能锚但只能当锚**（覆盖率 4.62% vs 0.05% = 84× 偏斜，塞成特征就是 `strs` 同类错误），留出半一格没动 |
| "换权重/换配置能再涨"（63 号到 3,000 组） | **否（不显著）** | 3,000 组调参半只比 24 组多 3 个样本；预注册口径仍 **76.4%** |

| "原版弹道是用 `draw_*` 画原语画的" | **否** | 一个 `draw_*` 都不用：弹道是 **`Line2D` 子节点逐点追加**。链：`add_point` 字面量 `0x29f39f` → 包装 `0x8b7210` → GOT 槽 `0x9bbf98` → 全 `.text` **唯一**引用 `0x498f6e` ∈ `FunctionDrawer::update_function`（`0x498c70`），唯一直接调用者 = `GraphwarPlane::draw`（`0x4b5df0`）。线宽 f32 **0.125**、joint=ROUND、每帧揭示 `min(len,(tick−active_tick)×30.0)`（`docs/06` §65） |
| "回放播放时钟是浮点累加器 / 每帧 `delta×60`" | **否** | 全局唯一时基 = `GraphwarTickHandler`（`(secs,nanos)` + `tick:u32`），每 tick **+16 666 666 ns**（`0xfe52a` @`0x51986c`）；**暂停不是布尔量**，是 `pair#3.nanos==1e9` 标记（`is_paused` = `0x3d7685 cmp`+`0x3d768b setne`）；回放播放头只是镜像它的 tick（`0x3d7583`）。真机：进屏 0.00 tick/s、▶ 后 59.8–60.3 tick/s（`docs/06` §63） |
| "`0x4c6f40` 里有回放时钟" | **否** | 188 个 call（94 间接、91 条解出 GOT 目标）无一指向时基/播放器；整函数零 xmm ⇒ 旧文档把"时钟没解"误挂在它名下 |
| "换架构（arm64/Windows）会悄悄算错" | **否（数值路径）** | x86-64 与 arm64 两份**真机器码**在 unicorn 下同输入：**175 组里 168 组逐位相同、7 组只差 NaN 符号位**、0 组没跑成；数据表逐字节相同。真差异只有两条：`invalid` 生成的 NaN 符号位（产生差、传播 6/6 相同）与 **libm ULP**（缺 bionic `libm.so`，量到上界：RK4 有 2 组点数会变）⇒ `docs/06` §61 / `docs/19` §8 |
| "`LevelRules` 归属没有新证据" | **否** | 三个判据函数（`0x473520` / `0x4759f0` / `0x4780c0`）的 `panic::Location` 全落 `graphwar_game_state_manager.rs`，**没有一条落 `play_screen_control.rs`** ⇒ 归属 = GSM（文件仍在 `play_screen_control.rs`，未搬） |
| "掩膜的持有者与生命周期未知" | **否** | 持有者 = **`GraphwarObstacleManager` 的 `self+0x18`** → `ArcInner`（`Mutex` 锁字 `+0x10`、数据 ptr `+0x20`、len **600000**），读/写/挖三处同一段 `lock cmpxchg`；**没有每帧重建**：整表建一次（`0x466c40`→`0x3f1e50`），之后只有事件增量与 tick 队列。创建者 `GameClient::new 0x517690`（`docs/06` §65、`docs/08` §12） |
| "音效↔事件映射证不到" | **已结案（观测）** | 5 条 mp3 的槽与触发事件钉到**指针级**：`explosion`←`EffectInfo{0}`、`squash`←`EffectInfo{1}`、`spark`←`EffectInfo{2 \| 其余}`（兜底臂）、`pop`←`SoldierInfo`、`unpop`←`EntityRemoved`，音量 −15/−15/−15/0/−5 dB；`humm.wav` 走 generator 归 `FunctionActive`；`GameStateInfo{Setup}` 不出声。对齐用**三轴合证**（资源指针 5 处逐对相等 + payload 与回放 JSON 逐字节相等 + `value==start_tick` 且 play 落在 ±1 tick），真值表 `fixtures/audio_event_map.tsv`（复跑**逐字节可复现**）。新未知：**入队 ≠ 播放**（`Vec::retain(tick<value \|\| is_playing)` 会丢弃同帧条目，`docs/06` §70） |
| "AI 只有参数对上了" | **扩展了** | 参数层 `逐位` 71/71；**抽样层与生成/装配层也 `逐位`**——注入同一 ChaCha key 后 **27 行逐 token 与真二进制相同**（C3 闭合）；终止判死为**墙钟**（动态 6/6）。**仍未复原**：子代工厂三路选择（指令在、数据流读不通，硬搬会让 `tests/server_match.rs` 两条 TCP 用例变红 ⇒ 回退）；`hits` 二进制是**带符号**计数 |
| "段位算法在客户端" | **否 ⇒ E 档** | `rank_manager` 4 个函数**算术指令 = 0**（正对照 `Plane::draw` 1158 条有 div/mul）；写入 `0x455090` 两处调用点都在**收包侧**；`.rodata` 里只有 22 条 `res://ranks/rank_0..21.svg`、档位名/阈值表 0 命中 ⇒ 算法在服务端（`RankInfo` 线上 **2 字段**已定，2 vs 3 收敛） |

**已被证实、且已落地的**（不用重做）：
`像素级真值能抓源码复原错误`（2026-11-26：`update_sprite` 的 `scale.x` 漏了 `k`，精灵恒 2.13 px 宽，
  活了三轮、`docs/16` §33 还标着"全部算术已复原"；修法见 `docs/06` §58）；
`报告顺序 ≠ 结算顺序`（批内按**到达序**逐发挖洞，S3，三轴未退、净 2 批变好，内核与收敛层**共用一份实现**）；
`打不全就全部复原`（部分命中 ⇒ 回合末**整组**靶子广播 `alive:true`；全清 ⇒ 不复原）；
`pos_y` 只有两条写入指令、一条 16 字节 `movdqu` **同时写 x 和 y**。

---

## 3. 信息天花板（不是没干，是物理上到顶）

```
12,736 / 13,263 个函数**一个 src/*.rs 的 panic::Location 都不引用** ⇒ 没有名字可锚
   · 结构指纹只能给"同源候选"：预注册 **76.4%**（`scal=0.25`）/ 并列桶最高 78.2%，CI [72.12, 84.24]（n=165）
     ⚠️ **引用 76.4**；78.2 只是"23 组并列桶的最高值"（`docs/160` 复核）→ 口径块在 `docs/02` §35 开头
   · 换编码已到顶（71 号：边际只在 `gram`/`scal`，`size`/`consts` 纯冗余）
   · **换证据类型也试过了，三条全不通**（crash 栈 / campaign 表 / 真机帧，见 §2）
   · 已知的**族内失效**：关卡闭包族 **14/27 = 51.9%** vs 族外 407/454 = 89.6%（p=2.6e-6），错 19/19 全在族内
     ✅ **2026-11-24 已用「两轴盲区互补」解掉**：结构指纹（族内 56.8%）+ 顺序轴（族内 84.1%）分域混合 ⇒
     **族内 95.5%（可部署域 93.3%）**、族外与指纹逐格相同、代价 −1 行（已定位 `0x466c40`）⇒ `docs/02` §35.12c
     （算法/超参先 commit `6a85b1e` 再跑留出；`tools/hybrid_names.py`，13,263 行输出表 `fixtures/hybrid_names.tsv`）
· 服务端撮合逻辑：客户端二进制里**没有**（我们 `src/_scaffold/broker.rs` 是自建的，不是复原）
· 美术**源文件**（`.svg`/`.ttf`）：包里只有产物（167 `.ctex` / 7 `.fontdata`）。
  **但图能用** —— `.ctex` 内部就是 WebP（实测首字节 `GST2…RIFF…WEBP`），`client/assets/` 已在用
```

---

## 4. 还剩什么（按"能不能靠干活推上去"分档；权威表 `docs/00` §0.1b.2）

| 档 | 内容 | 备注 |
|---|---|---|
| **Ａ 缺工程** | ① **客户端像素保真**：回放三屏有真值（`docs/06` §53/§54）——`replay_lobby` 4.114%（只差列表行）、`replay_play` 41.330%（**2026-11-06 的旧读数，现为 0 px**）、`replay_end` 52.460%；静态屏（战役选择/关卡弹窗）**还没拍过** | **当前主线**（t4/t5/t6 在做）；判据 = 预注册门槛 + 元素 bbox |
|  | ② `client/src/screens/*.gd` 现 **≈6 397 行**（原 6 950；已删手抄节点树 ≈1 150 行）；控制器要迁回 Rust（按原模块名） | 量大，按切片推进 |
|  | ③ **1 个 manager 仍是 `assembled`**（**19 wired / 2 layout / 1 assembled / 0 stub = 22**）——余 1 格 = GSM，卡 `src/game.rs` 禁区 | 见 `docs/161` |
|  | ~~④ `ShotLifecycle` 文件归属~~ **2026-11-26 已结案**（按 `Location` 证据搬进 `graphwar_game_state_manager.rs`） | `LevelRules` 仍无归属证据，留在原地 |
| **Ｂ 缺样本（易得）** | 目前**空**（Level 18–20 / flip=1 / 部分命中 都已补齐） | 找人打一局即可 |
| **Ｃ 缺样本（难造）** | 1 ulp 正主 —— **四条路都没复现**、打印级判据结构性没有判别力 ⇒ 停做（**不是证明它不存在**） | — |
| **Ｄ 缺真值（可运行时挖）** | libm 边界；`0x4c6f40` 回放播放时钟（内部 `playing` 初值目前只有画面证据） | 开游戏 + frida |
| **Ｅ 物理不可得** | 服务端撮合、`.svg`/`.ttf` 源 | **别立项**；名字已由 T1 给出「推断级候选」（1,589 / 13,263） |

**2026-11-26 这一轮已落地**：t4 回放播放屏（载入即暂停 + 版式 + 棋盘几何）、t5 回放列表行（0 像素）、
t6 静态两屏（0 像素）；另由 lead 修掉 `update_sprite` 的源码级复原错误（`docs/06` §58）。

**S3/S4 现在只剩结构性残余（`docs/06` §59.6）**：`0.217%` = 4 836 px，`max|Δch|=255`，逐项是
① 士兵**合成尺寸** 36×29 vs 原版 ~54×43 + 原版主体下方多一层（缺 `GraphwarTextureLoader` 查表键与逐部件摆位证据）；
② `f(x)=` 图标小 2 px（636 px，要动 `FxContainer` 的 AR 结构）；③ 光标闪烁相位（270 px）；
④ 画布模型（1000×600 缩放 vs 原版按实尺寸画，格子线 1.28 px 抗锯齿，~1 970 px）。
**L3 要 `max|Δch|≤8` = 逐像素全等 ⇒ 上述任一处 1 px 差都会破，短期不追。**

**对局屏（新真值，`docs/06` §60）下一步**：① 补我方缺的 **12 个事件处理器**（`TargetInfo`/`NameInfo`/图形三兄弟…）
② `Fire!` 按钮字号/色 ③ 偏移量标签 ④ 准备圈位置 ⑤ S8 相位对齐（我方不发 `TickRequest`）。

## 5. 干活时必须遵守的（都是踩出来的）

### 合并 teammate 分支之前（`AGENTS.md` 合并纪律 A）

> 2026-11-26 起流程变了：**没有 `dispatch/` 任务书、没有 CG 下游分支**。teammate 在自己的
> worktree/分支（`agent/<topic>`，从 `master` 切）里干活，我在 master 上合并。
> `merge_check.py --cg` 那条是给旧 CG 分支用的，现在**只保留它的三个机器检查思路**：
> 范围（`git diff --name-only <分叉点>..<分支>`）、污染（二进制/大文件）、四件套（§下面 B）。

- **必须读它的验收报告，先读「遗留/未做」与「我写错了」两节** —— 跳过它我漏过"准备圈画面上没有了"这种真回退；
- 范围核对用 `git diff --name-only $(git merge-base master <branch>)..<branch>`（**永远用分叉点**，别用 `master..`）；
- 生成物冲突（`docs/coverage-matrix.*`）**重跑 `python3 tools/coverage_matrix.py`**，绝不"取一边"；
- 合并顺序：先无依赖的，再 rebase 有重叠的；**每个分支合完都要重跑四件套**。

### 合并之后（纪律 B）

```bash
export CARGO_HOME=$PWD/.reference/cargo-home CARGO_TARGET_DIR=$PWD/.reference/cargo-home/target
cargo test --offline                                  # 2026-11-26 实测 1761 / 0 / 1 ignored
cargo test --offline --features gdext-bindings        # 2026-11-26 实测 1867 / 0 / 1 ignored
python3 tools/coverage_matrix.py --check              # 527 可归属（**模块级**）/ 逐位 89 / 观测 438；函数级见 §87
python3 tools/check_doc_refs.py                       # rc=0
R=.reference/replays/gw2-replays-snapshot/replays     # 403 个文件（401 回放 + 2 元数据），只读
cargo run --release --offline -q --example replay_attrib -- --dir $R --drive fire  --policy default --json f.jsonl
cargo run --release --offline -q --example batch_conv   -- --dir $R --policy strict
python3 tools/bit_exact_attrib.py --json f.jsonl --check-baseline --axis fire   # 三轴各自一条
bash client/run.sh selftest                           # 717 / 0 / 1 skip（四个前置见 docs/18 §10.3.4）
```

**护栏（2026-11-26 本机实测，报数带轴名）**：
`fire` 内容 **647** / 种类 1075 ｜ `batch` 内容 **647** / 种类 1075 ｜ `batch_conv` 内容 **637** / 种类 1075；
死亡 **366 / 385**、挖洞 **207 / 294**。

**headless 巡检**只喂 `extends SceneTree` 的脚本（`client/tools/*_check.gd`，**14 个 / 1933 条**）：
拿 `plane_on_screen.gd` / `screenshots.gd` / `selftest.gd` 去 `--script` 会**弹 ALERT 并留僵尸实例**，
后续结果被污染（我踩过：selftest 报 704/4，干净重跑是 714/0）。
跑 headless 时若不给可写 `HOME`，`play_flow_check` 会少 4 条（202 而不是 206）。

### 三条硬约束

1. **护栏语料必须冻结**：三轴用 `.agent-work/replay-snapshot/` 的**固定 401 份**；
   新样本放 `replay-new*`。混进去三轴数字全变（401→403 那次：647→654、1080→1094）。
2. **报数必须带轴名与前置条件**（§1 那几条）。
3. **证据三档来源**（`AGENTS.md` §1b）：主证据=二进制；观测证据=真实报文/回放/真机；
   **玩家说法只是线索**（能指方向，不能进结论）。

---

## 6. 环境与地址（错了会静默出错）

```
地址          **本仓库文档 / pkl / 工具输出里的地址，一律是 ELF vaddr（= 模块内偏移 = RVA）**，
              直接当 vaddr 用、**不要平移**；frida 下断 = `base + RVA`。
              ⚠️ 2026-11-26 实测更正（旧写法「Ghidra vaddr = ELF + 0x100000」**只对 Ghidra 反编译文本成立**）：
              `funcstarts_ghidra_va.pkl` 的 13,263 条里 **11,881 条直接命中 `.eh_frame` FDE 起点**、
              减 0x100000 后只命中 **440**；pkl 最小/最大值 0x37b940/0x94eb70 正好是 `.text` 段两端
              ⇒ **那个文件名里的 `_ghidra_va` 是误导，它装的是 ELF 地址**。
              只有 Ghidra/REA **反编译文本**里的 `_DAT_xxxx` / `FUN_xxxx` 才是 ELF+0x100000 口径
              （`_DAT_004c2530` ↔ ELF `0x3c2530`）。教训：把这条套到文档地址上会定位到**另一个函数**（已踩，见
              `docs/06` §63.1 与本次 T12 的更正）。
引擎调用      gdext 走 **GOT 槽**：`call *0x…(%rip)`；只看 `call rel32` 会得出"原版不画任何东西"
              实例：draw_arc 字面量 0x19f9d0 → 包装 0x8abca0 → 槽 0x9bc020
函数归属      `panic::Location` 在 `.data.rel.ro`（指针是重定位）；解析 `.rela.dyn` 才能还原
函数起点      用 `analysis/funcstarts_ghidra_va.pkl`（**装的是 ELF 地址**，见上一行）；**`funcstarts.pkl` 是错的**（与上者只交 2 个）
GPU 口径      比"计算时间"还是"进程墙钟"？（后者含 `import torch` ≈5 s）
              实测：全量 252×；小规模内部 4.83×、进程墙钟 1.25×。**报 GPU 必须同时给口径**
```

**视觉对拍的两条并行陷阱（2026-11-26 实测）**：① **UDP `42968` 是本机共享资源** —— 多个 teammate
同时起「脚本化协议服务器」时，客户端 LAN 发现会连到别人那一份（`accepted=0`、大厅空），
处置是**重试/串行**，不能改端口（客户端固定 42968）；② `match_serve.py` 的 `--collect` 默认只有 8 s，
**拍结算屏/结束局前连接就被关掉** ⇒ 我方拍到的是断线弹窗假象；要用 `ENDGAME_SERVER_COLLECT=120`（两侧同值）。
③ TCP 端口来自**预注册 fixture 的 `[meta] tcp_port`** ⇒ 两个并发 run 用同一份 fixture 会 `Address already in use`，
必须串行（或用 `--tcp-port` 显式错开）。

**"改了但没生效"的三个静默陷阱**：新增 `.gd`/`.gdextension` 后没 `--import`；
`cargo build` 后没 `cp` 进 `client/`（客户端跑旧 `.so`）；残留旧的 `target/…/serve_match`。
`/tmp` 是逐命令 tmpfs，临时产物放工作区。

---

## 7. 我这一程犯过、已经写进纪律的错（别重犯）

```
① 跳过报告的 §7/§8 就合并 ⇒ 漏掉"准备圈画面上没有了"（用户发现）
② 把"某条轴的读数"记成另一条轴 ⇒ 1074 是 batch_conv 不是 batch（AGENTS 合并纪律的来源）
③ 拿局部观测当普遍结论三次 ⇒ "61 是唯一适合 GPU 的"/"63 是唯一需要 GPU 的"/"小矩阵 GPU 亏"
   ⇒ 真原因第一次是"口径混用"、第二次是"接口形状"，都不是"规模"
④ 算错量级 ⇒ 0.37578125 我写成"37.5 个步长"，实际是 **166 个采样点**
⑤ 把新样本混进冻结快照 ⇒ 三轴数字全变（已修：快照 401 份 + 指纹登记）
⑥ 拿"并列桶最高值"当新纪录 ⇒ 78.2 是 23 组里挑最大，预注册值是 76.4
⑦ 用 helper 脚本喂 `--script` ⇒ ALERT 弹窗 + 僵尸实例污染后续 selftest
⑧ README/文档写成 git 日志 ⇒ 文档只写"现在是什么"，历史归 git
⑨ 把"文档里的路径"当成通行 ⇒ 2026-11-26 `client/run.sh` 里 `ws="$here/../.."` 只认一种目录布局，
   换到本机（外部件在 checkout 内的 `.reference/`）就把 XDG 指到只读区 ⇒ `user://` 不可写 ⇒ selftest 静默降级。
   **修法与证据**：`docs/18` §10.3.4（Godot 按候选表探测、XDG 恒指 checkout 内）
⑩ 前置条件只写一半 ⇒ selftest 的**第四个前置**（真回放在 `$HOME/.local/share/godot/app_userdata/Graphwar/replays/`）
   以前没写进 README/HANDOVER，照着文档跑会少 63 条（654/0/3skip，`missing=63` 全 degraded）。
   ⇒ **判据永远是 `missing=0` + `silent=0`**，不是 `passed` 数
⑪ **把文档里的"现状"直接抄进任务书** ⇒ 2026-11-26 我照 `AGENTS.md` 的"docs/guide/（规划中）"派了"新建攻略手册"，
   实际那目录**早就有 7 个文件 2042 行**（teammate 发现并只补了真缺的 3 章）。
   ⇒ 派活前**自己跑一遍 `ls`/`wc -l`/`grep` 核现状**；任务书里写"现状"的地方注明"自己核"。
⑫ **扫描器没有正对照** ⇒ 我"证明" `.so` 里 `with N element` 串只有 47 条（真值 **67**）：
   正则写的是复数 `elements`，于是 **N=1 的 `with 1 element` 全被漏掉**，还顺势推出了"19 个有字段事件没有这条串"的假结论。
   teammate 用重叠扫描 + 三种口径把我推翻。⇒ **凡"扫不到/只有 N 条"的结论，先做正对照 + 换个口径复算**（本节 §2 已列这条代价）。
⑬ **地址口径写错一次代价很大**：一直以为"文档地址 = Ghidra 地址 = ELF + 0x100000"，实测
   `funcstarts_ghidra_va.pkl` 的 13,263 条里 **11,881 条直接命中 FDE 起点**、减 0x100000 后只剩 440
   ⇒ 文档/pkl/工具输出**全是 ELF vaddr**，只有 Ghidra 反编译文本里的 `_DAT_/FUN_` 才是 +0x100000。
   我按错口径写了任务书，好在 teammate 用 FDE 证伪并全程按 ELF 装载。
```

---

## 8. 现在的协作状态

```
主线（本机）  **32 核 / 30 GB / RTX 4060 / 无显示器（Xvfb 做视觉对拍）**；2026-11-26 起
              **我直接负责 `master`**：定位缺口 → spawn teammate（各自 worktree + 分支 + 共享任务板）
              → 我自己复现数字 → 合并 → 写文档。不再有"上游派活/下游执行"的两段式。
历史          `CG` 分支 = 2026-11-10 之前的"下游执行侧"，上面有 `dispatch/`（任务书 + 每轮报告）。
              master **不合并 `dispatch/`**；新活一律从 `master` 切 `agent/<topic>` 分支。
              旧文档里 `dispatch/NN` 的引用是**历史出处**（master 上没有那些文件），不是可点的路径。

2026-11-26 本轮      已合 9 个分支：t7 回放屏归零 / t8 对局屏真值 / t9 跨架构 / t10 视图层台账 /
                      t11 手册 3 章 / t12 回放时钟 / t13 升档 sweep / t14 四条未知 / t15 文档一致性 / t16 协议复核
                      在跑：t17 补 12 个事件处理器 · t18 结算弹窗真值 · t19 音效↔事件映射 · t20 剩余 8 屏像素真值
隔离工作区           `.agent-work/cg/wt-<task>`（每任务一 worktree + 独立 `CARGO_TARGET_DIR`，btrfs reflink
                      几乎不占额外磁盘）；共享任务板 `team_task_*`；规格在 `.agent-work/specs/`（gitignore）

关键文档      docs/00 状态总览 · docs/02 模块图 · docs/06 验证记录 · docs/16 客户端（§61/§62 视觉帧）
              docs/18 构建与自检前置（§10.3.4） · docs/161 manager 缺口 · docs/guide/ 攻略手册
```
