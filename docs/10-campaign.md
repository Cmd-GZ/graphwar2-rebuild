# 10 · 战役 21 关、关卡规则与队伍/颜色

> **本篇解决什么问题**：21 关的数据到底存在哪、`campaign.save` 是什么格式、每关的
> 障碍/士兵/靶子/Bot/回合上限是多少、胜负怎么判、"打不全就复原"到底是什么形状、
> 队伍与玩家颜色怎么分配、关卡界面（PlayScreen / JoinLevelPopUp）对齐到什么程度。
>
> **本篇合并自**：

| 原文件 | 内容 | 归档 |
|---|---|---|
| `118-campaign-data.md` | 关卡数据位置、三个存档格式、21 关全表、解锁、生成链 | [`archive/118-campaign-data.md`](archive/118-campaign-data.md) |
| `131-level-rules.md` | 胜负判定与"打不全就复原" | [`archive/131-level-rules.md`](archive/131-level-rules.md) |
| `102-color-assignment.md` | 玩家颜色分配（两张 6 色队色表 + 随机起点线性探测） | [`archive/102-color-assignment.md`](archive/102-color-assignment.md) |
| `105-team-sides.md` | 队伍固定在 x 两侧（`flip` 由队伍决定） | [`archive/105-team-sides.md`](archive/105-team-sides.md) |
| `125-playscreen.md` | PlayScreen / JoinLevelPopUp 的原版对齐 | [`archive/125-playscreen.md`](archive/125-playscreen.md) |
| `34-color-assignment.md` | **已被 102 取代**（只到"规则未定"） | [`archive/34-color-assignment.md`](archive/34-color-assignment.md) |

证据标记沿用四档（定义见 [`06-verification.md`](06-verification.md) §1）。
关卡生成的**算法**在 [`11-levelgen.md`](11-levelgen.md)；随机源在 [`12-rng.md`](12-rng.md)；
障碍在客户端的绘制与命中在 [`16-client.md`](16-client.md)。

## 1. 关卡数据在哪（结论：在二进制里）

### 一页结论（关卡数据在哪）

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

#### 正面证据：`level_info` 是一个 21 分支的 `match`，分支体在 `.text` 里

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


## 2. 三个存档文件的格式

#### `user://campaign.save`

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

##### 写的是 `store_string`（不是 `store_var`）

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

##### 读的是「整文件进 `String`」，再 `rsplit('_').parse::<u32>()`

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

##### 写路径是「只增不减」

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
节点访问器到底写了哪个属性；`02-module-map.md` §7.1 已判它是「Godot 节点取值 + 委托」）。

#### `user://player.config`

| 项 | 值 | 等级 |
|---|---|---|
| 路径常量 | `.rodata 0x1bfe12`，长度 `0x14` = 20 = `"user://player.config"` | **逐位验证** |
| 写 | `0x417560`（`open(path, 20, mode=2)` 于 `0x417582`） | **观测** |
| 读 | `0x4166b0`（`open(path, 20, mode=1)` 于 `0x4166c4`；`0x416911` 用长度 **1** 的 `":"` 做分隔） | **观测** |
| 格式 | **每行 `key: value`，无 `[section]` 头** ⇒ **不是** Godot `ConfigFile`；是手写的「按 `:` 切」 | **观测** |
| 字段顺序 | 写者按固定顺序拼：`name`, `skin`(?), `face`, `hat`, `muted`(?) 之一一对应 | **推断**（§3.2.1） |

##### 字段名是**逐字符写进 buffer 的**，`strings` 搜不到

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
`FaceGraphicsInfo`/`HatGraphicsInfo` + 静音开关一一对应，与 `17-resources.md` 的
`AudioServer.set_bus_mute` 对得上）。

**读法**（从 `0x4166b0` 的形状推断，缺的是逐字段的 `match` 分支，未逐条核）：
按 `\n` 切行 → 每行按第一个 `:` 切成 key/value → `match` 到字段。
**这一条判推断**：我只逐位确认了「分隔符是 `:`、长度 1」和「写者会写 `\n`」，
没有把 5 个 `match` 分支全部对到字节。

#### `user://local_ip.config`

| 项 | 值 | 等级 |
|---|---|---|
| 路径常量 | `.rodata 0x1c2dce`，`"user://local_ip.config"`（22 字节） | **逐位验证** |
| 使用者 | `0x4d3180`（`0x4d5d62` / `0x4d5efc` / `0x4d609e` 三处引用路径；`0x4d60ec` 引用紧跟其后的字面量 `"1"` @`0x1c2de4`） | **观测** |
| 内容 | `1`（单行一个数字）——与「紧跟路径的那个 `"1"` 字面量」一致 | **观测（弱）** |
| 语义 | **未知**（LAN 广播里选第几块网卡 / 是否启用，都没证），只确认它是 `0x4d3180` 读写的 | **未知** |

---


## 3. 21 关全表：标题 / 描述 / 四个模式

#### 标题

**21/21 都是 `Level 0` … `Level 20`**，逐字节精确（0..9 与 10..20 两种编码，见 §2.2 注）。
没有花名、没有副标题。

#### 描述（逐字节精确，共 21 段，`.rodata` 里首尾相接）

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

#### 四个模式（每关 4 字节，`+0x38..+0x3b`）

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


## 4. 21 关全参数表（权威表）

#### 主表

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
> （`10-campaign.md` §4.3 原先只说"3 最密是推断"，现在升为 **逐位验证 + 运行时**。）

