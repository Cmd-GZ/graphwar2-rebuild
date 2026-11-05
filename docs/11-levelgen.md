# 11 · 关卡生成：两种生产者、公式与逐位复现

> **本篇解决什么问题**：一局的障碍是怎么生成的——有几种生产者、公式是什么、
> 随机流怎么用、我们的 Rust 实现如何逐位复现真实关卡，以及 `0x4ea6dc`（"圆盘式"）
> 为什么**不是**第三种关卡生成模式。
>
> **本篇合并自**：

| 原文件 | 内容 | 归档 |
|---|---|---|
| `55-level-generator-closed.md` | 公式闭合：`0x5625b0` 是 ziggurat 标准正态采样器 | [`archive/55-level-generator-closed.md`](archive/55-level-generator-closed.md) |
| `56-levelgen-replay-complete.md` | 端到端重放 13/13 障碍逐位预测成功 | [`archive/56-levelgen-replay-complete.md`](archive/56-levelgen-replay-complete.md) |
| `61-levelgen-rust.md` | `src/levelgen.rs` 的实现与边界 | [`archive/61-levelgen-rust.md`](archive/61-levelgen-rust.md) |
| `103-disk-levelgen.md` | `0x4ea6dc` 的定性（**不是**关卡生成器） | [`archive/103-disk-levelgen.md`](archive/103-disk-levelgen.md) |
| `51-obstacle-emit-chain.md` | 从"生成"到"发送"的调用链 | [`archive/51-obstacle-emit-chain.md`](archive/51-obstacle-emit-chain.md) |
| `40-level-generator.md` | **已被推翻**（"OS 熵"结论，见 §7） | [`archive/40-level-generator.md`](archive/40-level-generator.md) |

随机源（ChaCha、种子、HashMap 序）在 [`12-rng.md`](12-rng.md)；每关的**参数**（障碍数 /
预设闭包 / min_count）在 [`10-campaign.md`](10-campaign.md) §4 与 §6。
证据标记沿用四档（定义见 [`06-verification.md`](06-verification.md) §1）。

## 1. 只有两个生产者：随机散布 / 中心单障碍


### 1.1 为什么它不是关卡生成器（5 条独立证据）

1. **不调 `place_obstacle`**。我按字节扫了全部 `E8 rel32`：`place_obstacle`
   （虚地址 `0x3f17a0`）在 `.text` 里只有 **5 个**调用点——
   `0x3e414c`、`0x3f1f4a`、`0x49f29a`、`0x49f44a`、`0x4a5f5a`，
   **没有一个**落在 `0x4ea6cb`…`0x4eae00` 这段里。
2. **记录布局对不上**。`place_obstacle` 推入的是 **32 字节**
   `{u64 tag = 0x8000000000000028, x: f64, y: f64, r: f64}`
   （`FUN_004f17a0`，`local_78`…`local_60`，经 `FUN_004d3530` 进 Vec）；
   圆盘写的是 **48 字节 7 字段**、**没有半径字段**。协议 `AddObstacle{pos_x,pos_y,radius}`
   只能来自前者。（`archive/ai/72` §2.2 当年那句"半径语义存疑"，答案就是：**根本没有半径字段**，
   因为它不是障碍。）
3. **它是 28+ 个 case 之一**，且同族 case 的产物样式各异（`style=0x1e`、`flag=4`、
   半径 `[2,4)` 的环形分布等）。一个游戏的"关卡模式"不会以"28 选 1 的图案库"形式出现。
4. **调用方是资源/动画侧**：唯一直接调用点 `0x3cb184`（在 `FUN_004cb080` 内）先用
   `FUN_0057dec0` 在哈希表里按 int 键查到一项，取出 `kind` 后再调分派器，随后用
   `(counter − 基准) / 60.0` 换算出的**秒数**去做后续处理。
   （`/60.0` 是帧→秒：**推断**这条链路是逐帧重算的动画。）
5. **运行时从未被调用**：`archive/ai/72` §5 的 hook 记录（一局 45 s）为 **0 次**；
   函数的直接调用图与函数指针槽位都只在分派器里出现一次。

> 结论：`archive/ai/70` 把 `0x4ea6dc` 叫"关卡生成器"是误读；`archive/ai/72` 的怀疑成立，
> 本文给出定论。**"三种模式"应改为"两种模式"**。

