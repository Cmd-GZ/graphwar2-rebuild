# 112 · 对局元信息八模块（`docs/02` §14.4 的 25 函数盲区）

> 目标：`docs/02` §14 里归为「**对局元信息** 25 个函数、`NO-CITE`」的那一堆。
> 本文覆盖其中 **23 个**（任务给定的地址表），另外 2 个（`event_handler`/`logger`
> 各多出 1 个）在 `fixtures/module_funcs.tsv` 里没有地址，见 §13。
>
> **本文的证据分级只有四档**：`逐位验证` / `观测` / `推断` / `未知`。定义见 §1.2。
> 凡"看起来合理但没对到字节"的，一律写 `推断` 并注明缺什么。

---

## 0. 一页结论

1. **名字存哪**（Q1）：`HashMap<u32 /*player_id*/, String>`，
   **entry 32 字节**、key 是 u32 @entry+0、`String` @entry+0x8（cap/ptr/len = +0x8/+0x10/+0x18）。
   名字本身是 Rust `String`，**不是** Godot `GString`。〔逐位〕
2. **`NameInfo` 的字段**（Q1）：serde 原文 `struct NameInfoEvent with 2 element`。
   线上事件 `name_info_event` 的判别值 = **60（0x3c）**，payload = `String` @+8 + `u32` @+0x20。
   两处独立命中（构造处 + 反查表），见 §4。〔逐位〕
3. **`RankInfo` 的字段**（Q2）：判别值 = **67（0x43）**，serde 原文也说 2 个元素，
   但 `rank_manager` 造出来的 payload 写了 **3 个 dword**（+8/+0xc/+0x10）。
   **这是一处未收敛的矛盾，按原样记录，不强行圆。** 见 §5.4。〔观测冲突〕
4. **段位怎么算**（Q2）：**`rank_manager` 里算不出来**。4 个函数里没有任何除法、取模、
   阈值比较或 22 项常量表；它只是一个 `HashMap<u32,u32>` 的存取 + 广播。
   22 张 SVG 对应"段位编号 0..21"这一点在本模块内**无法**验证。〔观测（负结果）〕
5. **每个士兵的函数存哪**（Q3）：`HashMap<u32 /*player_id*/, 112 字节>`，
   **entry 120（0x78）字节**，value 开头就是一个 `Vec<Token>`（元素 16 字节）。
   `Token = 16 字节` 这一条与 `AGENTS.md` 的硬事实**对得上**。〔逐位〕
6. **战役进度**（Q4）：`user://campaign.save` 确实存在（0x1bfe28），但它**不**被
   `graphwar_campaign_control.rs` 唯一那个函数（0x418820）引用，而是被 0x418170 / 0x4183b0 引用；
   这 2 个函数没有 panic 串、**无法证明**属于 `campaign_control.rs`。〔观测 + 归属未知〕
7. **副产品**：顺手把 `.rodata` 里的**事件名全表 + 判别值对应关系**解出来了（§3、§4.2）。
   `docs/96` §4 说"判别值不上线、不用管"——**结论仍成立**，但它现在不再是"不可验证"了。

---

## 1. 方法与可复现

### 1.1 工具（全部离线、不依赖 REA）

REA 用于**确认入口地址**（`procedure_info` 语义），其余全部本地：

| 用途 | 做法 |
|---|---|
| 入口确认 | `analysis/funcstarts_ghidra_va.pkl`（虚地址空间）。**23 个地址全部实测是函数入口**，`addr == 前一个入口`（`+0`），无一个落在函数体内 |
| 反汇编 | capstone 5.0.7 直接在 `libgraphwarrust.so` 字节上逐条解码；决定性指令另用 `objdump -d --start-address/--stop-address` 对过 |
| 虚地址 ↔ 文件偏移 | `.text` = vaddr `0x37b940` / off `0x37a940` ⇒ **off = vaddr − 0x1000**（Ghidra 地址 = vaddr + 0x100000，**只加一次**） |
| 重定位解析 | `readelf -rW` → GOT 槽 → 目标（`R_X86_64_RELATIVE`），用来给每条 `call qword [rip+X]` 定名 |
| panic 位置 | `.data.rel.ro` 里的 `core::panic::Location {file:&str, line:u32, col:u32}`，`&str` 是 `(ptr,len)` 16 字节，**必须**先用 `R_X86_64_RELATIVE` 把 ptr 解出来；这是"函数 → 源文件:行号"的唯一可靠来源 |
| 调用者 | `tools/callers.py`（直接调用）+ `tools/ptr_slots.py`（`.data.rel.ro` 函数指针槽） |
| 事件判别值 | 扫 `.text` 里 `... NN 00 00 00 00 00 00 80` 的 imm64 字节模式；另一种编码 `movabs rbp, 0x7fffffffffffffff` + `lea rax,[rbp+N]`（= `0x80..|(N-1)`） |

复现脚本在 `.agent-work/meta/`（`fn.py` / `loc.py` / `evfinal.py` / `tagscan.py` 等），
不在仓库里（跟 `analysis/` 同性质，属于再生品）。

### 1.2 证据四级（本文唯一使用的分档）

| 档 | 判据 |
|---|---|
| **逐位验证** | 常量/偏移能从字节直接抄下来，**且有第二条独立证据**（例如构造处 + 反查表都指向同一个数），或可复算自洽（`0x7fff…ff + 0xe = 0x8000…0d`） |
| **观测** | objdump/capstone 直接读到的一处事实；没有第二条证据，但也不需要推测 |
| **推断** | 有偏移/常量/调用关系，但语义闭环缺东西；**必须写明缺什么** |
| **未知** | 没拿到，或拿到但与另一处观测冲突且未收敛 |

---

## 2. 地址总表

`vaddr` 为 ELF 虚地址；`Ghidra = vaddr + 0x100000`。`size` 由 Ghidra 入口表给出。
`line` 来自该函数内引用的 `panic::Location`（`src/<file>.rs:LINE`），**是观测**，
不是"函数在源文件第几行开始"，但足以给出源文件内的相对次序。

### 2.1 `graphwar_function_manager.rs`（7，`NO-CITE`）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x455b60` | `0x2d0` | 165 | **取**某 player 的 token 向量（克隆 `Vec<Token>` 返回） | 观测→逐位（`shl r15,4` + `memcpy` 16N） |
| `0x455e30` | `0x7c0` | 152/153/158/161 | **设**：两个 `&str` 各喂一次解析器 `0x4ed670`，再发事件 tag 35 | 观测 |
| `0x4565f0` | `0x220` | 169 | 同类存取（与 0x455b60 的 self 骨架完全同形） | 推断 |
| `0x456810` | `0x370` | 192/199 | 按 player_id 在 `X+0x38` 的 map 上查一条并写字段 | 观测 |
| `0x456b80` | `0x270` | 181/186 | 同上，另一个字段 | 推断 |
| `0x456df0` | `0x190` | 208 | 同形，偏移 0x20/0x28/0x30/0x38/0x40/0x48 | 推断 |
| `0x456f80` | `0x190` | 213 | 与 `0x456df0` **偏移直方图完全一致**（疑似同构的另一字段/另一模式） | 推断 |