#### 障碍的确切公式（障碍数不是常数，是随机变量的截断）

散布式（`0x3f1e50`，逐位复现见 `11-levelgen.md`）：

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

#### 与描述的交叉验证（21/21 全对，**没有一处对不上**）

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


## 5. 解锁规则

#### `16-client.md` §7 第 4 条（我们自定的「上一关通关解锁」）现在可以改了

原版的行为是：**`user://campaign.save` 里就是一个 `level_<n>`，代表「到第 n 关为止都解锁」**。
没有「每关通关次数」「星级」「最高分」——文件里连第二个字段都没有。

#### 关卡总数 21 的三处独立证据

| 证据 | 内容 | 等级 |
|---|---|---|
| `CampaingLevelsButtons.gdc` | `for i in 21:` 造 21 个按钮，`input_text = "level_" + str(i)` | **逐位验证**（`20-source-recovery.md` §4.1） |
| `level_info()` 的 21 个分支 | §2.2，21 组 `level_<n>`/`Level <n>`/描述/模式 | **逐位验证** |
| `0x418820` 内 | `cmp ebp, 0x14`（0x14 = 20 = 最后一个下标）+ `cmove` 钳位 | **观测** |

#### 「`level_n` = 解锁到 n」是**推断**，缺什么写清楚

已证：文件只存这一个数；写入是单调的（只增大）；写完后用 `n+1` 去调按钮刷新函数。
**未证**：`0x418820` 到底把按钮改成什么样（`disabled=false`？换图？），
以及「解锁」是「可以点」还是「可以看描述」。
⇒ 缺的证据：`0x418820` 里那几个 Godot 节点访问器（`0x3ab390` / `0x4068f0` / `0x407630`）
落到哪个属性。**这一条留作推断**。

---


## 6. 关卡生成链：谁调用谁、参数从哪来

#### 调用链纠正：谁调用谁

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

#### Bot 难度值 vs `AI.md` 六档表 —— **有一处对不上，不圆**

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
`ChatGPT`/`Wolfram`/`Agent Smith` 都出现过），**与 `16-client.md` §543 的"名字不能当难度用"一致**。
⇒ 「难度值 = `AI.md` 的代数」判 **推断**，并明确记录 `50` 这个反例。

#### 运行时交叉验证（385 份 replay，21/21 全对）

本机 `~/.local/share/godot/app_userdata/Graphwar/replays/` 有 **385 份** replay
（gzip 的 **JSONL**，事件名是外部标签，见 `07-protocol.md`）。按 `GameNameInfo.game_name`
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


## 7. 仍未确定清单

### 仍未确定清单（2026-11-06 复核后）

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


## 8. 附录：用户实测——尾换行会让进度归零

### 附录 A · 用户实测：**尾换行会让进度归零**（2026-11-06）

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


## 9. 胜负判定：按关卡类型分开（原 131）

### 一页结论（胜负判定）

| 项 | 结果 | 档位 |
|---|---|---|
| 根因 | `_check_match_over()` **只数两队士兵存活数**（`alive = {0:n, 1:m}`）。打靶关**没有敌方士兵** ⇒ `alive[1] == 0` ⇒ 第一发一落地就判人类胜。**射击跟胜负无关，是存活统计误判** | 观测（旧代码 + 用户现象） |
| 靶子参与判定 | 是。靶子与士兵塞进**同一个** `hit_test` 第 3 参数（`[x,y,r]*n`），内核把它变成 `graphwar_shot_calculator::settle::Target{id: 下标}` —— **没有任何"士兵"语义过滤** | 逐位验证（`src/_scaffold/api.rs:266`/`:284`、`src/graphwar_shot_calculator/settle.rs:73`）+ 观测（端到端打中） |
| 打靶关胜利 | **全部靶子被摧毁** | 推断转观测：等价于"`Team2` 全灭"（`15-target-life.md` §2.1/§4.5，45/45 回放）；本文端到端实测 |
| 打靶关失败 | **回合上限用尽仍未清完**（`turn_limit > 0` 时） | **推断**（`15-target-life.md` §5.3 #1：语料 0 样本）+ 端到端实测 |
| 对战关胜负 | 对方队伍全灭 ⇒ 胜；我方全灭 ⇒ 负 | 观测（`15-target-life.md` §1 / `15-target-life.md` §4.5：45/45） |
| 回合上限 | **真的算**，按"结算一发 = 一个回合"计 | 逐位 + 运行时（`10-campaign.md` §7.1 的 `turn_limit` 列，`Level 7` = 1）+ 推断（"用尽 ⇒ 判负"） |
| 空放/走火 | **不判胜**；只消耗一个回合（所以 `Level 7`（上限 1）空放一发就是失败） | 观测（本文 §5.2 ①④） |
| 打不全就复原 | 实现了，标 **推断（依据用户实测 + `15-target-life.md` 的形状）**；**只翻 alive**、不重建实体、不碰掩膜 | 推断（形状部分逐位/观测，判定点**未知**） |
| 新文件 | `client/src/game/target_set.gd`（138 行）、`client/src/game/level_rules.gd`（183 行） | — |
| 改动 | `client/src/screens/play_screen.gd` **+231/−26**；`client/tools/selftest.gd` **+273/−0**（只新增断言） | — |
| `bash client/run.sh selftest` | **191 → 242 passed, 0 failed**（+51，**未改/未删任何既有断言**；191 = 已合入 `origin/main` 的基线） | — |
| `cargo test --offline` | **296 passed**（本任务没动 Rust；条数不变） | — |
| 回归反证 | 把 `_check_match_over()` 换回旧实现 ⇒ 新增断言里 **9 条失败**（§5.3 原文照录） | 观测 |
| 仍未定 | 重置的**判定点**、"全部复原"还是"只复原本发打掉的"、是否重置回合计数、视觉效果、`Level 3` 这种**没有写 "single shot"** 的多目标关是否也适用同一规则、靶子半径/坐标、同时回合下双方同时清场的判负 | 见 §6 |

