# 162 · manager 档位独立复核：`14 / 2 / 6 / 0` 到底几处对得上

> **一句话结论**：**四处口径全部 = 14 `wired` / 2 `layout` / 6 `assembled` / 0 `stub`（合计 22）**；
> 再加**运行时**实算（`assemble_check.gd` 跑出来 `{14, 2, 6, 0}`，**226 passed / 0 failed**）与
> `root_wiring.rs` **模块文档表**这一处静态度镜像，一共 **六处同值**。
> 三格（`Rank` / `Name` / `LocalPlayer`）的**构造点 / 订阅点 / 发布点**我在**原始 `.so` 上重新反汇编核了一遍，
> 全部落在文档所写的地址上（含两个"疑似悬案"的判否，见 §4.4）⇒ 按仓库自己的 `wired` 三件判据，**升档证据充分**。
>
> 但发现 **7 处文档侧不一致 + 1 处注释级不一致**（§6 D1–D8）：全部在**说明文字**里，**没有一处影响上面那四个数**；
> `docs/161` 占 4 处（1 处是**内部矛盾**：同一发布点的 tag 在 §2 写 **7**、在 §2.1 写 **8**；
> 2 处是"某文件仍写着旧值"的登记**已经过期**——那两个文件其实都改好了；1 处是标题停在升档前），
> `README.md` 占 2 处（仍停在 `11 / 2 / 9 / 0` 与 `assemble_check 217`），
> `assemble_check.gd` 占 1 处（一句文案还写"档位 `stub` 的那 1 个"），另 1 处是 `src` 注释里的 3 字节地址偏差（D8）。**本轮只登记、不改任何别人的文件**（只读复核）。

**复核者立场**：不读第十五轮的结论，四个数各自从代码/测试里重新算；不一致按"文件 + 行号 + 实际值"记。
地址一律 **ELF 虚地址**（Ghidra = 本值 + `0x100000`，只加一次）。

**审计对象**：`be74cbd`（= 克隆时的 master）。复核结束后 master 前进到 `c5c8c87`，其间只改了
`docs/00-status.md`（`:117` 未变）、`docs/HANDOVER.md` 与 `docs/guide/**`；**四个口径文件
（`root_wiring.rs` / `assemble_check.gd` / `docs/161` / `README.md`）逐字未变**
⇒ §1 的四个数、§2 的逐槽表、§6 的 D 清单在 `c5c8c87` 上同样成立，唯一漂移是
`docs/HANDOVER.md` 的行号（`:85` → `:90`，内容仍是 `14 wired / 2 layout / 6 assembled / 0 stub`）。

---

## 1. 四个口径 × 实际值 × 是否一致

| # | 口径 | 位置（文件:行） | 我实算的值 | 与 `14/2/6/0` |
|---|---|---|---|---|
| ① | `Wired::status()` 的匹配臂 | `src/_scaffold/root_wiring.rs:601-624` | `wired` 臂 **14** / `layout` 臂 **2** / 落 `_ =>` **6** / `stub` **0** | ✅ 一致 |
| ② | 四个 `*_TIER_COUNT` + 编译期闸 | 同文件 `:457-463`，闸在 `:465-469` | **14 / 2 / 6 / 0**；`assert!(四者之和 == Wired::COUNT)` | ✅ 一致 |
| ③ | `client/tools/assemble_check.gd` 的 `TIER_TALLY` | `client/tools/assemble_check.gd:120`（逐槽表 `:72-118`） | **14 / 2 / 6 / 0**；逐槽 22 条与 ① **逐行相同** | ✅ 一致 |
| ④ | `docs/161-manager-gaps.md` 的清单 | §0.1 表 `:24-26` / §2 主表 `:112-120` / §9 `:366` | **14 / 2 / 6 / 0**（九行清单里 **3 行**标 ✅ 已升、**6 行**未动） | ✅ 一致 |
| ⑤ | **运行时**实算（补充口径，不在"四处"里） | `assemble_check.gd` 跑一次 | `{ "wired": 14, "layout": 2, "assembled": 6, "stub": 0 }`，`ASSEMBLE 226 passed, 0 failed` | ✅ 一致 |
| ⑥ | `root_wiring.rs` **模块文档表**（静态度镜像） | `src/_scaffold/root_wiring.rs:29-52` | **14 / 2 / 6 / 0**，22 行与 ① 逐行相同 | ✅ 一致 |

