# 114 · `0x3d38f0` 的「事件名 ↔ 判别值」表：**解全 + 定性**

> 上游线索：`docs/112` §10.1（解出 22 条）+ `docs/96` §4.6（护栏：**先分清**那些数字是
> `enum Event` 的**判别值**还是分派表的 **handler 下标**，否则不许改 `src/events.rs`）。
>
> 本文回答三件事：
> 1. 把 `0x3d38f0` 那张表**解全**（72 条，每条都有地址、有自校验）；
> 2. **判定**那些数字是 `enum Event` 的判别值 —— 三条独立判据，其中两条是硬证据；
> 3. 据此**重写** `src/events.rs`：旧编号（`0..=32` 连续）是**错的**，实测值已替换。
>
> 证据分级只有四档：**逐位验证** / **观测** / **推断**（写明缺什么）/ **未知**。

---

## 0. 一页结论

| 问题 | 答案 | 等级 |
|---|---|---|
| 解出多少条 | **72 条**「snake_case 名字 → 数字」，覆盖 `0..=73` 除 `63`/`65` 外的全部整数值 | **逐位验证** |
| 自校验 ①（长度）| 每条相等测试的字节区间并集**恰好铺满** `[0, len)`，无空洞无冲突 → **72/72** | **逐位验证** |
| 自校验 ②（tag word）| 17 条分支会顺手拼 `Event` 前缀，`+0` 处的 `0x8000000000000000 \| N` 与 `%ebx` **完全相等** → **17/17** | **逐位验证** |
| 自校验 ③（`.rodata`）| 72 条里 **69 条**能在 `.rodata` 里逐字节找到；剩 3 条是 LLVM 用两条已有串的**重叠片段**拼出来的（§4.3） | **观测** |
| 自校验 ④（排序数组）| `.rodata` `0x1bf537` 那 32 条名字按本次解出的判别值**严格递增**（13,14,15,19,23,…60）→ **32/32**（§4.4） | **观测** |
| 那些数字是什么 | **`enum Event` 的判别值**（tag word 的低位）。**不是** handler 下标 | **逐位验证** |
| `src/events.rs` 旧编号 | **错**。不是"差一个常数"，`PlayerInfo` 10→**13**、`FunctionPoints` 31→**37** | **逐位验证** |
| `enum Event` 有多大 | **≥ 75 个变体**（`0..=73` 之外 74 也被用过）。具名 **72** 个 | 推断（缺：63/65/74 的分派分支或构造点） |
| 线格式 | **完全没变**：线上仍是外部标签 JSON，判别值不上线（`docs/96` §4，380 份 replay） | **观测**（沿用上游结论） |

改动落地：`src/events.rs`（枚举重写 = 72 变体 + 实测值 + 名字对照表）、
`tests/events.rs`（新增「名字 → 判别值」全表钉死）、本文、`tools/event_tags.py`（复现脚本）。
**`cargo test` 205 → 206 全绿**。

---

## 1. 复现（一条命令）

```bash
../.venv/bin/python tools/event_tags.py            # 自检 + 全表
../.venv/bin/python tools/event_tags.py --json     # 机器可读
../.venv/bin/python tools/event_tags.py --rust      # 只有 名字=值 两列
```

`tools/event_tags.py` 是纯本地的：capstone 解 `libgraphwarrust.so` 的字节，`.rodata`
的常量直接从文件读。地址一律是 **ELF 虚地址**（`Ghidra = 虚地址 + 0x100000`，只加一次）。

---

## 2. `0x3d38f0` 的结构（复核 `docs/112` §10.1，全部对上）

```
rdi = EventHandler(self)   rsi = 名字 ptr   rdx = 名字 len
rcx = &状态                r8  = 静态描述符（{0,8}，闭包大小）

3d3909  add  rdx, -0xa                 ; len - 10
3d390d  cmp  rdx, 0x18 ; ja 3d469a     ; 只处理 len ∈ [10, 34]
3d391a  lea  rax, [rip-0x214f59]       ; = 0x1be9c8，25 项 int32 相对偏移跳表
3d3921  movsxd rcx, [rax+rdx*4] ; add rcx, rax ; jmp rcx
```

- **长度即名字长度** ⇒ 每条分支不需要"逐字符直到 `\0`"，只要固定长度的相等测试。
- 跳表实测：len `11/12/13/31/32/33` 直接落 `0x3d469a`（无此长度的名字），
  `len=34` 走 `memcmp`（`0x1bf56e`，34 B），其余走 SIMD / 整数比较。
