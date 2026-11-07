# 03 · 函数语言

玩家在输入框里敲的不是普通数学表达式，而是一门有自己规则的小语言。
本章描述它的**全部**可见行为，均已与二进制逐位对拍。

实现文件：`math_function.rs`（解析 `0x4ed670`、求值 `0x4eb9d0`）。
⚠️ **2026-11 更正**：本节旧版把解析器写在 `graphwar_function_input.rs` 名下，那是错的。
`0x4ed670` 体内唯一的 `panic::Location` 锚是 `math_function.rs:75/285`（`docs/02` §16.3、
`docs/149` §1），所以解析属于 `math_function`；`graphwar_function_input` 是**输入框**
（`LineEdit` 子类），它对文本只做一件事——**删空格**（新增 §13）。

## 1. 记号的内存表示

一个 token 占 **16 字节**：

```
{ tag: u64, value: f64 }
```

`tag` 就是 `enum Function` 的判别值，`value` 只对 `CONST` 有意义。

| id | 记号 | id | 记号 | id | 记号 |
|---|---|---|---|---|---|
| 0 | `x` | 7 | `/` | 14 | `sin` |
| 1 | `y` | 8 | `^` | 15 | `cos` |
| 2 | `y'` | 9 | `sqrt` | 16 | `tan` |
| 3 | `+` | 10 | `log`（**log10**） | 17 | `const` |
| 4 | `-` | 11 | `ln` | 18 | `(` |
| 5 | `neg`（一元负） | 12 | `exp` | 19 | `)` |
| 6 | `*` | 13 | `abs` | | |

变体名在 `.rodata` 里连续排列，是判定编号的直接证据。

## 2. 预处理

1. 整串**转小写**（`SIN(x)` 合法）。
2. `"pi"` → `"n"`，`"π"` → `"n"`。也就是说 `n` 是 π 的别名。
3. 用一条固定正则切词：

```
[0-9]*\.?[0-9]+ | \( | \) | x | y' | y | \+ | - | \* | / | \^
                | sqrt | log | abs | sin | cos | tan | ln | n | exp | e
```

注意正则里 `y'` 排在 `y` 之前，`sqrt|log|...|ln` 也按长优先排列——这是隐式乘法和变量识别的关键。

## 3. 优先级（从紧到松）

```
函数应用  >  ^  >  一元负号  >  * /  >  + -
```

三处与"数学常识"不同，必须记牢：

| 现象 | 真实行为 |
|---|---|
| `^` 结合性 | **左结合**：`2^3^2 = (2^3)^2 = 64`，不是 512 |
| 一元负号与 `^` | `-x^2` = `(-x)^2`，因为负号比 `^` 松 |
| `log` | 是 **log10**；自然对数是 `ln` |

### 3.1 `^` 的**输入可达性**：Steam 运行时下这个键打不出来

原版的启动脚本（Steam depot 3066841 自带 `start_graphwar.sh`，逐字节见下）明确写着：

```bash
# Steam (Steam Linux Runtime / pressure-vessel) forces LC_ALL=C, which breaks
# X11 dead-key composition (e.g. typing ^ via the circumflex dead key) even
# though the game runs fine with a normal locale outside of Steam.
unset LC_ALL
export LC_CTYPE=C.UTF-8
exec "$DIR/Graphwar.x86_64" "$@"
```

⇒ **在 Steam 里直接启动（不经过这个 wrapper）时，`^` 因为 `LC_ALL=C` 破坏了 X11 dead-key
组合而打不出来**；作者用这个 wrapper 才修好。

对分析的意义（不是引擎行为，是**玩家可达性**）：

- `x^2` 这种写法在**未经 wrapper 的 Steam 启动**下输入困难 ⇒ 实战录像里幂函数少见，
  不代表引擎不支持，而是**输入路径**的问题（等价写法：`x*x`、`pow`... —— 注意 `pow` **不是**函数，
  见 §2，只能靠 `^`）；
- 复现"玩家怎么输入"这类行为时，**必须先确认是哪种启动方式**，否则会把环境问题误判成游戏行为。

