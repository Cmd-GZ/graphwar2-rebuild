# 150 · 胜负条件：**一条**规则还是两条？（裁定 + 修正）

状态：**现行**，本文**推翻**了 `docs/131` §2 的"按关卡类型两分"口径。

三件事，按证据回答：

1. **Q1：靶子是不是 `Team2` 的实体？** —— **是**。⇒ "靶子全清"与"`Team2` 全灭"
   在原版里**是同一件事**，`docs/123` 的 A 口径对，`docs/131` 的 B 口径是**我们自造的**。
2. **Q2：`0x46f310` 到底判了什么？** —— **它什么也不判**。它是**造靶子的构造函数**
   （分配 id + 发 4 条实体事件），队伍是它的**入参**。真正的胜负判定在 `0x473520`
   + `0x4759f0`，本文把它们读完了。
3. **Q3：回合上限用尽算不算输？** —— **算**，而且**不再是推断**：`0x4759f0` 里就有判定点，
   胜方被写死成 `1`（`Team2`）。

> 真值来源：`/home/user/MyProject/gw2/libgraphwarrust.so`
> （sha256 `9a538115…abed187`，**只读**）与
> `~/.local/share/godot/app_userdata/Graphwar/replays/`（**只读，不入库**，401 份）。
> 地址一律 **ELF 虚地址**（= `objdump` 打印的地址）。**`Ghidra/REA 地址 = 本文地址 + 0x100000`**，
> 只加一次。`.text` 的 `vaddr = 文件偏移 + 0x1000`，`.rodata` 的 `vaddr = 文件偏移`（`readelf -lW`）。
> 本文**不修改** `docs/INDEX.md` / `docs/HANDOVER.md`，也没有改 `docs/131`（不在本任务的文件范围里，
> §6 写清它该怎么改）。

证据分级沿用 `AGENTS.md`：**逐位验证** / **观测** / **推断** / **未知**。
⚠️ 按 `AGENTS.md` §0 第 2 条：`cargo test` / `selftest` 的条数**不是**复原证据，只是回归保护。

---

## 0. 一页结论

