# 119 · token → 文本：`0x4ec160` 的还原

状态：**已还原并实现**（分支 `agent/token-to-text`）。`cargo test` = **256 条全绿**（242 → 256）。
证据分级沿用仓库约定：**逐位验证**（`cargo test` / unicorn 对拍覆盖）/ **观测**（能指到地址或字节）/
**推断**（写明缺什么）/ **未知**。地址一律是 **vaddr**（Ghidra = vaddr + `0x100000`）。

> **一句话**：`0x4ec160` 把前缀 token 序列递归写成文本，格式是
> **"二元运算一律加一层圆括号、一元负号不加括号、函数名后紧跟圆括号、常量固定两位小数、
> 一个空格都不加"**；`((2.00*x)+1.00)`、`(-1.06*x)`、`tan(sin((-0.12*x)))` 都逐字符对得上。
> 它**不是单射**：`π → 3.14`、`neg` 当 `^` 左底座时括号会丢、缺操作数的空位在文本里无法表达。
> 这三条都是**真二进制行为**，我们用两份真值夹具钉住。

---

## 0. 为什么这个函数是"AI 上不了线"的最后一块

`aigen` 进化出来的是 **token**（[`src/aigen.rs`](../src/aigen.rs) 的 `FunctionSlot.tokens`），
而协议 `FunctionInfo.function` / `corrected_function` 是**文本**
（`{"FunctionInfo":{"owner_id":234,"function":"0.00","corrected_function":"0.00 + 4.638"}}`）。
中间那一步就是 `0x4ec160`。`docs/109` §6-H、`docs/117` §4/§6 都把它列为唯一的缺口。

本文只补**这一块**，不接线：`src/serverloop.rs` / `src/game.rs` / `src/turn.rs` /
`src/aigen.rs` / `src/protocol.rs` 一行未改（见 §8）。

---

## 1. 先证明它是函数入口（这一步不能省）

REA 有个静默陷阱：给非入口地址不报错，而是把它当**文件偏移**（`+0x101000`）返回无关函数
（`AGENTS.md`）。所以先用**字节**确认，再谈语义：

| 检查 | 结果 | 等级 |
|---|---|---|
| 段表换算 | `.text`：vaddr `0x37b940` / 文件偏移 `0x37a940` ⇒ **vaddr = 偏移 + `0x1000`**；`.rodata` 两者相等 | 观测 |
| 前一条指令 | `0x4ec158..0x4ec15f` 全是 `cc`（`int3` 填充） | 观测 |
| 序言 | `0x4ec160`：`55 41 57 41 56 41 55 41 54 53 48 81 ec 88 00 00 00`（`push rbp/r15/r14/r13/r12/rbx` + `sub $0x88,%rsp`） | 观测 |
| 函数起点表 | `analysis/funcstarts_ghidra_va.pkl`（13263 条）里 `0x4ec160` 是**精确成员**，前一个成员是 `0x4ec119` | 观测 |
| 自调用 | 函数体内 **19 处** `call 0x4ec160`（递归）；另有 **3 处外部调用**，见 §4.3 | 观测 |

> ⚠️ 同一个 pickle 在 `0x4ec160..0x4ed420` 之间还列了 22 个"起点"——那是 Ghidra 把
> switch 的目标地址也当成了函数入口（本函数的 jump table 有 18 个目标），**不影响**
> "`0x4ec160` 是入口"这一条。反过来，把 **Ghidra 地址 `0x5ec160`** 拿去查这张表是**查不到**的
> （已实测），这正是 `AGENTS.md` 里那个坑的量化版本。

**签名**（由序言后的取参逐条可读）：

```
fn to_text(out: &mut String, buf: &TokenBuf, pos: &mut usize)   // rdi 是 sret
```