---


## 2. 公式闭合：正态分布 + 均匀分布

### 2.1 关键发现：`FUN_005625b0` 是**标准正态采样器**

反编译 `0x5625b0`，看到的是 ziggurat 算法：

```c
uVar3 = uVar2 & 0xff;                                   // 256 项表索引
dVar6 = table_f[uVar3] * ((double)(uVar2 >> 12 | 0x4000000000000000) - 3.0);
if (ABS(dVar6) < table_r[uVar3]) return dVar6;           // 快速接受
dVar6 = exp(dVar6 * dVar6 * -0.5);                       // 高斯密度
if (uVar3 == 0) { /* 尾部对数采样，含 3.654152885361009 */ }
...
```

`3.654152885361009` 正是 `rand` 的 ziggurat `R` 常量 ⇒ 这是
**`rand::distributions::StandardNormal`**，返回 **N(0,1)** 而不是 `[0,1)` 均匀分布。

### 2.2 于是公式全部闭合

```c
n     = (int)trunc( normal() * 7.0 + 15.0 );      // 障碍个数
      if (n <= min_count) n = min_count;
loop n:
  r   = max( normal() * 1.6 + 2.6, 0.8 );         // 半径（下限 0.8）
  u   = (next_u64() >> 11) * 2^-53;               // [0,1) 均匀
  x   = (2*r + 25.0) * (2*u - 1.0);
  v   = (next_u64() >> 11) * 2^-53;
  y   = (2*r + 15.0) * (2*v - 1.0);
  place_obstacle(r, ctx, x, y);
```

指令级依据（`0x3f1f58` 起）：

```
call *%r13          ; next_u64
call 0x4625b0       ; normal()   ← 正态
mulsd 1.6 / addsd 2.6 / maxsd 0.8   ; 半径
...
call *%r13 ; next_u64  (u)
mulsd 2^-53, addsd -0.5, addsd x,x ; (2u-1)
addsd 15.0 ; mulsd ...             ; y
```

### 2.3 这解释了此前所有对不上的现象

| 现象 | 解释 |
|---|---|
| 障碍数 5, 8, 9, 14, 15, 17, 20, 21 | `N(0,1)*7+15` 的截断，标准差 7 ⇒ 分布很宽 |
| 半径 0.8 ~ 5.29 | `N(0,1)*1.6+2.6`，下界由 `maxsd 0.8` 钳位 |
| 半径与其前一个随机数是线性关系但 R² 只有 0.95 | 半径是**正态**值，且 ziggurat 的拒绝采样**消耗不定数量**的 u64，索引会漂移 |
| `12-rng.md` 里 u/v 精确命中、半径对不上 | u/v 是真正的均匀抽样；半径来自正态采样器 |

### 2.4 判据对照

| 判据 | 状态 |
|---|---|
| 1) 定位 RNG 实例（类型/种子/内存位置） | **达成**：ChaCha；key+计数器运行时可读（`12-rng.md`）；已被本轮公式确证 |
| 2) 重放预测真实障碍坐标 | **公式全部闭合**：u/v 已精确命中 13/13（`12-rng.md`）；半径/个数的来源已确定为正态采样器。**完整端到端重放仍差"实现 ziggurat + 读表"这一步**（表在 `.rodata`，未做） |
| 3) 断点 | 若仍要挑刺：ziggurat 的表数据与拒绝分支尚未实现验证 |


## 3. 端到端重放：13/13 障碍逐位预测

### 3.1 做法

1. 运行时 hook `0x47f700`，抓下每次 `next_u64` 的**返回值**（等价于抓下整条随机流）；
2. 从 `.rodata` 导出 ziggurat 的两张表：
   - `ZIG_NORM_X` @ 文件偏移 `0x25b428`（`3.910758, 3.654153, …`）
   - `ZIG_NORM_F` @ `0x25ac20`（`0.000477, 0.001260, …`）
3. 用同款 ziggurat 算法（`rand` 的 StandardNormal）从随机流里采样正态值；
4. 按 `11-levelgen.md` 的公式生成障碍，和真实报文逐条比对。

### 3.2 起点对齐

关卡生成从随机流的**第 3 个**随机数开始（前 3 个被别处消费）。从索引 3 起：