### 2.2 `graphwar_name_manager.rs`（3）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x518f10` | `0x120` | 69 | 遍历 `HashMap<u32,String>`，逐条发 `name_info_event`(60) | 逐位 |
| `0x519030` | `0x90` | 79 | **处理 `entity_removed_event`(14)**：从 map 里删掉该 player | 逐位 |
| `0x519350` | `0x100` | 57 | **设名**：`map.insert(player_id, String)` + 发 `name_info_event`(60) | 逐位 |

### 2.3 `graphwar_rank_manager.rs`（4）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x454d80` | `0x100` | 73 | 遍历 `HashMap<u32,u32>`，逐条发 `rank_info_event`(67) | 逐位 |
| `0x454e80` | `0x70` | 83 | 处理 `entity_removed_event`(14)：从 map 删 | 观测 |
| `0x454f30` | `0x160` | 33 | **初始化/订阅**：向事件处理器订阅 `new_connection_event`(20 B) 与 `entity_removed_event`(20 B) | 逐位 |
| `0x455090` | `0xd0` | 61 | **设/清**：`edx==1` → 写 `self+0x18` 的 map，否则删 `self+0x38` 的 map，然后发 (67) | 逐位 |

### 2.4 `graphwar_local_player_manager.rs`（3）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x519f40` | `0x630` | 107–128 | 处理 `player_info_event`(13)：按连接 id 过滤，push 进 `Vec<u32>`，首次入列时发 5 个"请求"事件 | 观测（tag 值逐位） |
| `0x51a570` | `0xa0` | 76/81 | 处理 tag **7**：`self+0x18:=1`、`self+0x1c:=payload.u32`、`self+0x10:=0`，并发 tag **8** | 逐位 |
| `0x51a610` | `0xe0` | 146 | 处理 `entity_removed_event`(14)；函数体只有一条 `panic_fmt` 出口 | 观测 |

### 2.5 `graphwar_event_handler.rs`（2）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x3d38f0` | `0x16b0` | 396/407/454 | **事件名 → 判别值**分派（~50 个名字，逐字节精确比较），每个名字把 `ebx` 置成判别值 | 逐位（比较常量可复算） |
| `0x3d3630` | `0x2c0` | 467 | 用**常量** key `0x800000000000004a` 去查一条 `HashMap<u64, 订阅者集合>`（entry 0x38 B） | 观测 |

### 2.6 `graphwar_logger.rs`（2）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x3b6c70` | `0xd0` | 12 | 线程局部（`thread_local!`，Location 落在 `std/thread/local.rs:428`）槽的**换值**：拿旧句柄、`dec` 强引用、`dec==0` 就 drop、再存新句柄 | 观测 |
| `0x44dd60` | `0x630` | 44 | 真正写日志的一条（大量 `0x528070` = dealloc 与 `0x45f160`/`0x461100` 格式化/落盘） | 推断 |

### 2.7 `graphwar_campaign_control.rs`（1）/ `graphwar_lobby_control.rs`（1）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x418820` | `0x2e0` | 94 | Godot 节点取值：`clone` / `init_ref()` / `upcast_ref`，随即调用 `graphwar_nodes.rs` 的节点访问器 | 观测 |
| `0x3ef990` | `0x6e0` | 28/29/30/42/47/52 | UI 装配：取 `ShowLocalButton` / `ShowGlobalButton` / `GraphwarLanVBox` 等节点并设文本/图标 | 观测 |

### 2.8 三个"影子"函数（不在 8 个模块里，但离开它们读不懂本批代码）

| vaddr | 名字（本文暂定） | 为什么必须知道它 | 证据 |
|---|---|---|---|
| `0x3d3530` | **事件入队 sink** | 本批 8 个模块共 12 处 `call [0x9bac80]` 全部指向它。`tools/ptr_slots.py` 实测 **113 个使用者**，覆盖 team/life/obstacle/turn/color/mode/… 几乎所有 manager ⇒ 它是全局唯一的"发事件"出口 | 逐位（无条件拷贝 0x48 字节） |
| `0x3e6830` | **`Event` 析构（drop glue）**，真长度 `0xd5` | `0x3d38f0` 的每个分支在置 `ebx` 前都 `call 0x3e6830(rdi=rsp)` | **观测**（`docs/151` §2.1） |
| `0x418170` / `0x4183b0` | 存档读写 | `user://campaign.save` 的**真正**使用者 | 观测 |

---

## 3. 产物 A：`Event` 的内存布局（逐位）

这一段是后面所有结论的地基，且有 4 处独立函数一致，故判 **逐位验证**。

```
Event（总长 0x48 = 72 字节，sink 0x3d3530 无条件整块拷贝 4×16B + 1×8B）
  +0x00  8B   tag word = 0x8000000000000000 | discriminant
  +0x08  …    variant payload（每个变体不同；下同）
```

**判别值写在 tag word 的低字节，高位一律置 `0x80`；整字比较，不是按字节。**
证据链：

1. `0x51a577`：`movabs rax, 0x8000000000000007` / `0x51a581`: `cmp qword ptr [rsi], rax`
   ⇒ 分发是**整 qword 比较**。
2. `0x519f4e`：`movabs rbp, 0x7fffffffffffffff`；`0x519f58`: `lea rax, [rbp + 0xe]`
   ⇒ `rax = 0x800000000000000d`，即**判别值 13**。反过来印证了 `0x7fff…ff + N` 这种
   写法等价于 `0x8000…00 | (N-1)`。同一函数里还有 `lea rax,[rbp+0x3c]`(=59)、
   `[rbp+0x34]`(=50)、`[rbp+0x36]`(=52)、`[rbp+0x38]`(=54)、`[rbp+0x43]`(=66)。
3. `0x519350`：`movabs rax, 0x8000000000000000` / `add rax, 0x3c` / `mov [rsp+0x20], rax`
   ⇒ 判别值 60。
4. sink `0x3d3530`：`lea rcx, [r15 + r15*8]`（= 9·i）+ `[rax + rcx*8]` ⇒ 元素步长
   **72 = 0x48**；随后 4 条 `movups`（+0x30/+0x20/+0x10/+0x00）+ 1 条 `mov [.. +0x40]`
   ⇒ 整块 0x48 字节。

### 3.1 `String` / `Vec` 的字段顺序（**容易搞反，实测是 cap 在前**）

```
String / Vec<T> = { cap: usize @+0x00, ptr: usize @+0x08, len: usize @+0x10 }
空串 = (cap=0, ptr=1, len=0)        ← ptr 是 dangling = 1
```

证据：`0x3d4b72`（`0x3d38f0` 里判别值 0x3c 的一个分支）写 `[rsp+8]=0`、`[rsp+0x10]=1`、
`[rsp+0x18]=0`、`[rsp+0x20]=0`，恰好是"空 String + 一个 0u32"。
如果按 `(ptr,cap,len)` 读，这组数会变成 `ptr=0,cap=1,len=0`，**不是**合法空串。

同样的顺序在 `0x455b60` 的 `Vec<Token>` 克隆尾部（`[r14]=cap`、`[r14+8]=ptr`、
`[r14+0x10]=len`）再次出现。⇒ `(cap, ptr, len)`。

> ⚠️ 这条对所有后续模块都有用：凡看到 `x+0 == 1 && x+8 == 0 && x+0x10 == 0` 的，
> 是**空 String**，不是"指针 1"。

