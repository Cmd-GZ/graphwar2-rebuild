# 118 · 战役 21 关：数据到底在哪、`campaign.save` 是什么格式、还缺什么

> 目标（本批任务的原始提问）：
> ① 关卡数据存在代码里 / 资源里 / 存档里，还是**根本不在客户端**？
> ② `user://campaign.save` 的格式与确切字段；
> ③ 21 关的标题；
> ④ 解锁规则；
> ⑤ 每关初始配置（障碍 / 士兵 / Bot）是数据驱动还是运行时生成；
> ⑥ `level_<n>` 这个 `input_text` 命令键把 n 传给谁。
>
> **本文的结论是：数据在 Rust 二进制里，硬编码，不在资源里、不在存档里、
> 不来自服务器。** 21 关的「标题 + 描述 + 四个模式」已经**全部解出**（§4 全表）。
>
> **2026-11-06 追加（本次改动，见 §12）：`§7` 的缺口表已经全部填满 —— 用户不需要再补任何关卡参数。**
> 同时**纠正本文 §2.3 的一处误判**：`0x466d20` **不是**关卡生成器（它是"关卡开场驱动"），
> 真正的生成器是 **`0x3f1e50`（散布）+ `0x49f1c0`/`0x49f370`/`0x4a5e80`（中心单障碍）**，
> 全部经 `0x466c40` 这一层调用。逐关参数由**闭包预设表**（`.data.rel.ro`）给出，
> 并有 **385 份 replay 的运行时真值**交叉验证（21/21 全对）。
>
> 证据分级只有四档：**逐位验证** / **观测** / **推断（写明缺什么）** / **未知**。
> 地址一律是 **ELF 虚地址**；`Ghidra = 虚地址 + 0x100000`，只加一次。

---

## 0. 一页结论

| # | 问题 | 答案 | 等级 |
|---|---|---|---|
| ① | 关卡数据在哪 | **`libgraphwarrust.so` 的 `.text` 里，21 个 `match` 分支硬编码**。不是 `.scn`/`.tscn`/`.tres`/`.csv`，不是 `campaign.save`，不是服务器下发 | **观测 + 逐位验证**（§2） |
| ② | `campaign.save` 格式 | **纯文本一行 `<key>`，内容是 `level_<n>`**（如 `level_20`）。写：`FileAccess::store_string`；读：整文件进 `String`，再 `rsplit('_').next().parse::<u32>()` | **观测**（写路径 **逐位验证**） |
| ②b | 里面存什么 | **只存「最高解锁关卡」这一个数**，不是每关分数/星级。**单调**：只有新值 **大于** 旧值才落盘 | **观测** |
| ③ | 21 关的标题 | **`Level 0` … `Level 20`**（21/21 全解）；另有 **21 段关卡描述**全解，逐字节精确 | **逐位验证**（§4） |
| ④ | 解锁规则 | 存档里的 `level_<n>` 就是「已解锁到 n」（0 基）；**没有**「星级/通关次数」概念；写完后调 `0x418820(self, n+1)` 刷新按钮。`0x418820` 里硬编码 `cmp ebp, 0x14`（= 20，最后一个下标） | **观测**（"这就是解锁语义"为**推断**，见 §5.3） |
| ⑤ | 每关初始配置 | **数据驱动，但数据就是代码**：每关有 **4 个模式字节**（函数/回合/坐标轴/时间）+ 一次关卡开场驱动调用 `0x466d20`。该调用带走 **两个闭包（fat pointer：data + vtable）**，其中一个就是**障碍生成器预设**（`.data.rel.ro` 静态表）。士兵数 / 目标数 / Bot 难度**都是立即数或常量池项**，已 21/21 全部解出 | **逐位验证 + 运行时**（§12） |
| ⑥ | `level_<n>` 传给谁 | 战役界面按钮 → `input_text` → Rust 命令分派（`0x499740` 内的 `cmp`/`movabs` 链，`level_10`…`level_20` 在 `0x499866`–`0x499930`）→ 选中该关 → 填 `JoinLevelPopUp`（标题/描述/4 个模式按钮）→ `start_level` → 该关分支调生成器 | **观测**（§6） |

**一句话给用户**：客户端**有**关卡数据，但那是「标题 + 一段英文教学描述 + 四个开关」，
**没有**逐关的障碍坐标表。要精确复刻每关的障碍/敌人布局，只能靠人补（§7 表格）。

---

## 1. 方法与复现

| 用途 | 做法 |
|---|---|
| 反汇编 | `objdump -d --start-address/--stop-address`；地址是**虚地址** |
| 虚地址 ↔ 文件偏移 | `.text`：vaddr `0x37b940` / off `0x37a940` ⇒ **off = vaddr − 0x1000**；`.rodata`：**off == vaddr** |
| 带注释的反汇编 | `.venv/bin/python tools/campaign_data.py`（本文新增，见 §9） |
| 字符串 | 注意 **7 字节的 `Level 0`..`Level 9` 不存在于字节流里**（用两条 `mov dword` 拼），`strings` 搜不到，见 §10 坑 1 |
| 常量证据 | 每关的描述都是 `.rodata` 里一段**连写**的英文，由 `movups`（16 字节）分块搬进堆上 `String`，随后 `mov qword [rsp+0x38], <len>` 写长度 |

**自校验（都通过）**：

* 21 段描述各自解码后长度**恰好等于**该关 `[rsp+0x38]` 立即数（21/21）；
* 21 段描述在 `.rodata` 里**首尾相接、无缝无重叠**（`0x1c21b1`…`0x1c27ba`，见 §4）；
* `level_<n>` 与 `Level <n>` 成对出现 **21/21**；
* 模式 dword 的高字节 21/21 都是 `0x05`，低字节只出现 `{0,1}` 且与描述文本一致（§4.3）。
  ⚠️ **`0x05` 的语义已被 `docs/123` §7 推翻**：21/21 关运行时 `TimeModeInfo` 都是 **`TimerInf`**
  （战役关卡根本没有回合倒计时）。所以 `0x05` **不是**时间档；它是什么**仍未知**，别再按"时间档 5"读。

---

## 2. 结论 A：关卡数据在**二进制**里（Q1）

### 2.1 排除法（每条都是负结果，能指到搜索边界）

| 候选位置 | 结果 | 证据 |
|---|---|---|
| `rsc/*.scn` / `*.tscn` / `*.tres` | **没有**关卡数据 | `campaign.tscn` 只有 92 行、节点 + `Label.text = "CAMPAIGN"`；`join_level_pop_up.tscn` 里 `LevelTitleLabel.text` 是占位 `"Level 0"`、`LevelDescriptionEdit.text` 是占位 `"This is the level description"`，**随关卡变化的内容全是运行期由 Rust 写进去的** |
| `.gdc` 字节码 | **没有** | 105/105 脚本已解完（`docs/101`）；`CampaingLevelsButtons.gdc` 只做「造 21 个按钮、`input_text = "level_" + str(i)`」，没有任何关卡内容 |
| `user://campaign.save` | **没有** | 文件内容就是一个 `level_20`（§3） |
| 服务器下发 | **没有** | 关卡在**单人战役**里就存在；且 §2.2 的 21 个 `match` 分支里是**字面量常量**，不是网络数据 |
| **`libgraphwarrust.so` 的 `.text`** | ✅ **在这里** | §2.2 |

### 2.2 正面证据：`level_info` 是一个 21 分支的 `match`，分支体在 `.text` 里

地址区间 `0x4ad380`（函数入口）→ `0x4af100`。**21 个分支**，每个分支依次做四件事：

```
① 造 key 字符串 "level_<n>"      （0..9 用两条 mov dword；10..20 用一条 movabs）
② 造标题 "Level <n>"             （同上：0..9 两条 mov dword，10..20 一条 movabs）
③ 从 .rodata 的连写文本里 movups 拷出该关描述，长度由 `mov qword [rsp+0x38], <len>` 给出
④ 填 12 字节尾巴：[+0x30] 一个 u32（部分关是常量、部分是运行期算的）、
   [+0x38..+0x3b] = 模式四元组（§4.3）
⑤ call 0x442aa0  ← 把上面这一坨做成一条记录（21 处调用，一一对应，§2.3）
```

**逐分支地址（key 构造点 / 标题构造点 / 描述长度立即数）**：

