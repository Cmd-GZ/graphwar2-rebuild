# 110 · 第 0 代初始种群从哪来 / 变体 B 什么时候启用

状态：结论性（两问两答）。本轮**纯静态**：GNU objdump 2.47 逐条读字节 + 从 `.rela.dyn`
反查 vtables/TypeId。**没有**新的运行时实验；转引 `docs/99` §6 的运行时计数时逐条标注。
未改 `src/`。

地址约定同全仓：**Ghidra 地址 = ELF vaddr + 0x100000，只加一次**。本文一律写 **vaddr**
（objdump 的输出就是 vaddr）。文件偏移换算：`.text` = vaddr − 0x1000，`.rodata` = 同值，
`.data.rel.ro` = vaddr − 0x2000，`.data` = vaddr − 0x3000。

---

## 0. 两句话结论

| # | 问题 | 结论 |
|---|---|---|
| ① | 第 0 代初始种群怎么来？ | **没有独立的初始化函数。** A（`0x51b440`）在整个二进制里**只有 1 个调用点**（`0x47a41e`，在分派器 `0x47a300` 里），它的第 8 实参是一个 `&Vec<个体>`；这个 Vec 由 **AI 回合体（`0x479520`）从一张 0x58 步长的哈希表里 `move` 出来**（`0x479c6d`–`0x479c89`，取走后槽位被重置成 `Vec::new()`）；**查不到就是空 Vec**（`0x479c9b`，`{cap:0, ptr:8, len:0}`）。所以：**空缓存时，第 0 代 = s 个槽全部走 `0x51b900` 随机分支**，随机分支固定是 **3 次调用、顺序固定**：`0x4ef020`(随机函数) → `0x4ee0e0`(装配/截断+按 50/34/16 补齐) → `0x4ee720`(变异)。它**不等价于**"随机生成+补齐"，而是"随机生成+补齐+**再变异一次**"，而且这条结果是该槽**1 克隆 + 4 工厂子代**的**共同父代**（所以第 0 代仍然产出 `5s` 个个体，不是 `s` 个）。 |
| ② | 变体 B 什么时候启用？ | `param_15`（= 配置 `+0x80`）= 模块容器里那个 `Arc<dyn Any>` 单例的 **`+0x32` 字节**，该单例是一个 **0x28 字节的"本局模式设置"模块**：`+0x30` = **time_mode**（6 档：30 s/1 min/2 min/3 min/5 min/∞，按 60 Hz tick 换算，**逐位验证**）、`+0x31` = **axis_mode**（4 档，对应 `axis_option_0..3.svg`，**观测**）、`+0x32` = **function_mode**（2 档：`0 = 普通函数 y=f(x)`、`1 = 微分方程 y'=f(x,y)`）。⇒ **只要本局函数模式是"微分方程"，AI 就走 B（`0x51d080`，(1+λ) 爬山）；普通函数局永远走 A（`0x51b440`，种群 GA）。** 实测"只见过 A"与"那局是普通模式"一致。 |

证据等级见 §3。②里"`+0x32` 是 function_mode"是**推断（强）**，缺的直接证据在 §2.5 写明。

---

## 1. ① 第 0 代的初始种群

### 1.1 A 的唯一入口，以及它的第 8 实参

`grep 'call   51b440'` 全文**只有 1 条**：

```
  47a41e:  e8 1d 10 0a 00   call  51b440
```

它在 A/B 分派器 `0x47a300` 里，选择点是紧邻的 `test r13b,r13b`（`0x47a393`）：

```asm
47a31f: movzx r13d,BYTE PTR [rsp+0x130]   ; 第 15 实参 = payload+0x80 = param_15（docs/99 §5.1）
...
47a393: test  r13b,r13b
47a396: je    47a3de                      ; ==0 → 走 A
47a3d3: call  51d080                      ; !=0 → 走 B
...
47a3de: mov   eax,DWORD PTR [rsp+0xf0]    ; A 的第 7 实参 = 每槽个体数
47a3e5: mov   rdx,QWORD PTR [rsp+0x140]   ; ★ 指向"初始种群 Vec"的指针
47a3ed: mov   rcx,QWORD PTR [rdx+0x10]    ; Vec 第 3 字
47a3f6: movups xmm0,XMMWORD PTR [rdx]     ; Vec 前 16 字节
47a3f9: movaps XMMWORD PTR [rsp+0x10],xmm0;  拷成栈上 24 字节
47a3fe: lea   r10,[rsp+0x10]              ; r10 = &Vec（第 8 实参）
47a41b: push  r10                         ; 先压第 8 实参
47a41d: push  rax                         ; 再压第 7 实参
47a41e: call  51b440
```

A 的栈帧（`0x51b44a sub rsp,0x188` + 6 个 push + 返回地址）⇒ 第 7 实参在 `[rsp+0x1c0]`、
第 8 实参在 `[rsp+0x1c8]`，与 A 内部两处读法**严丝合缝**：

```asm
51b70b: mov  rax,QWORD PTR [rsp+0x1c8]   ; &初始种群 Vec
51b713: mov  r13,QWORD PTR [rax]         ; +0x00
51b716: mov  rbp,QWORD PTR [rax+0x8]     ; +0x08
51b71a: mov  r12,QWORD PTR [rax+0x10]    ; +0x10 = len  ← 循环里跟槽号比
...
51b77f: mov  eax,DWORD PTR [rsp+0x1c0]   ; 每槽个体数
51b786: dec  eax
```