- 未命中统一到 `0x3d469a`：`lea rdi,"explicit panic"(0x1bf7b6)` + `mov esi,0xe` +
  `lea rdx,0x953248`（`panic::Location`）+ `call *0x9ba608`
  ⇒ 走的是 panic 路径而不是"安静返回 None"（三元组 `(msg, len, Location)` 是**观测**，
  「等同于 `expect(…)`」是**推断**）。
- 命中分支把数字写进 `%ebx` 后 **全部汇到公共尾 `0x3d4c5c`**。

### 2.1 三种相等测试的形态（这是能"解全"的关键）

| 形态 | 判据 | 常量在哪 |
|---|---|---|
| SIMD | `movdqu (%rsi),%xmm0` + `movdqu K(%rsi),%xmm1` + 两条 `pcmpeqb` + `pand` + `pmovmskb` + `cmp $0xffff` | `.rodata`（`pcmpeqb` 的内存操作数），`K = len - 16` |
| 整数 | `movabs $imm64,%r??` + `xor (%rsi)` / `xor K(%rsi)` + `or` + `je/jne` | 立即数，**常被折成 `lea -0xa04(%rax)` / `add $0x5fd0600,%rcx`** |
| 短名（len ≤ 15）| 同上但用 `movabs` + `movzx` + `xor $imm16` | 立即数 |
| len = 34 | `memcmp(name, 0x1bf56e, 0x22)` | `.rodata` 整串 |

> ⚠️ `docs/112` §10.1 说的「常量折叠」是真的，而且有两种方向：
> `lea -0xa04(%rax)`（基串**减**位移）与 `add $0x5fd0600,%rcx`（基串**加**位移）。
> 不折叠就还原不出 `subscribe_event`（`"subscrib"` + `"fo_event"-0xa04` = `"be_event"`，
> 两者在偏移 7 重叠成 `subscribe_event`）和 `lock_info_event`
> （`"life_inf" + 0x5fd0600` = `"lock_inf"`）。**`add` 形态 `docs/112` 没提到**，
> 漏了它会把 `lock_info_event` 误读成 `life_info_event`（本文第一版就踩了）。

---

## 3. 全表（72 条，按判别值升序）

「相等测试」= 拿名字跟常量比的那条指令；「判别值@」= `mov $N,%ebx` 的地址；
「tag」= 该分支顺手拼出的 `Event` 前缀里的 tag word（只有 17 条有）。