证据档：**观测**（来自发行版自带的 shell 脚本原文；脚本 340 字节、sha256 `a2c27ff9…`）。

## 4. 隐式乘法

任意相邻的"可结束"与"可开始"单元之间自动插入 `*`：

- `2x` → `2*x`
- `2sin(x)` → `2*sin(x)`
- `(x+1)(x-1)` → `(x+1)*(x-1)`
- `xy` → `x*y`

## 5. 函数可以不带括号

`sinx`、`sqrt x` 都合法。规则是：函数名只作用于**紧跟其后的一个 primary**。

由此产生一个著名怪癖：

```
sin^2(x)  →  sin(空) ^ 2 * x
```

因为 `sin` 后面紧跟的是 `^`，不是 primary，于是它的操作数被当成缺失
（缺失操作数按 `CONST 0` 填充，见下）。

## 6. 缺失操作数填 0

二元运算符缺操作数时，缺的那一侧填 `CONST 0` 而不是报错。这是"用户随手乱敲也能出图"的原因。

## 7. 常量

| 记号 | 值 |
|---|---|
| `e` | 2.71828182845904523536 |
| `n` / `pi` / `π` | 3.14159265358979323846 |

## 8. 输出形态

解析结果是**前缀（Polish）顺序**的 token 列表，不是语法树：

```
sin(x) + 2   →   [ADD, SIN, VAR_X, CONST(2.0)]
```

求值时对前缀序列做递归下降：遇运算符取后续若干操作数为子式。

## 9. 常量折叠

解析完成后有一趟折叠（`0x4eb830`）：纯常量子树被压成一个 `CONST`。
例如 `2*3+1` 直接变成 `CONST(7.0)`，求值阶段不再计算。

## 10. 括号在 token 列表里**不保留**（2026-11 已定，`archive/155-round-log.md` §30.2/§30.7）

* 求值器 `0x4eb9d0` 的跳转表只有 **18 项**（tag `0..=17`），进表前有 `cmp rax,0x11; ja → panic`：
  所以 `18/19`（`(`/`)`）**根本到不了求值**。
* 真解析器 `0x4ed670` 直调（`tools/eval_oracle.py`）在 `sin(x)`、`(x+1)(x-1)`、`2+(x))`
  上都返回**不含括号**的 tag 序列 ⇒ 括号在解析期就被消化掉了。
* **无主的 `)` 被跳过、解析继续**（`x)+1 → [ADD, VAR_X, CONST]`，跳过之后照样隐式乘法
  `x)2 → [MUL, VAR_X, CONST]`）。我方 `parse` 2026-11 已按这条修好（`archive/155-round-log.md` §30.7）：
  1290 发里"多右括号"的 8 发从"截断"变成"一致"（剩 1 发两边求值都是 NaN）。

## 11. `log` / `ln` 的定义域边界

> ⚠️ **2026-11 更正**（`archive/155-round-log.md` §30.3，unicorn + `ctypes` 直调 `libm.so.6`）：
> 本节旧版写"`log(0) = NaN`，判据是 `v <= 0`"——**那条是反的**，是 oracle 的假象。
> 二进制的 `log`/`ln` 就是 **libm 的 `log10`/`log`**（GOT 槽 `0x9bc270` / `0x9bbec0`），
> 所以 `±0` 给 **-inf**、负数给 **NaN**。旧值来自 `analysis/emu.py` 把导入映射到
> python 的 `math`（定义域外**抛异常**→ 兜成 `NaN`），不是二进制的行为。

直接对二进制喂值实测（`@0x4eb9d0`，复跑：`tools/eval_oracle.py --sweep`）：

| 输入 | `log`（libm `log10`） | `ln`（libm `log`） |
|---|---|---|
| `0.0` | **-inf**（`fff0000000000000`） | **-inf** |
| `-0.0` | **-inf** | **-inf** |
| `1e-320`（次正规） | `-320.000004834948` | `-736.8272408909739` |
| `-1.0` / `-2.0` | NaN（`7ff8…`） | NaN（**`fff8…`**，负 NaN） |
| `1.0` | `0.0` | `0.0` |
| `10.0` | `1.0` | `2.302585092994046` |