⇒ 初始种群的**长度在 `+0x10`**，元素是 48 字节个体（`0x51b832 lea rax,[r14+r14*2]; 0x51b836 shl rax,4`）。

### 1.2 这个 Vec 到底是谁给的：AI 回合体里"从哈希表里取走"

`0x47a300` 的 `[rsp+0x140]` 一路往上是 `0x438780`（配置解包）从 **cfg+0x28**（24 字节）取出的
Vec 的地址（`0x4387d4 movups xmm2,[rdi+0x28]` → 拷到 `[rsp+0x60]` → `0x4387e2` 把
`&[rsp+0x60]` 放进实参位置）。cfg 就是线程 payload，payload 由 **AI 回合体 `0x479520`** 装配。
payload 基址 = `rsp+0x100`，于是 payload+0x28/0x30/0x38 = `[rsp+0x128]/[0x130]/[0x138]`：

```asm
479d5b: mov QWORD PTR [rsp+0x128],rcx    ; Vec +0x00
479d63: mov QWORD PTR [rsp+0x130],r8     ; Vec +0x08
479d6b: mov QWORD PTR [rsp+0x138],rax    ; Vec +0x10
479da4: call 438d20                       ; spawn（含 RUST_MIN_STACK 解析）
```

这 3 个寄存器来自同一函数里的一张 **0x58 步长哈希表探测**（key 是 u32，来自第 2 实参
`0x47955e mov ebx,DWORD PTR [rsi+0xc]`；表在 `[r13+0x18]`，`r14` = 表+0x40）：

```asm
479c37: imul  r9,r9,0x58                  ; 表项 0x58 字节
479c3b: cmp   ebx,DWORD PTR [rdx+r9*1-0x58]  ; 比 key
479c6a: add   rdx,r9                       ; ← 命中
479c6d: mov   rcx,QWORD PTR [rdx-0x20]     ; 表项 +0x38  ┐
479c71: mov   r8, QWORD PTR [rdx-0x18]     ; 表项 +0x40  ├ 24 字节 = Vec<个体>
479c75: mov   rax,QWORD PTR [rdx-0x10]     ; 表项 +0x48  ┘
479c79: mov   QWORD PTR [rdx-0x20],0x0     ; 槽位被"取走"：置成 Vec::new()
479c81: mov   QWORD PTR [rdx-0x18],0x8     ; （ptr = 8 = align 8 的悬垂指针）
479c89: mov   QWORD PTR [rdx-0x10],0x0
...
479c9b: xor   eax,eax                      ; ← 未命中：len = 0
479ca5: mov   r8d,0x8                      ;    ptr = 8
                                           ;    （rcx 在 0x479c07 已 xor 成 0 = cap）
```

要点（全部**观测**）：

1. **初始种群来自"上一次留下的东西"**：命中就把那条 24 字节 Vec **整个 move 出来**，
   并把表项重置为 `Vec::new()`（"取走"，不是拷贝）⇒ 同一份只能被用一次；
2. **未命中 = 空 Vec**（`Vec::new()`：cap 0 / ptr 8 / len 0）；
3. 命中时还有一个字节 `表项 +0x50` 被读成 **mode**（`0x479bba movzx r13d,BYTE PTR [rcx-0x8]`，
   未命中时 `0x479bc1 xor r13d,r13d`），该字节进 payload+0x81（`0x479d53`）——**与 +0x32 无关**，
   见 §2.5 的辨析；
4. 表项的 key 是 u32，来自调用者实参 `[rsi+0xc]`（哪些字段是键值没继续追，见 §4）。

> 这次"取走"就是 `docs/99` §3 那条"初始种群是空 Vec"的**机制解释**：不是"构造出来就是空的"，
> 而是"**这张表里这一格没有东西 / 已经被上一次取空了**"。

### 1.3 空种群时第 0 代怎么走：随机分支的三次调用

槽循环头（`docs/99` §2.3 已给，这里只补"随机分支只有一个人口"）：

```asm
51b829: cmp   r12,r14       ; r12 = prev.len, r14 = 槽号
51b82c: jbe   51b900        ; prev.len <= 槽号 → 随机分支
```

`grep '51b900'` 全文只有 **1 条跳转**指向它（就是 `0x51b82c`）⇒ 该块**只能**从这里进。
块内**恰好 3 个 call，顺序固定**：

```asm
51b900: movzx ebx,BYTE PTR [rsp+0x7]      ; 函数模式字节
51b905: lea   rdi,[rsp+0x80]              ; out
51b90d: mov   esi,ebx
51b90f: call  4ef020                      ; ① 随机函数（长度 trunc(N*5+10)，每 token 一次 u32）
51b914: lea   rdi,[rsp+0x10]              ; out
51b919: mov   esi,ebx                     ; 模式
51b91b: lea   rdx,[rsp+0x80]              ; in = ①的结果
51b923: call  4ee0e0                      ; ② 装配/补齐（截断 + rand%50 补 const/x/y）
51b94f: lea   rdi,[rsp+0xf8]              ; out
51b957: lea   rsi,[rsp+0x10]              ; in = ②的结果
51b95c: call  4ee720                      ; ③ 变异一次
51b961: <本槽公共出口：5s 个体循环从这里开始>
```