一句话：**"发射一次就通关"是存活统计的误判，不是射击被当成胜利。**
现在靶子进了 `hit_test`、胜负按 `10-campaign.md` §7.1 的关卡类型分开判、回合上限真的算、
打不全就复原（推断）；四个端到端场景都走真实开火路径验过。

---

### 根因：存活统计把打靶关判成了"对方全灭"

#### 旧实现（`play_screen.gd`，改动前）

```gdscript
func _check_match_over() -> void:
	var alive := {0: 0, 1: 0}
	for s in _soldiers:
		if bool(s.alive):
			alive[int(s.team)] = int(alive[int(s.team)]) + 1
	if int(alive[0]) > 0 and int(alive[1]) > 0:
		return
	# 任一队为 0 ⇒ 结束；winner = 0 if alive[0] > 0 else 1
	_running = false
	app_node.report_match_result(winner == human_team, winner)
```

而靶子当时**只用于绘制**：`_plane.set_targets(_target_positions(...))`（`GwPlaneView` 画
`skins/target.svg`），命中判定里构造的实体数组**只来自士兵**
（`s.pos` + `GwCore.SOLDIER_HIT_RADIUS`）——靶子根本没进 `hit_test`。

#### 为什么打靶关一定误判（`10-campaign.md` §7.1 的 13 个目标关）

打靶关只有**一个玩家、没有 Bot**（`bot_tier = -1`）⇒ `_soldiers` 里**没有一个 team 1 的士兵**
⇒ `alive[1] == 0` 恒成立 ⇒ 第二轮循环条件 `alive[0] > 0 and alive[1] > 0` 为假
⇒ `_check_match_over()` **第一发就判结束**，且 `winner = 0`（人类队）⇒ `human_won = true`
⇒ `report_match_result(true, 0)` ⇒ `store.mark_completed(level_index)` ⇒ **通关**。

⇒ 三个现象一次解释完：**能不能打中靶子、打了几个，全都不影响结果。**

#### 旧代码在同时回合制下也不会"立刻"结束（澄清一条容易搞错的）

同时回合制下，不是"所有人打完"那一发只会**入队**、不会调 `_resolve_shots`，所以击杀还没落地；
旧代码随后调 `_check_match_over()` 时看到的仍是旧的 alive 表。这个分支在打靶关上不成立
（13 个打靶关**全部是顺序回合**，见 §2.2），所以上面那条误判覆盖了全部打靶关。

---

### 胜负条件：按关卡类型分开

#### 规则表

| 关卡类型 | 判据 | 胜利 | 失败 |
|---|---|---|---|
| **打靶关**（`targets > 0`、`bot == 0`；13 关 = `Level 0/1/2/3/4/6/7/8/10/12/13/16/17`，标题号即 0 基 index） | `GwLevelRules.for_campaign()` | **全部靶子被摧毁** | 回合上限用尽仍未清完（`turn_limit > 0`） |
| **对战关**（`bot > 0`、`targets == 0`；8 关 = `Level 5/9/11/14/15/18/19/20`） | 同上 | **对方队伍全灭** | 我方队伍全灭 |
| 两者都有（`targets > 0` 且 `bot > 0`） | 同上（`Kind.BOTH`） | 目标全部达成 | 任一失败条件 |

依据：

* **目标数 / Bot 数 / 回合上限** 三列来自 `10-campaign.md` §7.1（`S` 立即数 + `C` 闭包预设 + `R`
  385 份回放，21/21 对齐）；本实现直接读 `GwCampaign.LEVELS`（同一个数据源，`campaign.gd` 文件头有出处表）。
* **对战关 = 对方全灭**：`15-target-life.md` §1 与 `15-target-life.md` §4.5 —— 45/45 份出现 `EndingGame` 的回放里，
  胜方都是"某队全灭"，`Team2` 存活实体为 0。**观测**。
* **打靶关 = 靶子全清**：`15-target-life.md` §2.1 证明靶子就是 `Team2` 里的普通实体（`0x46f310` 里发
  `TeamInfo{Team2}`，逐位验证），§4.5 又证明 45/45 的结束都是 `Team2` 全灭 ⇒
  **"靶子全清"与"Team2 全灭"在原版里是同一件事**。**观测 + 逐位验证**。
  我们的客户端没有给靶子分配队伍（靶子只在 `GwPlaneView` 里画），所以这里显式写成
  "目标全部达成"，语义等价。
* **回合上限用尽 ⇒ 失败**：`10-campaign.md` §7.1 的 `turn_limit` 是真值（`Level 7` = 1，描述
  "Destroy your target in your first try"），但**用尽之后的判决语料里没有样本**
  （`15-target-life.md` §5.3 #1：唯一上限为 1 的样本首发就命中）⇒ **推断**。

#### 为什么 `Level 7` 的"1 回合"不会误伤