| 指令 | 含义 |
|---|---|
| `4ec171: mov %rdi,%rbx` | `out`（24 字节 `String{cap,ptr,len}`，见 `4ec1dd` 的三条 `movq`） |
| `4ec174: mov (%rdx),%rcx` | 读 `*pos` |
| `4ec177: cmp 0x10(%rsi),%rcx` + `jae 4ec1dd` | 与 `buf` 的**长度**（`+0x10`）比较；越界 ⇒ 写空 `String` 并返回 |
| `4ec183: mov 0x8(%rsi),%rdx` | `buf` 的**数据指针**（`+0x8`） |
| `4ec18a: shl $0x4,%rsi` | token 步长 **16 字节**（与 `AGENTS.md` 硬事实表一致） |
| `4ec192/4ec195: inc %rcx; mov %rcx,(%r14)` | **消耗式**：`*pos += 1` |
| `4ec19c: cmp $0x11,%rcx; ja 4ece6d` | `tag > 17` ⇒ 走 panic 分支（§4.6） |
| `4ec1a6..4ec1b4: lea 0x1c3cfc,%rdx; movslq (%rdx,%rcx,4),%rcx; add %rdx,%rcx; jmp *%rcx` | 18 项 `int32` 相对跳转表，基址 `.rodata 0x1c3cfc` |

`buf` 的字段顺序是 `{cap@0, ptr@8, len@0x10}`（`gwemu.make_vec` 用的也是这个布局，
`0x4eb9d0`/`0x4c2530` 等函数同样按它取参）。

---

## 2. 跳转表 → 每个 tag 的输出（全部有字节证据）

跳转表解出来的目标地址（`.rodata 0x1c3cfc`，18 项）：

| tag | 记号 | case | 写出的字节 | 等级 |
|---|---|---|---|---|
| 0 | `x` | `0x4ec1b6` | `movb $0x78` ⇒ `"x"` | 观测 |
| 1 | `y` | `0x4ec98f` | `movb $0x79` ⇒ `"y"` | 观测 |
| 2 | `y'` | `0x4ec65a` | `movw $0x2779` ⇒ `"y'"` | 观测 |
| 3 | `+` | `0x4ec739` | `'('` + 左 + `'+'`(`4ec7be`) + 右 + `')'` | 观测 |
| 4 | `-` | `0x4ec3e5` | 同上，`'-'` | 观测 |
| 5 | `neg` | `0x4eca02` | **只写 `'-'`**（`4eca24`），**不加括号** | 观测 |
| 6 | `*` | `0x4ecb20` | `'('`…`'*'`(`4ecba5`)…`')'` | 观测 |
| 7 | `/` | `0x4ec864` | `'('`…`'/'`(`4ec8e9`)…`')'` | 观测 |
| 8 | `^` | `0x4ecd06` | `'('`…`'^'`(`4ecd8b`)…`')'` | 观测 |
| 9 | `sqrt` | `0x4ec5b3` | `"sqrt"`(`4ec5d9`) + `'('`(`4ec5d5`) + 子式 + `')'` | 观测 |
| 10 | `log` | `0x4ecc4b` | `"log("`（`movl $0x28676f6c` @`4ecc6d`） | 观测 |
| 11 | `ln` | `0x4ec33f` | `"ln("`（`movw $0x6e6c` + `'('`） | 观测 |
| 12 | `exp` | `0x4ec510` | `"exp("`（`movl $0x28707865` @`4ec532`） | 观测 |
| 13 | `abs` | `0x4eca7d` | `"abs("`（`movl $0x28736261` @`4eca9f`） | 观测 |
| 14 | `sin` | `0x4ec29c` | `"sin("`（`movl $0x286e6973` @`4ec2be`） | 观测 |
| 15 | `cos` | `0x4ec696` | `"cos("`（`movl $0x28736f63` @`4ec6b8`） | 观测 |
| 16 | `tan` | `0x4ec1f9` | `"tan("`（`movl $0x286e6174` @`4ec21b`） | 观测 |
| 17 | `const` | `0x4ec9c9` | `add $0x8,%rax`（取 `&token.value`）→ 组一个 `fmt::Argument`（值 + 格式化函数 `0x43adf0`）→ 走 `core::fmt` 写入（GOT `0x9ba718` → `0x91ded0`）；**两位小数** | 观测（结构）/ **逐位**（精度，§3.4） |
| 18/19/其他 | `(` `)` | `0x4ece6d` | `panic!("Invalid token ")`（§4.6） | 观测 |