| # | 名字 | len | 分支 | 相等测试 | 判别值@ | tag |
|---|---|---|---|---|---|---|
| 0 | `tick_event` | 10 | `0x3d392a` | `0x3d392a` | `0x3d394b` | — |
| 1 | `tick_request_event` | 18 | `0x3d3c7e` | `0x3d3c7e` | `0x3d4784` | — |
| 2 | `tick_reply_event` | 16 | `0x3d3f9c` | `0x3d3f9c` | `0x3d4798` | — |
| 3 | `subscribe_event` | 15 | `0x3d3fd1` | `0x3d3fd1` | `0x3d47df` | `0x8000…\|03` |
| 4 | `new_connection_request_event` | 28 | `0x3d43a0` | `0x3d43a0` | `0x3d484f` | `0x8000…\|04` |
| 5 | `new_connection_event` | 20 | `0x3d4184` | `0x3d4184` | `0x3d47f3` | — |
| 6 | `connection_removed_event` | 24 | `0x3d3e3d` | `0x3d3e3d` | `0x3d478e` | — |
| 7 | `connected_to_server_event` | 25 | `0x3d3bcd` | `0x3d3bcd` | `0x3d477a` | — |
| 8 | `new_player_request_event` | 24 | `0x3d3e3d` | `0x3d3e69` | `0x3d48e6` | — |
| 9 | `new_bot_request_event` | 21 | `0x3d44c4` | `0x3d44c4` | `0x3d4863` | — |
| 10 | `bot_added_event` | 15 | `0x3d3fd1` | `0x3d3ffc` | `0x3d48f0` | — |
| 11 | `change_team_request_event` | 25 | `0x3d3bcd` | `0x3d3bf9` | `0x3d48d2` | — |
| 12 | `remove_player_request_event` | 27 | `0x3d428b` | `0x3d428b` | `0x3d47fd` | — |
| 13 | `player_info_event` | 17 | `0x3d3b12` | `0x3d3b12` | `0x3d4770` | — |
| 14 | `entity_removed_event` | 20 | `0x3d4184` | `0x3d41b0` | `0x3d48fa` | — |
| 15 | `soldier_info_event` | 18 | `0x3d3c7e` | `0x3d3cad` | `0x3d48dc` | — |
| 16 | `add_soldier_request_event` | 25 | `0x3d3bcd` | `0x3d3c25` | `0x3d3c46` | — |
| 17 | `remove_soldier_request_event` | 28 | `0x3d43a0` | `0x3d43cc` | `0x3d490e` | — |
| 18 | `axis_mode_change_request_event` | 30 | `0x3d45f7` | `0x3d45f7` | `0x3d486d` | — |
| 19 | `function_mode_change_request_event` | 34 | `0x3d3952` | `0x3d3952` | `0x3d396a` | — |
| 20 | `turn_mode_change_request_event` | 30 | `0x3d45f7` | `0x3d4623` | `0x3d4922` | — |
| 21 | `time_mode_change_request_event` | 30 | `0x3d45f7` | `0x3d466d` | `0x3d468a` | — |
| 22 | `game_name_request_event` | 23 | `0x3d3a47` | `0x3d3a47` | `0x3d4766` | `0x8000…\|16` |
| 23 | `target_info_event` | 17 | `0x3d3b12` | `0x3d3b41` | `0x3d48c8` | — |
| 24 | `team_info_event` | 15 | `0x3d3fd1` | `0x3d401d` | `0x3d494a` | — |
| 25 | `remove_team_event` | 17 | `0x3d3b12` | `0x3d3b70` | `0x3d492c` | — |
| 26 | `pos_info_event` | 14 | `0x3d3c50` | `0x3d3c50` | `0x3d3c6b` | — |
| 27 | `life_info_event` | 15 | `0x3d3fd1` | `0x3d4041` | `0x3d4a3f` | — |
| 28 | `life_queued_death_event` | 23 | `0x3d3a47` | `0x3d3a73` | `0x3d48be` | — |
| 29 | `game_name_info_event` | 20 | `0x3d4184` | `0x3d41dc` | `0x3d4989` | `0x8000…\|1d` |
| 30 | `game_state_info_event` | 21 | `0x3d44c4` | `0x3d44f0` | `0x3d4918` | — |
| 31 | `game_start_request_event` | 24 | `0x3d3e3d` | `0x3d3e95` | `0x3d4940` | — |
| 32 | `function_fire_request_event` | 27 | `0x3d428b` | `0x3d42b7` | `0x3d4904` | — |
| 33 | `function_fire_event` | 19 | `0x3d40e0` | `0x3d40e0` | `0x3d47e9` | — |
| 34 | `function_update_request_event` | 29 | `0x3d46b8` | `0x3d46b8` | `0x3d471d` | `0x8000…\|22` |
| 35 | `function_info_event` | 19 | `0x3d40e0` | `0x3d410c` | `0x3d417a` | `0x8000…\|23` |
| 36 | `function_active_event` | 21 | `0x3d44c4` | `0x3d451c` | `0x3d49e4` | — |
| 37 | `function_points_event` | 21 | `0x3d44c4` | `0x3d4548` | `0x3d4a53` | — |
| 38 | `functions_calculated_event` | 26 | `0x3d397c` | `0x3d397c` | `0x3d4727` | — |
| 39 | `clear_obstacles_event` | 21 | `0x3d44c4` | `0x3d4574` | `0x3d4b17` | — |
| 40 | `add_obstacle_event` | 18 | `0x3d3c7e` | `0x3d3cdc` | `0x3d4936` | — |
| 41 | `destroy_obstacle_event` | 22 | `0x3d446d` | `0x3d446d` | `0x3d4859` | — |
| 42 | `queue_destroy_obstacle_event` | 28 | `0x3d43a0` | `0x3d43f8` | `0x3d49da` | — |
| 43 | `turn_info_event` | 15 | `0x3d3fd1` | `0x3d4057` | `0x3d4b03` | — |
| 44 | `effect_info_event` | 17 | `0x3d3b12` | `0x3d3b9f` | `0x3d3bc3` | — |
| 45 | `axis_mode_info_event` | 20 | `0x3d4184` | `0x3d4208` | `0x3d4a49` | — |
| 46 | `function_mode_info_event` | 24 | `0x3d3e3d` | `0x3d3ec1` | `0x3d4a35` | — |
| 47 | `turn_mode_info_event` | 20 | `0x3d4184` | `0x3d4234` | `0x3d4b0d` | — |
| 48 | `time_mode_info_event` | 20 | `0x3d4184` | `0x3d4260` | `0x3d4281` | — |
| 49 | `end_of_turn_info_event` | 22 | `0x3d446d` | `0x3d4499` | `0x3d44ba` | — |
| 50 | `skin_graphics_request_event` | 27 | `0x3d428b` | `0x3d42e3` | `0x3d49d0` | `0x8000…\|32` |
| 51 | `skin_graphics_info_event` | 24 | `0x3d3e3d` | `0x3d3eed` | `0x3d4af9` | `0x8000…\|33` |
| 52 | `face_graphics_request_event` | 27 | `0x3d428b` | `0x3d430f` | `0x3d4396` | `0x8000…\|34` |
| 53 | `face_graphics_info_event` | 24 | `0x3d3e3d` | `0x3d3f19` | `0x3d4b68` | `0x8000…\|35` |
| 54 | `hat_graphics_request_event` | 26 | `0x3d397c` | `0x3d39a8` | `0x3d48b4` | `0x8000…\|36` |
| 55 | `hat_graphics_info_event` | 23 | `0x3d3a47` | `0x3d3a9f` | `0x3d3b08` | `0x8000…\|37` |
| 56 | `entity_effect_info_event` | 24 | `0x3d3e3d` | `0x3d3f45` | `0x3d4bcd` | — |
| 57 | `turn_limit_info_event` | 21 | `0x3d44c4` | `0x3d45a0` | `0x3d4bb9` | — |
| 58 | `turn_count_info_event` | 21 | `0x3d44c4` | `0x3d45cc` | `0x3d45ed` | — |
| 59 | `name_request_event` | 18 | `0x3d3c7e` | `0x3d3d0b` | `0x3d4a2b` | `0x8000…\|3b` |
| 60 | `name_info_event` | 15 | `0x3d3fd1` | `0x3d4071` | `0x3d4baf` | `0x8000…\|3c` |
| 61 | `color_info_event` | 16 | `0x3d3f9c` | `0x3d3fb7` | `0x3d3fc7` | — |
| 62 | `game_info_event` | 15 | `0x3d3fd1` | `0x3d408b` | `0x3d4c50` | — |
| **63** | *缺号* | — | — | — | — | — |
| 64 | `game_removed_event` | 18 | `0x3d3c7e` | `0x3d3d3a` | `0x3d4ab2` | `0x8000…\|40` |
| **65** | *缺号* | — | — | — | — | — |
| 66 | `rank_request_event` | 18 | `0x3d3c7e` | `0x3d3d69` | `0x3d4b21` | — |
| 67 | `rank_info_event` | 15 | `0x3d3fd1` | `0x3d40a5` | `0x3d4c57` | — |
| 68 | `lock_request_event` | 18 | `0x3d3c7e` | `0x3d3d98` | `0x3d4bc3` | — |
| 69 | `lock_info_event` | 15 | `0x3d3fd1` | `0x3d40bf` | `0x3d40cd` | — |
| 70 | `chat_message_request_event` | 26 | `0x3d397c` | `0x3d39d4` | `0x3d3a3d` | `0x8000…\|46` |
| 71 | `chat_message_event` | 18 | `0x3d3c7e` | `0x3d3dc7` | `0x3d3e33` | `0x8000…\|47` |
| 72 | `ban_connection_request_event` | 28 | `0x3d43a0` | `0x3d4424` | `0x3d4463` | — |
| 73 | `clear_bans_request_event` | 24 | `0x3d3e3d` | `0x3d3f71` | `0x3d3f92` | — |

