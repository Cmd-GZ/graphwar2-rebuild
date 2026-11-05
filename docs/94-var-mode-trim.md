# 94 · `FunctionMode`（Normal / DiffEq）是**生成时**区分，还是**生成后裁剪 y**

> 只读静态分析（Ghidra 12.1.4 + objdump），**未改动任何代码，未跑 `cargo test`**。
> 本条回答的问题是：`FunctionMode` 到底在"生成 token 时"就分开，还是"先生成含 `y` 的
> 流、再按模式删掉/替换 `y`"？——因为这决定了 `src/aigen.rs` 的双表实现是否正确。

## 1. 结论

**在生成时就区分，而且是"用模式直接当 tag"。**

`FUN_005ef020`（`y = f(x)` 随机生成器）只读一次模式参数：

```
0x5ef0c7  and  $0x1,%bpl           ; mode & 1
0x5ef0cf  mov  %rax,0x40(%rsp)     ; 存进栈槽
...
0x5ef230  mov  0x40(%rsp),%r13     ; 全程唯一一次读回
0x5ef235  cmp  $0x15,%r12d         ; 权重下标 < 21 ?
0x5ef239  jb   0x5ef2b0            ;   是 ⇒ 该 token 的 tag = r13 = (mode & 1)
```

也就是说，85 格权重表里**第 17–20 这 4 格**（`y` 的 4/85）产出的 tag 不是常量，
而是 `mode & 1`：

* DiffEq（`mode & 1 == 1`）→ tag `1` = `y`；
* **Normal（`mode & 1 == 0`）→ tag `0` = `x`**。

⇒ **Normal 模式从头到尾不产生 `y` token，因此没有东西可"裁"。**
`y` 和 `x` 是同一个 4 格槽位在两种模式下的两种取值，不是"先造出来再改"。

**对实现的影响：`src/aigen.rs` 的双表实现不用改，而且比"同位置替换"更强——它连区间边界都和二进制逐格相同（见 §9.1）。**

## 2. 先纠正一个前提：`0x5ef2b0` 不是独立函数

题目把 `0x5ef2b0` 当成"token 分派器"，`procedure_info` 表明它是
**`FUN_005ef020` 内部的一条公共出口**（"把 tag+value 写进 `Vec<Token>`、然后回去取下一个"）：

| 项 | 值 |
|---|---|
| 所属函数 | `FUN_005ef020`（vaddr `0x4ef020`，718 字节，44 个基本块） |
| 函数体 ranges | `0x5ef020–0x5ef0e6` / `–0x5ef177` / `–0x5ef21b` / `–0x5ef22d` / `–0x5ef230–0x5ef304` |
| `0x5ef2b0` 的角色 | 分派链的汇合点；写入 `Vec` 后判容量、回循环头 |

跳进 `0x5ef2b0` 的指令共 **11 条**，全部在同一个函数体内
（`0x5ef217`、`0x5ef229`、`0x5ef239`、`0x5ef245`、`0x5ef251`、`0x5ef25d`、
`0x5ef269`、`0x5ef275`、`0x5ef284`、`0x5ef290`、`0x5ef29c`），另有 `0x5ef2de`
先跳到 `0x5ef284` 再转来。全是区间比较分支的下落点。
**没有任何外部函数调它**，也没有任何"另一个分派器"。

⇒ 题目里"分派器 vs 调用者"其实是"权重链 vs 生成器调用点"，见 §5。

## 3. 完整权重链（逐条读死）

函数入口 `0x4ef020`，签名（Ghidra）：

```c
void FUN_005ef020(long *out /*rdi: Vec<Token> 返回值*/, byte mode /*esi*/)
```

第一步是长度（`normal()*5.0+10.0` → 饱和截断 → 负数/0 直接返回空向量），
常量已核对：`0x19f698 = 5.0`、`0x19f6a0 = 10.0`、`0x19fbc0 = -2147483648.0`、`0x19f7e0 = 2147483647.0`。

循环里每一轮：

```
0x4ef193  mov  0x10(%r13,%rcx,4),%r12d   ; r12 = 一个 u32 随机数（rand_chacha 风格的 u32 缓冲）
0x4ef1b3  imul %rbp,%rax                 ; rbp = 0xc0c0c0c1  ⇒ 下面的整除魔数
0x4ef1b7  shr  $0x26,%rax
0x4ef1bb  imul $0x55,%eax,%eax           ; ×85
0x4ef1be  sub  %eax,%r12d                ; r12 = rand % 85
```

然后是一条**线性区间梯**（不是跳转表，所以反编译出来是嵌套 `if`）：

