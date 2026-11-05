# 123 · 目标的「打不全就全部复原」规则（+ 其它逐关特殊规则）

状态：**现行**。本文回答三件事：

1. 用户口述的 **"一击必须打掉全部目标，否则已摧毁的目标全部复原"** 这条规则，
   在二进制里到底能定位到什么、定位不到什么；
2. **数据侧**（385/397 份真实回放）能不能看见这条规则；
3. 除了它，还有哪些**逐关特有**的机制（分「有证据 / 只有口述 / 未知」三档）。

> 真值来源：`/home/user/MyProject/gw2/libgraphwarrust.so`（sha256 `9a538115…abed187`，**只读**）
> 与 `~/.local/share/godot/app_userdata/Graphwar/replays/`（**只读，不入库**）。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。
> 前置：`118-campaign-data.md`（21 关参数表）、`114-event-tags.md`（事件判别值）、
> `30-kill-semantics.md`（致死判定）、`45-turn-count-and-ending.md`（结束条件）、
> `86-simultaneous-turns.md`（同时回合制）。
>
> ⚠️ **地址约定**：本文一律用 **ELF 虚地址**（= `objdump` 打印的地址）。
> Ghidra/REA 地址 = 本文地址 **+ 0x100000**（只加一次）。
> 反向换算：**`.text` 的 vaddr = 文件偏移 + 0x1000**；`.rodata` 的 vaddr **等于**文件偏移
> （因为 `LOAD #2` 是 `file 0x37a940 → vaddr 0x37b940`，见 `readelf -lW`）。
> 这条和 `docs/118` 里"文件偏移 = 虚地址"的写法差 0x1000，**只看 `.rodata` 时两者恰好相等**，
> 所以以前没暴露；本轮第一次大量读 `.text` 原始字节，必须记住这个差。

证据分级（沿用 `AGENTS.md`）：
**逐位验证** = 能逐字节/逐指令对上；**观测** = 能指到地址、字节或回放行；
**推断** = 写明缺什么；**未知** = 没有证据。

---

## 0. 一页结论

| # | 问题 | 结论 | 等级 |
|---|---|---|---|
| ① | 规则存在吗 | **存在**（用户口述 + 描述文本 5/5 + 没有它关卡就不成立，见 §1.2） | 口述 + 推断 |
| ② | 在目标管理器里吗 | **不在**。`graphwar_target_manager.rs` 是**很小的模块**（panic 行号只到 84），且它只订阅两个事件，没有任何"复原/重置"代码 | **逐位验证**（§2.5、§3.1） |
| ③ | 复原是**重建实体**吗 | **不是**。造目标的 `0x46f310` 只被**关卡开场**调用（13 个调用点全在 level-start 里）；回放里目标死亡**从不发** `EntityRemoved`（109 份 0 次） | **逐位验证** + **观测** |
| ④ | 那只能是**翻 alive 标志** | 是。目标只有两种状态事件：`TargetInfo`(23) 与 `LifeInfo`(27)；复原只能表现为 `LifeInfo{alive:true}` | 推断（缺：复原的构造点） |
| ⑤ | **有没有 broadcast** | **必须有**。回放记录的正是这条事件流，客户端靠它渲染；服务端不广播则客户端不可能画出"复原" | 推断（结构上唯一自洽解） |
| ⑥ | 触发时机 / 判据 | **未知**。语料里 **0 发部分命中**（§4.3），静态侧**没找到**判定点（§3.2 列了三条候选与各自缺的证据） | 未知 |
| ⑦ | 障碍的洞会不会一起恢复 | **不会**。整局里 `ClearObstacles` **只出现一次**（关卡开场），掩膜是"挖洞"不是"重建"（`docs/39`），目标规则碰不到它 | **观测**（397 份 0 次中途清除） |
| ⑧ | 同时回合制下「一次射击」有歧义吗 | **实际没有**。5 个"一次性"关卡 `Level 6/8/10/13/17` **全部是 `SequentialTurns`**（回放 21/21 一致） | **观测** |
| ⑨ | 还发现哪些逐关规则 | 见 §5。新增的有证据项：**胜利判据 = `Team2` 全灭**（45/45）、**`Level 7` 只有 1 回合**、**单发关卡全在顺序回合** | 观测 |

**一句话**：规则的**存在性与形状**已经钉住（翻 alive + 广播 + 不重建 + 不恢复洞），
但**判定点（哪一刻、拿什么条件）本轮没有找到**，语料里也没有一次部分命中可以反推。
§6 给了实现建议与"未证实项怎么落地而不撒谎"的做法。

---

## 1. 起因与范围

### 1.1 用户口述（本轮唯一的"规则级"输入）

> "关卡应该没问题，除了有些地方少写了东西。比如**第六关要你一次性命中所有目标，
> 只命中部分则目标会全部重置**。"

补充确认（父 agent 转述）：

> 第 6/8/10/13/17 关描述的 "in a single shot" **都是同一个意思**——目标必须在一次射击内
> 全部打掉，只命中一部分则已摧毁的目标全部重置。**按一条通用规则建模**，不需要逐关处理。

### 1.2 为什么"这条规则一定存在"（三重自洽）

| 证据 | 内容 | 等级 |
|---|---|---|
| 描述文本 | `Level 6/8/10/13/17` 的 21 段描述里，"in a single shot" 出现 **5 次**，与"多目标 + 有限回合"的关卡一一对应（`docs/118` §4.2） | **逐位验证** |
| 胜利判据 | 胜利 = **`Team2` 全灭**（§5.1），目标就生在 `Team2`。若没有"打不全就复原"，2 目标关卡只要 2 发就赢，`Level 6` 的 `turn_limit = 6` 就毫无意义 | 观测 + 推断 |
| 回合上限 | 5 个"一次性"关卡的回合上限是 **6 / 6 / 6 / 6 / 5**（§4.2），明显是"给你 5~6 次试错"，而不是"允许你分 2 次打完" | **观测** |

⇒ 规则存在的**行为层**证据充分；缺的是**代码层定位**。

### 1.3 本轮不回答的

* 4.7 版（用户手上的新构建）与 4.2.2 版（`/home/user/GW`）的差异 —— 本机只以
  `9a538115…` 这一份为准，**没有**去查别的构建。
* 联机对手关（`Level 5/9/11/14/15/18/19/20`）没有目标，与本规则无关。

---

## 2. 目标在二进制里的形态（结构证据）

### 2.1 目标就是普通实体，只是被标了「我是目标」

回放逐条（`1782640919` = `Level 6`，tick 15，关卡开场）：

```
TargetInfo        {target_id: 1099}          ← ① 登记"这是目标"
TeamInfo          {entity_id: 1099, team: Team2}
SkinGraphicsInfo  {entity_id: 1099, graphics_str: "target"}
EntityEffectInfo  {entity_id: 1099, effect_type: SparkEffect}
...（1100 同样四条）
```

死亡也是普通实体死亡：

```
LifeInfo {entity_id: 1099, alive: false}
```