---

## 4. `name_manager`（Q1：`NameInfo` 的字段 / 名字存哪 / 与 id 的对应）

### 4.1 `0x518f10`（line 69，0x120 B）— 广播全部名字

```
r12 = [rdi+0x30]                      ; 条目数，循环计数
r13 = [rdi+0x08]                      ; 共享状态对象 X
rbx = [rdi+0x18]                      ; 控制字节数组（hashbrown ctrl）
      test r12,r12; je done
      movdqa xmm0,[rbx]; pmovmskb r15d,xmm0; not r15d   ; EMPTY = 0x80
      每 16 个 ctrl 前进 0x10，同时 bucket 指针 add rbx,-0x200   ⇒ 桶步长 0x200/16 = 0x20
tzcnt eax, r15d ; shl eax,5           ; ← 步长 32，逐位
[r13+0x10] = -1                       ; hashbrown 迭代标记
ebp = dword ptr [rsi - 0x20]          ; ← key 是 u32
call [0x9ba8f0]                       ; 24B 值（String）拷到栈
[rsp+0x20] = 0x800000000000003c       ; 判别值 60
...
lea rdi,[r13+0x18] ; lea rsi,[rsp+0x20] ; call [0x9bac80]   ; = sink 0x3d3530
```

**结论**

| 问题 | 答案 | 等级 |
|---|---|---|
| 名字存在什么容器 | `HashMap<u32, String>`，**entry = 32 字节**（`shl eax,5`），key u32@entry+0，`String`@entry+8 | 逐位 |
| 名字类型 | Rust `String`（24 B，cap/ptr/len）——**不是** `GString` | 逐位 |
| 发什么事件 | 判别值 **60 = `name_info_event`**，payload = 该 `String` + 一个 u32 | 逐位 |
| 每条的 u32 是什么 | `ebp` 来自 map 的 **key**，即 `player_id` | 观测（key 语义见 §4.2） |

### 4.2 `0x519350`（line 57，0x100 B）— 设名

参数：`(rdi=self, esi=u32, rdx=&String/*24B*/)`。

```
lea rdi,[rsp+0x20] ; mov rsi,rdx ; call [0x9ba8f0]   ; 复制入参 String
lea rsi,[r14+0x18] ; mov edx,ebp ; lea rcx,[rsp+0x20]
call 0x43e180                                        ; map(self+0x18).insert(player_id, &String)
                                                     ;   返回旧值 24B @rsp+8 → 立刻 free
r15 = [r14+8] ; cmp qword [r15+0x10],0 ; jne panic
[r15+0x10] = -1
[rsp+0x20] = 0x800000000000003c          ; 判别值 60
[rsp+0x28..0x38] = 复制来的 String 前 16B（cap,ptr）
[rsp+0x38] = 入参 String 的 len
[rsp+0x40] = ebp                          ; player_id
lea rdi,[r15+0x18] ; lea rsi,[rsp+0x20] ; call [0x9bac80]
inc qword [r15+0x10]
```

⇒ **`set_name(player_id: u32, name: &String)`：写 map + 发 `name_info_event`。**
`map` 位于 `self+0x18`。〔逐位〕

### 4.3 `0x519030`（line 79，0x90 B）— 处理"实体移除"

```
movabs r15, 0x8000000000000000 ; lea rax,[r15+0xe]      ; = 0x800000000000000e
cmp qword ptr [rsi], rax ; jne 0x5190a4                  ; ← 判别值 14
mov eax, dword ptr [rsi + 8]                             ; ← payload 第一个 u32
...
lea rbx,[rdi+0x18]  ; map(self+0x18) 的地址
add rdi, 0x38                                            ; ← 另一处 +0x38
call 0x47dec0                                            ; hashbrown find
call 0x486990                                            ; hashbrown remove
（旧 String 的 cap/ptr 若有效则 call [0x9ba5a0] = dealloc(ptr,?,1)）
0x5190a4: lea rdi,"explicit panic"(0x1c645f) ; call 0x925660
```

⇒ **`on_entity_removed(event)`：`event.payload.u32` 就是从 map 里删掉的 `player_id`。**
〔逐位：判别值 14、`[rsi+8]` 取 key、find+remove 的调用序列〕

> ⚠️ 一处**没有收敛**的细节：本函数里 find 用 `self+0x38`，而 remove 用 `self+0x18`
> （`rsi = rbx = self+0x18`）。两者**不是同一个基址**。可能是：(a) find 作用于
> map 内部的 `RawTable` 而 remove 作用于整个 map；(b) 本函数的 `self` 类型与
> `0x519350` 的 `self` 类型不同。**本文不判**，列入 §13。

### 4.4 回答 Q1

| 问题 | 答案 | 等级 |
|---|---|---|
| `NameInfo` 有几个字段 | **2**（serde 原文 `struct NameInfoEvent with 2 element` @`0x1bbd83`） | 逐位 |
| 字段形态 | field0 = `String`（@payload+0x00），field1 = `u32`（@payload+0x18，即事件 +0x20） | 逐位 |
| field1 是 `player_id` 还是 `connection_id` | **`player_id`** 更可能：同一个 u32 既当 map 的 key，又出现在 `entity_removed_event` 的第一个 u32 里被用来删同一个 map。**但**二进制里没有任何 `player_id`/`connection_id` 字段名与 `NameInfoEvent` 相邻（§4.5），所以严格说这是 `推断` | 推断（缺：`NameInfoEvent` 的 serde 字段名） |
| 名字怎么和 connection 对应 | **没有直接对应**。`name_manager` 全程只有一个 `HashMap<u32,String>`，没有任何 `connection_id` 参与 | 观测（负结果） |

### 4.5 字段名为什么拿不到（把这个坑钉死）

`.rodata` 里有完整的 serde 字段词表（`0x1c4e78` 起，见 §4.6），但**名字与结构体的归属靠相邻性推不出来**——
`docs/96` §5 已经踩过一次。本次复核到一条更硬的边界：

* `struct NameInfoEvent with 2 element` 附近**没有**任何字段名；
* 字段名 `corrected_function` 紧跟在 `FunctionInfoEvent` 后面（这一处是**真的**配对，
  因为 `FunctionInfoEvent` 的 payload 确实有函数串，见 §6）；
* `player_connection_id` / `leader_connection_id` 只出现在 `BanConnectionRequestEvent` 之后。

⇒ 因此本文对 `NameInfoEvent.field1` 的命名**只到 `推断`**。

### 4.6 顺带拿到的完整事件名清单（`docs/96` 的一手补料）

`.rodata` `0x1c0d84`–`0x1c14fc` 是一段**连续的** `*_info_event` 名字数组，
被 `0x4500c0`（`graphwar_server_connection_handler.rs`）在栈上逐项建出来，
再对每一项用 `cmp r15, 0x26` 循环 **38** 次。数组按**判别值递增**排列（下面 §5.4 的交叉验证）：