| 权重下标 `r12` | 上界比较指令 | 产出的 tag | 权重 |
|---|---|---|---|
| `[0,11)` | `0x4ef1c1 cmp $0xb`，`jae` 落到下一档 | `0x11` = 17 `const`（值 = `normal()*10.2`，`0x19f758`） | 11 |
| `[11,17)` | `0x4ef220 cmp $0x11`，`jae` 落到下一档 | `xor %r13d,%r13d`（`0x4ef226`）⇒ **0 = `x`** | 6 |
| **`[17,21)`** | **`0x4ef235 cmp $0x15` / `jb 0x4ef2b0`** | **`r13 = [0x40(%rsp)] = mode & 1`**（`0x4ef230`） | **4** |
| `[21,31)` | `0x4ef241 cmp $0x1f` / `jb` | 3 `+` | 10 |
| `[31,36)` | `0x4ef24d cmp $0x24` / `jb` | 4 `-` | 5 |
| `[36,47)` | `0x4ef259 cmp $0x2f` / `jb` | 6 `*` | 11 |
| `[47,54)` | `0x4ef265 cmp $0x36` / `jb` | 7 `/` | 7 |
| `[54,56)` | `0x4ef271 cmp $0x38` / `jb` | 8 `^` | 2 |
| `[56,59)` | `0x4ef277 cmp $0x3b`，`jae` 落到下一档 | `r12-0x2f` ⇒ 9,10,11 = `sqrt`,`log`,`ln` | 1+1+1 |
| `[59,65)` | `0x4ef28c cmp $0x41` / `jb` | `0xc` = 12 `exp` | 6 |
| `[65,72)` | `0x4ef298 cmp $0x48` / `jb` | `0xd` = 13 `abs` | 7 |
| `[72,79)` | `0x4ef2a4 cmp $0x4f`，`jae` 落到下一档 | `0xe` = 14 `sin` | 7 |
| `[79,84)` | `0x4ef2d0 cmp $0x54` + `sbb $0x0` | `0x10-1` = **15 `cos`** | 5 |
| `[84,85)` | 同上，`sbb` 不进位 | 16 `tan` | 1 |

> 读法：每个分支都是"先把本档 tag 装进 `%r13`，再 `cmp` 上界，未越界就 `jb 0x4ef2b0`
> 把它写进 `Vec`"。所以 `cmp` 地址是**本档的上界**。

合计 `11+6+4+10+5+11+7+2+1+1+1+6+7+7+5+1 = 85`，与 `TOTAL_WEIGHT = 85`、
与 `docs/AI.md` §2 的权重表**逐项相同**。

## 4. `mode` 参数的生命周期（这条是关键）

在 `0x4ef020–0x4ef304` 这 718 字节里，栈槽 `0x40(%rsp)` 只出现两次：

```
0x4ef0cf  mov    %rax,0x40(%rsp)      ; 写：rax = movzbl(mode & 1)
0x4ef230  mov    0x40(%rsp),%r13      ; 读：唯一一次
```

字节级证据：

```
4ef0c7: 40 80 e5 01     and    $0x1,%bpl
4ef0cb: 40 0f b6 c5     movzbl %bpl,%eax
4ef0cf: 48 89 44 24 40  mov    %rax,0x40(%rsp)      ← 写
...
4ef230: 4c 8b 6c 24 40  mov    0x40(%rsp),%r13      ← 读（唯一）
4ef235: 41 83 fc 15     cmp    $0x15,%r12d
4ef239: 72 75           jb     4ef2b0               ← 直接跳去写 token
```

写入 token 的指令只有两条（整个函数里仅此一处）：

```
4ef0fc: 4c 89 2c 08     mov    %r13,(%rax,%rcx,1)        ; token.tag
4ef106: f2 0f 11 44 08 08  movsd %xmm0,0x8(%rax,%rcx,1)  ; token.value
```

⇒ **严格"生成时决定"**：`mode` 不参与任何"事后遍历 / 回写 tag"的路径，
因为函数体内除了这一条 append 之外没有任何对 `Vec` 元素的写入（另一处
`0x4ef2ea` 只写 `len`，是返回值封箱，不是改写）。

## 5. 调用者：7 处，全是 `generate → repair` 成对，中间不插东西

`objdump -d libgraphwarrust.so | grep "call.*4ef020"` 给出 **7 个直接调用点**
（Ghidra 的 `procedure_callers` 返回空，并自述"有 7 条 call 引用未归入 caller 清单"，
`relationship_coverage = partial`；所以这里以 objdump 为准）：