②的"截断 + 补齐"本轮也在字节层面复核了一处：`0x4ee113 lea r10,[#1c47d0]` 是**按 tag 查
"还缺几个操作数"的表**（`0x4ee124 add r14d,[r10+r9*4]`，就是 `docs/AI.md` §6 的第 1 条），
补齐分支的门限是 `0x4ee2c0 cmp r12d,0x2a`（=42，与"`rand%50`：<25 常数 / 25–41 x / 42–49
模式变量"一致）。

**这条随机结果不是"第 0 代的一个个体"，而是该槽的父代**：到 `0x51b961` 之后走的是和克隆
完全相同的路径（`0x51b993` 常量折叠 → 超 `ctx->limit` 修补 → Normal 模式加修正 → 粗评），
然后 `0x51bdb2` 起再从同一份 `cur` 造 `每槽个体数−1 = 4` 个工厂子代。

⇒ **第 0 代 = `s` 个随机父代 × 每槽 5 个个体 = `5s` 个个体**（与 `docs/99` §6 转引的运行时
事件流"`RAND FN_A FN_B SIMPL SCORE` 然后 4 组 `SIMPL FN_C SCORE`、`RAND` 间距恒 17"逐项对得上）。

### 1.4 有没有独立的初始化函数？—— 有界否定

`call 4ef020`（随机函数生成器）全文**只有 7 处**，全部在 A/B/工厂内部：

| # | 地址 | 属于 |
|---|---|---|
| 1 | `0x4ee470` | 工厂 `0x4ee3a0`（`parent.len < 20` 时用随机函数当基） |
| 2 | `0x51b90f` | **A 的随机重生分支**（本文 §1.3） |
| 3–5 | `0x51c3f6` / `0x51c731` / `0x51c9eb` | A 代末前 3 名精评不自洽时的整条替换 |
| 6 | `0x51d185` | B 的种子（哈希表里没有该士兵的函数时） |
| 7 | `0x51df27` | B 每代前 20 个变体 |

再加上 A 只有 1 个调用点（§1.1）⇒ **不存在"先调某个初始化函数把种群填满，再进 A"这种结构**：
第 0 代完全由 A 自己的槽循环 + `0x51b900` 分支产生。

### 1.5 与 GW1 那条线索的对照

用户线索："GW1 = 随机凑表达式 → 用某个函数裁剪，让表达式变合法"。

GW2 的对应物是：

| GW1 步骤 | GW2 对应 | 差别 |
|---|---|---|
| 随机凑 token | `0x4ef020` | 长度 `trunc(N(0,1)*5+10)`，每 token 一次 u32 抽样（`docs/AI.md` §3/§8.7） |
| 裁剪成合法表达式 | `0x4ee0e0` | **双向**：先按 tag 表算"还缺几个操作数"走到 0 就**截断**，还缺就按 `rand%50`（<25 常数 / <42 x / 其余模式变量）**补齐** —— 所以它既"裁"也"补" |
| — | `0x4ee720` | **多一次变异**（GW1 的这条线索里没有） |
| — | 作为父代再产 4 个工厂子代 | GW1 线索里没有 |

⇒ **形态同源（随机 + 一个"修成合法"的函数），但 GW2 多一次变异，且随机函数不是直接进种群，
而是当父代**（这一点会改变"第 1 代的选择压力"：第 0 代的 4s 个工厂子代是从这些随机父代
取子树造出来的）。

### 1.6 对 `docs/99` §3 那句话的更正

`docs/99` 写"随机重生分支**只在第 1 代**发生（因为初始种群是空 Vec）"。本轮结论：

* 机制上**准确的说法**是"**只要 `槽号 ≥ 上一代长度` 就走**"，而**第 0 代的上一代 = 初始种群 Vec**，
  它的长度**可以是 0，也可以不是 0**（§1.2 的哈希表命中）；
* 所以"只有第 1 代"成立的前提是**那张表里没有该 key 的存量**（首次调用/已被取空）。运行时
  样本里 30 个 `RAND` 全在前面（`docs/99` §6）⇒ 那局第 0 代的初始种群**确实是空的**，
  该结论对那局成立，但**不能推广成"永远只在第 0 代"**；
* 反过来：命中时 `0..min(s,len)-1` 号槽克隆存量、其余槽随机 —— 这是**混合**，不是二选一。
  （存量 Vec 的长度本轮没有运行时可证，见 §4。）

---

## 2. ② 变体 B 什么时候启用

### 2.1 那个单例在机器层面是什么

`0x479520` 里读 `param_15` 的那段（`docs/99` §5.1 已给）复述并补全：