```
player_info_event          entity_removed_event      soldier_info_event
target_info_event          team_info_event           remove_team_event
pos_info_event             life_info_event           life_queued_death_event
game_state_info_event      function_fire_event       function_info_event
function_active_event      function_points_event     clear_obstacles_event
add_obstacle_event         destroy_obstacle_event    queue_destroy_obstacle_event
turn_info_event            effect_info_event         axis_mode_info_event
function_mode_info_event   turn_mode_info_event      time_mode_info_event
end_of_turn_info_event     game_name_info_event      skin_graphics_info_event
face_graphics_info_event   hat_graphics_info_event   entity_effect_info_event
turn_limit_info_event      turn_count_info_event     name_info_event
color_info_event           rank_info_event           lock_info_event
chat_message_event
```

每一项的 `(ptr,len)` 都是**逐条从代码里抄的**（不是从 blob 切出来的），
所以 `name_info_event` = 15 B @`0x1c14ce`、`rank_info_event` = 15 B @`0x1c14dd`、
`lock_info_event` = 15 B @`0x1c0e26` 这些长度都是确定的。

> `docs/96` §3 把这一族记成 `NameInfo` / `RankInfo`（CamelCase）。
> **线上/spine 里的键是 snake_case 加 `_event` 后缀**（`name_info_event`），
> CamelCase 只是 Rust 变体名。补协议时别写错。

---

## 5. `rank_manager`（Q2：段位怎么算 / 22 张 SVG / `RankInfo` 的 2 个字段）

### 5.1 `0x454f30`（line 33，0x160 B）— 订阅

```
call 0x522890                                   ; 取共享状态（返回值 rbx 有 +0x10 计数、+0x18 内层指针）
[rbx+0x10] = -1 ; inc [rbx+0x10]                ; hashbrown 式标记
r15 = [rbx+0x18] ; inc qword [r15+8]            ; 强引用 +1
Box::new(8) = { r15 }                           ; ← 8 字节闭包捕获
lea rsi, "new_connection_event"  @0x1c0eef      ; edx = 0x14 = 20  ← 逐位（长度与串一致）
lea r8, 0x9567d8                                ; 静态描述符 {0, 8}
call [0x9bac88]                                 ; = 0x3d38f0（事件名分派）
……同样一次，换成 "entity_removed_event" @0x1c0e12，edx = 0x14 = 20
```

⇒ **`RankManager::new`：向事件处理器订阅 `new_connection_event` + `entity_removed_event` 两条。**
两条串的**确切长度 0x14=20** 是从 `mov edx, 0x14` 抄的，并与字符串长度互相印证。〔逐位〕

### 5.2 `0x454d80`（line 73，0x100 B）— 广播

```
shl eax, 3                       ; ← map 桶步长 8 = HashMap<u32,u32>
movq xmm0, qword ptr [rcx - 8]   ; 取整个 entry（key u32 + value u32）
pshufd xmm0, xmm0, 0xe1          ; 交换两个 dword → (value, key)
mov dword ptr [rsp + 8], 1
movq qword ptr [rsp + 0xc], xmm0 ; value@+0xc, key@+0x10
movabs rax, 0x8000000000000043   ; ← 判别值 67
mov qword ptr [rsp], rax
lea rdi,[r12+0x18] ; call [0x9bac80]
```

⇒ **`broadcast_all_ranks()`：`HashMap<u32,u32>` 的每一项发一条 `rank_info_event`(67)**，
payload = (`dword` 常量 1 @+8, `value` @+0xc, `key` @+0x10)。〔逐位：`pshufd 0xe1` + `shl 3`〕

### 5.3 `0x455090`（line 61，0xd0 B）— 设 / 清

参数 `(rdi=self, esi=u32 A, edx=u32 B, ecx=u32 C)`：

```
cmp edx, 1
  ── jne  → 删：find(self+0x38, &A) ; remove(self+0x18? ...
  ── je 0x4550b5:  call 0x442330(self+0x18, esi=A, edx=C)     ; 写另一张 map
之后统一：
  r15 = [self+8]
  [rsp+0x18] = edx ; [rsp+0x1c] = ecx ; [rsp+0x20] = esi
  [rsp+0x10] = 0x8000000000000043       ; 判别值 67
  call [0x9bac80]
```

⇒ **`set_or_clear(rank, player_id, flag)`**：`flag==1` 写入 `self+0x18` 的 map，
否则从 `self+0x38` 的 map 删，然后无论哪条都发一条 `rank_info_event`。〔逐位〕
两张 map 用了**不同的插入/删除助手**（`0x442330` vs `0x47dec0`+`0x486bc0`），
⇒ 它们是**两个不同类型的容器**（推断：一张 `HashMap<u32,u32>`，另一张键值语义不同；缺：容器类型）。

### 5.4 ⚠️ `RankInfo` 的字段数：2 还是 3 —— **未收敛**

| 证据 | 内容 | 等级 |
|---|---|---|
| serde 原文 `0x1bbda7` | `struct RankInfoEvent with 2 element` | 逐位 |
| `0x454d80` | 写 `+8` / `+0xc` / `+0x10` **三个 dword** | 逐位 |
| `0x455090` | 同上，也是三个 dword（`+8=edx, +0xc=ecx, +0x10=esi`） | 逐位 |
| `0x3d38f0` | 对 15 字节 `rank_inf…` 的分支置 `ebx = 0x43`（= 67） | 逐位 |
| 全 `.text` 扫 imm64 | `0x8000000000000043` 只有 **2 处**，都在 `graphwar_rank_manager.rs` | 观测 |

能同时满足前四行的唯一自洽读法是 **field0 = `u64`（占 +8/+0xc）、field1 = `u32`（+0x10）**；
但那样 `+0xc` 就只是 u64 的高半，与 `0x454d80` 里"value@+0xc、key@+0x10"的语义
（value 与 key 是 map 的独立两列）对不上。

⇒ **本文不给唯一答案**。取值 67 与"2 个元素"这两条是硬的；
"三个 dword 分别是什么"列为 §13 未确定项。**不要**在实现里把第三个 dword 当成已知字段。

### 5.5 回答 Q2 的"段位怎么算"

**负结果，且是可复现的负结果**：4 个函数（`0x454d80` / `0x454e80` / `0x454f30` / `0x455090`）里
**没有**任何 `div` / `idiv` / `imul`（除了 map 索引的移位）、没有阈值 `cmp` 立即数、
没有 22 项的常量表。它们只做三件事：读写两张 map、订阅两个事件、把 map 内容广播成
`rank_info_event`。

⇒ **"分数 → 段位"的换算不在 `rank_manager` 里。** 22 张 `res://ranks/rank_0..21.svg`
只能证明"存在编号 0..21 的图形资源"，**不能**证明段位是 22 档，也不能证明
`rank` 值域就是 0..21 —— 这两点本文都标 **未知**。

---

## 6. `function_manager`（Q3：每个士兵的函数存哪 / 原串与修正串）

### 6.1 `0x455b60`（line 165，0x2d0 B）— 取 token 向量

参数 `(rdi=返回缓冲 24B, rsi=self, edx=player_id)`。