```
第 1 个正态 → trunc(n*7+15) = 13        （真实发出 13 个障碍）
第 2 个正态 → 2.2836357471760804        （真实半径 2.2836357471760804，逐位相同）
之后位置正好是 5 —— 与我们先前定位的 u 索引一致
```

### 3.3 结果：13/13 完全一致

```
 # 0 预测 (   -7.959,   17.697) r= 2.284 | 真实 (   -7.959,   17.697) r= 2.284  ✓
 # 1 预测 (  -30.444,   -0.786) r= 3.059 | 真实 (  -30.444,   -0.786) r= 3.059  ✓
 # 2 预测 (  -29.067,  -12.115) r= 2.127 | 真实 (  -29.067,  -12.115) r= 2.127  ✓
 …
 #12 预测 (   -1.133,   -0.059) r= 0.800 | 真实 (   -1.133,   -0.059) r= 0.800  ✓

完全一致 13/13
```

**障碍个数、每个障碍的坐标与半径，全部逐位相同。**

### 3.4 判据对照

| 判据 | 状态 |
|---|---|
| 1) 定位 RNG 实例（类型/种子来源/内存位置） | **达成**：ChaCha；key+计数器运行时可读；块函数 `FUN_00850f30` 与 σ 常量佐证 |
| 2) 重放随机序列预测真实障碍坐标 | **达成**：13/13 逐位一致 |
| 3) 做不到就写清断点 | 不需要；失败路径已记录在 `11-levelgen.md`/`48`/`50`/`52` |
| 4) 不改游戏文件、提交推送、子仓不进上游 | 遵守 |


## 4. 圆盘式 `0x4ea6dc`：算法、RNG 调用序与否定证据

### 4.1 真正的入口：跳转表分派器 `FUN_004ea690`

`0x4ea6dc` 不是函数入口，它是分派器的第一个 case 落点。

```
004ea690  push rbp / r15 / r14 / r13 / r12 / rbx
004ea69a  sub  $0xc8,%rsp
004ea6a1  movsd %xmm1,0x50(%rsp)      ; 第 5 参（f64）→ 备用
004ea6a7  movsd %xmm0,0x48(%rsp)      ; 第 4 参（f64）
004ea6ad  mov   %rdi,0x68(%rsp)       ; 第 1 参：输出 Vec 的 &mut
004ea6b2  movzbl %sil,%eax            ; 第 2 参：kind（u8）← 分派键
004ea6b6  lea   0x1c3ba8(%rip),%rcx    ; 跳转表基址（虚地址 0x1c3ba8）
004ea6bd  movslq (%rcx,%rax,4),%rax
004ea6c1  add   %rcx,%rax
004ea6c4  mov   %rdx,0x60(%rsp)       ; 第 3 参：type 码
004ea6c9  jmp   *%rax
```

⇒ 签名（观测）：`fn out: &mut Vec<Rec> (rdi), kind: u8 (sil), type_code: u32 (rdx),
c0: f64 (xmm0), c1: f64 (xmm1)`。

跳转表（**观测**，从 `.rodata` 直接读 `int32` 相对偏移）：

| kind | 落点（虚地址） | 备注 |
|---|---|---|
| 0 | `0x4ea6cb` | **本文的"圆盘"** |
| 1 | `0x4eac8e` | 兄弟 case（`0x4ea8a0` 族） |
| 2 | `0x4ea858` | 同上 |
| 3 | `0x4eaaa7` | 同上 |
| 4…27 | `0x4eb4d8`…`0x4eb7d4` | 另一半 case（共用一段大尾部） |

表里至少 28 项；`movzbl` 说明键宽 8 位。**没有**任何字符串/常量把 kind 与
"关卡模式"绑定起来——kind 由调用方从哈希表里取出的 dword 传入（见 §6 第 4 条）。

---

### 4.2 圆盘 case 的算法（观测）

REA 反编译（`procedure_pseudo_code 0x5ea6dc`，节选；`param_15/16` = xmm0/xmm1）：