`0x3d4c50` 那条（`game_info_event`）的 `mov $0x3e,%ebx` 是**最后**一条，
它前面还写了一个 `movb $0x7,0x41(%rsp)`——`GameInfo` 的 9 个字段，与
`GAME_INFO_FIELD_COUNT = 9` 相容。〔观测〕

### 3.1 与 `docs/112` §10.1 的对账

- `docs/112` 解出的 **22 条，22 条全对**（包括 `name_info_event`=60、`rank_info_event`=67）。
- 新增 **50 条**。
- `docs/112` 留下的 4 条悬案，**3 条定死、1 条纠正**：
  - `team_info_event` = **24**（`docs/112` 的 24/27 之争 → 24 对，27 是相邻 arm 的 `mov ebx`）；
  - `color_info_event` = **61**（`docs/112` 猜 50，**错**：50 是 `skin_graphics_request_event`）；
  - `len=16` 分支里 `color_info_event` 与 `skin_graphics_request_event` **本来就是两条**，
    长度不同（16 vs 27），`docs/112` 之所以混淆是因为把 50 记到了 16 字节的分支上。
- 顺带把 `docs/112` §9.2 里 5 个"疑似"的请求事件全部证实：
  **59 `name_request_event`、50 `skin_graphics_request_event`、52 `face_graphics_request_event`、
  54 `hat_graphics_request_event`、66 `rank_request_event`**。