**结论：四处（含运行时与模块文档共六处）全部 `14 / 2 / 6 / 0`，没有一处不一致。**
"四处同步"这句话在**数字**上成立；不成立的部分全在 §6 的**文字**里。

两个口径细节（都影响"这句话有多强"，值得记住）：

1. **`stub = 0` 是结构性的，不是巧合**：`status()` 里根本没有 `=> SLOT_STUB` 臂（`:601-624`），
   所以 `STUB_TIER_COUNT: usize = 0` 与判据函数**不可能**分叉 —— 除非有人加回那条臂。
2. **编译期闸只保证"和 = 22"，不保证分布**：`assert!(WIRED+LAYOUT+ASSEMBLED+STUB == 22)` 在
   `:466-469`。分布若错成"15 / 2 / 5 / 0"仍然和 = 22、照样编过。真正钉住分布的，是
   `:2513-2520` 那条把 22 个槽逐个数出来的断言（在单测 `unwired_module_types_are_placeholders`，`:2474` 起）与 `:2535` 起的逐槽
   `slot_tiers_are_the_four_word_vocabulary`，加上 `assemble_check.gd` 的 `TIER_TALLY` 双向对拍。
   ⇒ 说"有编译期闸"是对的，说"改一槽漏改常量就编不过"也对（和会变 23），但**别把它当成分布的闸**。

---

## 2. 逐槽明细（22 槽，六个来源同值）

顺序 = `Wired::ALL` = 原版 `GraphwarGameClient::new` 的注册序（22 条 `add_module`）。
"四处同值"一列是我逐行对拍的结果（脚本见 §7，第 ③ 项输出 `22/22 一致`）。

| # | 槽（原版类型名） | 档位 | 四处 + 模块文档表 | 停在这一档的理由（文档口径，未复核其结论） |
|---|---|---|---|---|
| 1 | `GraphwarTickHandler` | `wired` | ✅ | 订 1 发 2 + 4 条交互 |
| 2 | `GraphwarClientConnectionHandler` | `layout` | ✅ | 112 B 九偏移全解，行为 0 行 |
| 3 | `GraphwarLocalPlayerManager` | `wired` | ✅ | **任务 74 升**：订 3 发 6 + 交互 |
| 4 | `GraphwarPlayerManager` | `wired` | ✅ | 订 2 发 2 + 6 条交互（任务 40） |
| 5 | `GraphwarSoldierManager` | `assembled` | ✅ | 只到类型 + 部分行为 |
| 6 | `GraphwarPlanePosManager` | `wired` | ✅ | 24 B 全解 + 订 2 发 1（任务 36） |
| 7 | `GraphwarGameStateManager` | `assembled` | ✅ | 事件层在禁区 `src/game.rs` |
| 8 | `GraphwarFunctionManager` | `layout` | ✅ | 布局全解、**发方向**未接 |
| 9 | `GraphwarTargetManager` | `wired` | ✅ | 订 2 发 1 + 5 条 |
| 10 | `GraphwarTeamManager` | `wired` | ✅ | 订 2 发 2 + 7 条 |
| 11 | `GraphwarLifeManager` | `wired` | ✅ | 订 3 发 2 + 5 条 |
| 12 | `GraphwarObstacleManager` | `wired` | ✅ | 订 1 发 4 + 7 条 |
| 13 | `GraphwarTurnManager` | `wired` | ✅ | 订 2 发 1 + 6 条 |
| 14 | `GraphwarEffectManager` | `wired` | ✅ | 订 1 发 1 + 7 条 |
| 15 | `GraphwarModeManager` | `wired` | ✅ | 订 1 发 3 + 5 条 |
| 16 | `GraphwarGraphicsManager` | `assembled` | ✅ | 三张表 + 4 处发布已读，未接线 |
| 17 | `GraphwarEntityEffectManager` | `wired` | ✅ | 订 2 发 1 + 7 条 |
| 18 | `GraphwarComputerPlayerManager` | `assembled` | ✅ | `BotData` 大半未解 |
| 19 | `FunctionHistoryRecorder` | `assembled` | ✅ | 33/35 口径未裁 |
| 20 | `GraphwarNameManager` | `wired` | ✅ | **任务 74 升**：订 2 发 2 + 交互 |
| 21 | `GraphwarColorManager` | `assembled` | ✅ | `+0x10` 字段语义未定 |
| 22 | `GraphwarRankManager` | `wired` | ✅ | **任务 74 升**：订 2 发 2 + 交互 |