| 关 | `level_<n>` 构造 | `Level <n>` 构造 | 描述长度写在 |
|---|---|---|---|
| 0 | `0x4ad412` | `0x4ad458` | `0x4ad4be` |
| 1 | `0x4ad565` | `0x4ad5ab` | `0x4ad606` |
| 2 | `0x4ad6be` | `0x4ad704` | `0x4ad75f` |
| 3 | `0x4ad803` | `0x4ad849` | `0x4ad8cd` |
| 4 | `0x4ad97b` | `0x4ad9c1` | `0x4ada24` |
| 5 | `0x4adac8` | `0x4adb0e` | `0x4adb71` |
| 6 | `0x4adc18` | `0x4adc5e` | `0x4adccc` |
| 7 | `0x4add7a` | `0x4addc0` | `0x4ade26` |
| 8 | `0x4aded4` | `0x4adf1a` | `0x4adf81` |
| 9 | `0x4ae025` | `0x4ae06b` | `0x4ae0c3` |
| 10 | `0x4ae16a` | `0x4ae1b0` | `0x4ae229` |
| 11 | `0x4ae2d7` | `0x4ae31d` | `0x4ae3ac` |
| 12 | `0x4ae453` | `0x4ae499` | `0x4ae4fc` |
| 13 | `0x4ae5a0` | `0x4ae5e6` | `0x4ae64c` |
| 14 | `0x4ae6f0` | `0x4ae736` | `0x4ae799` |
| 15 | `0x4ae840` | `0x4ae886` | `0x4ae8f4` |
| 16 | `0x4ae99e` | `0x4ae9e4` | `0x4aea3f` |
| 17 | `0x4aeae9` | `0x4aeb2f` | `0x4aebb6` |
| 18 | `0x4aec5a` | `0x4aeca0` | `0x4aed03` |
| 19 | `0x4aedaa` | `0x4aedf0` | `0x4aee65` |
| 20 | `0x4aef0c` | `0x4aef52` | `0x4aefc0` |

> 例：`0x4ae06b` 的字节是 `c7 40 03 65 6c 20 39` + `c7 00 4c 65 76 65`
> ⇒ 在偏移 3 写 `"el 9"`、偏移 0 写 `"Leve"` ⇒ 堆上 buffer 是 `"Level 9"`。

### 2.3 ⚠️ **本节 2026-11-06 已被纠正**：`0x466d20` 不是关卡生成器

原文（保留作过程记录）："`0x466d20` 全二进制有 22 个调用点，除 `0x474856` 外其余 21 个全在战役区间里，
每关一次 ⇒ 它是关卡生成器"。**调用点计数是对的（22 个，逐位复核通过），但"它是生成器"是错的。**
正确结论见 **§12.1**，一句话版：

| 函数 | 实际身份 | 判据 |
|---|---|---|
| `0x466d20` | **关卡开场驱动**：清空障碍 Vec（0x48 步长）→ 调闭包 A 最多 5 次 → 每次用闭包 B 校验布局 → 调闭包 B 收尾 | 函数体 0x466d20–0x467530；`call *0x58(%rsp)` / `call *0x60(%rsp)` 两个 vtable 槽 |
| `0x466c40(ctx, min_count: i32)` | **散布式障碍生成器**（对外唯一入口） | `0x466c80 call 0x3f1e50`，全二进制**唯一**一处 |
| `0x3f1e50(ctx, min_count)` | 散布式生成器本体（`docs/55` 的公式） | `place_obstacle` 调用点 `0x3f1f4a` |
| `0x49f1c0` / `0x49f370` / `0x4a5e80` | 中心单障碍生成器（同一算法的 3 份拷贝） | `place_obstacle` 调用点 `0x49f29a`/`0x49f44a`/`0x4a5f5a` |
| `0x49e570` / `0x49eab0` / `0x4a93e0` | **"无障碍"**：只 `reset_mask`（`0x3f19a0`），不产生障碍 | 函数体 0xe0 字节，无 `place_obstacle` 调用点 |

**为什么 `fixtures/module_funcs.tsv` 把 `0x466d20` 同时归到两个模块**：那张表是**启发式**的
（`tools/module_map.py`：用函数引用的 `src/<file>.rs:<line>` panic 串归堆）。`0x466d20` 的
panic 路径同时引用了 `graphwar_level_generator.rs` 和 `graphwar_plane_pos_manager.rs` 两个
`panic::Location`（`.data.rel.ro` `0x958000` / `0x958018` / `0x957fd0` / `0x958108` …），
所以两行都出现。**这不是映射表错误，也不是同一个函数被认错——是"一个函数引用了两个模块的
panic 位置"**。全表共有 75 个地址被归到 ≥2 个模块，属同一现象。
⇒ **不能用 module_funcs.tsv 判函数身份**；判身份只能用 `place_obstacle` / `0x3f1e50` 的调用点。

### 2.4 与 `docs/103` 的关系（必须讲清楚，别再被误读）

`docs/103` 证明的是「**关卡生成器**只有两种模式（随机散布 / 中心单障碍），
`0x4ea6dc` 的『圆盘式』不是第三种」。那是**生成器**的形状，与**战役关卡**是两层：

* 生成器：`graphwar_level_generator.rs`，输入参数、输出障碍 → `docs/61` 已逐位复现；
* 战役：`level_info()`（标题/描述/模式）+ 每关一次 `0x466d20`（开场驱动），后者带走
  **障碍生成器闭包**（散布的 `min_count` 逐关不同，或中心单障碍，或"无障碍"）与
  **布局校验闭包**。

所以「生成器只有两种模式」**不等于**「关卡只有两种布局」——`min_count` 与选哪种预设不同，
出来的布局就不同。**逐关预设现在已解出（§12.2）：21 对（A/B）各 0x28 字节的静态闭包记录；A 侧只有 6 种行为，逐关一一对上。**

---

## 3. 三个存档文件的格式（Q2）

> 用户提供的真实文件内容（观测，来自父 agent 转述）：
> `campaign.save` = `level_20`（一行）；`player.config` = 5 行 `key: value`；
> `local_ip.config` = `1`。下面把它们和代码对上。

### 3.1 `user://campaign.save`

| 项 | 值 | 等级 |
|---|---|---|
| 路径常量 | `.rodata 0x1bfe28`，长度 `0x14` = **20** 字节 = `"user://campaign.save"` | **逐位验证** |
| 读 | `0x418170()` | **观测** |
| 写/更新 | `0x4183b0(self, &String)`，内部两次引用路径（`0x4183d0` 读、`0x4185f9` 写） | **观测** |
| 解析辅助 | `0x417ef0(ptr, len) -> (ok: u32, value: u32)` | **逐位验证** |
| 解锁刷新 | `0x418820(self, index)` | **观测** |
| 编码 | **纯文本**，`FileAccess` + `store_string`，**不是** `store_var` / `ConfigFile` / JSON / 二进制 | **逐位验证**（§3.1.1） |
| 字段 | **只有一个**：已保存的最高进度，形如 `level_<n>` | **观测** |
| 单调 | 新值 **>** 旧值才写；否则直接返回，不动文件 | **逐位验证**（§3.1.2） |

#### 3.1.1 写的是 `store_string`（不是 `store_var`）

`0x3ea720` 是 gdext 生成的 `FileAccess::store_string` 包装，字节里能直接读到它内部
interned 的 **(类名, 方法名)** 对：

```
0x3ea778  lea rax,[rip-0x22aba3]        ; → 0x1bfbdc  "FileAccess"   （随后 mov [rsp+0x60], 0xa   = 长度 10）
0x3ea78d  lea rax,[rip-0x22abae]        ; → 0x1bfbe6  "store_string" （随后 mov [rsp+0x70], 0xc   = 长度 12）
```

同段 `.rodata` 里，`FileAccess` 的**被引用方法名只有三个**：
`store_string`(`0x1bfbe6`)、`get_file_as_bytes`(`0x1bfbf2`)，以及 `open`(`0x1b97c4`)。
**没有 `store_var`、没有 `store_line`、没有 `ConfigFile`。**
⇒ 文件里不会出现 `Variant` 头或二进制前缀，只能是原样的字符串。

`open` 也是逐位确认的（`0x3eacf0` 就是 `FileAccess::open` 的包装）：

```
0x3ead4d  lea rax,[0x1bfbdc] ; mov [rsp+0x78],rax ; mov qword [rsp+0x80], 0xa   → "FileAccess", len 10
0x3ead65  lea rbx,[0x1b97c4] ; mov [rsp+0x88],rbx ; mov qword [rsp+0x90], 4     → "open",       len 4
```

> `0x1b97c4` 处字节是 `"openfire"`——**`open` 是它的前缀**，长度 4 由紧邻的
> `mov qword [rsp+0x90], 4` 给出。这也是 §10 坑 2 的另一种形态：**共享前缀的字符串池**，
> 只看 `strings` 会以为这里是 `"openfire"`。

调用点：`0x41860f` 用 `mode = 2` 打开 → `0x418647 call 0x3ea720`，参数
`(rdi = FileAccess, rsi = [r14+8], rdx = [r14+0x10])` = `store_string(入参字符串)`。

> `mode = 1` 读 / `mode = 2` 写，来自**行为配对**：`0x418170`（解析文件内容）与
> `0x4166b0`（解析 `key: value`）都用 1，两个写者（`0x4183b0`、`0x417560`）都用 2；
> 与 Godot `FileAccess.Mode {READ=1, WRITE=2}` 一致。判 **观测 + 推断**
> （独立证据是 Godot 的枚举定义，不在本二进制里）。