| # | generate 调用点 | 同一函数里紧接着的 repair 调用 | 形态 |
|---|---|---|---|
| 1 | `0x4ee470`（evolve/factory 体内） | `0x4ee61d` | 变异/重生成/交叉三分支 |
| 2 | `0x51b90f` | `0x51b923` | AI 主循环的一份单态化 |
| 3 | `0x51c3f6` | `0x51c40a` | 同上（不同难度常量） |
| 4 | `0x51c731` | `0x51c745` | 同上 |
| 5 | `0x51c9eb` | `0x51c9ff` | 同上 |
| 6 | `0x51d185` | `0x51d199` | 同上 |
| 7 | `0x51df27` | `0x51df3b` | 同上 |

6 个 AI 侧调用点的指令序列**完全一致**（只是栈帧偏移不同，以 `0x51b900` 为例）：

```
51b900: 0f b6 5c 24 07   movzbl 0x7(%rsp),%ebx   ; 取模式字节
51b905: 48 8d bc 24 80..  lea    0x80(%rsp),%rdi
51b90d: 89 de            mov    %ebx,%esi        ; arg2 = mode
51b90f: e8 0c 37 fd ff   call   4ef020           ; generate(mode)
51b914: 48 8d 7c 24 10   lea    0x10(%rsp),%rdi
51b919: 89 de            mov    %ebx,%esi        ; arg2 = mode（同一个寄存器）
51b91b: 48 8d 94 24 80..  lea    0x80(%rsp),%rdx  ; arg3 = 刚生成的 Vec
51b923: e8 b8 27 fd ff   call   4ee0e0           ; repair(mode, vec)
```

两次调用之间**没有第三个调用**、没有循环、没有对 `0x80(%rsp)` 的遍历。
`repair`（`FUN_005ee0e0`）拿到 `mode` 也只做一件事（见 §6），不会再交出去。

`FUN_005ee720`（变异）**根本不收 mode**（只有 `rdi` 输出、`rsi` 父代两个参数，
`0x4ee72e mov %rsi,%rbx` / `0x4ee731 mov %rdi,%r14`），所以它也不可能按模式改 tag。

## 6. `repair` 也不改写已有元素，只追加 + 截断

`FUN_005ee0e0` 的伪代码里：

```c
local_38 = param_2 & 1;                         // 模式，取一次
... uVar5 = local_38;                           // 42..49：tag = mode&1
... if (uVar4 < 0x2a) uVar5 = 0;                // 25..41：tag = 0 = x
*(ulong  *)(lVar3 + lVar2 * 0x10) = uVar5;      // lVar2 = 当前 len ⇒ 只写"末尾"
*(double *)(lVar3 + 8 + lVar2*0x10) = dStack_50;
param_3[2] = uVar5_len + 1;                     // 只改 len
```

* 补齐三段与 `docs/AI.md` §6 一致：`rand%50` 的 `<25` → 常数、`25..41` → `x`、`42..49` → `mode&1`；
* 截断（`uStack_48`）只用 `DAT_002c47d0` 这张**与模式无关**的 arity 表；
* 全函数**没有任何一处读回已有 token 的 tag 再写回**。

arnity 表（file/vaddr `0x1c47d0`，18 × i32）实测：

```
0:x=-1 1:y=-1 2:y'=-1 3:+ =1 4:- =1 5:neg=0 6:* =1 7:/ =1 8:^ =1
9:sqrt=0 10:log=0 11:ln=0 12:exp=0 13:abs=0 14:sin=0 15:cos=0 16:tan=0 17:const=-1
```

⇒ `x` 与 `y` 的 arity 贡献**都是 −1**。这也解释了为什么"把 `y` 换成 `x`"
在结构上不可能有任何影响：两者在装配器眼里完全同类。

## 7. 全二进制扫描：不存在"按模式改写 token 流"的独立 pass

### 7.1 方法

先从 `objdump -d libgraphwarrust.so` 全量反汇编（1 521 070 条指令，89 MB），
按 `analysis/funcstarts_ghidra_va.pkl` 切函数，然后找同时满足两条的函数：

1. **tag==1 测试**：`mov (mem0),%r` 后 1–3 条内出现 `cmp $0x1,<同一寄存器的任一宽度>`；
2. **写 0 到 offset 0**：同一函数内、同一基址寄存器、距离 ≤ 60 条指令有 `mov[lwqb] $0x0,(mem0)`。

这正是

```rust
if t.tag == 1 { t.tag = 0 }      // y → x 原地改写
tokens.retain(|t| t.tag != 1)    // 删除（压缩）——也是上面两个特征的上位集
```