`turn_limit` 只在**打靶关**生效（判定里 `has_targets()` 才看它）。21 关里有上限的 **11** 关
**全是打靶关**（`Level 1/2/3/4/6/7/8/10/12/13/17`），有上限的对战关为 **0** 个；
而 13 个打靶关**全部是顺序回合**（`turn=0`），
所以"一个回合"没有歧义（`15-target-life.md` §5.1 #3 的结论在这里扩到 13/13）。

#### 判定顺序与 `reason` 码（`GwLevelRules.evaluate()`）

```
① 对战：我方全灭            → over, human_won=false, reason="own_wiped"
② 打靶：靶子全部被摧毁      → over, human_won=true,  reason="targets_cleared"
③ 对战：对方全灭            → over, human_won=true,  reason="enemy_wiped"
   ③' 同一刻两边都空        → over, human_won=false, reason="mutual_wipe"   ← 未观测，见 §6
④ 打靶：turn_limit>0 且 已用回合 >= 上限 → over, human_won=false, reason="turn_limit"
⑤ 其余                    → not over, reason="playing"
   连"双方都不在场"也判不了 → not over, reason="no_verdict"（旧代码在这里会随便判一个人赢）
```

"失败优先"是**我们的选择**（`BOTH` 类型才可能同时命中 ①②③）——21 关数据里**没有**
同时有靶子和 Bot 的关卡（`10-campaign.md` §7.1），所以这条分支目前是**不可达**的，只按规则表定义好。

#### 打靶关失败时"胜方"是谁

`report_match_result(human_won, winner_team)` 的签名**没有改**（`app.gd` 那段另一位 agent 在动）。
打靶关没有别的玩家队，所以 `winner_team` 取 `1 - human_team` = **1 = Team2**。
这**不是凑数**：靶子在原版里就属于 `Team2`（`15-target-life.md` §2.1），所以"关卡没打完 ⇒ Team 2 wins"
与原版语义一致，结算框的文案也就对得上。

#### 我们对旧实现的**有意**行为变更

| 场景 | 旧 | 新 | 理由 |
|---|---|---|---|
| 打靶关空放一发 | 判胜、通关 | 不判胜，只消耗一个回合 | §1 的根因 |
| 打靶关打掉部分靶子 | 判胜、通关 | 复原 + 继续 | §4 |
| 打靶关用尽回合上限 | 不可能发生（早就"胜"了） | 判负 | §2.1 |
| 一队都没有（例如回放模式没有玩家表） | `0 == 0` ⇒ 判人类胜 | `no_verdict`，不判 | 旧行为是纯粹的假阳性 |
| 只有一个队伍、也没靶子（UI 里不可达） | 判人类胜 | `no_verdict` | 同上 |

---

### "打不全就全部复原"（**推断**）

#### 形状部分（有证据）

| 结论 | 依据 | 档位 |
|---|---|---|
| 复原**不是**重建实体（不新分配 id、不发 `TargetInfo`、不发 `EntityRemoved`） | 造目标的 `0x46f310` **只有 13 个调用点，全在关卡开场**；109 份含目标的回放里目标**从未**被 `EntityRemoved`（`15-target-life.md` §2.2、§4.3 ③） | 逐位验证 + 观测 |
| 复原只能表现为 **`LifeInfo{alive:true}`** | 目标只有 `TargetInfo`/`LifeInfo` 两种状态事件；分派器 `0x3e2830` 的 70 个分支里没有一处构造 `life_info`（`15-target-life.md` §3.4） | 逐位验证 + 推断 |
| 复原**不碰掩膜**（障碍的洞留着） | 掩膜整局只建一次、`ClearObstacles` 397 份里从不中途出现（`15-target-life.md` §3.2 问题 5） | 观测 |
| 目标整局**不移动、不重生、不计时** | `PosInfo` 只在开场与开局快照出现（`15-target-life.md` §5.3 #7） | 观测 |

#### 判定点与条件（**推断，且必须当作推断读**）

```gdscript
# GwTargetSet.settle_shot(destroyed_this_shot)
# 本发一个都没打掉            → 不复原（走火/空放什么都不发生）
# 本发把靶子清空              → 不复原（全部达成）
# 否则                        → 复原**全部已摧毁的靶子**
```

* **判据**："本发摧毁的集合 ≠ 全部靶子 ⇒ 全部复原"。等级 **推断**：来源于
  **用户实测**（"一发没打全 ⇒ 已摧毁的目标全部复原"）与 `15-target-life.md` §3.2 问题 2 的
  最强推断（"等价说法：结算后 `Team2` 仍有存活 ⇒ 全部复原"）。
* **判定时刻**：放在**一次射击结算结束时**（`_settle_round()` 里、判胜负之前）。
  `15-target-life.md` §3.2 问题 1 明确这是**三种候选之一**（射击结算 / `end_turn` / 下次开火前），
  **二进制里没找到判定点**，语料里 **397 份回放 0 次部分命中**（`15-target-life.md` §4.3 ①）
  ⇒ **没有真值可以对**。
* **为什么不写进文档当结论**：`AGENTS.md` 硬规矩 1。本文与代码注释都把它标成
  `推断（依据用户实测 + 15-target-life.md 的形状结论）`。

#### 回合粒度与"顺序"这两条口径（也是推断）

* **一个回合 = 一次射击结算**。战役关 `TimeModeInfo = TimerInf`（`15-target-life.md` §5.1 #9），
  **没有超时路径**，所以一个回合只能由"打完一发"推进；LAN 的 3 分钟超时不计回合
  （那些对局 `turn_limit` 恒为 0）。