> ⚠️ **一处没定死**：`store_string` 只写字符串本身，**不加换行**。用户看到文件末尾有换行，
> 说明**调用方传进来的 `&str` 里就带 `"\n"`**（或者用户的编辑器显示如此）。
> 本机没有抓到写入时的实参，故这一条判 **推断**（缺：`0x4183b0` 调用方构造字符串的那 3 条指令）。

#### 3.1.2 读的是「整文件进 `String`」，再 `rsplit('_').parse::<u32>()`

`0x418170` 的结构：

```
0x418190  call 0x3eacf0                    ; FileAccess::open("user://campaign.save", 0x14, mode=1)
0x418195  cmp dword [rsp+0x50], 1          ; Option::Some ?
0x41819a  jne 0x4182e9                     ; 打开失败 → 返回 (0, 0)
...
0x41820a  lea rdi,[rsp+0x18] ; lea rsi,[rsp+0x38] ; call [0x9ba8e8]
                                           ; ↑ 把文件全文灌进 [rsp+0x20..0x38] 那个 Rust String
0x418222  r14=[rsp+0x20](cap) r15=[rsp+0x28](ptr) rsi=[rsp+0x30](len)
0x418234  call 0x417ef0                    ; 解析
0x418239  mov ebx,eax ; mov ebp,edx        ; 返回值 = (是否成功, 关卡号)
```

`0x417ef0(ptr, len)` 的本体就是标准库的 `split` + `FromStr for u32` 内联：

```
0x417f01  lea rcx,[0x1bfe27]  ; 分隔符 = '_'（0x1bfe27）
0x417f0d  mov r8d, 1          ; 模式长度 = 1
0x417f19  call [0x9baf98]     ; str::split('_')
0x417f4a  rax = 分段数；取「最后一段」（shl rax,4 后读 [rcx+rax-0x10] / [rcx+rax-8]）
0x417f96.. 逐位解析十进制：cmp al,0x2b('+') / 0x2d('-') 处理符号；
           每步 lea esi,[rbx+rbx*4] / lea ebx,[rdx+rsi*2] = n*10+d；rax>=9 走溢出分支
```

⇒ **语义：取最后一个 `_` 之后的部分，当十进制 `u32` 解析。**
所以 `level_20` → `20`，`level_0` → `0`。
**这也解释了为什么「关号」是 0 基**：字符串本身就是 `level_0`…`level_20`。

#### 3.1.3 写路径是「只增不减」

`0x4183b0(self, s)`：

```
0x4183e6  FileAccess::open("user://campaign.save", mode=1)      ; 先读
0x4184??  读全文进 String（同 §3.1.2 的 [0x9ba8e8]）
0x418554  0x417ef0(入参 s)      -> ebx = ok1, ebp = 新值
0x418563  0x417ef0(文件内容)    -> eax = ok2, edx = 旧值
0x418570  or (ok1^1),(ok2^1) ; je 0x4185b3     ; 两个都解析成功才比较
0x4185b3  cmp ebp, edx ; jbe 0x418579          ; 新值 <= 旧值 → 直接返回（不写）
                                               ; 否则落到写路径
0x41860f  FileAccess::open("user://campaign.save", mode=2)
0x418647  store_string(入参 s)
0x4186c8  0x417ef0(入参 s) -> edx ; inc edx
0x4186d9  call 0x418820(self, edx)             ; ← 用「新值+1」去刷新
```

⇒ **存档只有一个进度标量，且只会被更大的值覆盖。**
`0x418820(self, n+1)` 收到的是 **n+1**，函数体里有一处 `cmp ebp, 0x14`（= 20，最后一个下标）
+ `cmove`，说明它在遍历/钳位 21 个战役按钮。语义判 **推断**（缺：`0x418820` 里
节点访问器到底写了哪个属性；`docs/112` §7.1 已判它是「Godot 节点取值 + 委托」）。

### 3.2 `user://player.config`

| 项 | 值 | 等级 |
|---|---|---|
| 路径常量 | `.rodata 0x1bfe12`，长度 `0x14` = 20 = `"user://player.config"` | **逐位验证** |
| 写 | `0x417560`（`open(path, 20, mode=2)` 于 `0x417582`） | **观测** |
| 读 | `0x4166b0`（`open(path, 20, mode=1)` 于 `0x4166c4`；`0x416911` 用长度 **1** 的 `":"` 做分隔） | **观测** |
| 格式 | **每行 `key: value`，无 `[section]` 头** ⇒ **不是** Godot `ConfigFile`；是手写的「按 `:` 切」 | **观测** |
| 字段顺序 | 写者按固定顺序拼：`name`, `skin`(?), `face`, `hat`, `muted`(?) 之一一对应 | **推断**（§3.2.1） |

#### 3.2.1 字段名是**逐字符写进 buffer 的**，`strings` 搜不到

写者 `0x417560` 的字节（`0x417700` 起）：

```
0x417700  movw [rax+rsi+4], 0x203a      ; ": "
0x417707  movl [rax+rsi],   0x65636166  ; "face"
...
0x417793  movb [rax+rsi+4], 0x20        ; " "
0x417798  movl [rax+rsi],   0x3a746168  ; "hat:"
...
0x4176d9  movb [rax+rdi], 0x0a          ; '\n'   ← 每行以换行结尾
```

所以二进制里**不存在** `"skin: "` / `"muted: "` 这样的连续字符串（`grep` 全都 0 命中）——
这正是 §10 坑 2。用户给的实际文件已经把字段集说清楚了：
`name` / `skin` / `face` / `hat` / `muted`（5 个，与协议里 `NameInfo`/`SkinGraphicsInfo`/
`FaceGraphicsInfo`/`HatGraphicsInfo` + 静音开关一一对应，与 `docs/111` 的
`AudioServer.set_bus_mute` 对得上）。

**读法**（从 `0x4166b0` 的形状推断，缺的是逐字段的 `match` 分支，未逐条核）：
按 `\n` 切行 → 每行按第一个 `:` 切成 key/value → `match` 到字段。
**这一条判推断**：我只逐位确认了「分隔符是 `:`、长度 1」和「写者会写 `\n`」，
没有把 5 个 `match` 分支全部对到字节。

### 3.3 `user://local_ip.config`

| 项 | 值 | 等级 |
|---|---|---|
| 路径常量 | `.rodata 0x1c2dce`，`"user://local_ip.config"`（22 字节） | **逐位验证** |
| 使用者 | `0x4d3180`（`0x4d5d62` / `0x4d5efc` / `0x4d609e` 三处引用路径；`0x4d60ec` 引用紧跟其后的字面量 `"1"` @`0x1c2de4`） | **观测** |
| 内容 | `1`（单行一个数字）——与「紧跟路径的那个 `"1"` 字面量」一致 | **观测（弱）** |
| 语义 | **未知**（LAN 广播里选第几块网卡 / 是否启用，都没证），只确认它是 `0x4d3180` 读写的 | **未知** |

---

## 4. 21 关全表：标题 + 描述 + 四个模式（Q3）

### 4.1 标题

**21/21 都是 `Level 0` … `Level 20`**，逐字节精确（0..9 与 10..20 两种编码，见 §2.2 注）。
没有花名、没有副标题。

### 4.2 描述（逐字节精确，共 21 段，`.rodata` 里首尾相接）

| 关 | `.rodata` 偏移 | 长度 | 描述（原文，未做任何改写） |
|---|---|---|---|
| 0 | `0x1c21b1` | 40 | Write a function of x to hit the target. |
| 1 | `0x1c21d9` | 153 | A constant is always added to your function. You can see it on the bottom right corner of the screen. It makes sure the function is shot by your soldier. |
| 2 | `0x1c2272` | 129 | There are obstacles in the cartesian plane that don't let your function go through. Try to make a function that goes around them. |
| 3 | `0x1c22f3` | 94 | You now have multiple soldiers in your team. At each turn you will shoot from a different one. |
| 4 | `0x1c2351` | 45 | Try going around a plane with more obstacles. |
| 5 | `0x1c237e` | 42 | Play against an opponent that shoots back. |
| 6 | `0x1c23a8` | 60 | You must hit these targets in a single shot to destroy them. |
| 7 | `0x1c23e4` | 38 | Destroy your target in your first try. |
| 8 | `0x1c240a` | 49 | Hit all targets in a single shot to destroy them. |
| 9 | `0x1bcaa0` | 32 | Play against a smarter opponent. |
| 10 | `0x1c243b` | 80 | Hit all targets in a single shot to destroy them. The finer grid lines are gone. |
| 11 | `0x1c248b` | 111 | Play against an opponent on a plane with fewer grid lines. You and the opponent are also firing simultaneously. |
| 12 | `0x1c24fa` | 47 | Most grid lines are gone. Try hitting a target. |
| 13 | `0x1c2529` | 37 | Destroy two targets in a single shot. |
| 14 | `0x1c254e` | 46 | Play against an opponent with almost no grids. |
| 15 | `0x1c257c` | 58 | The main axes are gone. The opponent is smarter than ever. |
| 16 | `0x1c25b6` | 251 | You are now writting a differential equation instead of a function. Write a differential equation in terms of x and y. Your shot will be the solution to the differential equation using the position of the soldier that shot it as the initial condition. |
| 17 | `0x1c26b1` | 88 | You must hit both targets in a single shot with a differential equation to destroy them. |
| 18 | `0x1c2709` | 46 | Fight an opponent with differential equations. |
| 19 | `0x1c2737` | 67 | Fight an opponent with differential equations with less grid lines. |
| 20 | `0x1c277a` | 64 | Fight an opponent with differential equations and no grid lines. |