在机器码上的样子。（脚本：`.rea-work/v94/scan_trim2.py`，可重跑。）

### 7.2 结果

| 量 | 数量 |
|---|---|
| `tag==1` 测试点 | 226 |
| 写 0 到 offset 0 的点 | 1721 |
| 同函数 + 同基址 + ≤60 条内配对 | **21** |
| **落在 AI 候选流水线 `[0x4ee0e0,0x4ef520]` ∪ `[0x51b800,0x51e000]` 的** | **0** |

21 个配对里能看的几个（`0x4d85e0`、`0x5a5160`、`0x8a7fa0` …）形状完全一样：

```
mov  (%rsi),%rax        ; 读一个 Option/Box 的判别值
movq $0x0,(%rsi)        ; 无条件清空槽位（take/mem::replace 语义）
cmp  $0x1,%rax          ; 再用"取出的旧值"分支
jne  ...
```

即 `Option::take()` 一类，**不是** "tag==1 才写 0"，也不在 AI 代码里。

### 7.3 扫描的边界（不要当成绝对证明）

* 只认立即数 `$0x1` / `$0x0`；用跳转表按 tag 分派、或先 `xor` 再把寄存器写进内存的写法**抓不到**；
* 只认 `cmp`，不认 `test`/`bt`；
* 函数边界来自 Ghidra 表；`0x5ee3a8–0x5ee72x` 这段（evolve/factory 主体）
  **Ghidra 没建成函数**（`FUN_005ee3a0` 只有 8 字节），所以它的边界是 objdump 线性反汇编给的。

因此 §7 是"**有界否定**"；真正把结论钉死的是 §3–§6 的**正向**证据：
`y` 的 tag 只在 `0x5ef230` / `0x5ee2xx` 两处由 `mode & 1` 产生，
Normal 下这两处都等于 0。

## 8. 行为层佐证（用既有观测数据，未重新验证）

题目给的出口观测：

| 模式 | `x` | `y` |
|---|---|---|
| Normal | 11.92% | 0% |
| DiffEq | 6.83% | 4.86% |

| 假说 | Normal 下 `x` 的预测占比 | 与 11.92% 的差 |
|---|---|---|
| **`y` 槽位在 Normal 下产出 `x`**（本文结论） | 10/85 = **11.76%** | **0.16 个百分点** |
| `y` 被**删除**（6/85 生成、再滤掉 4/85） | 6/85 = 7.06%，或过滤后 6/81 = 7.41% | **4.5 个百分点** |

⇒ 观测值直接压死"删除"假说。DiffEq 侧 `6.83% / 4.86%` 对 `6/85 = 7.06% / 4/85 = 4.71%`
也都在 0.25 个百分点内。

> 这是**统计佐证**（推断级），不是指令级证据；指令级证据在 §3–§5。

## 9. 对实现的影响

### 9.1 不用改，而且双表与二进制**逐格**等价

`src/aigen.rs` 现在是两张表 + 累计权重线性扫描。把两张表的**累计区间**列出来：

| 下标区间 | `TABLE_NORMAL` 给 | `TABLE_DIFFEQ` 给 | 二进制 `FUN_005ef020` 给 |
|---|---|---|---|
| `[0,11)` | `Const` | `Const` | `0x11` const |
| `[11,17)` | `VarX` | `VarX` | `0` x |
| `[17,21)` | `VarX`（`TABLE_NORMAL` 里 `VarX` 是**单项权重 10**，覆盖 `[11,21)`，所以后 4 格仍是 `VarX`） | `VarY` | `mode & 1` |
| `[21,85)` | 同 | 同 | 同（§3 表） |

即：**Normal 表在 `[17,21)` 继续给 `VarX`，正好等于二进制的 `mode & 1 == 0`**；
DiffEq 表在 `[17,21)` 给 `VarY`，正好等于 `mode & 1 == 1`。
对任意一次 `rand % 85`，两边产出的 tag **逐格相同**。

因此：

* token 流**长度**相同（长度由 §3 第一段单独抽，与模式无关）；
* **arity/装配**相同（`x` 与 `y` 的 arity 贡献都是 −1，§6）；
* **RNG 消耗**相同（每个 token 恰好 1 次 u32 抽样，常数再多 1 次正态；两处 `mode & 1`
  分支都不额外取样）；
* 结论：**双表实现不需要改**。任务书里"同位置替换 ⇒ 完全等价"的判断成立，
  而且实际发生的比"替换"更简单——Normal 下 `y` 根本没被生成过。

### 9.2 唯一需要留意的是"别把它实现成删除"

