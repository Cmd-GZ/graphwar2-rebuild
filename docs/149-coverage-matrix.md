# 149 · 覆盖率矩阵：按**原版模块**算的进度尺
> 本文件由 `tools/coverage_matrix.py` 生成，**不要手改**。改口径请改脚本，然后重跑。
> `python3 tools/coverage_matrix.py` 重新生成；`--check` 只校验不写，不一致就退出码 1。

## 0. 一句话总账

```
原版可归属函数 527 个，分布在 78 个模块（58 个 graphwar_*.rs 占 431、15 个非 graphwar 游戏模块占 96）；我们已实现 351 个函数，其中 逐位 88 / 观测 263 / 推断 0；未做 125；完全未知的模块 18 个（graphwar 里 12 个）。
```

拆成两个口径（下面的数字全部由脚本算出，重跑即复现）：

| 口径 | 模块数 | 可归属函数 | 已实现 | 逐位 | 观测 | 推断 | 未做 | 未知模块 |
|---|---|---|---|---|---|---|---|---|
| 58 个 `graphwar_*.rs` | 63 | 431 | 303 | 81 | 222 | 0 | 85 | 12 |
| 15 个非 graphwar 的游戏模块 | 15 | 96 | 48 | 7 | 41 | 0 | 40 | 6 |

还有 `weak_caller.rs`（40 个归属函数）按 `docs/02` §9 判为第三方依赖库，单独一行留在 TSV 里备查，**不计入**任何合计。

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

### 2.1 58 个 `graphwar_*.rs`