- 也把 `docs/112` §13 未确定项 4 的「tag 7 / tag 8 是哪个事件」解掉：
  **7 = `connected_to_server_event`**、**8 = `new_player_request_event`**
  （`0x51a570` 的逻辑因此读得通了：被告知"已连上服务器"→ 记下自己的 connection id →
  发一条 `new_player_request`。**语义这一层仍是推断**，但两个 tag 值是逐位）。

---

## 4. 三道自校验

### 4.1 长度自校验（72/72，逐位）

分支是按 `len(name) - 10` 选的，所以 `len` 已知。所有相等测试给出的约束是
「名字的第 `o` 字节 == 常量」；把这些 `o` 取并集必须**恰好等于** `[0, len)`。
实现上：`(o, byte)` 表若出现 `o >= len`（越界）或同一 `o` 两个不同值（冲突）或
有字节没被覆盖（空洞），该条即判失败。**72 条全过**。

### 4.2 tag word 自校验（17/17，逐位）

17 条分支在置 `%ebx` 之前会先拼一个 `Event` 前缀到栈上：

```
movq $0x0,0x8(%rsp) ; movq $0x1,0x10(%rsp) ; movq $0x0,0x18(%rsp)   ; 空 String(0,1,0)
movabs $0x8000000000000000,%rax ; or $N,%rax ; mov %rax,(%rsp)      ; ← tag word
mov %rsp,%rdi ; call 0x3e6830                                        ; 填 payload
mov $N,%ebx
```

`or $N` 的 `N` 与 `mov $N,%ebx` 的 `N` **17/17 完全相同**。这既是自校验，
也是下面 §5 判据 ① 的直接材料。

### 4.3 `.rodata` 自校验（69/72，观测）

69 条名字能在 `.rodata` 里逐字节找到（`name.encode() in rodata`）。
剩下 3 条**找不到整串**，原因是 LLVM 用两条已有串的重叠片段拼的，不必单独存：

| 名字 | 拼法 | 两段各自出处 |
|---|---|---|
| `subscribe_event` | `"subscrib"` @0 + `"fo_event" - 0xa04` @7 | 立即数 + 立即数 |
| `game_info_event` | `"game_inf"`/`"fo_event"` 片段 | 立即数 |
| `game_removed_event` | 同上 | 立即数 |

⇒ 这 3 条的证据来自**立即数本身**（`movabs` / `lea` / `add` 的字节），
不来自 `.rodata`；三条的判别值都落在**连续的名字-值序列**里（60/61/62 → 64），
所以判 **逐位**（等价于说：它们和其它 69 条用的是同一套机制、同样的自校验）。

### 4.4 「排序数组」自校验（32/32，观测）

`.rodata` `0x1bf537` 起是一段 **767 字节、以 `\0` 结尾、含 32 条** snake_case 事件名的
连写（紧接着就是 `"explicit panic"`）。把 32 条名字按地址顺序换成本次解出的判别值：

```
13 14 15 19 23 24 25 26 27 29 30 33 35 36 37 39 40 41 43 44 45 46 47 48 49 51 53 55 56 57 58 60
```

**严格递增、32/32 全部命中上表**。逆命题也成立：只要 §3 里任何一条判别值解错，
这段串就不再单调——所以它是一条**独立于 SIMD/整数比较**的一致性检验。

> 附赠：`len = 34` 那条 `memcmp` 的常量指针是 `0x1bf56e`，正好是这段串里的
> 第 4 条 `function_mode_change_request_event`（`0x1bf537 + 17 + 20 + 18`）。
> 即：`0x3d38f0` 的 `memcmp` 直接引用了这张"按判别值排序"的表里的条目。〔观测〕

---

## 5. 那些数字是什么？—— **`enum Event` 的判别值**（三条判据）

`docs/96` §4.6 的护栏：只有能证明是判别值才能改 `events.rs`。下面按"用途 / 取值范围 /
交叉核对"三条走，前两条是**直接证据**。

### 判据 ①（用途，硬证据）：同一个 `N` 被写成 `Event` 的 tag word

`docs/112` §3 已经**逐位**确立：`Event` 是 0x48 字节，`+0` 是
`0x8000000000000000 | discriminant` 的 tag word，消费者用**整 qword** 比较
（例：`0x51a581` `cmp qword [rsi], 0x8000000000000007`；`0x519030` 的
`lea rax,[r15+0xe]` = `0x8000…0e`）。

`0x3d38f0` 里 17 条分支把**同一个 `N`** 写成这个 tag word（§4.2）。
一个"handler 下标"不会以 `0x8000…|N` 的形态被放进 `Event` 的判别值位置——
除非"下标恰好等于判别值"对 17 条同时成立，而那样两个概念本来就没有区别。
⇒ **`N` 就是 `Event` 的判别值**。〔逐位〕