⇒ 目标**不是**独立的一类对象，而是 `Team2` 里一个带 `SkinGraphics="target"` 的实体。
⇒ 胜利判据用的是**队伍全灭**，不是"目标计数归零"（§5.1）。

### 2.2 全二进制只有**两个**地方会构造 `TargetInfo` 事件

`TargetInfo` 的判别值 = **23**（`docs/114`），tag word = `0x8000000000000017`。
全文件搜这个 8 字节常量，**只有 2 处**：

| 地址 | 所在函数 | 作用 |
|---|---|---|
| `0x46f3af` | `0x46f310` | **造一个目标**（`docs/118` §12.3 就是数它的调用次数得到目标数） |
| `0x46580e` | `0x465780` | **把一个集合里的每个 id 都发一遍 `TargetInfo`** |

等级：**逐位验证**（原文件只有这两处 `movabs`，见 §8 复现脚本）。

`0x46f310` 的调用点（经 GOT 槽 `0x9bbed8`，`xrefs` 全表）：

```
0x499ae0 0x49a6c0 0x49b0f0 0x49b630 0x49bca0 0x49c200
0x49c2b0 0x49f87a 0x4a1290 0x4a255b 0x4a3740 0x4a87c4 0x4a97e0
```

**13 个，全部落在 `0x499ae0`–`0x4a97e0` 区间**，正是 21 个 `start_level` 分支的地址段
（有目标的 13 关：0/1/2/3/4/6/7/8/10/12/13/16/17，与 `docs/118` §7.1 的目标数表 13/13 吻合）。
⇒ **目标只在关卡开场被造一次**，运行期**没有任何"重新造目标"的调用点**。
⇒ 复原**不是重建实体**（这一条把问题从"三种可能"缩到"一种"）。

等级：**逐位验证**（GOT 槽 + 13 个调用点的地址全在 level-start 段）。

### 2.3 `0x465780` 的真身：**不是 SIMD 像素扫描**，是 hashbrown 集合迭代器

`docs/02` §13.4 当年记的是"典型 SIMD 字节扫描，可能在扫 1000×600 掩膜，未证实"。
**本轮推翻了它**：这段代码是 Rust `HashSet<u32>` 的**迭代 + 逐元素发事件**。

逐指令（`0x465780`–`0x46583c`，`objdump -d -M intel`）：

```asm
46578e  mov   r15,[rdi+0x30]        ; = HashSet.items（环形缓冲里还剩几个元素）
465792  test  r15,r15 ; je 46583e   ; 空表直接返回
46579b  mov   r12,[rdi+0x8]         ; 输出端：&mut Vec<Event>
46579f  mov   r13,[rdi+0x18]        ; = HashSet.ctrl（hashbrown 的 16 字节控制组指针）
4657a3  movdqa xmm0,[r13]
4657a9  pmovmskb ebx,xmm0           ; 每个 ctrl 字节的 bit7 → 一个 bit
4657ad  not   ebx                   ; 取反 ⇒ 置位 = 槽位「有人」
4657af  lea   rbp,[r13+0x10]
...
4657f1  tzcnt eax,ebx               ; 最低置位 = 下一个非空槽（哈希下标）
4657f5  shl   eax,2                 ; ×4 = u32 的字节宽
4657fb  sub   rcx,rax               ; bucket(i) = data_end − (i+1)*4  ← hashbrown 的倒挂布局
465807  mov   eax,[rcx-0x4]         ; 取出一个 u32 = target id
46580e  movabs rax,0x8000000000000017   ; tag = target_info_event(23)
465818  mov   [rsp],rax ; mov [rsp+8],eax
465824  call  qword ptr [rip+…] # 0x9bac80   ; Vec<Event>::push
465834  dec   r15 ; jne 4657c0      ; 直到 items 用完
```

判据（每条都可逐位复核）：

* `pmovmskb` + `not` + `tzcnt` + `dec items` 是 hashbrown `RawIter` 的标准形状；
* `shl eax,2` 与 `[rcx-4]` 说明**元素是 u32**，且桶是"从 `data_end` 往回"编号的
  （hashbrown `Bucket::from_base_index = base − (i+1)·size`）；
* `[rdi+0x30]` = items、`[rdi+0x18]` = ctrl、`[rdi+0x10]` = bucket_mask ⇒
  **HashSet 从 `rdi+0x10` 开始**，`[rdi+0x8]` 是输出 `&mut Vec<Event>`；
* 没有任何缓冲区长度、没有 `1000×600`，**和掩膜无关**。

⇒ `0x465780(&self, out)` 的语义 = **"把已知的全部 target id 重新播一遍 `TargetInfo`"**。
`docs/02` §13.4 的"SIMD 掩膜扫描"猜测**就此划掉**。

等级：**逐位验证**（指令与常量都可指认）。

### 2.4 目标管理器只订阅**两个**事件

`0x465910`（`graphwar_target_manager.rs` 的构造/注册函数）做两件同样的事，各一次：

```asm
46597c  lea rsi,[rip-0x2af0d6]  # 0x1c18ad      ; "new_connection_event"   (len 0x14 = 20)
465983  lea r8,[rip+0x4f24fe]   # 0x957e88      ; panic::Location
46598a  mov edx,0x14
465995  call qword ptr [rip+…]  # 0x9bac88      ; <注册一个 8 字节闭包>
...
4659cd  lea rsi,[rip-0x2fd5e0]  # 0x1c18c1      ; "entity_removed_event"   (len 0x14 = 20)
4659d6  mov edx,0x14
4659e6  call qword ptr [rip+…]  # 0x9bac88
```

两个字符串在 `.rodata` 里首尾相接（`0x1c18ad` 之后正好 `+20 = 0x1c18c1`），
长度都是 `0x14`，与 `docs/114` 的 `new_connection_event` / `entity_removed_event` 同名。

配套的另一个函数 `0x465870` **断言**收到的 tag 是 `0x800000000000000e`（= 14 =
`entity_removed_event`，不等就 panic），然后把 id 从集合里删掉。

⇒ 目标管理器的完整语义只有三条：

| 触发 | 动作 | 证据 |
|---|---|---|
| `target_info`(23) 到达 | 把 id 插进集合 | `0x3e3197` 分支里 `0x43e390(set+0x30, id)` |
| `entity_removed`(14) 到达 | 把 id 从集合删掉 | `0x465870`（tag 断言 + `0x47dec0`/`0x486e20` 两次 remove） |
| **新连接**到达 | **把集合里所有 id 各发一条 `TargetInfo`** | §2.3；由"新连接时才需要补发"解释 |

⇒ **`0x465780` 跑在"新连接"上，不是跑在"重置"上。**
（这也解释了为什么它需要一个集合 + 一个输出 Vec：每个新连接一个自己的待发队列。）

等级：`0x465910` 的两个字符串与长度 = **逐位验证**；
"`0x9bac88` = subscribe_event" = **推断**（缺：`0x9bac88` 槽的 addend 未解析到具名函数）；
"`0x43e390` 是 insert" = **推断**（形状是 `HashSet::insert`，未逐条核）。