| module | orig_funcs | anchors | ours_file | ours_funcs | status |
|---|---|---|---|---|---|
| `graphwar_ad_switch.rs` | 6 | 3 | `src/graphwar_ad_switch.rs` | 21 | 观测 |
| `graphwar_audio_control.rs` | 7 | 16 | — | 0 | 未做 |
| `graphwar_backend.rs` | 6 | 12 | — | 0 | 未做 |
| `graphwar_button.rs` | 6 | 3 | `src/graphwar_button.rs` | 13 | 观测 |
| `graphwar_campaign_control.rs` | 1 | 1 | — | 0 | 未做 |
| `graphwar_client_connection_handler.rs` | 8 | 16 | — | 0 | 未做 |
| `graphwar_color_manager.rs` | 4 | 6 | — | 0 | 未做 |
| `graphwar_computer_player_manager.rs` | 5 | 27 | — | 0 | 未知 |
| `graphwar_crash_reporter.rs` | 0 | 0 | — | 0 | 未知 |
| `graphwar_draw_util.rs` | 1 | 4 | — | 0 | 未做 |
| `graphwar_effect_manager.rs` | 1 | 3 | — | 0 | 未知 |
| `graphwar_entity_effect_manager.rs` | 3 | 4 | — | 0 | 未知 |
| `graphwar_event.rs` | 0 | 0 | `src/events.rs` | 2 | 观测 |
| `graphwar_event_handler.rs` | 2 | 4 | `src/events.rs` | 2 | 观测 |
| `graphwar_event_queue.rs` | 0 | 0 | — | 0 | 未做 |
| `graphwar_function_evolutionator.rs` | 16 | 25 | `src/graphwar_function_evolutionator.rs` | 88 | 观测 |
| `graphwar_function_input.rs` | 13 | 33 | `src/graphwar_function_input.rs` | 41 | 观测 |
| `graphwar_function_manager.rs` | 7 | 12 | `src/game.rs` | 70 | 观测 |
| `graphwar_game_client.rs` | 36 | 59 | `src/graphwar_game_client.rs` | 10 | 观测 |
| `graphwar_game_info.rs` | 0 | 0 | — | 0 | 未做 |
| `graphwar_game_recorder.rs` | 2 | 4 | — | 0 | 未知 |
| `graphwar_game_server.rs` | 3 | 86 | `src/graphwar_game_server.rs+src/server.rs` | 23 | 逐位 |
| `graphwar_game_state_manager.rs` | 33 | 207 | `src/game.rs+src/turnstate.rs` | 76 | 逐位 |
| `graphwar_graphics_manager.rs` | 6 | 8 | — | 0 | 未知 |
| `graphwar_lan_broadcaster_v6.rs` | 4 | 4 | — | 0 | 未做 |
| `graphwar_lan_vbox.rs` | 11 | 19 | `src/graphwar_lan_vbox.rs` | 37 | 观测 |
| `graphwar_level_generator.rs` | 4 | 26 | `src/graphwar_level_generator.rs+src/graphwar_level_generator/tables.rs` | 28 | 逐位 |
| `graphwar_level_manager.rs` | 40 | 590 | `src/graphwar_level_manager.rs` | 20 | 观测 |
| `graphwar_life_manager.rs` | 9 | 12 | `src/graphwar_life_manager.rs` | 7 | 逐位 |
| `graphwar_lobby_control.rs` | 1 | 6 | — | 0 | 未做 |
| `graphwar_local_player_manager.rs` | 3 | 15 | — | 0 | 未做 |
| `graphwar_logger.rs` | 2 | 2 | — | 0 | 未做 |
| `graphwar_long_press_animation.rs` | 7 | 7 | `src/graphwar_long_press_animation.rs` | 11 | 观测 |
| `graphwar_mode_manager.rs` | 2 | 6 | — | 0 | 未做 |
| `graphwar_module_container.rs` | 20 | 1 | — | 0 | 未做 |
| `graphwar_name_manager.rs` | 3 | 3 | — | 0 | 未做 |
| `graphwar_nodes.rs` | 11 | 1 | — | 0 | 未做 |
| `graphwar_obstacle_manager.rs` | 8 | 14 | `src/plane.rs` | 16 | 逐位 |
| `graphwar_path_calculator.rs` | 1 | 9 | `src/graphwar_path_calculator.rs` | 2 | 逐位 |
| `graphwar_plane.rs` | 10 | 19 | `src/graphwar_plane.rs+src/plane.rs` | 37 | 观测 |
| `graphwar_plane_pos_manager.rs` | 6 | 9 | `src/plane.rs` | 16 | 观测 |
| `graphwar_play_screen_control.rs` | 10 | 64 | `src/graphwar_play_screen_control.rs` | 8 | 观测 |
| `graphwar_player_config.rs` | 0 | 0 | — | 0 | 未做 |
| `graphwar_player_manager.rs` | 3 | 6 | — | 0 | 未知 |
| `graphwar_player_screen_control.rs` | 8 | 22 | — | 0 | 未知 |
| `graphwar_rank_manager.rs` | 4 | 4 | — | 0 | 未做 |
| `graphwar_remote_event_handler.rs` | 3 | 171 | `src/events.rs` | 2 | 观测 |
| `graphwar_replay_control.rs` | 2 | 9 | — | 0 | 未做 |
| `graphwar_replay_player.rs` | 5 | 18 | `src/graphwar_replay_player.rs` | 53 | 观测 |
| `graphwar_replay_reader.rs` | 0 | 0 | — | 0 | 未做 |
| `graphwar_root_node.rs` | 23 | 178 | `src/graphwar_root_node.rs` | 57 | 观测 |
| `graphwar_server_broker_handler.rs` | 3 | 5 | — | 0 | 未做 |
| `graphwar_server_connection_handler.rs` | 6 | 24 | `src/server.rs` | 7 | 逐位 |
| `graphwar_setup_control.rs` | 21 | 70 | `src/graphwar_setup_control.rs` | 48 | 观测 |
| `graphwar_shot_calculator.rs` | 1 | 12 | `src/graphwar_shot_calculator.rs+src/graphwar_shot_calculator/settle.rs` | 23 | 逐位 |
| `graphwar_soldier_manager.rs` | 4 | 5 | — | 0 | 未知 |
| `graphwar_stream_buffer.rs` | 0 | 0 | — | 0 | 未做 |
| `graphwar_target_manager.rs` | 3 | 4 | — | 0 | 未做 |
| `graphwar_team_manager.rs` | 7 | 10 | — | 0 | 未知 |
| `graphwar_texture_loader.rs` | 1 | 1 | — | 0 | 未知 |
| `graphwar_tick_handler.rs` | 2 | 3 | `src/graphwar_tick_handler.rs` | 9 | 逐位 |
| `graphwar_turn_manager.rs` | 14 | 45 | `src/graphwar_turn_manager.rs` | 60 | 逐位 |
| `graphwar_video_writer.rs` | 3 | 4 | — | 0 | 未知 |

### 2.2 非 `graphwar_` 前缀的游戏模块

| module | orig_funcs | anchors | ours_file | ours_funcs | status |
|---|---|---|---|---|---|
| `axis_drawer.rs` | 6 | 3 | `src/axis_drawer.rs` | 12 | 观测 |
| `backtracer.rs` | 0 | 0 | — | 0 | 未做 |
| `button_manager.rs` | 2 | 3 | — | 0 | 未知 |
| `effect_drawer.rs` | 11 | 17 | `src/effect_drawer.rs` | 19 | 观测 |
| `entity_drawer.rs` | 14 | 36 | `src/entity_drawer.rs` | 19 | 观测 |
| `function_button_manager.rs` | 2 | 3 | — | 0 | 未知 |
| `function_drawer.rs` | 1 | 1 | — | 0 | 未知 |
| `function_history_recorder.rs` | 1 | 3 | — | 0 | 未知 |
| `function_token.rs` | 0 | 0 | — | 0 | 未做 |
| `math_function.rs` | 7 | 15 | `src/math_function.rs+src/math_function/to_text.rs` | 43 | 逐位 |
| `obstacle_drawer.rs` | 2 | 7 | — | 0 | 未知 |
| `player_showcase.rs` | 10 | 18 | `src/player_showcase.rs` | 44 | 观测 |
| `polish_notation_converter.rs` | 1 | 2 | — | 0 | 未做 |
| `server_broker_request_connection.rs` | 0 | 1 | — | 0 | 未知 |
| `weak_caller.rs` | 39 | 2 | — | 0 | 未做 |