合计：**`wired` 14 / `layout` 2 / `assembled` 6 / `stub` 0 = 22**。
`assembled` 六格 = `5 Soldier` / `7 GameState` / `16 Graphics` / `18 ComputerPlayer` /
`19 FunctionHistoryRecorder` / `21 Color`，与 `docs/161` §0.2 的"现在剩的 6 个"逐格相同。

---

## 3. 方法（每一处都自己算，不抄结论）

* **① 判据函数**：不数注释，把 `status()` 的函数体取出来，**对 `Wired::ALL` 的 22 个槽逐个
  往后找最近的 `=> SLOT_x`**；某槽一次都没出现在任何臂里 ⇒ 落通配臂（其目标从 `_ =>` 现读，
  不写死）。脚本见 §7 第 ③ 项。
* **② 常量 + 闸**：只 `grep` 常量字面值，**再由编译器验闸** —— `cargo build --release
  --features gdext-bindings` 通过（闸在 `const _` 上，编译期必跑）⇒ `14+2+6+0 == 22` 成立。
* **③ 运行时**：`assemble_check.gd` 用 `module_slot_status()` 逐槽取回**编译出来的 Rust 答案**，
  自己累计四档并与 `TIER_TALLY` 比（`:250-256`）。这一步不是我算的、也不是文档写的，
  是**产物自己报的**，所以单独列一行口径（§1 第 ⑤ 行）。
* **④ 文档清单**：`docs/161` §2 主表九行，按行首编号解析、按是否含"已升 `wired`"计数
  ⇒ **3 已完成 / 6 未动**，与"9 → 3 升 / 6 剩"自洽。
* **⑤ 地址级**：见 §4，全部在**原始 `.so`**（`sha256 9a538115…bed187`，Linux 2.0.60 / 2026-08-28）
  上重新反汇编/quoted relocation，**不看注释里写的地址对不对，而是按地址去读指令**。

---

## 4. 三格地址级证据抽查（我自己反汇编，不是抄清单）

判据（`docs/161` §1.1，本轮照用）：**容器 + 事件流两方向（订阅 / 发布都指到指令）+ 最小交互断言**，
三件齐才算 `wired`。下面每一行都是我用 `tools/addr_probe.py dis` 与 `readelf -r` 亲自读出来的，
可复跑（§7 第 ④ 项）。

### 4.1 `GraphwarRankManager`（#22，72 B）

**构造点**：`0x454f30` 是真正的函数入口（FDE 与符号都对得上），体内两条订阅：

```text
0x454f30  push r15 / push r14 / push rbx            ← GraphwarRankManager::new
0x454f9c  lea rsi, [0x1c0eef]   'new_connection_event'   + 0x454faa mov edx, 0x14 (len 20)
0x454fa3  lea r8, [0x9567d8]                            ← 订阅器句柄结构
0x454fb5  call [0x9bac88] -> 0x3d38f0                   ← subscribe_to_event
0x454fed  lea rsi, [0x1c0e12]   'entity_removed_event'   + 0x454ffb mov edx, 0x14
0x455006  call [0x9bac88] -> 0x3d38f0                   ← 第二条订阅
```

句柄那一跳我用**重定位表**单独验（`.data.rel.ro` 里存的是 0，真值在 `.rela.dyn` 的 addend 上）：

```text
0000009567f8  R_X86_64_RELATIVE  454f10     ← 第①条的处理器
000000956820  R_X86_64_RELATIVE  454ef0     ← 第②条的处理器
0x454f10: mov rdi,[rdi] / inc [rdi+8] / lea rsi,[0x454d80] / jmp 0x3d27d0   ⇒ 真处理器 = 0x454d80
```

**发布点**两处，tag word 都是 `0x8000_0000_0000_0043` = **67**（`RankInfo`）：

```text
0x454e1f  movabs rax, 0x8000000000000043
0x454e35  call [0x9bac80] -> 0x3d3530      ← send_all_info_events（整表重播）
0x45510b  movabs rax, 0x8000000000000043
0x45511f  call [0x9bac80] -> 0x3d3530      ← set_rank
```