结论：**没有显式定义域检查**——就是 libm 本身：`v < 0` → NaN、`v == ±0` → `-inf`。
次正规数仍按正常对数计算（`1e-320` 那条），所以这不是"下溢保护"。

这一条有实际后果：`NaN` 会让点序列**立刻终止**，而 `-inf` 会让二分继续（弦长被算成 `inf`，
于是不断收缩上界）。所以 `x = 0` 处 `log` 的行为决定了子弹会不会停在原地。
`src/math_function.rs::m_log10` / `m_ln` 现在直接调 `log10()` / `ln()`（= 同一个 libm）。

---

## 12. token → 文本：`0x4ec160`

> 合并自 `119-token-to-text`（全文与逐条证据轨迹：`archive/119-token-to-text.md`）。
> 这一层是 **AI 上不了线**的最后一块：`aigen` 进化出的是 token，而协议
> `FunctionInfo.function` / `corrected_function` 是**文本**，中间那一步就是它。

**签名**（序言逐条可读）：`fn to_text(out: &mut String, buf: &TokenBuf, pos: &mut usize)`，
`buf` 字段顺序 `{cap@0, ptr@8, len@0x10}`，token 步长 **16 字节**，**消耗式**（`*pos += 1`）。
跳转表在 `.rodata 0x1c3cfc`（18 项 `int32` 相对表）；`tag > 17` 走 panic 分支。
〔观测：段换算、`int3` 填充、序言、函数起点表成员〕

### 12.1 跳转表 → 每个 tag 的输出（全部有字节证据）

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

### 12.2 格式规则（这就是要写进实现的东西）

#### 3.1 空格

**一个都不加。** 抓包里的 AI 产物可以逐字符对上（§7.4）：

```
sin(x)     0.00     (-1.06*x)     (-23.81*x)     tan(sin((-0.12*x)))     abs((-3.49+x))
```

#### 3.2 括号：二元运算一律一层，一元负号零层

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

#### 3.3 越界（子式缺失）⇒ 空串

`4ec177` 的比较失败就写空 `String`。所以缺操作数的流会写出畸形但"合法"的文本：

| token 流 | 文本 |
|---|---|
| `[Sin]` | `sin()` |
| `[Add]` | `(+)` |
| `[Add, x]` | `(x+)` |
| `[neg]` | `-` |
| `[]` | ``（空串） |

另外它**只消费一棵子树**：`[x, y]` 只写 `x`、`*pos` 停在 1（与二进制的 `pos` 语义一致）。

#### 3.4 常量：`{:.2}`，没有 π / e 特判

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

#### 3.5 括号的真相：`FunctionInfo.corrected_function` 的 `+ c` 不是它

`"0.00 + 4.638"` 里的 `+ 4.638` 是**另一处**字符串拼接（`src/graphwar_shot_calculator::corrected_text`，
`09-network.md` §6.2、`05-shot-pipeline.md`），`to_text` 不负责它。`to_text(parse("0.00")) == "0.00"` 才是本函数的输出。

---

### 12.3 往返：`parse → to_text → parse` 到底等不等价

结论分三层，**都不是"完全等价"**，而且**每一条偏差都是真二进制行为**（不是我们的近似）。

#### 5.1 结构（tag 序列）：37/37 一致

`fixtures/parse.tsv` 全部 37 条表达式：`parse(e) → to_text → parse` 的 **tag 序列 100% 相同**。
⇒ `to_text` 不丢语法树形状（除了 §5.3 那一个例外）。

#### 5.2 常量：只有"两位小数能精确表示"的才逐位回来（32/37）

| 表达式 | 往返后 | 差异 |
|---|---|---|
| `2*x+1`、`2^3^2`、`sin(x)`、`-x^2`、`log(x)`、`1+2+3`… | 逐位相同 | — |
| `PI` / `pi` / `n` | `3.14` | π 被截断 |
| `e` | `2.72` | e 被截断 |
| `2.5e` | `(2.50*2.72)` | 同上 |