* **同时回合制下，一轮 = 一个回合**：`_resolve_shots()` 把整批弹道一起结算，
  所以"打不全就复原"与"判胜负"都作用在**整批**上。13 个打靶关都是顺序回合，
  所以这条口径目前不影响任何一关（`15-target-life.md` §5.1 #3）。
* **复原事件与回合边界的先后未证实**：语料的回合边界块（`TurnCountInfo` 等四条）里
  **从来不含 Life/Target 事件**（`15-target-life.md` §4.4），所以"复原插在哪"没有样本。
  我们的客户端没有协议层，`_settle_round()` 的顺序是"先复原、再判胜负"，仅此而已。

#### 一个**没有采用**的替代方案

`15-target-life.md` §6.2 建议过逐关布尔量 `all_in_one_shot`（只有 `Level 6/8/10/13/17` 为真，
即描述里写了 "in a single shot" 的 5 关）。本文**没有**采用，而是**对所有打靶关通用**，
理由与代价都写在 §6 第 5 条：

* 用户口述是**一条通用规则**，父 agent 也按通用规则下令；
* "N 个靶子 + 有限回合"的关卡如果允许分次打完，`Level 3` 的 10 回合、
  `Level 9` 的 6 回合就都变成了宽松的计时器；
* **代价**：`Level 3`（3 个靶子、10 回合，描述里**没写** single shot）从"分次能打完"变成
  "必须一发全清"——13 个打靶关里**只有它**同时满足"多目标"与"描述没写 single shot"，
  而这一条**与原版是否一致是未知的**（0 样本）。

⇒ 实现上只有**一个**决策点（`GwTargetSet.settle_shot()`），要改成逐关开关只需给它多加
一个 `bool` 参数，不改别处。

---

### 仍未确定

| # | 问题 | 状态 | 缺什么 |
|---|---|---|---|
| 1 | 复原的**判定点**（射击结算 / `end_turn` / 下次开火前） | **未知**（本文取"射击结算结束时"） | 二进制里的判定点，或一份**部分命中**的回放（语料 0/397） |
| 2 | "全部复原"还是"只复原本发打掉的" | **未知**（实现取"全部已摧毁的"，单发场景下两者等价） | 跨回合累积的样本 |
| 3 | 复原会不会**重置回合计数** | **未知**（实现：不重置） | `15-target-life.md` §5.3 #3，同上缺样本 |
| 4 | 复原有没有**视觉效果**（火花） | **未知**（实现：无） | 同上 |
| 5 | `Level 3`（3 靶、10 回合，描述里**没写** "in a single shot"）是否也适用"打不全就复原" | **未知** ⇒ 本实现按**通用规则** | 该关的一次部分命中回放 |
| 6 | 靶子的**致死半径** | **推断 0.5**（`15-target-life.md` 证明半径是数据字段，靶子值未观测） | 回放里的 `PosInfo.radius`（靶子的） |
| 7 | 靶子的**坐标**分布 | 我们的自定值（`_target_positions()`，右侧半区） | `10-campaign.md` §8 #16：原版随机，本来就没有固定坐标 |
| 8 | 同时回合制下**双方同时把对方清空**怎么判 | **未观测**（实现：判人类负，`mutual_wipe`） | `15-target-life.md` §5.3 #5 同类 |
| 9 | 对手会不会开火 | **不会**（客户端没有 AI 开火循环，既有缺口，不是本次引入） | AI/服务端接线（另一位 agent 的线） |
| 10 | `BOTH` 类型（既有靶子又有 Bot） | **21 关数据里不存在**，规则按 §2.1 的表格实现 | 自定义关/服务端下发的关卡 |
| 11 | 打靶关失败时结算框的文案（"Team 2 wins"） | 语义上对（靶子属于 Team2），但**没有原版真帧** | 一次"回合用尽"的原版录像 |
| 12 | 回放模式（没有玩家表）现在的行为 | 从"第一发判人类胜"改成 `no_verdict`（不判） | 回放播放器整体未做（`00-status.md` §4.2 第 1 项） |

---


## 10. 队伍与玩家颜色

### 一句话

颜色**不是**按玩家顺序取的调色板，也**不是** id 哈希，更**不是**每局随机 RGB：
游戏里有两张**固定的 6 色队色表**（Team1 暖色 / Team2 冷色），
`ColorInfo` 在**加入队伍**和**换队**时发出：从**本队那张表**里
**随机挑一个起点**，然后沿表**线性向前探测**，取第一个**当前没人用**的颜色；
6 个全被占时**兜底生成** `(r8()%180, r8()%180, r8()%180)` 的随机色。
随机数是 `rand_chacha` 的**全局**流（`0x47f700`，OS 熵播种，见 `12-rng.md`/`12-rng.md`）。

---

### 结论：可直接照抄的规则

#### 两张表（顺序就是内存里的顺序，已逐条从指令解出）

| 队 | key | 表（索引 0 → 5） | 观感 |
|---|---|---|---|
| **Team1** | `0` | `(186,42,32) (238,114,0) (125,104,0) (122,160,0) (149,5,62) (234,0,219)` | 红/橙/黄/品红 = 暖 |
| **Team2** | `1` | `(107,0,234) (17,0,108) (21,83,221) (0,203,163) (0,116,26) (2,85,63)` | 蓝/青/绿 = 冷 |