四个函数 case 的收尾都是同一段尾代码（`4eccd3` / `4ece0f`：`movb $0x29` 追加 `')'`）。

---

## 3. 格式规则（这就是要写进实现的东西）

### 3.1 空格

**一个都不加。** 抓包里的 AI 产物可以逐字符对上（§7.4）：

```
sin(x)     0.00     (-1.06*x)     (-23.81*x)     tan(sin((-0.12*x)))     abs((-3.49+x))
```

### 3.2 括号：二元运算一律一层，一元负号零层

| 结构 | 文本 |
|---|---|
| `add(x, const 1)` | `(x+1.00)` |
| `mul(const 2, x)` | `(2.00*x)` |
| `pow(pow(x,2),3)`（`^` 左结合） | `((x^2.00)^3.00)` |
| `sin(x)` | `sin(x)` |
| `sqrt(add(x,const 1))` | **`sqrt((x+1.00))`**（函数括号 + 二元括号，两层都在） |
| `neg(x)` | `-x`（**没有** `(-x)`） |
| `neg(neg(x))` | `--x` |
| `neg(pow(x,2))` | `-(x^2.00)` |

推论：**输出文本不依赖优先级**——每个二元节点自带括号，所以再解析时不需要考虑
`^` 左结合、一元负号比 `^` 松这些怪癖（唯一例外就是 §5.3 的 `neg` + `^`）。

### 3.3 越界（子式缺失）⇒ 空串

`4ec177` 的比较失败就写空 `String`。所以缺操作数的流会写出畸形但"合法"的文本：

| token 流 | 文本 |
|---|---|
| `[Sin]` | `sin()` |
| `[Add]` | `(+)` |
| `[Add, x]` | `(x+)` |
| `[neg]` | `-` |
| `[]` | ``（空串） |

另外它**只消费一棵子树**：`[x, y]` 只写 `x`、`*pos` 停在 1（与二进制的 `pos` 语义一致）。

### 3.4 常量：`{:.2}`，没有 π / e 特判

| 值 | 文本 | 备注 |
|---|---|---|
| `0.0` | `0.00` | 抓包里那条 `"function":"0.00"` |
| `-0.0` | `-0.00` | 符号位保留 |
| `0.5` | `0.50` | **不是** `0.5` |
| `π` | `3.14` | **没有** π 特判 ⇒ `pi` 这个名字在文本里永远不出现 |
| `e` | `2.72` | 同上 |
| `NaN` / `±inf` | `NaN` / `inf` / `-inf` | |
| `0.125` / `0.135` | `0.12` / `0.14` | 按**确切二进制值**四舍五入（`0.125` 是精确平局 ⇒ 取偶） |
| `2.675` / `1.005` / `0.015` | `2.67` / `1.00` / `0.01` | 这些字面量的 f64 值略小于十进制值 |
| `1e300` | 300 位整数部分 + `.00` | **不使用科学计数法** |
| `1e-9` / `5e-324` | `0.00` | 小数侧只有两位 |

⇒ 实现里就是 `format!("{:.2}", v)`（`core::fmt` 的定精度浮点格式化）。这一条是**逐位验证**：
`fixtures/to_text.tsv` 里有 96 个单常量例 / 114 个单 token 例（含 `1e300`、`5e-324`、`NaN`、`-0.0`、
`9007199254740993`、`0.125`…），全部与二进制逐字符相同。