5 条被截断的恰好都是含 **π/e** 的。这条在**二进制侧**也验证过（`real_parse → real_to_text → real_parse`）：
32/37 逐位、37/37 结构，与我们的实现完全一样。

#### 5.3 三类"文本表达不了"的偏差（全部实测）

1. **负常量换形态**：`const(-1.06)` → 文本 `-1.06` → 再解析成 `neg(const 1.06)`。
   数值不变，但 tag 变了。若负常量正好是 `^` 的**左底座**，数值都会变：
   `pow(neg(2.5), y)` 写成 `(-2.50^y)`，再解析成 `neg(pow(2.50,y))`——因为
   `^` 比一元负号紧（`03-function-language.md` §3）。`neg` 当左底座同理（`(-x^-y')`）。
2. **缺操作数的空位**：文本里没有"空位"这种东西。`(x+)` 再解析会把缺失的右侧填成
   `const 0`（`+`/`-` 的补齐规则，`03-function-language.md` §6），或把后面的 token 吞走。
3. **NaN/inf 常量**：`NaN` 写出来是 `NaN`，而 `n`（π 的别名）+ 跳过字母会解析成 `3.14`；
   `inf` 同理。

#### 5.4 实测数字（文本级两代往返，`fixtures/to_text_roundtrip.tsv`）

| 指标 | 结果 |
|---|---|
| 第二代文本与二进制逐字符一致 | **1040/1040** |
| 第一代 → 第二代**完全相同**（不动点） | **963/1040（92.6%）** |
| 不闭合的 77 例 | **73 例是"缺操作数空位"** + **4 例是 `^` 左底座带 `-`** |

⇒ 准确的说法是：**`to_text` 非单射；但"文本 → token → 文本"对"操作数齐全 + 常量两位小数
可精确表示"的流是恒等**。`parse.tsv` 的 37 条表达式全部落在后一类（文本层面 37/37 不动点）。

#### 5.5 语义（`eval` 逐位）也能守住

对 1032 条 token 流里**操作数齐全、常量两位小数可精确表示、且没有 `^` 负底座**的
**509** 条，断言 `eval(原 token)` 与 `eval(to_text 之后)` 在 4 个 `x` 上**逐位相同**——
全绿。⇒ 只要不踩 §5.3 那三类，文本是可以拿去做数值等价的替身的。

---

## 13. 输入框：玩家敲进去的文本到底被改了什么（本轮新增）

落码：`src/graphwar_function_input.rs`；验收：`tests/graphwar_function_input.rs`。

### 13.1 结论先行

玩家按下的字符从键盘到解析器要过三道手，**只有第一道会改文本，而且只删空格**：

| 关卡 | 模块 | 对文本做了什么 | 档位 | 证据 |
|---|---|---|---|---|
| ① 按键 | `graphwar_function_input`（输入框） | **把 28 个"死键合成/上标"字符折回 ASCII**；`'^'` 单独判；ASCII 一律不管（交基类） | 观测 | `gui_input` `0x4fedc0` 的跳转表 `0x1c3d5c`（198 项）＋每个分支的 `mov $0x??,%eax` |
| ② 刷新 | 同上 `force_update_cb` | **删掉所有空格**（`text.replace(' ', "")`） | 观测 | `0x4f28be`–`0x4f28dc`：`str::replace`（`0x502d20`）＋字面量 `0x1c44a1` = `' '` ＋ `to` = 空 `&str` |
| ③ 解析 | `math_function`（`0x4ed670`，**不是**本模块） | **ASCII 小写化** → `replace("pi","n")` → `replace("π","n")` → 按 77 字节模式分词 | 观测 | `0x4ed7a3`–`0x4ed7ba`（小写化循环）、`0x4ed7ce` / `0x4ed831`（两次 `str::replace`）、`0x4ed884`（77 字节模式 `0x1c4347`） |

