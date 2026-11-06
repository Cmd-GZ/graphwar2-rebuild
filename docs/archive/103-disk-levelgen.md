# 103 · "圆盘式"（`0x4ea6dc`）：**不是**第三种关卡生成模式

> 一句话：`0x4ea6dc` 确实存在、确实是"往一个 Vec 里撒 80 个极坐标点"，但它
> **不调 `place_obstacle`、写出的记录不是障碍、也没有任何关卡流程引用它**。
> 它是跳转表分派器 `FUN_004ea690` 的 **case 0**，一族的"图案段"生成器之一。
> 关卡生成只有**两个**生产者：随机散布与中心单障碍。
>
> 证据等级：本文所有地址/常量/记录布局都是**观测**；"这是线段/是动画"是**推断**；
> 没有真值样本这一点是**已核实的不存在**（见 §7）。

---

## 1. 地址：先纠正三处坐标

| 说法 | 值 | 判定 |
|---|---|---|
| `archive/ai/70` §1–2 写的"`0x5ea6dc`" | 当成虚地址 | ❌ 那是 **Ghidra 地址** |
| `archive/ai/72` §5 表格里的圆盘式"虚地址 `0x4ea6dc`" | 虚地址 | ✅ 正确 |
| **本文采用** | **虚地址 `0x4ea6dc` = Ghidra `0x5ea6dc`** | ✅ |

换算仍是老规矩：`Ghidra = 虚地址 + 0x100000`（`.text` 的 `p_offset` 比 `p_vaddr`
小 `0x1000`，所以 `Ghidra = 文件偏移 + 0x101000`）。交叉验证：
`FUN_005ea6dc` 里调 `FUN_0057f700`，正是 `next_u64` 的 Ghidra 地址
（虚地址 `0x47f700`）——换算一致。

> ⚠️ **本轮踩到的坑（比地址错更隐蔽）**：给 REA 一个*不存在*的过程地址时，
> `procedure_pseudo_code` 会把它当成**文件偏移**再映射（`+0x101000`），
> **不报错**，直接返回一个完全无关的函数。我第一次用 `0x6ea6dc`（= 把 Ghidra 地址
> 又加了 `0x100000`）就拿到了 `FUN_007eacb0`——一个 SSE 图像滤波循环，差点信了。
> 判别方法：看返回里的 `selection.mapped_address`，或先 `procedure_info` 确认
> 该地址真的是函数入口（本例 `procedure_info 0x5ea6dc` → `FUN_005ea6dc`，371 字节）。

---

## 2. 真正的入口：跳转表分派器 `FUN_004ea690`

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

## 3. 圆盘 case 的算法（观测）

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

### 3.1 记录布局（0x30 = 48 字节）

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

### 3.2 参数汇总（回答任务问的三项）

| 项 | 值 |
|---|---|
| 数量 | **80**（64 + 16），与随机流无关，恒定 |
| 半径 | 外圈 `u2·7.5`（均匀在 [0,7.5)）、内圈 `u2·3.0`（均匀在 [0,3)） |
| 位置 | 极坐标 `(sin(2π·u1)·r, cos(2π·u1)·r)`，**圆心恒为原点**（中心坐标来自入参，不参与随机） |
| 分布 | 圆盘内**面**均匀（半径均匀、角度均匀），不是圆环 |

---

## 4. RNG 调用顺序与次数（观测）

* 每条记录 **恰好 2 次** `next_u64`（`FUN_0057f700`）：**先 u1（角度），后 u2（半径）**；
* 一批 **80 × 2 = 160 次**；
* 只有均匀数，**没有** ziggurat 正态采样、**没有**拒绝采样 ⇒ 消耗是定长的。

这一点与随机散布正相反：散布每个障碍要 `normal()`（拒绝采样，不定长）+ 2 个均匀数，
一批 17 个障碍就吃掉 120 个随机数（`fixtures/levelgen_draws.txt` 正好 120 个）。

---

## 5. 与"随机散布"的逐项对照

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

## 6. 为什么它不是关卡生成器（5 条独立证据）

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

## 7. 验证

### 7.1 用 replay 反查形状（真值扫描）

`~/.local/share/godot/app_userdata/Graphwar/replays/`：**380 个 replay、4217 条
`AddObstacle`、361 条 `ClearObstacles`**。按 `ClearObstacles` 切分成 **274 个关卡实例**：

| 统计量 | 结果 |
|---|---|
| 每关障碍数 | **1 … 35**（众数 15、10、19、16） |
| 障碍数 ≥ 60 的关卡 | **0 个** |
| 半径范围 | **0.8 … 7.651520295358276** |

* 圆盘式一批是 **80** 条，且半径分布是 `[0,7.5)` 均匀；真值里**没有任何一关**接近这个形状 ⇒ 与 §6 一致。
* 半径下界**恰好 0.8**，正是散布式 `max(N·1.6+2.6, 0.8)` 的下限 ⇒ 真值里全是散布式（外加少量中心式）。

### 7.2 有没有"圆盘式"真值样本？——**没有，而且不可能有**

因为该函数不产生障碍，replay 里不会有它的产物；抓包里也没有 mode/pattern 字段
（`archive/ai/72` §3 已确认协议层面无模式位）。所以本文的算法**只能**由静态证据支撑，
`cargo test` 只能锁"实现 ↔ 反出来的公式"，**不能**声称"实现 ↔ 真实关卡"。