```
r12 = [rsi+0x18]                                  ; 共享状态 X
lock cmpxchg dword ptr [r12+0x10], 1               ; 一次性初始化闸门
movzx eax, byte ptr [r12+0x14] ; test al,al ; jne panic
cmp qword ptr [r12+0x30], 0 ; je <空表分支>
lea rdi,[r12+0x38] ; mov rsi,rsp ; call 0x47dec0   ; hashbrown find，key = &player_id(u32)
……
cmp r15d, dword ptr [rcx + r8 - 0x78]              ; ← key u32，entry 起点 = rcx-0x78
……（命中）
mov rsi, qword ptr [rcx - 0x68]                    ; entry+0x10 = ptr
mov rbx, qword ptr [rcx - 0x60]                    ; entry+0x18 = len
mov r15, rbx ; shl r15, 4                          ; ← 元素 16 字节
……call [0x9ba630](rdi=16N, rsi=8)                  ; alloc(16N, 8)
……call [0x9ba638]                                  ; memcpy(dst, src, 16N)
mov qword ptr [r14], rcx                           ; out.cap
mov qword ptr [r14+8], rax                         ; out.ptr
mov qword ptr [r14+0x10], rbx                      ; out.len
```

| 事实 | 值 | 等级 |
|---|---|---|
| map 位置 | `[self+0x18]` → X；find 的 `self` 是 `X+0x38` | 逐位 |
| entry 步长 | **0x78 = 120 字节**（`imul r8, r8, 0x78`） | 逐位 |
| key | `u32` @entry+0 | 逐位 |
| value 大小 | 120 − 8 = **112 字节** | 逐位 |
| value 开头 | `Vec<Token>`，ptr@entry+0x10 / len@entry+0x18 ⇒ 相对 value 起点 +0x08/+0x10 | 逐位 |
| `Token` 大小 | **16 字节**，对齐 8 | 逐位（`shl 4` + `alloc(16N,8)` + `memcpy(16N)`） |
| 本函数返回 | 只有那 24 字节 `Vec<Token>` 的**克隆** | 逐位 |

`Token = 16 字节`与 `AGENTS.md` 的"16 bytes `{tag: u64, value: f64}`"**完全一致** —— 这是本批
唯一能与既有硬事实对撞的机会，**对上了**。

### 6.2 `0x455e30`（line 152/153/158/161，0x7c0 B）— 设函数

参数 6 个：`(rdi=self, esi=u32, rdx=?, rcx=?, r8=?, r9=?)`；`r8`/`r9` 被拼成
`(cap=len, ptr, len)` 的临时 `String`（`0x455f32`：`[rsp+0x50]=r15; [rsp+0x58]=r12; [rsp+0x60]=r15`）。

```
add rdi, 0x10 ; call 0x46d040                      ; graphwar_module_container.rs
mov ecx, dword ptr [rax + 0x58]                    ; ← 从容器读一个 u32
…… 用空串（0,1,0）调用解析器：
lea rdi,[rsp+0x148] ; lea rsi,[rsp+0x50] ; call [0x9bacf8]   ; 0x9bacf8 → 0x4ed670 解析器
…… 用第二个串再调一次同一槽位（0x455f4e）
…… 发事件：0x456381 `lea rcx,[rbp+0x24]`（rbp=0x7fff…ff）⇒ 判别值 35
```

| 事实 | 值 | 等级 |
|---|---|---|
| 调用解析器 | **2 次**，槽位 `0x9bacf8` 静态解析到 `0x4ed670`（`AGENTS.md` 的 parser 入口） | 逐位 |
| 发的事件 | 判别值 **35**（`0x23`） | 观测 |

**和 `docs/05` §?? 的说法对不对得上？**
`docs/05` 只说"原串、修正串由 `graphwar_function_manager.rs` 分别管理"。
本次**能确认**的是：本函数确实**同时**接收两个字符串并**各解析一次**（两串都过 parser），
这与"原串/修正串两份都在这里"相容。

**但本文没能证明**第二个串到底是"修正串"还是别的（例如"AI 进化的原串 + 用户手输串"）。
理由：`0x455e30` 是 0x7c0 字节，里面**没有**找到写 0x78 条目的最终 store
（`[self+0x18]→X` 的所有访问集中在 `X+0x10/+0x14/+0x18/+0x1c/+0x20/+0x28/+0x30/+0x38/+0x40/+0x48/+0x50/+0x58`，
是栈和共享状态，不是 map entry）。⇒ 见 §13 未确定项 2。

### 6.3 其余 5 个函数

`0x4565f0`(169) / `0x456810`(192/199) / `0x456b80`(181/186) / `0x456df0`(208) / `0x456f80`(213)
的**访问偏移直方图**（对 `[reg + 0xNN]` 计数）显示前 4 个是同一套"取/放某字段"骨架：

| 函数 | 独有偏移 | 判断 |
|---|---|---|
| `0x4565f0` | 0x10×3 0x14×3 0x18×2 0x20×1 0x30×1 0x38×1 | 与 `0x455b60` 的 self 骨架相同 ⇒ 同类取/放（推断） |
| `0x456810` | 0x10×16 0x14×4 0x18×5 0x20 0x26 0x28 0x30 0x38 | 查 map 后写字段（观测：`[rdi+0x18]→X`、`X+0x38` 做 find） |
| `0x456b80` | 0x10×11 0x14×3 0x18×5 0x1c 0x20 0x25 | 同上（推断） |
| `0x456df0` | 0x20 0x28 0x30 0x38 0x40 0x48 | 偏移成对递增，疑似逐字段拷贝（推断） |
| `0x456f80` | 与 `0x456df0` **完全相同** | 两者是同构的两条路径（推断：例如"按 player" / "按 soldier"） |

`0x456810`/`0x456b80` 里出现的 `0x26`/`0x25` 是 `[r13+0x26]`/`[r13+0x25]`——
**相对 `X`（=`[self+0x18]`），不是相对结构体本身**。按另一个 agent 踩过的坑换算：
若结构体从 ArcInner+0x10 开始，则结构体内偏移是 `+0x16`/`+0x15`。本文引用的一律标 `X+`。

---

## 7. `campaign_control` + `user://campaign.save`（Q4）

### 7.1 `0x418820`（line 94，0x2e0 B）

只有一个 `panic::Location`（`src/graphwar_campaign_control.rs:94`），直接调用：
`0x461100`、`graphwar_nodes.rs` 的 `0x3ab390`、`0x4068f0`、`0x407630`；
字符串只引到 `"clone"` / `"init_ref() failed"` / `"upcast_ref "` / `"null instance ID …"`。

⇒ 它是**一个 Godot 节点取值 + 委托**的函数（`clone`/`init_ref`/`upcast_ref` 是 gdext
取子节点的固定套路），**不碰存档**。〔观测〕

### 7.2 `user://campaign.save` 到底谁在用

`0x1bfe28` = `"user://campaign.save"`（24 B）。xref 全量：

| 引用点 | 所在函数 | 该函数的源文件归属 |
|---|---|---|
| `0x41817a` | `0x418170`（0x240 B） | **证不出**：只有 `alloc/string.rs:2916` 的 panic 位置 |
| `0x4183d0` | `0x4183b0`（0x470 B） | 同上 |
| `0x4185f9` | `0x4183b0` | 同上 |

同一段 `.rodata` 里紧邻的还有 `user://player.config`（`0x1bfe12`，16 B），
被 `0x4166b0` 和 `0x417560` 引用。

⇒ **`campaign_control.rs` 与存档代码的关系只有"地址相邻"这一条弱证据**（`0x418170` /
`0x4183b0` / `0x418820` 都在 `0x418xxx`），本文判 **推断**，不判观测。〔推断（缺：这 2 个函数
的模块归属证据；`module_funcs.tsv` 只收"有 panic 串被引用"的函数，它们恰好没有）〕