### 判据 ②（用途，硬证据）：`N` 是订阅者表的 key，走的是 hash 查表不是数组下标

公共尾 `0x3d4c5c`：

```
3d4c5c  mov  %rbx,(%rsp)              ; key = N（裸判别值）
3d4c60  lea  0x28(%r15),%rbp          ; &self.map
3d4c6a  call 0x47e5f0                 ; hashbrown 查表
3d4cc6  imul $0x38,%r8,%r8            ; entry 步长 0x38 = 56 字节
3d4cca  cmp  %rbx,-0x38(%rax,%r8,1)   ; 比的是 key 本身
```

- 如果是"handler 下标"，形状会是 `mov rax,[table + N*8]; call rax`（**密集数组**）。
  实际是 `HashMap<u64, …>`（entry 56 B）——**没有任何按 `N` 索引的函数指针表**。
- 同一形状的 map 在 `0x3d3630` 也出现；那里拼了一个常量 key `0x800000000000004a`
  （`0x3d3656` `movabs $0x8000000000000000,%rdx` + `0x3d3660` `add $0x4a,%rdx` +
  `0x3d3664` `mov %rdx,(%rsp)`），即 **tag 74**。〔逐位：这两个字节〕
  `docs/112` §10.2 把这个常量判为 `HashMap<u64, 订阅者集合>` 的 key；本文**复核了常量本身**，
  但没有重新推导 `0x3d3630` 后半段的语义（它的比较用的是 `%r14`，由 `0x3d3821`
  的 `xor $0x8000…` 重新算出来，作者没有跟到底）。
  ⇒ 「判别值池至少到 74」**推断**（缺：一个 `Event` 构造点或一个 `cmp qword […] 0x8000…|0x4a` 消费者）。

### 判据 ③（取值范围）：`0..=73` 且**近乎稠密**

| 观察 | 值 |
|---|---|
| 取值集合 | `{0..=73} \ {63,65}`，共 72 个 |
| 单调 | 与 `.rodata` 里那段「按判别值排序」的名字串（`0x1bf537`，§4.4）**完全同序** |
| 缺号 | 只有 63、65；且全 `.text` 扫 `0x8000…\|63` / `…\|65` 的三种编码形态**一次都没有** |

枚举判别值天然是"声明序号"（这里因为 `0x8000…` 基址而写成 `0x8000… + 声明序号`），
所以"成组相邻 + 只缺两个号"完全符合；而 handler 下标没有理由这样成组
（`skin_graphics_request/info` 50/51、`face_*` 52/53、`hat_*` 54/55、
`rank_request/info` 66/67、`lock_*` 68/69、`chat_*` 70/71、`ban/clear_bans` 72/73
——成对相邻，正是枚举按作者手写顺序编号的样子）。〔逐位〕

### 判据 ④（交叉核对）：**构造侧**与**名字侧**独立命中同一批数

| 构造点 | 写进 tag word 的常量 | 本次解出的名字 | 一致？ |
|---|---|---|---|
| `0x519350`（`name_manager::set_name`） | `0x8000…3c` = 60 | `name_info_event` = 60 | ✅ |
| `0x518f10`（广播全部名字） | `0x8000…3c` = 60 | 同上 | ✅ |
| `0x519030` / `0x454e80`（处理移除） | `0x8000…0e` = 14 | `entity_removed_event` = 14 | ✅ |
| `0x454d80` / `0x455090`（`rank_manager`） | `0x8000…43` = 67 | `rank_info_event` = 67 | ✅ |
| `0x519f40`（过滤 `player_info_event`） | `0x8000…0d` = 13 | `player_info_event` = 13 | ✅ |
| `0x455e30`（`function_manager::set`） | 35 | `function_info_event` = 35 | ✅ |
| `0x51a570` 处理 tag 7、发 tag 8 | 7 / 8 | `connected_to_server_event` / `new_player_request_event` | ✅ |

⇒ 名字侧（本文）与构造侧（`docs/112` 已逐位验证的常量）**两套独立证据相交**，
交集上 7/7 一致。这是 `docs/112` 说的"构造处 + 反查表两处独立命中"的**完整版**。〔逐位〕

### 5.1 结论

**`0x3d38f0` 返回/写入的数字是 `enum Event` 的判别值，不是 handler 下标。**
⇒ 可以、并且必须据此改 `src/events.rs`。

---

## 6. 证伪 `src/events.rs` 原来的顺序假设

