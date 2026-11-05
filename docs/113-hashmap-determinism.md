# 113 · 哪些输出依赖 HashMap 迭代序 —— 运行时实证

状态：**已结案（定性）**。本文回答三件事：① 回合出场顺序是不是 HashMap 迭代序；
② **广播消息的顺序**是不是也走 HashMap（`docs/06` §17.4 标着"未查"的那条）；
③ 如果钉住那份熵，能不能把顺序**变成逐位可复现**。

证据分级沿用仓库约定：**逐位验证**（`cargo test` 覆盖）/ **观测**（能指到地址、字节或运行时值）/
**推断**（写明缺什么）/ **未知**。地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。

---

## 0. 一句话

> **顺序来自 HashMap 迭代序的不止"出场顺序"一项。** 一局里约 **45% 的报文**（开局那批
> "全量重发"——13 类消息）以及**出场顺序**，都走 `HashMap` 迭代序；键来自 Rust std 的
> `RandomState`，其 base key 由 `getrandom(16)` 逐线程播种。
> **但这是 A 类，不是 B 类**：把两份熵（16 字节哈希种子 + ChaCha 种子）钉成常量后，
> 两次独立进程的报文顺序**完全一致**，剩下的差异只有墙钟 tick。

---

## 1. 结论表

### 1.1 顺序类

| 输出 | 同进程两局是否重复 | 跨进程是否重复 | 钉住 16 字节种子后跨进程是否重复 | 判定 |
|---|---|---|---|---|
| 开局"全量重发"批的顺序（`TeamInfo` `ColorInfo` `SkinGraphicsInfo` `FaceGraphicsInfo` `HatGraphicsInfo` `NameInfo` `RankInfo` `PlayerInfo` `SoldierInfo` `PosInfo` `LifeInfo` `FunctionInfo` `EntityEffectInfo`） | **❌ 否** | **❌ 否** | **✅ 是**（4 次样本，2 组不同种子） | **观测**：HashMap 迭代序 |
| 加入时的外观回复（一次 reply 里按**实体**遍历：玩家 + 其士兵，如 `SkinGraphicsInfo` 229/231/230） | ❌ | ❌ | ✅ | **观测**：同一机制 |
| **回合出场顺序**（`TurnInfo.player_id` / `.soldier_id`） | **看局**：12 次运行里 6 同 6 异（§3.2） | ❌ 否（12 次出现 3 种首发） | ✅（与 ChaCha 一起钉） | **观测** |
| 实体**创建**批（`PlayerInfo`→`SoldierInfo` 首次公告，一局 4 个士兵） | ✅ 是 | ✅ 是 | — | **观测**：按加入顺序，**不是** HashMap |
| `AddObstacle` 的顺序与个数 | ❌ | ❌ | ❌（钉哈希不管用） | **观测**：来自 ChaCha 关卡生成，**不是** HashMap |
| `ColorInfo` 的**值**、bot 名字、`PosInfo` 坐标、AI 函数 | ❌ | ❌ | 需要 ChaCha | **观测**：ChaCha 流 |
| `end_of_turn_tick` / `active_tick` / `start_tick` / `current_tick` | ❌ | ❌ | **❌ 仍不同** | **观测**：墙钟（B 类，`docs/06` §17.2 已知项） |

### 1.2 同一条消息、同一局里，两次遍历的顺序也可能不同

同一批里 `PosInfo` 出现了**两次**（两个不同的 map），两次的顺序**不一样**：

```
p1 m1:  PosInfo [2,3,0,1]  …（中间隔了别的类型）…  PosInfo [3,0,2,1]
q1 m1:  PosInfo [0,3,1,2]  …                    …  PosInfo [3,2,1,0]
```

⇒ 不是"一个全局乱序"，而是**每个 HashMap 实例各有自己的键**（std 的 `RandomState` 每 new 一次
就把 `k0` 加一）。

### 1.3 样本表：开局批里 `SoldierInfo` 的顺序（4 个士兵，创建序固定为 `230,231,233,234`）