### 7.3 21 关的关卡数据在代码里还是资源里

- 全文里 `campaign` 只出现 3 次：`src/graphwar_campaign_control.rs`（模块名串）、
  `0x3af988` / `0x3b0ee4` 两个二进制块里的 `campaign` 片段（GDScript/GDC 侧）。
- 二进制里**没有** `21` / `0x15` 这种"关卡数量"常量与 campaign 沾边；
  也**没有**任何 `campaign.level` 之类的字符串。
- `0x418820` 不引用任何数字常量表。

⇒ 支持 `docs/02` §14.5 的推断：**进度在存档文件里，关卡数据不在 `campaign_control.rs`。**
但"21 关的数据在哪"本文仍判 **未知**（未查 `.gdc`；客户端 agent 的域）。〔推断〕

---

## 8. `lobby_control`（1 个函数）

`0x3ef990`（line 28/29/30/42/47/52，`0x6e0` B）：

- 6 个 `panic::Location` 全在 `graphwar_lobby_control.rs`，另有 2 个在
  `gdext/godot-cell/src/guards.rs:156/184`（说明用了 `Gd<T>` 的 guard/`upcast`）。
- 直接调用里 `graphwar_nodes.rs` 出现 3 次（`0x3aaea0`×2、`0x3ab690`），
  另有 `0x4068f0`×3、`0x4098f0`×2、`0x523c20`、`0x527fa0`×4。
- 字符引用：`"ShowLoca…"`（`0x1bfca1`）、`"ShowGlob…"`（`0x19e000`）、
  `"Graphwar…"`（`0x1bfcb0`）——与 `.rodata` 大 blob 里的
  `ShowLocalButton` / `GraphwarLanVBox` 对上。

⇒ **职责：大厅界面装配（取按钮/列表节点、连信号、设文本）。** 与 `docs/02` §6
"开局/大厅"的推断一致。〔观测（调用与串），"设文本"细节为推断〕

---

## 9. `local_player_manager`（3 个函数）

### 9.1 `0x51a570`（line 76/81，0xa0 B）— 处理 tag 7

```
movabs rax, 0x8000000000000007
cmp qword ptr [rsi], rax ; jne panic("explicit panic")
mov ecx, dword ptr [rsi + 8]        ; payload 第一个 u32
mov dword ptr [rdi + 0x18], 1       ; self+0x18 := 1
mov dword ptr [rdi + 0x1c], ecx     ; self+0x1c := payload
r14 = [rdi+0x28]
…… [rsp+8] = ecx ; inc rax（→ tag 8）; [rsp] = rax
call [0x9bac80]                     ; 发 tag 8，payload 同一个 u32
mov qword ptr [rbx + 0x10], 0
```

⇒ **收到 tag 7 就把"本机 id"记到 `self+0x18/+0x1c`，并立刻广播 tag 8（同一个 id）。**
〔逐位；tag 7 / 8 的**语义**未知，见 §13〕

### 9.2 `0x519f40`（line 107–128，0x630 B）— 处理 `player_info_event`(13)

```
movabs rbp, 0x7fffffffffffffff ; lea rax,[rbp+0xe]      ; ⇒ tag 0x800000000000000d = 13
cmp qword ptr [rsi], rax ; jne panic
cmp dword ptr [rdi+0x18], 1  ; jne 返回
mov eax, dword ptr [rsi + 0xc] ; cmp dword ptr [r14+0x1c], eax ; jne 返回
r15d = dword ptr [rsi + 8]
rbx = [r14+0x10] ; cmp rbx, [r14] ; jne …
call [0x9baf48]                          ; Vec 扩容
mov dword ptr [rax + rbx*4], r15d        ; ← Vec<u32> push（元素 4 字节）
[r14+0x10] = rbx+1                       ; len++
test rbx, rbx ; je <首次入列分支>        ; 只有"push 前是空的"才继续
```

首次入列分支里连发 5 条事件，判别值（`lea rax,[rbp+N]` 换算成 `0x8000…|(N-1)`）：

| 指令 | tag | 与 `docs/96` 事件的对应 | 等级 |
|---|---|---|---|
| `0x51a162` `[rbp+0x3c]` | **59** | 疑似 `name_request_event` | 推断 |
| `0x51a239` `[rbp+0x34]` | **50** | `skin_graphics_request_event`（另一路已证 50） | 观测 |
| `0x51a310` `[rbp+0x36]` | **52** | 疑似 `face_graphics_request_event` | 推断 |
| `0x51a3e7` `[rbp+0x38]` | **54** | `hat_graphics_request_event`（另一路已证 54） | 观测 |
| `0x51a443` `[rbp+0x43]` | **66** | 疑似 `rank_request_event` | 推断 |

⇒ **职责：本地玩家拿到 `player_info_event` 后，按连接 id 过滤、把 `player_id` 收进
`Vec<u32>`；当这个 Vec 从空变非空时，一次性向服务端索要该玩家的名/皮肤/脸/帽子/段位。**
这条把 `docs/96` §3 里缺的 `NameRequest`/`RankRequest`/三个 `graphics_request` 的**发起侧**接上了。〔观测 + 推断〕

### 9.3 `0x51a610`（line 146，0xe0 B）— 处理 tag 14

`movabs rax, 0x800000000000000e`；函数体只有一条 `panic_fmt` 出口（`0x925660`，
串 = `"explicit panic"`，长度 `esi=0xe=14`，逐位）。

⇒ 观测上它是一条 `entity_removed_event` 的"空处理"（要么是 `unreachable`，
要么是本地不关心）。**为什么存在**列为未知。〔观测〕

---

## 10. `event_handler`（2 个函数）

### 10.1 `0x3d38f0`（line 396/407/454，`0x16b0` B）— 事件名 → 判别值

参数 `(rdi=事件处理器, rsi=名字 ptr, rdx=名字 len, rcx=&状态, r8=静态描述符)`。

```
add rdx, -0xa ; cmp rdx, 0x18 ; ja default        ; ← 名字长度必须在 10..34
lea rax, [rip - 0x214f59]                          ; 0x1be9c8 = 25 项跳表
movsxd rcx, dword ptr [rax + rdx*4] ; add rcx, rax ; jmp rcx
```

跳表实测（`0x1be9c8`，`int32` 相对偏移）：

| len | 目标 | len | 目标 |
|---|---|---|---|
| 10 | `0x3d392a` | 23 | `0x3d3a47` |
| 11/12/13 | default `0x3d469a` | 24 | `0x3d3e3d` |
| 14 | `0x3d3c50` | 25 | `0x3d3bcd` |
| 15 | `0x3d3fd1` | 26 | `0x3d397c` |
| 16 | `0x3d3f9c` | 27 | `0x3d428b` |
| 17 | `0x3d3b12` | 28 | `0x3d43a0` |
| 18 | `0x3d3c7e` | 29 | `0x3d46b8` |
| 19 | `0x3d40e0` | 30 | `0x3d45f7` |
| 20 | `0x3d4184` | 31/32/33 | default |
| 21 | `0x3d44c4` | 34 | `0x3d3952`（改用 `memcmp`，`edx=0x22=34`） |
| 22 | `0x3d446d` | | |