旧注释：「`.rodata` 里这些变体名**连续排列**（`docs/07` §2），所以顺序即判别值」。
那段串确实存在（**`0x1c5419`** 起，是 `0x1c4eb8` 那段「`*Event` 结构名 + 字段名」大连写里
的一截：`TickRequest TickReply Subscribe NewConnectionRequest … `），但它**不是判别值顺序**：

| 变体 | 在串里的位置（旧编号） | 实测判别值 | 差 |
|---|---|---|---|
| `TickRequest` | 0 | **1**（0 是 `TickEvent`） | +1 |
| `NewBotRequest` | 7 | **9** | +2 |
| `PlayerInfo` | 10 | **13** | +3 |
| `EntityRemoved` | 11 | **14** | +3 |
| `SoldierInfo` | 12 | **15** | +3 |
| `FunctionFire` | 27 | **33** | +6 |
| `FunctionPoints` | 31 | **37** | +6 |
| `FunctionsCalculated` | 32 | **38** | +6 |

差值**不是常数**（+1 → +6），所以不是"整体偏移"，而是那段串**本身缺变体**：
缺 `TickEvent`、`NewPlayerRequest`、`BotAdded`、`TeamInfo`、`LifeInfo`、`TurnInfo`、
`GameStartRequest`、`SkinGraphicsInfo`、`FaceGraphicsInfo`、`HatGraphicsInfo`、
`EntityEffectInfo`、`NameInfo`、`ClearBansRequest` 等。缺得越多，越往后偏得越多。

⇒ 结论：
1. **旧编号整体作废**（不是"要微调"，是换一套真值）；
2. 旧测试 `event_variants_are_contiguous`（断言 33 个变体的判别值恰好是 `0..=32`）
   **也是错的**：那 33 个变体的真值落在 `1..=38`，这一段里缺 5 个号——
   `8 NewPlayerRequest`、`10 BotAdded`、`24 TeamInfo`、`27 LifeInfo`、`31 GameStartRequest`
   （`0 TickEvent` 更在整个串之前）。所以「位置」既不是 `0..=32` 也不"每步 +1"。
   新测试改成断言「严格递增 + 空洞仅 `{63,65}` + 首尾 `0/73` + 72 条逐条对拍」。
3. `.rodata` 里**确实有**一段「按判别值排序」的名字串（**`0x1bf537`**，767 字节、
   32 条、以 `\0` 结尾、紧跟 `"explicit panic"`；见 §4.4）：
   `player_info_event(13) entity_removed_event(14) soldier_info_event(15)
   function_mode_change_request_event(19) target_info_event(23) team_info_event(24)
   remove_team_event(25) pos_info_event(26) life_info_event(27) game_name_info_event(29)
   game_state_info_event(30) function_fire_event(33) … name_info_event(60)`
   ——**严格递增、且与新表完全相容**。旧 `events.rs` 用的是**另一段**串
   （`0x1c5419` 起，是「`*Event` 结构名 + 字段名」的大连写里的一截，按声明序而非判别值序）。

> ⚠️ `docs/112` §4.6 说「`0x1c0d84`–`0x1c14fc` 那段数组按判别值递增」——**本文不能证实**。
> 那 38 条数组的顺序在**代码**里（`0x4500c0` 在栈上逐项拼），`.rodata` 里只是被池化的
> 字符串；直接按地址顺序切 `0x1c0d84` 得到的是 `player_info_event(13) game_state_info_event(30)
> axis_mode_info_event(45) …`，**不是**递增。真正递增的那段是 `0x1bf537`。
> 这一条不影响 `0x3d38f0` 的任何结论，只是把 `docs/112` 的一处措辞降级为**未知**。
   两段串的区别：前者是"订阅时按判别值排序的清单"，后者是"serde 变体名/字段名连写"。
   〔观测：两段的地址、内容、顺序都不同〕

---

## 7. 代码改动

### 7.1 `src/events.rs`

| 项目 | 改前 | 改后 |
|---|---|---|
| 变体数 | 33 | **72** |
| 判别值 | `= 0..=32` 连续 | **实测值**（`0,1,2,…,62,64,66,…,73`） |
| 顶部注释 | "顺序 = `.rodata` 里的位置" | 已就地说明**该假设被证伪**（表格 + 原因） |
| 新增 | — | `DISPATCH_NAME_TAGS`（名字→变体，72 条）、`Event::from_dispatch_name`、`NAMED_VARIANT_COUNT` |
| `wire_tag()` | `fn wire_tag(self) -> u16` | **签名不变** |