```c
iVar7 = 0x40;                                   /* 64 */
do {
  uVar5  = FUN_0057f700(plVar4 + 2);            /* next_u64 #1 → u1 */
  param_12 = (double)(uVar5 >> 0xb) * 1.1102230246251565e-16;   /* u1 */
  ...
  uVar5  = FUN_0057f700(plVar4 + 2);            /* next_u64 #2 → u2 */
  param_10 = (double)(uVar5 >> 0xb) * 1.1102230246251565e-16;   /* u2 */
  param_10 = param_10 * 7.5;                                   /* r = 7.5·u2 */
  sincos((param_12 + param_12) * 3.141592653589793, &param_24, &param_23);
  /* +0x00 ← param_15, +0x08 ← param_16                              */
  *pdVar1      = param_12 * param_10;          /* +0x10 = sin(2π·u1)·r */
  pdVar1[1]    = param_13 * param_10;          /* +0x18 = cos(2π·u1)·r */
  *(int *)(...+0x20)  = param_3 + 10;          /* type = 入参 + 10     */
  *(undefined4 *)(...+0x24) = 0xf;             /* style = 0xf          */
  *(undefined1 *)(...+0x28) = 1;               /* flag  = 1            */
} while (--iVar7);

iVar7 = 0x10;                                   /* 16：同上，但 */
  param_10 = param_10 * 3.0;                    /*   r = 3.0·u2 */
  *(undefined1 *)(...+0x28) = 0;                /*   flag = 0   */
```

汇编里的硬证据（虚地址）：

| 事实 | 证据 |
|---|---|
| 记录步长 **0x30** | `4ea729 lea (%r12,%r12,2),%rcx` + `4ea72d shl $0x4,%rcx` = idx·0x30 |
| 外圈 64 条 | `4ea6ea mov $0x40,%ebp` … `4ea768 dec %ebp` |
| 内圈 16 条 | `4eaec1 mov $0x10,%ebp` |
| `2^-53` | `4ea78e`/`4ea7c7` 乘 `.rodata` `0x19f578` = `0x1.0p-53` |
| `7.5` | `4ea804` 乘 `0x19f628` = `0x1.e000000000000p+2` |
| `3.0` | `4eafc4` 乘 `0x19f858` = `3.0` |
| `π` | `4ea7f6` 乘 `0x19fdc8` = `0x1.921fb54442d18p+1` |
| 角度 = `(u1+u1)·π` | `4ea7f2 addsd %xmm0,%xmm0` 后紧接 `mulsd π` |
| `sin` 进 `+0x10`、`cos` 进 `+0x18` | `4ea812 lea 0x90(%rsp),%rdi`（第 2 参 = sin）/ `4ea81a mov %r13,%rsi`（`r13 = rsp+0x88` = cos），回来后 `movsd 0x90` + `movhpd 0x88` 打包成 `{sin,cos}` |
| 中心坐标整批复制 | `4ea71c movsd 0x48(%rsp)` → `+0x00`，`4ea73c movsd 0x50(%rsp)` → `+0x08` |
| `style=0xf` / `flag=1` / `flag=0` | `4ea753 movl $0xf,0x24`、`4ea75b movb $0x1,0x28`、`4eaf1b movb $0x0,0x28` |

#### 4.2.1 记录布局（0x30 = 48 字节）

```text
+0x00  f64  a       整批相同的第 1 个中心坐标（分派器 xmm0）
+0x08  f64  b       整批相同的第 2 个中心坐标（分派器 xmm1）
+0x10  f64  c       sin(2π·u1) · r
+0x18  f64  d       cos(2π·u1) · r
+0x20  u32  kind    type_code + 10
+0x24  u32  style   0xf（圆盘 case 固定）
+0x28  u8   flag    外圈 1 / 内圈 0
+0x29…+0x2f          填充
```

`(a,b)` 与 `(c,d)` 是"一个中心 + 一个随机点"：**推断**它们是一条线段的两个端点，
依据是同族 `FUN_004ea8a0` 在 `+0x00` 写的是**每条各不相同**的中心附近抖动值
`((2u−1)·0.25) + 中心`，而不是常数——那更像"每段各自的起点"。这一条**未证实**。

#### 4.2.2 参数汇总（回答任务问的三项）