| 组 | 运行 | 顺序 | 说明 |
|---|---|---|---|
| 不钉 | `to_1`…`to_9`（`docs/47` 的旧样本，9 个进程） | `231,233,234,230` / `231,230,234,233` / `230,231,234,233` / `230,234,231,233` / `230,233,231,234` / `234,233,231,230` / `231,233,234,230` / `233,230,234,231` / `233,231,230,234` | **9 个进程 8 种顺序** |
| 不钉 | `p1` / `p2` / `p3` | `233,234,230,231` / `230,233,234,231` / `234,230,231,233` | 本次 3 个进程，3 种 |
| 钉哈希=A | `q1` `q2` `s1` `s2` | **全部** `230,234,231,233` | 4 次一致 |
| 钉哈希=B | `r1` `r2` `r3` `r4` `r5` | **全部** `230,233,231,234` | 5 次一致 |
| 钉哈希=B + ChaCha | `r3` `r4` `r5` | `230,233,231,234` | 同上（与 ChaCha 无关） |

⇒ **顺序是种子（+键）的确定性函数**：同种子 ⇒ 同顺序，换种子 ⇒ 换顺序。

---

## 2. 方法

### 2.1 隔离

```bash
git clone /home/user/MyProject/gw2/gw2-rebuild /home/user/MyProject/gw2/.agent-work/hashdet/repo
# 分支 agent/hashmap-determinism
# harness 用自己的一份拷贝，不碰共享的 godot-live：
cp -a /home/user/MyProject/gw2/godot-live /home/user/MyProject/gw2/.agent-work/hashdet/godot-live
```
临时产物全部在 `.agent-work/hashdet/`（本机 `/tmp` 是逐命令 tmpfs）。

### 2.2 在**一个进程里连开两局**

改 `probe.gd`（`tools/probe_two_matches.gd`，harness 里覆盖 `probe.gd`）：

1. 第一局：`Battle → Create Game → AddBot(档3) → TurnTime×2 → Start`（`docs/06` §14.4 原链路）；
2. 退出：`PlayScreen/…/LeftTopButtonsContainer/ExitButtonContainer/**LongPressButton**`
   → `ConfirmPlayExitPopUp` 里按文本找 `Yes` 按钮（`MarginContainer2/GraphwarButton`）；
3. 退出后落点是 **`LanLobby`**（不是主菜单），所以第二局直接按 `CreateButton` 再走一遍；
4. 每步写一行墙钟标记到 `user://hashdet_markers.txt`（`Time.get_unix_time_from_system()`），
   供事后把 frida 的时间轴对齐。

实测两局都能进（进程内）：

```
PRESS Start ok        / MATCH START 1
PRESS PlayExitLongPress ok
TEXTBTN ConfirmYes pressed …/ConfirmPlayExitPopUp/…/MarginContainer2/GraphwarButton text=[  Yes  ]
[2026-11-05 16:21:56] src/graphwar_game_server.rs:336 - Game server thread stopping normally (self.running set to false).
SCREENS after_exit visible=["LanLobby"]
PRESS CreateGame2 ok  / PRESS Start2 ok / MATCH START 2
```

> **这条日志本身就是一个结论**：每局有自己的 **game server 线程**（`graphwar_game_server.rs:336`），
> 退出时停止。抓到的线程号也对得上：第一局的 `TurnInfo` 在 tid 64 上发，
> 第二局在 tid 69 上发（本次抓到的客户端请求都在主线程 tid 25 上）。

### 2.3 采集器：重建报文帧 + 两个熵源

`tools/capture_hashmap_order.js`（frida，`createScript(SRC, { runtime: 'v8' })`——默认 QuickJS
访问 `CpuContext` 会静默无数据，见 `docs/100` §60 那类坑）：

| 抓什么 | 怎么抓 |
|---|---|
| 报文 | hook libc `send` / `write` / `writev` / `sendmsg`，**按 fd 累积**后按 `u16` 大端长度拆帧 |
| 哈希种子 | hook libc `getrandom`，记 `len==16` 的返回字节 + 线程号 + 返回点 |
| ChaCha 种子 | hook libc `syscall`，`SYS_getrandom==318` 且 `len==32` 的返回字节 |
| 打钉 | `onLeave` 里 `buf.writeByteArray(常量)`（哈希种子/ChaCha 种子分别可钉） |

两个踩过的坑：

1. **帧头是 2 字节**（`docs/24` §1：`u16` 大端 + JSON）。我第一版按 3 字节解析，
   结果 0 帧——**而且不报错**，只是永远凑不齐；
2. 游戏把 `send` 拆成**两次系统调用**（先 2 字节长度，再 JSON 体），所以必须按 fd 累积，
   不能按一次 `send` 当一帧。