> **注意** 关 9 的描述**不在**那段连写区里（它是 `0x1bcaa0`，另一处 32 字节），
> 但长度与 `0x4ae0c3` 的 `0x20` 一致，两处互为独立证据。
> **注意** 关 16 原文就是 `writting`（双 t，拼写错误是原版自带的，别"修"）。

### 4.3 四个模式（每关 4 字节，`+0x38..+0x3b`）

数据来源：每关分支里 `mov dword ptr [rsp + 0x58], <imm32>`（`0x4ad4dd`、`0x4ad62c`、…、
`0x4aefdf` 共 21 条）。结构体基址是 `[rsp+0x20]`，所以这 4 字节落在 **+0x38..+0x3b**。

**字段顺序（与 `join_level_pop_up.tscn` 的四个按钮同名同序）**：
`+0x38 = FunctionMode`、`+0x39 = TurnMode`、`+0x3a = AxisMode`、`+0x3b = 时间档`。

| 关 | `imm32` | func | turn | axis | byte3 | 与描述是否自洽 |
|---|---|---|---|---|---|---|
| 0–9 | `0x05030000` | 0 | 0 | **3** | `0x05` | 前 10 关都是完整网格 ✔ |
| 10 | `0x05020000` | 0 | 0 | **2** | `0x05` | "The finer grid lines are gone" ✔ |
| 11 | `0x05020100` | 0 | **1** | **2** | `0x05` | "also firing simultaneously" ✔ |
| 12 | `0x05010000` | 0 | 0 | **1** | `0x05` | "Most grid lines are gone" ✔ |
| 13 | `0x05010000` | 0 | 0 | **1** | `0x05` | 描述不提网格 ⇒ 沿用上一关 ✔ |
| 14 | `0x05010100` | 0 | **1** | **1** | `0x05` | "almost no grids" + 同时开火 ✔ |
| 15 | `0x05000000` | 0 | 0 | **0** | `0x05` | "The main axes are gone" ✔ |
| 16 | `0x05030001` | **1** | 0 | **3** | `0x05` | "writting a differential equation" ✔ |
| 17 | `0x05030001` | **1** | 0 | **3** | `0x05` | 微分方程 ✔ |
| 18 | `0x05030001` | **1** | 0 | **3** | `0x05` | 微分方程 ✔ |
| 19 | `0x05010001` | **1** | 0 | **1** | `0x05` | 微分方程 + "less grid lines" ✔ |
| 20 | `0x05000001` | **1** | 0 | **0** | `0x05` | 微分方程 + "no grid lines" ✔ |

**解读的自校验（这是判「逐位验证」的理由）**：
* `axis ∈ {0,1,2,3}` 与描述里「网格越来越稀」**21/21 完全一致**；
* `func = 1` **恰好** 是关卡 16–20，与「从第 16 关开始教微分方程」**5/5** 一致；
* `turn = 1` **恰好** 是关卡 11（描述明写 "firing simultaneously"）与关卡 14（同样描述）**2/2** 一致；
* `byte3 = 0x05` 21/21 恒定 ⇒ 判为「时间档固定为 5」**（推断）**，
  也可能是别的字段；缺的是 `0x442aa0` 内部对该字节的消费者。

`axis` 的方向（3 = 最密、0 = 无轴）由描述反推，判 **观测**（数字与文本同向变化），
**没有**独立的第三处证据说明「3 就是最密」——这一条严格说是 **推断**。

### 4.4 「+0x30」的 u32 —— **仍然未知（§12 已把"每关真正的回合上限"改从别处解出）**

每个关分支还会写 4 或 8 字节到 `[rsp+0x50]`（= 结构体 `+0x30`）：

* 部分是**常量**：关 0 = `0`、关 10 = `0x400000001`（= u32 `1` 和 `4`）、关 11 = `0`、
  关 14/15/18/19/20 = `0`；
* 部分是**运行期算出来的寄存器**（`rbp`/`r12`/`rax`）：关 1/2/3/4/6/7/8/12/13/17。

**2026-11-06 复核结论**：这个字段**不是**回合上限。
每关真正的回合上限写在 `start_level` 分支里（`turn_limit_info`，§12.4），
例如关 9（1 基关 10）的运行期值是 `6`，而本字段是 `(1,4)`，两者不等。
⇒ 语义仍 **未知**；缺的证据是 `0x442aa0`（记录构造）之后该字段的**消费者**
（`join_level_pop_up` 只读人眼可见的标题/描述/4 个模式按钮，本字段没人读）。
**它不影响"每关有什么"这张表**，故不阻塞 §7。

---

## 5. 解锁规则（Q4）

### 5.1 `docs/108` §7 第 4 条（我们自定的「上一关通关解锁」）现在可以改了

原版的行为是：**`user://campaign.save` 里就是一个 `level_<n>`，代表「到第 n 关为止都解锁」**。
没有「每关通关次数」「星级」「最高分」——文件里连第二个字段都没有。

### 5.2 关卡总数 21 的三处独立证据

| 证据 | 内容 | 等级 |
|---|---|---|
| `CampaingLevelsButtons.gdc` | `for i in 21:` 造 21 个按钮，`input_text = "level_" + str(i)` | **逐位验证**（`docs/101` §4.1） |
| `level_info()` 的 21 个分支 | §2.2，21 组 `level_<n>`/`Level <n>`/描述/模式 | **逐位验证** |
| `0x418820` 内 | `cmp ebp, 0x14`（0x14 = 20 = 最后一个下标）+ `cmove` 钳位 | **观测** |

### 5.3 「`level_n` = 解锁到 n」是**推断**，缺什么写清楚

已证：文件只存这一个数；写入是单调的（只增大）；写完后用 `n+1` 去调按钮刷新函数。
**未证**：`0x418820` 到底把按钮改成什么样（`disabled=false`？换图？），
以及「解锁」是「可以点」还是「可以看描述」。
⇒ 缺的证据：`0x418820` 里那几个 Godot 节点访问器（`0x3ab390` / `0x4068f0` / `0x407630`）
落到哪个属性。**这一条留作推断**。

---

## 6. `level_<n>` 的生命周期（Q6）

```
CampaingLevelsButtons.gd                     ← .gdc 字节码（docs/101 §4.1）
   for i in 21:
       button.input_text = "level_" + str(i)          ← 运行期拼，所以 .tscn 里没有这 21 个串
       button.set_name("level_" + str(i))
                 │
                 ▼  按下 GraphwarButton → 把 input_text 交给 Rust
Rust 命令分派（0x499740 内，0x4995a0 起）
   cmp ... "level_10"   0x499866   ← 0..9 用两条 mov dword 拼，字节流里搜不到
   cmp ... "level_11"   0x499879
   ...
   cmp ... "level_20"   0x499924
                 │
                 ▼  选中某关 → 填 join_level_pop_up
level_info()  0x4ad380
   写 LevelTitleLabel.text      = "Level <n>"
   写 LevelDescriptionEdit.text = <21 段之一>
   写 AxisModeButton / FunctionModeButton / TurnModeButton / TimeModeButton 的图标
                 │
                 ▼  按下 "start_level"（join_level_pop_up.tscn，LongPressButton 旁边那个）
   该关分支：设 4 个模式 → 发 axis/function/turn/time_mode_info 事件
            → 发 turn_limit_info 事件
            → call 0x466d20（关卡生成器）**一次**，生成该关障碍
            → 生成士兵 / 目标 / Bot
```

**顺带一个负面发现**：`libgraphwarrust.so` 里**没有** `"start_level"` / `"cancel_level"` /
`"exit_campaign"` 这三个字符串（`grep -abo` 全 0 命中），但它们确实写在
`join_level_pop_up.tscn` / `campaign.tscn` 的 `input_text` 里。
⇒ 这三个键的**分派侧**不是直接字符串比较（可能被拆成片段/或被 `GraphwarButton`
在 GDScript 侧消费）。**这一条未解释，列为未知**（§7）。

---

## 7. 21 关全参数表（**2026-11-06 已填满，无 `?`**）

> 编号：表格用 **1..21**；二进制内部是 `level_0`..`level_20`（**0 基**）。「第 1 关」= `level_0`。
>
> 每格的来源都用下列短码标注（定义见 §12）：
> `S` = `start_level` 分支里的**立即数**（地址可指认）；
> `C` = `.data.rel.ro` **闭包预设记录**（地址可指认）；
> `L` = `level_info()` 的**模式 dword**（§4.3，已有）；
> `R` = **运行时真值**（385 份 replay，§12.5，与 `S`/`C` 独立）。
> 只有 `S`/`C` 与 `R` **两边都对上**时才写「逐位验证」；只靠单边的写「观测」。