| 项 | 值 |
|---|---|
| 数量 | **80**（64 + 16），与随机流无关，恒定 |
| 半径 | 外圈 `u2·7.5`（均匀在 [0,7.5)）、内圈 `u2·3.0`（均匀在 [0,3)） |
| 位置 | 极坐标 `(sin(2π·u1)·r, cos(2π·u1)·r)`，**圆心恒为原点**（中心坐标来自入参，不参与随机） |
| 分布 | 圆盘内**面**均匀（半径均匀、角度均匀），不是圆环 |

---

### 4.3 RNG 调用顺序与次数（观测）

* 每条记录 **恰好 2 次** `next_u64`（`FUN_0057f700`）：**先 u1（角度），后 u2（半径）**；
* 一批 **80 × 2 = 160 次**；
* 只有均匀数，**没有** ziggurat 正态采样、**没有**拒绝采样 ⇒ 消耗是定长的。

这一点与随机散布正相反：散布每个障碍要 `normal()`（拒绝采样，不定长）+ 2 个均匀数，
一批 17 个障碍就吃掉 120 个随机数（`fixtures/levelgen_draws.txt` 正好 120 个）。

---

### 4.4 与"随机散布"的逐项对照

| 维度 | 随机散布 `0x3f1e50` | 圆盘 `0x4ea6dc` |
|---|---|---|
| 产物 | `place_obstacle(...)` → 32 字节障碍记录 | 直接写 0x30 字节"图案段" |
| 条数 | `trunc(N(0,1)·7+15)`，可被 `min_count` 抬高（1…35） | 恒定 80 |
| 半径 | `max(N(0,1)·1.6+2.6, 0.8)`，下限 0.8 | `u·7.5` 或 `u·3.0`（无下限） |
| 位置 | 矩形 `x=(2r+25)(2u−1)`、`y=(2r+15)(2v−1)` | 极坐标圆盘，围绕入参中心 |
| 随机数 | `normal()`（不定长）+ 2×`next_u64` | 2×`next_u64`，定长 |
| 副作用 | `FUN_004f19a0` 重置碰撞掩膜 | **无**（不碰掩膜、不碰障碍 Vec） |
| 触发 | 开局被直接调用（`FUN_00566c40`，`src/graphwar_level_generator.rs`） | 未被任何关卡流程调用（见 §6） |

---

### 4.5 顺带纠正：`place_obstacle` 有 **5** 个调用点，不是 3 个

`archive/ai/72` §1 说只有 3 个调用者（并自陈"另有 2 条未解析"）。字节级扫描得到 5 个，
按所属函数归类：

| 调用点（虚地址） | 所属函数 | 是什么 |
|---|---|---|
| `0x3e414c` | `0x3e2830` | 网络事件分发器（`AddObstacle` 事件） |
| `0x3f1f4a` | `0x3f1e50` | **随机散布**生成器 |
| `0x49f29a` | `0x49f1c0` | **中心单障碍**（`src/graphwar_level_manager.rs` 里的第 1 份） |
| `0x49f44a` | `0x49f370` | **中心单障碍**（`archive/ai/72` 记的那份；挂在指针槽 `0x95ad60`） |
| `0x4a5f5a` | `0x4a5e80` | **中心单障碍**（`level_manager.rs` 里的第 2 份） |

`0x49f1c0` / `0x49f370` / `0x4a5e80` 三个函数的算法**逐字相同**：
`reset_mask` → `u = (next_u64>>11)·2^-53` → `place_obstacle(u*5+3, 原点)`。
即"中心单障碍"是同一个图案的 3 份拷贝（不同分派入口），不是 3 种模式。

⇒ 关卡生成的生产者只有 **2 类**：散布 + 中心单障碍。

---


## 5. Rust 实现与它的边界

### 5.1 实现

`src/levelgen.rs`：

```rust
pub fn normal(next: &mut dyn FnMut() -> u64) -> f64   // ziggurat 标准正态
pub fn generate(next: &mut dyn FnMut() -> u64, min_count: i32) -> Vec<Obstacle>
```

公式与常量全部来自 `11-levelgen.md`/`11-levelgen.md`；ziggurat 的两张表由
`src/levelgen_tables.rs` 承载，**从 `.rodata` 原样导出**
（`ZIG_NORM_X` @ `0x25b428`、`ZIG_NORM_F` @ `0x25ac20` 起）。

### 5.2 验证：逐位复现

夹具 `fixtures/levelgen_draws.txt`（真实抓包的随机流前 120 个值）
+ `fixtures/levelgen_obstacles.txt`（同一局真实发布的 13 个障碍）。