### 2.5 `graphwar_target_manager.rs` 是个**很小的模块**

用 `src/graphwar_target_manager.rs`（`.rodata` `0x1a569d`，32 字节）在 `.data.rel.ro`
里的 `panic::Location` 记录反查（`Location{ptr,len,line,col}`），全二进制只有 **4 条**，
行号 **35 / 65 / 74 / 84**：

| Location | 行:列 | 引用它的函数 |
|---|---|---|
| `0x957e40` | 65:26 | `0x46f596`（`add_target` 里内联的取锁器）、`0x3e4bd8`（分派器里内联的同一个取锁器） |
| `0x957e58` | 74:30 | `0x46584d`（`0x465780` 的 RefCell 重复借用 panic） |
| `0x957e70` | 84:18 | `0x4658bc`（`0x465870` 的 tag 断言） |
| `0x957ed8` | 35:24 | `0x465a21`（`0x465910` 的 RefCell panic） |

⇒ 这个源文件**不到 ~100 行**，只包含"一个 `HashSet<u32>` + 三个入口"，
**装不下任何"判定/复原"逻辑**。

等级：**逐位验证**（`Location` 记录的 line/col 直接从 `.data.rel.ro` 读出）。
**注意**：`0x3e2830`（事件分派器）之所以也被归到本模块（`docs/02` §13.1 的旧结论），
是因为它**内联了**本模块第 65 行的取锁器，只贡献 1 条 panic 位置 ⇒
"`0x3e2830` = target_manager"这条老结论**应当降级为"不确定"**（§3.1 注）。

---

## 3. 重置规则：能确定什么、不能确定什么

### 3.1 先排除：不在目标管理器里

§2.5 已经把"目标管理器里有判定/复原"这条路堵死。剩下的候选只有三个模块：
`graphwar_game_state_manager.rs`（63 个函数，含 `EndingGame` 判定）、
`graphwar_level_manager.rs`（71 个函数，21 个关卡分支 + 关卡驱动 `0x466d20`）、
`graphwar_life_manager.rs`（9 个函数，`LifeQueuedDeath` 的唯一构造点 `0x453f6d` 在此）。

顺带修正一条老结论：`docs/02` §13.4 / `docs/06-verification.md` 把 `0x3e2830` 归到
`graphwar_target_manager.rs`，依据是 `fixtures/module_funcs.tsv`——而 `docs/118` §12.6 坑 2
已经证明那张表用的是 panic 串启发式、不可靠。用本轮的行号法复核，`0x3e2830` **只有 1 条**
panic 位置指向 target_manager，且那一条可以被"内联取锁器"完整解释 ⇒
**`0x3e2830` 归属未知，但肯定不只是一个 target manager**（它处理 40 多种事件）。
这条不影响本文结论，只记录，避免以后又拿它当证据。

### 3.2 逐项回答父 agent 的五个问题

| # | 问题 | 回答 | 等级 / 缺什么 |
|---|---|---|---|
| 1 | **重置在哪一刻** | **未知**。三个候选：<br>(a) 射击结算（`ExecutingResults` 阶段、`LifeInfo` 全部落完之后）<br>(b) `end_turn` 时（和 `TurnCountInfo` 同批）<br>(c) 下一次开火前<br>→ 语料里没有一次部分命中，回放的**回合边界块**（`TurnCountInfo`/`TurnInfo`/`EndOfTurnInfo`/`GameStateInfo` 四条，§4.4）**不含任何 Life/Target 事件**，所以连"重置事件长什么样、插在哪"都没有样本 | 未知（缺：一次部分命中的回放，或静态判定点） |
| 2 | **判据** | 最强推断：**`本发摧毁的目标集合 ≠ 全部目标` ⇒ 全部复原**。理由是"一次射击"是描述原文，而目标是 `Team2` 的全集；等价说法是"结算后 `Team2` 仍有存活 ⇒ 全部复原"。<br>**"用了几发"这一维度不需要**——因为 5 个关卡全是顺序回合（§5.3），一发就是一回合 | 推断（缺：判定点；缺：部分命中样本） |
| 3 | **实现方式** | **翻 alive 标志 + 广播 `LifeInfo{alive:true}`**。三条硬约束：<br>① 造目标的 `0x46f310` 运行期无调用点（§2.2）⇒ 不重建；<br>② 目标死亡只发 `LifeInfo`，109 份回放里**没有任何** `EntityRemoved`/`RemoveTeam` 提及 target id（§4.3）⇒ 目标不销毁；<br>③ 目标只有 `TargetInfo`/`LifeInfo` 两种状态事件（§2.2、§2.4） | 推断（②③是逐位/观测，①的推论很强） |
| 4 | **有没有 broadcast** | **必须有，而且必然走事件流**。理由：回放记录的就是客户端收到的事件流（`docs/96` §4：线上是外部标签 JSON），目标"复原"这件事客户端要能看见，只能靠一条 `LifeInfo{alive:true}`。<br>**但直接证据为零**：397 份回放里**没有任何一次**"先 `alive:false` 后 `alive:true`"（唯一的假阳性都在 `Setup` 阶段，§4.3） | 推断（缺：部分命中样本） |
| 5 | **障碍的洞会一起恢复吗** | **不会**。① 洞是"挖"出来的（`docs/39`：`fill=0xFF, r=0.5`），掩膜**整局只建一次**；② 397 份回放里 `ClearObstacles` 在 `TurnCountInfo` 之后**出现 0 次**（全部发生在关卡开场）；③ 目标管理器不持有任何掩膜句柄（§2.4 只有两张表） | **观测** |

### 3.3 为什么静态侧这么难找（一条方法论记录，**含一个必须记住的坑**）

事件 tag word 在**大部分函数里不是常量**：编译器把基底 `0x7fffffffffffffff` 用一条
`movabs` 装好，之后每次只 `add reg,N`。

```
tag word = 0x8000000000000000 | tag
         = 0x7fffffffffffffff + (tag + 1)
⇒ add 的立即数 N 与 tag 差 1：        tag = N − 1
```

例：`0x3e2830` 的 `target_info` 分支

```asm
3e2844  movabs r13,0x7fffffffffffffff     ; 函数开头，只此一条
3e31f3  add    r13,0x18                   ; 0x7fff…+0x18 = 0x8000000000000017 = TargetInfo(23)
3e31f7  mov    [rsp+0x30],r13
3e31fc  lea    rsi,[rsp+0x30]
3e3201  call   qword ptr [rip+…] # 0x9bac80
```

**本轮在这里栽过一次**：先用 `N = tag` 读表，结果把 `pos_info(26)` 的构造点
（`0x3c653b` `movabs r12,0x7fff…` + `0x3c659f` `add r12,0x1b`）误判成 `life_info(27)`——
`+0x1b` 给的是 `0x1a = 26`。判据：`pos_info` 的载荷正是
`{id, 2 doubles}` + `{radius: f64}`（16 B + f64 + u32），与 `life_info` 的 `{id, alive}` 不同。
⇒ **凡是靠 `+N` 认 tag 的地方，必须拿载荷形状再交叉对一次。**