**代码侧落地**：`src/graphwar_rank_manager.rs:11-14` 的函数表（`new` = `0x454f30` / `set_rank` =
`0x455090` / `send_all_info_events` = `0x454d80` / `handle_entity_removed_event` = `0x454e80`）、
`:102-116` 的**指令级**块（与我上面读到的一字不差）、`:170-181` 的 `SUBSCRIPTIONS: [Subscription; 2]`。
**交互读数 9 条** `rank.*`（`root_wiring.rs:1899-1916`，`assemble_check.gd:458-463` 断 3 条）。

### 4.2 `GraphwarNameManager`（#20，72 B）

```text
0x519100  （new，FDE 入口）   0x5191f0 'new_connection_event' + edx=0x14
0x519209  call [0x9bac88] -> 0x3d38f0
0x51923c  'entity_removed_event' + 0x51924a edx=0x14
0x519255  call [0x9bac88] -> 0x3d38f0
```

处理器更是一跳就到底（`readelf -r`）：`0x962800 → 0x5190c0`、`0x962828 → 0x5190e0`
（正是 `docs/161` §2.1 写的两个"跳板"值）。发布两处，tag word `0x8000_0000_0000_003c` = **60**（`NameInfo`）：

```text
0x518fcd  movabs rax, 0x800000000000003c
0x518fe5  call [0x9bac80] -> 0x3d3530      ← send_all_info_events
0x5193e8  call [0x9bac80] -> 0x3d3530      ← set_name（0x5193af 起拼 tag）
```

代码侧：`src/graphwar_name_manager.rs:11-16` 函数表、`:121-131` 指令级块、`:186-197`
`SUBSCRIPTIONS: [Subscription; 2]`；交互读数 8 条 `name.*`（`root_wiring.rs:1926-1939`）。

### 4.3 `GraphwarLocalPlayerManager`（#3，64 B）—— 唯一没有独立 `new` 的一格

"`new` 内联在 `GraphwarGameClient::new` 里"这句话我验了**FDE 边界**（不是看注释）：

```text
$ tools/addr_probe.py fde 0x517690
eh_frame   0x517690..0x51823a  size 0xbaa          ← 一个函数体
$ tools/addr_probe.py fde 0x517a77
note       NOT an FDE function start               ← 第三条订阅落在上面那个体内
```

三条订阅（全在 `0x517690` 体内，名字/长度/@站点）：`0x5179d4`（`connected_to_server_event`，len 0x19）/
`0x517a27`（`player_info_event`，len 0x11）/ `0x517a77`（`entity_removed_event`，len 0x14）。
发布六条 —— 五条在 `0x519f40`，tag 用 `rbp = 0x7fff_ffff_ffff_ffff` 加立即数拼出：

```text
0x51a162 lea rax,[rbp+0x3c]  ⇒ 0x800000000000003b = 59   @0x51a170 call [0x9bac80]
0x51a239 lea rax,[rbp+0x33]  ⇒ 0x8000000000000032 = 50   @0x51a247
0x51a310 lea rax,[rbp+0x35]  ⇒ 0x8000000000000034 = 52   @0x51a31e
0x51a3e7 lea rax,[rbp+0x37]  ⇒ 0x8000000000000036 = 54   @0x51a3f5
0x51a443 add rbp,0x43        ⇒ 0x8000000000000042 = 66   @0x51a451
```

第六条在 `0x51a570`（`handle_connected_to_server`），**这是本轮查到的一处"文档自相矛盾"的现场**：

```text
0x51a577  movabs rax, 0x8000000000000007
0x51a581  cmp    qword ptr [rsi], rax       ← 这是在**断言收到的**事件 tag == 7
0x51a5b1  inc    rax                        ← ⇒ rax = 0x8000_0000_0000_0008
0x51a5b8  mov    rsi, rsp
0x51a5bb  call [0x9bac80] -> 0x3d3530       ← 发出去的是 **8**（NewPlayerRequest）
```

⇒ **发布 tag = 8**，`7` 只是"订阅进来的那条"的断言值。三处旁证同值：
`src/events.rs:82`（`NewPlayerRequest = 8`）、`src/graphwar_local_player_manager.rs:411`（"…里 8"）、
`assemble_check.gd:77` 注释（"0x51a5bb 的 8"）。**`docs/161` §2 第 3 行写的 `7` 是错的**（§6 D4）。