⇒ **"输入框会不会预处理"的答案是：会，但只有"删空格"这一条**（外加键盘层的 28 条字符折叠）。
去空格发生在**刷新**（`force_update_cb`）里，**不在提交路径**上：
`on_text_submitted` 把文本原样交给回调的 `"fire"`（`0x4f4250`，字面量 `0x1b97c8`），
整条路径上没有字符串变换。

**π / 隐式乘 / `log`=log10 / `^` 左结合都不在输入框里**——见 §13.6。

### 13.2 本模块的 17 个 x86 函数（2.0.60）

`docs/02-module-map.md` §18 A 表按 **2024-07-30 版**给 15 个；2.0.60 用
`panic::Location` 定出 **13 个**，另有 **4 个**无锚（没有 panic 路径），合计 17 条。
地址是 **ELF 虚地址**（Ghidra = +`0x100000`）。

| vaddr | 跨度 | 原版名 | 角色 | 档位 |
|---|---|---|---|---|
| `0x4e6dc0` | `0x180` | — | gdext 惰性注册驱动（once 锁 + 遍历静态表）；Location `434:1` | 观测 |
| `0x4e6f40` | `0x180` | — | 同上 | 观测 |
| `0x4e9ee0` | `0x1a0` | — | 同上 | 观测 |
| `0x4ea080` | `0x250` | — | 同上；唯一引用 Location `466:1` | 观测 |
| `0x4ea2d0` | `0x1b0` | — | 同上（`434:1` 四兄弟之一） | 观测 |
| `0x4ea480` | `0x210` | — | once 锁下把 0x78 字节注册记录压进全局 `Vec`；Location `27:10` | 观测 |
| `0x4f2200` | `0xfa0` | `force_update_cb` | 读文本 → **删空格** → 送路径计算 → 与 game_client 对账 | 观测（5 锚） |
| `0x4f3210` | `0x700` | `move_caret_left` | 光标左移一个 token | 观测（1 锚） |
| `0x4f3910` | `0x940` | `move_caret_right` | 光标右移一个 token | 观测（6 锚） |
| `0x4f4250` | `0xd0` | — | 把提交的文本交给 `dyn` 回调的 `"fire"` | 推断（强） |
| `0x4f4320` | `0xf0` | — | 插入文本 + `call("force_update_cb")` | 推断（强） |
| `0x4f4410` | `0xb0` | — | 循环 N 次 `call("delete_char_at_caret")` | 观测 |
| `0x4f44c0` | `0x5a0` | `delete_token_at_caret` | 删光标前一个 token（N 次退格） | 观测（1 锚） |
| `0x4f4a60` | `0x970` | `delete_token_after_caret` | 删光标后一个 token（N 次退格） | 观测（6 锚） |
| `0x4fd1b0` | `0x2f0` | `ready` | 接三条自己的信号（§13.5） | 观测（1 锚） |
| `0x4fd4a0` | `0x1920` | `process` | 两次 `call("force_update_cb")` + 与 game_client 交互 | 观测（10 锚） |
| `0x4fedc0` | `0x357` | — | 虚拟 `gui_input`：unicode → 28 条折叠 + `'^'` 特判 | 推断（见下） |

`gui_input` 的定位链（三步都可指）：`.rodata` `0x1c46c9` = `"gui_input"`(9) →
唯一引用者 = 注册闭包 `0x4f8dc0` → 它的 trampoline `0x425a20` →
`0x425c04` `call *0x9bb7c8` → 相对重定位槽 addend = `0x4fedc0`。
⚠️ 跨度 `0x357` 是 Ghidra 的函数体跨度：线性"下一个起点表"会在 `0x4fefd5` 处
截断它（那是别的函数的冷块），别拿表当结束地址。

**`name_to_addr.tsv` 复核结论**：7 个"唯一锚"全部通过（锚行号集合与地址一一对应，
锚数与表里记录一致：`force_update_cb` 5 / `move_caret_left` 1 / `move_caret_right` 6 /
`delete_token_at_caret` 1 / `delete_token_after_caret` 6 / `ready` 1 / `process` 10）。
**没有冲突。**