> 精度常量本身没有从 `.rodata` 里读出来：常量 case 走的是标准 `core::fmt` 路径
> （`0x91ded0`，pieces 指针 `0x19f3e0`），那个 `Arguments` 布局我没解到底。
> **"两位"这个结论来自 1032 例运行时对拍**，不是从字节推的——写在这里是因为
> "能对上"和"知道为什么"是两件事。

### 3.5 括号的真相：`FunctionInfo.corrected_function` 的 `+ c` 不是它

`"0.00 + 4.638"` 里的 `+ 4.638` 是**另一处**字符串拼接（`src/shot.rs::corrected_text`，
`docs/24` §6.2、`docs/05`），`to_text` 不负责它。`to_text(parse("0.00")) == "0.00"` 才是本函数的输出。

---

## 4. 与真二进制对拍（`0x4ec160` 本体）

### 4.1 怎么调的

`tools/gen_to_text_fixture.py` 用分析仓库的 `gwemu`（unicorn 映射 `libgraphwarrust.so`）直接调用：

```python
v   = g.make_vec(toks)        # {cap,ptr,len} 布局
out = g.alloc(24)             # String{0,0,0}
pos = g.alloc(8); g.wq(pos, 0)
g.call(0x4EC160, [out, v, pos])
```

### 4.2 语料与结果

| 语料 | 例数 | 结果 |
|---|---|---|
| `fixtures/parse.tsv` 的 37 条真解析器 token 流 | 37 | 全一致 |
| 手写结构例（每个 tag、`^` 左结合、负号、深层嵌套、缺操作数、多余 token） | 42 | 全一致 |
| 常量单例（含极值/次正规/NaN/±inf/-0.0） | 36 | 全一致 |
| 随机**形状合法**树（深度 1–4，常量两位小数） | 700 | 全一致 |
| 随机 tag 序列（可能截断） | 600 | 全一致 |
| 合计（去重前 1415，去重后） | **1032** | **1032/1032 逐字符一致** |

夹具：`fixtures/to_text.tsv`（`tag:hexbits;…` <TAB> 文本）。
扩大语料复跑：`GW2_TO_TEXT_TREES=… GW2_TO_TEXT_STREAMS=…` 重生成即可（§9）。

### 4.3 谁在调它（外部调用点，本轮新观测）

`.text` 里对 `0x4ec160` 的**直接** `call rel32` 共 22 处：19 处是它自己（递归），
另外 3 处在 `0x478727` / `0x4789b3` / `0x4789dc` —— **正好落在
`computer_player_manager`（`0x4785f0`，`docs/98` §4）旁边**：

```
4789a5: lea 0xb8(%rsp),%rdi    ; sret
4789ad: mov %r15,%rsi          ; token 缓冲
4789b0: mov %r14,%rdx          ; &pos
4789b3: call 4ec160
```

⇒ `docs/98` §4 表格里那句"AI 的函数就是这样变成 `FunctionInfo.function` 的"
从**推断**升级为**观测**（有调用点，不是猜的）。

### 4.4 真实抓包里的 AI 文本全部是不动点

`docs/AI.md` §2.1 的 6 条（harness 抓到的 `FunctionUpdateRequest.function`）：
`sin(x)`、`0.00`、`(-1.06*x)`、`(-23.81*x)`、`tan(sin((-0.12*x)))`、`abs((-3.49+x))`
——**每一条**都满足 `to_text(parse(s)) == s`（二进制侧也验证过，见生成脚本输出）。
其中 `(-1.06*x)` 顺带证明了 §3.2 那条"负号不加括号"：`neg(const 1.06)` 写作 `-1.06`，
外层乘法再包一层括号，正好是 `(-1.06*x)`。

### 4.5 函数名/负号之外的 text 形态

`sqrt((x*y'))`、`exp(sqrt((x/(y'-x))))` 这类"双括号"也逐字符一致
（夹具里就有），说明函数 case 不会为了好看去合并括号。

### 4.6 `tag > 17` 走 panic（我们的实现不崩）