### 7.1 主表

| 关 | 标题 | 障碍（模式 / 数量） | 我方士兵 | 目标数 | 敌方 Bot（难度档） | 回合上限 | 模式 func/turn/axis |
|---|---|---|---|---|---|---|---|
| 1 | `Level 0` | **无障碍**〔C `0x95a350`/`0x95a378`；R 0 个〕 | **1**〔S `edx=1`；R 1〕 | **1**〔S 1 次 `add_target`；R 1〕 | 无〔R〕 | **无上限**〔S；R〕 | 0 / 0 / 3 |
| 2 | `Level 1` | **无障碍**〔C；R 0〕 | **1**〔S；R 1〕 | **1**〔S；R 1〕 | 无〔R〕 | **5**〔S `(1,5)`；R 5〕 | 0 / 0 / 3 |
| 3 | `Level 2` | **中心单障碍**（原点，r∈[3,8)）〔C `0x49f1c0`；R 1 个 @(0,0) r=3.34〕 | **1**〔S；R 1〕 | **1**〔S；R 1〕 | 无〔R〕 | **5**〔S；R 5〕 | 0 / 0 / 3 |
| 4 | `Level 3` | **中心单障碍**〔C `0x49f370`；R 1 个 @(0,0) r=4.81/6.75〕 | **2**〔S `edx=2`；R 2〕← "multiple soldiers" | **3**〔S 3 次；R 3〕 | 无〔R〕 | **10**〔S；R 10〕 | 0 / 0 / 3 |
| 5 | `Level 4` | **散布式，min_count = 15**〔C `0x49fe80`；R 15 个〕 | **1**〔S；R 1〕 | **1**〔S；R 1〕 | 无〔R〕 | **10**〔S `(1,10)`；R 10〕 | 0 / 0 / 3 |
| 6 | `Level 5` | **散布式，min_count = 5**〔C `0x4a12f0`；R 17 个〕 | **2**〔S；R 2〕 | **0**〔R；S 无 `add_target`〕 | **有 ×1，档 0**（`bot_0`）〔S 难度值 `3`；R〕 | **无上限**〔S；R〕 | 0 / 0 / 3 |
| 7 | `Level 6` | **散布式，min_count = 10**〔C `0x4a2410`；R 17 个〕 | **1**〔S；R 1〕 | **2**〔S 2 次；R 2〕 | 无〔R〕 | **6**〔S `(1,6)`；R 6〕 | 0 / 0 / 3 |
| 8 | `Level 7` | **散布式，min_count = 6**〔C `0x4a2da0`；R 16 个〕 | **1**〔S；R 1〕 | **1**〔S；R 1〕 | 无〔R〕 | **1**〔S `(1,1)`；R 1〕← "first try" | 0 / 0 / 3 |
| 9 | `Level 8` | **散布式，min_count = 10**〔C `0x4a2410`；R 16 个〕 | **1**〔S；R 1〕 | **3**〔S 3 次；R 3〕 | 无〔R〕 | **6**〔S；R 6〕 | 0 / 0 / 3 |
| 10 | `Level 9` | **散布式，min_count = 5**〔C `0x4a12f0`；R 15/17 个〕 | **3**〔S `edx=3`；R 3〕 | **0**〔R；S 无〕 | **有 ×1，档 2**（`bot_2`）〔S `25`；R〕 | **无上限**〔S；R〕 | 0 / 0 / 3 |
| 11 | `Level 10` | **散布式，min_count = 10**〔C `0x4a2410`；R 10/13 个〕 | **1**〔S；R 1〕 | **2**〔S 2 次；R 2〕 | 无〔R〕 | **6**〔S `(1,6)`；R 6〕 | 0 / 0 / **2** |
| 12 | `Level 11` | **散布式，min_count = 5**〔C `0x4a12f0`；R 15 个〕 | **3**〔S；R 3〕 | **0**〔R；S 无〕 | **有 ×1，档 3**（`bot_3`）〔S `50`；R〕 | **无上限**〔S；R〕 | 0 / **1** / **2** |
| 13 | `Level 12` | **中心单障碍**〔C `0x4a5e80`；R 1 个 @(0,0) r=5.69〕 | **1**〔S；R 1〕 | **1**〔S；R 1〕 | 无〔R〕 | **5**〔S `(1,5)`；R 5〕 | 0 / 0 / **1** |
| 14 | `Level 13` | **散布式，min_count = 10**〔C `0x4a2410`；R 12/27 个〕 | **1**〔S；R 1〕 | **2**〔S 2 次；R 2〕← "two targets" | 无〔R〕 | **5**〔S；R 5〕 | 0 / 0 / **1** |
| 15 | `Level 14` | **散布式，min_count = 5**〔C `0x4a12f0`；R 21 个〕 | **4**〔S `edx=4`；R 4〕 | **0**〔R；S 无〕 | **有 ×1，档 3**（`bot_3`）〔S `100`；R〕 | **无上限**〔S；R〕 | 0 / **1** / **1** |
| 16 | `Level 15` | **散布式，min_count = 10**〔C `0x4a2410`；R 10…29 个〕 | **5**〔S `edx=5`；R 5〕 | **0**〔R；S 无〕 | **有 ×1，档 4**（`bot_4`）〔S `250`；R〕 | **无上限**〔S；R〕 | 0 / 0 / **0** |
| 17 | `Level 16` | **无障碍**〔C `0x95d358`；R 0〕 | **1**〔S；R 1〕 | **1**〔S；R 1〕 | 无〔R〕 | **无上限**〔S；R〕 | **1** / 0 / 3 |
| 18 | `Level 17` | **散布式，min_count = 10**〔C `0x4a2410`；R 10 个〕 | **1**〔S；R 1〕 | **2**〔S 2 次；R 2〕← "both targets" | 无〔R〕 | **6**〔S `(1,6)`；R 6〕 | **1** / 0 / 3 |
| 19 | `Level 18` | **散布式，min_count = 10**〔C `0x4a2410`；R 14/15/23/29 个〕 | **3**〔S；R 3〕 | **0**〔R；S 无〕 | **有 ×1，档 3**（`bot_3`）〔S `100`；R〕 | **无上限**〔S；R〕 | **1** / 0 / 3 |
| 20 | `Level 19` | **散布式，min_count = 10**〔C `0x4a2410`；R 10…25 个〕 | **4**〔S `edx=4`；R 4〕 | **0**〔R；S 无〕 | **有 ×1，档 3**（`bot_3`）〔S `100`；R〕 | **无上限**〔S；R〕 | **1** / 0 / **1** |
| 21 | `Level 20` | **散布式，min_count = 15**〔C `0x49fe80`；R 15…31 个〕 | **5**〔S `edx=5`；R 5〕 | **0**〔R；S 无〕 | **有 ×1，档 4**（`bot_4`）〔S `250`；R〕 | **无上限**〔S；R〕 | **1** / 0 / **0** |

> `axis` 的数字：**3 = EveryUnit（最密）、2 = EveryFive、1 = OnlyMain、0 = NoAxis**。
> 这是 `docs/4.3` 那张表的"方向"问题第一次有**独立证据**：运行时事件里
> `EveryUnit/EveryFive/OnlyMain/NoAxis` 与 `3/2/1/0` **21/21 一一对应**（§12.5）。
> （`docs/118` §4.3 原先只说"3 最密是推断"，现在升为 **逐位验证 + 运行时**。）

### 7.2 障碍的确切公式（障碍数不是常数，是随机变量的截断）

散布式（`0x3f1e50`，逐位复现见 `docs/61`）：

```
n = trunc( normal() * 7.0 + 15.0 )      # N(15, 7²) 的四舍五入截断
if n <= min_count: n = min_count        # ← §7.1 里的 min_count
每颗:  r = max(normal()*1.6 + 2.6, 0.8)
       x = (2r + 25) * (2u - 1)
       y = (2r + 15) * (2v - 1)
```

中心单障碍（`0x49f1c0`/`0x49f370`/`0x4a5e80`）：

```
r = u * 5.0 + 3.0     位置恒为 (0, 0)     恰好 1 颗
```

**`min_count` 是下界而不是定值**，这条有运行时硬证据：关 11/18/20 的实际障碍数
**出现过恰好 10**（= `min_count`），而关 5/21 出现过恰好 **15**（= `min_count`）；
其余样本都比下界大。⇒ 「min_count 被钳位」判 **逐位验证 + 运行时**。

### 7.3 与描述的交叉验证（21/21 全对，**没有一处对不上**）