⚠️ 一处**工具**冲突（不是表的冲突）：仓库的 `tools/class_probe.py` 把这些 Location
归到了隔壁商店按钮函数（例如 `97:63` 被归给 `0x4f57d0`）。真正引用该 Location 的
指令是 `0x4f4974`，落在 `0x4f44c0` 体内（REA `resolve_containing_procedure`：
Ghidra `0x5f4974` ∈ `FUN_005f44c0`）。**以反汇编为准**，`class_probe.py` 的
`module` 子命令的归属列不要直接引用。

### 13.3 键盘层：28 条折叠（`gui_input`）

`gui_input` 只处理 `InputEventKey`：取 `unicode`（一个 `char`），先挡掉代理区
（`0x4fef2b`–`0x4fef3d` 是 `char::from_u32` 的判据），再按**字符码**分发。
跳转表在 `0x1c3d5c`（`0x4fef60` 处 `jmp *`），**198 项**覆盖 `0xB2..=0x177`，
其中只有 **28 项**不落到默认分支（`0x4ff27b`，直接返回）：

| 字符 | 折成 | 分支 |
|---|---|---|
| `²` U+00B2 | `2` | `0x4fef70` `mov $0x32` |
| `³` U+00B3 | `3` | `0x4ff1a4` `mov $0x33` |
| `¹` U+00B9 | `1` | `0x4ff1b9` `mov $0x31` |
| `·` U+00B7 | `.` | `0x4ff096` `mov $0x2e` |
| `Â`/`Ê`/`Î`/`Ô`/`Û` U+00C2/CA/CE/D4/DB | `A`/`E`/`I`/`O`/`U` | `0x4ff188`/`0x4ff181`/`0x4ff18f`/`0x4ff15e`/`0x4ff17a` |
| `â`/`ê`/`î`/`ô`/`û` U+00E2/EA/EE/F4/FB | `a`/`e`/`i`/`o`/`u` | `0x4ff173`/`0x4ff140`/`0x4ff14a`/`0x4ff1b2`/`0x4ff10e` |
| `Ĉ`/`ĉ` U+0108/0109 | `C`/`c` | `0x4ff19d`/`0x4ff122` |
| `Ĝ`/`ĝ` U+011C/011D | `G`/`g` | `0x4ff154`/`0x4ff1ab` |
| `Ĥ`/`ĥ` U+0124/0125 | `H`/`h` | `0x4ff118`/`0x4ff196` |
| `Ĵ`/`ĵ` U+0134/0135 | `J`/`j` | `0x4ff1ce`/`0x4ff12c` |
| `Ŝ`/`ŝ` U+015C/015D | `S`/`s` | `0x4ff136`/`0x4ff16c` |
| `Ŵ`/`ŵ` U+0174/0175 | `W`/`w` | `0x4ff1c7`/`0x4ff1d5` |
| `Ŷ`/`ŷ` U+0176/0177 | `Y`/`y` | `0x4ff1c0`/`0x4ff165` |

**这 28 个字符是什么**：`^`（抑扬符）在 **AZERTY / QWERTZ** 上是**死键**，按完再按元音，
引擎给的是合成字符 `â ê î ô û`；`Ĉ Ĝ Ĥ Ĵ Ŝ Ŵ Ŷ` 是同一族在拉丁扩展 A 里的对应；
`² ³ ¹` 是上标。函数语法里只有 ASCII 字母有意义，所以输入框把它们折回 ASCII 再插入
（插入走 `0x4ff1e1` 汇合点：`format!("{}", ch)` → `0x4ff227 call 0x4f4320`）。

**ASCII 一个都不在表里**（198 项的索引区间是 `0xB2..=0x177`）⇒ 数字、括号、运算符、
`x` / `y` / 字母**全部**落默认分支直接返回，交给基类 `LineEdit` 处理。
这条是"输入框没有字符白名单"的直接证据。