测试 `replays_captured_level_exactly`：从第 3 个随机数开始跑 `generate`，
对每个障碍比较 `x/y/r` 的 **`to_bits()`**（不是近似比较）：

```rust
assert_eq!(a.x.to_bits(), b.x.to_bits());
assert_eq!(a.y.to_bits(), b.y.to_bits());
assert_eq!(a.r.to_bits(), b.r.to_bits());
```

结果：**13/13 逐位一致**。

### 5.3 一个未被观测的边界

ziggurat 的最后一个桶（`i == 255`）需要 `ZIG_NORM_F[i+2]`，而表只有 257 项。
真实游戏走这一桶时取什么值**我们没有观测到**（夹具也没触发），
所以实现里遇到就越界重抽，**不凭空造值**。这在代码里有注释说明。

### 5.4 为什么这一步重要

服务端要能自己造关卡，就必须有可用的生成器。现在它既存在、
又被真实数据逐位验证过——不是"照着公式抄一遍"而已。


## 6. 从生成到发送的调用链

### 6.1 确定：从"生成"到"发送"的调用链

hook libc `send`，在报文里出现 `AddObstacle` 的那一刻取调用栈，得到稳定链路：

```
GW+0x8f46c3   ← 发送封装（send 的返回点）
GW+0x45d404   ← 构造/发送报文
GW+0x3d8d2b   ← 广播
GW+0x45a60c   ← 处理"添加障碍"
GW+0x3f1d85   ← 调用者（疑似生成循环）
GW+0x3d21f5
GW+0x3d3730
GW+0x451be5
GW+0x438876
GW+0x43b6cb
GW+0x90ab4f   ← 线程入口
```

三帧样本的链路完全一致，说明这是唯一的产生路径。

**这条链是目前最可靠的抓手**：随机数一定在 `0x3f1d85` 及其更上层被消费。


## 7. 结论演进：这些旧结论**已被推翻**，不要引用

关卡生成这条线在 2026-10/11 走过一段弯路，过程记录全部保留在 `archive/`，
但**旧结论作废**。按时间顺序：

| 旧文档 | 当时的结论 | 现在的状态 |
|---|---|---|
| `40-level-generator.md` | 隐藏 921 次 `getrandom` ⇒ 随机源是 OS 熵，**不可确定性复现** | **推翻**。915 次来自 libudev（Godot 硬件枚举），游戏本体只有 3 次且全是 `HashMap` 播种（见 [`12-rng.md`](12-rng.md) §5） |
| `48-rng-replay-attempt.md` | 抓 `getrandom` 返回值当种子去重放——**没成功** | 方法错：Rust 的 `getrandom` crate 走原始 syscall（318 号），hook libc 看不到 |
| `50-rng-source-reopened.md` | 用调用栈把"OS 熵"推翻，随机源**重新成为未知** | 这一步是对的，是后续 `53`/`57` 的起点 |
| `52-rng-hunt-status.md` | 只确定了"生成 → 发送"的调用链 | 已被 `53` 超越；调用链本身仍有效（§6） |
| `53-rng-found.md` §4 | "ChaCha 找到了，但重放没对上" | 已被 `55`（公式闭合）+ `56`（13/13 逐位）取代 |
| `54-rng-replay-verified.md` | 随机流 → 真实障碍坐标精确命中 | **仍有效**，是 §3 的方法基础 |

## 8. 复现

| 要复现什么 | 命令 / 入口 |
|---|---|
| 公式与 ziggurat 表 | `analysis/` 侧脚本 + `.rodata` 两张表地址（§2.1） |
| 13/13 逐位重放 | `tools/capture_rand_values.js`（hook `0x47f700` 取 `next_u64` 返回值）+ 真实 `AddObstacle` 报文 |
| Rust 侧逐位对照 | `cargo test --offline`（`src/levelgen.rs` 的单测：`generate` 13/13、`generate_center` 4 个真值样本） |
| 客户端方法 | `GW2Rebuild.level_obstacles(level_index, seed)`（[`16-client.md`](16-client.md)） |
| 圆盘式的否定 | `tools/` 下的反汇编检查脚本，见 `archive/103-disk-levelgen.md` §7 |