```asm
479ac3: mov   rdi,r13
479ac6: call  46e140                  ; 取"那一个模块"
479acb: test  rax,rax
479ace: je    479fb6                  ; 取不到 → 整条路径放弃
479ad4: mov   QWORD PTR [rsp+0x100],rax
479ae6: cmp   QWORD PTR [rax+0x10],0x7fffffffffffffff
479aea: jae   479fc5                  ; 借用计数溢出 → panic 路径
479af0: movzx ecx,BYTE PTR [rax+0x32] ; ★ param_15
479af4: mov   BYTE PTR [rsp+0x3f],cl
479af8: dec   QWORD PTR [rax]         ; Arc 释放（强计数）
...
479d47: movzx edx,BYTE PTR [rsp+0x3f]
479d4c: mov   BYTE PTR [rsp+0x180],dl ; payload+0x80 = variant
479d53: mov   BYTE PTR [rsp+0x181],r13b; payload+0x81 = 函数模式（来自另一处，见 §2.5）
479da4: call  438d20                  ; spawn
```

`0x46e140` 是**模块容器的按 key 取模块**：

```asm
46e19c: lea rdi,[r14+0x38]                    ; 哈希器状态
46e1a0: lea rsi,[rip+...] # 19dab0            ; ★ 16 字节 key
46e1a7: call 47e490                           ; hash
46e1d3: movdqa xmm2,[rip+...] # 19dab0        ; key 比对
46e1e0: movdqu xmm3,[rdx+rcx]                 ; hashbrown 控制字节组
46e1ff..46e22c: 16 组探测（条目 0x20 字节 = 16 字 key + 16 字 fat-ptr）
46e257: mov rax,[r9-0x10] ; mov rbx,[r9-0x8]  ; 值 = (data, vtable) = Arc<dyn Any>
46e25f: inc QWORD PTR [rax]                   ; Arc 强计数++
46e27d: lea rsi,[rax+rcx+0x10]                ; self = ArcInner+0x10
46e28d: call QWORD PTR [rbx+0x18]             ; dyn Any 的 type_id()
46e296: pcmpeqb xmm0,[rip+...] # 19d8b0       ; ★ 与期望 TypeId 比 16 字节
46e2ac: cmove rbx,r15 / cmovne rcx,r15        ; 全等才算 downcast 成功
```

本轮额外解出来的机器级事实（全部**观测**，都能自己复算）：

| 项 | 值 | 怎么得到的 |
|---|---|---|
| 容器 key（16 字节） | `d3 83 de 09 ed 48 85 b2 2f 99 4e 09 bf 5b 6b dd` @**vaddr `0x19dab0`** | `0x46e1a0`/`0x46e1d3` 的 rip 目标 |
| 模块 TypeId（16 字节） | `2c d2 68 28 13 23 05 ac 6e 93 5a a8 79 11 22 8a` @**vaddr `0x19d8b0`** | `0x46e296` 的 rip 目标 |
| `type_id()` shim | **`0x47abc0`**（`movups xmm0,[0x19d8b0]; movups [rdi],xmm0; ret`，一整个 0x10 对齐的 shim 块 `0x47ab00`–`0x47ac8x`，每个模块一个） | 反汇编 + `pcmpeqb` 常量 |
| 该模块的 `Arc<dyn Any>` vtable | **`0x9584b8`**（= 注册函数里 `lea rcx,[#9584b8]` 的那个） | `.rela.dyn` 里 addend = `0x47abc0` 的 R_X86_64_RELATIVE 落在 `0x9584d0`，减 0x18 |
| 值类型 size / align | **0x28 / 8** | vtable `+0x08` / `+0x10` |
| 注册（insert）函数 | **`0x469a90`**：`movups xmm0,[0x19dab0]`（key）→ `call 0x443bc0`（hashbrown insert）；调用者只有 `0x4512c3` 与 `0x517d42`（后者在容器初始化那一大段里，与另外 ~23 个同构的 `0x4695xx`–`0x46a4xx` 插入器并列） | `grep 'call   469a90'` |

> ⚠️ **地址空间陷阱（再次踩到）**：`docs/99` §9 写的"以常量 `0x29dab0` 为 key"是 **Ghidra 地址**，
> 它对应的 **vaddr 是 `0x19dab0`**。按 vaddr `0x29dab0` 去 `.rodata` 里读会读到
> `"set_scalePackedSceneinstantiateControl…"`（一串 Godot 方法名字符串），**完全无关**。
> 另外：`+0x32` 是**相对 `ArcInner` 的偏移**；模块自己的数据从 `ArcInner+0x10` 开始，
> 所以这个字节在**模块结构体里是 `+0x22`**（size 0x28 也只在 `+0x22` 这个解释下才自洽）。

### 2.2 三个字段各是什么（这是本轮的核心新事实）

模块值对象（`ArcInner+0x10` 起，0x28 字节）上被读写的字节有 3 个：

| Arc 侧偏移 | 结构体内偏移 | 语义 | 取值 | 证据 |
|---|---|---|---|---|
| `+0x20` | `+0x10` | **time_mode** | 0..5 = 30 s / 1 min / 2 min / 3 min / 5 min / ∞ | **逐位验证**（见下） |
| `+0x21` | `+0x11` | **axis_mode** | 0..3（对应 `res://axis_option_0..3.svg`） | 观测 |
| `+0x22` | `+0x12` | **function_mode** | 0 = 普通函数（`fx_option`）、1 = 微分方程（`dx_dy_option`） | 推断（强），见 §2.5 |

**time_mode 的逐位验证**（这条最硬）：回合时限的写法是"当前 tick + 本档时长"，跳转表在
`0x1c17a8`，被 `0x4644de` 用来按 `[模块+0x30]` 分派：