分析：`tools/compare_message_order.py`（按 fd 分匹配、按类型比顺序、按创建序归一化 id）。

---

## 3. 实验 A：同进程两局，顺序重不重复

**结论分两半**：**广播批的顺序不重复**（§3.1）；**出场顺序在同进程两局里有时同、有时不同，
不能当判据**（§3.2）。两者都**不是**"所以不是 HashMap"——原因有两条：

1. 每局一个新的 server 线程（§2.2），**新线程 ⇒ 新的 `getrandom(16)` ⇒ 新的 base key**；
2. 实体 id 逐局递增（`229/232/230…` → `450/453/451…`），**HashMap 的键本身变了**。

也就是说：**同进程两局本来就不该期望重复**。这一点必须写在前面，否则很容易把
"同进程里跑两局也一样"当成"证明不是 HashMap"——反过来也一样（见 §3.2）。

```
p1 m1:  SoldierInfo 批内顺序 [2,3,0,1]（创建序归一化）   PosInfo [2,3,0,1]
p1 m2:  SoldierInfo 批内顺序 [2,1,0,3]                  PosInfo [2,1,0,3]
```

### 3.1 广播批：同进程两局基本从不重复

把两局的"批内顺序"归一化到创建序逐类型比（10 次运行都做了）：

| 运行 | 两局同序的类型数 | 哪些类型同序 |
|---|---|---|
| `p1` / `p2` / `p3` | 2/14 · 2/14 · 1/14 | 只有 `RankInfo`（以及偶尔 `PlayerInfo`） |
| `q1` / `q2` / `s1` / `s2` | 2/14 | `ColorInfo`、`RankInfo` |
| `r1` / `r2` / `r3` | 4/14 · 3/14 · 2/14 | `PlayerInfo`、`ColorInfo`、`RankInfo` |

（剩下"同序"的都是**只有 2 个元素**的类型——两种排列，撞上不奇怪。）

### 3.2 出场顺序：同进程两局有时同、有时不同 —— 所以它**不是**判据

首发（`TurnInfo` 归一化到创建序）逐运行列出来：

| 运行 | 第 1 局首发 | 第 2 局首发 | 同进程相同？ |
|---|---|---|---|
| `p1` | `(p1,s3)` | `(p1,s3)` | 同 |
| `p2` | `(p1,s3)` | `(p0,s0)` | 异 |
| `p3` | `(p0,s1)` | `(p1,s3)` | 异 |
| `q1` `q2`（钉哈希A） | `(p1,s2)` | `(p1,s3)` | 异 / 异 |
| `r1` `r2`（钉哈希B） | `(p0,s1)` / `(p1,s3)` | `(p0,s1)` / `(p1,s3)` | 同 / 同 |
| `r3` `r4` `r5`（钉B+ChaCha） | `(p1,s3)` | `(p0,s1)` | 异 ×3 |
| `s1` `s2`（钉A+ChaCha） | `(p1,s2)` | `(p0,s1)` | 异 ×2 |

跨进程看第 1 局首发：12 次里 3 种（`(p1,s3)`×6、`(p1,s2)`×4、`(p0,s1)`×2），
其中 `(p1,s2)` 全部来自"钉哈希 A"的 4 次、`(p1,s3)` 有 4 次来自"钉哈希 B"——**换种子就换首发**。

⇒ **"同进程两局顺序重复"不能当判据**：两局的实体 id 不同（`229/232…` → `450/453…`），
HashMap 的**键**本来就变了；"相同"只是巧合。真正能定性的是实验 C（打钉）。

---

## 4. 实验 B：广播顺序是不是也走 HashMap

**结论：是，而且范围比预想的大。**

### 4.1 跨进程（`p1` / `p2` / `p3`，什么都没钉）

48 类消息里 **30 类**不同（含全部"批"类）。第一处差异出现在**第 4 帧**：

```
p1 #3  {"TurnModeInfo":{...}}          ← 连"房间设置"那一批的顺序都在变
p2 #3  {"AxisModeInfo":{...}}
```

### 4.2 影响面：一局的 ~45% 报文

```
p1 m1: 批=87 帧 / 全匹配 189 帧 (46%)，涉及 24 类消息
p1 m2: 批=79 / 193 (41%)
p2 m1: 批=79 / 174 (45%)
q1 m1: 批=76 / 171 (44%)
s1 m1: 批=91 / 184 (49%)
```