若哪天有人把实现改成"先生成 85 格随机流、再按模式删掉 `y`"，那就**错了**：
会同时改变 token 数、arity 装配点和最终 `V`（长度惩罚）——而真二进制不会。
`src/aigen.rs:812` 的 `normal_mode_never_emits_y` 测试正好守住这条，不用动。

### 9.3 附注：一处**与本题无关**的抽样宽度差异（**未改，不建议在本轮改**）

二进制每个 token 是**一次 `u32` 抽样**（`0x4ef193 mov 0x10(%r13,%rcx,4),%r12d`，
`rand_chacha` 式 64 个 u32 的块缓冲）；而 `src/aigen.rs` 的 `next` 是
`r.next_u64()`（`src/aigen.rs:462`），`src/rng.rs:80` 的 `next_u64` 一次吃掉
ChaCha 的**两个 32 位字**。所以即使从同一个种子出发，我们的 token 抽取值也会和
二进制不同。

* 证据等级：**观测**（两侧代码/指令都在）；
* 影响：只影响"逐位复现整个 AI 搜索"，而 `docs/AI.md` §10 已写明搜索过程
  **本来就不可逐位复现**（种子来自 OS 熵），所以现有 21 条测试不依赖它；
* **本轮不改**，也不在本文档范围内下结论。

## 10. 复现命令

```bash
cd /home/user/MyProject/gw2

# 生成器全文（含 0x4ef2b0 分派汇合点）
objdump -d --start-address=0x4ef020 --stop-address=0x4ef400 libgraphwarrust.so

# 全部 generate 调用点 / 全部 repair 调用点
objdump -d libgraphwarrust.so | grep -E "call +4ef020 |call +4ee0e0 "

# arity 表与浮点常量
objdump -s --start-address=0x1c47d0 --stop-address=0x1c4818 libgraphwarrust.so

# 全二进制"按模式改写 tag"扫描（约 4 s；脚本按 cwd 读 full.asm）
objdump -d libgraphwarrust.so > .rea-work/v94/full.asm
(cd .rea-work/v94 && ../../.venv/bin/python scan_trim2.py)
```

REA 侧：`procedure_pseudo_code(0x5ef020)`、`procedure_pseudo_code(0x5ee0e0)`、
`procedure_info(0x5ef020)`、`procedure_references(0x5ef020, incoming)`。
地址一律 **Ghidra = vaddr + 0x100000**；本文 objdump 行用 vaddr，`FUN_xxxxxxxx` 用 Ghidra。

## 11. 证据等级汇总

| # | 结论 | 等级 | 依据 |
|---|---|---|---|
| 1 | `0x5ef2b0` 是 `FUN_005ef020` 内部汇合点，非独立函数 | **观测** | `procedure_info` body ranges + 13 条入边全是函数内分支 |
| 2 | 权重 85 格、区间边界、tag 映射（§3 表） | **观测** | 逐条指令 + 字节 |
| 3 | `mode` 只取一次 `&1`、只喂给 `[17,21)` 那一格 | **观测** | 栈槽 `0x40(%rsp)` 全函数仅 2 次访问 |
| 4 | **Normal 下 `y`（tag 1）永不产生** | **观测** | 3 的直接推论；tag 1 的两个产生点都乘了 `mode & 1` |
| 5 | 全部 7 处调用点都是 `generate → repair` 成对、无中间 pass | **观测** | objdump 全量 grep + 6 处逐字节同构 |
| 6 | `repair` 只追加/截断，不改写已有 tag | **观测** | 伪代码 + 唯一的元素写指令地址 |
| 7 | 全二进制无"tag==1→0"改写循环 | **有界否定** | 扫描（§7.3 列了盲区） |
| 8 | 观测占比排除"删除 `y`" | **推断（统计）** | §8 表，用题目给的出口观测 |
| 9 | 双表实现与二进制逐格等价 ⇒ 不用改 | **推断** | 4 + 5 + 6 + 9.1 的区间对照 |
| 10 | `next_u64` vs 二进制 `u32` 抽样宽度差异 | **观测** | §9.3 |

### 仍未解 / 没拿到

* `0x5ee3a8–0x5ee72x`（evolve/factory）**没有 Ghidra 函数**，本文对它的阅读是
  objdump 线性反汇编，未经反编译器交叉验证。它与本题结论无关（该段只在三种工厂分支里
  调 `generate`/`mutate`/`repair`），但若要研究工厂本身需要重建函数边界。
* "先裁 y、再装配"这种写法在本二进制里**未观测到**；本文只证明"没有这样做"，
  不证明作者原本的设计意图（那需要源码，拿不到）。