| 关 | 描述要点 | 解出的参数 | 结论 |
|---|---|---|---|
| 4 | "multiple soldiers in your team" | 士兵 1→**2** | ✔ |
| 6 | "hit these target**s**"(复数) | 目标 **2** | ✔ |
| 7 | "in your first try" | 回合上限 **1** | ✔ |
| 9 | "all targets" | 目标 **3** | ✔ |
| 11 | "all targets" | 目标 **2**；axis 2 | ✔ |
| 12 | "also firing simultaneously" | turn=**1**（SimultaneousTurns）；有对手 | ✔ |
| 14 | "Destroy **two** targets" | 目标 **2** | ✔ 精确 |
| 16 | 网格 "main axes are gone" | axis=**0**（NoAxis） | ✔ |
| 17/21 | 微分方程 + 无轴 | func=1、axis=0 | ✔ |
| 18 | "hit **both** targets" | 目标 **2** | ✔ 精确 |
| 19/20/21 | "Fight an opponent…" | 8 个对手关**全部无目标**、各 1 个 Bot | ✔ |
| 1/2/3/13/17 | "the target"（单数） | 目标 **1** | ✔ |

唯一"描述没提"的地方：**关 4（`Level 3`）除 2 个士兵外还有 3 个目标**，
描述只讲士兵、没讲目标数。这不是矛盾（描述没否定），但属于**描述未覆盖的额外信息**，
记在这里以免被当成"多解出来了一个"。

---

## 8. 仍未确定清单（2026-11-06 复核后）

| # | 项 | 状态 | 缺什么 |
|---|---|---|---|
| 1 | 每关 **障碍布局** | ✅ **已解**（§7.1/§12.2） | — 障碍 = 预设闭包（无 / 中心单 / 散布 + `min_count`），坐标由 RNG 决定，**本来就没有固定坐标** |
| 2 | 每关 **士兵数** | ✅ **已解**（§7.1） | — 立即数 `N` 传给 `0x476320(mgr, player, N)` |
| 3 | 每关 **目标数** | ✅ **已解**（§7.1） | — `N` 次 `0x46f310(team)`；坐标为随机，非固定 |
| 4 | 每关 **Bot 数量 / 难度** | ✅ **已解**（§7.1） | — 0 或 1 个；难度值 3/25/50/100/250，`bot_N` 皮肤由运行时给出 |
| 5 | 结构体 `+0x30` 的 u32 | **未知** | `0x442aa0` 之后该字段没有消费者；**不影响 §7** |
| 6 | 模式 `byte3 = 0x05` | **推断 → 观测** | 运行时 `TimeModeInfo` 恒为 `TimerInf`（21/21），与 `0x05` 同向；仍缺"5 = TimerInf 枚举值"的直接证据 |
| 7 | `axis` 的方向（3=最密） | ✅ **逐位 + 运行时**（§7.1 注、§12.5） | — |
| 8 | `store_string` 是否带 `\n` | **推断** | 缺 `0x4183b0` 调用方构造字符串的那几条指令（附录 A 已用用户实验旁证：带 `\n` 会让进度归零） |
| 9 | `0x418820(self, n+1)` 改的是什么属性 | **推断** | §5.3 |
| 10 | `player.config` 的 5 个字段名逐条对到 `match` 分支 | **推断** | 只证了分隔符 `:`（长度 1）+ 写者会写 `\n` |
| 11 | `local_ip.config` 里 `1` 的语义 | **未知** | `0x4d3180` 里该值的用途 |
| 12 | `start_level` / `cancel_level` / `exit_campaign` 的分派侧 | **未知** | 三个串在二进制里 0 命中，但确实写在 `.tscn` 的 `input_text` 里 |
| 13 | 每关的 **回合上限** | ✅ **已解**（§7.1） | — `turn_limit_info` 载荷 `(1, N)` 的 `N` 就是上限，运行时 `TurnLimitInfo` 21/21 对上 |
| 14 | `turn_limit_info` 载荷**第一个 u32 恒为 1** | **未知** | 21/21 都是 `1`；缺消费者，无法判定它是"队伍数 / 回合所属 / 开关" |
| 15 | Bot 难度值 `3/25/50/100/250` 与 `AI.md` 六档表的映射 | **推断**（§12.4 有反例） | `3/25/100/250` 命中 `AI.md` 的「代数」列（档 1/3/4/5），但 **`50`（关 12）不在任何一列**；缺 `new_bot` 消费者里那张表的用法 |
| 16 | 士兵 / 目标**坐标** | **不适用** | 运行时是随机的（每份 replay 都不同）⇒ **本来就没有"每关固定坐标"这回事** |

**一句话：`§7` 那张表已经没有 `?`；用户不需要再补任何关卡参数。**
剩下 15 条的 5/6/8/9/10/11/12/14/15 与"每关有什么"无关或已降级为可用推断。

---

## 9. 复现

```bash
# 一条命令打印 21 关全表（标题 / 描述 / 四模式 / 描述在 .rodata 的地址）
cd gw2-rebuild
GW2_SO=../libgraphwarrust.so ../.venv/bin/python tools/campaign_data.py
GW2_SO=../libgraphwarrust.so ../.venv/bin/python tools/campaign_data.py --tsv   # 机器可读

# 存档读写函数（虚地址）
../.venv/bin/python ../../.agent-work/meta/fn.py 0x418170 0x4183b0 0x417ef0 0x418820
# 或者直接 objdump（虚地址！）
objdump -d --start-address=0x417ef0 --stop-address=0x418180 ../libgraphwarrust.so
objdump -d --start-address=0x4183b0 --stop-address=0x418820 ../libgraphwarrust.so
```

**自检判据**（错了这些一定不成立）：

* `tools/campaign_data.py` 必须报 `21 levels / 21 descriptions / 21 mode words` 且
  **每段描述解码后的字节数等于表里的长度**（脚本内部断言，失败会 `SystemExit`）；
* 21 段描述在 `.rodata` 里首尾相接（**关 9 除外**，它在 `0x1bcaa0` 的另一段）：
  `0x1c21b1 + 40 = 0x1c21d9`，`0x1c21d9 + 153 = 0x1c2272`，…，`0x1c240a + 49 = 0x1c243b`，
  `0x1c243b + 80 = 0x1c248b`，…，`0x1c2737 + 67 = 0x1c277a`，末段收在 `0x1c27ba`
  （关 0–8 与关 10–20 共 **18/18** 个相邻接缝全部吻合）；
* 每个关分支里 `mov qword [rsp+0x38], imm` 的 `imm` 必须等于该关描述长度；
* `level_<n>` 与 `Level <n>` 必须成对出现 21 次。

---

## 10. 本轮踩到的三个坑（都真踩过，写下来省别人时间）

### 坑 1：7 字节的 `Level 0`…`Level 9` **不在字节流里**

`Level 10`…`Level 20` 是 8/9 字节，LLVM 用一条 `movabs` 直接搬，所以在 `.text`
里能看到 `"Level 10"` 这样的 ASCII。
但 `Level 0` 只有 **7** 字节，`movabs` 装不下 8 字节整串，编译器改用两条 4 字节立即数：

```
c7 40 03 65 6c 20 30   mov dword [rax+3], 0x30206c65   ; "el 0"
c7 00 4c 65 76 65      mov dword [rax],   0x6576654c   ; "Leve"
```

⇒ 字节流里**不存在** `"Level 0"`。第一次用 `grep -abo "Level [0-9]"` 只找到 22 个
（全是 10..20），差点得出「前 10 关没有标题」的错误结论。
**正确做法**：按 `c7 40 03 65 6c 20 <digit>` 这条指令形状搜（本文就是这么找到 21/21 的）。
同理，`level_0`…`level_9` 也是用 `0x??5f6c65`（`"el_"` + 数字）拼出来的。

### 坑 2：`player.config` 的字段名搜不到

写者是把 `"face"`/`"hat:"`/`": "`/`"\n"` 逐段用 `movl`/`movw`/`movb` 写进 buffer 的
（§3.2.1），所以 `grep "skin: "` / `grep muted` **全 0 命中**。
不要由此推断「原版没有这几个字段」——用户给的实际文件里就是有。

### 坑 3：`xxd -s <地址>` 用的是**文件偏移**，不是虚地址

`.text` 里 `off = vaddr − 0x1000`。我第一次用 `xxd -s 0x4ad120` 看「关卡数据表」，
看到的是 `vaddr 0x4ae120` 的内容（`level_manager` 的 `Drop` 实现，一堆 `pmovmskb`/`tzcnt`），
差点把 HashMap 的清理循环当成关卡表。`.rodata` 里两者相等，所以只有 `.text` 会中招。

**补充坑（沿用 `AGENTS.md`）**：`funcstarts_ghidra_va.pkl` 在这片区域**缺入口**——
`0x4ad120` 被当成入口（实际在 `0x4ace40` 体内），`0x4ae0d0` 被算进 `0x4ad380`。
本文所有分块都是按**指令形状**（`leaq`/`movabs` 立即数）切出来的，没有依赖那张表。

---

## 11. 对现有文档的影响