`'^'`（`0x5e`）是唯一在表**之前**单独判的字符（`0x4feeeb`–`0x4fef06`）：
三次引擎属性读取之后才 `je 0x4fef7a` 插入 `"^"`（字面量 `0x1c4341`，长度 1）。
**那三次读取的语义未知**（见 §13.7）。

### 13.4 编辑层：`token` → N 次退格

四个编辑方法（`move_caret_left` / `move_caret_right` / `delete_token_at_caret` /
`delete_token_after_caret`）都不自己改字符串：`0x4f4410` 是个循环，每轮
`call 0x39fe10`，方法名 = `"delete_char_at_caret"`（`.rodata` `0x1c44a2`，长度 `0x14` = 20）：

```text
0x4f4440  call 4ddb40                 ; 取对象
0x4f4453  lea  "upcast_ref"(10)       ; 基类引用
0x4f447a  call 39fe10                 ; NameCall("delete_char_at_caret")
0x4f448c  dec  %ebx / jne 0x4f4440    ; N 次
```

`delete_token_at_caret`（`0x4f44c0`）调它 4 次，`delete_token_after_caret`（`0x4f4a60`）
调 5 次。⇒ **"删一个 token" 的落地方式**（观测）；**N 怎么算出来的**未知
（内联的 `0x3e90f0` 调用序列 = `str` 字符迭代，逐指令没还原）。

### 13.5 `ready` 接的三条信号（自己接自己）

`0x4fd1b0` 里三段结构相同（`Callable::from_object_method(self, handler)` +
`StringName::from(signal)` + `connect`）：

| 信号 | 字面量 | 长度 | 处理函数 | 字面量 | 长度 |
|---|---|---|---|---|---|
| `size_changed` | `0x1c46d2` | 12 | `on_size_changed` | `0x1c444f` | 15 |
| `text_changed` | `0x1c430d` | 12 | `on_text_change` | `0x1c42ff` | 14 |
| `text_submitted` | `0x1c46de` | 14 | `on_text_submitted` | `0x1c445e` | 17 |

三条处理函数都是 `#[func]` 导出，与 `docs/18-build.md` §6.5 的扩展 API 表逐条一致
（`on_text_change` / `on_text_submitted` / `on_size_changed` / `force_update_cb`）。
与 `docs/16-client.md` §1 的运行时信号快照（那 27 条"人为连接"里的两条
`GraphwarFunctionInput.text_changed→on_text_change` / `text_submitted→on_text_submitted`）
**互相印证**。

`process`（`0x4fd4a0`）还两次 `call("force_update_cb")`（`0x4fdeea` / `0x4fe1d2`，
长度都是 `0xf`；同一地址、长度 `0xc` 时是 `"force_update"`——LLVM 合并了两个字面量）。

### 13.6 解析器入口的归一化（补 §1 的口径）

`MathFunction::new`（`0x4ed670`）的入口**逐条**是：

```text
0x4ed7a3  movzbl (%r15,%rax,1),%ecx   ; c = s[i]
0x4ed7a8  lea    -0x41(%rcx),%esi     ; c - 'A'
0x4ed7ab  cmp    $0x1a,%sil
0x4ed7af  setb   %sil                 ; 大写 ASCII？
0x4ed7b3  shl    $0x5,%sil            ;  +0x20
0x4ed7b7  or     %cl,%sil
0x4ed7ba  mov    %sil,(%r15,%rax,1)   ; 就地写回 ⇒ ASCII 小写化
0x4ed7ce  replace(from="pi"(2), to="n"(1))
0x4ed831  replace(from="π"(2 字节), to="n"(1))
0x4ed884  lea 0x1c4347 -> rsi, mov $0x4d,%edx   ; 77 字节 token 模式 → 引擎
```

77 字节的模式（`0x1c4347`，逐字节观测）：

```text
[0-9]*\.?[0-9]+|\(|\)|x|y'|y|\+|-|\*|/|\^|sqrt|log|abs|sin|cos|tan|ln|n|exp|e
```