`life_info(27)` 的正确构造点（`add …,0x1c`，全文件 15 处，其中 2 处在
`graphwar_life_manager.rs` 的函数区间内）：

```asm
4542a5  movabs rax,0x7fffffffffffffff
4542a1  mov    [rsp+0x2c],al          ; alive = 从实体表里读出的一个字节
45429c  mov    [rsp+0x28],r12d        ; entity_id
4542af  add    rax,0x1c               ; = 0x800000000000001b = LifeInfo(27)
4542b3  mov    [rsp+0x20],rax
4542c1  call   qword ptr [rip+…] # 0x9bac80   ; push
```

以及第二处 `0x454c52`（同一模块），alive 来自**入参 `cl`** 而不是表——
即"**按参数设置某个实体的存活位**"这一路。**若要找"复原"，这一路是唯一入口**
（见 §3.5）。

⇒ **"搜 `0x8000…|N` 常量"只能覆盖一部分构造点**。本文所有"只有 N 处"的断言都做了
**两条路**的交叉验证（8 字节常量搜索 + `add reg,imm8` 扫描），复现脚本见 §8。

另一条：`0x3e2830` 的分派表是 `.rodata 0x1beda8` 的 **70 项 `i32` 相对偏移**，
`index = tag − 2`（覆盖 tag 2…0x47；tag 0/1 由调用方先处理）。全表见附录 A。

### 3.4 静态侧**没有**找到的东西（负面清单，防止重复劳动）

以下都在本轮逐条查过，**都没有**"打不全就复原"的痕迹：

| 查过 | 结果 |
|---|---|
| `0x3e2830` 分派器全部分支（70 项） | 每个分支把**它自己那条事件原样转发**（`tag = N−1` 复核过 17 处构造点：69/46/56/29/45/23/71/48/14/15/13/7/44/61/33/57/58）。**`target_info`(23) 只转发自己那一份**，**没有任何分支构造 `life_info`(27)**（全函数没有 `add r*,0x1c`）⇒ **分派器既不复原生命、也不补发目标表** |
| `0x465780` / `0x465870` / `0x465910` | 见 §2.3–2.5，全是"新连接补发 + entity_removed 删表" |
| `0x46f310`（造目标） | 只有开场调用点（13 个，§2.2） |
| `graphwar_target_manager.rs` 的 4 条 panic 位置 | 行号 ≤ 84，模块太小，装不下（§2.5） |
| GOT 槽 `0x9bbed8` / `0x9bbee0` / `0x9bbee8`（目标管理器的方法槽） | `0x9bbed8`→`0x46f310`（13 个调用点，全在 level-start）；`0x9bbee0`→`0x46f690`（22 个调用点，含 level-start 与 `0x47483d`=关卡开场收尾）；`0x9bbee8`→`0x46f740`（1 个）。**没有"回合/结算"侧的调用者** |
| 21 个关卡分支里的 `mov byte [rsp+X],1` 立即数 | 出现过的 `(偏移, 值)` 只有 `+0x30=5 / +0x31∈{0,1} / +0x32=1 / +0x50∈{0,1,5} / +0x68∈{1,2,5} / +0x70=5`，**没有**一个"恰好 5 关为 1、且这 5 关 = {6,8,10,13,17}"的布尔量 ⇒ 该规则的开关**不在**关卡分支的立即数里（或不是布尔量） |

### 3.5 下一步该往哪查（本轮预算用尽处的交棒点）

`life_info(27)` 全文件只有 **15 处** `add …,0x1c`，其中落在
`graphwar_life_manager.rs`（函数区间 `0x453b50`–`0x454c00`）里的只有 **2 处**：

| 站点 | 形态 | 语义（推断） |
|---|---|---|
| `0x4542af` | `alive = byte [实体表 + i*8 − 4]` | **读实体表、公布当前存活状态**（回合/开局快照走这条） |
| `0x454c52` | `alive = cl`（入参） | **按参数设置某个实体的存活位** |

⇒ "复原"只可能走 `0x454c52` 这一路。要坐实需要：
1. 找 `0x454c52` 所在函数（`0x454c00` 起）的**调用者**，看有没有"回合结算/射击结算"侧的上游；
2. 看它的入参 `cl` 在什么条件下算成 `true`；
3. 若上游是 `graphwar_game_state_manager.rs`（`EndingGame` 判定所在），
   则"关卡规则位"也应在那附近。

其余 13 处 `add …,0x1c`：10 处在 21 个关卡分支的地址段（`0x499ae0`–`0x4a97e0`，即
**关卡开场把所有实体置 alive**）、3 处在 `0x8fd…`/`0x908…`（远段，未查）。

---

## 4. 数据侧：397 份真实回放

### 4.1 语料

```
~/.local/share/godot/app_userdata/Graphwar/replays/   397 个文件（gzip+JSONL）
  其中含 TargetInfo 的                                  109 个
  其中出现 EndingGame 的                                 45 个
  覆盖关卡                                              21/21
```

事件总量（前 12）：`FunctionInfo 29030 / EntityEffectInfo 6858 / LifeInfo 6533 /
TeamInfo 6407 / GameStateInfo 5400 / PosInfo 5306 / SoldierInfo 4524 /
SkinGraphicsInfo 4502 / AddObstacle 4384 / … / TargetInfo 250 / TurnLimitInfo 21`

⚠️ `docs/118` §12.5 写的是 **385** 份；本机实际是 **397** 个文件。
差别来源未查（可能后来又存了几局）。**统计结论不受影响**（下表的每一关样本数都 ≥1）。

### 4.2 逐关参数：与 `docs/118` §7.1 **21/21 完全一致**（独立复核）

按 `GameNameInfo` 分组、取"开场段"统计（**0 基关卡号**，`docs/118` 表里的"关 N" = 本表 `N−1`）：