两表**不相交**（12 个值两两不同）。这就是为什么全语料里"同时活着的玩家颜色互不相同"
永远成立——它是两条池子各管各的**副产品**，不是额外的全局约束。

#### 分配算法（汇编 `vaddr 0x464d10`，逐句对应见 §3.3）

```rust
fn assign(manager, entity_id: u32, team_key: u8) {      // team_key = 0 → Team1, 1 → Team2
    let pool = manager.pools[team_key];                 // 长度恒为 6
    let n = pool.len();                                  // = 6
    let base = next_u64() % n;                           // ← 全局 ChaCha 流（0x47f700）
    for k in 0..n {
        let c = pool[(base + k) % n];                    // 沿表向前探测
        if !manager.used.contains_colour(c) {            // used = 当前"实体 → 颜色"表
            manager.used.insert(entity_id, c);
            send(Event::ColorInfo { entity_id, color: c });   // 事件 tag 0x800000000000003d
            return;
        }
    }
    // 6 个全被占（即本队已有 ≥6 人）——兜底：不是回绕、不是换色相，是随机 RGB
    let c = (next_u8() % 180, next_u8() % 180, next_u8() % 180);
    manager.used.insert(entity_id, c);
    send(Event::ColorInfo { entity_id, color: c });
}
```

三个要点：

1. **起点是随机的**（`next_u64() % 6`），不是"下一个空闲索引"、不是"玩家序号"；
2. **探测是 +1 循环**，所以"第二个进队的人"拿到的颜色分布**不是均匀的**
   （比起点晚 1 位的那个颜色概率翻倍）——这一条在 replay 里被 86 个样本证实，见 §4.6；
3. **`used` 里已有的颜色会被跳过**（同一颜色的活人不会出现两次）。

#### 什么时候发（= 什么时候重分配）

| 触发 | 行为 | 证据 |
|---|---|---|
| 玩家**加入**队伍（新连接 / 加 bot） | 按 §1.2 分配，发 `ColorInfo` | 二进制：调用点 `0x473b4c`（§3.6） |
| 玩家**换队** | **重新分配**（用**新队**的表；旧色自动退回池子，因为 `used` 是 `实体 → 颜色` 表，给同一实体写新值就顶掉旧值） | 二进制：调用点 `0x473f29` + 「取反队伍」逻辑（§3.6）；replay：55 次换色**全部**伴随换队，0 次例外（§4.5） |
| 玩家**离开**（实体删除） | 颜色**释放**，可供他人再拿 | 二进制：`entity_removed_event` → `0x465570`（§3.4、§3.5）；replay：3 次"释放后被人捡走"（§4.5） |
| 有新人连进来（`new_connection_event`） | **把所有人的颜色重发一遍**（不重新分配） | 二进制：`new_connection_event` → `0x465460` 遍历 `used` 全表逐条发 `ColorInfo`（§3.5）；replay：同一实体整局颜色不变，只在换队时变 |
| 重连 | 新连接 → 新玩家实体 → 走"加入"路径，**拿新颜色**（不保留旧的） | 二进制 + 推论，见 §5.4 |

**颜色是绑在"玩家实体 id"上的属性**，一旦发出去就存住；跨局不重算
（同一 session 的连续几局里，同一实体颜色不变，见 §4.5）。

#### RNG

用的是**全局那一支 ChaCha 流**：`0x47f700`（`next_u64`，`12-rng.md` §1 已 hook 过它：
惰性 OS 熵播种、48 字节 getrandom、32 字节 key）。同一条流也是关卡生成
（`53`）与回合顺序（`47`，原 `59` 已并入）的来源。
⇒ **颜色不可由种子复现**，而且**它消耗随机数**，会扰动同一进程里后续的
关卡/回合顺序序列。这一点对"replay 逐位复现"是有影响的（见 §7）。

---

### 规则

**我方（Team1）永远在 `x < 0` 侧，敌方（Team2）永远在 `x > 0` 侧。**
由此开火方向也固定：

| 队伍 | 所在半场 | 开火方向 | `flip` |
|---|---|---|---|
| Team1 | `x ≤ 0` | 向右（+x） | `false` |
| Team2 | `x ≥ 0` | 向左（−x） | `true` |


## 11. 关卡界面：PlayScreen / JoinLevelPopUp 的原版对齐（原 125）

### 一页结论（关卡界面）

| 用户列的差异 | 修法 | 状态 |
|---|---|---|
| 棋盘深灰黑底、网格几乎看不见 | 改成**原版真配色**：白底 `#FFFFFF`、细网格 `#E6E6E6`、粗网格 `#B2B2B2`、主轴 `#333333`、外框纯黑 3px | ✅ 逐像素（§2） |
| 顶部工具条完全没有 | 补 `ExtraOptionsHBox` 的 4 个开关（原版图标 + 原版判据显隐 + 原版居中/尺寸），默认可见 | ✅ 逐项（§3.2、§4） |
| `f(x) =` 是个小框；没有修正常数位 | 底栏按原版比例重排：`f(x)=` 图标 + **横跨的输入框**（75% 屏宽）+ 右下角修正常数（`+ 4.638` / `- 8.315` 形状） | ✅ 逐项（§3.3） |
| 士兵是小色点 + 白字 | 三层精灵（皮肤/脸/帽）+ **玩家颜色名字 + 黑描边**；玩家颜色按 `10-campaign.md` 的 12 色表分配 | ✅（§3.4） |
| 左下 `00:57` 计时（原版没看到） | 战役 21 关 `time` 全 = 5 = **∞** ⇒ 倒计时**不显示字符**（但保留原版的 140 px 占位宽） | ✅ 观测（§3.5） |
| 底部说明文字 `战役第 0 关（占位…）` | **删除**（节点保留但 `visible=false`，只给 selftest 当诊断用） | ✅ |
| 键盘/函数面板深灰底、图标自绘 | 已是原版图标与 `#1A3B2D` 主题；本轮把按钮 **content_margin 从 12 改成 3**（原版没有该字段），键盘列宽因此与原版对上 | ✅（§3.1） |
| 弹窗占位文字 | 21 关**标题 + 描述原文 + 四个模式初值**全部换成真数据 | ✅ 逐位（§5） |
| 弹窗底部 4 个小图标 | 换成原版 `axis_option_N` / `fx_option` / `turns_sequential` / `turn_inf`，并做到 **228×228 @1920**（原版 228） | ✅ 观测（§5.2） |
| Cancel/Start 圆角大按钮 | 尺寸/字号按原版：194×73 / 218×73 @1920（我们 197×78 / 220×78） | ✅ 观测（§5.2） |