**长度即名字长度**，所以每个分支里只要 16 字节 SIMD 比较就够（`movdqu xmm,[rsi]` +
`movdqu xmm,[rsi+K]` + 两条 `pcmpeqb` + `pand` + `pmovmskb` + `cmp eax,0xffff`），
其中 `K = len − 16`。这给出一个可复算的自洽校验：`K + 16 == len`，实测全中。

**已逐位解出的名字→判别值**（下表每一行都能用"比较常量 + `mov ebx,imm`"两条字节复原）：

| 名字 | len | tag | 名字 | len | tag |
|---|---|---|---|---|---|
| `bot_added_event` | 15 | 10 | `function_active_event` | 21 | 36 |
| `player_info_event` | 17 | 13 | `function_points_event` | 21 | 37 |
| `entity_removed_event` | 20 | 14 | `clear_obstacles_event` | 21 | 39 |
| `soldier_info_event` | 18 | 15 | `add_obstacle_event` | 18 | 40 |
| `target_info_event` | 17 | 23 | `destroy_obstacle_event` | 22 | 41 |
| `remove_team_event` | 17 | 25 | `queue_destroy_obstacle_event` | 28 | 42 |
| `life_queued_death_event` | 23 | 28 | `axis_mode_info_event` | 20 | 45 |
| `game_state_info_event` | 21 | 30 | `function_mode_info_event` | 24 | 46 |
| `function_fire_event` | 19 | 33 | `turn_mode_info_event` | 20 | 47 |
| `entity_effect_info_event` | 24 | 56 | `turn_limit_info_event` | 21 | 57 |
| `name_info_event` | 15 | **60** | `rank_info_event` | 15 | **67** |

另有 4 条**只有长度、名字未唯一确定**（同一长度分支内的 16 字节比较不足以区分）：
`len=16` 分支的 `color_info_event`（50 也出现在 skin_graphics_request 上，两者未分开）、
`team_info_event` 两次扫描给出 24 / 27 两值（**未收敛**，其中一个来自相邻 arm 的 `mov ebx`）。
本节其余每一条都能用"比较常量 + `mov ebx,imm`"两条字节复原。

### 10.2 `0x3d3630`（line 467，0x2c0 B）— 用一个**常量** tag 查订阅者

```
rcx = [rdi] ; cmp qword ptr [rcx+0x10], 0 ; jne panic
movabs rdx, 0x8000000000000000 ; add rdx, 0x4a ; mov [rsp], rdx   ; ⇒ 常量 key = tag 74
…… hashbrown 查找（imul r8, r8, 0x38；cmp r14, qword ptr [rcx+r8-0x38]）
   ⇒ HashMap<u64, V>，entry 0x38 = 56 字节，**key 是整条 tag word（u64）**
…… 命中后 [rcx-0x30] = entry+0x08，[rcx-0x18] = entry+0x20，再对后者做第二层迭代
```

⇒ 观测：**`HashMap<u64 /*tag word*/, 订阅者集合>`，elem 56 字节**；
本函数对常量 tag 74 做一次派发。〔观测〕
`0x4a`(74) 对应哪个事件 **未知**（`.rodata` 里没有 len 属于 10..34 的
`..._event` 与它对上；74 已经超出了 `0x3d38f0` 覆盖的范围）。

---

## 11. `logger`（2 个函数）

### 11.1 `0x3b6c70`（line 12，0xd0 B）

```
mov rax, qword ptr [rdi]      ; rdi 是一个 &FnOnce
mov qword ptr [rsp], rsi
xor edi, edi
call rax                      ; 调它、传 0，得到 thread_local 槽的 Option
r15 = rax ; test rax,rax ; je <null 分支 → panic @ std/thread/local.rs:428>
cmp qword ptr [r15], 0 ; jne <panic @ graphwar_logger.rs:12>     ; ← 已占用就炸
[r15] = 0xffffffffffffffff
rax = [r15+8] ; test rax,rax ; je skip
dec qword ptr [rax] ; jne skip
lea rdi,[r15+8] ; call [0x9ba7a8]      ; 强引用归零 → drop
[r15+8] = rbx ; inc qword ptr [r15]
```

⇒ 观测：一个**线程局部槽的写入器**，槽 = `{ flag@+0, handle@+8 }`；`flag` 被写 `-1`
后再 `inc` 回 0，`handle` 是带强引用的对象（`dec` 到 0 就调 drop）。
⇒ 推断：这是 `set_logger()` / `with_logger()` 的安装侧；`panic` 落在 `logger.rs:12`
说明该行有 `.expect()`/`assert!`。〔观测 + 推断（缺：谁来调它）〕

### 11.2 `0x44dd60`（line 44，0x630 B）

直接调用 `0x3c6e90`×2、`0x45f160`×2、`0x45ea50`×2、`0x45eff0`、`0x461100`；
`0x528070`（dealloc）出现 **11 次**，`0x9ba628`/`0x9ba630`（alloc 前后）出现 5 次；
字符串只引到 `logger.rs`（`0x1b50e5`）。

⇒ 观测：这是**真正做格式化/落盘**的那条（分配/释放密集 + 格式化器调用）。
`0x461100` 在 `campaign_control` 里也出现，判为"取 Godot 单例"。〔观测；"落盘"为推断〕

---

## 12. 三个影子函数的细节（供后来者省掉重新推导）

### 12.1 `0x3d3530` = 事件入队 sink（**建议命名 `EventHandler::emit`**）

```
cmp byte ptr [rdi+0x38], 1 ; jne <VecDeque 路径>
  ── 环形缓冲路径：r15 = [rdi+0x30](head) ; cmp r15,[rdi+0x20](cap)
      相等则 call [0x9baef0]（扩容）
      dst = [rdi+0x28] + 9*r15*8 = + 0x48*r15
  ── 否则（[rdi+0x38] != 1）走 VecDeque：{cap@+0, ptr@+8, head@+0x10, len@+0x18}
       `lea rdx,[rax+1]; [r14+0x18]=rdx` … `cmovae` 环绕
两条路径都：写 4 条 movups(+0x30/+0x20/+0x10/+0x00) + 1 条 qword(+0x40) ⇒ 整块 0x48
```

⇒ 两种队列后端（定长环 / VecDeque），由 `+0x38` 的一个字节选择。**这是"事件去哪里"的答案。**
〔逐位〕

> **2026-11 已结案（`docs/151` §2.1）**：`0x3e6830` = **`Event` 的析构（drop glue）**，
> 真长度 **`0xd5`**（`0x3e6830..0x3e6905`，`.eh_frame` FDE），不是 `0x140` ——
> Ghidra 把隔壁 `0x3e6910`（`push r14/rbx/rax`，中间隔 11 字节 `int3`）并了进来，
> 所以下文"自身调 `0x3e6600`、`0x3e6370`"其实是 `0x3e6910` 的调用。
> 依据：全函数**零次写回 `(%rdi)`**；69 项跳表里 50 项指向同一条 `pop rbx; ret`，
> 其余臂只 `jmp *0x9ba5a0`（`R_X86_64_RELATIVE` → `0x528070` = dealloc）。
> ⇒ **不存在"填 payload / 校验"的语义，不需要复原。**

### 12.2 `0x3e6830`（0x140 B，模块归属未定）