| # | 问题 | 结论 | 档位 |
|---|---|---|---|
| Q1 | 靶子是 `Team2` 的实体吗 | **是**。111 份含靶子的回放里 **127/127** 个靶子 id 都有 `TeamInfo{Team2}`；**127/127 都没有 `SoldierInfo`**（靶子不是士兵，但**是**队伍成员） | **观测** |
| Q1' | 同一个函数造靶子、队伍是入参吗 | **是**。`0x46f310(sink, team)`，`team` 走 `movzx edx,bpl` 传给 `TeamInfo` 构造点；**21/21 个调用点全部 `mov esi,1`（= Team2）** | **逐位** |
| Q2 | `0x46f310` 判了什么 | **不判任何东西**：分配 id → `TargetInfo` → `TeamInfo{team}` → `SkinGraphicsInfo("target")` → `EntityEffectInfo(SparkEffect)`。它是**构造**，不是**判定** | **逐位** |
| Q2' | 真正的判据是什么 | `0x473520(self) -> (Team1 有活实体, Team2 有活实体)`：遍历**生命表**的 id，`0x454950` 判活 → `0x3c2120` 查队伍（`0`=Team1/`1`=Team2/`2`=无队）。**只数"有没有"，不数"几个"** | **逐位** |
| Q2'' | 结束条件是什么 | `0x4759f0`（回合末）：① 回合上限用尽 ② `al==0`（Team1 没人）③ `dl==0`（Team2 没人）—— **三条都跳同一个出口** `0x475bac`（`state=6` = `EndingGame`） | **逐位** |
| Q2''' | 原版有"靶子清空 ⇒ 胜"的独立分支吗 | **没有**。`0x4759f0` 里唯一和靶子有关的代码是**"打不全就复原"**（§4），而且它只在**游戏还没结束**时才跑 | **逐位**（负面证据） |
| 语料 | 回放支持吗 | **217/217** 次 `EndingGame` 都"存在某队零活实体"（含整队被 `RemoveTeam`）；45 份打靶关收场**全部**是 `Team2` 零活实体且 `Team1` 还有人 | **观测** |
| Q3 | 回合上限用尽算输吗 | **算**。上限用尽 ⇒ 走同一个出口 `EndingGame`；此时若两队都还有人，胜方被写死成 `1`（Team2）⇒ **玩家输**。`docs/131` 的"推断"升级为**真值** | **逐位** |
| 附带 | "打不全就复原"的**判定点**找到了吗 | **找到了**：`0x4759f0` 回合末、`TurnCountInfo` **之后**，条件 = `[self+0x11d] == 1` 且"至少一个靶子还活着" ⇒ 把**全部**靶子 id 逐个 `0x454b40(id, alive=1, mode=0)`。`docs/123` §3.2 的"判定点未知"可以取消了 | **逐位** |
| 附带 | 那个 `[self+0x11d]` 是哪几关打开的 | **未知，但负结论已做实（`docs/151` §2.6）**。手法换成「全 `.text` 按**位移**过滤」（不依赖反编译器）：全库 `+0x11d` 的访问**一共 10 处**，其中 `0x475a91: cmpb $0x1,0x11d(%rbx)` 是唯一与本结构相关的读；其余 9 处是 `cmpb $0x86,0x11d(%rax)`（`0x5ce/0x5d0/0x5d1/0x5d3`，另一族对象）与 `setg/movb 0x11d(%r8,%rdx,1)`（`0x737xxx`，**带索引的字节数组**，不是字段）⇒ 没有一个字段写者，只能是被整块 `memcpy`/结构体赋值写进去的 ⇒ **"是不是只有 6/8/10/13/17 五关"仍然没有证据** | **未知**（负结论 = 观测） |

**一句话**：原版只有**一条**胜负规则——*某队没有存活实体*；靶子带队伍，所以打靶关天然被这条规则覆盖。
`docs/131` 的"两分法"是**我们客户端因为没给靶子分配队伍而自造的**，本轮已把它收敛掉。

---

## 1. Q1：靶子是不是 `Team2` 的实体？

### 1.1 语料侧（**观测**，401 份回放里 111 份含靶子）

对每一个出现过 `TargetInfo` 的 id，取它在**同一份回放**里的其它事件：

| 检查 | 结果 |
|---|---|
| 有 `TeamInfo` 吗、是哪一队 | **127/127 → `Team2`**（没有一条是 Team1，也没有一条缺 `TeamInfo`） |
| 有 `SoldierInfo` 吗 | **0/127**（一个都没有） |
| `SkinGraphicsInfo.graphics_str` | **127/127 → `"target"`** |
| `PosInfo.radius` | **302/302 条 → `0.5`** |
| 造它时伴随的 `EntityEffectInfo` | **127/127 → `SparkEffect`** |

⇒ 靶子 = **有队伍、有位置、有存活位的普通实体**，只是**不在士兵表里**。
这不是"我们的建模选择"，是运行时真值。

### 1.2 二进制侧（**逐位**）

`0x46f310` 的完整动作（`objdump -d -M intel`，函数体 `0x46f310`–`0x46f689`）：

```asm
46f31e  mov    ebp,esi                  ; ① 第 2 个参数 = team
46f323  add    r14,0xe0                 ; 管理器注册表
46f32a  call   46ee00                   ;   取 target_manager（集合在 +0x30）
46f389  lock xadd [0x9c10c0],ebx        ; ② 全局原子计数器 ⇒ 新实体 id
46f398  call   43e390                   ; ③ 把 id 插进 target 集合
46f3ad  movabs r15,0x8000000000000017   ; ④ tag 23 = TargetInfo
46f413  movzx  edx,bpl                  ; ⑤ team（就是入参！）
46f419  call   3c17d0                   ;   ⑥ TeamInfo{id, team}
46f463  mov    DWORD PTR [rax],0x67726174  ; 'targ'
46f45d  mov    WORD  PTR [rax+4],0x7465    ; 'et'
46f487  call   468c70                   ; ⑦ SkinGraphicsInfo{id, "target"}
46f4ca  mov    BYTE PTR [rsp+0x2c],0x2  ; ⑧ effect = 2 (SparkEffect)
46f4dd  call   [0x9bac80]               ;   EntityEffectInfo{id, 2}
```

**没有一条比较、没有一个分支能决定胜负** ⇒ `0x46f310` 是**构造**。

调用点（经 GOT 槽 `0x9bbed8`，21 处）**全部**在 `mov esi,1` 之后调用：

```
0x499ca9 0x49a171 0x49a19b 0x49a744 0x49b28e 0x49b7ce 0x49bdc3 0x49bdee
0x49be19 0x49c29d 0x49c2c7 0x49f87f 0x4a167f 0x4a16aa 0x4a279f 0x4a375f
0x4a378a 0x4a37b5 0x4a8bd0 0x4a9830 0x4a985b          ← 21/21 都是 mov esi,1
```

队伍编码：`0` = Team1、`1` = Team2、`2` = 无队（`0x3c2120` 的缺省返回就是 `2`，
见 `3c2179  mov r14b,0x2`）。所以 `esi = 1` 就是 **Team2**。

⇒ **结论**：靶子是 `Team2` 的实体。**"Team2 全灭" ≡ "靶子全清"**，A 口径对，B 口径多余。

### 1.3 为什么 `docs/131` 会写出 B

`docs/131` §3 自己写了原因：我们的客户端**没有给靶子分配队伍**，靶子只在
`GwPlaneView` 里画，判定时靠 `targets_total/targets_alive` 两个**并行**的计数。
于是"原版的一条规则"在**我们的实现里**被拆成了两条 —— 这就是 `docs/148` §3.1
登记的那处矛盾。**它不是"原版有两种胜负"，是"我们的模型缺了靶子的队伍字段"。**

---

## 2. Q2：真正的判定在哪、判了什么

### 2.1 `0x473520`：数"两队各有没有活实体"（**逐位**）

```asm
47353b  call 46e7a0        ; 取 team_manager
473551  call 46d6a0        ; 取 life_manager
473591  call 453c60        ; → 生命表里**全部实体 id** 的快照（Vec<u32>，元素宽 4）
4735b7  shl rbx,2          ; 循环 n 个 id
473605  mov r12d,[r14+r15] ;   id
473617  call 454950        ;   is_alive(life_mgr, id)  → al
47361c  test al,1 ; je 跳过 ;  不是"活"就跳过（缺省返回 2 = 查不到，也跳过）
47364a  call 3c2120        ;   team_of(team_mgr, id) → 0/1/2
47364f  test al,al   ; je  → [rsp]   = 1    ; 值 0 = Team1 有活实体
47365a  cmp  eax,0x2 ; je  跳过              ; 值 2 = 无队伍（玩家实体就是这样）
473663                → [rsp+4] = 1          ; 值 1 = Team2 有活实体
4736c3  mov eax,[rsp] ; mov edx,[rsp+4]      ; 返回 (Team1 有活人, Team2 有活人)
```

要点三条（都影响我们的实现）：

1. **数的是"有没有"，不是"几个"** —— 返回值是 `bool`，不是计数。
2. **数据源是生命表**（`0x453c60` 快照 + `0x454950` 判活），不是士兵表。
   靶子有 `LifeInfo` ⇒ 在生命表里 ⇒ **算**；玩家实体（`PlayerInfo`）没有 `LifeInfo`
   ⇒ **不算**。这一条正好解释了语料里那 18 份"整队 `RemoveTeam` 后游戏才结束"的联机局。
3. 队伍查不到时返回 `2` 并被**跳过**，所以"没队伍的实体"**不影响**判定。

### 2.2 `0x4759f0`：回合末的三个结束入口（**逐位**）

```asm
4759fe  inc DWORD [rsi+0x118]         ; turn_count += 1
475a42  add rax,0x3b ; push           ; tag 58 = TurnCountInfo
475a5e  mov ebp,[rbx+0xc8]            ; 上限"有没有设过"（0x472400 写的那个 dword）
475a64  cmp r15d,[rbx+0xcc]           ; turn_count vs turn_limit
475a6b  setae r15b                    ; r15b = (turn_count >= limit)
475a72  call 473520                   ; (al, dl) = (Team1 有活实体, Team2 有活实体)
475a77  test bpl,r15b ; jne 475bac    ; ① 上限用尽（且设过）⇒ END
475a80  test al,1     ; je  475bac    ; ② Team1 没人     ⇒ END
475a88  test dl,1     ; je  475bac    ; ③ Team2 没人     ⇒ END
475a91  cmp BYTE [rbx+0x11d],1 ; jne 475d7e   ; ← 只有"游戏没结束"才走到这里
475bac  mov BYTE [rbx+0x120],0x6      ; state = 6 = EndingGame
475bc8.. push tag 30 字节 6            ; GameStateInfo{EndingGame}
```

**胜方**（写进 `[self+0x11f]` 的那个字节，`0`=Team1 / `1`=Team2 / `2`=没人赢）：

```asm
475bf3  call 473520                   ; 再数一次
475bfb  test al,1 ; je 475c0d
475bff  xor  eax,eax                  ; 候选 = 0 (Team1)
475c01  test dl,1 ; je 475c14          ;   Team2 没人 ⇒ 胜方 = 0
475c06  cmp  ebp,1 ; adc al,1         ;   两边都有人 ⇒ 胜方 = 1 (Team2)
475c0d  and  dl,1 ; mov al,2 ; sub al,dl   ; Team1 没人 ⇒ 2-dl（Team2 有人=1，两边都没=2）
```

### 2.3 原版**没有**"靶子清空 ⇒ 胜"这条分支（负面证据，**逐位**）

`0x4759f0` 里与靶子有关的代码**只有一段**，就是 §4 的复原，而且它在
`0x475a91` 之后 —— 也就是**前面三条结束判定都没命中**时才执行。
全函数里没有第二处访问 target manager，也**没有**任何"target 集合为空 ⇒ END"的分支。

⇒ **B 口径（"打靶关 = 靶子全清"作为独立规则）在原版里不存在。**
它在打靶关上"看起来对"，纯粹是因为打靶关的 `Team2` 里**只有靶子**。

---

## 3. 语料核对（**观测**，401 份回放）

| 统计 | 结果 |
|---|---|
| 出现 `EndingGame` 的回放 | **217** 份（`docs/123` §4.5 的 "45/45" 只统计了**含靶子**的 109 份，不是全部） |
| 这 217 份里"存在某队零活实体"（士兵 ∪ 靶子；整队被 `RemoveTeam` 也算） | **217/217** —— **0 例外** |
| 其中"打靶关收场" | **45** 份：`Team2` 零活实体 **45/45**、`Team1` 还有人 **45/45**、被清空的队只有 Team2 **45/45** |
| 反向：有"某队零活实体"却**没有** `EndingGame` 的回放 | **2** 份（`1782674564` / `1782765027`）—— 两份都是联机局的录像，最后一发之后**没有回合边界事件**就结束了 ⇒ 最可能是**录像截断**（推断，缺：这两局的完整录像） |
| 目标被 `EntityRemoved` | **0 次**（`docs/123` §4.3 ③ 复核：结论不变） |
| 实战中"先死又活" | **0 次**（复核：结论不变） |

⇒ `EndingGame` ⟺ "某队零活实体"这条**双向**关系在语料里成立（217/217 + 2 份截断样本）。

### 3.1 13 个打靶关（回放里的 `GameNameInfo` 名字，**观测**）

| 关卡 | 靶子/局 | 有靶子的回放 | 其中出现 `EndingGame` |
|---|---|---|---|
| `Level 0` | 1 | 33 | 14 |
| `Level 1` | 1 | 3 | 2 |
| `Level 2` | 1 | 2 | 1 |
| `Level 3` | **3** | 3 | 1 |
| `Level 4` | 1 | 2 | 1 |
| `Level 6` | 2 | 2 | 1 |
| `Level 7` | 1（上限 **1**） | 1 | 1 |
| `Level 8` | **3** | 1 | 1 |
| `Level 10` | 2 | 2 | 1 |
| `Level 12` | 1 | 1 | 1 |
| `Level 13` | 2 | 2 | 1 |
| `Level 16` | 1 | 57 | 18 |
| `Level 17` | 2 | 2 | 2 |

合计 **45** 份收场，与 `docs/123` §4.5 的数字一致（那份文档只统计了含靶子的 109 份，
本文统计的是全部 401 份）。每一份的收场都是 `Team2` 零活实体、`Team1` 还有人。

---

## 4. 附带收获①："打不全就复原"的**判定点**（**逐位**）

`docs/123` §3.2 问题 1 把判定点列为**未知**（三个候选：射击结算 / `end_turn` / 下次开火前）。
**它就在 `0x4759f0` 里**，而且条件比原来推断的更具体：

```asm
475a91  cmp BYTE [rbx+0x11d],1 ; jne 475d7e    ; 关卡位；不是 1 就跳过整段
475aa8  call 46ee00                            ; target_manager
475abe  call 46d6a0                            ; life_manager
475af5/475b06  遍历 target 集合（hashbrown：[A+0x48] items / [A+0x30] ctrl）
475b80  mov esi,[rdx-0x4]                      ;   id
475b83  call 454950                            ;   is_alive(id)
475b8c  test al,1 ; jne 475c79                 ;   **只要有一个靶子还活着** ⇒ 去复原
475ba1  jne 475b20                             ;   否则继续扫；扫完 ⇒ 475d45（什么都不做）
475d2d  call 454b40 (rdi=life+0x18, esi=id, edx=1, ecx=0)   ; ← 复原：alive := 1
475d43  jne 475cd0                             ;   对**集合里每一个** target id 都做一遍
```

逐条含义：

| 事实 | 证据 | 档位 |
|---|---|---|
| 判定点是**回合末**（`0x4759f0` = `end_turn`），在 `TurnCountInfo` **之后** | `0x475a42` 先发 `TurnCountInfo`，`0x475a91` 在其后 | **逐位** |
| 判据是"**至少一个靶子还活着**" ⇒ 把**全部**靶子翻回 alive | `0x475b8c` / `0x475d2d` 的循环 | **逐位** |
| 复原 = `life_manager.set(id, alive=1, mode=0)`，**不**新建实体、**不**发 `TargetInfo` | `0x454b40` 把 `dl` 写进表一（`0x440d40`），`mode==1` 才写表二；这里是 `mode=0` | **逐位** |
| 游戏已经结束时**不会**复原 | 三条 `je 475bac` 在 `0x475a91` **之前** | **逐位** |
| 靶子**全部**已死时什么都不做（那段循环直接扫完） | `0x475ba7 jmp 475d45` | **逐位** |

⇒ `docs/123` §3.2 的形状推断（"只翻 alive 位"）**被证实**；"判定时刻"从
三选一变成**唯一**（回合末），"全部复原还是只复原本发"也变成**全部**。

**仍未定的一条**：`[self+0x11d]` 这个关卡位**是哪几关打开**的。
全文件**没有任何一条指令写它**（唯一访问就是 `0x475a91` 的读；把整个 `.text` 的
`+ 0x11d]` 引用列出来只有这一条落在本结构上），所以它只能是被**整块结构赋值 /
`memcpy`** 写进去的，指令流里看不见 ⇒ **未知**（缺：关卡配置结构体的布局，或一次
"部分命中"的原版录像）。用户口述说只有 `Level 6/8/10/13/17`，但那是**口述**，不是真值。

---

## 5. Q3：回合上限用尽算不算输

**算。而且这是真值，不是推断了。**

| 事实 | 证据 | 档位 |
|---|---|---|
| 上限是**每关的数据**：`0x472400(esi=标志, edx=N)` 写 `[self+0xc8]`/`[self+0xcc]`，再发 `TurnLimitInfo` | `0x472405`/`0x47240b`/`0x472433`；唯一调用者是关卡分支所在的 `0x499740` | **逐位** |
| 上限用尽 ⇒ **结束**（和"某队全灭"**同一个出口**） | `0x475a64`–`0x475a7a` → `0x475bac` | **逐位** |
| 上限用尽时若两队都还有人 ⇒ **胜方 = 1（Team2）**，即玩家输 | `0x475bff`–`0x475c0b` | **逐位** |
| 报文形状 `{"TurnLimitInfo":{"turn_limit":N}}` | 语料 21 条（N ∈ {1,5,6,10}），**只有一个字段** | **观测** |
| "没上限" = **根本不发**这条事件 | 语料：`Level 0` 与全部对联机关一次都没发 | **观测**（沿用 `docs/123` §5.1 #4） |
| 语料里"打满上限还没分胜负"的样本 | **0 份**（唯一上限 = 1 的 `Level 7` 样本首发就命中） | **观测** |

⇒ 结论：`EndingGame` + `Team2` 胜 = **玩家失败**。`docs/131` §2.1 / §5.2 场景④
的"判负"与二进制一致；差别只在**依据**：那边标的是**推断**，现在是**逐位**。
"上限用尽会不会进结算框"这类 UI 问题仍然**没有原版真帧**（见 §7）。

---

## 6. 我们错了什么、改了什么

### 6.1 我们错了（直说）

1. **`docs/131` §2 的"两分法"是我们的发明。** 原版只有一条判据（§2）；
   它能在打靶关上"工作"，只是因为打靶关的 `Team2` 里只有靶子。
2. **`docs/131` §2.1 把"回合上限用尽 ⇒ 判负"标成推断**，其实二进制里有判定点。
3. **`docs/131` §3.2 说靶子半径 0.5 是"推断"**：语料里 **302/302** 条靶子的
   `PosInfo.radius` 都是 `0.5` ⇒ 升为**观测**。
4. **`docs/123` §3.2 把"复原判定点"标成未知**：它就在 `0x4759f0`（§4）。
5. 我们客户端**没给靶子分配队伍**（`docs/131` §3 自述），这是 1 的根因。

### 6.2 改了什么

| 文件 | 改动 | 依据 |
|---|---|---|
| `src/game.rs` | 新增 `Match::targets`（`id -> alive`）、`turn_limit`；新增 `add_target/kill_target/restore_target/targets_total/targets_alive/is_target/team_has_survivor/set_turn_limit/ending`；`end_turn()` 改成**只调 `ending()`**；新增 `Ending` / `EndingReason` | §2/§4/§5 的逐位结论 |
| `src/game.rs` | `team_has_survivor` = **在场士兵 ∪ 存活靶子**，是唯一判据；`set_team` 的注释改成"实体（士兵或靶子）→ 队伍" | §2.1 |
| `tests/win_rule.rs` | **新增** 10 条断言：打靶关全清 / 对战关全灭 / **两者走同一条路径** / 混合队伍反证 / 靶子不是士兵 / 复原只翻 alive / 上限用尽判 Team2 胜 / 无限则不结束 / 互相清空无胜方 / 空局不判 | §0 的表 |
| `client/src/game/level_rules.gd` | **整份重写判据**：删掉"按 `kind` 分两支"，改成"逐队数存活实体（士兵+靶子），为 0 就结束"；`kind` 降级为**标签**；`reason` 字符串**全部保留**（`targets_cleared` / `enemy_wiped` / `own_wiped` / `mutual_wipe` / `turn_limit` / `no_verdict` / `playing`） | §2 |
| `client/src/game/target_set.gd` | 新增 `const TEAM := 1`（= Team2，靶子的队伍）；半径注释从"推断"改成"观测（302/302）" | §1 |
| `client/libgw2_rebuild.so` | `cargo build` 后覆盖（内核） | — |

**没有改**：`client/src/screens/*`、`client/src/play/*`、`src/graphwar_path_calculator.rs`、`src/_scaffold/api.rs`、
`src/_scaffold/replaydiff.rs`、`src/_scaffold/diffconv.rs`、`tests/server_match.rs`、`docs/INDEX.md`、
`docs/HANDOVER.md`、`docs/131`（交接见下）。

### 6.3 `docs/131` 的口径该不该改：**该改**

按 `AGENTS.md` §0 第 6 条（文档不许虚报），`docs/131` 应改成：

* §2.1 的表格从"按关卡类型两分"改成**一条判据**，并注明"此前的两分法是客户端
  因为靶子没有队伍而自造的（`docs/150` §6.1）"；
* §2.1 的"回合上限用尽 ⇒ 判负"档位从 **推断** 改成 **逐位**（`0x4759f0`）；
* §3.2 的靶子半径从 **推断 0.5** 改成 **观测 0.5**（302/302）；
* §4.2 的"判定点未知"改成 **逐位**：回合末（`0x4759f0`），条件 `[self+0x11d]==1`，
  且"至少一个靶子还活着 ⇒ 全部复原"；
* §6 的 #1/#2/#5 里"未知"的条目按 §4/§7 更新。

**本轮没动它**（不在本任务的文件白名单里）。`docs/148` §3.1 的矛盾 #1 也可以关掉：
A 裁定成立，B 作废（B 是本项目客户端的实现口径，不是原版行为）。

---

## 7. 仍然是推断 / 未知

| # | 问题 | 状态 | 缺什么 |
|---|---|---|---|
| 1 | `[self+0x11d]`（"打不全就复原"的关卡位）是哪几关打开的 | **未知** | 关卡配置结构体的布局，或一次"部分命中"的原版录像。全 `.text` 里**没有**任何指令写这个字节 ⇒ 只能靠整块拷贝，静态侧很难追 |
| 2 | 6/8/10/13/17 之外的多目标关（尤其回放里叫 `Level 3` 的那关：3 个靶子、上限 10、描述里没写 "single shot"）是否也复原 | **未知** | 同 1 |
| 3 | 靶子"部分命中"的原版样本 | **0 份**（`docs/123` §4.3 复核不变） | 一次打掉一部分的录像 |
| 4 | 复原有没有视觉特效（火花） | **未知** | `0x46f310` 造靶子时发 `SparkEffect`；复原路径（`0x475c79`）**只**调 `0x454b40`，**没有**任何事件构造 ⇒ 至少**没有** `EntityEffectInfo`。我们的实现不发特效，与这条负面证据一致 |
| 5 | `Level 7`（上限 1）打空之后结算框长什么样 | **未知** | 原版真帧 |
| 6 | 同时回合制下"一发清两队" | **未观测** | 真值里 `0x475c0d` 给"两边都没人"写 `2`（没人赢），我们的 `winner: None` 与之一致；但**没有任何样本** |
| 7 | 玩家实体（`PlayerInfo`）不参与判定这条 | **推断**（依据：玩家没有 `LifeInfo`，`0x454950` 缺省返回 `2` 被跳过；语料 217/217 与之一致） | 一次"玩家在但士兵全灭"的完整录像（语料里有 18 份行为符合，但没有直接观测到判定瞬间） |
| 8 | `0x4759f0` 之后 `[self+0xc0]=1`、`[self+0xc4]=0x12c+[tick_mgr+0x58]` 的含义 | **未知** | 未查。看起来像"结算展示延迟 300 tick"，与我们无关 |

---

## 8. 复现

```bash
export W=/home/user/MyProject/gw2
cd $W && sha256sum libgraphwarrust.so          # 必须是 9a538115…abed187

# ① Q2：判定函数（Ghidra 地址 = 这里 +0x100000）
objdump -d -M intel --start-address=0x473520 --stop-address=0x4737a0 libgraphwarrust.so
objdump -d -M intel --start-address=0x4759f0 --stop-address=0x475f50 libgraphwarrust.so
objdump -d -M intel --start-address=0x472400 --stop-address=0x472470 libgraphwarrust.so  # 上限设置

# ② Q1：造靶子（构造，不是判定）
objdump -d -M intel --start-address=0x46f310 --stop-address=0x46f690 libgraphwarrust.so

# ③ Q1：21 个调用点全是 `mov esi,1`（= Team2）
python3 - <<'PY'
import pickle, bisect
ins=pickle.load(open('analysis/insns.pkl','rb'))
xr =pickle.load(open('analysis/xrefs.pkl','rb'))
st =sorted(pickle.load(open('analysis/funcstarts_ghidra_va.pkl','rb')))
own=lambda a: st[bisect.bisect_right(st,a)-1]
for a in sorted(e[0] for e in xr[0x9bbed8]):      # GOT 槽 → 0x46f310
    p=a
    for _ in range(14):
        p-=1
        if p in ins and ins[p][0]=='mov' and ins[p][1].startswith('esi'):
            print(hex(a), 'in', hex(own(a)), ins[p][1]); break
PY

# ④ 语料统计（只读；脚本见下）
#   对每份回放：取 TargetInfo 的 id → 看 TeamInfo / SoldierInfo / PosInfo.radius，
#   并在 EndingGame 那一刻统计"每队存活士兵数 + 存活靶子数"（玩家实体不计）。
#   §1.1/§3 的六张表就是这支脚本的输出（`.agent-work/` 里的临时脚本已按约定不入库）。

# ⑤ 我们的实现
cd <你 clone 的 gw2-rebuild>          # 本文在一份隔离开的克隆上跑，`.agent-work/` 用完即删
export CARGO_HOME=$W/.cargo-home CARGO_TARGET_DIR=$W/.agent-work/winrule/target
cargo test --offline                       # 373 passed（含 tests/win_rule.rs 的 10 条）
                                          # ⚠️ 审计 2026-11-08：373 是本文写作时的**当轮读数**；
                                          #    现行 1585 / 0 failed（2026-11-08 审计实测，69 个测试二进制）
cargo build --offline && cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/
"$GODOT" --headless --path client --import
CARGO_TARGET_DIR=$CARGO_TARGET_DIR bash client/run.sh selftest    # 658 passed, 0 failed
                                          # ⚠️ 审计 2026-11-08：658 是**当轮**读数；现行 709 / 0
                                          #    （前置同 §：assets + serve_match，见 `archive/156-client-rounds.md` §49.7）
```

> ⚠️ `selftest` 的两条联机断言只有在 `CARGO_TARGET_DIR` 指向**已构建**的
> `examples/serve_match` 时才会跑；不给就是 `658 passed, 2 skipped`（当轮口径；现行"全齐"是 709）。