| 文档 | 原说法 | 本文结论 |
|---|---|---|
| `docs/108` §7 第 3 条「关卡数据」 | 「**完全未知**」 | 标题/描述/4 模式**已全部解出**；障碍/士兵/目标/Bot 仍未知 |
| `docs/108` §7 第 4 条「关卡解锁」 | 「自定：上一关通关」 | 原版是 `campaign.save` 里的 `level_<n>`（**最高已解锁关**，0 基，单调递增）。可以照这个改 |
| `docs/112` §7.2 | 「`campaign.save` 的使用者归属证不出」 | 归属仍未证，但**功能已证**：`0x418170` 读、`0x4183b0` 写（§3.1） |
| `docs/112` §7.3 | 「21 关数据在哪 **未知**（未查 `.gdc`）」 | 已查完 `.gdc`（无）+ 二进制（**有**，§2） |
| `docs/101` §4.1「8 列 4 行」 | 已被证伪（实为 7+7+7 三行） | 本文不涉及，但请勿再引旧说法 |

## 12. 2026-11-06：关卡生成链纠正 + 21 关参数全解

### 12.0 复现（一条命令）

```bash
# 需要 binutils；纯静态，不跑游戏
GW2_SO=../libgraphwarrust.so ../.venv/bin/python tools/campaign_params.py            # 人读
GW2_SO=../libgraphwarrust.so ../.venv/bin/python tools/campaign_params.py --tsv      # 机器可读
# 可选：和本机 replay 对拍（不读游戏资源，只读存档目录里的 .replay 文本）
../.venv/bin/python tools/campaign_params.py --verify-replays
```

脚本自带断言：21 关 × (障碍预设, `min_count`, 士兵数, 目标数, Bot 难度, 回合上限)
必须全部从字节里解出，否则 `SystemExit`。

### 12.1 调用链纠正：谁调用谁

`level_<n>` 命令分派（`0x499740`，见 §6）不是"一把 match"，而是：

1. **7 字节键** `level_0`…`level_9`：`cmp $7,%rsi` 后逐条 `xor`/`or` 比较
   （`0x49976d`…`0x49983e`），命中后跳到该关的 `start_level` 分支；
2. **8 字节键** `level_10`…`level_20`：`cmp $8,%rsi` 后 11 条 `movabs`+`cmp`
   （`0x499865`…`0x49992d`）；
3. 分支落点（**逐位**，按标题立即数定序，见下表）——
   `0x499740` 这一个函数里**内联了 8 关**（0/10/1/11/2/12/3/13），
   其余 13 关各自是一个独立函数。

**关键纠正**：上一版把 21 个 `call 0x466d20` 的位置当"第 1…21 关"，
**顺序是错的**——它们按"共享代码体"排列，不是按关号。真实映射（用每支里
构造 `"Level <n>"` 的那条 `movl $0x??206c65` / `movabs` 钉死）：

| 关（0 基） | 分支入口 | `"Level <n>"` 构造点 | `call 0x466d20` |
|---|---|---|---|
| 0 | `0x499957` | `0x499b71` (`'0'`) | `0x499db5` |
| 10 | `0x499e1e` | `0x49a038` (`movabs "Level 10"`) | `0x49a389` |
| 1 | `0x49a3f2` | `0x49a60c` (`'1'`) | `0x49a8bb` |
| 11 | `0x49a924` | `0x49ab22` (`"Level 11"`) | `0x49aed2` |
| 2 | `0x49af3c` | `0x49b156` (`'2'`) | `0x49b409` |
| 12 | `0x49b472` | `0x49b694` (`"Level 12"`) | `0x49b9a6` |
| 3 | `0x49bb63` | `0x49bcf9` (`'3'`) | `0x49beee` |
| 13 | `0x49bf41` | `0x49c163` (`"Level 13"`) | `0x49c4b5` |
| 4 | `0x49f510` | `0x49f742` (`'4'`) | `0x49f9f3` |
| 5 | `0x4a04d0` | `0x4a06b8` (`'5'`) | `0x4a09eb` |
| 6 | `0x4a1310` | `0x4a1542` (`'6'`) | `0x4a1835` |
| 7 | `0x4a2430` | `0x4a2662` (`'7'`) | `0x4a2913` |
| 8 | `0x4a33f0` | `0x4a3622` (`'8'`) | `0x4a3940` |
| 9 | `0x4a43a0` | `0x4a4588` (`'9'`) | `0x4a48bb` |
| 14 | `0x4a6980` | `0x4a6b94` (`"Level 14"`) | `0x4a6f59` |
| 15 | `0x4a78f0` | `0x4a7b04` (`"Level 15"`) | `0x4a7ec6` |
| 16 | `0x4a8860` | `0x4a8a92` (`"Level 16"`) | `0x4a8d30` |
| 17 | `0x4a94c0` | `0x4a96f2` (`"Level 17"`) | `0x4a9a3f` |
| 18 | `0x4aa640` | `0x4aa82e` (`"Level 18"`) | `0x4aabcc` |
| 19 | `0x4ab500` | `0x4ab6ee` (`"Level 19"`) | `0x4abaea` |
| 20 | `0x4ac440` | `0x4ac62e` (`"Level 20"`) | `0x4aca2a` |

⇒ 上一版"`0x4aca2a` 是第 21 关"这个**推断**现在成立（它就在 `Level 20` 的函数体里，
`0x4ac440`…`0x4ace3f`，且唯一）。

`call 0x466d20` 的实参（**逐位**，以关 4 为例 `0x49f9da`–`0x49f9f3`）：

```
rdi = &局部结果        # 第 1 参
rsi = 8 字节堆块       # 闭包 A 的 data（逃逸捕获的 &mut 状态）
rdx = 0x95afe8         # 闭包 A 的 vtable（.data.rel.ro）
rcx = 0x20 字节堆块    # 闭包 B 的 data
r8  = 0x95b010         # 闭包 B 的 vtable
```

### 12.2 闭包 A = 障碍生成器：`.data.rel.ro` 的一族 **0x28 字节** 静态记录

每条记录 = Rust 的 `dyn FnMut` **vtable 片段**（逐位，`0x95a350` 起）：

```text
+0x00  u64  0                  闭包 data 为 ZST 时是空指针
+0x08  usize size              逃逸捕获的字节数（8 / 0x18 / 0x20 / 0x30）
+0x10  usize align             恒 8（关 3 的 B 是 ZST：size 0、align 1）
+0x18  fn   thunk              `jmp`/`call` 到 +0x20 那个函数
+0x20  fn   call_mut           **`0x466d20` 真正 `call *` 的就是这一格**
```

A 侧 `+0x20` 一共出现 **12 个不同地址，但只有 6 种行为**（每种行为通常是"thunk + 本体"一对），**这就是"每关障碍怎么来"的全部信息**：

| A 侧 `+0x20` | 含义 | 用它的关（0 基） |
|---|---|---|
| `0x49e570` / `0x49eab0` / `0x4a93e0` | **无障碍**：只 `reset_mask`（`call 0x3f19a0`），不 `place_obstacle` | 0, 1, 16 |
| `0x49f1c0` / `0x49f370` / `0x4a5e80` | **中心单障碍**（`r = u*5+3`，@原点） | 2, 3, 12 |
| `0x49fe80` (esi=**15**) / `0x4af630` | **散布式，`min_count = 15`** | 4, 20 |
| `0x4a12f0` (esi=**5**) / `0x4af6b0` | 散布式，`min_count = 5` | 5, 9, 11, 14 |
| `0x4a2410` (esi=**10**) / `0x4af5b0` | 散布式，`min_count = 10` | 6, 8, 10, 13, 15, 17, 18, 19 |
| `0x4a2da0` (esi=**6**) / `0x4af790` | 散布式，`min_count = 6` | 7 |

> `0x49fe80` 这类 0x20 字节的 thunk 就是 `mov $N,%esi; jmp 0x466c40`
> （`0x49fe8c` 起：`be 0f 00 00 00` = `esi=15`）。`0x466c40` 再
> `call 0x3f1e50`（`0x466c80`，全二进制唯一调用点）。
> **`0x3f1e50` 不是直接每关调一次，而是每关一次 `0x466d20` → 里面最多 5 轮 × `0x3f1e50`。**

`0x466d20` 的循环结构（`0x466d90`–`0x466e15`，逐位）：

```
循环 i = 0..5:
    清空障碍 Vec（0x48 步长；rdi = [r14+0x40], n = [r14+0x48]）
    call *闭包A            ; 生成障碍
    call 0x46c9e0(out)     ; 取一份校验输入
    call 0x46e580(out)     ; 再取一份
    ... 布局校验 ...
    if 校验通过 && i == 5 → 退出
```

闭包 B（`r8` 那一路）是**布局校验谓词**，返回值只有 `bool`：