注意它**没有 π**：π 在上一步已经被换成 `n`，所以 `n` 就是 π（这解释了 `docs/02` 与
本仓库 `AGENTS.md` 的硬事实「`pi`/`π`/`n` = π」——它是**两步文本替换**的结果，
不是分词器的别名表）。也**没有空格**：空格已经在输入框那一步删掉了（§13.1 ②）。

⚠️ **推断**：这 77 字节交给的是**引擎**的 `RegEx`（Godot 4 `RegEx.new()`），不是
Rust 的 `regex` crate —— `0x4ed898` 是 `call *0x9bc280(%rip)`，该 GOT 槽是
`R_X86_64_RELATIVE` → `0x7547f0`（GDExtension 接口包装器）。缺的证据：`0x7547f0`
对应的 `GDExtensionInterface` 成员名。

### 13.7 未知

完整清单见 `src/graphwar_function_input.rs` 的 `UNKNOWNS`（6 条）。要点：

1. **token 边界判据**：`move_caret_*` / `delete_token_*` 里内联的 `0x3e90f0`
   调用序列各代表什么（字符类判定？只看左右各一个字符？）——只固定"砸 N 次退格"。
2. **`'^'` 的附加条件**：`0x4fee94` / `0x4feea5` / `0x4feee3` 三次读引擎属性
   （字符串 `"upcast_ref"` 经 `0x407350`）后与 `'^'` 比较。条件语义未知
   （shift？键盘布局？caret 是否为空？）。
3. **`0x4f4250` 的 dyn 回调**：`[rdi+0x68]` / `[rax+0x20]` 是个弱引用 trait 对象；
   `weak_caller` 模块有同类 idiom，没有证据钉死字段类型。
4. **`move_caret_end` / `move_caret_home` / `insert_text_at_caret` /
   `delete_chars_at_caret` / `clear` / `update_sizes` 在 2.0.60 没有独立函数体**
   （`name_to_addr.tsv` 也把它们标"未知/歧义"）：或已内联，或只以 `Callable` 形式存在。

另外 `process`（6.4 KB / 60 个基本块）只做到"结构级"：入口构造位置串
`"src/graphwar_function_input.rs:519"`（34 字节，`0x4fd4d6`–`0x4fd4ed` 用两条 xmm
+ 一个 `movw $0x3931` 拼出来），体内引用 `src/graphwar_game_client.rs:152:22/:152:49`
的借用位置各 7 处。逐块语义未画完。

### 13.8 落码清单

| 原版 | 落在 | 覆盖程度 | 档位 |
|---|---|---|---|
| 17 条函数地址 + 跨度 + 锚 | `FUNCTIONS` / `function_at` / `named_functions` / `ghidra` | 17 条 | 观测 |
| 7 个唯一锚 | 模块头表格 + `tests/graphwar_function_input.rs` 的第 1 条断言 | 7/7 复核 | 观测 |
| 4 个导出方法 | `EXPORTS` / `MethodRegistration` | 名字+长度+4 闭包+4 trampoline | 观测 |
| 3 个虚拟方法 | `VIRTUALS` / `VirtualRegistration` | 名字+长度+info 闭包+trampoline+实现+槽 | 观测 |
| 3 条信号接线 | `SIGNAL_WIRING` / `handler_for_signal` / `signal_for_handler` | 3/3 | 观测 |
| **删空格** | `strip_spaces` / `STRIPPED_CHAR` | 唯一文本变换 | 观测 |
| **28 条折叠** | `COMPOSED_FOLDS` / `fold_composed` / `classify_key` / `intent_for_key` | 28/28 | 观测 |
| 跳转表区间 | `FOLD_TABLE_RANGE` = `0xB2..=0x177` | 198 项 | 观测 |
| `'^'` 特判 | `CARET_CHAR` + `classify_key` 的 `caret_condition` 参数 | 分支观测 / 条件未知 | 观测＋未知 |
| N 次退格 | `delete_chars_at_caret_plan` / `BACKSPACE_METHOD` / `Buffer::run_plan` | 落地方式 | 观测（N 的算法未知） |
| 提交原样 | `submit_intent` / `REFRESH_METHOD` / `REFRESH_PREFIX` | — | 观测 |