```
4ec19c: cmp $0x11,%rcx
4ec1a0: ja  4ece6d
...
4ece6d: ... 组 fmt::Argument ...
4ece9b: call *0x9ba698      ; GOT -> 0x925680（panic 入口）
```

| 证据 | 值 |
|---|---|
| 消息字符串 | `.rodata 0x1a24a2` = `"Invalid token "`（14 字节，池里带长度前缀 `0x0e`） |
| `Location.file` | `.data.rel.ro 0x95fea8` → 字符串 `0x1b52cd` = **`src/math_function.rs`**（20 字节） |
| 行/列 | `0x95feb8` = **285 : 28** |

⇒ 原作者的 token→文本 在 **`src/math_function.rs:285`** 这个文件里，与 `Function` 枚举、
`parse`、`eval` 同属"数学函数"这一层（**这也是我们把它放进 `src/function/` 的依据**，见 §6.1）。

括号 token（18/19）**不可能**出现在 `parse` 的产物里（`flatten` 只写 `Const` 与运算符），
所以线上这条 panic 走不到；但我们的实现仍然不 panic（§6.3）。

---

## 5. 往返：`parse → to_text → parse` 到底等不等价

结论分三层，**都不是"完全等价"**，而且**每一条偏差都是真二进制行为**（不是我们的近似）。

### 5.1 结构（tag 序列）：37/37 一致

`fixtures/parse.tsv` 全部 37 条表达式：`parse(e) → to_text → parse` 的 **tag 序列 100% 相同**。
⇒ `to_text` 不丢语法树形状（除了 §5.3 那一个例外）。

### 5.2 常量：只有"两位小数能精确表示"的才逐位回来（32/37）

| 表达式 | 往返后 | 差异 |
|---|---|---|
| `2*x+1`、`2^3^2`、`sin(x)`、`-x^2`、`log(x)`、`1+2+3`… | 逐位相同 | — |
| `PI` / `pi` / `n` | `3.14` | π 被截断 |
| `e` | `2.72` | e 被截断 |
| `2.5e` | `(2.50*2.72)` | 同上 |

5 条被截断的恰好都是含 **π/e** 的。这条在**二进制侧**也验证过（`real_parse → real_to_text → real_parse`）：
32/37 逐位、37/37 结构，与我们的实现完全一样。

### 5.3 三类"文本表达不了"的偏差（全部实测）

1. **负常量换形态**：`const(-1.06)` → 文本 `-1.06` → 再解析成 `neg(const 1.06)`。
   数值不变，但 tag 变了。若负常量正好是 `^` 的**左底座**，数值都会变：
   `pow(neg(2.5), y)` 写成 `(-2.50^y)`，再解析成 `neg(pow(2.50,y))`——因为
   `^` 比一元负号紧（`docs/03` §3）。`neg` 当左底座同理（`(-x^-y')`）。
2. **缺操作数的空位**：文本里没有"空位"这种东西。`(x+)` 再解析会把缺失的右侧填成
   `const 0`（`+`/`-` 的补齐规则，`docs/03` §6），或把后面的 token 吞走。
3. **NaN/inf 常量**：`NaN` 写出来是 `NaN`，而 `n`（π 的别名）+ 跳过字母会解析成 `3.14`；
   `inf` 同理。

### 5.4 实测数字（文本级两代往返，`fixtures/to_text_roundtrip.tsv`）

| 指标 | 结果 |
|---|---|
| 第二代文本与二进制逐字符一致 | **1040/1040** |
| 第一代 → 第二代**完全相同**（不动点） | **963/1040（92.6%）** |
| 不闭合的 77 例 | **73 例是"缺操作数空位"** + **4 例是 `^` 左底座带 `-`** |

⇒ 准确的说法是：**`to_text` 非单射；但"文本 → token → 文本"对"操作数齐全 + 常量两位小数
可精确表示"的流是恒等**。`parse.tsv` 的 37 条表达式全部落在后一类（文本层面 37/37 不动点）。