### 4.4 顺便判否的两个"可能悬案"

* `docs/161` 的三跳（`0x454f10 → 0x454d80` / `0x5190c0 → 0x518f10` / `0x5190e0 → 0x519030`）
  **都成立**：重定位 addend 与 thunk 体内的 `lea rsi,<handler>` 两边对得上（§4.1/§4.2 已贴）。
* `addr_probe dis 0x454f30` 里 `lea r8,[0x9567d8]` 注解出的 `-> 0x45eea0` 一开始像个反例：
  `0x45eea0` 是 **drop glue**（`decq [rdi+8]` + 按 `0x60` 释放），不是处理器。
  原因在**结构**：`0x9567d8` 是一张 `{drop, …}` 表，**处理器在 `+0x20`**（`0x9567f8`），
  与源码注释 `src/graphwar_rank_manager.rs:104` 的 `lea r8,<0x9567d8> +0x20 = 0x9567f8` 一致。
  ⇒ 不是错，是"读第一个字会读到析构器"。

**证据充分性判定**：三格的**构造点 / 订阅点 / 发布点**都独立可复现到指令，`wired` 的三件判据齐；
`Rank` 剩 `+0x08` 序列化口径（G3）、`LocalPlayer` 剩第五道门那张表（G7）——按 `docs/161` §1.1 的口径
这两条**不阻塞** `wired`（档位只要求订阅/发布/断言）。**判：够。**

---

## 5. `assemble_check` 实测条数 = **226**

```text
$ export GODOT=/home/user/MyProject/gw2/tools/Godot_v4.7.2-stable_linux.x86_64
$ cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/
$ "$GODOT" --headless --path client --import
$ "$GODOT" --headless --path client --script res://tools/assemble_check.gd
...
      四档: { "wired": 14, "layout": 2, "assembled": 6, "stub": 0 }（越界 []）
ok   四档计数 == wired 14 / layout 2 / assembled 6 / stub 0（实际 {…同上…}）
ok   四档合计 == 22（每个槽只报一档）
ok   GraphwarLocalPlayerManager 的档位 = wired（实际 wired）
ok   GraphwarNameManager 的档位 = wired（实际 wired）
ok   GraphwarRankManager 的档位 = wired（实际 wired）
ASSEMBLE 226 passed, 0 failed
```

* `ok` 行实计数：**226**；`FAIL` 行 **0**；退出码 0。与 `docs/161` §9（`217 → 226`，+9）一致。
* `+9` 我也逐条数了：`assemble_check.gd:458-476` 恰好 **9** 个 `ok()`，三格各 3 条
  （订阅 / 发布 / 状态）—— 与"三格各 3 条"的说法对得上。
* 旁证（独立于上面这条命令）：`README.md:162` 那 14 个脚本的条数加起来 = 1818 = `docs/161` §9
  的"before 合计 1818"；把 `assemble_check` 从 217 换成 226 ⇒ 1827，正是 §9 的 after 值。

---

## 6. 发现的差异（**只登记，不改**；都不影响 §1 的四个数）