```asm
464496: movzx ebx,BYTE PTR [rax+0x30]     ; ← time_mode
464468: mov   r14d,DWORD PTR [rax'+0x58]  ; 当前时刻（另一个模块，60 Hz tick 计数）
4644de: lea   rcx,[rip+...] # 1c17a8
4644e5: movsxd rax,DWORD PTR [rcx+rax*4]
4644e9: add   rax,rcx
4644ec: jmp   rax
4644ee: add   r14d,0x708     ; 1800 tick = 30 s
464512: add   r14d,0xe10     ; 3600      = 1 min
464500: add   r14d,0x1c20    ; 7200      = 2 min
464509: add   r14d,0x2a30    ; 10800     = 3 min
4644f7: add   r14d,0x4650    ; 18000     = 5 min
4645f1: (第 6 档，不加时长)   ; ∞
464519: mov   DWORD PTR [r15+0x38],1
464521: mov   DWORD PTR [r15+0x3c],r14d   ; 写回"回合截止 tick"
```

`0x708/0xe10/0x1c20/0x2a30/0x4650` 正好是 `60 × {30, 60, 120, 180, 300}`，与
`res://turn_30s.svg / turn_1m / turn_2m / turn_3m / turn_5m / turn_inf` 六张图**逐一对应**；
跳转表本身也解出来是 6 项（index 0→`0x4644ee`、1→`0x464512`、2→`0x464500`、3→`0x464509`、
4→`0x4644f7`、5→`0x4645f1`）。**所以 `+0x30` 是"回合时限档位"，6 档，单位 60 Hz tick。**

**设置界面的"循环"写手**（三个按钮处理器，都是 `GraphwarSetupControl` 的方法：先
`cmp BYTE [rdi+0x120],1`，再用 `rdi+0xe0` 当容器句柄；类名字符串在
`0x1bfe9c`：`graphwarrust::graphwar_setup_control::GraphwarSetupControl`，
节点名字符串在 `0x1c2dce` 段：`… PlayerCount AxisModeButton FunctionModeButton TurnModeButton TimeModeButton …`）：

| 函数 | 写谁 | 循环方式（`shr` 查表） |
|---|---|---|
| `0x4765f0` | `+0x30` time_mode | `movabs rdx,0x504030201; shr rdx,cl`（cl = 旧值×8）⇒ `0→1→2→3→4→5→0`，**6 档** |
| `0x4764b0` | `+0x31` axis_mode | `mov edx,0x2010003; shr edx,cl` ⇒ `0→3→2→1→0`，**4 档** |
| `0x477410` | `+0x32` function_mode | `movzx ecx,[rbx+0x32]; xor cl,1` ⇒ **两态翻转** |

（三个函数各自被一个 GOT 槽引用：`0x477410→[0x9bb060]`、`0x4765f0→[0x9bb070]`、
`0x4764b0→[0x9bb090]`；间接调用点在 `0x3df290` / `0x3df587` / `0x3dfd94`，
都在 `graphwar_remote_event_handler.rs` 那一片——即"客户端改设置 → 本地循环 → 广播请求"。）

**图标索引**（`0x4c9190` 那个预加载函数）也印证 4/6 与 0/1：
`axis_option_0..3.svg` 分别以 index `0,1,2,3` 装载（`0x4c91f9` 起，
`xor edx,edx` / `mov edx,1` / `2` / `3`）；`fx_option.svg` 以 index **0**（`0x4c94a3 xor edx,edx`）、
`dx_dy_option.svg` 以 index **1**（`0x4c9529 mov edx,1`）装载 —— 同一个装载 helper `0x4406a0`。

### 2.3 为什么 +0x32 是 function_mode 而不是 turn_mode

网络事件落到这个模块上的写入点，是**一个函数（`0x3e2830`）里的相邻 4 个分支**。该函数的
分派表在 `0x1beda8`（tag = `[msg]`，减去 2 后索引）：

| tag | 分支 | 做的事 |
|---|---|---|
| 45 | `0x3e30a3` | `call 0x46e140` → `mov BYTE PTR [r15+0x31],bpl`（= **axis_mode**） |
| **46** | `0x3e2b1b` | `call 0x46e140` → `mov BYTE PTR [r15+0x32],bpl`（= 本文要问的那个） |
| 47 | `0x3e4173` | `call 0x46ce20`（**另一个模块**）→ `call 0x462d20(模块+0x18, bpl)` |
| 48 | `0x3e336e` | `call 0x46e140` → `mov BYTE PTR [r15+0x30],bpl`（= **time_mode**） |

两端（45 = axis、48 = time）已经由 §2.2 的字段语义**钉死**；二进制里两份独立的事件名字表
都按 **axis → function → turn → time** 的顺序列出：

* `0x1bf4f8` 段：`… axis_mode_info_event function_mode_info_event turn_mode_info_event time_mode_info_event …`
* `0x1c0c46` 段：`… game_state_info_event axis_mode_info_event function_mode_info_event turn_mode_info_event time_mode_info_event …`