| Level | 目标数 | 回合上限（`TurnLimitInfo`） | turn / func / axis | 样本数 |
|---|---|---|---|---|
| 0 | 1 | **无该事件**（无上限） | Sequential / Normal / EveryUnit | 31 |
| 1 | 1 | 5 | Sequential / Normal / EveryUnit | 3 |
| 2 | 1 | 5 | Sequential / Normal / EveryUnit | 2 |
| 3 | **3** | 10 | Sequential / Normal / EveryUnit | 3 |
| 4 | 1 | 10 | Sequential / Normal / EveryUnit | 2 |
| 5 | 0 | 无 | Sequential / Normal / EveryUnit | 3 |
| **6** | **2** | **6** | **Sequential** / Normal / EveryUnit | 2 |
| 7 | 1 | **1** | Sequential / Normal / EveryUnit | 1 |
| **8** | **3** | **6** | **Sequential** / Normal / EveryUnit | 1 |
| 9 | 0 | 无 | Sequential / Normal / EveryUnit | 2 |
| **10** | **2** | **6** | **Sequential** / Normal / **EveryFive** | 2 |
| 11 | 0 | 无 | **Simultaneous** / Normal / EveryFive | 1 |
| 12 | 1 | 5 | Sequential / Normal / OnlyMain | 1 |
| **13** | **2** | **5** | **Sequential** / Normal / OnlyMain | 2 |
| 14 | 0 | 无 | **Simultaneous** / Normal / OnlyMain | 1 |
| 15 | 0 | 无 | Sequential / Normal / NoAxis | 40 |
| 16 | 1 | 无 | Sequential / **DiffEq** / EveryUnit | 57 |
| **17** | **2** | **6** | **Sequential** / **DiffEq** / EveryUnit | 2 |
| 18 | 0 | 无 | Sequential / DiffEq / EveryUnit | 5 |
| 19 | 0 | 无 | Sequential / DiffEq / OnlyMain | 16 |
| 20 | 0 | 无 | Sequential / DiffEq / NoAxis | 26 |

结论三条（都是本轮新增的**观测**）：

1. **`Level 7` 的回合上限 = 1**，与描述 "Destroy your target **in your first try**" 精确对应；
   这也是语料里**唯一**接近"上限用完"的关卡（§4.5）。
2. **"无上限" = 完全不发 `TurnLimitInfo` 事件**（不是发一个 0）——`Level 0`、全部对手关、
   全部 `DiffEq` 单人关（17 除外）都是这样。`docs/42` §3 说的 `TimerInf → 0` 是**时间档**，
   与**回合上限**是两件事，别混。
3. **5 个"一次性"关卡 `6/8/10/13/17` 全部是 `SequentialTurns`** ⇒ 同时回合制下
   "一次射击"的语义歧义（父 agent 的问题 4）**在实际关卡里不存在**。
4. **21/21 关的 `TimeModeInfo` 都是 `TimerInf`**（`end_of_turn_tick = 0`，即**没有回合倒计时**）。
   这条**与 `docs/118` §4.3 的 `byte3 = 0x05`「时间档固定为 5」冲突**——
   回放是运行时真值，`0x05` 那个字节**不是**时间档（`docs/118` 原文也标的是「推断」）。
   ⇒ 战役关的"回合时限"其实不存在，只有"回合上限"（`TurnLimitInfo`）；
   这两件事以前被混在一起过。

### 4.3 三个**负结果**（这是本轮最重要的数据结论）

#### ① 部分命中：**0 次**

在多目标关卡（`Level 3/6/8/10/13/17`）上，语料共有 **7 次"打到了东西"的射击**，
**7 次全部一次打掉全部目标**：

| 回放 | 关卡 | 目标 | 该发打掉的 |
|---|---|---|---|
| `1782640692` | 3 | 444,445,446 | **3/3** |
| `1782640919` | 6 | 1099,1100 | **2/2** |
| `1782641222` | 8 | 1531,1532,1533 | **3/3** |
| `1782760565` | 10 | 356,357 | **2/2** |
| `1782760774` | 13 | 1225,1226 | **2/2** |
| `1782763110` | 17 | 2954,2955 | **2/2** |
| `1783254892` | 17 | 564,565 | **2/2** |

其余射击**一个目标都没打到**（`Level 3/6/10/13` 还各有 1 份回放**一发都没开**）。
⇒ **语料里不存在"打掉一部分"的样本**，无法从数据反推判定点。

#### ② "复活"（`alive:false` 之后又 `alive:true`）：**0 次**（gameplay 内）

全 397 份扫 `LifeInfo` 序列，**有** 48 次"先死又活"，但**全部**发生在 `Setup` 阶段：
实体先被登记成 `alive:false`，随后在同一段 `Setup` 里被登记成 `alive:true`
（例：`1782927590`，`EU Room 2`，t=44010 假死 → t=45635 复活，然后是 `TurnCountInfo 0`）。
**没有任何一次**发生在 `WaitingForFunctions` 之后的实战回合里。

#### ③ 目标被 `EntityRemoved`：**0 次**

109 份含目标的回放里，`EntityRemoved` / `RemoveTeam` **从未**提及任何 target id
（它们只用于开场删掉临时士兵，例如 `1782640919` 的 `entity_id 1097`）。
⇒ 目标**不会被销毁**，只翻 `alive`。

### 4.4 状态快照**只在开局播一次**

`1782640919`（`Level 6`）tick 15 的那一大段（索引 65–99）是**整局状态广播**：

```
GameNameInfo → TurnInfo → EndOfTurnInfo → GameStateInfo(WaitingForFunctions)
→ PosInfo×3 → ColorInfo → TeamInfo×4 → AxisModeInfo/FunctionModeInfo/TimeModeInfo
→ EntityEffectInfo×3 → TurnModeInfo → SkinGraphicsInfo×2 → TargetInfo×2
→ LifeInfo{alive:true}×3 → SoldierInfo → PlayerInfo → NameInfo → Skin/Face/Hat
```

而**第 2 个回合边界**（`1782640520` 的 turn 1，索引 113–116）只有四条：

```
TurnCountInfo → TurnInfo → EndOfTurnInfo → GameStateInfo(WaitingForFunctions)
```

⇒ 后续回合**不重播** `TargetInfo` / `LifeInfo`。
⇒ 如果目标的复原要"让客户端知道"，**不能**指望这个快照，必须是显式的一条 `LifeInfo{alive:true}`。
（这也是 §3.2 问题 4 的推理链。）

### 4.5 结束条件：45/45 都是 `Team2` 全灭

```
109 份含目标的回放里，45 份出现 GameStateInfo{EndingGame}
这 45 份里，"最终仍存活的目标"数量 = 0（每一个 target id 最后一条 LifeInfo 都是 false）
```

⇒ 与 `docs/45` §2 的"某队全灭 ⇒ `EndingGame`"一致，**并且**说明：
**目标未全灭时游戏不会结束**（否则 45 份里也该有"目标还活着就 EndingGame"的样本）。

`Level 7`（上限 1）是唯一"上限可能被用到"的关卡；它的样本 `1782641048` 是**首发命中**
（`turn_count` 只到 1，且以 `EndingGame` 收场）⇒
**"回合上限用完但目标没死算什么"仍然没有样本**（§5.3）。

---

## 5. 其它逐关特殊规则清单

### 5.1 有证据的（本轮或既有文档，能指到地址/回放）