### 7.3 附带成果：**中心单障碍拿到 4 个真值样本**

274 个关卡实例里有 **4 个只有 1 个障碍、位置恰为 `(0.0, 0.0)`**：

```
r = 3.337540   4.811697   6.750724   5.694277
```

全部落在 `[3, 8)`，与 `generate_center` 的 `u*5.0 + 3.0` 一致（**观测**，
不是逐位——没有对应的随机流）。这是 `archive/ai/72` §2.1 中心模式第一次有运行时真值。

---

## 8. 顺带纠正：`place_obstacle` 有 **5** 个调用点，不是 3 个

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

## 9. 实现与测试

`src/graphwar_level_generator.rs` 新增：

* `PatternSegment` —— 0x30 字节记录（含布局注释）；
* `generate_disk_pattern(next, type_code, centre)` —— 本文 §3 的实现；
* `DISK_PATTERN_KIND = 0` —— 分派器表里的 kind；
* `push_disk_pattern(...)` —— 一个圆盘子过程（64/16 共用）。

5 条新测试（`cargo test`，`CARGO_TARGET_DIR` 独立）：

| 测试 | 锁住什么 |
|---|---|
| `disk_pattern_is_two_disks_of_64_and_16` | 80 条、64/16 分组、全部落在各自圆盘内、field/flag/style/kind 取值、最大半径接近外沿 |
| `disk_pattern_consumes_two_draws_per_segment` | 整批恰好消耗 160 个随机数 |
| `disk_pattern_follows_the_draw_stream_exactly` | 第 k 条用第 2k、2k+1 个数；`+0x10 = sin·r`、`+0x18 = cos·r` 逐位 |
| `disk_pattern_first_60_follow_real_draw_stream` | 用**真实随机流**夹具（120 个数 = 60 条）逐位回归 |
| `disk_pattern_radii_are_uniform_inside_the_disk` | 半径均值 ≈ 7.5/2（区分圆盘与圆环） |

**未覆盖**：`sin/cos` 的位级一致性。汇编调的是 glibc `sincos`（GOT `0x9bbed0`），
Rust 用的是 `f64::sin_cos()`；两者都是同一套多项式，但没有证据证明最后 1 ulp 相同。
所以本文只声称"RNG 流与算术**逐位**"，三角函数末位**未验证**。

---

## 10. 证据等级

| 结论 | 等级 |
|---|---|
| 虚地址 `0x4ea6dc` = Ghidra `0x5ea6dc`；`place_obstacle` 5 个调用点；跳转表 28 项 | **观测**（地址/字节可指认） |
| 64+16 条、`r=7.5u`/`3.0u`、`sin`→`+0x10`、`cos`→`+0x18`、`style=0xf`、`flag=1/0` | **观测**（汇编 + 常量地址） |
| 每记录 2 次 `next_u64`，先角度后半径；一批 160 次 | **观测** |
| 它不是关卡生成器；关卡只有散布 + 中心两种 | **观测 + 运行时（0 次调用）+ 真值（274 关无此形状）** |
| 记录是"线段"、整条链路是"逐帧动画" | **推断**（`+0x00` 语义与 `/60.0` 的用途未直接验证） |
| `kind` 由哈希表资源键决定、哪个 build 会触发 kind=0 | **未知** |
| 另外 27 个 case 的图案 | **未知**（未展开） |

---

## 11. 复现命令

```bash
# 反编译（注意用 Ghidra 地址；先 procedure_info 确认入口）
rea ...            # 或在本仓库用 REA MCP：
#   procedure_info 0x5ea6dc            -> FUN_005ea6dc, 371 字节
#   procedure_pseudo_code 0x5ea6dc
#   procedure_pseudo_code 0x5ea690     # 分派器
#   procedure_pseudo_code 0x5f1e50 / 0x5f17a0   # 散布 / place_obstacle

# 跳转表 + 常量
python3 - <<'EOF'
import struct; d=open("../../libgraphwarrust.so","rb").read()
for i in range(28):
    print(i, hex(0x1c3ba8 + struct.unpack_from("<i", d, 0x1c3ba8+4*i)[0]))
for a in (0x19f578,0x19f628,0x19f858,0x19fdc8):
    print(hex(a), struct.unpack_from("<d", d, a)[0])
EOF

# 障碍生产者（不依赖 Ghidra）
# 扫 .text 的 E8 rel32 → 0x3f17a0，见 §6 第 1 条

# 真值形状扫描
python3 tools/parse_replay.py ...   # 或直接按 ClearObstacles/AddObstacle 切分

# 测试
CARGO_HOME=<ws>/.cargo-home CARGO_TARGET_DIR=<ws>/.agent-work/disk-levelgen/target \
  cargo test --offline
```

---

## 12. 仍未确定

1. 这 80 条记录最终被谁画出来（哪个 Godot 侧特性/资源），`style=0xf`、`flag` 的作用；
2. `kind` 从哈希表里取值的那个"资源 id"是什么，哪个模式/界面对应 kind=0；
3. 同族另外 27 个 case 的图案（`0x4ea8a0`/`0x4ea9bc`/`0x4eab32`/`0x4eaca6` 只是其中 4 个）；
4. 中心单障碍为什么存在 3 份拷贝（`0x49f1c0`/`0x49f370`/`0x4a5e80`）——是 3 个入口还是历史遗留。