⇒ 45/46/47/48 就是 `AxisModeInfoEvent / FunctionModeInfoEvent / TurnModeInfoEvent / TimeModeInfoEvent`。
**46 号（写 `+0x32`）夹在 axis 与 turn 之间 ⇒ 它是 function_mode。**

另有两条**互相独立**的旁证：

1. tag 47 写的是**另一个模块**（`0x46ce20`，经 `0x462d20` 带参 setter）；而设置界面里同族还有一个
   "无参翻转" setter **`0x476734`**：`call 0x46ce20` → `call 0x462c10(模块+0x18)` → `xor al,1`
   ⇒ 那才是 **turn_mode** 的两态翻转（`0x462c10` 读、`0x462d20` 写，同一模块的一对）。
   turn_mode 有自己的模块与自己的 setter，**所以 `+0x32` 不是它**。
2. `+0x32` 的读者里，`0x4717b8` 把它塞进一个 **step = 0.01** 的任务 payload（
   `0x4717f7 movabs rax,0x3f847ae147ae147b`）的 `+0x39` —— 那是弹道计算/求值侧的 job，
   它需要的正是"用 f(x) 还是解微分方程"这个模式位（修正项 `c=y0−f(x0)` 只在普通模式加，
   `docs/99` §2.3 的 `ctx+0x5c`；而 `0x4717ef` 同一结构里 `+0x38` 被写成常量 1）。

**极性**：图标索引把 `fx_option`(=普通函数) 放 0、`dx_dy_option`(=微分方程) 放 1，
与"`ctx+0x5c == 0` 才走 Normal 修正"（`0x51bbaa cmpb $0x0,0x5c(%rsi)`）一致 ⇒
**`+0x32 == 0` = 普通函数模式，`!= 0` = 微分方程模式**。

### 2.4 结论（可直接照做）

```
variant = 该局 function_mode        // 0 = 普通函数, 1 = 微分方程
variant == 0  →  A  (0x51b440, 种群 GA，s 个槽 × 5 个体)
variant != 0  →  B  (0x51d080, (1+λ) 单点爬山，λ = 种群/5，返回 1 个个体)
```

* 这不是"另一个开关"，而是**函数模式的直接后果**：微分方程局用爬山、普通函数局用 GA；
* 本轮没有可用的运行时样本证明"微分方程局确实调了 B"（`docs/99` §6 那局是普通模式，
  所以 `B` 零调用**与本文结论一致**，不能反过来当反例）；
* 预测（可被一条 frida 记录直接证实或推翻，见 §6）：**打一局微分方程模式，
  `0x51d080` 应该被调用、`0x51b440` 不被调用**。

### 2.5 这一步仍然没确定的东西（写清楚，不编）

1. **模块的 Rust 类型名拿不到。** 容器用 **TypeId（128 位哈希）** 当 key，`std::any::type_name`
   在二进制里没有被调用；`.data.rel.ro` 里只有 TypeId 常量与 shim。能确定的只有：
   它是 `src/graphwar_module_container.rs` 这套"服务定位器"里的一个模块、size 0x28、
   语义 = "本局的三种模式设置"。**"它就是 `src/graphwar_mode_manager.rs` 里的那个结构体"
   是推断**，缺的证据是类型名字符串（静态上不存在）。
2. **`+0x32 = function_mode` 的最后一环是"枚举声明顺序 axis→function→turn→time"**，
   靠两份事件名字表的顺序 + 两端字段语义钉住。**直接观测不到枚举声明**；
   要把它升到"逐位验证"，只需 §6 那条 frida 脚本。
3. `+0x32` **不是** `payload+0x81` 的那个 mode：后者在 `0x479bba` 从**另一张表**的
   `表项+0x50` 读（未命中则 0），两者是不同的来源。为什么 AI 要分两次拿"模式"没追
   （可能是"本局模式"vs"该士兵函数自己的模式"）。
4. `docs/99` §9 的"`+0x32` 是模块容器单例的 `+0x32`"这层描述**地址对、偏移解释不够**：
   `+0x32` 是相对 `ArcInner` 的，模块结构体里是 `+0x22`（size 0x28）。
5. 模块 `+0x10`（结构体内）有一个引用计数型指针（三处 setter 都会 `mov r14,[r15+0x20]` 再
   对它做同样的借用检查），指向什么**没追**。

---

## 3. 证据等级表