| # | 规则 | 证据 | 等级 |
|---|---|---|---|
| 1 | **胜利 = `Team2` 全灭**（不是"目标数归零"）。目标全部生在 `Team2` | 45/45 回放；`0x46f310` 里 `TeamInfo{Team2}`；`docs/45` §2 | **观测** |
| 2 | **`Level 7` 只有 1 个回合**（"in your first try"） | 回放 `TurnLimitInfo = 1`；`docs/118` §7.1 的 `S` 路 `(1,1)` | **逐位 + 运行时** |
| 3 | **5 个"一次性"关卡都是顺序回合**，同时回合制与它们无关 | 回放 21/21（§4.2） | **观测** |
| 4 | **"回合上限"有/无是两态**：无上限就是**不发** `TurnLimitInfo` | 回放（`Level 0/5/9/11/14/15/16/18/19/20` 一次都没发） | **观测** |
| 5 | **每回合射手轮转**（多士兵关：3/5/9/11/14/15/16?、19/20）；单士兵关不轮转 | `docs/98` §…；回放里 `TurnInfo.soldier_id` 的轮换 | 观测（既有结论） |
| 6 | **障碍被摧毁 = 挖洞**（`fill=0xFF, r=0.5`），不是移除；**整局不重建** | `docs/39`；回放里 `ClearObstacles` 只在开场出现（397 份 0 次中途） | **观测** |
| 7 | **击穿障碍不影响弹道**：掩膜只放障碍，士兵单独做"经过即死" | `docs/30` | **逐位验证**（已有） |
| 8 | 同一关的不同样本**障碍数会变**（`min_count` 是下界） | `docs/118` §7.2；回放复现 | **逐位 + 运行时** |
| 9 | **战役关没有回合倒计时**：21/21 关 `TimeModeInfo = TimerInf`、`EndOfTurnInfo.end_of_turn_tick = 0` | 回放 21/21（§4.2 第 4 条） | **观测** |

### 5.2 只有用户口述的

| # | 规则 | 状态 |
|---|---|---|
| 1 | **"一击打掉全部目标，否则已摧毁的全部复原"**（`Level 6/8/10/13/17`） | 用户口述；本文 §3 给了形状推断，**判定点未知** |

### 5.3 完全未知的（有疑问但没有任何证据）

| # | 问题 | 为什么未知 |
|---|---|---|
| 1 | **回合上限用完、目标没打完 ⇒ 什么结果？** | 语料 0 样本（唯一上限=1 的 `Level 7` 首发就中）。可能的三种：`EndingGame`（判负）/ 重开本关 / 无限续。缺：一次 `Level 7` 打空的回放，或 `graphwar_game_state_manager.rs` 里 `turn_count >= limit` 的判定点 |
| 2 | **复原时"全部复原"还是"只复原本发打掉的"** | 两者在"目标只死一次"的场景下等价；只有在**跨回合累积**时才分得开。缺：部分命中样本 |
| 3 | **复原会不会重置 `turn_count`** | `docs/45` 的回绕本身未观测，这里更没有 |
| 4 | **复原有没有视觉效果**（火花/闪烁） | 缺样本。`0x46f310` 造目标时会发 `EntityEffectInfo{…, SparkEffect}`；复原可能复用同一条，未证 |
| 5 | **同时回合制下"每人各打掉一部分"怎么算** | 5 个相关关卡全是顺序回合（§5.1 #3），**规则不必定义**；但若以后做自定义关，这是个坑 |
| 6 | **每关是否有"必须用某种函数形式"的限制** | 无证据。`FunctionMode` 是全局模式（`NormalFunction` / `DiffEqFunction`），由关卡设置，**不是**对单发函数形状的限制。`Level 16/17/18/19/20` 只是 `func=DiffEq` |
| 7 | **目标是否移动/重生/计时** | **不移动、不重生**。`1782640919`（`Level 6`）里 target `1099/1100` 的 `PosInfo` 只出现在两处：关卡开场布局（索引 49–59）与开局状态快照（索引 69–71），之后**再也没有**新的 `PosInfo`——如果目标会移动/重生，一定会继续有位置事件。计时也无痕迹（`Level 6` 的 `TimeModeInfo` 是 `TimerInf`）⇒ **观测** |

---

## 6. 对我们实现的建议

### 6.1 结论先说：**这条规则要加，而且只能加在"关卡规则层"**

不变量（已由 §2/§4 钉死，实现必须遵守）：

1. **不要重建实体**——不新分配 id、不发 `TargetInfo`、不发 `EntityRemoved`。
2. **只发 `LifeInfo{alive:true}`**（每个被复原的目标一条），字段与协议一致
   （`src/events.rs` 的 `LifeInfo`）。
3. **不要碰掩膜**（洞保留）。
4. **确定性**：复原集合与顺序必须是确定的（用 `BTreeSet`/`Vec` 保序，别用 `HashMap` 迭代序）。

### 6.2 建议的新模块与 API

```rust
// src/target.rs（新文件，约 80 行）
/// 目标的"集合 + 本发战果"。语义对应二进制里 graphwar_target_manager.rs 的两张表
/// （target_info 插入 / entity_removed 删除）**加上**用户口述的"打不全就复原"。
pub struct TargetSet {
    ids: std::collections::BTreeSet<i64>,   // 全部目标（只要还在场上）
    killed_this_shot: Vec<i64>,             // 本发打掉的，保序
    all_in_one_shot: bool,                  // 关卡规则位（Level 6/8/10/13/17 = true）
}

impl TargetSet {
    pub fn new(all_in_one_shot: bool) -> Self;
    /// 造一个目标 → 对应 0x46f310；返回要广播的事件（TargetInfo）。
    pub fn add(&mut self, id: i64) -> Vec<String>;
    /// entity_removed → 对应 0x465870。
    pub fn on_entity_removed(&mut self, id: i64);
    /// 一发结算时记录"这个目标是本发打掉的"（喂 LifeInfo 的地方）。
    pub fn note_killed(&mut self, id: i64);
    /// 新连接补发 → 对应 0x465780（**不是**重置！）。
    pub fn rebroadcast_all(&self) -> Vec<String>;
    /// 回合结算：判"本发是否打全"，打不全就把本发打掉的全部复原。
    /// 返回要追加到事件流里的 LifeInfo{alive:true}（可为空）。
    pub fn settle_shot(&mut self) -> Vec<String>;
    pub fn is_cleared(&self) -> bool;       // ids.is_empty()
}
```

`settle_shot()` 的实现（把 §3.2 的推断写成代码，**并在注释里写明这是推断**）：

```rust
pub fn settle_shot(&mut self) -> Vec<String> {
    let all_dead = self.ids.is_empty();
    if self.all_in_one_shot && !all_dead && !self.killed_this_shot.is_empty() {
        let back = std::mem::take(&mut self.killed_this_shot);
        for id in &back { self.ids.insert(*id); }
        return back.into_iter()
                   .map(|id| life_info_alive_json(id, true))   // 只发 LifeInfo
                   .collect();
    }
    self.killed_this_shot.clear();
    Vec::new()
}
```

### 6.3 接在哪（`src/` 里改两处）