完整的 evidence（每行一句话的依据）在 `docs/coverage-matrix.tsv`，那张表是本脚本的唯一真源，本页的表格由它渲染。

## 3. 哪些模块连「它干什么」都不知道（问题 1）
**58 个 `graphwar_*.rs` 里有 12 个**属于这一类 —— 判据是：没有实现、且除模块名之外**没有任何**能指到地址/报文/真机的观测。

| module | orig_funcs | anchors | 现状 |
|---|---|---|---|
| `graphwar_computer_player_manager.rs` | 5 | 27 | 只有名字 |
| `graphwar_crash_reporter.rs` | 0 | 0 | 只有名字 |
| `graphwar_effect_manager.rs` | 1 | 3 | 只有名字 |
| `graphwar_entity_effect_manager.rs` | 3 | 4 | 只有名字 |
| `graphwar_game_recorder.rs` | 2 | 4 | 只有名字 |
| `graphwar_graphics_manager.rs` | 6 | 8 | 只有名字 |
| `graphwar_player_manager.rs` | 3 | 6 | 只有名字 |
| `graphwar_player_screen_control.rs` | 8 | 22 | 只有名字 |
| `graphwar_soldier_manager.rs` | 4 | 5 | 只有名字 |
| `graphwar_team_manager.rs` | 7 | 10 | 只有名字 |
| `graphwar_texture_loader.rs` | 1 | 1 | 只有名字 |
| `graphwar_video_writer.rs` | 3 | 4 | 只有名字 |

另外还有 **7 个模块一个 panic 锚都没有**（连「它有没有函数」都答不出来）：`graphwar_crash_reporter.rs`、`graphwar_event.rs`、`graphwar_event_queue.rs`、`graphwar_game_info.rs`、`graphwar_player_config.rs`、`graphwar_replay_reader.rs`、`graphwar_stream_buffer.rs`。
它们的 `orig_funcs` 是 0，**不代表模块为空**，只代表这个方法对它们完全失效。
非 graphwar 前缀里还有 6 个未知模块（`obstacle_drawer` / `function_drawer` / `button_manager` / `function_button_manager` / `function_history_recorder` / `player_showcase` / `server_broker_request_connection`），它们的共同点是**连「是不是游戏代码」都只有名字可依**。

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
| `src/_scaffold/gdext.rs` | GDExtension 入口脚手架（原版的入口在 Godot 模块侧，不在这 58+12 个文件里） |
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
2. **模块级分母（58）比函数级分母更可信，但它也是下限**：58 个名字是从二进制里的 `src/graphwar_*.rs` 字面量直接读出来的，只要模块里有一个 panic 点就会被列出来；**完全没有 panic 的模块不会出现**，所以真实的模块数只能 ≥ 58。函数级归属则只有 4.0%。
3. 想提高函数级覆盖，只有两条路：(a) 把 `0x3c2bc0` / `0x3c2530` 这类无锚函数**逐个反编译并人工归属**；(b) 沿调用图从锚点扩散 —— 但那是**推断**，`docs/02` §10 做过（885/13263，6.7%，hub 节点被高估），不能进总账。
4. 一次 `--check` 只要几秒到一分钟（取决于 capstone 扫 `.text`），**`src/` 一改，`ours_funcs` 就会漂，`--check` 会失败** —— 这是故意的：过期矩阵比没有矩阵更糟。

## 7. 与既有文档的口径差异（别以为是矛盾）

| 旧口径 | 出处 | 本表怎么处理 |
|---|---|---|
| 「544 个函数归属 / 56 个模块」 | `docs/02` §8 | 那张表把引用两个模块的函数在两个模块下**各算一次**。本表压平后不重复计数，并把跨模块数单独列为 `contested` |
| 「885/13263，观测 441 + 推断 444」 | `docs/02` §10 | 那是**调用图扩散**，属推断层。本表只收硬证据，不把扩散数写进总账 |
| 「57 个模块」 | 各处 | 按 `src/graphwar_[a-z_]+\.rs` 数是 57；**加数字字符后是 58**（`graphwar_lan_broadcaster_v6.rs` 被漏了）。本表用 58 |
| 「`fixtures/module_funcs.tsv` 的地址清单」 | `docs/02` §14.2 | 那张表已自我更正为「不能判定函数身份」；本表用 Ghidra 边界 + 冲突压平重算 |
| 「解析器 `0x4ed670` 在 `graphwar_function_input.rs`」 | `docs/02` §1 | 该函数体内唯一的 Location 锚是 `math_function.rs:75`，按证据归 `math_function.rs`；所以函数语言的实现落在本表的 `other` 口径，不在 58 个 graphwar 模块里 |

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
| `src/` 树（46 个文件、1265 条 `fn`） | `7ada03b5f6f950fc46c967ef89af4cd9c328041a5e32589fcfa9ce33c5e8e33a` |

