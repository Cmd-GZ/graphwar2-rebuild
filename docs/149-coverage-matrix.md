# 149 · 覆盖率矩阵：按**原版模块**算的进度尺
> 本文件由 `tools/coverage_matrix.py` 生成，**不要手改**。改口径请改脚本，然后重跑。
> `python3 tools/coverage_matrix.py` 重新生成；`--check` 只校验不写，不一致就退出码 1。

## 0. 一句话总账

```
原版可归属函数 527 个（Location 口径），分布在 82 个模块（scope=graphwar 66 + other 16；其中 64 个有同名 `graphwar_*.rs` 文件）；我们已实现 527 个函数，其中 逐位 89 / 观测 435 / 推断 3；未做 0；完全未知的模块 0 个（graphwar 里 0 个）；**没实现的模块 2 个**（`未做`/`未知`；这里的「未做 0」是**函数**条数，别混）；`sym_funcs` 合计 740（Android 符号表 FUNC，**另一条轴，不进上面的分母**）。
```

拆成两个口径（下面的数字全部由脚本算出，重跑即复现）：

| 口径 | 模块数 | 可归属函数 | 已实现 | 逐位 | 观测 | 推断 | 未做 | 未知模块 |
|---|---|---|---|---|---|---|---|---|
| 64 个有同名 `graphwar_*.rs` 文件的模块（`scope=graphwar` 共 66 个） | 66 | 431 | 431 | 81 | 347 | 3 | 0 | 0 |
| 16 个 `scope=other` 的模块（16 个不带 `graphwar_` 前缀；带 `graphwar_` 前缀的已于 T112 全部归回 graphwar） | 16 | 96 | 96 | 8 | 88 | 0 | 0 | 0 |

`weak_caller.rs` 在上表里算进 **scope=`other`** 那一档（39 个 `orig_funcs`，TSV 里单列一行）。**审计 2026-11-08 更正**：旧文写「40 个归属函数 …… **不计入**任何合计」，两处都不对——① 表值是 **39**（`docs/coverage-matrix.tsv` 的 `orig_funcs` 列，另 1 个符号查明是内联进 `GraphwarGameRecorder` 的方法，不是本模块的函数）；② 它**确实**计入 `other` 的 97 与总数的 527（430 + 97 = 527）。它也不是第三方依赖：`docs/02` §34 已按符号表判它是 crate 内模块（`graphwarrust::weak_caller`），只是文件名不带 `graphwar_` 前缀。

### 0.1 这个分母有多大、有多可信

| 事实 | 值 |
|---|---|
| 二进制函数边界（Ghidra 全量 / 落在 `.text`、本机函数表里有） | 13503 / **13263** |
| `panic::Location` 结构体（`len` 字段自校验后） | **2042** |
| 其中涉及的 `src/*.rs` 文件 | 69 |
| 至少有一个 Location 锚的函数（=13263 里） | **527**（4.0%） |
| 其中同时引用两个以上模块 Location 的（已压平，不重复计数） | 86 |

**归属率只有 4.0%，而且偏得厉害 —— 这是本表最重要的一条警告。**
只有带 panic 点（`unwrap`/`expect`/越界/`panic!`）的函数才留锚。纯计算函数没有：求值器 `0x4eb9d0` 和采样器 `0x3c2530` 的**整个函数体里一个 `Location` 引用都没有**，所以它们根本归属不出来。具体后果是 `graphwar_path_calculator.rs` 的 `orig_funcs` 只有 1，而采样器就装在那个模块里。
⇒ **`orig_funcs` 只能当「有多少函数能被硬证据钉到某个源文件」的下限，不能当模块规模。**写「这个模块有 N 个函数」是错的。

### 0.2 模块分母：82 个（2026-11-28 T112 从 78 修正）
**旧的 78 = 62 + 16 是错的**，错在**分母的建法**，不在实现：模块清单以前也是按 `panic::Location` 建的，而 Linux `.so` 里 `src/<模块>.rs` 字面量出现 **0 次**的模块（本表里就有 **11 个**）**连一行都不会有** ⇒ 分母被系统性漏算。
现口径（预注册 `fixtures/module_denom_plan.txt`）：模块清单取**未 strip 的 Android 2.0.60 `.symtab` 的符号路径**（与逆向目标同版本），模块名 = 路径第 2 段。