**一句话**：颜色、几何、关卡数据现在都有**可指认的来源**（原版真帧像素 / 原版 `.gd` 的比例公式 /
`.rodata` 的字节），不再有"目测取色"或"占位数据"。

---

### 棋盘配色：全部逐像素取自原版真帧

#### 取到的值

| 元素 | 颜色 | 线宽 | 取样（`orig_campaign_play.png`） |
|---|---|---|---|
| 棋盘底色 | **`#FFFFFF`** | 填充 | 棋盘矩形内 215 897 个采样点是纯白 |
| 细网格（每 1 格） | **`#E6E6E6`** = (230,230,230) | 1 canvas px | 行 `y=210`：x=325/351/376/402/453/478/504… |
| 粗网格（每 5 格） | **`#B2B2B2`** = (178,178,178) | 2 canvas px | 行 `y=210`：x=427,428 / 555 / 683 / 811（间隔 128） |
| 主坐标轴（x=0/y=0） | **`#333333`** = (51,51,51) | 2 canvas px | 列 `x=939` 整列；行 `y=578,579` 两行 |
| 棋盘外框 | **`#000000`** | 3 canvas px | `x=300..302`、`y=195..197`、右 `1576..1578` |

代码在 `client/src/play/plane_view.gd` 顶部，**逐条写明来源**：
`COL_BOARD / COL_GRID_MINOR / COL_GRID_MAJOR / COL_AXIS / COL_BORDER`。

#### `08-plane.md` 的坐标换算仍然成立

| 项 | `08-plane.md` §4 记的 | 本轮实测 | 结论 |
|---|---|---|---|
| 画布 | 1000 × 600 px | 棋盘控件 1279×767（= 1.279 倍），网格 50×30 格 | ✔ |
| 比例 | 20 px / 单位 | 1279 / 50 = **25.58 px/单位**；细线间隔实测 25–26 | ✔（同比例放大） |
| 中心 | (500, 300) | y 轴 x=939 ≈ 300+639.5；x 轴 y=578.5 ≈ 196+383 | ✔ |
| 网格 | 50×30 | 细线每 1 单位、粗线每 **5** 单位（427→555 = 128 = 5×25.58） | ✔（新增：粗线周期 = 5） |

⇒ `px = 20x + 500`、`py = 20y + 300` **仍然成立**，只是画布被等比放大到控件尺寸；
线宽用"画布像素"表示 ⇒ `1 canvas px = _scale()/20`，在任何窗口尺寸下与原版同比例。

#### 精灵的"内容占比"（一个容易画小一半的坑）

`skins/yellow.svg`、`skins/target.svg` 的可见内容只占 512×512 画布的 **46.5%**
（逐像素量取：bbox 53..146）。所以精灵方块边长必须是"身体直径 ÷ 0.465"：

| 量 | 值 | 来源 |
|---|---|---|
| 士兵身体直径 | 1.0 棋盘单位（= 26 px @1920） | `orig_campaign_play.png` 的 `crop_soldier.png` 量取 |
| 靶心直径 | 0.94 单位（= 24 px @1920） | 同上（`crop_target` bbox 1462..1486） |
| 精灵方块 | 2.15 单位 | 1.0 / 0.465（推断：把资产的内容占比当成缩放因子） |
| 名字字号 | 1.12 单位 = 28.8 px @1920 | 原版 `name_label.tscn` 是 RichTextLabel，字号取主题默认 24@1600；实测 x-height ≈13 px 与 28.8 px 字号吻合 |

---

### 数值对照表（1920×1160，原版真帧 vs 我们）

测量脚本：`.agent-work/playfit/measure.py`（棋盘矩形/网格间距/键位/输入框）+
`--shots` 里新增的 `GEOM|` 行（控件几何）。所有数字单位都是**设备像素**。