被 `0x3d38f0` 的每个分支在 `mov ebx,imm` 之前调用（`rdi = rsp`，即刚拼好的 0x28 字节事件骨架）。
自身调 `0x3e6600`、`0x3e6370`、`0x528070`。**语义未知**。

### 12.3 `0x418170`（0x240 B）/ `0x4183b0`（0x470 B）

`user://campaign.save` 的使用者。`0x4183b0` 里引用两次（`0x4183d0` 读、`0x4185f9` 写）。
两者都只有 `alloc/string.rs:2916` 的 panic 位置 ⇒ **源文件归属证不出**。

---

## 13. 仍未确定（按重要性）

| # | 项目 | 现状 | 缺什么 |
|---|---|---|---|
| 1 | `RankInfoEvent` 3 个 dword vs serde "2 element" | 两处构造都写 3 个 dword（`+8/+0xc/+0x10`），serde 原文说 2 个元素 | 一个能读到该事件 payload 的消费侧，或 serde 的 `deserialize` 分支 |
| 2 | `0x455e30` 里"写 0x78 条目"的那一步 | 只见两次 parser 调用 + 发 tag 35；没找到 `[X+0x38]` map 的 insert | 该函数后半段（0x4562xx–0x4567xx）的逐字段还原 |
| 3 | `function_manager` 112 字节 value 的其余 88 字节 | 只证了开头 24 字节是 `Vec<Token>`（+0x08/+0x10/+0x18 相对 entry） | 需要一处"整块拷贝/逐字段写"的字节 |
| 4 | `local_player_manager` 的 tag 7 / tag 8 是什么事件 | 状态效果逐位清楚（`+0x18:=1`、`+0x1c:=id`、广播同一 id） | `.rodata` 里 len<10 或 >34 的短名/长名，`0x3d38f0` 不覆盖 |
| 5 | `event_handler` 0x4a（74）是哪个事件 | `HashMap<u64,订阅者>` 的常量 key = 74 | 同上 |
| 6 | `0x456f80` 与 `0x456df0` 偏移直方图完全一致 | 两个 0x190 函数，疑同构 | 调用者（两者都没有直接调用者） |
| 7 | 段位换算 / 22 档 / SVG 索引 | `rank_manager` 内**没有**算法 | 须另找模块（`player_screen_control`? `.gdc`?） |
| 8 | `campaign.save` 的二进制格式 | 只知道 3 处引用 | 需要它的读写函数逐字节还原 |
| 9 | `0x418170`/`0x4183b0`/`0x4166b0`/`0x417560` 的模块归属 | 只有地址相邻 | `module_funcs.tsv` 没有它们（无 panic 串） |
| 10 | `event_handler` / `logger` 各多出的第 3 个函数 | `module_audit` 说 3 个，`module_funcs.tsv` 只有 2 个 | 地址未知（表里没有） |
| 11 | `name_manager` 的 map 基址：`0x519350` 用 `self+0x18`，`0x519030` 的 find 用 `self+0x38` | 两处都是逐位观测 | 需要第三个函数确认 self 类型是否同一个 |

---

## 14. 与现有文档的冲突 / 修正

| # | 现有说法 | 本文实测 | 结论 |
|---|---|---|---|
| 1 | `docs/96` §3 把缺的事件记成 `NameInfo` / `RankInfo` / `LockInfo`（CamelCase） | `.rodata` 与 `0x3d38f0` 里被比较的键是 **`name_info_event` / `rank_info_event` / `lock_info_event`**（snake_case + `_event`） | **要改**：补协议时用 snake_case 名 |
| 2 | `docs/02` §14.5「`graphwar_campaign_control.rs` 只有一个函数，配合 `user://campaign.save` ⇒ 战役进度存在存档里」 | `user://campaign.save` 的引用点**不在**该函数里，而在 `0x418170` / `0x4183b0`；这两者的模块归属证不出 | **结论方向成立，但证据链要换**：不能说"那 1 个函数只做存取"，它做的是节点装配 |
| 3 | `docs/02` §14.5「段位图标 22 张」 | 只证实 22 个**资源文件**；`rank_manager` 里没有任何 22 项表或取模 22 | 不要把"22 张图"读成"22 段位" |
| 4 | `docs/96` §4「33 个判别值至今没有独立验证」 | `0x3d38f0` 的长度跳表 + 比较常量 + `mov ebx,imm` 可**逐位**解出判别值；已解 20+ 条（§10.1） | 结论（"不影响可观测行为"）不变，但"不可验证"这句可以撤了 |
| 5 | `AGENTS.md`「token 16 字节」 | `0x455b60` 的 `shl 4` + `alloc(16N,8)` + `memcpy(16N)` | **对上** |
| 6 | 常见 Rust 布局假设 `{ptr,cap,len}` | 本二进制是 **`{cap,ptr,len}`**，空串 = `(0,1,0)` | 读别的模块时会踩，先记在这里 |

---

## 15. 证据等级自查表（给 review 用）

| 函数 | 最高到达的档 | 关键依据 |
|---|---|---|
| `0x454f30` | **逐位** | 两条订阅串 + `edx=0x14` 长度 + `lea rsi` 地址 |
| `0x454d80` | **逐位** | `shl 3`、`pshufd 0xe1`、tag `0x8000…43` |
| `0x455090` | **逐位** | 参数寄存器 → payload dword 的 3 条 store + tag |
| `0x518f10` | **逐位** | `shl 5`、`[rsi-0x20]` key、tag 0x3c |
| `0x519030` | **逐位** | tag 0x…0e、`[rsi+8]`、find+remove |
| `0x519350` | **逐位** | insert 助手 + 24B String + tag 0x3c + u32@+0x20 |
| `0x455b60` | **逐位** | `shl 4` + `alloc(16N,8)` + `memcpy` + cap/ptr/len 三处 store |
| `0x3d38f0` | **逐位** | 跳表 + 比较常量可复算（`K+16==len` 全中） |
| `0x51a570` | **逐位** | `cmp qword [rsi], 0x8000…07` + 两条 `mov [rdi+0x18/0x1c]` |
| `0x455e30` | **观测** | 两次 `0x9bacf8`(=0x4ed670) 调用 + tag 35；**没**读到 map 写入 |
| `0x456810` | **观测** | `[rdi+0x18]→X`、`X+0x38` find |
| `0x454e80` | **观测** | tag 0x…0e；remove 助手调用 |
| `0x51a610` | **观测** | tag 0x…0e + 唯一 `panic_fmt` 出口 |
| `0x519f40` 发出的 tag 59/52/66 | **推断** | tag 值逐位，**名字**靠单调次序推 |
| `0x3d3630` | **观测** | entry 0x38、key=u64 tag、常量 0x4a |
| `0x3b6c70` | **观测** | 槽 `{flag@0, handle@8}`、TLS panic 位置 |
| `0x44dd60` | **推断** | 分配/格式化密集，**没**读出写盘目标 |
| `0x4565f0` / `0x456b80` / `0x456df0` / `0x456f80` | **推断** | 只有偏移直方图 |
| `0x418820` | **观测** | 节点取值调用 + 串；语义（战役界面哪一屏）为推断 |
| `0x3ef990` | **观测** | 节点名/图名串 + `graphwar_nodes.rs` 调用 |