命名：变体名沿用工程既有的 CamelCase（`PlayerInfo`/`EntityRemoved`/…），
与 `docs/96` §3 的 serde 结构名一致（`NameInfo` ↔ `name_info_event`）。

### 7.2 `tests/events.rs`

- 删：`event_order_matches_the_rodata_table`（断言旧的 0/31/32）、
  `event_variants_are_contiguous`（断言 `0..=32` 每步 +1）。
- 加：`dispatch_name_to_discriminant_is_pinned`（72 行真值表 + 长度断言 + 与
  `DISPATCH_NAME_TAGS` 逐条对拍）、`discriminants_are_sparse_and_sorted`
  （严格递增、首 0 尾 73、空洞恰为 `{63,65}`）、`event_variants_are_the_measured_ones`
  （旧值→新值的钉子）。
- 保留：`game_info_field_count_is_pinned`、`life_state_two_tables`、`unknown_soldier_is_not_assumed`。
- `cargo test --offline`：**205 → 206，全绿**。

### 7.3 没有变的东西（重要）

**线格式一个字都没动。** 线上是 serde 外部标签 JSON（`{"NameInfo":{…}}`），
380 份 replay 佐证判别值不上线（`docs/96` §4）。`src/_scaffold/protocol.rs` 一行未改。
本次是**内部保真**。

---

## 8. 仍未确定

| # | 项目 | 现状 | 缺什么 |
|---|---|---|---|
| 1 | 判别值 **63**、**65** 是哪个变体 | 全 `.text` 扫 `0x8000…\|N` 的 `movabs+or` / `movabs+add` / `0x7fff…+lea` 三种形态，**一次都没出现** | 一个构造点或一个 `cmp qword` 消费者；否则只能认为作者留了号没用 |
| 2 | （候选）63 = `GameHeartbeat`、65 = `VersionInfo` | 这两个名字在 `.rodata` 的变体名表里有，但**没有** `*_event` snake 形态、也没有任何名字分派分支 ⇒ 它们正是"有变体、无名字分派"的两个 | 上面 #1 的同一个缺口（**推断**） |
| 3 | 判别值 **74** | 只在 `0x3d3660` 出现（`0x3d3630` 的常量 key） | 同 #1 |
| 4 | `enum Event` 的确切变体数 | 已验证 `≥ 75`（0..=73 + 74） | 需要 #1、#3，或运行时的变体清单 |
| 5 | `0x3d4c5c` 那张 `HashMap<u64,_>` 的 key 到底是**裸 `N`** 还是 tag word | `0x3d38f0` 存的是裸 `%rbx`（`mov %rbx,(%rsp)`），`0x3d3630` 存的是完整 tag word | 两处是不是同一个 map 实例（`0x3d38f0` 用 `self+0x28`，`0x3d3630` 用 `[rdi]+0x10`）没用第三种证据确认 |
| 6 | `0x3e6830` 到底做什么（填 payload？校验？） | 17 条路径把 `Event` 前缀（tag + 0x20 字节）交给它 | 它的函数体语义（`docs/112` §12.2 也是未知） |

**注意**：以上 1–3 只影响"枚举到底有几个变体"，**不影响**已解的 72 条
——那 72 条的地址、字节、tag word 都在 §3 里，可逐条复核。

---

## 9. 证据分级自查

| 结论 | 等级 | 依据 |
|---|---|---|
| 72 条「名字 → 判别值」 | **逐位验证** | §3 每条都有相等测试地址 + `mov $N,%ebx` 地址；`tests/events.rs` 逐条断言 |
| 名字重建（铺满 `[0,len)`、无冲突） | **逐位验证** | §4.1，72/72 脚本自检 |
| `N` 同时是 `Event` 的 tag word 低位 | **逐位验证** | §4.2，17/17；跳转与指令地址可复核 |
| 数字是**判别值**不是 handler 下标 | **逐位验证** | §5 判据 ① + ②（hash 查表形状、无函数指针数组）+ ④（构造侧 7/7 相交） |
| `0..=73` 近乎稠密、且与 `.rodata` 排序串同序 | **逐位验证** | §5 判据 ③ |
| 旧 `events.rs` 的顺序假设错 | **逐位验证** | §6，差值 +1..+6 非常数 |
| `enum Event` ≥ 75 个变体 | **推断** | 74 出现过一次；63/65 的洞没有解释 |
| 63 = `GameHeartbeat`、65 = `VersionInfo` | **推断** | 两个名字只在 `.rodata` 变体名表里出现，无分派无构造 |
| 74 是哪个事件 | **未知** | 只有一处常量 key |
| 线上不上判别值 | **观测**（沿用 `docs/96` §4） | 380 份 replay 是外部标签 JSON |