| 位置 | 改什么 |
|---|---|
| `src/game.rs` `struct Match` | 加字段 `targets: TargetSet`（默认 `all_in_one_shot=false`），并加 `pub fn config_targets(&mut self, all_in_one_shot: bool)`（对应关卡分支里"造 N 个目标"的那一步） |
| `src/game.rs` `Match::end_turn()` | 在**发 `TurnCountInfo` 之前**追加 `self.targets.settle_shot()` 的事件；并把"是否 `EndingGame`"的判据改成 `self.targets.is_cleared()`（当 `!ids.is_empty()` 时不结束）——与 `docs/45` 的"某队全灭"等价，但把目标池显式化了 |
| `src/game.rs` `Match::add_target(id)`（新） | 内部 `targets.add(id)` 的 `Vec<String>` 直接拼进返回的事件流（和 `0x46f310` 一样一次发 1 条 `TargetInfo`） |
| 关卡表（`src/levelgen.rs` 或将来放 21 关数据的地方） | 加一个布尔列 `targets_single_shot`，0 基 `6/8/10/13/17` 为 `true` |

**⚠️ 顺序是未证实的**：`TurnCountInfo` 与复原事件谁先谁后，本轮**没有证据**
（§4.4：回合边界块里从来没有任何 Life/Target 事件）。建议先放在 `TurnCountInfo` **之前**
（"本回合结算的一部分"），并在代码注释里标注
`// 顺序未证实：docs/123 §3.2 —— 缺部分命中样本`。

### 6.4 不要做的事

* ❌ 不要用"重建目标实体"来实现复原（§2.2 已证：造目标只有开场调用点）。
* ❌ 不要在复原时发 `TargetInfo`（那会被客户端当成"又多了一个目标"，且会让
  `SkinGraphics`/`TeamInfo` 重放；§2.3 已证 `TargetInfo` 全量重发只用于新连接）。
* ❌ 不要顺手清障碍/补掩膜（§4.3 ③、§5.1 #6）。
* ❌ 不要把这条规则写成"逐关特判"——用户明确说是一条通用规则，用 `all_in_one_shot: bool`
  表达；将来若要"必须打掉至少 k 个"，把 bool 换成 `min_killed: Option<usize>` 即可。

### 6.5 测试建议（`tests/`）

1. `targets_all_in_one_shot_resets_on_partial_hit`：2 目标，一发打掉 1 个
   → 事件流末尾**必须**有一条 `LifeInfo{id=那个目标, alive:true}`，且 `TurnCountInfo` 照发，
   且**没有** `EndingGame`。
2. `targets_all_in_one_shot_wins_on_full_house`：一发打掉 2 个
   → **没有** `LifeInfo{alive:true}`，有 `EndingGame`。
3. `targets_not_single_shot_keeps_partial_kills`：同场景但 `all_in_one_shot=false`
   → 没有复原事件，目标保持死（这是 `Level 3/4/9` 的行为）。
4. `target_rebroadcast_is_not_reset`：`rebroadcast_all()` 只发 `TargetInfo`、**不发**
   `LifeInfo`（防止把 §2.3 的"新连接补发"误用成重置）。

---

## 7. 证据分级汇总

| 断言 | 等级 | 依据 |
|---|---|---|
| `TargetInfo` 全文件只有 2 个构造点 | **逐位验证** | 8 字节常量搜索 + `add reg,imm8` 扫描，2/2 命中（§8） |
| `0x465780` = hashbrown 集合迭代 + 逐 id 发 `TargetInfo` | **逐位验证** | `pmovmskb/not/tzcnt/shl 2/sub/[rcx-4]` + `dec items`（§2.3） |
| `0x465780` **不是**掩膜扫描 | **逐位验证** | 同上（没有宽度、没有 `1000×600`） |
| `0x465870` 只接受 tag 14（`entity_removed`） | **逐位验证** | `movabs rax,0x800000000000000e; cmp [rsi],rax; jne panic` |
| `0x465910` 订阅 `new_connection_event` + `entity_removed_event` | **逐位验证**（字符串/长度）+ **推断**（`0x9bac88` = subscribe） | §2.4 |
| `graphwar_target_manager.rs` ≤ ~100 行、只有 3 个入口 | **逐位验证** | `panic::Location` 行号 35/65/74/84（§2.5） |
| 目标只在关卡开场创建（13 个调用点全在 level-start） | **逐位验证** | GOT 槽 `0x9bbed8` 的 13 个调用者地址（§2.2） |
| `0x3e2830` 分派表 70 项、`index = tag − 2` | **逐位验证** | `.rodata 0x1beda8` 相对偏移表 + 附录 A |
| tag word 由 `0x7fffffffffffffff + N` 现算（`tag = N − 1`） | **逐位验证** | `0x3e2844` + `0x3e31f3` 的两条指令 |
| `target_info`(23) 分支原样转发 tag 23 | **逐位验证** | 整条分支 `0x3e3197`–`0x3e3228` 读完 |
| **分派器不构造 `life_info`(27)** | **逐位验证** | 全函数无 `add reg,0x1c`（附录 B 末行） |
| `life_info`(27) 在 `life_manager` 有 2 个构造点：`0x4542af`（读实体表的字节）/ `0x454c52`（用入参 `cl`） | **逐位验证**（指令）+ **推断**（语义） | §3.3、§3.5 |
| 胜利 = `Team2` 全灭 | **观测** | 45/45 回放（§4.5） |
| 逐关目标数/回合上限/模式 = `docs/118` §7.1 | **观测** | 21/21 复核（§4.2） |
| 语料里 **0 次**部分命中 | **观测** | 7 发多目标射击全中（§4.3 ①） |
| 语料里 **0 次**实战复活 | **观测** | 48 次假阳性全在 `Setup`（§4.3 ②） |
| 语料里 **0 次**目标 `EntityRemoved` | **观测** | 109 份（§4.3 ③） |
| 障碍洞不随复原恢复 | **观测** | 397 份 0 次中途 `ClearObstacles`（§4.3/§5.1 #6） |
| 复原 = 翻 alive + 广播 `LifeInfo{alive:true}` | **推断** | 三条硬约束把可能性收到唯一（§3.2 #3） |
| 判据 = "本发摧毁集合 ≠ 全部目标" | **推断** | 描述原文 + 目标池 = `Team2` 全集；缺判定点 |
| 复原发生在回合结算（`TurnCountInfo` 前后） | **推断（弱）** | 缺样本；三种时机都可能（§3.2 #1） |
| 复原时是否连带视觉特效 / 是否重置 `turn_count` | **未知** | — |
| 复原规则的**代码位置** | **未知** | 已排除 target_manager；三个候选模块待查（§3.1、§3.4） |
| 回合上限用完而目标未死的结果 | **未知** | 0 样本（§5.3 #1） |

---

## 8. 复现