| 读数 | 值 | 说明 |
|---|---|---|
| **N（总账用这个）** = N_2.0.60(J1') | **82** | 符号路径**任意位置**出现 `graphwarrust::<m>::`（含 impl / 泛型实参） |
| N_2.0.60(J1) | 81 | 更严：只在**定义路径**里出现的（差 1 个：`time_probe.rs`） |
| N_union(J1') | 82 | 并上 `docs/02` §18.3 的 78 个名字（2024-07-30 x86-64 版）；两者并集仍是 82 ⇒ **没有「只有 2024 版才有」的模块被漏掉** |

拆开：**66 个 `scope=graphwar` + 16 个 `scope=other`**。这轮改的 4 处：

| 模块 | 本轮处理 | 原版证据 |
|---|---|---|
| `graphwar_ban_list.rs` | **补行**：已实现（T109 落码，观测） | Android `.symtab` FUNC=2 |
| `graphwar_version.rs` | **补行**：已实现（T109 落码，观测） | Android `.symtab` FUNC=1 |
| `graphwar_id_maker.rs` | **补行**：**未实现**（我方无文件） | Android 只剩 1 个 `LOCAL OBJECT`（`get_unique_id::CURRENT_ID`），`get_unique_id` 已被内联 ⇒ 只数 FUNC 会漏（T109 数出 65 而非 66 就是这里） |
| `time_probe.rs` | **补行**：**未实现**（0 函数 / 0 方法） | Android 只剩 `core::ptr::drop_in_place<…TimeProbe>`；2024 版记录相同 |
| `graphwar_texture_loader.rs` | **改 scope**：`other` → `graphwar`（分类错，不是新模块） | 名字带 `graphwar_` 前缀 |

⚠️ **诚实读法**：分母变大让**模块级完成度从 100 % 掉到 80/82 = 97.6%** —— 但新增的 4 个里 **2 个是早就做完的**（`ban_list` / `version`，只是不在分母里，等于旧读数里白送了两个模块），**真正没做的是 2 个**（`graphwar_id_maker` / `time_probe`）。别读成「突然多出 4 个模块的活」——真正要动手的只有这 2 个。
`sym_funcs` 列合计 **740**（Android 符号表 FUNC）—— 这是**另一条轴**，**不并入** 527（Location 可归属），两者不可混报。
正/负对照 8 条：PASS 8 / FAIL 0 / SKIP 0（明细见 `fixtures/module_denom_audit.tsv` 头部；三方差集表也在那里）。

## 1. 方法（可复现）
### 1.1 原版这一半

1. 扫所有 alloc 段里 `src/<name>.rs` 字面量的虚拟地址；
2. 解析 `.rela.dyn` 的 `R_X86_64_RELATIVE`：`addend` 等于某个字面量地址时，该重定位的 `r_offset` 就是 `panic::Location` 结构体的 `file.ptr` 槽；
3. 自校验：`struct+8` 必须恰好等于字面量长度，`line/col` 必须是合理正整数；不满足就丢掉（去掉这步会把任意 `(ptr,len)` 对当成 Location）；
4. capstone 线性扫 `.text`，收集 rip-relative 与立即数引用；
5. 引用地址 → 用 `analysis/funcstarts_ghidra_va.pkl`（**Ghidra 边界**）二分到所属函数；`funcstarts.pkl` 那张 `.eh_frame` 的表是坏的，不能用（`docs/02` §12）。
**地址空间**：本表全部是 **ELF 虚地址**。Ghidra/REA 地址 = 虚地址 + `0x100000`，只能加一次。函数表本身已经是虚地址（`fs[0] = 0x37b940`、`fs[-1] = 0x94eb70`，正好等于 `.text` 区间），四个已知入口 `0x4eb9d0`/`0x4ed670`/`0x3c2530`/`0x3c5f70` 在表里**精确命中** —— 这同时钉死了地址口径。

### 1.2 我们这一半
`src/**/*.rs` 里 `fn` 的条数，先剔除注释、字符串与字符字面量再匹配（否则注释里的 `fn` 会被数进来）。**只有 `src/` 算实现**：`client/` 那套 GDScript 是仿制品（`AGENTS.md` §0.4），不计。

### 1.3 五档怎么判

| status | 判据 |
|---|---|
| `逐位` | 有 src/ 实现，且已与真二进制/真抓包**逐字节或逐位对拍通过** |
| `观测` | 有 src/ 实现，依据是反汇编/运行时/真机观测，但没有逐位对拍 |
| `推断` | 有 src/ 实现，依据是推断（evidence 里写缺什么） |
| `未做` | **没有** src/ 实现，但原版这块有地址级观测或文档描述 |
| `未知` | **没有**实现，且除了模块名没有任何证据 |

`--check` 会验证不变式：`未做`/`未知` 的 `ours_file` 必须为空，其余三档必须非空。

### 1.4 自校验：那条「求值器/解析器都应落 math_function.rs」只成立一半
`AGENTS.md` 把「`0x4eb9d0`（求值器）/ `0x4ed670`（解析器）应落 `src/math_function.rs`」当成这套方法的自校验。实测（本脚本每次运行都会重算）：

| 函数 | 体内的 Location 引用 | 归属 |
|---|---|---|
| `0x4ed670`（解析器，0xa70 字节） | `0x4edf1f: lea -> 槽 0x95fec0` = `math_function.rs:75` | ✅ `math_function.rs` |
| `0x4eb9d0`（求值器，0x131 字节） | **一条都没有**（全函数只有 3 处 rip 引用，分别指向 `.rodata` 常量和两个 `jmp *GOT`） | ❌ 归属不出来 |

`0x4eb9d0` 是自递归的求值器（`call 0x4eb9d0` 两次递归左右操作数），用 `bt` 位测试分派 tag，**整条路径没有 panic**，所以没有 `Location` 可抓。
⇒ 这条自校验**只能验解析器**。求值器要归属，得靠人工反编译或调用图推断。这不否定它属于 `math_function.rs`（`docs/03` 的函数语言证据链仍然成立），但它**不是**这套方法能自动得出的结论，本表也不假装能。

## 2. 逐模块矩阵
`ours_funcs` 是整个文件的 `fn` 条数，**不是**「这个模块实现了多少个函数」；`src/game.rs` 一个文件服务多个原版模块，所以那些行会重复出现同一个数。

### 2.1 64 个有同名 `graphwar_*.rs` 文件的模块

| module | orig_funcs | anchors | ours_file | ours_funcs | status |
|---|---|---|---|---|---|
| `graphwar_ad_switch.rs` | 6 | 3 | `src/graphwar_ad_switch.rs` | 46 | 观测 |
| `graphwar_audio_control.rs` | 7 | 16 | `src/graphwar_audio_control.rs` | 27 | 观测 |
| `graphwar_backend.rs` | 6 | 12 | `src/graphwar_backend.rs` | 44 | 观测 |
| `graphwar_ban_list.rs` | 0 | 0 | `src/graphwar_ban_list.rs` | 2 | 观测 |
| `graphwar_button.rs` | 6 | 3 | `src/graphwar_button.rs` | 67 | 观测 |
| `graphwar_campaign_control.rs` | 1 | 1 | `src/graphwar_campaign_control.rs` | 25 | 观测 |
| `graphwar_client_connection_handler.rs` | 8 | 16 | `src/graphwar_client_connection_handler.rs` | 44 | 观测 |
| `graphwar_color_manager.rs` | 4 | 6 | `src/graphwar_color_manager.rs` | 32 | 观测 |
| `graphwar_computer_player_manager.rs` | 5 | 27 | `src/graphwar_computer_player_manager.rs` | 34 | 观测 |
| `graphwar_crash_reporter.rs` | 0 | 0 | `src/graphwar_crash_reporter.rs` | 18 | 观测 |
| `graphwar_draw_util.rs` | 1 | 4 | `src/graphwar_draw_util.rs` | 1 | 观测 |
| `graphwar_effect_manager.rs` | 1 | 3 | `src/graphwar_effect_manager.rs` | 16 | 观测 |
| `graphwar_entity_effect_manager.rs` | 3 | 4 | `src/graphwar_entity_effect_manager.rs` | 17 | 观测 |
| `graphwar_event.rs` | 0 | 0 | `src/events.rs` | 2 | 观测 |
| `graphwar_event_handler.rs` | 2 | 4 | `src/graphwar_event_handler.rs` | 9 | 观测 |
| `graphwar_event_queue.rs` | 0 | 0 | `src/graphwar_event_queue.rs` | 5 | 观测 |
| `graphwar_function_evolutionator.rs` | 16 | 25 | `src/graphwar_function_evolutionator.rs` | 128 | 观测 |
| `graphwar_function_input.rs` | 13 | 33 | `src/graphwar_function_input.rs` | 105 | 观测 |
| `graphwar_function_manager.rs` | 7 | 12 | `src/graphwar_function_manager.rs` | 31 | 观测 |
| `graphwar_game_client.rs` | 36 | 59 | `src/graphwar_game_client.rs` | 136 | 观测 |
| `graphwar_game_info.rs` | 0 | 0 | `src/graphwar_game_info.rs` | 0 | 观测 |
| `graphwar_game_recorder.rs` | 2 | 4 | `src/graphwar_game_recorder.rs` | 29 | 观测 |
| `graphwar_game_server.rs` | 3 | 86 | `src/graphwar_game_server.rs+src/server.rs` | 19 | 逐位 |
| `graphwar_game_state_manager.rs` | 33 | 207 | `src/game.rs+src/turnstate.rs+src/graphwar_game_state_manager.rs` | 189 | 逐位 |
| `graphwar_graphics_manager.rs` | 6 | 8 | `src/graphwar_graphics_manager.rs` | 25 | 观测 |
| `graphwar_id_maker.rs` | 0 | 0 | — | 0 | 未做 |
| `graphwar_lan_broadcaster_v6.rs` | 4 | 4 | `src/graphwar_lan_broadcaster_v6.rs` | 20 | 观测 |
| `graphwar_lan_vbox.rs` | 11 | 19 | `src/graphwar_lan_vbox.rs` | 62 | 观测 |
| `graphwar_level_generator.rs` | 4 | 26 | `src/graphwar_level_generator.rs+src/graphwar_level_generator/tables.rs` | 28 | 逐位 |
| `graphwar_level_manager.rs` | 40 | 590 | `src/graphwar_level_manager.rs` | 45 | 观测 |
| `graphwar_life_manager.rs` | 9 | 12 | `src/graphwar_life_manager.rs` | 25 | 逐位 |
| `graphwar_lobby_control.rs` | 1 | 6 | `src/graphwar_lobby_control.rs` | 7 | 观测 |
| `graphwar_local_player_manager.rs` | 3 | 15 | `src/graphwar_local_player_manager.rs` | 22 | 观测 |
| `graphwar_logger.rs` | 2 | 2 | `src/graphwar_logger.rs` | 4 | 观测 |
| `graphwar_long_press_animation.rs` | 7 | 7 | `src/graphwar_long_press_animation.rs` | 50 | 观测 |
| `graphwar_mode_manager.rs` | 2 | 6 | `src/graphwar_mode_manager.rs` | 21 | 观测 |
| `graphwar_module_container.rs` | 20 | 1 | `src/graphwar_module_container.rs` | 17 | 观测 |
| `graphwar_name_manager.rs` | 3 | 3 | `src/graphwar_name_manager.rs` | 17 | 观测 |
| `graphwar_nodes.rs` | 11 | 1 | `src/graphwar_nodes.rs` | 50 | 观测 |
| `graphwar_obstacle_manager.rs` | 8 | 14 | `src/graphwar_obstacle_manager.rs` | 20 | 逐位 |
| `graphwar_path_calculator.rs` | 1 | 9 | `src/graphwar_path_calculator.rs` | 2 | 逐位 |
| `graphwar_plane.rs` | 10 | 19 | `src/graphwar_plane.rs+src/plane.rs` | 134 | 观测 |
| `graphwar_plane_pos_manager.rs` | 6 | 9 | `src/graphwar_plane_pos_manager.rs` | 20 | 观测 |
| `graphwar_play_screen_control.rs` | 10 | 64 | `src/graphwar_play_screen_control.rs` | 100 | 观测 |
| `graphwar_player_config.rs` | 0 | 0 | `src/graphwar_player_config.rs` | 8 | 观测 |
| `graphwar_player_manager.rs` | 3 | 6 | `src/graphwar_player_manager.rs` | 19 | 观测 |
| `graphwar_player_screen_control.rs` | 8 | 22 | `src/graphwar_player_screen_control.rs` | 48 | 观测 |
| `graphwar_rank_manager.rs` | 4 | 4 | `src/graphwar_rank_manager.rs` | 17 | 观测 |
| `graphwar_remote_event_handler.rs` | 3 | 171 | `src/graphwar_remote_event_handler.rs` | 13 | 推断 |
| `graphwar_replay_control.rs` | 2 | 9 | `src/graphwar_replay_control.rs` | 6 | 观测 |
| `graphwar_replay_player.rs` | 5 | 18 | `src/graphwar_replay_player.rs` | 87 | 观测 |
| `graphwar_replay_reader.rs` | 0 | 0 | `src/graphwar_replay_reader.rs` | 12 | 观测 |
| `graphwar_root_node.rs` | 23 | 178 | `src/graphwar_root_node.rs` | 146 | 观测 |
| `graphwar_server_broker_handler.rs` | 3 | 5 | `src/graphwar_server_broker_handler.rs` | 19 | 观测 |
| `graphwar_server_connection_handler.rs` | 6 | 24 | `src/graphwar_server_connection_handler.rs` | 31 | 逐位 |
| `graphwar_setup_control.rs` | 21 | 70 | `src/graphwar_setup_control.rs` | 191 | 观测 |
| `graphwar_shot_calculator.rs` | 1 | 12 | `src/graphwar_shot_calculator.rs+src/graphwar_shot_calculator/settle.rs` | 52 | 逐位 |
| `graphwar_soldier_manager.rs` | 4 | 5 | `src/graphwar_soldier_manager.rs` | 21 | 观测 |
| `graphwar_stream_buffer.rs` | 0 | 0 | `src/graphwar_stream_buffer.rs` | 22 | 观测 |
| `graphwar_target_manager.rs` | 3 | 4 | `src/graphwar_target_manager.rs` | 12 | 观测 |
| `graphwar_team_manager.rs` | 7 | 10 | `src/graphwar_team_manager.rs` | 31 | 观测 |
| `graphwar_texture_loader.rs` | 1 | 1 | `src/graphwar_texture_loader.rs` | 7 | 观测 |
| `graphwar_tick_handler.rs` | 2 | 3 | `src/graphwar_tick_handler.rs` | 37 | 逐位 |
| `graphwar_turn_manager.rs` | 14 | 45 | `src/graphwar_turn_manager.rs` | 110 | 逐位 |
| `graphwar_version.rs` | 0 | 0 | `src/graphwar_version.rs` | 1 | 观测 |
| `graphwar_video_writer.rs` | 3 | 4 | `src/graphwar_video_writer.rs` | 8 | 观测 |

### 2.2 非 `graphwar_` 前缀的游戏模块

| module | orig_funcs | anchors | ours_file | ours_funcs | status |
|---|---|---|---|---|---|
| `axis_drawer.rs` | 6 | 3 | `src/axis_drawer.rs` | 21 | 观测 |
| `backtracer.rs` | 0 | 0 | `src/backtracer.rs` | 13 | 观测 |
| `button_manager.rs` | 2 | 3 | `src/button_manager.rs` | 7 | 观测 |
| `effect_drawer.rs` | 11 | 17 | `src/effect_drawer.rs` | 58 | 观测 |
| `entity_drawer.rs` | 14 | 36 | `src/entity_drawer.rs` | 142 | 观测 |
| `function_button_manager.rs` | 2 | 3 | `src/function_button_manager.rs` | 6 | 观测 |
| `function_drawer.rs` | 1 | 1 | `src/function_drawer.rs` | 32 | 观测 |
| `function_history_recorder.rs` | 1 | 3 | `src/function_history_recorder.rs` | 20 | 观测 |
| `function_token.rs` | 0 | 0 | `src/function_token.rs` | 9 | 观测 |
| `math_function.rs` | 7 | 15 | `src/math_function.rs+src/math_function/to_text.rs` | 60 | 逐位 |
| `obstacle_drawer.rs` | 2 | 7 | `src/obstacle_drawer.rs` | 34 | 观测 |
| `player_showcase.rs` | 10 | 18 | `src/player_showcase.rs` | 82 | 观测 |
| `polish_notation_converter.rs` | 1 | 2 | `src/polish_notation_converter.rs` | 6 | 逐位 |
| `server_broker_request_connection.rs` | 0 | 1 | `src/server_broker_request_connection.rs` | 10 | 观测 |
| `time_probe.rs` | 0 | 0 | — | 0 | 未做 |
| `weak_caller.rs` | 39 | 2 | `src/weak_caller.rs` | 8 | 观测 |

完整的 evidence（每行一句话的依据）在 `docs/coverage-matrix.tsv`，那张表是本脚本的唯一真源，本页的表格由它渲染。

## 3. 哪些模块连「它干什么」都不知道（问题 1）
**64 个有同名文件的 graphwar 模块里有 0 个**属于这一类 —— 判据是：没有实现、且除模块名之外**没有任何**能指到地址/报文/真机的观测。

| module | orig_funcs | anchors | 现状 |
|---|---|---|---|

另外还有 **10 个模块一个 panic 锚都没有**（连「它有没有函数」都答不出来）：`graphwar_ban_list.rs`、`graphwar_crash_reporter.rs`、`graphwar_event.rs`、`graphwar_event_queue.rs`、`graphwar_game_info.rs`、`graphwar_id_maker.rs`、`graphwar_player_config.rs`、`graphwar_replay_reader.rs`、`graphwar_stream_buffer.rs`、`graphwar_version.rs`。
它们的 `orig_funcs` 是 0，**不代表模块为空**，只代表这个方法对它们完全失效。
非 graphwar 前缀里没有「未知」模块（未实现的模块见 §0.2 与 TSV 的 `status` 列）。

## 4. 我们自造的、原版没有的东西（问题 2）
这一栏**不算复原**，列出来是为了别把「能跑」当「还原了」。
判据：`src/` 里没有任何原版 `src/<file>.rs` 与它对应，或它实现的是原版明确没有的东西。

| 位置 | 为什么它不是复原 |
|---|---|
| `src/_scaffold/api.rs` | 内核 → GDScript/JSON 的对外 API；原版把这件事交给 Godot 模块反射，没有这一层 |
| `src/_scaffold/capi.rs` | C ABI，纯为把我们自己的实现暴露出去做对拍；原版无此物 |
| `src/_scaffold/replaydiff.rs` | 回放**消息级**对拍器；原版没有这种工具模块 |
| `src/_scaffold/diffconv.rs` | 上一条的收敛层；同上 |
| `src/_scaffold/replay_inflate.rs` | 手写 DEFLATE/gzip；原版用的是第三方 crate，不是游戏模块 |
| `src/_scaffold/rng.rs` | ChaCha12 的重写；原版是 rand_chacha crate，不是游戏模块 |
| `src/_scaffold/gdext.rs` | GDExtension 入口脚手架（原版的入口在 Godot 模块侧，不在这 82 个模块里） |
| `src/_scaffold/gdext_class.rs` | 同上：classdb_register_extension_class4 的封装 |
| `src/_scaffold/broker.rs` | **我们自己的一台 broker 服务端**；原版只有连 broker 的客户端，没有服务端 |
| `src/_scaffold/protocol.rs` | 报文编解码装配层；原版没有同名模块，编解码分散在各 handler 里 |
| `src/_scaffold/ai_explain.rs` | AI 决策链解释器（工具）；不引入新行为，只把已对过地址的零件按原版顺序跑一遍 |
| `client/` | 整个 Godot 客户端是照 .tscn 与截图重写的仿制品，**不是**原版源码（AGENTS §0.4） |

合计 **12 处**。其中 `src/_scaffold/broker.rs` 和 `client/` 最容易误判：前者是我们自己写的服务端（原版只有连 broker 的客户端），后者整个是仿制品。

## 5. 抽查：三个模块的函数数是手数出来的
`--check` 会重算并断言下面三个数没漂。

| module | 手数 | 脚本 | 怎么核的 |
|---|---|---|---|
| `graphwar_plane.rs` | 10 | 10 | docs/02 §11 独立写下的 10 个虚地址（0x4b2260 … 0x4be3e0）与本脚本算出的集合**逐个相同**；objdump 复核 0x4be3e0 体内 0x4be510 处是 `lea 0x4a0419(%rip),%rdx # 95e930`，该槽即 Location{len=21=len("src/graphwar_plane.rs"), line=174} |
| `graphwar_life_manager.rs` | 9 | 9 | docs/02 §13.1 那张独立写的表正好列了 9 个虚地址，与本脚本算出的集合**逐个相同**；objdump 复核 0x454950 体内 0x454ada 是 `lea 0x501c7f(%rip),%r8 # 956760`（life_manager.rs:114） |
| `graphwar_turn_manager.rs` | 14 | 14 | docs/02 §13.3 正文写「14 个函数」但只列了 13 个地址；本脚本算出的 14 个里多出 0x4638f0，objdump 确认 0x4638f0 是 `push rbp/r15/r14/r13/r12/rbx; sub rsp,0x68` 的正规 prologue，且是 Ghidra 函数表的精确入口 ⇒ 正文的 14 是对的，表的 13 漏列 |

三个抽查里两个（`graphwar_plane.rs` 10 个、`graphwar_life_manager.rs` 9 个）与 `docs/02` §11 / §13.1 **独立写下**的地址表**逐个相同**；第三个 `graphwar_turn_manager.rs` 正文写 14、表里只列 13，本脚本算出 14，多出的 `0x4638f0` 用 objdump 确认是函数入口。

## 6. 怎么读这把尺（以及它现在量不了什么）

1. **不要**把 `orig_funcs` 相加当「原版函数总数」。它是可归属下限，而且系统性漏掉所有纯计算模块（求值器、采样器就是现成的反例）。
2. **模块级分母（`scope=graphwar` 共 66 个，其中 64 个有同名 `src/graphwar_*.rs`；总数 82）现在**不再**是「Location 下限」，而是**未 strip 符号表**的读数（§0.2）：符号路径证明模块存在，与「这个模块有没有 panic 点」无关。它仍然只是**那一个构建**的下限（整模块被内联到只剩 `drop glue` 时只能看见类型名），所以要跨版本并集（N_union）一起来读。**函数级**归属仍只有 4.0%（Location 口径，见 §0.1）。
3. 想提高函数级覆盖，只有两条路：(a) 把 `0x3c2bc0` / `0x3c2530` 这类无锚函数**逐个反编译并人工归属**；(b) 沿调用图从锚点扩散 —— 但那是**推断**，`docs/02` §10 做过（885/13263，6.7%，hub 节点被高估），不能进总账。
4. 一次 `--check` 只要几秒到一分钟（取决于 capstone 扫 `.text`），**`src/` 一改，`ours_funcs` 就会漂，`--check` 会失败** —— 这是故意的：过期矩阵比没有矩阵更糟。

## 7. 与既有文档的口径差异（别以为是矛盾）

| 旧口径 | 出处 | 本表怎么处理 |
|---|---|---|
| 「544 个函数归属 / 56 个模块」 | `docs/02` §8 | 那张表把引用两个模块的函数在两个模块下**各算一次**。本表压平后不重复计数，并把跨模块数单独列为 `contested` |
| 「885/13263，观测 441 + 推断 444」 | `docs/02` §10 | 那是**调用图扩散**，属推断层。本表只收硬证据，不把扩散数写进总账 |
| 「57 个模块」 | 各处 | 旧读数 `57` 已作废（连它自己的两个口径都不对）：按**文件** `src/graphwar_*.rs` 是 **65** 个；只匹配 `[a-z_]+` 会漏掉`graphwar_lan_broadcaster_v6.rs` ⇒ **64**。本表的口径是**模块**：有同名文件的模块 **64** 个（65 个文件里有一个不是模块名，见 §17）。计数一律算出来、不写死 |
| 「`fixtures/module_funcs.tsv` 的地址清单」 | `docs/02` §14.2 | 那张表已自我更正为「不能判定函数身份」；本表用 Ghidra 边界 + 冲突压平重算 |
| 「解析器 `0x4ed670` 在 `graphwar_function_input.rs`」 | `docs/02` §1 | 该函数体内唯一的 Location 锚是 `math_function.rs:75`，按证据归 `math_function.rs`；所以函数语言的实现落在本表的 `other` 口径，不在带前缀的那 64 个模块里 |

## 8. 复现

```bash
python3 tools/coverage_matrix.py --check   # 只校验（缺 capstone 会自动换 .venv 解释器）
python3 tools/coverage_matrix.py           # 重新生成 TSV 与本页
```

输入摘要（也写在 TSV 头部，`--check` 会核对）：

| 输入 | SHA-256 / 值 |
|---|---|
| `libgraphwarrust.so` | `9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187` |
| `analysis/funcstarts_ghidra_va.pkl` | `b9bba35bbca95ce1040109b0a711cf77d0ff2d678b7e0b5f7220db588df74c3a` |
| `src/` 树（123 个文件、4290 条 `fn`） | `e5d7fd317a63682a6b0fab0c59058d905e224446204e36f47ade997afccb6418` |