| 项 | 原版 | 我们 | 差 | 判据 |
|---|---|---|---|---|
| 棋盘矩形 | (303,198)–(1575,959) = **1273×762** | (310,205)–(1564,954) = **1255×750** | −18 / −12（−1.4%） | 白色区域 bbox |
| 棋盘宽高比 | 1273/762 = 1.671 | 1255/750 = 1.673 | ✔ | 均 ≈ 50:30 |
| 细网格间距 | 25.5 px（26/25/26） | 25.0 px（25/25/25） | −0.5 px | 行内非白像素 x 序列 |
| 粗网格间距 | 127.8 px | 124 px | −3.8 px（−3%） | 同上 |
| 主轴 x（y 轴） | 939 | 937 | −2 | 列扫描 |
| 主轴宽 | 2 px | 3 px | +1 | 见 §7 |
| 左键盘 键填充宽 | 84（x 6–89 / 106–189 / 206–289） | 84（6–89 / 108–191 / 210–293） | ✔ | 主题色游程 |
| 右键盘 列宽 | 99 / 92 / 102 | 101 / 94 / 104 | +2 | 同上 |
| 键高 | 96 | 99 | +3 | GEOM |
| 左键盘 网格原点 | (0, 281) 296×596 | (0, 268) 300×621 | y −13 | GEOM |
| 顶栏 4 键 | 146×96 @ x=641/791/941/1091 | 144×99 @ x=640/790/940/1090 | ✔（±2） | GEOM |
| Flight `Fire!` | 337×96 @ (1583,0) | 345×99 @ (1575,0) | +8 | GEOM |
| 底栏 y / 高 | 1062 / 98 | 1063 / 96 | +1 / −2 | GEOM |
| 定时器宽 | 140（空白） | 141（空白） | +1 | GEOM |
| `f(x)=` 图标 | 96×98 | 96×96 | −2 | GEOM |
| 函数输入框 | x 267–1736 = **1470** | x 275–1732 = **1458** | −12（−0.8%） | 黑框游程 |
| 修正常数位 | 173 宽（x 1746–1919） | 174（x 1746–1920） | +1 | GEOM |
| 修正常数样例 | `- 8.315` | `- 190.100`（同一发不同函数，格式一致） | ✔ 格式 | 像素裁剪 |
| 士兵身体直径 | 26 px | 26 px | ✔ | 裁剪量取 |
| 靶心直径 | 24 px | 25 px | +1 | 裁剪量取 |
| 弹窗面板 | 1434×910 @ (243,100) | 1422×973 @ (249,78) | −12 / +63 | GEOM |
| 弹窗 4 个模式键 | 228×228 | 232×232 | +4 | GEOM |
| 弹窗描述字号 | x/40 | x/40 | ✔ | 原版脚本 |
| Cancel / Start | 194×73 / 218×73 | 197×78 / 220×78 | +3…+5 | GEOM |

**残差的系统性来源**（不是随机误差）：我们的基准视口是 1280×720，原版是 1600×900，
所有**绝对像素**量（边框 3、圆角 10、content margin 3）在我们的逻辑空间里被放大 1.5 倍，
而在原版里只放大 1.2 倍 ⇒ 同样的 3 px 边框我们画成 4.5 设备像素、原版 3.6。
这就是键盘列宽 +2、棋盘 −12 的来源。要彻底消掉得把基准视口改成 1600×900（**未做**，
影响面是整个客户端，见 §7）。

---

### 仍然存在的差异（照实写）

| # | 差异 | 为什么没做 | 证据等级 |
|---|---|---|---|
| 1 | 棋盘 **1255×750** vs 原版 1273×762（−1.4%） | 基准视口 1280 vs 1600 导致绝对 px 量（边框/圆角/content margin）等比放大 1.5 vs 1.2 | 观测 |
| 2 | 网格线宽 1.5/3 设备像素 vs 原版 1/2 | 同上 | 观测 |
| 3 | 士兵手里的小枪（原版 `draw_polygon` 的绿色小块）没画 | 形状未逐点取证 | 未知 |
| 4 | **障碍物不画** | `10-campaign.md` §7.2 有公式，但原版用全局 ChaCha 抽位置，我们没有同一条流 | 未知 |
| 5 | 靶子位置 | 原版随机放置；我们用关卡种子的确定性伪随机（右侧半区） | 自定 |
| 6 | 我方士兵出生位置 | 原版随机；我们固定 x=±18、y=(slot−1.5)×3.4 | 自定（**10-campaign.md** 的队伍侧向是对的） |
| 7 | 顶栏第 1 个按钮的三档 | 原版三张图标都有，但"三档各显示什么"没有逐档取证；我们按 single/recent/all 过滤函数列表 | 推断 |
| 8 | 弹窗面板高 973 vs 910 | 顶部返回行/间距略有差异（`MarginContainer2` 40 行结构未逐节点复刻） | 观测 |
| 9 | 计时空白时的 140 px 宽度来源 | 只知道宽度、不知道是哪个字符 | 未知 |
| 10 | 回合上限 `turn_limit` 未接 | 界面没有回合计数 | 未知 |
| 11 | 弹窗 4 个模式键点下去不动 | 原版在 JoinLevelPopUp 里也是纯显示（真正能循环的是 GameSetup）；我们连"点了会怎样"都没做 | 推断 |
| 12 | 原版底栏在 **LAN 局**里显示 `00:58` 且带一条弹道；战役局没有计时 | 两种状态我们都截了（`PlayScreen.png` / `PlayScreenCampaign.png`） | 观测 |

> 另外登记一条**给父 agent 的判断依据**：用户把顶栏第 1/2 个图标读成"网格/坐标轴"，
> 但原版真帧 + `.tscn` + `.so` 三处都指向 `view_*_functions` / `view_names`。
> 本轮**按原版实现**；若用户坚持要"网格/坐标轴开关"，那就是**新增功能**（原版没有），
> 应当另开一条"有意偏离"（`16-client.md` §7）而不是算作保真修复。

---