### 5.5 语义（`eval` 逐位）也能守住

对 1032 条 token 流里**操作数齐全、常量两位小数可精确表示、且没有 `^` 负底座**的
**509** 条，断言 `eval(原 token)` 与 `eval(to_text 之后)` 在 4 个 `x` 上**逐位相同**——
全绿。⇒ 只要不踩 §5.3 那三类，文本是可以拿去做数值等价的替身的。

---

## 6. 实现

### 6.1 放在哪，为什么

**`src/function/to_text.rs`**，由 [`src/function/mod.rs`](../src/function/mod.rs) re-export：

```rust
pub fn to_text(tokens: &[Token]) -> String        // 与 parse 对称（parse 也不返回 Result）
pub fn try_to_text(tokens: &[Token]) -> Option<String>  // 非法流（括号 token）→ None
pub fn const_text(v: f64) -> String               // `{:.2}` 的唯一真源，测试/文档用
pub fn is_printable(tag: Function) -> bool
```

理由（三条）：

1. **它是 `parse` 的逆运算**。两个方向放同一个模块，`docs/03` 的文法表和这份格式表才不会漂。
2. **二进制里它就在"数学函数"这一层**：panic 的 `Location` 是 `src/math_function.rs:285`（§4.6），
   不是 `graphwar_event.rs`。
3. 不动 `src/turn.rs` / `src/aigen.rs` / `src/protocol.rs` / `src/game.rs` / `src/serverloop.rs`
   / `src/gdext*.rs` / `client/`（本次约束）。接线留给下一步。

`docs/02` 的模块映射表把 `0x4ed670`（解析）记在 `graphwar_function_input.rs` 名下；
本文件是该表的**一处补充**：`0x4ec160` 来自 `math_function.rs`。

### 6.2 实现要点（逐条对二进制）

* 递归：`emit(out, tokens, pos)`，返回 `Result<(), InvalidToken>`；
* 越界 → 什么都不写（与 `4ec1dd` 写空串等价）；
* 二元/函数/负号/常量按 §2 的表；
* 常量用 `write!(out, "{:.2}", v)`（§3.4）；
* `pos` 的消耗式语义保留（只消费一棵子树）。

### 6.3 不 panic

* `match` 显式覆盖 `Function` 的 **20** 个变体，没有 `unwrap` / 索引，**没有可 panic 的位置**；
  所以不需要 `catch_unwind`（而且 `catch_unwind` 也接不住栈溢出）。
* 括号 token ⇒ `Err(InvalidToken)` ⇒ `try_to_text` 给 `None`，`to_text` 给空串。
* 递归深度 = token 数（AI 上限 200；测试跑到 2000 层不爆栈）。
  真实二进制同样递归，**递归深度上限没有在我们这边额外收紧**（收紧就会与二进制分叉）。

---

## 7. 验证清单

### 7.1 测试（新增 14 条，总数 242 → **256**）

`src/function/to_text.rs` 单测 7 条：叶子/函数、二元必带括号、负号不带括号、
常量的 8 种边界、缺操作数、括号不 panic、多余 token 不消费。

`tests/to_text.rs` 集成 7 条：

| 测试 | 断言 |
|---|---|
| `matches_the_real_binary_on_every_fixture_line` | `fixtures/to_text.tsv` **1032** 例逐字符一致 |
| `text_roundtrip_matches_the_real_binary` | `fixtures/to_text_roundtrip.tsv` **1040** 例的第二代文本一致；不动点比例 ≥ 90% |
| `roundtrip_keeps_operator_structure_for_every_parse_fixture` | `parse.tsv` 37/37 结构不变 |
| `roundtrip_constants_only_lose_decimal_digits` | 常量只允许被 `{:.2}` 截断；被截断的必须含 π/e（实测 32 逐位 / 5 截断） |
| `semantics_survive_the_roundtrip` | 509 例 × 4 个 `x` 的 `eval` 逐位相同 |
| `captured_ai_functions_are_fixed_points` | 6 条真实 AI 文本是不动点 |
| `degenerate_streams_never_panic` | 空/缺操作数/括号/2000 层深都不 panic |