| # | 结论 | 证据（vaddr） | 等级 |
|---|---|---|---|
| 1 | A 全文只有 1 个调用点 `0x47a41e`；B 只有 `0x47a3d3`；(非)0 → A/B 由 `r13b`（第 15 实参）选 | `grep 'call   51b440'`/`51d080`；`0x47a31f`/`0x47a393` | **观测** |
| 2 | A 的第 7/第 8 实参位置与 `0x51b77f`/`0x51b70b` 的读法一致（帧 0x188 + 6 push + RA） | `0x51b44a`、`0x51b70b`–`0x51b71a`、`0x51b77f` | **观测** |
| 3 | 初始种群 Vec 由 AI 回合体从 0x58 步长哈希表**move** 出（取走即置空），未命中为 `Vec::new()` | `0x479c37`、`0x479c6d`–`0x479c89`、`0x479c9b` | **观测** |
| 4 | 该 Vec 经 payload+0x28 → cfg+0x28 → 分派器 → A 第 8 实参 | `0x479d5b`–`0x479d6b`、`0x479da4`、`0x4387d4`/`0x4387e2`、`0x47a3e5`–`0x47a41e` | **观测** |
| 5 | `0x51b900` 只能从 `0x51b82c jbe` 进；块内**只有** 3 个 call，顺序 `4ef020→4ee0e0→4ee720` | `0x51b82c`、`0x51b90f/0x51b923/0x51b95c` | **观测** |
| 6 | 随机父代与克隆走同一条"折叠→修补→修正→粗评"路径，并各自再生 4 个工厂子代 | `0x51b961`–`0x51b993`、`0x51bd5f`/`0x51be93` | **观测** |
| 7 | 无独立初始化函数（`call 4ef020` 全库 7 处，全在 A/B/工厂内） | 7 个调用点，见 §1.4 | **观测**（有界否定，范围=全库直接 call；函数指针派发另见 §7） |
| 8 | `0x4ee0e0` 既截断又补齐（tag 表 `0x1c47d0` + 门限 0x2a） | `0x4ee113`/`0x4ee124`/`0x4ee2c0` | **观测** |
| 9 | 容器 key/TypeId 的 16 字节值、`type_id` shim `0x47abc0`、vtable `0x9584b8`、size 0x28/align 8 | `0x19dab0`、`0x19d8b0`、`0x46e1a0`/`0x46e296`、`.rela.dyn`（addend `0x47abc0`） | **观测** |
| 10 | `+0x30` = time_mode，6 档，60 Hz tick（1800/3600/7200/10800/18000/∞） | 跳转表 `0x1c17a8` + `0x4644ee`…`0x464521` | **逐位验证**（表与常量互证） |
| 11 | `+0x31` = axis_mode，4 档（循环表 `0x2010003`；图标 `axis_option_0..3`） | `0x47651b`、`0x4c91f9` 起 | **观测** |
| 12 | `+0x32` 是两态，翻转写手 `0x477410`（`xor cl,1`） | `0x477461`–`0x477470` | **观测** |
| 13 | 事件 tag 45/46/47/48 = axis/function/turn/time，其中 46 写 `+0x32`、47 写另一个模块 | `0x1beda8` 表 + `0x3e30a3`/`0x3e2b1b`/`0x3e4173`/`0x3e336e` | **观测**（表值与分支） |
| 14 | `+0x32` = **function_mode**（0=普通、1=微分方程） | §2.3 的 tag 邻接 + 两份事件名字表顺序 + `fx_option` 索引 0 / `dx_dy_option` 索引 1 | **推断（强）**；缺"枚举声明顺序"的直接观测 |
| 15 | 该模块的 Rust 类型名 | —— | **未知**（TypeId 是哈希；二进制里无 type_name 字符串） |
| 16 | 运行时"微分方程局走 B" | —— | **推断（未实测）**，§6 给出可一次证伪的实验 |

---

## 4. 若要落到 `src/`（本轮**没改**，只写怎么改）

### 4.1 第 0 代（`src/aigen.rs::evolve`）

1. 现在的实现若"先随机初始化 `5s` 个个体"，与游戏**不符**：游戏是 `s` 个随机**父代**
   （`mutate(assemble(random_function()))`），每个父代再产 `per_slot − 1` 个工厂子代 + 1 个
   自身克隆。等价改法：

   ```rust
   // prev 为空时（第 0 代）
   for slot in 0..s {
       let parent = mutate(&assemble(&random_function(mode), mode), mode); // 4ef020→4ee0e0→4ee720
       born.push(fold_and_score(&parent));                  // 克隆那一份
       for _ in 1..per_slot { born.push(factory(&parent)); } // 4 个共用父代
   }
   ```
   ⇒ 第 0 代**仍然是 `5s` 个个体**，会被截断到 `s`。随机数消耗顺序必须也是
   随机函数 → 补齐 → 变异（不要在中间插别的抽样）。
2. `evolve` 的入参建议加 `initial: &[Individual]`（对应 A 的第 8 实参），语义是
   "**取走**"（调用方在使用后应把它置空）。若暂时不实现缓存，就传空切片——这与本机
   实测的那局一致。
3. 不要给"第 0 代"开特例函数：游戏的槽循环对"槽号 ≥ prev.len"只有一个出口，混用
   （部分槽克隆存量、其余随机）是正常路径。

### 4.2 变体 B

1. 选择器**不是**独立开关，而是 `function_mode`：
   `let use_variant_b = function_mode != FunctionMode::Normal;`（0 = 普通 → A）。
2. B 本轮未实现，建议仍按 `docs/99` §7.7：接口留 `use_variant_b`，实现 A，
   并在注释里写明"**B 的触发条件是微分方程模式**"（依据本文 §2）。
   等 §6 的实验做过再决定要不要实现 B。

---

## 5. 复现（全部只读）