| # | 文件:行 | 现在写的 | 实际值 / 应为 | 严重度 |
|---|---|---|---|---|
| **D1** | `docs/161-manager-gaps.md:20` | 标题「### 0.1 三个互相独立的口径，全部给出 **9**」 | 该节表内三行都是 **14 / 2 / 6 / 0** ⇒ 标题停在升档前 | 低（纯标题） |
| **D2** | `docs/161-manager-gaps.md:60-63` | 「`docs/00-status.md` §0.1b.2 与 `docs/HANDOVER.md` §遗留 那两处**仍写着**旧的 9 格 / 11-2-9-0」 | **已经不成立**：`docs/00-status.md:117` = "6 个 manager 仍是 `assembled`（14 wired / 2 layout / 6 assembled / 0 stub = 22）"，`docs/HANDOVER.md:85` 同值 —— 合并提交 `be74cbd` 已同批改掉 | 中（读者照它去找"没改的地方"会扑空） |
| **D3** | `docs/161-manager-gaps.md:327`（§7 `C7`） | "**登记**（这两个文件不在任务 74 的允许清单里）。下一轮改它们的人顺手改" | 同上：**这两个文件已经改过了**（见 D2 的实际值） | 低（与 D2 同因） |
| **D4** | `docs/161-manager-gaps.md:112`（§2 第 3 行） | "`handle_connected_to_server`(`0x51a570`) 里 1 处 = **7**`@0x51a5bb`" | **8**：`0x51a577` 的 `movabs …07` 是**入向**断言，`0x51a5b1 inc rax` 之后发出去的是 **8**（`NewPlayerRequest`）。**同一篇文档 §2.1（`:127`）写的就是 8** ⇒ 文档内部打架 | 中（唯一一处"数值级"错误，且自相矛盾） |
| **D5** | `README.md:91` | "**9 个 manager 仍是 `assembled`**（11 wired / 2 layout / 9 assembled / 0 stub = 22）…建议顺序 Rank → Name → LocalPlayer" | **6 个**（`5/7/16/18/19/21`）；三格 **已完成**，`14 / 2 / 6 / 0`。建议顺序里那三格要划掉 | 中（对外门面页，读者第一眼看到的就是它） |
| **D6** | `README.md:162` | "`assemble_check` 217 …共 **1,800** 条" | `assemble_check` = **226**；14 个脚本次实为 **1,827**（清单自身逐项相加 = 1818，headline 的 1,800 也与自己的清单不符） | 低（回归条数，`AGENTS.md` §0.2 本就不当进度） |
| **D7** | `client/tools/assemble_check.gd:237-238` | 断言文案"…（`SlotKind::Stub`；**不是**档位 `stub` 的**那 1 个**）" | 档位 `stub` 现在是 **0 个**（`root_wiring.rs:481` 自己就写着 0）；`SlotKind::Stub` 是 5 个。**只错文案，断言本身（`== 5`）是对的** | 低（纯文案；每次跑都会把它 print 出来） |
| **D8** | `src/graphwar_local_player_manager.rs:165` | 「`0x517a5e lea rsi,'entity_removed_event'` + `0x517a6c mov edx,0x14`」 | `mov edx,0x14` 实际在 **`0x517a6f`**；`0x517a6c` 那条是 `mov rdi, rsp`（`lea rsi` 7 B → `lea r8` 7 B → 3 B 之后才是它）。**注释级偏差 3 字节，不影响任何结论或断言** | 很低（`AGENTS.md` §0.3 里注释也是证据，所以登记） |

**另有两处"看着像、其实不是"的命中，别误改**（我逐条读过了）：

* `docs/HANDOVER.md:177` 的「在跑的 第十五轮：① 9 个 manager 升档」是**轮次标题**（那一轮就是
  "把 9 个 `assembled` 里的几格升上去"），不是现值计数；
* `docs/00-status.md:223` 的「剩余 19 个 manager 的装配」是**节点化/装配**那条线的旧行，
  与档位是两件事（档位 22 个槽全都装进容器了，`container_module_count() == 22`）。

处置建议（不在本轮权限内，供下一轮派活）：
`D2`/`D3` 是**同一件事的两半**——`be74cbd` 已经把 `docs/00`/`HANDOVER` 改好了，所以 `docs/161`
那两段应当**从"登记"改成"已修"**（保留"起因"说明即可，别把"仍写着旧值"留着）；
`D4` 只改一个字符（`7` → `8`），顺手把 §2 第 3 行的"门是五道"补上 tag 的出处；
`D5`/`D6` 一起改（`README.md` 两行）；`D7` 把"那 1 个"改成"（现在是 **0** 个）"。

---

## 7. 复现（干净克隆，全部只读；数字都是这套命令出来的）