* 关 3 的 B 是 `0x49f360` = `mov $1,%al; ret`（恒真），其 vtable 是 **ZST**（size 0 / align 1）；
* 其余关的 B 体在 `0x49e650` / `0x49e880` / `0x49eb90` / `0x49fea0` / `0x4a0db0` /
  `0x4a1cd0` / `0x4a2dc0` / `0x4a3df0` / `0x4a4c80` / `0x4a5390` / `0x4a5940` / `0x4a6020` /
  `0x4a63d0` / `0x4a73b0` / `0x4a8320` / `0x4a91b0` / `0x4a9f00` / `0x4aafc0` / `0x4abf00` /
  `0x4ace40`，都调 `0x3c62b0`（`graphwar_plane_pos_manager.rs`，一次 HashMap 查表）
  再拿两个 `.rodata` 常量 `0x19f190` / `0x19f690` 做 `cmpltsd` 比较 ⇒ "是否在合法范围内"。
  **B 的定义判断为"布局校验"是推断**（缺：返回值在 `0x466d20` 里的分支极性，未逐条核）。

### 12.3 每关的数值：`start_level` 分支里的立即数

三件事，都是**立即数**（地址见 `tools/campaign_params.py` 的输出）：

| 事件 | 调用 | 立即数含义 |
|---|---|---|
| **士兵数** | `call *[0x9bbed8]` = `0x476320(mgr, player_id, edx=N)` | **`edx` = 该玩家的士兵数** |
| **目标数** | `call *[0x9bbed8]` 的另一路 = `0x46f310(team_id)` **调 N 次** | **调用次数 N = 目标数**；每个目标建在 `Team2`，名字 `"target"` |
| **回合上限** | `movabs $0x...00000001,%rax` → 写 `[rbx+0xe0]` → 发 `turn_limit_info` | **高 32 位 = 上限**（低 32 位恒 `1`） |
| **Bot 难度** | `bot_added_event`（tag 10）载荷第 3 个 u32 | **`3 / 25 / 50 / 100 / 250`** |

`0x46f310` 逐位确认是"造**目标**"而不只是"造实体"：它在 `0x46f3ad` 拼
`movabs $0x8000000000000017`（= **23 = `target_info_event`**，见 `docs/114`），
在 `0x46f45d`/`0x46f463` 拼出 6 字节串 `"target"`（`movw $0x7465,0x4` + `movl $0x67726174`），
再发 `0x8000000000000038`（= 56 = `entity_effect_info_event`，载荷 `{id, 2}`）。

`bot_added_event` 载荷布局（逐位，`0x4a0882`–`0x4a0898` 与 `0x4a6d62`–`0x4a6d7c` 两处对照）：

```text
+0x00  u64  tag = 0x800000000000000a        (10 = bot_added_event)
+0x08  u32  a   （运行时算出的 player/team id）
+0x0c  u32  b   （运行时算出的 player/team id）
+0x10  u32  c   = 难度值（3/25/50/100/250）
```

`turn_limit_info`（tag 57 = `0x39`）载荷就是那个 8 字节 `(1, N)`。

### 12.4 Bot 难度值 vs `AI.md` 六档表 —— **有一处对不上，不圆**

| 关（0 基） | 静态难度值 | 运行时皮肤 | `AI.md` 里对得上的是 |
|---|---|---|---|
| 5 | **3** | `bot_0` | 代数 3 = 档 1 |
| 9 | **25** | `bot_2` | 代数 25 = 档 3 |
| 11 | **50** | `bot_3` | **对不上**：`AI.md` 的代数/种群/`limit` 三列都没有 50 |
| 14 | **100** | `bot_3` | 代数 100 = 档 4 |
| 15 | **250** | `bot_4` | 代数 250 = 档 5 |
| 18 | **100** | `bot_3` | 代数 100 = 档 4 |
| 19 | **100** | `bot_3` | 代数 100 = 档 4 |
| 20 | **250** | `bot_4` | 代数 250 = 档 5 |

⇒ 7/8 能对上「代数」列，**关 12 的 `50` 对不上任何一列**。
另：`bot_N` 皮肤与难度值**不同向**（`bot_3` 同时出现在 50 和 100），
且 Bot **名字是随机的**（关 16/20 的 40/26 份 replay 里名字覆盖了 9–10 个不同值，
`Alpha Zordon`/`Deep Blue`/`Skynet`/`Multivac`/`Deep Thought`/`HAL`/`Cleverbot`/
`ChatGPT`/`Wolfram`/`Agent Smith` 都出现过），**与 `docs/92` §543 的"名字不能当难度用"一致**。
⇒ 「难度值 = `AI.md` 的代数」判 **推断**，并明确记录 `50` 这个反例。

### 12.5 运行时交叉验证（385 份 replay，21/21 全对）

本机 `~/.local/share/godot/app_userdata/Graphwar/replays/` 有 **385 份** replay
（gzip 的 **JSONL**，事件名是外部标签，见 `docs/96`）。按 `GameNameInfo.game_name`
分组，**21 关全部有样本**（关 1 有 31 份、关 16 有 57 份、关 21 有 26 份…）。

从每份里取"关卡开场段"（到第一次 `FunctionFire` 前），去重后得到：

* 士兵数 = `SoldierInfo` 的**去重 id** 减去 `EntityRemoved`；
* 目标数 = `TargetInfo` 的**去重 id**；
* 障碍数 / 半径 / 坐标 = `AddObstacle`；
* 模式 = `AxisModeInfo` / `FunctionModeInfo` / `TurnModeInfo`；
* 回合上限 = `TurnLimitInfo`；
* Bot = `SkinGraphicsInfo` 的 `bot_N` + `NameInfo`。

**结果与静态解析 21/21 完全一致**（§7.1 的 `R` 列），包括：

* 关 0/1/16 障碍 = **0**；
* 关 2/3/12 障碍 = **1** 且位置恰为 `(0.0, 0.0)`（半径 3.34 / 4.81 / 5.69 / 6.75，全在 `[3,8)`）；
* 关 4 障碍 = 恰好 **15**；关 10/17/19 出现过恰好 **10**（= `min_count` 被钳位）；
* 关 7 回合上限 = **1**；关 13/17/18 目标 = **2**；关 4/9 目标 = **3**；
* `axis` 数字 → 名字的映射 `3/2/1/0 = EveryUnit/EveryFive/OnlyMain/NoAxis` **21/21**；
* 8 个对手关 `TargetInfo` **0 次**，各 1 个 `bot_N`。

> 这是**运行时真值**，与 `S`/`C` 两路静态证据互不依赖 ⇒ §7.1 那些格子可以标"逐位验证"。
> replay 属于用户存档，**不入库**（脚本只在 `--verify-replays` 时读）。

### 12.6 本轮的三个坑

1. **不要把"共享代码体"的地址顺序当成关号**。`0x499740` 里内联了 8 关，
   顺序是 0/10/1/11/2/12/3/13 —— 因为关 `n` 和 `n+10` 只有模式字节不同，LLVM 合并了。
   判关号只能靠分支里构造 `"Level <n>"` 的立即数，或靠 `level_info()` 的顺序。
2. **`module_funcs.tsv` 不能判函数身份**。它是 `src/<file>.rs:<line>` panic 串的启发式归堆，
   75 个地址被归到 ≥2 个模块。`0x466d20` 同时出现在
   `graphwar_level_generator.rs` 与 `graphwar_plane_pos_manager.rs` 就是这个原因。
   判身份只能用**行为证据**（谁调 `place_obstacle` / 谁调 `0x3f1e50`）。
3. **`.got` 槽的 addend 要换算对**。`call *0x51a869(%rip) # 9bb068` 里的 `9bb068` 是槽地址，
   真正的目标是 `analysis/relocs.pkl` 里该槽的 addend（这里是 `0x4739c0`）。
   本轮第一遍手工把 `4649744` 写成 `0x46f050`，差点得出"GOT 指向函数中间"的错误结论。

---

## 附录 A · 用户实测：**尾换行会让进度归零**（2026-11-06）

复现过程（用户在真游戏上做的）：

1. 通关一关后，游戏自己写的文件是 `level_0`（**8 字节，末尾没有换行**）；
2. 用户用 `echo 'level_20' > campaign.save` 写进去（**9 字节，多一个 `\n`**）；
3. 进游戏 → 已通关进度显示 **0**。

**代码侧的对应证据**：读路径是「整文件读成 String → `split('_').last()` → `parse::<u32>()`」
（`0x417ef0`）。Rust 的 `str::parse::<u32>()` **不会跳过空白**，所以 `"20\n"` 解析失败 ⇒ 取默认 0。

⇒ 这条把上一节"格式"从"看起来是 `level_<n>`"升级成**逐位要求**：

> `campaign.save` 必须**恰好**是 `level_<n>`，**行尾不能有任何字节**（没有 `\n`，没有空格）。

我们的实现如果用 `writeln!` / `println!` 写这个文件，就会写出 9 字节而原版是 8 字节——
即使游戏逻辑全对，这也是一个可观测的行为差异。

顺带一个**用户观察 + 代码吻合**的点：打完**第 1 关**写的是 `level_0`，不是 `level_1`
⇒ 存的是"最后通关的那一关"（0 基下标），配合写路径的**单调**（只有新值 > 旧值才落盘）
与 `cmp ebp, 0x14`（下标上限 20），"共 21 关"这件事现在有三处独立证据。