```bash
cd /home/user/MyProject/gw2

# ① A 的唯一调用点 / 实参搬运
objdump -d -M intel --start-address=0x47a380 --stop-address=0x47a430 libgraphwarrust.so
objdump -d -M intel --start-address=0x51b700 --stop-address=0x51b980 libgraphwarrust.so
# ① 初始种群的"取走"
objdump -d -M intel --start-address=0x479c6a --stop-address=0x479cb4 libgraphwarrust.so
objdump -d -M intel --start-address=0x479d40 --stop-address=0x479db0 libgraphwarrust.so
# ① 随机分支（只有 3 个 call）与唯一入口
grep -n '51b900' <(objdump -d -M intel libgraphwarrust.so)
objdump -d -M intel --start-address=0x51b900 --stop-address=0x51b970 libgraphwarrust.so
# ① 有界否定
grep -c 'call   4ef020' <(objdump -d -M intel libgraphwarrust.so)   # → 7

# ② key/TypeId 常量（vaddr！不是 Ghidra 地址）
python3 -c "d=open('libgraphwarrust.so','rb').read(); print(d[0x19dab0:0x19dac0].hex()); print(d[0x19d8b0:0x19d8c0].hex())"
objdump -d -M intel --start-address=0x46e1a0 --stop-address=0x46e2c0 libgraphwarrust.so
objdump -d -M intel --start-address=0x47abc0 --stop-address=0x47abd0 libgraphwarrust.so
# ② vtable → size（.data.rel.ro vaddr − 0x2000 = 文件偏移）
python3 -c "d=open('libgraphwarrust.so','rb').read(); import struct; print([hex(x) for x in struct.unpack('<4Q', d[0x9584b8-0x2000:0x9584b8-0x2000+0x20])])"
# ② time_mode 的 6 档跳转表
objdump -d -M intel --start-address=0x464490 --stop-address=0x464530 libgraphwarrust.so
# ② 三个循环写手
objdump -d -M intel --start-address=0x4764b0 --stop-address=0x476530 libgraphwarrust.so
objdump -d -M intel --start-address=0x4765f0 --stop-address=0x476670 libgraphwarrust.so
objdump -d -M intel --start-address=0x477410 --stop-address=0x477480 libgraphwarrust.so
# ② 事件 tag 表 + 四个相邻分支
objdump -d -M intel --start-address=0x3e2830 --stop-address=0x3e28a0 libgraphwarrust.so
objdump -d -M intel --start-address=0x3e2b1b --stop-address=0x3e2b60 libgraphwarrust.so
objdump -d -M intel --start-address=0x3e30a3 --stop-address=0x3e30e0 libgraphwarrust.so
objdump -d -M intel --start-address=0x3e4173 --stop-address=0x3e41b8 libgraphwarrust.so
objdump -d -M intel --start-address=0x3e336e --stop-address=0x3e33a8 libgraphwarrust.so
```

---

## 6. 可一次性把 §2.5 第 2 条升到"观测"的实验（本轮未做）

用现成范例 `gw2-rebuild/tools/hook_ai_phases.js`（frida，`runtime:'v8'`，寄存器在 `this.context`）：

1. 在 `0x47a41e`（A）与 `0x47a3d3`（B）各下 Interceptor，记录返回地址/次数；
2. 在 `0x479af0`（读 `+0x32`，`movzx ecx,[rax+0x32]`）下 Interceptor，记录 `rax+0x32` 的值；
3. 打两局：**普通函数模式**一局、**微分方程模式**一局（设置界面 `FunctionModeButton` 切一下，
   或直接在 `0x3e2b1b` 分支之前把 `[msg+8]` 改 1）。
   预期：普通局 `+0x32 == 0` 且 A 有调用、B 为 0；微分方程局 `+0x32 == 1` 且 B 有调用、A 为 0。
4. 顺手可确认 §1.2：在 `0x479c79`（把表项 Vec 置空）与 `0x479c9b`（空 Vec 分支）各下一处计数，
   看一局里"命中/未命中"各几次、命中时 `len`（`rdx-0x10` 的值）是多少 ——
   这条能回答"存量 Vec 是 1 个函数还是上一代的 s 个存活者"（本轮**未知**）。

---

## 7. 本轮方法与局限

* **全部用 objdump**（`objdump -d -M intel` 全文 92 MB 落在自己的 `.agent-work/gen0/` 里做检索），
  地址定位用 "rip 相对目标 = 下一条指令地址 + disp32" 手算，跳转表用 Python 解 `int32`；
  vtable/TypeId 用 `.rela.dyn` 里 `R_X86_64_RELATIVE` 的 addend 反查。
* **没有用 REA/Ghidra**（本轮不需要伪代码；也避免 `open_binary` 退掉别人的共享 session）。
* 局限：
  1. "无独立初始化函数"是**直接 call** 范围的有界否定；`docs/AGENTS.md` 提醒 Rust 大量函数指针
     派发。本轮已额外检查：`call 4ef020`/`call 4ee0e0`/`call 4ee720` 都只在这些函数体内部，
     且它们在 `.data.rel.ro` 里**没有**出现在函数指针表中（`0x51b900` 那三条是直接 call）。
     若要彻底，可用 `tools/ptr_slots.py` 再扫一遍这三个地址的 GOT 槽（本轮未做）。
  2. §1.2 那张哈希表的**业务归属**（哪个管理器的表、key 到底是什么 id）没追：
     key 来自 AI 回合体第 2 实参 `+0xc`（u32），值 = `{…, Vec<个体> @+0x38, mode @+0x50}`。
  3. §2 的"模块类型名"是**未知**（上界已说明：静态拿不到）。