```bash
WS=/home/user/MyProject/gw2
export CARGO_HOME=$WS/.cargo-home CARGO_TARGET_DIR=$WS/.agent-work/tiers/target

# ① 判据函数：逐槽现算（不数注释；通配臂的目标也现读）
python3 - <<'PY'
import collections, re
RS = "src/_scaffold/root_wiring.rs"
body = re.search(r"pub const fn status\(self\) -> &'static str \{\n(.*?)\n    \}\n",
                 open(RS, encoding="utf-8").read(), re.S).group(1)
ALL = re.findall(r"Wired::(\w+)", re.search(
    r"pub const ALL: \[Wired; Self::COUNT\] = \[\n(.*?)\n    \];", open(RS, encoding="utf-8").read(), re.S).group(1))
WILD = re.search(r"_ =>\s*SLOT_(WIRED|LAYOUT|ASSEMBLED|STUB)", body).group(1).lower()
def tier_of(name):
    hits = set()
    for m in re.finditer(r"Wired::" + name + r"\b", body):
        t = re.search(r"=>\s*SLOT_(WIRED|LAYOUT|ASSEMBLED|STUB)", body[m.end():])
        hits.add(t.group(1).lower() if t else "?")
    return WILD if not hits else hits.pop()
t = collections.Counter(tier_of(n) for n in ALL)
print("status() 合计 = %d/%d/%d/%d（槽数 %d）" % (t["wired"], t["layout"], t["assembled"], t["stub"], len(ALL)))
PY

# ② 四个常量 + 闸（闸由编译器验：release 构建通过即 14+2+6+0 == 22）
grep -n "_TIER_COUNT: usize =" src/_scaffold/root_wiring.rs
cargo build --release --offline --features gdext-bindings

# ③ 运行时断言表 + 逐槽对拍
grep -n "const TIER_TALLY := " client/tools/assemble_check.gd
sed -n '/const WIRED_TIER :=/,/^]/p' client/tools/assemble_check.gd \
  | grep -c '"wired"\|"layout"\|"assembled"\|"stub"'      # 22 条逐槽

# ④ 三个数之外的两处静态度：模块文档表 / docs/161 清单
grep -c '^//! | [0-9]* | .*`wired` |' src/_scaffold/root_wiring.rs
awk '/^## 2\./,/^### 2\.1/' docs/161-manager-gaps.md | grep -c "已升 \`wired\`"   # 3

# ⑤ 运行时（226）：先 import 再只跑这一个脚本
cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/
"$WS/tools/Godot_v4.7.2-stable_linux.x86_64" --headless --path client --import
"$WS/tools/Godot_v4.7.2-stable_linux.x86_64" --headless --path client \
  --script res://tools/assemble_check.gd | tail -3

# ⑥ 地址级抽查（目标 .so：Linux 2.0.60 / 2026-08-28，sha256 9a538115…bed187）
PY=$WS/.venv/bin/python
$PY tools/addr_probe.py dis 0x454f30        # Rank new：两条订阅 @0x454fb5 / 0x455006
$PY tools/addr_probe.py dis 0x455090        # Rank set_rank：tag word …43 + @0x45511f
$PY tools/addr_probe.py dis 0x51a570        # LocalPlayer 第 6 条发布：…07 断言 → inc → 发 8
$PY tools/addr_probe.py fde 0x517690        # LocalPlayer 的 new 是内联：0x517690..0x51823a
readelf -r $WS/.reference/steam/2026-08-28/libgraphwarrust.so | grep -E "9567f8|956820|962800|962828"

# ⑦ 文档链（本报告落地后必须 rc=0）
python3 tools/check_doc_refs.py; echo rc=$?
```

---

## 8. 边界（本报告**不**主张的）

* **不**复核 14 个 `wired` 的**全部**订阅/发布（只抽了任务 74 升的三格，且是"抽"，不是全量；
  其余 11 格沿用既有文档，本轮没重读）。
* **不**复核交互读数（`rank.*` / `name.*` / `local_player.*`）的**语义**是否正确 —— 只核了
  "这些读数存在、条数与文档一致、断言在跑并通过"。按 `AGENTS.md` §0.2，`assemble_check` 的
  226 条是**回归保护**，本身不构成复原证据；本报告把它当"四处口径的第三方仲裁"用，不当证据用。
* **不**判 `layout` / `assembled` 那 8 格该不该升（那是任务 75/下一轮的事），只判"记账是否对"。
* **不**核 `.so` 三个构建（arm64 / x86 Linux / Windows）的地址是否互相对得上；本轮只读
  **x86 Linux 2.0.60（2026-08-28）** 这一个。
* 本报告**没有改动** `src/`、`client/`、`tools/`、`fixtures/`、`docs/`（除了新增本文件与
  `docs/INDEX.md` 的一行）——§6 的 7 条差异因此只登记、不就地修。