```bash
# 0) 环境（不碰 /tmp）
export W=/home/user/MyProject/gw2
cd $W && sha256sum libgraphwarrust.so      # 必须是 9a538115…abed187

# 1) 分派表（tag → handler）——附录 A 就是这支脚本的输出
python3 - <<'PY'
import struct
f=open('libgraphwarrust.so','rb').read(); TBL=0x1beda8      # .rodata：vaddr == 文件偏移
for i in range(0x46):
    rel=struct.unpack_from('<i',f,TBL+4*i)[0]
    print(f"tag {i+2:2d} -> {TBL+rel:#x}")
PY

# 2) 全文件的 TargetInfo(23) / LifeInfo(27) 构造点
python3 - <<'PY'
import struct
f=open('libgraphwarrust.so','rb').read(); D=0x1000          # .text: vaddr = 文件偏移 + 0x1000
for tag in (0x17,0x1b):
    v=struct.pack('<Q',0x8000000000000000|tag)
    pos=[]; i=f.find(v)
    while i>=0: pos.append(i+D); i=f.find(v,i+1)
    print(f"tag {tag}: movabs 常量 {len(pos)} 处 {[hex(p) for p in pos]}")
PY

# 3) 13 个"造目标"调用点（经 GOT 槽 0x9bbed8 → 0x46f310）
python3 - <<'PY'
import pickle
W='/home/user/MyProject/gw2/'
xr=pickle.load(open(W+'analysis/xrefs.pkl','rb'))
st=sorted(s-0x100000 for s in pickle.load(open(W+'analysis/funcstarts_ghidra_va.pkl','rb')))
import bisect
own=lambda a: st[bisect.bisect_right(st,a)-1]
print(sorted({hex(own(e[0])) for e in xr[0x9bbed8]}))
PY

# 4) 回放侧的三条负结果（§4.3）——需要 385/397 份 replay（只读）
#    §4.1/§4.3 的三个扫描脚本见本轮工作目录 .agent-work/treset/{scan4,scan5,scan6}.py
```

---

## 附录 A · `0x3e2830` 分派表（tag → handler，逐位）

`index = tag − 2`，表在 `.rodata 0x1beda8`（70 × `i32` 相对偏移）。
未列出的 tag（3,4,7,8,9,11,12,16–22,31,32,34,38,50,52,54,59,62–66,68,70）
统一落 `0x3e4267`（忽略，不做事）。

| tag | 事件 | handler |
|---|---|---|
| 2 | `tick_reply_event` | `0x3e288a` |
| 5 | `new_connection_event` | `0x3e3847` |
| 6 | `connection_removed_event` | `0x3e39b5` |
| 10 | `bot_added_event` | `0x3e3d75` |
| 13 | `player_info_event` | `0x3e36e1` |
| 14 | `entity_removed_event` | `0x3e3464` |
| 15 | `soldier_info_event` | `0x3e34b2` |
| 23 | **`target_info_event`** | `0x3e3197` |
| 24 | `team_info_event` | `0x3e33fc` |
| 25 | `remove_team_event` | `0x3e2d64` |
| 26 | `pos_info_event` | `0x3e32d4` |
| 27 | **`life_info_event`** | `0x3e38dd` |
| 28 | `life_queued_death_event` | `0x3e3787` |
| 29 | `game_name_info_event` | `0x3e2f9d` |
| 30 | `game_state_info_event` | `0x3e3dd3` |
| 33 | `function_fire_event` | `0x3e3e36` |
| 35 | `function_info_event` | `0x3e3ffe` |
| 36 | `function_active_event` | `0x3e3f93` |
| 37 | `function_points_event` | `0x3e2ba9` |
| 39 | `clear_obstacles_event` | `0x3e37ed` |
| 40 | `add_obstacle_event` | `0x3e40e4` |
| 41 | `destroy_obstacle_event` | `0x3e2e6b` |
| 42 | `queue_destroy_obstacle_event` | `0x3e364d` |
| 43 | `turn_info_event` | `0x3e3131` |
| 44 | `effect_info_event` | `0x3e3b39` |
| 45 | `axis_mode_info_event` | `0x3e30a3` |
| 46 | `function_mode_info_event` | `0x3e2b1b` |
| 47 | `turn_mode_info_event` | `0x3e4173` |
| 48 | `time_mode_info_event` | `0x3e336e` |
| 49 | `end_of_turn_info_event` | `0x3e3558` |
| 51 | `skin_graphics_info_event` | `0x3e3cf0` |
| 53 | `face_graphics_info_event` | `0x3e35c8` |
| 55 | `hat_graphics_info_event` | `0x3e2910` |
| 56 | `entity_effect_info_event` | `0x3e2dc3` |
| 57 | `turn_limit_info_event` | `0x3e3eed` |
| 58 | `turn_count_info_event` | `0x3e41d5` |
| 60 | `name_info_event` | `0x3e2a96` |
| 61 | `color_info_event` | `0x3e3c3d` |
| 67 | `rank_info_event` | `0x3e2a29` |
| 69 | `lock_info_event` | `0x3e2995` |
| 71 | `chat_message_event` | `0x3e322d` |

## 附录 B · 分派器里 17 处 tag 构造点（`base + N`，`tag = N − 1`）

| 站点 | `N` | tag | 事件 | 所在分支（按地址区间，**推断**） |
|---|---|---|---|---|
| `0x3e29ef` | 0x46 | 69 | `lock_info` | 69 ✔ 自我转发 |
| `0x3e2b6f` | 0x2f | 46 | `function_mode_info` | 46 ✔ |
| `0x3e2e1e` | 0x39 | 56 | `entity_effect_info` | 56 ✔ |
| `0x3e3062` | 0x1e | 29 | `game_name_info` | 29 ✔ |
| `0x3e30f7` | 0x2e | 45 | `axis_mode_info` | 45 ✔ |
| **`0x3e31f3`** | **0x18** | **23** | **`target_info`** | **23 ✔（整条分支读过，是原样转发）** |
| `0x3e3260` | 0x48 | 71 | `chat_message` | 71 ✔ |
| `0x3e33c2` | 0x31 | 48 | `time_mode_info` | 26 ⚠ 不是自我转发 |
| `0x3e3481` | 0x0f | 14 | `entity_removed` | 14 ✔ |
| `0x3e351d` | 0x10 | 15 | `soldier_info` | 15 ✔ |
| `0x3e374c` | 0x0e | 13 | `player_info` | 42 ⚠ |
| `0x3e3864` | 0x08 | 7 | `connected_to_server` | 5 ⚠（`new_connection → connected`，语义上说得通） |
| `0x3e3bfd` | 0x2d | 44 | `effect_info` | 44 ✔ |
| `0x3e3ca4` | 0x3e | 61 | `color_info` | 61 ✔ |
| `0x3e3eb3` | 0x22 | 33 | `function_fire` | 33 ✔ |
| `0x3e3f58` | 0x3a | 57 | `turn_limit_info` | 57 ✔ |
| `0x3e422d` | 0x3b | 58 | `turn_count_info` | 58 ✔ |

⚠ 三处"不是自我转发"要么是派生事件，要么是 LLVM 把多个分支的尾巴合并了——
**区间归属只按地址相邻推出的，未逐条核控制流**（判「观测」不判「逐位」）。

**全函数没有 `add reg,0x1c`** ⇒ 分派器**不会**构造 `life_info(27)`。
这是 §0 第 ⑥ 条与 §3.2 问题 4 的关键负面证据。