（"批"= 从 `ClearObstacles` 到第一条 `FunctionUpdateRequest`；即开局把场上实体状态整体重发一遍。）

### 4.3 创建批不受影响

每局开头那 4 条 `SoldierInfo`（`PlayerInfo` 之后立刻发的"创建公告"）在 **12 次运行 × 2 局**
里顺序恒为 `230,231,233,234`（第二局 id 不同但相对序相同）：

```
p1 m1 [230, 231, 233, 234] | p2 m1 同 | p3 m1 同 | q1/q2/r1..r5/s1/s2 m1 同
```

⇒ **"创建顺序"是 Vec/加入序，可以复刻；只有"重发/遍历"这一层是 HashMap。**

顺带一个结构观测：外观那批（`SkinGraphicsInfo` / `FaceGraphicsInfo` / `HatGraphicsInfo`）在一次
reply 里把**玩家和士兵放在同一个 map 里**遍历（归一化后形如 `[p0, s2, s1, s0, p1, s3]`），
而 `PosInfo` / `LifeInfo` / `FunctionInfo` 那一批只遍历士兵。

---

## 5. 实验 C：把两份熵钉死 —— 决定性

### 5.1 哈希种子

`getrandom` 的 16 字节，钉成 `00112233445566778899aabbccddeeff`（A）/ `aabbccddeeff00112233445566778899`（B）：

```
q1（钉 A）SoldierInfo 批 [230,234,231,233]   ← 与 q2、s1、s2 完全一致
r1（钉 B）SoldierInfo 批 [230,233,231,234]   ← 与 r2、r3、r4、r5 完全一致
q 组 ≠ r 组                                   ← 换种子就换顺序
```

### 5.2 连 ChaCha 一起钉（32 字节，走 `syscall(318)`）

钉 A + ChaCha 常量，跑两次独立进程（`s1` / `s2`）：

```
全部报文按到达顺序比：345/370 帧**逐字节相同**
按消息类型比：43/48 类**完全相同（含顺序）**；不同的 5 类全是 tick 字段：

  EndOfTurnInfo    3688            vs 3687
  TickReply        183,243,303…    vs 186,246,306…
  FunctionActive   active_tick 198 vs 172
  LifeQueuedDeath  tick 292        vs 266
  EffectInfo       start_tick 292  vs 266   （其余字段：effect_id/坐标/类型全同）

（`r3`/`r4`/`r5` 用另一组种子复做，结论相同：351/372 帧、43/48 类、差异同样只有 tick。）
```

⇒ **顺序完全由这两份熵 + 实体 id 决定**；剩下的只有墙钟 tick（`docs/06` §17.2 的 B 类）。
**这就是"逐位可复现"的实验证明，只要那份熵拿得到。**

### 5.3 反证：只钉哈希不钉 ChaCha

`q1` vs `q2`：56/365 帧相同，48 类里 **16 类**不同——但差异**全是内容**（颜色、障碍、坐标、
bot 名字、AI 函数），**顺序类全部一致**。⇒ 两个熵源分工清楚：**顺序 ← 哈希种子，内容 ← ChaCha**。

---

## 6. 把 `getrandom` → 哈希键钉成观测（`0x47dec0`）

`0x47dec0` 的开头就是内联的 SipHash-1-3 初始化，键是 **`[rdi]` 的 16 字节**：

```
47dec0: sub $0x58,%rsp
47dec4: movdqu (%rdi),%xmm0        ← 16 字节键
47dec8: pshufd $0x44,%xmm0,%xmm1 ; pxor 0x19cf20  ← 把 k0 广播到两个 64 位槽，异或 IV 常量
47ded5: pshufd $0xee,%xmm0,%xmm2 ; pxor 0x19c790  ← 把 k1 广播到两个 64 位槽，异或 IV 常量
47df07: mov (%rsi),%eax            ← 被哈希的值（本实例是 i32 键）
```

hook 它并打印 `[rdi]`（钉住种子 `00112233445566778899aabbccddeeff` 的那次运行）：