### 7.2 `cargo test --offline`

```
lib 209 · broker 4 · events 6 · evidence 6 · golden 4 · live_hits 12 · plane 6
server_match 2 · to_text 7   ⇒  256 passed, 0 failed
```

### 7.3 复现（需要分析仓库的 `.venv` 与 `libgraphwarrust.so`）

```bash
GW2_ROOT=<分析仓库根> ../.venv/bin/python tools/gen_to_text_fixture.py   # 重生成两份夹具
# 扩大语料做一次性复核（不改变默认夹具）：
GW2_TO_TEXT_TREES=700 GW2_TO_TEXT_STREAMS=600 ../.venv/bin/python tools/gen_to_text_fixture.py
cargo test --offline --test to_text
```

---

## 8. 这次碰了哪些文件（自检）

| 文件 | 动作 |
|---|---|
| `src/function/to_text.rs` | **新增**（实现 + 7 条单测） |
| `src/function/mod.rs` | 加 3 行：`mod to_text;` + `pub use` |
| `tests/to_text.rs` | **新增**（7 条集成测试） |
| `fixtures/to_text.tsv`、`fixtures/to_text_roundtrip.tsv` | **新增**（真二进制生成） |
| `tools/gen_to_text_fixture.py` | **新增**（夹具生成器，含抓包不动点自检） |
| `docs/119-token-to-text.md` | **新增**（本文） |

**没动**：`src/turn.rs`、`src/aigen.rs`、`src/protocol.rs`、`src/game.rs`、`src/serverloop.rs`、
`src/gdext*.rs`、`client/`、`docs/INDEX.md`、`docs/HANDOVER.md`、任何已有夹具。

---

## 9. 仍未确定（诚实清单）

| # | 项 | 缺什么 |
|---|---|---|
| A | 常量**两位小数**在 `.rodata` 里是哪个字段（`0x19f3e0` pieces + `0x91ded0` 的 `Arguments` 布局没解到底） | 需要读懂那版 rustc 的 `Arguments`/`Placeholder` 生成格式。**结论本身有 1032 例运行时对拍兜底**，但"为什么是 2"目前是观测而非字节证据 |
| B | AI 的 token 流**是否真会**产生 §5.3 那三类退化（`^` 负底座 / 缺操作数 / NaN 常量） | 我们没有"真游戏 AI 的 token 数组"样本，只有它转出来的文本。变异规则（`docs/AI.md` §7）只把常量乘 `(1+normal())`，**看起来会**产生负常量，但没观测到 |
| C | 客户端拿到 `FunctionInfo.function` 后**是否按文本重画**曲线 | 若按文本重画，§5.3 的形态变化会让"客户端画出来的曲线"与服务端模拟的不一致。这是**推断**，缺客户端侧证据（`docs/108` 的重建外壳还没走内核） |
| D | `y'` 在生产路径上永不出现（`docs/AI.md` §2：Normal 不产生、DiffEq 产生 0 次） | 夹具覆盖了，但线上见不到 |
| E | 外部 3 个调用点各自的**参数语义**（sret 落在哪个栈槽、`pos` 初值是否 0、是否复用缓冲） | 只读到 `lea/mov`，没读它们的上下游 |

---

## 10. 给下一步的接口说明

`game.rs` / `serverloop.rs` 接线时只需要：

```rust
let text = gw2_rebuild::function::to_text(&slot.tokens);        // AI 的函数文本
let corrected = gw2_rebuild::shot::corrected_text(&text, x0, y0, flip);  // 已有
```

`to_text` 对空 token 返回 `""`，与开局那条空 `FunctionInfo` 一致
（抓包里空函数的 `corrected_function` 就是 `" + 10.990"`，即 `"" + " + c"`）。