```
t+ 339ms tid=33 n=228    2d112233445566778899aabbccddeeff
t+ 405ms tid=33 n=1506   21112233445566778899aabbccddeeff
t+1359ms tid=72 n=1      02112233445566778899aabbccddeeff
t+1359ms tid=72 n=8      07112233445566778899aabbccddeeff
t+1359ms tid=72 n=2      08112233445566778899aabbccddeeff
t+1359ms tid=72 n=1      09112233445566778899aabbccddeeff
t+1359ms tid=72 n=1      0a112233445566778899aabbccddeeff
t+1359ms tid=72 n=39     0b112233445566778899aabbccddeeff
   …                                    （首字节 = k0 的低字节，逐个 +1）
共 397019 次调用、400 个不同键（400 是我设的采集上限）
```

⇒ **观测**：`getrandom` 的 16 字节**原样**成为 SipHash 键；每个新 `HashMap` 把 `k0` **加一**
（Rust std `RandomState::new` 的 `keys.set((k0.wrapping_add(1), k1))`）。
**交叉验证**：钉与不钉两次运行里，最先出现的两个 map 的计数器都是 `0x2d`（45）和 `0x21`（33）；
不钉那次这两个键的低字节是 `0x35` / `0x29` ⇒ 该线程的 base 低字节 = `0x08`
（正是同一次运行里 `getrandom` 抓到的 `084c2fb3…2a75eb66…` 的首字节，`0x08+0x2d=0x35` ✓）。

---

## 7. 对逐位验收的影响

### 7.1 口径要补一条

`docs/06` §17.1 现在写的是"**每条广播消息的字节**"。实测表明还差半句：
**字节相同的同一批消息，顺序可以不同**。建议改成

> …在**每个 tick 的状态**、**每条广播消息的字节与它们在一批内的顺序**、**每条弹道的每个采样点**上完全相等。

### 7.2 哪些现在可以判"输入决定"

| 项 | 结论 | 依据 |
|---|---|---|
| 所有消息的**内容**字段 | ✅ 输入决定（ChaCha 种子固定后逐字节一致） | `s1` vs `s2`、`r3` vs `r4` |
| 实体**创建**顺序、加入顺序 | ✅ 输入决定 | 12 次运行全同 |
| 逐发结算 / 弹道 / 掩膜 | ✅ 输入决定（`docs/89` 起已逐点验证） | 本文未涉及，无变化 |
| 消息**顺序**（13 类批 + 外观回复） | ✅ 输入决定，**输入是那 16 字节/线程** | §5.2 |
| **回合出场顺序** | ✅ 输入决定，但输入是 16 字节 **+** ChaCha **+** 实体 id | §5.2、§8-1 |
| 墙钟 tick 字段 | ❌ 不由输入决定（B 类） | `end_of_turn_tick` 3688/3687 |

### 7.3 但"可复现"的门槛比 `docs/06` §17.2 写的高

`docs/06` §17.2 说 A 类只要凑齐三样（算法 / 键 / 表几何）。实测多出两条：

1. **键不是一份，是"每线程一份 + 每个 map 一个序号"**：一局里 6~7 个线程各播一次种，
   同一个线程里的每个 `HashMap` 再各 +1。要复现顺序，必须复现**"第几个 map"**——也就是
   **HashMap 的创建序列**，而它不仅取决于代码，还取决于走哪条代码路径（`r1` 那次的翻转，
   见 §8-1，就是"路径不同 ⇒ 序号不同 ⇒ 顺序不同"的样子，未证实）；
2. **实体 id 会漂移**：同样两局、同样配置，第二局的 id 可以是 `446/447/449/450`，也可以是
   `450/451/453/454`（`p3` / `r1`）。id 变了键就变了，顺序必然变。

⇒ 实务上的退化口径（建议写进 `docs/06` §17.2 的"怎么才能逐位"栏）：

* **顺序按回放里的顺序消费**（回放已经把顺序钉死），不要求重建版自己"算出"顺序；
* 若要求"同种子复现"，则必须额外记录：每线程 16 字节 + 每个 map 的创建序号 + 实体 id 分配。
  这三样目前**都没有**采集，而且第 2、3 样在真机上是随路径/时序变化的。

### 7.4 对 `docs/47` 的影响

`docs/47` §5 的**判定方向是对的**（HashMap 迭代序），本文把它从"推断"升级为"观测"，
并纠正两处：

* §5 说"最可能来自 HashMap" ⇒ 现在是**观测**（`0x47dec0` 的键 + 打钉实验）；
* §9 说"要实证还差：抓到 getrandom 字节 / 还原键类型与容量 / 另一实例上重跑" ⇒ 前两项
  **已补**（本文 §5、§6），第三项被 §5.2 的做法**绕过**（不需要知道容量：钉住种子就复现了）。

`docs/06` §17.4 最后一行的"HashMap 的其他用途……**未查**" ⇒ 本文补上，答案是**有，而且很大**。

---

## 8. 仍未确定

| # | 未确定 | 现有材料 | 缺什么 |
|---|---|---|---|
| 1 | 首发球员在"只钉哈希"时**偶尔**翻转：`r1` 首发出场 `(229,231)`，`r2` 是 `(232,234)`，两局哈希种子、实体 id、批内各类消息的 id 序列**全都一样**（`PosInfo`/`Skin`/`Face`/`Hat`/`TeamInfo`/`SoldierInfo`/`ColorInfo`/`NameInfo` 逐项相同） | 6 次"只钉哈希"里翻转 1 次；"哈希+ChaCha 都钉"后 3/3 一致（`r3`,`r4`,`r5`） | 样本太少（n=6/3）。需要 ≥20 次/组，并同时抓"turn_manager 那个 map 的键"来判定是不是"创建序号漂移" |
| 2 | 实体 id 漂移的**原因** | `p3` m2 = `446,447,449,450`；`r1` m2 = `450,451,453,454`；其余 9 次 = `451,452,454,455` | 没查。怀疑与上一局收尾（`ConnectionRemoved`/对象析构）的时序有关，**未验证** |
| 3 | 每个 `HashMap` 拿到的是**第几个** `RandomState` | §6 观测到 +1 序列；知道线程数（6~7） | 没有把"map ↔ 序号 ↔ 它发出的消息类型"逐一对账；不建这个账，就无法在不钉种子的情况下复刻 |
| 4 | `0x47dec0` 之外是否还有别的内联 SipHash 实例 | 只看了这一处（`docs/98` §6 也是这一处） | 未逐 map 确认；`vals` 显示本实例哈希的是**小整数**（0..7），不一定是实体 id |
| 5 | 客户端主线程（tid 25）是否也有"顺序化成消息"的地方 | send 方向抓到的都是单条请求（`NameRequest` 等），没看到成批 | 只覆盖"加入/开局"这一段；聊天、房间列表、重连未查 |
| 6 | 真实对局里那 16 字节的**持久化** | 本文是即时挂钩钉住的，不是从回放/文件里读出来的 | 若要走"补一份熵"的路线，得先决定把这份熵记在哪（回放旁边？日志？） |

---

## 9. 复现

```bash
W=/home/user/MyProject/gw2/.agent-work/hashdet
cp -a /home/user/MyProject/gw2/godot-live $W/godot-live
cp repo/tools/probe_two_matches.gd $W/godot-live/probe.gd      # harness 的 autoload 探针

# A/B：钉 16 字节哈希种子 + 32 字节 ChaCha 种子，各跑 36~42 s，一次进程两局
node repo/tools/capture_hashmap_order.js $W/run/s1.json 40 \
     00112233445566778899aabbccddeeff \
     deadbeefcafebabe0123456789abcdef0123456789abcdefdeadbeefcafebabe
node repo/tools/capture_hashmap_order.js $W/run/s2.json 40 \
     00112233445566778899aabbccddeeff \
     deadbeefcafebabe0123456789abcdef0123456789abcdefdeadbeefcafebabe

# 比顺序（按 fd 分匹配、按创建序归一化 id）
python3 repo/tools/compare_message_order.py $W/run/s1.json $W/run/s2.json
```

不钉种子时把最后两个参数去掉即可（那就是"随机"分支）。
`0x47dec0` 的键观测脚本见本文 §6 的命令行说明（`siphash` 探针随 `capture_hashmap_order.js`
的 `--keys` 模式）。

---

## 10. 一句话给后来者

**"顺序"是这套游戏里第二个熵源的另一半，不是"随机噪声"。** 它可复现、可打钉、可验证，
但复现它需要的不是"知道算法"，而是**每线程 16 字节 + 每个 map 的创建序号 + 实体 id 分配**。
在拿到这三样之前，逐位对拍里"顺序"这一维应当按**回放给定的顺序**消费，并把这条退化口径
明写进 `docs/06` §17.2；不要再把它当成"整局无法比对"的理由——内容与几何完全不受影响。
