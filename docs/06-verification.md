# 06 · 如何证明复原是对的（证据分级 / oracle / 活体 harness）

逆向最怕"看起来像"。本仓库的每条结论都标注证据等级，并说明它凭什么成立。

**本文件的结构**（2026-11 归并后，原 `11`/`20`/`21`/`22`/`23`/`26` 六篇并入）：

| 节 | 内容 |
|---|---|
| §1–§11 | 证据分级、静态 oracle（unicorn / 差分测试）、工具链现状与已知边界 |
| §12 | **调用图的盲区**：函数指针派发怎么补（原 `11`） |
| §13 | **活体 oracle**：frida 注入真进程（原 `20`） |
| §14 | **原版游戏 harness**：五条被否证的路 + 最后怎么跑起来（原 `21`→`22`） |
| §15 | **用真实对局验证与取证**：采样器 69/69、`shot_calc`/`life_entry` 形态、harness 当玩家（原 `23`/`26`） |
| §16 | 手段选择顺序 |

## 1. 证据分级

| 等级 | 含义 | 例子 |
|---|---|---|
| **逐位验证** | 与真二进制输出完全相等（浮点、点数、顺序、NaN 位置） | 解析器、求值器、采点 |
| **观测** | 直接读到了字节/指令/字符串/调用点，但未做端到端验证 | 常量地址、调用链、事件字段 |
| **推断** | 由名字、规模、调用关系推测，未证实 | 大多数模块职责 |
| **未知** | 明确没有证据 | 命中判定、障碍裁剪 |

文档里凡是推断，都写了"推断"；不确定的地方宁可留空也不编。

## 2. 三条腿的 oracle

stripped 二进制没有符号，所以用三样东西互相校验：

1. **函数边界** —— 以 **Ghidra 函数表**为准（13503 条，其中 13263 条落在 `.text`）。
   ~~`.eh_frame` 恢复出 13430 条"真实函数"~~ 这条**当年写错了**：那份解析是坏的，
   与 Ghidra 只交集 2 条，大量地址不是指令边界（见 `02` §12）。现行口径见 §11。
2. **capstone 建 rip-relative 交叉引用 + panic 字符串锚定** ——
   把代码定位到 `src/xxx.rs:line`。
3. **unicorn 指令级模拟器当预言机** —— 把 `.so` 映射进 unicorn，
   直接以普通函数调用 `parse` / `eval` / `calculate_path`，读回真实输出。
   （需 hook `__rust_alloc/realloc`、`memcpy`、`bcmp`、`getrandom`、
   `posix_memalign`、`__tls_get_addr` 等少数外部符号。）

第 3 条是关键：它让"真二进制"变成一个可被批量调用的函数，从而能做差分测试。

## 3. 差分测试结果

| 测试 | 用例 | 结果 |
|---|---|---|
| `analysis/difftest.py` 定点网格（11 函数 × 4 起点 × 4 步长 × 2 方向 × 2 模式） | 704 | 全部逐位相同 |
| `analysis/fuzz.py` 随机语法树（深度 1–3） | 1200（多 seed） | 全部逐位相同 |
| `analysis/difftest_parser.py` | 73 正常 + 17 病态 | 全部相同 |
| `analysis/final_verify.py` 端到端（含修正串） | 648 序列 + 40 串 | 全部相同 |

合计 2552 条点序列、逐点比对，无差异。

## 4. 独立复核（本仓库建立时重跑）

以上四套在本仓库建立时**重新执行**过，而非引用旧结论：

```
ok 704 bad 0
done, mismatches: 0 of 300
73/73 match ; 17/17 match
checked 648 point sequences; mismatches: 0 ; correction terms OK
```

## 5. 对抗性抽查（网格之外）

除固定网格外，另做了一轮非常规参数抽查（`|x0|` 贴边 24.99/25/30、
`step` 取 50 / 1e9 / 3e9 / 负数、`ln`/`tan`/`1/x` 等奇点函数）：**299/300 一致**。

唯一的差异：

```
ln(x), x0=-24.99, step=49.99  →  真二进制 1 点，复刻 2 点
sin(x), 同一组参数            →  两边都是 2 点
```

- 触发面很窄：`step` 接近 50 + 起点贴 `|x|=25` + 函数在负半轴无定义。
- 真二进制在该退化情形下更保守（提前放弃），复刻多吐一个合法点。
- **不影响游戏**：服务端 `step` 恒为 `0.01`，该组合不可达。

因此准确表述是：**在游戏可达参数域内逐位一致**，而非无条件全域一致。

## 6. 交叉验证：换一个反编译器

本仓库建立时用**新下载的 Ghidra 12.1.4**（经 REA）重新反编译了 `calculate_path`，
与上一轮 Ghidra 12.2 的结论逐条一致：

```c
dVar7 = 200.0 / local_98;                     /* 点数预算 */
iVar4 = 100;                                  /* 二分上限 */
if (0.001 <= ABS(dVar8 - local_98)) { ... }   /* 容差 */
if (((NAN(a) || NAN(b)) || (25.0 < ABS(x))) || (15.0 < ABS(y)))   /* 边界 */
```

换工具、换会话、换 Ghidra 版本，结论不变——这排除了"某次分析配置错了"的可能性。

## 7. 复现

```bash
cd ../                       # 上游分析仓库
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt
.venv/bin/python analysis/difftest.py
.venv/bin/python analysis/fuzz.py 300 7
.venv/bin/python analysis/difftest_parser.py
.venv/bin/python analysis/final_verify.py
```

## 8. 复原代码的自我约束

本仓库的 Rust 实现**不自我证明**。任何改动都必须回到上面那套对拍里，
以真二进制的输出为准。若本仓库与其冲突，错的一定是本仓库。

## 9. Rust 复原的验收方式

`cargo test` 不是"跑通就行"，它读的是 `fixtures/*.tsv` 里的真值：

| 夹具 | 用例 | 真值来源 |
|---|---|---|
| `parse.tsv` | 37 | 已验证的 Python 参照实现（其对二进制做过 90 例对拍） |
| `eval.tsv` | 70 | **真二进制**：unicorn 调用 `0x4eb9d0` |
| `path.tsv` | 224 | **真二进制**：unicorn 调用 `0x3c2530`（含 flip × ode 全组合） |

合计 331 条，加上 6 条把常量钉在 `.rodata` 比特上的断言。
夹具由 `tools/gen_fixtures.py` 生成，脚本里写明了每类真值的确切出处——
不要把"对照 Python"和"对照二进制"混为一谈。

比较语义与上游对拍保持一致：数值用 `==`（因此 `-0.0 == 0.0`），
NaN 只要求双方都是 NaN，不比较载荷位。

## 10. 移植过程中抓到的两个真问题

这两个都不是"写错了 Rust"，而是**参照实现与二进制不一致**，属于逆向收益：

**10.1 `log` / `ln` 在 `x ≤ 0` 的行为**

二进制给 `NaN`（含 `0` 与 `-0.0`），参照实现给的是 `-inf`。判据是 `v <= 0`
——次正规数 `1e-320` 仍正常计算，所以这不是下溢保护而是显式定义域检查。
后果真实：`NaN` 让曲线立刻终止，`-inf` 会让二分继续。上游已修正并回归全绿。

**10.2 除零的符号规则**

参照实现是 `math.copysign(inf, a) * math.copysign(1, b)`。移植时若把
`copysign` 的参数顺序写反（Rust 是 `x.copysign(y)`，取 `x` 的量值、`y` 的符号），
`1/0` 会得到 `0` 而不是 `+inf`。夹具里的 `div` 用例就是为这一条准备的。

## 11. 工具链现状与已知限制

| 能力 | 状态 | 代价 |
|---|---|---|
| 快速索引（capstone 反汇编 + rip-relative 交叉引用） | ✅ 可用 | 2.5 分钟，生成 139 MB `insns.pkl` / 19 MB `xrefs.pkl` |
| 函数边界 | ✅ **Ghidra 函数表 13503 条**（13263 在 `.text`）；`analysis/funcstarts_ghidra_va.pkl` | 秒级 |
| 模块归属 | ✅ 观测 544 / 扩散后 885（占 13263 的 6.7%），见 `02` §8/§10。~~483 / 3.6%~~ 是旧边界下的数，作废 | 秒级（依赖上面的索引） |
| Ghidra 12.1.4 反编译 | ✅ 已装并验证 | **首次全量分析约 19 分钟** |
| REA 的 MCP 原生分析 | ⚠️ 受客户端超时限制 | 见下 |

**为什么 MCP 侧的 Ghidra 目前不好用**：导入 + 全量 auto-analysis 约 19 分钟，
而 DeepSeek Harness 的 MCP 调用默认 60 秒超时。REA 自身可以放宽
（`ghidra.startup_timeout_ms` 已设到 40 分钟），但**客户端**的截止时间在注册配置里，
需要给 `rea-mcp` 条目加 `toolCallTimeoutMs` 并重启宿主。

因此重活既可以走 REA CLI，也可以走 MCP（客户端截止时间已设为 30 分钟，
实测 Ghidra 首次导入约 21 分钟可通过）。

**一次代价很高的教训（详见 `02-module-map.md` §12）**：本仓库第一版用的函数边界
（上游 `funcstarts.pkl`，来自 `.eh_frame`）与 Ghidra 的边界只交集 2 条，
且大量地址越出 `.text`。现在一律以 **Ghidra 函数表**为准，
换算 `Ghidra 地址 = 虚地址 + 0x100000`。此前"反汇编漂移"的判断是错的。

**另一个负结果**：这个 `.so` **没有 `.dep-v0`（cargo-auditable）段**
（`inspect_rust_binary` 已确认），所以无法从依赖清单反推 crate 结构。

---

## 12. 调用图的盲区：函数指针派发（原 `11`）

> 2026-11 归并：原文 `archive/11-dispatch.md` 已移入 `docs/archive/`，本节收其全部结论。

### 12.1 现象

做调用关系分析时反复出现两种情况：

- `0x3c2bc0`（开火结算）**本地索引里零调用者**，Ghidra 的 `procedure_callers` 也是空；
- `0x5eb57d`（掩膜测试的薄包装）同样零调用者。

原因不是它们没被用，而是**经函数指针调用**：Rust 的 `dyn Trait`/闭包被编译成
「`.data.rel.ro` 里一个指针槽 + 运行时间接跳转」。静态调用图与 Ghidra 的 caller 列表都断在这里。

### 12.2 方法：从重定位表反查指针槽

`analysis/relocs.pkl` 记录了 `R_X86_64_RELATIVE` 重定位（`槽位 → 目标地址`）。
把目标地址反过来查，就能找到"哪个数据槽装着这个函数的地址"；再用 capstone 的
交叉引用找"谁引用这个槽"。

```python
slots = [slot for slot, adds in relocs.items()
         if any(typ == 8 and addend == TARGET for typ, addend in adds)]
users = {owner_of(insn) for slot in slots for insn, *_ in xrefs.get(slot, ())}
```

工具封装：`tools/ptr_slots.py <vaddr>`。

### 12.3 结果（观测）

| 目标函数 | 指针槽 | 引用槽的函数 | 模块 |
|---|---|---|---|
| `calculate_path` `0x3c2530` | 1 | `0x3c2bc0` | `graphwar_shot_calculator.rs` |
| | | `0x3e2830` | **`graphwar_target_manager.rs`** |
| `shot_calculator` `0x3c2bc0` | 1 | `0x470eb0` | `graphwar_game_state_manager.rs` |
| | | `0x51a750` | **`graphwar_function_evolutionator.rs`** |
| `mask_test` `0x4f15b0` | 0 | — | 无指针槽，调用者全是直接调用 |

**观测**：`calculate_path` 被 `shot_calculator` 与 `target_manager` 两处间接调用；
`shot_calculator` 被 `game_state_manager` 与 `function_evolutionator` 两处间接调用。

**当时的两条推断（已分别结案）**：

| 推断 | 现在 |
|---|---|
| `function_evolutionator` 会"模拟开火"——它拿到的是整个开火结算函数，说明 AI 在批量试射候选函数评分 | ✅ **成立**：AI 把候选函数当一次真实射击跑（`AI.md` §4、`98`） |
| `target_manager` 也会算曲线（直接用 `calculate_path`），暗示它在做瞄准/威胁评估 | 🔶 **仍未验证**：`target_manager` 本身就是未解模块（`02` §13.4） |

### 12.4 方法学结论（已写进 `00-status` §7）

- "谁调用谁"只看直接调用**会得出错误结论**（会以为某函数是死代码）；
- 补法：重定位表反查指针槽 → 交叉引用找使用者；
- 仍有盲区：经**寄存器/结构体字段**传递的函数指针（不是静态槽位）依然看不到。
  所以任何"没有被调用"的判断都必须写成"**未观测到调用**"，而不是"没有调用"。

---

## 13. 活体 oracle：frida 注入真游戏进程（原 `20`）

> 2026-11 归并：原文 `archive/20-live-oracle.md` 已移入 `docs/archive/`。

### 13.1 为什么这一步是分水岭

| 手段 | 能回答 | 不能回答 |
|---|---|---|
| 静态反编译（Ghidra） | 代码长什么样 | 它实际返回什么 |
| unicorn 模拟 | **纯函数**的返回值 | 需要真实堆/锁/Godot 状态的东西 |
| **frida 注入运行中的游戏** | 内部函数实际返回什么、实际被调用多少次 | 需要先造出合法 Rust 对象的参数 |

### 13.2 环境准备（踩坑记录）

REA 自带的 frida 工具报 `Could not locate the bindings file`——因为当初 npm 的
`allowScripts` 策略拦了 frida 的安装脚本。修复：

```bash
cd .rea-runtime
npm install-scripts approve frida       # 放行
npm rebuild frida --foreground-scripts  # 触发 prebuild-install 下载预编译绑定
# → node_modules/frida/build/frida_binding.node
```

两个后续坑：

1. **REA 的 MCP 进程会缓存启动时的失败**，所以它那边的 frida 工具仍然不可用；
   本仓库的做法是**绕过 REA、直接用 frida 的 Node API**（`tools/frida_live.js`）；
2. **每次 bash 调用是独立 PID 命名空间**，`enumerateProcesses()` 看不到上一次调用启动的进程
   ⇒ 必须用 **spawn 模式**：让 frida 自己拉起游戏，再从同一进程树里 attach。

### 13.3 工具

```bash
node tools/frida_live.js modules
node tools/frida_live.js call <rva-hex> <ret-type> [arg-type:value,...]
```

`rva` 用**虚地址**（`base + rva`）。注意 Ghidra 地址与虚地址差 `0x100000`：
Ghidra 的 `FUN_00a302b0` → 虚地址 `0x9302b0`。

实测（真游戏进程内）：

```
$ node tools/frida_live.js modules
{"gw":{"name":"libgraphwarrust.so","base":"0x7f44e1e00000","size":10241312}}

$ node tools/frida_live.js call 0x9302b0 double double:2.7   → 2
$ node tools/frida_live.js call 0x9302b0 double double:2.3   → 2
$ node tools/frida_live.js call 0x9302b0 double double:-2.7  → -3
$ node tools/frida_live.js call 0x9302b0 double double:8.718 → 8
```

### 13.4 顺带纠正一个结论：`0x9302b0` 是 `floor`，不是截断

`08-plane.md` 早先说它是 `round`（错），后按掩膜反推说"截断"。**活体调用定案：`floor`**
（`2.7→2`、`-2.7→-3`）。对光栅化没有影响——那里的输入是 `sqrt(...) ≥ 0`，
`floor` 与向零截断等价，所以 `08-plane.md` 的像素结论仍然成立；但函数身份必须写准。

### 13.5 能力边界

**能**：用标量参数调用任意内部函数、读返回值；枚举模块、读写内存、hook 函数看调用序列。

**不能**：直接构造 Rust 的 `Arc`/`Vec`/`StringName` 当参数（要先在进程里造出合法对象）；
headless 无输入 ⇒ 光有注入能力**驱动不了对局**（这条后来由 §14 的 harness 解决）。

---

## 14. 原版游戏 harness：从"不可达"到"能开一局"（原 `21` → `22`）

> 2026-11 归并：`archive/21-oracle-limits.md`、`archive/22-live-game-harness.md` 已移入 `docs/archive/`。
> 本节把"当时判定不可达的五条路"和"最后怎么绕过去的"记在一起——
> **两半都有价值**：前一半是负结果清单，后一半是可复现的做法。

### 14.1 五条被否证的路径（原 `21`，**结论已被 §14.2 推翻**）

目标当时是"为回合/目标选择/AI 建立可复现的真值来源"。逐条试过：

| # | 尝试 | 结果 |
|---|---|---|
| 1 | 从 GDScript 调原版逻辑 | ❌ **前提不成立**：原扩展只注册 12 个类、25 个方法，几乎全是 UI 回调（`on_text_change`/`on_resized`/`pressed`）；`AxisDrawer`/`EffectDrawer`/`GraphwarLanVBox`/`GraphwarLongPressAnimation` **零方法**。玩法逻辑不对外 |
| 2 | 让真游戏 headless 跑起来再观察 | ❌ 跑得起来但停在主菜单：12 个关键函数 hook 8 秒**全部零调用**（`calculate_path`/`parse_function`/`eval`/`rasterize`/`point_test`/`mask_ctor`/`turn_a`/`turn_b`/`life_entry`/`shot_calc`/`levelgen_a`/`levelgen_b`） |
| 3 | 用原始场景自建 harness | ❌ 两个硬障碍：AdMob 原生插件缺失（`LoadAdError` 未定义 ⇒ `Ad_holder.gd` 解析失败 ⇒ `play_screen` 连带失败）；包里只有 `.import`+`.ctex`，**源 `.svg` 不在包内** ⇒ `Resource file not found` |
| 4 | 直接 `ClassDB.instantiate("GraphwarRootNode")` | ❌ 根节点自身就要拉起整条 UI 场景，绕不开上述两个障碍（无资源时 panic `failed to load resource at 'res://axis_option_0.svg'`） |
| 5 | 补桩 + 占位资源 | 🔶 **资源障碍已解除**（`tools/make_placeholders.py` 生成 149 个 `.svg` + 16 个 `.png` 占位；`godot-live/stubs/` 补了 `AdSize`/`AdPosition`/`AdRequest`/`AdView`/`AdListener`/`LoadAdError`/`MobileAds`/`PoingAdMobVersionHelper`），但卡在 `Ad_holder.gdc` 是**编译字节码**（1699 字节，`GDSC` 魔数，无符号表），改不动 |

当时的结论是"玩法逻辑没有可复现的真值来源"。**这个结论作废了**——见 §14.2。

### 14.2 三个卡点逐个拆掉（原 `22` §1）

| # | 卡点 | 解法 |
|---|---|---|
| 1 | 包里只有 `.ctex`/`.import`，源 `.svg`/`.png` 不在包内 → `Resource file not found` | `tools/make_placeholders.py` 生成同名占位（149 svg + 16 png） |
| 2 | `Ad_holder.gd` 依赖 AdMob **原生插件** → `LoadAdError` 等类型未定义，脚本解析失败 | 用自己的 `Ad_holder.gd`（`extends Control` + 5 个信号）**替换**编译字节码 `Ad_holder.gdc`（§14.1 尝试 5 卡住的那一步，正解是替换而不是修字节码） |
| 3 | 自动加载/脚本一个都不跑，引擎只打印 banner 就无限加载资源 | 游戏自带 **`project.binary`**（编译后的项目设置）覆盖了我们的 `project.godot`，**删掉它** |

第 3 条是排查出来的：同样的 `project.godot` + 脚本放到空项目里能正常跑（`PROBE_READY` 打印），
放进 `godot-live` 就完全没反应；把 `project.binary` 改名后立刻正常。

### 14.3 结果（实测）

```
$ godot --headless --path godot-live
PROBE_READY
PROBE_F60 current=GraphwarRootNode:<GraphwarRootNode#60246984152>
TREE root [Window]
TREE   probe [Node]
TREE   GraphwarRootNode [GraphwarRootNode]
TREE     VBoxContainer [VBoxContainer]
TREE     IAP_holder [Control]
TREE     Ad_holder [Control]
TREE     IOS_album_holder [Control]
TREE     Android_mediastore_holder [Control]
```

**原版 `GraphwarRootNode`（Rust 写的游戏根节点）运行在我们的项目里**，并建起了整棵子节点。

复现：

```bash
cd godot-live && rm -f project.binary            # 关键：让 project.godot 生效
python3 ../gw2-rebuild/tools/make_placeholders.py .
godot --headless --editor --path . --quit         # 首次导入
godot --headless --path .                         # 运行
```

`godot-live/` 由 `rsc/`（修好命名后的导出资源）+ 原版 `.so` + 我们的桩构成，属本地产物，已 gitignore。

### 14.4 开一局真实对局（原 `22` §5）

**按钮链路**（靠枚举按钮摸出来的）：

```
MainMenu  .../VBoxContainer2/LocalButton                    text=Battle
LanLobby  .../MarginContainer/HBoxContainer/CreateButton    text= Create Game
GameSetup .../OptionsContainer/AddBotButton                 打开 AddBotPopUp
AddBotPopUp .../VBoxContainer/OptionsContainer/AspectRatioContainer/AddBotButton   选难度，加 Bot
GameSetup .../MarginContainer/HBoxContainer/StartButton     text= Start
```

按完后 `ScreensControl` 的可见子节点从 `GameSetup` 变成 **`PlayScreen`** —— 对局开始。
**坑**：`AddBotPopUp` 打开时会挡住 Start；不加 Bot 直接 Start 不会报错、也不会切屏（静默无操作）。

**玩法函数确实在跑**（`node gw2-rebuild/tools/hook_live_game.js`，12 秒一局）：

```
parse_function  11      eval            1       rasterize     38
point_test   21629      mask_ctor       2       life_entry    12
turn_a       1259       turn_b       1531       levelgen    1 + 1
shot_calc       0       calculate_path  0
```

**让 Bot 真正开火的关键开关**：默认回合时限太长，Bot 不动手（45 秒里 `shot_calc`/`calculate_path`
都是 0）。在 GameSetup 里先按 `TurnTimeButton` 两次、再按 `TurnModeButton` 把节奏调快，
然后 Start，60 秒内就得到完整覆盖：

```
shot_calc 159   calculate_path 159   parse_function 66
turn_a 2075     turn_b 2085          life_entry 12   levelgen 1+1
```

⇒ 开火链路（解析 → 采点 → 结算）在真实对局里被完整执行过 159 次。

---

## 15. 用真实对局做验证与取证（原 `23`、`26`）

> 2026-11 归并：`archive/23-live-verification.md`、`archive/26-harness-control.md` 已移入 `docs/archive/`。

### 15.1 采样器：真实对局逐点一致（69/69）

`tools/capture_path.js`：frida 拉起 harness 并自动开局（Bot 开火），
在 `calculate_path`（虚地址 `0x3c2530`）上挂钩：

- **onEnter**：读入参 —— `rdi` 输出 Vec、`rsi` tokens Vec、`rdx` flip、`rcx` ode、
  `xmm0/1/2` = `x0`/`y0`/`step`；
- **onLeave**：读输出 Vec（`cap,ptr,len`）里的点序列，每点 16 字节 `f64 x, f64 y`；
- Token 按 `{u64 tag, f64 value}` 共 16 字节解析。

产物：`.rea-work/captured_path.json`。实测：

```
调用 0: x0=9.175978751627657 y0=-2.447450870359054 step=0.5 flip=True ode=False tokens=3
  真游戏 69 点 / 我们 69 点 → 逐点一致 69/69
调用 1: 同上 → 69/69
```

两个附带观察：

1. **这一次抓到的 `step` 是 `0.5`**，但**不能**推出"实况步长是 0.5"——实况是 **0.01**
   （服务端常量 `0x3f847ae147ae147b`）。抓样里出现 0.5 的原因**仍未查**
   （该抓样来自我们自己的 harness，可能不走服务端的配置分支）。
   ⇒ 这条 69/69 只证明"`step=0.5` 下算法逐点一致"，**不覆盖实况 0.01**；
   0.01 的证据在 oracle 24/24（`91` §8）与 82 发停点对照（`86` §2.0/§14）。
2. **士兵坐标是连续值**（`9.175978751627657`），不是整点 —— 与"换算成像素才取整"一致。

### 15.2 开火结算与生命的活体取证（只到形态，未成规格）

`tools/capture_args.js` 抓 `shot_calc`/`life_entry` 的寄存器与指针前 4 个字：

```
shot_calc（159 次/局）
  调用前 rdi Vec = [cap=3, ptr, len=3]
  调用后 rdi Vec = [cap=8, ptr(新), len=5]
  xmm0 = 0.5（就是 step），xmm1 = xmm2 = 0

life_entry（12 次/局）
  rdi = 固定上下文指针（12 次相同）
  rsi = 230 / 231 / 233 / 234（士兵 id 之类）
  rdx = 1,  rcx = 0
```

⇒ `shot_calc` **接收一个 3 元素 Vec、产出 5 元素 Vec**（容量 3→8、指针换了新的），
符合"往事件表里追加"的形态，但每个元素的字段布局**没解**，所以不宣称结算规则已还原；
`life_entry` 的形态是 `life_entry(ctx, id, 1, 0)`，12 个样本不足以推出伤害/死亡规则。

### 15.3 把 harness 当玩家用：能做什么、卡在哪（原 `26`）

| 能力 | 做法 | 证据 |
|---|---|---|
| 进对局 | 按 `LocalButton → CreateButton → AddBotButton → StartButton` | `ScreensControl` 切到 `PlayScreen` |
| 找到开火按钮 | `PlayScreen/.../RightControls/HBoxContainer/FireContainer/FireButton` | 对局开始后 `FireContainer` 有 2 个子节点（`ClipButton` + `FireButton`） |
| **真的开火** | `emit_signal("pressed")` on `FireButton` | 协议里出现 `{"FunctionFireRequest":{"player_id":229}}` |
| 找到函数输入节点 | `find_children("*", "GraphwarFunctionInput", true, false)` | 唯一一个：`PlayScreen/.../HBoxContainer2/GraphwarFunctionInput` |

**卡住的地方**：想让玩家提交**非空函数**，试了三种调用组合都不生效——协议里
`FunctionUpdateRequest` 始终是 `{"player_id":229,"function":""}`：

```gdscript
fi.on_text_change("0")
fi.on_text_submitted("0")
# 以及
fi.on_text_change("0"); fi.force_update_cb(); fi.on_text_submitted("0")
```

节点内部**没有** `LineEdit`/`TextEdit` 子节点（`find_children` 返回 0）⇒ 文字是 Rust 自绘、
状态在 Rust 内部；`on_text_change` 大概是"输入框内容变了"的回调，**不是**"设置内容"的接口。

⇒ 未找到"程序化设置玩家函数"的入口。要继续得逆向 `GraphwarFunctionInput` 的内部状态，
或模拟真实按键事件。**这一条至今没做**（见 `107-docs-consolidation.md` 的遗留清单）。

同一次实验抓到的 AI 提交函数样本（`k*x` 过原点为主）→ 收在 [`AI.md`](13-ai.md) §2.1。

### 15.4 这套手段能证明什么、不能证明什么

| 问题 | 答案 |
|---|---|
| 能不能拿到"真值"？ | 能：`calculate_path`/`shot_calc`/`life_entry` 的入参与返回值可以直接读 |
| 能不能按需制造场景（如友军误伤）？ | **不能**：玩家函数注入没打通（§15.3），只能等 Bot 随机撞上 |
| 能不能复现游戏一局？ | **不能**：出场顺序/颜色/AI 搜索都依赖 OS 熵（`47`、`34`、`AI.md` §8.7） |
| 界面侧能不能驱动？ | 能：按钮可以脚本化点击，且已证实等价于真点（`92` §3） |

---

## 16. 手段选择顺序

1. **读现有文档 / 已有反汇编结论** —— 能查到就别重新逆；
2. **`objdump`** —— 需要精确指令字节（判据、常量、调用点），这几轮所有确凿结论都来自它；
3. **frida 运行时观测** —— 需要"实际怎么跑"（`93` 的 25 s 上限就是运行时确认的）；
4. **REA 伪代码** —— 只有在①~③都给不出结构时，用来快速看懂控制流（前置检查见 `90` §2.6/§2.8）。

REA 的定位、静默陷阱与强制前置检查见 [`90`](06-verification.md) §2.6–§2.8。

## 17. 整机"逐位相同"的口径 —— 以及哪几处**证明做不到**

§1 的分级是**单个结论**的等级。这一节定的是**整个复原品**的验收标准：
"能跑一局"不算完成，**逐位相同**才算。但这事必须先说清边界，否则会变成
一句无法证伪的口号。

### 17.1 口径（可判定的写法）

> 给定**相同的输入序列**（同一份回放的消息流，或同一串客户端请求）、
> **相同的启动配置**（模式四元组 / 种子 / 难度）、并且**没有触发 25 s 墙钟截断**，
> 那么我们的实现与真游戏在**每个 tick 的状态**、**每条广播消息的字节及其在一批内的顺序**、
> **每条弹道的每个采样点**上都应完全相等。

三条都必须满足才叫"逐位相同"：**状态、消息字节、几何**。
只对上前两条而弹道差 1e-9 不算；只对上弹道而消息字段顺序不同也不算什么。

### 17.2 三处无法直接逐位的点，按**原因**分类

先把话说准：**这三处都是随机/非确定的，但"是不是随机"不是有用的分类轴**。
有用的轴是"**结果是否由输入决定**"。按这条分两类：

| 类 | 判据 | 处 | 怎么才能逐位 |
|---|---|---|---|
| **A. 可复现，但缺一份尚未捕获的熵** | 输入不够，**补一份熵就够** | **回合出场顺序**（`HashMap` 迭代序，键来自 `getrandom`） | 拿到那份熵 + 还原表结构 |
| **B. 本质不由输入决定** | 再补多少输入也不够 | **25 s 墙钟截断**（取决于机器速度）、**多客户端竞态**（取决于调度） | 只能退化口径 |

**关于 A 的说明**：出场顺序**不是**"不是随机数"，它**就是随机**——只不过来自**另一条
独立的熵流**（OS `getrandom` → SipHash 的 key），和我们已验证的 ChaCha 主流不是一回事。

**`12-rng.md` 已把这个 A 类做成了实测**：`0x47dec0` 的内联 SipHash 键就是 `[rdi]` 的
16 字节 `getrandom` 原值（每建一个 map 的 k0 加一）。**把它钉住**以后跨进程顺序完全一致；
**连 ChaCha 种子一起钉**之后，两次独立进程 **345/370 帧逐字节相同、43/48 类消息完全相同**，
只剩 tick 字段不同（那是已知的 B 类）。
⇒ A 类的门槛比原先写的高：需要「每线程 16 字节 + 每个 map 的创建序号 + 实体 id 分配」，
后两样在真机上会漂移。
`12-rng.md` §2–§3 已用 9 局否证它来自 ChaCha 流，但那是"排除了这条流"，不是"排除了随机"。

要让它逐位，三样得凑齐：

1. **算法**：可以精确钉死。二进制里有 `/rustc/ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96/`，
   这个 commit 唯一确定 std 版本 ⇒ `RandomState`（SipHash-1-3）与 hashbrown 表布局的实现版本；
2. **键**：`getrandom` 给的 16 字节。**这一份我们没有捕获**，而它等价于 ChaCha 的种子——
   同一个算法、不同 key，迭代序就不同。所以**"保证算法一样"只能保证分布一样，不能保证顺序一样**；
3. **表几何**：容量、增长时机、插入顺序（hashbrown 的迭代序与容量直接相关）。

⇒ 现状：1 能钉，2 没抓，3 没还原。所以**归 A 类，不是"做不到"**。

| 处 | 现有证据 |
|---|---|
| 出场顺序 | `12-rng.md` §2/§3（9 局否证 ChaCha 流）、§5（判为 HashMap 迭代序，**推断**）、§9（要实证还差什么） |
| 25 s 墙钟截断 | `13-ai.md` 同一次运行两种结局都出现过、`13-ai.md` |
| 多客户端竞态 | `09-network.md` §5（**推断**，未验证：没抓到过两路同时到达的样本） |

⇒ 对这三处，验收口径要退一步，写成可判定的形式：

- 出场顺序与**广播批内顺序**：**按回放给定的顺序消费**（回放已经把顺序固定下来了），
  而不是"同种子复现"。`12-rng.md` 实测：开局那批"全量重发"的 **13 类消息顺序逐局不同**，
  约占一局 **45%** 的报文。
- 墙钟：只在**未截断**的运行上要求逐位；被截断的运行只要求"截断点之前的全部代逐位"。
- 竞态：只要求"给定同一条消息序列，状态转移函数逐位"。

### 17.3 最有力的验收手段：**回放对拍**

`~/.local/share/godot/app_userdata/Graphwar/replays/` 有 **380 份真对局录像**，
每份是 gzip + JSONL 的**完整消息流**（`{"event":{...},"tick":N}`）。
它同时固定了：出场顺序、墙钟截断的后果、网络到达顺序。

所以端到端逐位验收应该长这样：

1. 把一份回放的事件流喂给我们的服务端/状态机；
2. 每处理一条事件，比较**我方状态**与回放推出的状态（`tools/replay_state.py` 已是这套）；
3. 再比较**我方本该发出的下一条消息**与回放里的下一条（逐字节/逐字段）；
4. 380 份全过 = "在真实输入分布的覆盖下逐位相同"。

现状：`tools/parse_replay.py`、`tools/replay_state.py`、`examples/replay_compare.rs`
已经能读和比对**状态**；**还没有**"预测下一条消息并逐字节比对"这一环。
⇒ 这是验收链上最大的一块空白，记在这里，别当成已完成。

### 17.4 容易被"能跑"掩盖的三类偏差

`cargo test` 绿、能开一局，不代表逐位。以下三类只有专门对拍才抓得到：

| 偏差类型 | 例子 | 怎么抓 |
|---|---|---|
| **浮点运算顺序** | 同样的公式，`a*b + c*d` 与 `fma` / 不同结合顺序的末位不同 | 用真二进制当 oracle 逐点比（`path.tsv` 就是这么来的） |
| **JSON 数字序列化** | `1.0` 写成 `1`、`1e6` 写成 `1000000.0` | 逐字节比抓包/replay 的原始文本，别 parse 后再比 |
| **HashMap 的其他用途** | 除了出场顺序，障碍/玩家遍历是否也走 HashMap？**未查** | 审 `tools/ptr_slots.py` + 运行时换插入顺序看输出是否变 |

### 17.5 一句话

**"能跑"是必要条件，"逐位"是验收条件。** 三处做不到的地方已经写明退化口径，
其余全部按 17.1 判。谁要是把"能跑起来"写成"已复原"，就是没读这一节。

---

## 18. 验收清单（每条都能照着跑）——原 `49-verification-checklist`

> 全文：[`archive/49-verification-checklist.md`](archive/49-verification-checklist.md)。

### 18.1 前置

```bash
cd gw2-rebuild
export CARGO_HOME=$PWD/../.cargo-home CARGO_TARGET_DIR=$PWD/../.cargo-home/target
```

需要游戏二进制的条目还要：

```bash
ln -sf ../libgraphwarrust.so .          # 或把仓库根目录的 .so 复制进来
python3 -m venv ../.venv && ../.venv/bin/pip install -r requirements.txt
```


## 19. 覆盖面矩阵："我的绿"到底盖住了多大一块——原 `88-coverage-matrix`

> 全文：[`archive/88-coverage-matrix.md`](archive/88-coverage-matrix.md)。

### 19.1 起因

用户问：「你之前是不是不知道有 `SimultaneousTurns` 模式？如果是的话，那 README 全绿也复原不了游戏吧。」

**答案是：不知道，而且他说得对。** `SimultaneousTurns` 是他实际去玩才暴露出来的。
我的 ✅ 是在**我采样到的那个表面上**绿，而那个表面本身从来没有被枚举过。

### 19.2 游戏模式的实际空间（从 replay 实测取值）

| 维度 | 取值 | 种类 |
|---|---|---|
| `TurnMode` | `SequentialTurns`、`SimultaneousTurns` | 2 |
| `FunctionMode` | `NormalFunction`、`DiffEqFunction` | 2 |
| `TimeMode` | `Timer30s/1m/2m/3m/5m/Inf` | 6 |
| `AxisMode` | `EveryUnit`、`EveryFive`、`OnlyMain`、`NoAxis` | 4 |

组合空间 = 2×2×6×4 = **96**。

### 19.3 哪些维度真的影响"我们复现的东西"

| 维度 | 影响物理？ | 依据 |
|---|---|---|
| `FunctionMode` | **是** | Normal 用 `y=f(x)`；DiffEq 用 **RK4 积分**（`graphwar_path_calculator::calculate` 的 `Mode::Ode`）；生成器变量槽也变（`archive/ai/71` §5.0） |
| `TurnMode` | **是** | Sequential 每回合一发；Simultaneous 同 tick 多发同时结算 |
| `TimeMode` | 否 | 只影响玩家思考时间 |
| `AxisMode` | 否 | 只影响坐标轴绘制 |

⇒ 真正需要覆盖的物理维度只有 **2×2 = 4 格**。


## 20. 复现手册：怎么确认这些代码是对的——原 `16-reproduce`

> 全文：[`archive/16-reproduce.md`](archive/16-reproduce.md)。

### 20.1 两个仓库的分工

| 仓库 | 内容 | 远端 |
|---|---|---|
| `gw2`（上游） | 曲线算法的论证（`REPORT.md`）+ 对拍工具链（`analysis/`）+ 已验证的 Python 参照实现 | `Cmd-GZ/graphwar2-analysis` |
| `gw2-rebuild`（本仓库） | 复原实现（Rust）+ 观测量与推断记录（`docs/`） | `Cmd-GZ/graphwar2-rebuild` |

本仓库**不包含**二进制、反编译产物或夹具以外的原始数据。上游也从没收录过本仓库的任何文件
（可用 `git -C .. ls-files | grep gw2-rebuild` 验证，应为 0 行）。

### 20.2 跑测试（不需要二进制）

```bash
cd gw2-rebuild
export CARGO_HOME=$PWD/../.cargo-home CARGO_TARGET_DIR=$PWD/../.cargo-home/target
cargo test --offline
```

期望：**21 条通过**。其中

| 组 | 条数 | 依赖 |
|---|---|---|
| 常量锚定 | 6 | 无（断言写的是 `.rodata` 里的比特） |
| 金标准 | 4 | `fixtures/*.tsv`（已入库） |
| 坐标系 + 掩膜 | 6 | 无 |
| 形状（事件/生命） | 5 | 无 |

`fixtures/` 已提交，所以**只跑测试不需要游戏二进制**——这是刻意的。

### 20.3 真值从哪来

| 夹具 | 用例 | 真值来源 |
|---|---|---|
| `parse.tsv` | 37 | 上游已验证的 Python 参照实现 |
| `eval.tsv` | 70 | **真二进制**：unicorn 调用 `0x4eb9d0` |
| `path.tsv` | 224 | **真二进制**：unicorn 调用 `0x3c2530` |
| `module_map*.tsv` | — | 静态索引（非真值，是归属统计） |

重新生成（需要二进制与 `.venv`）：

```bash
../.venv/bin/python tools/gen_fixtures.py
```

### 20.4 用真二进制复核全链路

真正的"标准答案"在上游那套对拍里，本仓库**不自我证明**：

```bash
cd .. && .venv/bin/python analysis/difftest.py         # 768 例（原 704 + 64 条早停回归用例）
.venv/bin/python analysis/difftest_parser.py           # 90 例
.venv/bin/python analysis/final_verify.py              # 648 + 40
.venv/bin/python analysis/fuzz.py 300 7                # 随机语法树
```

若本仓库与它们冲突，**错的一定是本仓库**。

### 20.5 环境坑（都踩过）

| 现象 | 原因 | 处理 |
|---|---|---|
| `npm EROFS` | `~/.npm` 只读 | 用工作区内的 `npm_config_cache` |
| `/tmp` 文件消失 | `/tmp` 是按命令独立的 tmpfs | 中间产物写进工作区 |
| `cargo` 找不到 registry | `~/.cargo` 只读 | 设 `CARGO_HOME` 到工作区 |
| git push 报 ssh 配置错误 | `/etc/ssh/ssh_config.d/` 权限问题 | `GIT_SSH_COMMAND="ssh -F /dev/null"` |
| REA 工具调用 60 秒超时 | 客户端默认值 | 注册里加 `toolCallTimeoutMs: 1800000` |
| 反编译里 `_DAT_xxxx` 读不对 | 那是 Ghidra 地址 | 读原始字节要减 `0x100000` |

### 20.6 改动本仓库的规矩

1. **不得**把推断写成结论——文档里每个结论都要能指到"观测"或"推断"。
2. 改动 `src/` 的行为后，`cargo test` 必须仍全绿；若夹具需要更新，
   说明真值变了，必须回到 §4 用真二进制复核。
3. 新发现的观测先进 `docs/`，再考虑写代码。顺序反了容易把猜测固化成实现。
4. 上游 `REPORT.md` 若与本仓库冲突，先判断是不是上游错；是则修上游（本目标允许），
   不是则改本仓库。


## 21. 回放消息级对拍：从 0–1/400 到 71/400

> 两篇过程档案：[`archive/136-replay-diff.md`](archive/136-replay-diff.md)（口径与语料统计，数字已被 142 取代）、
> [`archive/142-diff-converge.md`](archive/142-diff-converge.md)（收敛的原因与四条修正）。

### 21.1 一句话

> 380+ 份真回放现在能**逐条对拍**了：把它们喂进 `game::Match::fire`，比"我方产出的报文"
> 与"回放里的报文"。当前**按发货状态（`fire` 用 step=0.5、不支持 DiffEq）只有 18/1079
> 个结算批逐字节相同**；但几何探针显示，**换回真服务端的 0.01 步长后，739 发里 605 发的
> `end_x` 逐位相同**（step=0.5 时只有 27 发）。⇒ 最大的一处缺口不是物理，是
> `src/game.rs:516` 那个写死的 `0.5`。

---

### 21.2 一句话

> 剩下的一直过不去，不是因为"口径松不下来"，而是 **`game.rs::fire` 把三个模式相关的
> 决定写死了**：采样模式恒 `Normal`、修正文本恒加后缀、死亡特效恒 `DeathEffect`；
> 外加**爆炸点不是采样器停点**这一条没被认出来。四条都有回放真值，改完就涨 70 倍。

---


## 22. 防无用功守则（失败清单与对策）——原 `90-anti-waste`

> 全文：[`archive/90-anti-waste.md`](archive/90-anti-waste.md)。

### 22.1 失败模式（全部真实发生过，按次数排）

| # | 模式 | 本次实例 | 代价 |
|---|---|---|---|
| 1 | **不查权威来源就写常量** | `step=0.5` 硬编码，而 `AGENTS.md` Hard facts 写着 0.01 | 4 轮分析作废 |
| 2 | **测量工具没验证就下结论** | 提取器取空函数、对比工具没做掩膜走查、死亡数重算 | 3 轮 |
| 3 | **拿"看起来对"当验证** | 0.5 步长下停点差 0.13，"大致对"骗过好几轮 | 隐性 |
| 4 | **实验改动没回滚干净** | `lo=mid` 用 `/tmp` 备份（per-command tmpfs）丢失，坏代码被提交 | 1 轮 |
| 5 | **等被人指出才发现功能缺口** | `SimultaneousTurns`、`Player`、`Replays` | 3 次 |
| 6 | **复现失败时归咎于对方** | 说"复现不出来"，实际是我参数错（漏外层括号） | 1 轮 |


## 23. 三条与步长有关的旧记录（都带勘误）

> 这三篇的全部数字都是 **step = 0.5** 下测的，而**实况步长是 0.01**（本文 §3、[04-path-sampling.md](04-path-sampling.md)）。
> 结论未被推翻，但引用时必须换口径。全文：[`archive/25-live-collision.md`](archive/25-live-collision.md)、
> [`archive/37-paired-replay.md`](archive/37-paired-replay.md)、[`archive/89-real-step.md`](archive/89-real-step.md)。

**25（活体碰撞端到端）**：真实一局抓包 → 起点+函数 → 轨迹 → 掩膜 → 首次碰撞 全链路对上，
残差 0.228/0.460/0.433/0.393 是 0.5 折线的离散化量级；0.01 下同类比较的残差是 **1e-6 ~ 4e-4**。

**37（按回合配对的精确回放）**：致死集合一致，偏差 0.394/0.159 同样是 0.5 口径；0.01 下的逐发强证据是 82 发并集 **152/152**。

**89（实况步长 = 0.01）**：这篇是过程档案，只有两条仍现行 ——
① **实况步长 = 0.01**（82 发里曲线完整的 63 发，停点中位差 0.000000）；
② 早停缺陷的根因是二分复检漏写 `hi = mid2`（`0x3c2a0a`），已修复并 **24/24 逐点相等**（见 [04-path-sampling.md](04-path-sampling.md) §9）。
§5–§29 的推理**全部作废**，不要引用。

---

## 24. 整局对拍**剩余失败**的归因（2026-11-07，**401 份回放**＋2 份元数据的定格快照；纯分析）

> 任务书：[`dispatch/03-bit-exact-report.md`](../dispatch/03-bit-exact-report.md)。
> 口径一个字都没改：仍然调 `_scaffold::replaydiff::compare_batch`（投影 → 折叠 → 分块 →
> 块内多重集 → 字段级 diff），仍然用 `_scaffold::replaydiff::Policy`。
> 本节只多做一件事：把**每一批**的失败落进一个互斥的桶里，并让"各桶之和 == 失败批数"
> 能一条命令复算（`tools/bit_exact_attrib.py`）。

### 24.1 一句话

> **1059 个失败批（旧驱动）/ 512 个（收敛层），每一批都落进了一个有主的桶**：
> 收敛层里**未定性只有 1 批**；确切未知的另有三处（`E2` 真几何 25 批、`C` 文本残留 14 批、
> 以及整根"挖洞点"轴：66/291）。按可修性排：**爆炸点 y 的求值末位差（176 批）** >
> 多发批结构未接线（130 批，独占 26）> **`end_x` 老是一个采样点（95 批）** >
> **`pos_x` 与 `end_x` 在原版里是两个数（67 批）**。
> 任务书假设的 **A 类熵 0 批、`EffectInfo` 位置/时序 0 批** —— 这两项**不是**剩余失败的原因，
> 有逐批数据否证（§24.5、§24.6(b)）。

### 24.2 语料与基线：任务书的 5 个数**全部复现**（语料口径已改正）

**目录里不是 401 个文件，也不是 403 份回放**：`ls | wc -l` = **403**，其中
**回放 401 份**（`file -b` 带 gzip 魔数的正好 **401** 个）+ **元数据 2 份**
（`MANIFEST.tsv`、`SHA256SUMS`，明文、不是 gzip —— 前者首行写着"401 份回放定格快照"）。
⇒ **任务书写的"401 份"本来是对的**。

| 项 | 任务书 | 本轮实测 | 档 |
|---|---|---|---|
| 目录里的文件 | — | **403** = 回放 **401** + 元数据 **2**（`MANIFEST.tsv` / `SHA256SUMS`） | **观测** |
| 401 份回放的结局 | — | **282 份有结算批**、**4 份有窗口但每批 0 发**、**115 份整个文件没有窗口**、**真回放读取失败 0 份** | **观测** |
| 结算批总数 | — | **1080** | **观测** |
| 结算批内容命中 | 21 / 1080 | **21 / 1080** | **观测** |
| 消息种类序列命中 | 818 / 1080 | **818 / 1080** | **观测** |
| 完全一致的回放 | 1 / 401 | **1 / 401**（按"有结算批的 282 份"算是 1 / 282） | **观测** |
| 延迟段死亡集合命中 | 211 / 379 | **211 / 379**（多报 13 / 漏报 154 / 错人 1） | **观测** |
| 延迟段挖洞点命中 | 1 / 259 | **1 / 259** | **观测** |

⇒ **任务书的 5 个数一位不差，语料也一份不少。** 上一版本节把两份**元数据**当成了回放，
写成"403 份回放 / 2 份 `Replay::load` 失败 / 119 份没有结算批"，是**口径错**（2+119≠117），
已改正；**归因表、地址级证据、收益估计一个字都没变**（它们的分母是"批"，不是"文件"）。


> ⚠️ **本表是 `fire` 驱动的"修前"基线**（2026-11-07 之前）。[§26](#26-爆炸点-pos_y坐标来源逐指令定死末位差拆成两桶2026-11-07dispatch09)
> 改掉爆炸点常量口径之后，同一份快照上 `fire` 驱动的读数是
> **内容命中 324 / 1080、种类序列 818 / 1080、死亡集合 209 / 379、挖洞点 138 / 258**，
> 所以 `tools/bit_exact_attrib.py --check-baseline` 现在会报 4 项不一致 —— 那是**工具里写死的旧基线**
> （它的表头就写着"任务书基线"，本轮不允许改 `tools/`）。要复现下面这张表，先 `git stash` 掉 §26 的改动。

**工具那行 `文件 286（读失败 117）` 的可复算分解**（三个数必须这样加）：

```text
403 文件 = 401 回放 + 2 元数据
files_ok  286 = 282（有结算批）+ 4（有窗口、但每批 0 发 ⇒ 一个批行都不吐）
files_err 117 =   2（元数据：Replay::load 报「第 0 字节处出现未知值起始符 `#` / `b`」）
              + 115（真回放，但整个文件没有 CalculatingResults…ExecutingResults 窗口）
286 + 117 = 403 ✓        401 回放 = 282 + 4 + 115 ✓
```

⇒ **"读失败 2"的那 2 份不是回放，是元数据**；换成回放口径是 **真回放读取失败 0 份**。
（`examples/replay_diff` / `diff_conv` 打印的"读取失败 2 / 有结算批的 282"是同一件事：
它们也把元数据文件当普通文件读。）复算命令：

```bash
R=<ws>/.reference/replays/gw2-replays-snapshot/replays
ls $R | wc -l                                                          # 403
for f in $R/*; do file -b "$f" | grep -q gzip && echo x; done | wc -l   # 401（回放）
file -b $R/MANIFEST.tsv $R/SHA256SUMS                                   # 明文，非 gzip
# JSONL 一侧的分解（examples/replay_attrib 的输出）
python3 -c "
import json, collections
L = [json.loads(l) for l in open('out.jsonl')]
err = [r for r in L if r.get('batch', -1) < 0]
print(len(err), collections.Counter(r['error'][:12] for r in err))
print(len({r['file'] for r in L if r.get('batch', -1) >= 0}))
"
# → 117 Counter({'没有结算批': 115, '第 1 行: json:': 2}) ；282
```

**分母含义（本节所有比例）**：`内容命中` / `种类序列命中` 的分母是**结算批数 1080**；
`完全一致的回放` 的分母是**回放份数 401**；延迟段两个数的分母是**有可比对象的批**
（死亡 379、挖洞 259），与 1080 **不是**同一根轴。§24.3 / §24.4 两张归因表的分母都是**批**，
所以语料口径的更正**不影响**它们。

### 24.3 归因表 A：**任务书基线**（`examples/replay_diff` 的驱动，默认 `Policy`）

失败 **1059** 批（= 1080 − 21）。分桶规则与优先级见 `tools/bit_exact_attrib.py` 头部表。

| 类别 | 批数 | 占失败 | 独占 | 根因（指到地址/文档） | 可修性 |
|---|---|---|---|---|---|
| `C` `corrected_function` 文本 | 369 | 34.8% | 15 | `docs/142` §2.2（DiffEq 不加后缀 / Normal 补括号）；旧驱动没接线 | **已修，未接线** |
| `Y2` 爆炸点 y 中等差（1e-6~1e-3） | 332 | 31.4% | **332** | `docs/142` §2.3：原版拿**印刷文本**（常数 3 位小数）回代求 y；旧驱动用停点 | **已修，未接线** |
| `D` 死效块数量（死亡集合） | 132 | 12.5% | **132** | `docs/16` §33（`execute_results` 查表、查不到不发）；实际差的是**死亡集合** | 需先修几何 |
| `S` 多发批结构（逐发窗口） | 93 | 8.8% | 0 | `docs/141` §3（按类型分组）；`Match::fire_batch` 已存在 | **接线即得** |
| `E1` `end_x` 差一个采样点 | 50 | 4.7% | 0 | 本节 §24.5 ① | 可修（规则明确） |
| `SD` 多发批结构 + 死效块 | 37 | 3.5% | 0 | 同上两条叠加 | 同上 |
| `X` 爆炸点 x 末位差 | 29 | 2.7% | 0 | 本节 §24.5 ② | 可修（需两处算法） |
| `E2` `end_x` 真分歧（几何） | 17 | 1.6% | 0 | 掩膜/碰撞（`docs/142` §4 的 ring/fill 差） | 难 |
| **合计** | **1059** | 100% | 479 | | |

> ⚠️ 这张表是**旧驱动**的账：`C` 与 `Y2` 两项（701 批，66%）在 `_scaffold::diffconv::ConvPredictor`
> 里**已经修好并有真值**，只是 `examples/replay_diff` 走的是老的 `Predictor`。
> 所以"剩余失败"要看表 B，不看表 A。

### 24.4 归因表 B：**收敛层**（`--drive conv --policy strict`，即仓库里当前最好的驱动）

失败 **512** 批（内容命中 568/1080 = 52.6%，与 `examples/diff_conv` 报的 568/1080 逐数相同）。

| 类别 | 批数 | 占失败 | 独占 | 根因 | 可修性 |
|---|---|---|---|---|---|
| `Y` 爆炸点 y 末位差（≤1e-9） | **170** | 33.2% | **170** | 求值器末位（§24.5 ③） | 可修但需真值手段 |
| `S` 多发批结构（逐发窗口） | **123** | 24.0% | 26 | `docs/141` §3；`Match::fire_batch` 已落码（`src/game.rs`） | **接线即得** |
| `E1` `end_x` 差一个采样点 | **95** | 18.6% | 0 | §24.5 ① | 可修（规则明确） |
| `X` 爆炸点 x 末位差（`pos_x ≠ end_x`） | **67** | 13.1% | 42 | §24.5 ② | 可修（需两处算法） |
| `E2` `end_x` 真分歧（几何） | 25 | 4.9% | 0 | 掩膜 ring/fill + 碰撞判据 | 难 |
| `C` `corrected_function` 文本 | 14 | 2.7% | 0 | §24.5 ④（残留 14 例） | 可修 |
| `SD` 多发批结构 + 死效块 | 7 | 1.4% | 0 | 两条叠加 | 同上 |
| `Y2` 爆炸点 y 中等差 | 6 | 1.2% | **6** | 回代在少数函数上失效（解析/NaN 回退） | 可修 |
| `D` 死效块数量（死亡集合） | 4 | 0.8% | 4 | 死亡集合多/少 | 需先修几何 |
| `K` **其它（未定性）** | **1** | 0.2% | 0 | 未查（唯一一批） | 未知 |
| **合计** | **512** | 100% | 248 | | |

三种驱动的横向对照（同一份语料、同一个 `compare_batch`）：

| 驱动 | 是什么 | 内容命中 | 种类序列命中 | 多发批（130）里命中 |
|---|---|---|---|---|
| `fire`（表 A） | 每发一次 `Match::fire` | 21 / 1080 | 818 / 1080 | 0 / 130 |
| `batch` | 每批一次 `Match::fire_batch`（`docs/141` 的分组） | 21 / 1080 | **918 / 1080** | **0 / 130（结构对了，值还错）** |
| `conv`（表 B） | `diffconv::ConvPredictor`（模式/文本/回代/皮肤） | **568 / 1080** | 945 / 1080 | 0 / 130 |
| 两者都修（**未接线的合并**） | — | **预计 ≥594 / 1080** | — | 见 §24.8 |

> `batch` 驱动那一行的意思是：**把分组接上以后种类序列从 818 涨到 918（+100）**，
> 但内容命中仍是 21 —— 因为 `game.rs` 还没有"修正文本按模式分 / 爆炸点回代"这两条。
> 两条修法分居两个模块，谁也替不了谁。
>
> ⚠ **2026-11-07 第二轮已接线**（见 **§25**）：本表 `conv` 那一行是**接线前**的值。
> 分组接进收敛层之后：种类序列 **1074 / 1080**、内容 **595 / 1080**、
> 多发批骨架 128 / 130、**单发批 946 / 950 一条没变**（§25.4）。

### 24.5 四类机制：**依据与判别式**（都能复算）

**① `E1`：回放的 `end_x` 是「碰撞点的前一个采样点」**（95 批 / 表 B 18.6%）— **观测**

`FunctionPoints.end_x` 差 ≤ 0.02（实测集合只有 29 个值，全部是 `step/2^k` 的整数倍：
0.009375×19、0.0075×20、0.00875×8、0.00625×6、0.0025×5 …），**而同一个批的
`EffectInfo(Explosion).pos_x` 与我们算的停点只差 1 ulp**。用
`examples/replay_endx_probe` 把我方采样点列出来，直接看到**两个回放值分别命中相邻两个采样点**：

```text
1782677510 批 #4 owner=187266
  回放 end_x = 24.986478985755284  → 我方 pts[5391]
  回放 pos_x = 24.99585398575528   → 我方 pts[5392]（= 我方停点，Δ=3.55e-15）
1782812255 批 #0 owner=517981
  回放 end_x = 14.994964223829621  → pts[3788]
  回放 pos_x = 15.00246422382962   → pts[3789]
1783255470 批 #8 / 批 #10        同类（pts[5551]/[5552]、pts[4821]/[4822]）
```

⇒ 我方现在把**同一个 f64**（最后一个采样点）同时当 `end_x` 与爆炸点。
原版不是：`end_x` 取**前一个**点。这条是**确定性规则**，不需要任何熵 ⇒ 可修。
它解释 95 批；配合 `X`+`Y` 一起修可以清掉 **363** 批（§24.8）。

**② `X`：原版的 `pos_x` 与 `end_x` 不是同一个数**（67 批 / 13.1%）— **观测**

这批的特征是：`end_x` 与回放**逐位相同**，而 `pos_x` 差 **1 ulp**（`25.0` 上 ulp=3.55e-15，
`15.0` 上 1.78e-15，全语料 174 处、档位单一）。若原版把同一个 f64 写进两条报文，
不可能差 1 ulp ⇒ **两条报文的 x 来自两条不同的计算路径**。
代码侧支持这条：`FunctionPoints`（tag `0x25`=37）与 `EffectInfo`（tag `0x2C`=44）是**两个不同
生产点**（§24.6），与 `docs/16` §29.3(b) 记的"`execute_results` 里有两处 `EffectInfo` 生产点"一致。
缺的证据：`EffectInfo` 的**生产者地址没定位到**（§24.6 末），所以"它到底怎么算出这个 x"仍是**未知**。

**③ `Y`：爆炸点 y 的求值末位差**（170 批）— **观测**；机制 **推断****

`end_x`/`pos_x` 都逐位相同、只有 `pos_y` 差。176 批里 154 批带着可解析的 ulp：
**117 批恰 1 ulp、23 批恰 2 ulp、14 批 ≥3 ulp**（最多一例 10 ulp），
且**符号全部同向**（`我方 > 回放`，154/154）。同向 ⇒ 不是随机舍入，而是**同一条式子的系统性偏差**。
函数里含超越函数的 **149 / 176**（`sin`/`cos`/`tan`/`exp`/`ln`/`sqrt` 各种组合），
**推断**主因是超越函数实现（libm）与求值顺序的差异；缺的证据：把原版求值器
（`0x4eb9d0`，见 §17.2 的自校验）在这些确切输入上的输出抓出来比。
**未知**的部分：剩下 **27 批**函数只有四则运算、却仍差末位 —— 说明除 libm 之外还有一处
**结合顺序**不同（未定位）。

**④ `C`：残留 14 批 `corrected_function` 文本** — **未知**（未逐例）

`docs/142` §2.2 的规则在 1080 批里只剩 14 批不符；这 14 批同时还有几何差（13 批 `E2`），
即它们的 `function` 文本本身与我们的解析前置条件不同（例：函数里有未闭合括号且常数项为空）。
**没有逐例看**，不猜。

### 24.6 地址级证据（x86-64 `2026-08-28/libgraphwarrust.so`，本节自测）

**约定**：本节所有地址都是 **ELF 虚地址**（Ghidra/REA = 虚地址 + `0x100000`）。
脚本：`/home/CG/Myprojects/graphwar2-rebuild/.reference/venv/bin/python` + `capstone`。

**(a) `execute_results` 的边界是实测的**：`0x472470` 起 `push rbp/r15/r14/r13/r12/rbx` + `sub rsp,0x148`，
到 `0x47351b` 起是 `int3` 填充 ⇒ **`GraphwarGameStateManager::execute_results` = `0x472470`–`0x47351a`**
（`0x47351b - 0x472470` = **4267** 字节，与 `docs/14` §11 记的 4272 字节同一量级、同一函数）。

**(b) 它内部的广播顺序**（`call` 站点按地址递增）：

| 地址 | 调用 | 干了什么（实测） |
|---|---|---|
| `0x472ad8` | `call 0x453de0` | 入队 **`life_queued_death_event`**：被调函数在 **`0x453f6d`** 有 `movabs rax,0x800000000000001c`（tag **28**），随后 `mov [rsp],rax; lea rsi,[rsp]; call qword [rip+0x566cfc]`（`0x453f7e`，GOT 间接）⇒ 事件队列 |
| `0x472d19` | `call 0x3f20f0` | 入队 **`queue_destroy_obstacle_event`**：**`0x3f22b2`** `movabs rax,0x800000000000002a`（tag **42**），载荷 `movups [rsp+0x28],xmm1`（`pos_x,pos_y` 一对）、`movsd [rsp+0x38]`（radius）、`mov [rsp+0x40],ebx`（tick） |

⇒ **死亡块 → 挖洞点** 这个先后是有指令证据的（`0x472ad8 < 0x472d19`），而 `docs/141` §3.1
第 5 条当时写的是"`QueueDestroyObstacle` 的位置回放里没有证据"。
"每个死者发一条 `EffectInfo`"这一步在 x86 侧本轮**没有**逐指令证到（只证到"死亡入队发生在这里、
在任何挖洞/爆炸之前"）；那个结论仍以 `docs/16` §33.6 的 arm64 指令级证据为准。
但表 B 的 `D` 桶判据（批内种类计数**只差**死效块、位置与顺序都不差）说明：
**不是顺序错，是死亡集合错** —— 我方少判一个死者，整块就少一条。
`0x472aad`–`0x472ad8` 还顺带露出落地 tick：`mov ecx,0x88888889 / imul / shr 0x24 / add 0x3c`
（除法魔术数 + **60**）作为 `LifeQueuedDeath` 的 tick 参数。

**(c) 事件判别值表**（`tools/event_tags.py`，全部 72 条自检 72/72）：

```text
life_queued_death=28   function_active=36   function_points=37
queue_destroy_obstacle=42   effect_info=44   skin_graphics_info=51   entity_effect_info=56
```

**(d) 两个"查不动"的点（诚实登记）**

1. **`EffectInfo`（tag 44）的生产者不在任何 `movabs` 站点上**：全 `.text` 的
   `movabs reg, 0x800000000000002C` **只有 2 处**（`0x4bf09b`、`0x4e2dd4`），
   **两处都是消费者**（`cmp qword ptr [rsi], rax`；前者还按 `[rsi+0x20]`＝`effect_type`
   跳表 —— 与 `docs/16` §33.6 的 72 字节布局 `+0x20 = effect_type` 吻合）。
   同一现象也出现在 `function_active_event`（tag 36）：**movabs 站点 0 处**。
   ⇒ 这两个变体的 tag 在这份 x86-64 构建里**不是 movabs 出来的**（可能是
   `mov dword [mem],0x…` + `mov dword [mem+4],0x80000000` 的双半写法，扫过 ±0x40 字节窗口没找到）。
   **等级：未知。** 想接手的下一步：从 `0x4bf09b` 的消费者反推 payload 布局 →
   用 frida 在 `EffectManager::add_effect`（`docs/16` §29.3(b) 的 arm64 `0x4a8c94`）断点抓
   一次真实死亡的 `(pos_x,pos_y)`，就能定 `X` 类到底是哪条算式。
2. **`Y` 类的求值器差异**没落到指令：需要"同一输入下原版 `0x4eb9d0` 的输出"，
   本轮没有活体手段（只能标 **推断**）。

### 24.7 延迟段（**另一根轴**，不进上面的表）

分母不是全部结算批：只有"我方认为有死者/有挖洞"且"回放这一段真有对象"的批才可比。
按 `replaydiff::run_file` 的口径（**逐文件**去重）复算：

| 口径 | 死亡集合 | 多报/漏报/错人 | 挖洞点 |
|---|---|---|---|
| `fire`（任务书基线） | **211 / 379**（55.7%） | 13 / 154 / 1 | **1 / 259**（0.4%） |
| `conv` + `strict` | **362 / 385**（94.0%） | 20 / 3 / 0 | **66 / 291**（22.7%） |

⇒ ① **收敛层的死亡集合已经基本对了**（94%），`fire` 口径的 154 条"漏报"主要是**旧驱动不认模式**
（DiffEq 局按 `Normal` 采样 ⇒ 弹道完全不同 ⇒ 谁都不死）。② **挖洞点是唯一"越修越差"的轴**：
`fire` 口径 1/259 是因为它几乎从不判"撞障碍"（`blocked_at` 少），而 `conv` 会判，判了以后
点位对不上 —— 225 个不命中里，**25 个是末位差（≤1e-12）**，**200 个"我方这个点在该回放
任何一条 `DestroyObstacle` 里都不存在"**（逐文件扫过，不是窗口/时序问题）；
基线几何探针的档位也一致：`dx > 5e-2` 107/259、`dy > 5e-2` 105/259。
**机制未知**：最可能是"挖洞点不是停点"（与 `E1`/`X` 同源的"原版有第二个点"），但没验。

> ✅ **2026-11-07 夜已结案（任务 14，见 [`docs/14-turn.md`](14-turn.md) §12）**：
> **挖洞点 = 爆炸点**，不是"机制问题"而是一根坐标轴——401 份回放里 **503/503** 条
> `DestroyObstacle` 的坐标与同批 `ExplosionEffect` **逐位相同**；我方拿的是**原始文本**采样的
> 停点 y，原版用**修正文本**（常数只印 3 位小数）在 `x = end_x` 处重算的 y。
> 200 例的四类归因是 `a/b/c = 0`、**`d = 200`**（其中 d1 192 = 纯参照点错，d2/d3 各 4 = 掩膜边界差 3~6 点）。
> 修正后命中：真实路径 **1/259 → 134/259**，收敛层 **66/291 → 198/291**。
> 另：本节上文的 `0x88888889 / >>36` 是 **÷30**，不是 ÷60（已在 §25 的代码块处更正）。

### 24.8 如果只修一类：收益排序（**独占批数**，都是可加的）

| 修什么 | 表 A（fire）增 | 表 B（conv）增 | 依据 |
|---|---|---|---|
| **爆炸点 y 求值末位（`Y`）** | +332（`Y2` 那类已修） | **+176** | 判据 `diff ⊆ {pos_y}`：conv 176 批、fire 332 批 |
| x 与 y 一起（`X`+`Y`） | +361 | **+243** | `diff ⊆ {pos_x,pos_y}` |
| 再加 `end_x` 采样点规则（`E1`） | +428 | **+363** | `diff ⊆ {end_x,pos_x,pos_y}` |
| 多发批接线（`S`/`SD`）单独 | 0 | **+26** | `S` 桶的"独占"列（其余要多条一起修） |
| 死效块/死亡集合（`D`）单独 | +132 | +4 | `D` 桶 |

**最大单项 = 爆炸点 y（conv +176 批 ⇒ 内容命中 568 → 744，68.9%）**；
**最大工程性价比 = 多发批接线**（代码已在 `src/game.rs::fire_batch`，只需让对拍/服务端走它，
且它的收益不体现在上表的"独占"里：种类序列 818 → 918 是**先把结构对齐**，
后面 `Y`/`X` 的修法才谈得上）。
两条都修 + `E1` + `X` 的合并预估：**内容命中 ≥ 363 + 568 = 931 / 1080（86.2%）**（**推断**：
按"字段集 ⊆ {end_x,pos_x,pos_y}" 的批数直接相加，没考虑修完以后新暴露的差异）。

### 24.9 复现（两条命令 + 一个校验）

```bash
export CARGO_HOME=<ws>/.reference/cargo-home CARGO_TARGET_DIR=$CARGO_HOME/target
R=<ws>/.reference/replays/gw2-replays-snapshot/replays      # 只读语料，别往里写

# 1) 任务书基线（旧驱动 + 默认口径）→ 表 A，并核对 §24.2 的 7 个数字
cargo run --release --offline --example replay_attrib -- \
    --dir $R --drive fire --policy default --json $OUT/fire.jsonl
python3 tools/bit_exact_attrib.py --json $OUT/fire.jsonl --check-baseline

# 2) 收敛层 → 表 B
cargo run --release --offline --example replay_attrib -- \
    --dir $R --drive conv --policy strict --json $OUT/conv.jsonl
python3 tools/bit_exact_attrib.py --json $OUT/conv.jsonl

# 3) E1 的单批探针（把回放的 end_x/pos_x 落到我方第几个采样点）
cargo run --release --offline --example replay_endx_probe -- --dir $R --file 1782677510 --tail 4
```

`--drive batch`（`Match::fire_batch` 驱动）用来量"只接分组"这一步：种类序列 **918 / 1080**。
`tools/bit_exact_attrib.py` 会打印"加总校验：N == 失败 M ⇒ OK"，并把延迟段两个数复算出来。

### 24.10 未知清单（可直接接手）

| # | 未定 | 缺什么 | 等级 |
|---|---|---|---|
| 1 | `EffectInfo` 的 x 到底怎么算（`X` 类 67 批的根因） | 生产者地址（§24.6(d)1） | **未知** |
| 2 | `Y` 类末位差的具体算式（170 批） | 原版求值器在同样输入上的逐位输出 | **推断**（libm/顺序） |
| 3 | 挖洞点为何与我方停点无关（200 个"点不存在"） | 一条真实挖洞的 `QueueDestroyObstacle` 地址级追踪 | **未知** |
| 4 | `E2` 25 批的真几何分歧（掩膜 ring/fill 还是碰撞判据） | 逐发掩膜对照（`docs/142` §4 的 ring/fill 口径已有工具） | **未知** |
| 5 | `K` 1 批 + `C` 14 批 | 逐例看 | **未知** |

## 25. `Y` 桶（爆炸点 `pos_y`）的形状：170 = 16 格式 + 154 真末位差（2026-11-07，预研）

§24 的表把 `Y` 当成**一类**（"爆炸点 y 末位差"），那是**读法错误**：`Y` 的判据只是
"无 `end_x`/`pos_x` 差、`max|Δpos_y| ≤ 1e-9"，而**相等的值也会有字节差**。
本节把 170 批逐批拆开，并把任务书里三个候选假设逐条否掉。

### 25.1 复现（两条命令 + 一个脚本）

```bash
export CARGO_HOME=<ws>/.cargo-home CARGO_TARGET_DIR=<ws>/.agent-work/<topic>/target
R=<ws>/.agent-work/replay-snapshot             # 401 份定格快照

cargo run --release --offline --example replay_attrib -- \
    --dir $R --drive conv --policy strict --json $OUT/conv.jsonl     # 既有
cargo run --release --offline --example posy_probe  -- \
    --dir $R --json $OUT/posy.jsonl                                  # **本轮新增**
python3 tools/posy_attrib.py --conv $OUT/conv.jsonl --probe $OUT/posy.jsonl
# x 扫描（可选，慢）：再挂 --scan-ulps 2000 --oracle <上游>/graphwar_curve.py
```

`conv.jsonl` 的 `diffs[].ours/observed` 是**报文原文**，可直接解析回 f64（是 `f64` 的最短
往返十进制，所以"文本相同 ⇔ 位相同"）。`posy.jsonl` 才有**我方候选算式**的逐发原值：
停点、点序列尾三点、精确 c 回代、印刷版文本回代、`fx/fx0/c_exact`。

### 25.2 拆分与 ulp 分布（`tools/posy_attrib.py` 的输出）

| 子桶 | 批数 | 模式 | 说明 |
|---|---|---|---|
| **序列化格式**（`值同、字节不同`） | **16** | DiffEq 10 / Normal 6 | `ours='0'` vs `observed='0.0'` |
| **真末位差**（`数值（末位差）`） | **154** | DiffEq 114 / Normal 40 | 见下 |

格式批的原文对（13 种）：`0/0.0`×2、`-9/-9.0`×2、`15/15.0`×2、`7/7.0`、`5/5.0`、`8/8.0`、
`11/11.0`、`-5/-5.0`、`-3/-3.0`、`-13/-13.0`、以及**三条**指数写法
（`0.000…02638231208698597` vs `2.638231208698597e-62`、
`0.0000000000000017763568394002505` vs `1.7763568394002505e-15`、
`0.0000000000000000007606517635480697` vs `7.606517635480697e-19`）。

> **审计更正**（[`152`](152-audit-of-merged-claims.md) §1-C）：本节原写"13 种"但只列了 12 种（15 批），
> 漏掉的就是上面第三条指数写法（`1783343266` 批）。16 批 / 13 种两个数字本身实测无误。

⇒ 回放用的是 **serde_json/ryu** 语义：整数值带 `.0`、小量级用 `e` 记法；
我方 `_scaffold/protocol.rs` 的 `emit_*` 用 `format!("{}")`（Rust `Display`：`0.0→"0"`、
`2.6e-62→66 位小数`）。**这 16 批与求值器无关，是纯打印问题。**

154 批真末位差（`obs` 相对交付值的有符号 ulp 距离取绝对值）：

| ulp | 1 | 2 | 3 | 4 | 8 | 33 | 59 | 64 | 803 | 1797 |
|---|---|---|---|---|---|---|---|---|---|---|
| 批数 | **117** | 23 | 6 | 2 | 1 | 1 | 1 | 1 | 1 | 1 |

**≤4 ulp 合计 148 / 154（96.1%）**；6 个离群批的 `|Δ|` 仍在 1e-9 内（`Y` 的判据），
只是该处 `|y|` 小、1 ulp 很细。136/154 批的停点就是**我方点序列的最后一个点**
（不是被障碍挡住的，而是飞到边界/用完点数）。

### 25.3 三个假设：全部否掉（及"哪一条还剩半条命"）

判据是**逐位**（`bits(a)==bits(b)`），分母 154：

| 候选算式 | 命中批数 |
|---|---|
| 交付值（`eval(我方印刷版文本, 我方停点 x)`） | 0（桶定义如此） |
| 采样器停点 y（精确 c） | 0 |
| 点序列尾 `pts[i_end].y` | 0 |
| 上一个采点 `pts[i_end-1].y` | 0 |
| 下一个采点 `pts[i_end+1].y` | 0 |
| 精确 c 在 `end_x` 回代 `f(x)+c` | 0 |
| 精确 c 在停点 x 回代 | 0 |
| 三项结合 `(f(x)-f(x0))+y0` | 0 |
| 三项结合 `f(x)+(y0-f(x0))` | 0 |
| 印刷版文本回代 `eval(回放 corrected_function, end_x)` | 0 |

* **H1「在最终 x 上重算」：结构成立，但不是这批残差的解释。**
  `_scaffold/diffconv.rs::explosion_point` 已经这么做了（`Mode::Normal` 时拿印刷版文本在
  `stop.0` 求值），它把 `Y` 从"整条几何错"压到"末一位"——本轮**证实了它用的一定是
  ±3 位小数的常数**：`1782927404` 批 `fn = "x"`，`c_exact = 12.980598822920722`、
  印刷值 `12.981`，两者给出的 y 相差 **4.0e-4**，而 `observed` 落在印刷值那一侧 1 ulp 内。
  **但**：`obs` 不等于"我方求值器在**任何** x 上的值"——把 `x=end_x` 在 ±2000 ulp 内扫一遍，
  12 批里有 **6 批任何 x 都算不出 `obs`**（另 4 批落在平坦段，y 对 x 不敏感、多解；
  2 批在 ±1~3 ulp 命中，属巧合）。⇒ 不是"取了另一个 x"。
* **H2「取的是相邻采点的 y」：不成立。** `pts[i-1]` / `pts[i+1]` / `pts[i]` 三个候选
  在 154 批里命中 **0**；`Y` 桶里 `pos_x` 与 `end_x` **逐位相同**也说明几何 x 没错，
  错的只是同一个 x 上的 y。
* **H3「常数 c 的回代顺序」：不成立（且不可能）。** `c+f(x)`、`f(x)+c`、
  `(f(x)-f(x0))+y0`、`f(x)+(y0-f(x0))` 在 IEEE 下**逐位相同**（加法可交换，且这几种写法
  的每一步舍入都一致）；真正能有区别的是"用**哪个** c"（精确 vs 3 位小数），而那已经是
  4e-4 量级、属于 `Y2` 桶。反证：`1782927404` 的 `c≈12.98`，改 1 ulp（1.8e-15）会让结果
  动 ~32 ulp（结果只有 0.416 量级），而观测只动 1 ulp ⇒ 观测**不可能**来自 c 的 ±1 ulp 变化。
  再退一步：`1793874543` 批 `fn = "-13.41"`、`corrected = "-13.41 + 12.994"`，
  两个常数都是 4~5 位十进制、解析精确，`fl(12.994) - fl(13.41)` 由 Sterbenz 引理**精确**
  = `0xbfda9fbe76c8b440`，而 `observed = 0xbfda9fbe76c8b441` —— **任何二操作数 f64 加法
  都到不了那一侧**，所以原版 y 不是"这条文本的一次求值"。

**剩下的唯一解释**：同一个公式、同一个输入，**最后一次舍入不同**（求值器内部的运算顺序
/libm，或 y 在进报文之前多过/少过了一步精度）。证据等级：**排除法 + 观测**；
具体是哪一步 —— **未知**（§24.10 第 2 行的升级版）。

### 25.4 二进制侧：两处 `add_effect` 的 `pos_y` 实参（arm64 2.0.60，逐条 objdump）

* `execute_results` 的 arm64 体 = **`0x4a8558`–`0x4a93d8`**（`sub sp,sp,#0x180` … 末尾 `ret`/冷块）。
  ⚠ **审计更正**（[`152`](152-audit-of-merged-claims.md) §1-B）：本节原写体尾 `0x4a8e50`，实测那只是
  **热路径的 `ret`**；`.symtab` 给的函数尺寸 **3712 B**（与本节 `:1275` 自己写的 3712 B 一致）
  ⇒ 真实体到 `0x4a93d8`，其后紧接 `get_alive_teams`。下面 ★ 那几条指令（`0x4a891c` / `0x4a89ec`）不受影响。
  ⚠ 文档里的 x86 `0x472470`（`game.rs` 注释）在 **2026-08-28 那份 x86 上不是这个函数**：
  `objdump -d --start-address=0x472470` 出来的是 hashbrown 探测循环
  （`pcmpeqb/pmovmskb/tzcnt`）。这个地址是旧版本的，**别再照着用**。
* **① `0x4a89ec`（恒 `ExplosionEffect`）**

  ```text
  004a8810  ldp  w23, w20, [x8, #8]      ; 记录里 Vec 的 ptr（低/高 32 位）
  004a88e8  orr  x22, x23, x20, lsl #32  ; 拼回 ptr
  004a88ec  ldr  x20, [x8, #0x10]        ; len
  004a8918  add  x8, x22, x20, lsl #4    ; ptr + len*16
  004a891c  ldp  d9, d8, [x8, #-0x10]    ; ★ pos = points[len-1]（16 字节/点）
  004a89b4  stp  d9, d8, [x29, #-0x60]   ; effect{pos_x=d9, pos_y=d8}
  004a89b8  sturb wzr, [x29, #-0x4c]     ; effect_type = 0（ExplosionEffect）
  004a89a4  ldxr/stxr w24, [x28]         ; id = 全局计数器自增
  004a89e8  mov  x8, #0x2c / 0x4a89ec movk x8, #0x8000, lsl #48   ; 枚举 tag
  004a8a08  bl   #0x3d7a40               ; 推入报文
  ```

  ⇒ **`pos_y` = 点数组最后一个元素的第二个 f64**，中间**没有**求值器调用。
  这个数组在 `[x19+0x30..0x40]`（`mem::take` 取走，见 `0x4a878c`–`0x4a879c`），
  **填充者在别处**（这一步本轮没定位，见 §25.5 第 2 条）。
* **② `0x4a8cb0`（照抄出生登记）**

  ```text
  004a8b38  bl   #0x3b7f58               ; 取 Option<(f64,f64)>（sret 在 x8=sp+0x78）
  004a8b44  ldr  x8, [sp, #0x78] / cmp x8, #1   ; 判别式 == 1 才用
  004a8b50  ldp  d8, d9, [sp, #0x80]     ; ★ (pos_x=d8, pos_y=d9)
  004a8c50  ldurb w21, [x13, #-4]        ; effect_type = entity_effect_map[id]
  004a8c68  ldxr/stxr w23, [x28]         ; id 仍来自同一个计数器
  004a8c78  stp  d8, d9, [x29, #-0x60]   ; 4a8c7c sturb w21,[x29,#-0x4c] = 类型
  ```

  `0x3b7f58` 的形状：`ldaxr/stxr` 的 `Once` 闸门 + 之后的哈希查找，返回
  `Option<(f64,f64)>`（35 个调用点）—— 与"按 entity id 查位置表"一致（**推断**：
  是否就是 `PlanePosManager::get_position` 未证）。
* 另一条旁证：`x9 == 1` 的分支在 `0x4a889c` 调 `0x426094`，实参是
  `fmov d0,#0.5`（半径）+ `tick+0x3c`，形状与 `queue_destroy_obstacle` 一致（**推断**）。

**把 25.2/25.4 合起来**：`pos_x` 与 `end_x` **逐位相同**、`pos_y` 与"印刷版修正文本在
`end_x` 处的值"只差 1 ulp ⇒ 那个点数组最可能是**客户端按印刷版 `corrected_function`
重画出来的曲线**（x 用采样器的 x，y 重算），而不是采样器内部用精确 c 算的点。
这与 `docs/142` §4.2 的 529/739 逐位命中是同一件事。**推断**，未证。

### 25.5 给任务 09 的三条可执行结论

1. **先拿 16 批（确定、便宜、与会话几何无关）**：把 `_scaffold/protocol.rs` 的 `emit_*`
   里 f64 的写法换成 ryu/serde_json 语义（整数值补 `.0`、改用 `e` 记法、最短往返）。
   这 16 批**不是**求值器问题，改打印就归零。
2. **154 批的 1 ulp 不要再去改"取哪个点/常数顺序"**（H1 变体 / H2 / H3 都已被数据否掉，
   见 25.3）。要看的是**点数组的填充者**：
   `[x19+0x30..0x40]` 这个 `Vec<{tag, Vec<(f64,f64)>}>` 是谁 push 的、
   以及 `0x3b7f58` 到底读的是哪张表。**下一个探针**：在 arm64 上找
   "写 `+0x30/+0x38/+0x40` 三连"的站点（本轮只找到 `mem::take` 的**清空**写入，
   没有找到填充）。
3. 复现口径别退：`tools/posy_attrib.py` 的 0/154 表是**回归护栏**——谁要是把
   `explosion_point` 改成取相邻点或改结合顺序，这张表会立刻显示"命中数变成十几"，
   那是撞上了平坦段，不是修好了。

---

---

## 26. 爆炸点 `pos_y`：坐标来源逐指令定死，末位差拆成两桶（2026-11-07，`dispatch/09`）

> 任务书：[`dispatch/09-fx-explosion-y.md`](../dispatch/09-fx-explosion-y.md)。归因桶 = §24.4 的 `Y`
> （收敛层 170 批 / 33.2%）＋ `Y2`（6 批）。本节**推翻任务书的两个前提**（见 §26.1）并给出落地修正。
>
> **反汇编口径**：逐指令证据一律引 **arm64 Android 2.0.60 `libgraphwarrust.so`**
> （`.reference/steam/android-2.0.60/`，**未 strip**，`readelf -sW` 直接给符号，地址 = `st_value`）。
> x86-64 那份（`2026-08-28/`）是 stripped，同一段代码在 `0x472470`（`execute_results`）里，
> 指令形态一致但符号不可读，所以本节不引它的地址。

### 26.1 一句话（两个前提被推翻）

1. **爆炸点不是"停点经过某种变换"，就是点序列的最后一个元素**：
   `EffectInfo(Explosion)` 的 `pos_x/pos_y` = `pts[len-1].x/.y`，**没有投影、没有取整、没有翻转**
   （arm64 `0x4a8918`/`0x4a891c` → `0x4a89b4`/`0x4a89f0`，见 §26.2）。⇒ 任务书"y 经过了某次变换"**不成立**。
2. **这 170 批不是一个机制**：176 个爆炸点里 **124 个是 DiffEq（Ode）模式的 RK4 积分值**、
   **52 个是 Normal 模式的求值值**（按批前最后一个 `FunctionModeInfo` 判，见 §26.7）。
   旧 §24.5③ 把它当成单一"求值器末位"问题，所以怎么找都对不上。
3. 顺手推翻旧 §24.5③ 的一句话：**`pos_y` 的差不是同向的**。
   176 条 `pos_y` diff 里 `ours<obs` **105**、`ours>obs` **55**、逐位相同 **16**
   （旧文"符号全部同向，154/154 我方 > 回放"复现不出来；应该是把两个桶混在一起算的）。

**落地结果**：`fire` 驱动内容命中 **21 → 324 / 1080**（种类序列 **818 → 818**，结构一条没动），
`batch` 驱动 **21 → 341**；延迟段挖洞点 **1/259 → 138/258**（副作用，见 §26.6）。

### 26.2 坐标来源 = 点序列最后一个元素（**观测**，逐指令）

`GraphwarGameStateManager::execute_results`（arm64 **`0x4a8558`**，3712 B）里那段"逐项处理开火队列"：

```text
004a87ec  ldr   x27,[x26],#0x20      ; 队列项 tag（步长 0x20）
004a8800-004a8818                    ; tag - 0x8000000000000001 → 索引 0/1/2
004a88e8  orr   x22,x23,x20,lsl#32   ; 索引 0：指针由两个 u32 半拼回（pts）
004a88ec  ldr   x20,[x8,#0x10]       ; n = 点数
004a8918  add   x8,x22,x20,lsl#4     ; &pts[n]（元素 16 B = 一对 f64）
004a891c  ldp   d9,d8,[x8,#-0x10]    ; ★ d9 = pts[n-1].x，d8 = pts[n-1].y
004a895c  bl    set_function_active_tick        ; FunctionActive
004a898c-004a8998                    ; start_tick = active_tick + n/30（除 30 用 0x88888889 魔术数）
                                     ; ⚠️ 2026-11-07 夜更正（任务 14，docs/14 §12.4）：
                                     ; 0x88888889 >> 36 = ÷30，不是 ÷60；0..20000 逐个验过。
                                     ; 原写"n/60"是读错了除数的位宽。
004a89b4  stp   d9,d8,[x29,#-0x60]   ; ★ effect 记录的 (pos_x,pos_y)
004a89b8  sturb wzr,[x29,#-0x4c]     ; effect_type = 0 = ExplosionEffect
004a89e8  mov   x8,#0x2c
004a89ec  movk  x8,#0x8000,lsl#48    ; tag 44 = EffectInfo（任务书给的地址指的就是这里）
004a89f0  stp   d9,d8,[x29,#-0x58]   ; ★ 事件载荷用**同一对** d9/d8
```

⇒ ① 事件里的 `pos_x/pos_y` 与 effect 表里的坐标**同源同值**；② `start_tick` 里出现 `n/60`
说明这个数组就是**整条弹道的采样点**（≈4700 点 × 60 fps），不是"只有端点"的表。
同一函数里的第二处生产点（死亡）用的是另一条路：`0x4a8b40 bl get_position`（`GraphwarPlanePosManager::get_position`）
→ `0x4a8b50 ldp d8,d9,[sp,#0x80]` → `0x4a8cb4 stp d8,d9,[x29,#-0x58]`，与 `0x4a8c50 ldurb w21,[x13,#-4]`
（`entity_effect_map` 查表，查不到就 `0x4a8c38` 跳过）—— 与 `docs/16` §29.3(b) 一致。

### 26.3 真值轨迹用的是**印出来的三位小数修正常数**（**观测** + 数值侧独立验证）

* `GraphwarShotCalculator::calculate_corrected_function`（**`0x3b79a8`**）= 原文 + 补齐未闭合的 `)`
  （`0x28`/`0x29` 计数，`0x3b7ba8` 那段写 `)`）+ 偏移串；
* `GraphwarShotCalculator::calculate_function_offset_str`（**`0x3b7c70`**）逐条：

```text
003b7cf8  bl    MathFunction::new               ; 解析函数文本
003b7cfc  ldr   d0,[x20]                        ; x0
003b7d0c  fcsel d0, -x0, x0, flip!=0            ; flip ? -x0 : x0
003b7d20  bl    MathFunction::evaluate_recursive; f(∓x0)
003b7d24/28     ldr d1,[x20,#8] ; fsub d0,d1,d0 ; c = y0 - f(∓x0)
003b7d2c  fcmp  d0,#0
003b7d38-003b7d60  fabs + format(0x82fa28) + 补 " - "(strh 0x2d20)  ; c < 0
003b7dbc-003b7df8  format         + 补 " + "(strh 0x2b20)          ; c >= 0
```

  ⇒ 文本里的常量是 **3 位小数**（`" - 6.861"`）。三位这个数字在 `.rodata` 的格式描述符里没有逐字节解出来
  （`0x179468` 是 `format` 的 pieces/args 结构），但**回放的 `corrected_function` 逐字节吻合 1079/1080 批**，
  这一条是**观测**级；我们 `graphwar_shot_calculator::correction_text` 的 `{:.3}` 与此一致。
* 调用序（**观测**）：`GraphwarGameStateManager::fire_functions` 内先 **`0x4a7458 bl calculate_corrected_function`**，
  再 **`0x4a798c bl calculate_shot_results`**，而 `calculate_shot_results` 在 **`0x3b5b0c`** 调采样器
  `calculate_function` ⇒ **采样发生在"修正文本"造出来之后**。
* **推断**（缺的证据写明）：采样器拿到的那棵树是从**修正文本**建出来的
  （`MathFunction::new` 的两处调用在 `GraphWarFunctionManager::set_function` 的 `0x488b28`/`0x488b74`），
  但"树 → 采样器"这一跳没有点对点证到（`calculate_shot_results` 内部零 `evaluate` 调用，树是从函数管理器取的，
  取的是哪一棵没逐指令确认）。
* **数值侧独立支持**（52 个 Normal 样本，`pairs.json` 见 §26.9 的复现手法）：
  「全精度 `c` 平移」的 y 有 **47/52** 与回放差 **1e-6~5e-4**；
  「三位小数文本回代」的 y 有 **5/52 逐位相同、40/52 恰差 1 ulp**。⇒ 常量确实是三位小数那一版。

### 26.4 采样器与 RK4 的具体算术（**观测**，本轮新解）

`GraphwarShotCalculator::calculate_function`（arm64 **`0x3b5374`**，1020 B）：

| 项 | 地址 | 内容 |
|---|---|---|
| 点数预算 | `0x3b53cc`/`0x3b53ec`/`0x3b53f8` | `mov x8,#0x4069000000000000`（**200.0**）`fdiv`(step) → `fcvtzs` → `max(n,1)` |
| 首点 | `0x3b53e4` | `stp d11,d9` = `(x0,y0)` |
| 求值用的 x | `0x3b53f0` | `fcsel d8, -x0, x0, flip` |
| 边界 | `0x3b5560`/`0x3b5578` | `fmov #25.0` / `fmov #15.0`（`|x|>25` 或 `|y|>15` 就停） |
| Normal 分支 | `0x3b55c8`–`0x3b5618` | 逐点 `evaluate_recursive`（一个中点求值 + 一个复检求值，`0x3b5618`/`0x3b5664`） |
| Ode 分支 | `0x3b5408`/`0x3b54f4`–`0x3b5518` | RK4（下面是它的**确切**结合顺序） |

```text
h2  = mid - x                       ; 0x3b547c，mid = (x + (x+step)) * 0.5
y2  = y + (k1*h2)*0.5               ; 0x3b5480 → 0x3b5488
x2  = x + h2*0.5                    ; 0x3b5484 → 0x3b548c
y3  = y + (k2*h2)*0.5               ; 0x3b54ac → 0x3b54b4
y4  = y + h2*k3                     ; 0x3b54d4 → 0x3b54e0
sum = ((k1 + 2*k2) + 2*k3) + k4     ; 0x3b54f8..0x3b5510，**左结合**（`fadd d2,d3,d2` 的顺序）
y'  = y + (h2/6) * sum              ; 0x3b5500 先算 h2/6，再 0x3b5514 乘，0x3b5518 加
```

⇒ 与我们的 `src/graphwar_path_calculator.rs` 的 `Mode::Ode` 分支**逐条同序**（含 `(h/6)` 先算、
`k1 + 2k2 + 2k3 + k4` 左结合、`(k1*h)*0.5`）。**所以 DiffEq 那 124 个不是公式错**，
残差只能来自 `evaluate_recursive`（右端函数求值）的末位 —— 与 §26.8 的未知项同源。

### 26.5 落地修正（`src/game.rs::fire_functions`）

旧实现把**原文**交给 `settle::resolve_owned`，后者内部按 `shifted_tokens`（**全精度** `c`）平移；
真值是"**先造修正文本、再让采样器对那串文本求值**"（§26.3）。改成：

```rust
let corrected = graphwar_shot_calculator::corrected_text(text, x0, y0, flip);
let points = graphwar_shot_calculator::shot_points(&corrected, x0, y0, SERVER_STEP, flip, Mode::Normal, /*apply_correction=*/false);
settle::resolve_points_owned(&mask, &targets, &points, Some(owner));
```

只动了这一处（`execute_results` 一个字没改：它本来就用"最后一个采样点"当爆炸点，与 §26.2 一致）。

### 26.6 before / after（**同一份 401 回放 / 1080 批快照**）

| 驱动 | 内容命中 | 消息种类序列命中 | 备注 |
|---|---|---|---|
| `fire`（任务书基线） before | **21 / 1080** | **818 / 1080** | 复现 §24.2 的 7 个数全部一致 |
| `fire` after | **324 / 1080** | **818 / 1080** | 结构零变化 ⇒ 这次只修了"值" |
| `batch` before / after | 21 → **341** | 918 → **915** | 4 批少判死者（见 §26.8），1 批新增 |

> ⚠ **2026-11-07 复核（§29）**：这 −3 **不是**「致死判据在阈值上」，而是**括号不平衡**让弹道退化成 1 个点（ 4 批全是「一个死者都没打死」）。已修：`batch` **919 / 内容 346**、`fire` 819 / 325，0 批变差。下面 §26.8 第 3 行的「未知」因此可以撤掉 —— 见 §29.7 的新清单。

| `conv`（收敛层） before / after | 568 → **568** | 945 → 945 | **不动**：它的爆炸点在 `_scaffold/diffconv.rs`，不在本任务的允许改文件里 |

延迟段（另一根轴）：死亡集合 **211/379 → 209/379**；挖洞点 **1/259 → 138/258**（53.5%）。
挖洞点这一项是**副作用**：位置来自同一条弹道，轨迹一改就大面积命中——`docs/06` §24.7 里
"挖洞点为何与我方停点无关（200 例点位不存在）"的问题因此从 200 例缩到 ~120 例（**观测**，未逐例复盘）。

修正后的归因表（`fire`，失败 756 批）：`C` 368 / `D` 132 / `S` 91 / **`Y` 52** / `E1` 46 / `SD` 39 / `X` 28。
**`Y2`（332 批）整桶清零**，新出现的 `Y` 52 批就是 §26.7 那 52 个 ±1 ulp 的爆炸点。

### 26.7 差值形态分组（176 个爆炸点 / 170 批，`conv` 收敛层）

| 分组 | 条数 | 判据 | 机制 |
|---|---|---|---|
| **DiffEq（Ode）** | **124** | 批前最后一个 `FunctionModeInfo` = `DiffEqFunction` | `pos_y` = RK4 积分值，**不能回代**；§26.4 的公式与我们同序 ⇒ 残差在求值器末位 |
| **Normal** | **52** | 同上 = `NormalFunction` | `pos_y` = `f(end_x)`；用"修正文本回代"重算：**5 逐位相同、40 恰 1 ulp**（26 偏低 / 14 偏高）、7 例特例（NaN、极大常量） |

`pos_y` 的 ulp 直方图（`ours - observed`，176 条 diff 逐条）：`-1`:72、`-2`:22、`+1`:45、
`-3`:5、`0`:16、`-4`:2、`+2`:1、`+3`:1、`|u|≥8`:12。
**符号不同向**（105 负 / 55 正 / 16 零）⇒ 排除"同一条式子的系统性偏差"，也排除"我方一律取整偏大"。

5 个代表样本（`end_x` 已确认 `obs_x == end_x` 逐位相同）：

| 回放 #批 | 模式 | 函数原文 | `end_x` | 我方 `pos_y` | 回放 `pos_y` | Δulp |
|---|---|---|---|---|---|---|
| `1782676871` #3 | Normal | `1/(6-x)`（文本 `… - 6.861`） | `-5.9542382857185725` | `14.991328211529705` | `14.991328211529703` | +1 |
| `1782927404` #0 | Normal | `x`（文本 `x + 12.981`） | `2.0172735861302553` | `14.998273586130255` | `14.998273586130257` | −1 |
| `1793874543` #25 | Normal | `sin(x)`（`+ 11.437`） | `0.45076283631408426` | `11.001347698764203` | `11.001347698764205` | −1 |
| `1793874543` #46 | Normal | `(x*-0.26)`（`+ 10.734`） | `1.3457628363140568` | `11.083898337441655` | `11.083898337441656` | −1 |
| `1783342447` #2 | DiffEq | `tan(-2x)` | `22.775784224310193` | `9.187486940569265` | `9.187486940569263` | +1 |

**规律**：同一 `x`（逐位相同）下差 1 ulp，且函数里只有四则运算的样本（`x + 12.981`）也差 1 ulp
⇒ 与 libm 无关；差量在 `c` 的 1 ulp 量级，指向"常量/求值末位"而不是"取了另一个点"。
任务书里的示例 `我方 6.11832353472057 / 回放 6.118000000000002` 属于 `Y2`（三位小数舍入，
已由 §26.5 修掉），**不是** `Y` 这一桶。

### 26.8 未知清单（接管时从这里开始）

| # | 未定 | 缺什么 | 等级 |
|---|---|---|---|
| 1 | 剩下 **52 批 ±1 ulp**（Normal）的确切来源 | 原版 `MathFunction::evaluate_recursive`（`0x4452d4`）在这些**确切输入**上的输出。unicorn 可跑，但要按 72 B 的树布局喂进去（本轮没做，别猜 libm） | **未知** |
| 2 | DiffEq 那 124 条的末位 | 同上（右端函数求值的末位会累积进积分值） | **未知** |
| 3 | `batch` 驱动 4 批种类序列由对变错（`1782675382#0`、`1782675586#0`、`1782676398#0`、`1783429569#3`） | — | **已解 + 已修（§29）**：括号不平衡 ⇒ `parse` 截断 ⇒ 弹道退化成 1 个点；不是「致死判据在阈值上」（那 4 批是一个死者都没打死，不是差一个）。 修后 919 / 346 |
| 4 | `conv` 驱动不涨 | 它的爆炸点算法在 `_scaffold/diffconv.rs`（实验脚手架），要接得改那里、或让 conv 走 `game.rs` | 范围问题，非未知 |

### 26.9 复现（干净树，逐条命令）

```bash
export CARGO_HOME=<ws>/.reference/cargo-home
export CARGO_TARGET_DIR=<ws>/.agent-work/cg/target09     # 单独一行；并行时必须私有
R=<ws>/.reference/replays/gw2-replays-snapshot/replays
OUT=<ws>/.agent-work/out                                # /tmp 是逐命令 tmpfs，别放中间产物

# 1) before（先 `git stash` 掉本次改动）→ 21 / 818
cargo run --release --offline --example replay_attrib -- --dir $R --drive fire --policy default --json $OUT/fire_before.jsonl
python3 tools/bit_exact_attrib.py --json $OUT/fire_before.jsonl --check-baseline   # 7 个数全部 OK

# 2) after → 324 / 818
cargo run --release --offline --example replay_attrib -- --dir $R --drive fire --policy default --json $OUT/fire_after.jsonl
python3 -c "import json;R=[json.loads(l) for l in open('$OUT/fire_after.jsonl')];R=[r for r in R if r.get('batch',-1)>=0];print(sum(r['content_ok'] for r in R), sum(r['kind_seq_ok'] for r in R))"
python3 tools/bit_exact_attrib.py --json $OUT/fire_after.jsonl                    # 分类表；Y2 应为 0

# 3) batch / conv 两个驱动（before 用 stash，after 用工作树）
cargo run --release --offline --example replay_attrib -- --dir $R --drive batch --policy default --json $OUT/batch.jsonl
cargo run --release --offline --example replay_attrib -- --dir $R --drive conv  --policy strict  --json $OUT/conv.jsonl
python3 tools/bit_exact_attrib.py --json $OUT/conv.jsonl | head -12               # Y 应仍是 170 批

# 4) 回归
cargo test --offline                       # 1512 passed / 0 failed
python3 tools/coverage_matrix.py --check   # OK
```

§26.7 的"按模式分组 + ulp 直方图"由一次性脚本从 `$OUT/conv.jsonl` + 回放
（`gzip` 的 JSONL，`FunctionInfo.corrected_function` / `FunctionPoints.end_x` / `EffectInfo.pos_y`）
算出来，口径写在上表里，**没有**新的入库工具。

## 27. `FunctionPoints.end_x` 的取值点：逐指令定死 = `pts[len-1]`（2026-11-07，`dispatch/11`）

> 任务书：[`dispatch/11-endx-sample-point.md`](../dispatch/11-endx-sample-point.md)（P1，任务书口径"失败 95 批"，
> 那是 §26 修前的 `conv` 归因；**当前 `fire` 基线里这一桶是 `E1` 46 批**，见 §27.4）。
> 本节**推翻任务书的前提**"`end_x` 取 `pts[n-2]`"，并给出一次**被实测否决的改法**的完整数据。
>
> **反汇编口径**同 §26：arm64 Android 2.0.60 `libgraphwarrust.so`（未 strip，地址 = `st_value`）。

### 27.1 一句话

1. **`end_x` = 送进 `set_function_points` 那条点序列的最后一个元素 `pts[len-1].x`**，
   不是倒数第二个（arm64 **`0x489420`/`0x489428`**，§27.2）。⇒ 任务书"取 `pts[n-2]`"**不成立**。
2. 同一条点序列、同一个 `len` 也喂给 `execute_results` 的爆炸点（`0x4a88ec`/`0x4a8918`/`0x4a891c`）
   ⇒ **在报文构造这一层，`end_x` 与 `EffectInfo(Explosion).pos_x` 必然是同一点**（§26.2 的结论不变）。
3. 回放里这两条报文确实**有时差一个采样点**（1290 发里 240 发，§27.3），但差别在**点序列本身**
   （哪一步停），不在"取哪个下标"。按"退一格"去改报文，整局对拍 **324 → 229**、`E1` **46 → 162**
   （§27.4，实测，已回退）。
4. **采样器一个字没改**（`docs/04` 的 24/24 逐位基线未动）。

### 27.2 逐指令：`end_x` 就是 `pts[len-1]`（**观测**）

生产点 `GraphwarFunctionManager::set_function_points`（arm64 **`0x489278`**，884 B）：

```text
004893a8  cmp   w8,#0x2                      ; 状态判断
00489400  ldr   x8,[x19,#0x10]               ; len = 传入点序列的长度
00489404  cbz   x8, 0x48945c                 ; len==0 ⇒ 一条报文都不发
00489414  ldr   x9,[x19,#0x8]                ; ptr
00489420  add   x8,x9,x8,lsl#4               ; &pts[len]
00489424  ldr   q0,[x9]                      ; ★ start = pts[0]（16 B = 一对 f64）
00489428  ldur  d1,[x8,#-0x10]               ; ★ end_x = pts[len-1].x（**最后一个**）
0048942c  mov   x8,#0x25                     ; tag 37 = FunctionPoints
00489434  movk  x8,#0x8000,lsl#48
00489430  stur  q0,[sp,#0x10]                ; 载荷 +0x08 = start_x/start_y
0048943c  str   d1,[sp,#0x20]                ; 载荷 +0x18 = end_x
00489438  str   w22,[sp,#0x28]               ; 载荷 +0x20 = owner_id
0048944c  bl    GraphwarEventQueue::add_event
```

同文件全 `.text` 里 tag 37 的**生产者只有两处**（`0x48942c`、`0x4abad4`
`handle_function_points` 的转发路径），本地路径就是上面这一处。

这条点序列是谁给的 —— `GraphwarGameStateManager::fire_functions`（**`0x4a7160`**）：

```text
004a7a2c  ldr   x24,[x8,#0x10]     ; len = 队列项 +0x10
004a7a38  lsl   x20,x24,#4         ; 分配 len*16
004a7a50  ldr   x23,[x8,#0x8]      ; ptr = 队列项 +0x8
004a7ac8/4a7af0                    ; 逐点 clone（16 B/点）
004a7b28  stp   x24,x21,[x29,#-0x68]  ; 传参结构 (len, ptr, cap=len)
004a7b3c  bl    set_function_points
```

同一批的爆炸点走的是**同一个队列项、同一个 `len`**（`execute_results` `0x4a8558`）：

```text
004a87ec  ldr   x27,[x26],#0x20    ; 队列项 tag（步长 0x20）
004a88e8  orr   x22,x23,x20,lsl#32 ; 变体 0：pts 指针由两半 u32 拼回（项 +0x8）
004a88ec  ldr   x20,[x8,#0x10]     ; n = 项 +0x10（与上面 `0x4a7a2c` 同一字段）
004a8918  add   x8,x22,x20,lsl#4   ; &pts[n]
004a891c  ldp   d9,d8,[x8,#-0x10]  ; ★ pts[n-1] → effect 记录 + 载荷（同 §26.2）
```

⇒ **两处取的是同一个下标**。所以"差一个"不可能来自"取 `pts[n-2]`"。

### 27.3 那 240 发的差从哪来（**观测**，回放自身的两条报文对比）

先把"回放的 `end_x`"和"同一发的爆炸点"直接比（不经过我方实现），全语料 1290 发：

| 关系 | 条数 |
|---|---|
| `end_x` 与 `pos_x` **逐位相同** | **1050**（81.4%） |
| 差 `< 5e-4` | 136（含 1e-16 量级的 ulp 差） |
| 差 `∈ [5e-4, 5e-3)` | 25 |
| 差 `≥ 5e-3`（≈ **一个采样步**） | 79 |

差值的量级都是**采样步的二进制分数**（`15/16`、`3/4`、`1/64`…；5 条例外是实际步长本身不是 0.01 造成的），
即"**相邻两个采样点**"的距离，而不是别的量。

再看这 240 发在我方点列上的位置（用 `examples/replay_endx_probe.rs` 的点列，1290 发）：

| 回放 `end_x` 落在 | 条数 | 说明 |
|---|---|---|
| 我方 `pts[M-1]`（**末点**，逐位） | **773** | 我方现在的取值就是对的 |
| 我方 `pts[M-2]`（倒数第二，逐位） | **103** | "差一个"的全在这一桶 |
| 更靠前（我方点列没截断，真值被障碍截断） | 361 + 53 | 探针不带掩膜，属预期 |
| **能用的边界停止样本** | 876 | 773 / 103 |

那 103 发里，回放爆炸点 `pos_x` 与我方 `pts[M-1].x` **只差 1~800 ulp**、`pos_y` 常逐位相同
（是"同一个点的另一次计算"，不是另一个点）。⇒ 真相的形状是：

* 绝大多数发：点序列末元素 = 停点，`end_x == pos_x`（逐位）；
* 少数发：点序列**少一个元素**，`end_x` 退一格，而停点是**另算的一个点**（所以差 ulp）。

"什么时候会少那一个元素"**没有定论**（§27.6）。

### 27.4 被实测否决的改法：`blocked` 时 `end_x` 退一格

`E1` 的 46 批里 **7 批是 `blocked`（撞障碍）**，且这 7 批的形状一致：

```text
回放 FunctionPoints.end_x = 我方「阻挡点之前那个采样点」（逐位）
回放 EffectInfo.pos_x     = 我方「阻挡点」（差 1~8 ulp）
```

照这个改（`src/game.rs::execute_results` 里 `blocked ⇒ 取阻挡点前一点`）后实测：

| 量 | before | after（退一格） |
|---|---|---|
| `fire` 内容命中 | **324 / 1080** | **229 / 1080**（−95） |
| `E1` | 46 | **162** |
| 逐批对照 | — | **丢 95 批 / 得 0 批**，95 批**全部**是 `blocked` |

⇒ 对绝大多数 `blocked` 批次，回放的 `end_x` **就是阻挡点本身**（那 7 批是另一回事）。
**这个改法已回退，入库的是"不改" + 一个把它钉死的测试**
（`tests/live_hits.rs::end_x_and_explosion_x_are_the_same_sample_point`：
自由飞行与撞障碍两种情况都断言 `end_x == pos_x`，且 `end_x` = 阻挡点本身 / 点列末点）。

### 27.5 顺手更正 §26.2 的两处（**观测**）

1. **`start_tick = active_tick + n/60` 应为 `n/30`**：
   `0x4a898c/0x4a8990` 的魔术数 `0x88888889` 配 `0x4a8998` 的 `lsr #36` 是 **÷30**
   （`0x88888889 / 2^36 = 1/30`；若按 ÷60 算，`n = 60` 会得 2 而不是 1）。
   数值侧：1290 发里 **895 发** 满足 `爆炸 start_tick − FunctionActive.active_tick == 点列长 // 30`，
   其余的是被障碍截断（点列比真实长）—— 按 ÷60 一条都对不上。
2. `FunctionActive.active_tick = [函数管理器 +0x58] + 0x3c`（`0x4a8940 add w24,w21,#0x3c`，60 tick = 1 s），
   不是 §26.2 暗示的"当前 tick"。这条只影响 tick 数值，不影响坐标。

### 27.6 未知清单（接管时从这里开始）

| # | 未定 | 缺什么 | 等级 |
|---|---|---|---|
| 1 | 那 103 发（≈12% 的边界停止）为什么点序列**少一个元素** | 边界/障碍的"停点"是**另算**的（差 ulp 说明是重算而非拷贝），而点序列少一个 —— 需要 `calculate_shot_results`（`0x3b5770`）里"停点"的完整写法（本次只确认了它不来自队列项 `+0x8/+0x10`） | **未知** |
| 2 | 那 7 个 `blocked` 的 `E1` 为什么与另外 95 个 `blocked` 行为相反 | 同上：可能是"落在采样点上 vs 落在两点之间"的碰撞粒度（差值分别是整步 / 整步 / 3.9e-4…）；要 `is_occupied` 那侧的逐指令对照（掩膜 1 px = 0.05 单位） | **未知** |
| 3 | 逐个点的末位（±1 ulp） | 与 §26.8 #1 同源：`MathFunction::evaluate_recursive`（`0x4452d4`）在这些确切输入上的输出 | **未知** |

### 27.7 复现（干净树）

```bash
export CARGO_HOME=<ws>/.reference/cargo-home
export CARGO_TARGET_DIR=<ws>/.agent-work/cg/target11     # 私有 target
R=<ws>/.reference/replays/gw2-replays-snapshot/replays
OUT=<ws>/.agent-work/out11                               # /tmp 是逐命令 tmpfs

# 1) 基线（= §26 之后的口径）：324 / 1080，E1 46
cargo run --release --offline --example replay_attrib -- --dir $R --drive fire --policy default --json $OUT/fire.jsonl
python3 tools/bit_exact_attrib.py --json $OUT/fire.jsonl

# 2) 单发点列探针（§27.3 的"end_x 落在我方第几个点"就是这么来的）
cargo run --release --offline --example replay_endx_probe -- --dir $R --file 1782641987 --tail 6
```

§27.3 的两张表由一次性脚本算：回放按 `gzip` 的 JSONL 顺序取 `FunctionPoints.end_x` 与
`EffectInfo(ExplosionEffect).pos_x` 逐发配对（同一份回放里两者条数相等才计），
点列侧用 `replay_endx_probe` 的输出（每发打印 `点 M` + 末尾 6 个点 + `end_x 命中 idx`），
按 `idx == M-1 / M-2` 分桶。**没有**新的入库工具；入库的只有上面那个测试。

---

## 28. 多发批结构接线（2026-11-07 第二轮）：**逐发窗口 → 一次事务**

> 任务书：[`dispatch/10-multi-shot-wiring.md`](../dispatch/10-multi-shot-wiring.md)。
> 上一轮（§24.4）把它记成 "`S` 123 批 + `SD` 7 批 / 接线即得 / 内容预计 ≥594"；
> 本节把它**接上**，并给出三个数（总 / 多发批 / 单发批）。
> 证据档位沿用仓库约定；本节所有比例的分母都是**结算批**（1080），不是文件。

### 28.1 一句话与三个数

> 多发批的差别**不是"少了一条报文"，而是"窗口开了 N 个"**：我们原来**每发各开一个**
> `CalculatingResults…ExecutingResults` 窗口（一批 3N 条 `GameStateInfo`），
> 真回放是**一个**窗口（3 条）、前缀**按类型连排**、后缀**每发一对**。
> 接上之后：种类序列 **946 → 1074 / 1080**、多发批 **0/130 → 128/130**、
> **单发批 946/950 一条没变**；内容命中 569 → 595。

| 指标 | 接线前（逐发 `fire`） | 接线后（按批 `fire_batch`） | 差 |
|---|---|---|---|
| 消息种类序列命中（1080 批） | **946** (87.6%) | **1074** (99.4%) | **+128** |
| ├ 多发批（≥2 发，**130** 批） | **0** (0.0%) | **128** (98.5%) | **+128** |
| └ 单发批（**950** 批） | **946** (99.6%) | **946** (99.6%) | **0** |
| 内容命中（1080 批） | **569** (52.7%) | **595** (55.1%) | **+26** |
| ├ 多发批 | 0 | 26 (20.0%) | +26 |
| └ 单发批 | 569 (59.9%) | 569 (59.9%) | **0** |
| 完全一致的回放（有结算批的 282 份） | 71 | **83** | +12 |
| 失败批（`tools/bit_exact_attrib.py`） | 511 | **485** | −26 |

**逐批对照：变好 128 批、变差 0 批**（`--drive both` 会把"变差"的批直接列出来，为空）。

> 表里两列的语料与代码**完全相同**，只差驱动方式（这是"before/after"唯一合法的读法）。
> 任务书里的 before **945 / 内容 568** 是**上一轮代码**的数；本轮另修了一处
> `explosion_point` 的非有限值回退（§25.7，1 批），所以两列都比任务书各 +1。

### 28.2 那条修正的语义：**合并窗口 + 重排**，不新增任何消息种类

规则来源是归档里的同时回合制三段流程 [`docs/archive/141-sim-turns.md`](archive/141-sim-turns.md)
§1–§3（**观测**：130 个真多发包 **130/130** 对上分组与顺序）。落到报文上是三处差别：

```text
我方（接线前，N = 3，每发一个窗口）
  Calc | Finfo₁ | Fpts₁ | Sync | [死效₁] | FActive₁ | Expl₁ | Exec
  Calc | Finfo₂ | Fpts₂ | Sync | [死效₂] | FActive₂ | Expl₂ | Exec
  Calc | Finfo₃ | Fpts₃ | Sync | [死效₃] | FActive₃ | Expl₃ | Exec
真回放（N = 3，一个事务）
  Calc | Finfo₁ Finfo₂ Finfo₃ | Fpts₁ Fpts₂ Fpts₃ | Sync | [死效块] |
  FActive₁ Expl₁ FActive₂ Expl₂ FActive₃ Expl₃ | Exec
```

| # | 改的是 | 判定写在哪（源码） | 证据 |
|---|---|---|---|
| 1 | **窗口数 3N → 3**：`CalculatingResults` / `SynchronizingResults` / `ExecutingResults` 各**一条** | `fire_batch` 只在这三个位置各 push 一次 | 观测（130/130；归因表里 `S` 123 + `SD` 7 = **130 = 全部多发批**，它们的种类计数都多出 `GameStateInfo`） |
| 2 | **前缀按类型分组**：N 条 `FunctionInfo` 连排、再 N 条 `FunctionPoints` 连排，两组顺序都 = 批内发序 | 两个 `for` 循环分别遍历 `shots`（发序 = 回放里 `FunctionPoints` 的顺序） | 观测（130/130） |
| 3 | **后缀每发一对**：`FunctionActive(owner_i)` 紧跟 `EffectInfo(Explosion)`，交替 N 次 | 一个 `zip` 循环 | 观测（130/130） |

**"补哪一条"——本规则一条都不补。** 批内消息**种类计数**除 `GameStateInfo` 外逐条相等
（`examples/replay_attrib.rs` 导出的 `bm_equal` / `kind_multiset_equal` 就是这条判据）。
任务书里"回放多一条 `EffectInfo/SparkEffect`"是**另一根轴**：那条由**死亡集合**与
**出生登记表**（`docs/16` §37：皮肤 `bot_` ⇒ `AshEffect`、靶子 ⇒ `SparkEffect`）决定，
不是分组规则；§25.6 里剩下的 6 批全是它。

另外两条与"分组"同批落地的规则（都来自 `docs/archive/141` §2）：

* **几何并行**：每一发都对着**批开始那一刻**的士兵表算（152/152，观测）；谁死都不改变别人的弹道 ⇒
  接线后多发批的 `killed` 不再依赖"上一发把谁打死了"；
* **死亡块按飞行时间排**：排序键 `(弧长, 发序号, 士兵 id)`，弧长取自
  `graphwar_shot_calculator::settle::ShotOutcome.hits[].arc`；同一士兵**只留最早到达**的那一发。
  同批所有弹丸 `active_tick` 相同（13/13 回合）⇒ 弧长序 = 到达时刻序，**不需要**知道弹速。
  真并列怎么判仍是 **未知**（`docs/archive/141` §5 第 5 条），这里的三元组兜底只是确定性。

> 档位说明：**"规则是什么"是观测**（130/130 真回放）；**"我们照它实现了"是实现，不构成复原证据**——
> 判定仍然只看真回放（本节所有数字都是与真回放逐批比出来的）。

### 28.3 接在哪：**只接驱动**，一行游戏逻辑没动

| 文件 | 改了什么 |
|---|---|
| `src/_scaffold/diffconv.rs` | 新增 `ConvPredictor::fire_batch(&[ShotInput], tick)`（**整批**报文）；`fire(...)` 改成它的 **N = 1 特例**；`run_file` 的预测路径由"逐发 `fire` + 拼接"换成"**每批一次** `fire_batch`"；`Cause::MultiShot` 的判据收紧成"种类计数**确实**差 `GameStateInfo`"（否则"结构已对、值还错"的多发批会被误报成结构问题）；`explosion_point` 的 NaN 判据放宽成"非有限值"（§25.7） |
| `examples/batch_conv.rs` | **新增**（既有工具**一行没改**）：`--drive fire\|batch\|both`，同一份语料、同一个 `compare_batch`、同一个 `ConvPredictor`，**只换驱动**；JSONL 与 `replay_attrib` 同格式，可直接喂 `tools/bit_exact_attrib.py` |
| `docs/coverage-matrix.tsv`、`docs/149-coverage-matrix.md` | 生成物重跑（`src/` 变了：2257 → 2265 条 `fn`） |

**刻意没动的两处**（写清理由，免得下一个人以为漏了）：

1. **`replaydiff::Predictor::predict`（逐发 `Match::fire`）保持原样。**
   它是 `tools/bit_exact_attrib.py --check-baseline` 钉住的**冻结基线**
   （种类序列 818、内容 21、延迟段 211/379 与 1/259）。把它也改成按批，
   §24.2 那 7 个数会一起变，那条验收就再也核不了对。所以仓库里现在**两条驱动并存**：
   `Predictor`（逐发）= §24 的历史基线，只在 `--drive fire` 出现；
   `ConvPredictor::fire_batch`（按批）= 当前最好的驱动，`diff_conv` / `batch_conv` 用它。
2. **`src/game.rs` 的结算逻辑**（`Match::fire_batch` / `execute_results`）**一字未动**——
   那是任务 09 的地盘，而且它已经按 `docs/archive/141` §3 实现了同一套分组（`--drive batch` 走的就是它）。

`examples/replay_attrib.rs` 本轮**只许加不许改**（任务书允许清单），所以它里面的 `--drive conv`
仍是逐发的；新增的 `batch_conv --drive fire` 与它**逐字节相同**（§25.4 的第一条校验）。

### 28.4 量化与复现（干净树可重跑）

```bash
export CARGO_HOME=<ws>/.reference/cargo-home
export CARGO_TARGET_DIR=<本 worktree 的私有 target>   # 共用 target 会跨 worktree 串味
export R=<ws>/.reference/replays/gw2-replays-snapshot/replays        # 只读语料
OUT=<ws>/.agent-work/<会话>/out                                      # `/tmp` 是逐命令 tmpfs

# ① 一次跑两种驱动 + 逐批对照（"变好/变差"各多少批，"变差"的会被列出来）
cargo run --release --offline --example batch_conv -- \
    --dir $R --drive both --policy strict --json $OUT/batch.jsonl

# ② 只跑一边：①=接线前、②=接线后
cargo run --release --offline --example batch_conv -- --dir $R --drive fire  --policy strict --json $OUT/fire.jsonl
cargo run --release --offline --example batch_conv -- --dir $R --drive batch --policy strict --json $OUT/batch.jsonl

# ③ 接线后的归因分桶（`S` / `SD` / `K` 三桶应当清零）
python3 tools/bit_exact_attrib.py --json $OUT/batch.jsonl

# ④ 冻结基线仍然自洽（这条**必须**还是 818 / 21 / 211 / 1）
cargo run --release --offline --example replay_attrib -- --dir $R --drive fire --policy default --json $OUT/base.jsonl
python3 tools/bit_exact_attrib.py --json $OUT/base.jsonl --check-baseline
```

**两条口径校验**（"before/after 来自同一套记账"，不靠人工看）：

1. `batch_conv --drive fire --policy strict` 的 JSONL 与
   `replay_attrib --drive conv --policy strict` 的产物**逐字节相同**（`diff` 为空）——
   本轮的 `fire` 就是把 `fire_batch` 按 N = 1 调一次，账本代码是 `replay_attrib` 的逐行副本；
2. `--drive both` 的逐批对照直接按 `(kind_seq_ok, content_ok)` 比较**同一个批**，
   列出"变差"的批（本次 0 个）。

**回归**：

```text
cargo test --offline                 1516 passed / 0 failed   （基线 1512 + 本节新增 4 条 diffconv 单元测试）
python3 tools/coverage_matrix.py --check   OK（527/527 可归属、78 模块；src 106 文件 2265 fn）
```

新增的 4 条单元测试（`src/_scaffold/diffconv.rs` 的 `mod tests`）：

| 测试 | 钉住的东西 |
|---|---|
| `single_shot_is_byte_identical_to_fire_batch` | `fire` ≡ `fire_batch(N=1)`，报文**逐字节**相同 |
| `two_shot_batch_is_grouped_by_type` | 两发一批的完整种类序列 = §25.2 那张图；负样本：逐发窗口会有 6 条 `GameStateInfo` |
| `death_block_follows_flight_time` | 同一士兵一批只死一次、死者坐标来自**批开始**的快照、状态在报文之后落地 |
| `non_finite_explosion_point_falls_back_to_stop` | `±inf` 不写进报文（§25.7） |

### 28.5 要不要"按条件接"？—— **不需要**，判据是"N = 1 恒等"

任务书提醒："如果接线后某些单发批反而变差，说明规则不该无条件应用"。实测**没有**变差的批，
而且这不是运气 —— 这条规则对 N = 1 是**结构上的恒等**：

1. **代码上恒等**：`fire(...)` 现在只做一件事——把一发包成 `ShotInput` 交给 `fire_batch`，
   取回唯一那个 `ShotOutcome`。两条路不可能分家（`single_shot_is_byte_identical_to_fire_batch`）。
2. **三条规则在 N = 1 时退化**：窗口 1 = 1；前缀 N = 1 无分组；后缀 1 对；
   死亡块只有一发，`(弧长, 发序号, 士兵 id)` 在**一发内**就是 `settle` 的"经过顺序"（同一发内弧长单调不减）。
3. **语料上恒等**：950 个单发批的 `kind_seq_ok` / `content_ok` **逐批**前后相同（946 / 569），
   逐批对照"变差 0 批、变好 128 批"。
4. **几何那一条也不影响单发批**：几何并行的差别只在于"用哪一刻的士兵表"，
   一发时批开始那一刻就是唯一那一刻。

⇒ 唯一值得设条件的情形（"单发也会变"）**不存在**，所以不设开关。
**判据留在代码里而不是留在文档里**：`fire` 不允许有自己的实现，只能调 `fire_batch`；
谁以后想加"只对多发生效"的开关，必须先推翻上面 4 条。

### 28.6 剩下的 **6 批**种类序列失败：**没有一个是分组错**

| 现象 | 批 | 成因 | 归到哪根轴 |
|---|---|---|---|
| 单发，我方**少**一条死效（回放 `LifeInfo` 说 1316 死了、我方 `killed=[]`） | 1（`1782641048` #0） | 死亡集合**漏报** | 几何/掩膜（§24.5 ①④） |
| 单发，我方**多**一条 `DeathEffect` | 2（`1782673653` #1、`1782927404` #1） | 死亡集合**多报** | 同上 |
| 单发，我方**多**一条 `AshEffect` | 1（`1787655342` #1） | 同上（`bot_` 皮肤） | 同上 |
| 多发 3 发，我方多一条 `DeathEffect` | 1（`1782675099` #0） | 同上 | 同上 |
| 多发 2 发，我方多一条 `DeathEffect` | 1（`1782928065` #1） | 同上 | 同上 |

⇒ 分组这条轴的**残差为 0**：130 个多发批的**骨架** 128 个全对，另 2 个错在"死亡集合"上、不在分组上。
多发批里还有 102 个"种类序列已对、内容还错"的批 —— 那是 §24.5 的 `Y` / `E1` / `X` / `E2` 四条几何轴，
`docs/141` §4.6 早就把这三条排成"下一步的收益"，本节没有动它们。

### 28.7 顺手修的一处：`explosion_point` 不写非有限值

`1787655980` 第 1 批原来是这样坏的：我方那条爆炸报文长成

```json
{"EffectInfo":{"effect_id":237,"pos_x":-19.210168751748913,"pos_y":-inf,"effect_type":"ExplosionEffect","start_tick":18776}}
```

`-inf` **不是合法 JSON** ⇒ `Msg::parse_wire` 整条丢掉 ⇒ 比较器眼里我方"少一条 `ExplosionEffect`"，
这一批的种类序列白挂。真因是 `explosion_point` 只挡了 `NaN`、没挡 `±inf`
（回放那边这个值是有限的 **11.36**，我方回代却溢出）。

改法：`!y.is_finite()` 时回退到停点（`pos_y ≈ 11.3607`）。**这不是真值**——真值是 11.3608 那一档进
`Y` 桶——只是"不要发出一个不是数字的坐标"。影响正好 1 批：
种类序列 **945 → 946**（逐发）/ **1073 → 1074**（按批），内容命中 **568 → 569** / **594 → 595**
（本节 §25.1 表里两列都是**修完之后**的数，所以两边都比任务书的 before 各 +1）。

> 顺带暴露一个**既有工具的口径洞**：`examples/replay_attrib.rs` 的 conv 分支把解析不了的报文
> **静默丢掉、不计数**（`invalid` 只填 `Predictor` 那条路），所以这条坏报文在旧工具里是隐形的。
> 新工具照实计数；`--drive fire` 与旧工具仍**逐字节相同**，因为修完之后两边都没有非法报文了。

### 28.8 归因表的前后（`tools/bit_exact_attrib.py`，互斥分桶）

| 桶 | 接线前 | 接线后 | 说明 |
|---|---|---|---|
| `Y` 爆炸点 y 末位差 | 170 | **196** | 结构不再挡着，这些批的 y 差露出来了 |
| `S` 多发批结构 | **123** | **0** | 本节修掉 |
| `E1` `end_x` 差一个采样点 | 95 | **127** | 同上（露出来） |
| `X` 爆炸点 x 末位差 | 67 | **91** | 同上 |
| `E2` `end_x` 真分歧 | 25 | **39** | 同上 |
| `C` `corrected_function` 文本 | 14 | **18** | 同上 |
| `SD` 多发批结构 + 死效块 | **7** | **0** | 本节修掉 |
| `Y2` 爆炸点 y 中等差 | 6 | 8 | 同上 |
| `D` 死效块数量 | 4 | 6 | 同上 |
| **合计（失败批）** | **511** | **485** | −26 |

> `K`（未定性）这一档**两列都是 0**：上一版归因表里那 1 批 `K` 就是 §25.7 的 `-inf`，
> 修掉之后它进了 `Y` 桶。所以 `485` 与 `511` 的差 **26 全部**是"从 `S`/`SD` 变成了完全一致"，
> 没有一处是靠重新分桶挪出来的。

> 读法：**总失败批只降了 26，不是 128**。因为"结构错"会把**同一个批**的字段差异全挡住
> （种类序列一错就 `return`，不再做块内比对），所以 128 个批只是**从 `S`/`SD` 挪到了别的桶**，
> 真正变成"完全一致"的是其中 **26** 批（= §24.4 里 `S` 桶的"独占 26"）。
> 这也解释了为什么别的桶全都变大：**账本没变，只是盖子掀开了**。

### 28.9 本轮**未**确定 / 未做的（诚实清单）

| # | 事项 | 缺什么 | 档 |
|---|---|---|---|
| 1 | 任务书"`diffconv` 里已经修好"这句**不成立**：`diffconv.rs` 里只有 `Cause::MultiShot` 这个**归因标签**，没有分组实现；分组在 `src/game.rs::Match::fire_batch`（`docs/archive/141` §3） | — | 观测（本轮读了两个文件并对照） |
| 2 | 任务书"种类序列 818 → 918"是**另一根轴**（`Match::fire` → `Match::fire_batch`，默认口径），本轮复跑**逐数复现**（818 / 918）；收敛层这一轴是 946 → 1074 | — | 观测 |
| 3 | 两发**真并列**到达同一士兵时谁赢 | 真回放里没有并列（最小间隔 4.0 tick，`docs/archive/141` §5） | **未知**（实现用 `(弧长, 发序号, id)` 兜底） |
| 4 | `EffectInfo(爆炸)` 的 x 与 `end_x` 为什么是两个数（`X` 91 批） | 生产者地址（§24.6(d)1） | **未知** |
| 5 | §25.7 那个 `-inf` 的**真值**（回放是 11.3608 档） | 原版求值器在同样输入上的逐位输出 | **未知**（当前回退到停点，只是不发非法 JSON） |
| 6 | `AshEffect` 与 `DeathEffect` 混排时"发哪一个" | `EffectInfo` 发射路径复核（`docs/16` §37 已解"按皮肤"，未解"同批混排"） | **推断**（`docs/archive/141` §3.3 记的仍是未知） |

---

## 29. 一个数字的**口径**与一处**真回归**：`1074` 与 918 → 915（2026-11-07 复核）

> 起因：下游（`CG` 分支，第二轮）的验收表里写了"种类序列 **1074 / 1080**"，而复核方在**合并后的
> master** 上用 `replay_attrib --drive batch` 量到 **915 / 1080**。本节回答两件事：
> **1074 是哪根轴上的数**、**918 → 915 的那 −3 是谁、是不是 bug**。
> 结论先说：1074 **不是虚报**（另一根轴，可逐数复现）；−3 **是真 bug**，已在 `src/game.rs` 修掉，
> 修后同一份快照 **919 / 1080**（比 09 之前还多 1）。

### 29.1 一句话结论（三条）

1. **1074 = 收敛层（`ConvPredictor::fire_batch`）轴，不是 `Match::fire_batch` 轴。**
   复现命令是 `batch_conv --drive batch --policy strict`；同一份 401 回放我拿到
   **1074 / 1080**、多发批 **128 / 130**、单发批 **946 / 950**、内容 **595 / 1080** —— 与下游逐个相等。
   而 `replay_attrib --drive batch --policy default`（真值 `src/game.rs::Match::fire_batch`）是
   **918 / 915**。两者**语料相同、分母相同、驱动不同**，所以"1074 vs 918"不是同一个量掉了 156，
   而是**表头写错了轴**（`docs/06` §24.4 那张三驱动表、§28.9 第 2 条本来就分开列）。
   ⇒ **判定：口径不同（表头错配），不是虚报**，也没有"未提交的中间态"达到过 1074
   （`git log --all` / `git fsck` 的悬空提交里没有任何 multi-shot 工作痕迹）。
2. **918 → 915 发生在 `50246fc`（09 修正文本采样），`9818c21`（14 挖洞点）一个数都没动。**
   而且不是"掉 3 批"：**4 批由对变错 + 1 批由错变对 = 净 −3**（§29.3）。
3. **那 4 批不是"致死判据在阈值上"（§26.8 第 3 行当时的猜测），而是新代码踩到一处老 bug：**
   函数原文**括号不平衡**时，我方 `math_function::parse` 会**就地截断**，追加在后面的修正常数
   被整段丢掉 ⇒ 曲线不经过 `(x0, y0)` ⇒ 采样器第一步收敛不了 ⇒ **弹道退化成 1 个点、一个人都没打死**。
   **已修**（只改 `src/game.rs`，两处调用 + 三个私有函数）：**919 / 1080，内容 346，0 批变差**（§29.5–29.6）。

### 29.2 逐点测量（同一份快照 / 同一个驱动 / 逐点私有 `CARGO_TARGET_DIR`）

```bash
export CARGO_HOME=<ws>/.cargo-home CARGO_TARGET_DIR=<本 worktree 私有>
R=<ws>/.agent-work/replay-snapshot          # 401 份定格快照
cargo run --release --offline --example replay_attrib -- \
    --dir $R --drive batch --policy default --json $OUT/<点>.jsonl
python3 -c "import json,sys;R=[json.loads(l) for l in open(sys.argv[1])];R=[r for r in R if r.get('batch',-1)>=0];\
print('kind_seq',sum(r['kind_seq_ok'] for r in R),'/',len(R),' content',sum(r['content_ok'] for r in R))" $OUT/<点>.jsonl
```

| 提交 | 是什么 | kind_seq / 1080 | 内容 | 多发批（130） | 单发批（950） |
|---|---|---|---|---|---|
| `690dd24` | 10 multi-shot-wiring 尖端 | **918** | 21 | 100 | 818 |
| `50246fc` | 09 fx-explosion-y 尖端 | **915** | 341 | 97 | 818 |
| `9818c21` | 14 hole-axis 尖端 ⚠️**不含 09**（与它同级，父提交都是 `f0ea3139`） | **918** | 21 | 100 | 818 |
| `origin/CG` `415fd27d` | CG 尖端 | **915** | 341 | 97 | 818 |
| （参考）`38f97d7f` | 合并 10 之后 = `690dd24` 的内容 | 918 | 21 | 100 | 818 |
| （参考）`607be667` | 合并 09 之后 | **915** | 341 | 97 | 818 |
| （参考）`965ae25d` / `5c7cb47e` | 合并 13 / 合并 14 之后 | 915 / 915 | 341 | 97 | 818 |
| （参考）`master` `8755db9e` | 下游 16 提交合并后 | 915 | 341 | 97 | 818 |
| **本节修复后（同一快照）** | `src/game.rs` 一处 | **919** | **346** | **100** | **819** |

⚠️ **三个尖端是兄弟不是父子**：`690dd24`、`50246fc`、`9818c21` 的父提交**都是** `f0ea3139`，
所以"14 的尖端"里**没有** 09 的改动 —— 单独测 `9818c21` 得到 918 / 21 是**正常的**，
它只说明"14 对这两个数零影响"。要按时间线读，看合并点（`607be667` 之后一直是 915，直到本节修复）。

> 又一次的"两根轴"提醒：这张表**全部**是 `Match::fire_batch`（`--drive batch`，默认口径）。
> 同一时刻的收敛层轴是 1074（§29.1），两者的多发批数是 **100/130 vs 128/130** ——
> 下游报告里的"多发批 0/130 → 128/130"同样属于收敛层轴。

### 29.3 −3 的真相：4 批掉、1 批涨

逐批对照（`kind_seq_ok` 由 true 变 false / 由 false 变 true）：

| 回放 `#批` | 发数 | `EffectInfo/DeathEffect` 条数 `690dd24 → master`（真值） | 掉的是什么 | 括号 |
|---|---|---|---|---|
| `1782675382#0` | 2 | **1 → 0**（真值 2） | 死者 184208（本来已经判对） | 原文**多 1 个** `)` |
| `1782675586#0` | 2 | **3 → 0**（真值 2） | 死者 184209 / 184208 / 184743 全丢 | 原文**多 1 个** `)` |
| `1782676398#0` | 2 | **2 → 0**（真值 2） | 死者 184208 / 184209 全丢 | 原文**缺 1 个** `)` |
| `1783429569#3` | 1 | **2 → 0**（真值 2） | 死者 366693 / 367034 全丢 | 原文**缺 1 个** `)` |
| `1782641048#0` | 1 | 0 → 0（**+1：由错变对**） | 09 修好的那 1 批 | 平衡 |

**首个 diff 四批完全相同**：`<批内第 4 块的种类>` —— 我方块序列是
`GameStateInfo | FunctionInfo | FunctionPoints | GameStateInfo | **FunctionActive** | …`，
真值是 `… | GameStateInfo | **EffectInfo/DeathEffect** | FunctionActive | …`。
也就是**整个死者块消失**，不是"顺序错"、也不是"多一条少一条"。
（`kind_seq_ok` 比的是**折叠后的块序列**，所以 `1782675586#0` 那个"我们发了 3 条、真值 2 条"
在块层面仍然算对 —— 它在内容层面另记。）

⇒ 用一句话说：**09 把"平移在 token 里"改成"平移在文本里"，于是我们的解析器第一次真的
去解析那串文本；而它见到多余的 `)` 就截断、见到缺的 `)` 就把常数塞进括号里，两条都会
让弹道退化成 1 个点。** 09 本身没有算错：它修好的那 1 批（`1782641048#0`）在修完这一处后
仍然是好的，所以修复是"919"而不是回到"918"。

### 29.4 证据：真值解析器**不**在多余的 `)` 处截断（逐位）

三件事叠起来才够格写"逐位"：

1. **真值文本 = 我们生成的文本**（多余 `)` 的 2 批）：
   回放 `FunctionInfo.corrected_function` 与 `graphwar_shot_calculator::corrected_text(fn, x0, y0, flip)`
   **逐字节相同**（`1782675382#0`、`1782675586#0`）⇒ 文本不是本次的锅。
   缺 `)` 的 2 批则**不同**：真值是 `fn + ")" + " ± c.ddd"`，我们的是 `fn + " ± c.ddd"`
   （全语料 11 条缺括号，真值 11/11 补了右括号，与 `docs/142` §2.2 一致）。
2. **把回放自己的 `corrected_function` 喂给我方采样器**：多余 `)` 的批仍然是
   `toks=12, pts=1` —— 截断发生在 `math_function::parse`，不在文本生成。
3. **端点逐位相同**（决定性）：
   * `1782675382#0` 第 1 发（`fn = "14/(1+exp(100*(x-19)^2)))+24/(1+exp(100*(x-24)^2))"`，
     `x0 = 22.908325288230134`，`c = -4.323`）：把**多余的 `)` 去掉**再采样，端点
     `(-24.99612783676475, -4.323)` 与回放 `EffectInfo(ExplosionEffect).pos` **逐位相同**；
     照原文采样则 `pts = 1`（不动）。
   * `1782676398#0` 第 1 发（`fn` 缺 1 个 `)`，`c = 0.820`）：照真值文本补齐后端点
     `(-24.99641030734599, -1.6800000000000002)` 与回放 `pos` **逐位相同**。
   * 同一个批里括号**平衡**的那一发（`1782675382#0` 第 2 发）在我们原实现下就已经逐位对了
     （`(-0.07041023390040979, 3.604177854360204)`）—— 反证问题只在括号。

复现（两个一次性探针，随本节入库）：

```bash
# 逐发把「原文 + 全精度 c」与「修正文本」两条弹道并排 + 到每个士兵的最近距离
cargo run --release --offline --example kseq_probe -- --dir $R --file 1782675382 --batch 0
# 回放自己那条 corrected_function，以及各种候选文本的端点
cargo run --release --offline --example kseq_text  -- --dir $R --file 1782676398 --batch 0
```

全语料计数（401 份回放，`FunctionInfo.corrected_function` 非 null 的 **1291 发**）：

| 括号形态 | 发数 | 我方旧文本 | 后果 |
|---|---|---|---|
| 平衡 | **1272** | 与真值逐字节相同 | 正常 |
| **多 `)`** | **8** | 与真值逐字节相同 | `parse` 截断 ⇒ 常数丢失 ⇒ 弹道 1 个点 |
| **缺 `)`** | **11** | 少一个 `)`（真值补了） | 常数被塞进括号里 ⇒ 弹道 1 个点（其中 2 发是 DiffEq：真值既不补也不加后缀，§29.7 第 3 条） |

### 29.5 改了什么（`src/game.rs`，一处调用 + 三个私有函数）

| 位置 | 改法 | 为什么 |
|---|---|---|
| `balanced_texts`（新） | 扫一遍括号，返回 `(只补不删的串, 补齐+去掉多余 `)` 的串)` | 真值**发**的是前者，我们的 `parse` 能吃的是后者 |
| `corrected_function_text`（新） | `只补不删 + " ± c.ddd"`，`c` 由**补完括号的树**算 | 与回放 `corrected_function` 同一条规则（顺带修掉 `C` 桶里"缺括号"那部分） |
| `sampling_function_text`（新） | `补齐+去多余 + " ± c.ddd"` | 采样器实际吃的串 |
| `Match::fire_functions` | 采样用 `sampling_function_text`（原来是 `corrected_text`） | 弹道不再退化成 1 个点 |
| `Match::execute_results` 的 `FunctionInfo` | 发 `corrected_function_text`（原来是 `corrected_text`） | 缺括号时补上 `)`（多余 `)` 时**一个字符都不动**） |

`math_function::parse` **没动**：它是 331 条真值夹具钉住的解析器，截断行为要改属于另一件事
（§29.7 第 1 条）。本节的做法是"**把真解析器能吃的文本喂给我方解析器**"，
不是"让我们解析器的行为符合真值"—— 后者仍然**未知**（真解析器遇到多余 `)` 是"跳过"还是
"忽略整段尾部"，本节的证据只能否掉"截断"，不能定死是哪一种）。

### 29.6 after 数字（同一份 401 回放 / 1080 批，`--policy default`）

| 驱动 | 指标 | before（master `8755db9e`） | **after（本节）** | 差 |
|---|---|---|---|---|
| `batch`（`Match::fire_batch`） | 种类序列 | 915 | **919** | **+4** |
| | 内容命中 | 341 | **346** | **+5** |
| | ├ 多发批（130） | 97 | **100** | +3 |
| | └ 单发批（950） | 818 | **819** | +1 |
| `fire`（逐发 `Match::fire`） | 种类序列 | 818 | **819** | +1 |
| | 内容命中 | 324 | **325** | +1 |
| `conv`（收敛层，`batch_conv --drive batch --policy strict`） | 种类序列 / 内容 | 1074 / 595 | **1074 / 595** | 0（它不走 `game.rs`） |

**逐批：变好 4 批、变差 0 批**（`kind_seq`：`1782675382#0` / `1782675586#0` / `1782676398#0` /
`1783429569#3`）；**内容变好 5 批、变差 0 批** —— 除了上面 4 批里的 2 批，
另 3 批（`1782675099#1`、`1782927966#1`、`1783430575#2`）**种类序列本来就对**，
是 `corrected_function` 少了右括号 + 弹道退化，只在内容层面露头。

修完这 4 批的现场：

| 回放 `#批` | 修后 |
|---|---|
| `1782675382#0` | **完全一致**（0 条 diff，爆炸点逐位） |
| `1782676398#0` | **完全一致**（0 条 diff） |
| `1782675586#0` | 种类序列对；剩 1 条 —— 2 发批里我们多判死者 184743（与本节无关，见 §29.7 第 2 条） |
| `1783429569#3` | 种类序列对；剩 1 条 —— `EffectInfo.pos_x` 差 **1 ulp**（`Y` 类，§25.2） |

**回归**：`cargo test --offline` **1520 passed / 0 failed**（基线 1519 + 本节新增 1 条，见下）；
`python3 tools/coverage_matrix.py --check` **OK**（527 / 527 可归属、78 模块）。

新增的那 1 条单元测试钉住的就是这次的两条规则 + 一个**真值端点**：

| 测试（`src/game.rs` 的 `mod tests`） | 钉住的东西 |
|---|---|
| `bracket_balancing_splits_emitted_and_sampled_text` | 缺 `)` ⇒ 补到平衡、两个串相同；多 `)` ⇒ **发出去的串一字不动**、采样串去掉那一个；以及 `1782675382#0` 第 1 发的采样端点 `(-24.99612783676475, -4.323)` **逐位**等于回放 `EffectInfo(ExplosionEffect).pos` |


### 29.7 仍未确定的（诚实清单）

| # | 事项 | 缺什么 | 档 |
|---|---|---|---|
| 1 | **真解析器**遇到没有配对的 `)` 到底怎么处理（跳过？忽略整段尾部？） | 二进制 `math_function` 那条解析路径的逐指令；本节只否掉"截断"（两条端点逐位） | **未知**（我方 `parse` 仍是截断，本节只绕开它） |
| 2 | `1782675586#0` 我们多发 1 个死者（184743，`d = 0.42 < radius 0.5`） | 真值的致死半径/判据（现在全体士兵都按 0.5 算） | **未知** |
| 3 | 2 发 DiffEq 的 `corrected_function` 真值**既不补括号也不加后缀**，而 `game.rs` 恒按 Normal 加后缀 | `Match::fire` 的"按模式分"（收敛层 `diffconv` 已做，`game.rs` 没做） | **未知**（`docs/06` §28.9 第 4 条的同一件事） |
| 4 | `hole_point` 里同一个模式（多余的 `)` 会让"按印刷文本回代"丢掉常数） | — | 实测**改掉它整份 401 回放 0 行差异**（md5 相同），所以**没改**（无证据的改动不做）；等有独立证据再动 |

## 30. 求值器与二进制**逐位一致**；154 批 1 ulp 残差**不在**求值器里（2026-11-07，`dispatch/15`）

> 任务书：`dispatch/15-eval-unicorn.md`（P0）——"用 unicorn 对拍 `evaluate_recursive`，解掉最后 1 ulp"。
> 本节回答三件事：**对拍台怎么搭**、**求值器到底差在哪**（答案：不再差）、
> **那 154 批 1 ulp 是什么**（答案：不是求值器，且给出反证）。
> 顺带把 §25.2/§25.5 的 16 批"纯序列化格式差"打到 **0**。
> ⚠️ 任务书说"最大 §27，你用 §28"；本提交时文件里 §28/§29 已被并行任务占了，所以本节用 **§30**
> （同号冲突按任务书约定由协调者合并）。

### 30.1 一句话结论（六条）

1. **对拍台搭好了、可复跑**：`tools/eval_oracle.py`（unicorn 直调 `0x4ed670` 解析 + `0x4eb9d0` 求值）
   ＋ `examples/eval_oracle.rs`（把我方 `parse`/`eval` 的 token 与结果逐发导出）。
   逐发比较是**端到端**的（真解析器 + 真求值器，不吃我方 token），比"喂同一串 token"更硬。
2. **求值器有 3 处真偏差，已修**（`src/math_function.rs`），全部是"照搬上游 python 参照实现"留下的：
   `log(0)`/`ln(0)` 给 `NaN`（真二进制 **-inf**）、`0/0` 与 `sqrt(-1)` 的 NaN **符号位**不同。
   修完 `--sweep` 的 17 条边角语义**逐位一致**。
3. **`fixtures/eval.tsv` 有 3 行真值是错的**（`log(0)`、`ln(0)`、`ln(-2)`），根因是
   `analysis/emu.py` 把数学导入映射到 **python 的 `math`**（定义域外**抛异常**→ 兜成 NaN），
   不是 libm（**返回值**）。已改 `tools/gen_fixtures.py`（换成 `ctypes` 调 `libm.so.6`）并重生成；
   `path.tsv` / `parse.tsv` **一字未变**（正对照：说明或旧夹具的其它部分本来就是对的）。
4. **真解析器对"无主的 `)`"是跳过、不是截断**（`docs/06` §29.7 第 1 行那个"未知"），
   我方 `parse` 已按它**修好**（`src/math_function.rs`，`skip_stray_close`）：
   全量 1290 发里"多右括号"的 **8 发**从"解析截断"变成"解析一致"（7 发逐 tag 相同，
   剩 1 发只差一处结合、**两边求值都是 NaN**）。§29.7 那张表因此可以划掉第 1 行。
5. **那 154 批 1 ulp **不是**求值器**：残留里挑的 **8/8** 条最小复现，真二进制给的值与我方**逐位相同**，
   而回放观测值比**两者**都差 **±1 ulp**。另外给出**不可能性证明**：`x + 35.910` 这类"一次加法"里，
   把 x 与常数各扫 ±3000 ulp 都到不了观测值（§30.5）。
   ⇒ 任务书"H1'求值器最后一次舍入不同'"这条前提，**被证伪**（与第二轮另外 5 条同一种性质）。
6. **16 批序列化格式差 → 0**：`emit_*` 的 f64 打印改成 ryu 语义（`{:?}` + `e-5` 那一档补定点），
   `posy_attrib.py` 的"纯序列化格式"从 **16 批降到 0 批**；`fire` 内容命中 **325 → 331**、
   `batch` **346 → 355**、`batch_conv` 内容 **595 → 615**（种类序列 **1074 不退**）。

### 30.2 对拍台（地址级事实先钉死）

`evaluate_recursive` 的**签名与四个跳转目标**（`objdump -d` 于
`steam/2026-08-28/libgraphwarrust.so`，sha256 `9a538115…`）：

```text
0x4eb9d0  push r15/r14/rbx; sub rsp,0x40
          movapd xmm4,xmm0          ; x
          mov    rax,[rsi]          ; i = *pos
          cmp    rax,[rdi+0x10]     ; i vs len      （rdi = &Vec<Token>，字段 {cap@0,ptr@8,len@0x10}）
          jae    0x4ebb78           ; 越界 -> 0.0
          mov    rcx,[rdi+0x8]      ; tokens
          mov    rax,[rcx+rdx]      ; tag（16 字节/token：{tag:u64, value:f64}）
0x4eba0b  bt 0x1fe20,rax          ; 一元算子集合 {5,9,10,11,12,13,14,15,16}
0x4eba16  bt 0xe0007,rax          ; 叶子集合   {0,1,2,17,18,19}
          ... else 两次递归（4eba44 / 4eba66）= 二元 {3,4,6,7,8}
0x4ebab7  lea rcx,0x1c3c6c; movsxd rax,[rcx+rax*4]; add rax,rcx; jmp rax   ; 18 项跳转表
```

跳转表 `0x1c3c6c`（18 项 `int32` 相对偏移）解出来的 **每个 tag 的真实指令**：

| tag | 记号 | 目标 | 实际运算 | 与我方实现 |
|---|---|---|---|---|
| 0 / 1 / 2 | `x` / `y` / `y'` | `0x4ebb78` / `0x4ebacb` / `0x4ebb2f` | 直接返回 `xmm4/xmm1/xmm2` | 一致 |
| 3 / 4 / 6 / 7 | `+ - * /` | `0x4ebb48` / `0x4ebb0d` / `0x4ebb70` / `0x4ebb4e` | `addsd` / `subsd` / `mulsd` / `divsd` | **`/` 曾不一致**（§30.3） |
| 5 | `neg` | `0x4ebb5c` | `xorpd` 符号掩码 `0x19e690` | 一致（`-x`） |
| 8 | `^` | `0x4ebb95` | `jmp [0x9bc278]` = **libm `pow`** | 一致（`powf` → `pow`） |
| 9 | `sqrt` | `0x4ebb26` | `xorps xmm0,xmm0` + `sqrtsd` | **曾不一致**（§30.3） |
| 10 | `log` | `0x4ebb82` | `jmp [0x9bc270]` = libm **`log10`** | **曾不一致**（§30.3） |
| 11 | `ln` | `0x4ebafa` | `jmp [0x9bbec0]` = libm `log` | **曾不一致**（§30.3） |
| 12 | `exp` | `0x4ebb13` | `jmp [0x9bbeb8]` = libm `exp` | 一致 |
| 13 | `abs` | `0x4ebb66` | `andpd` 掩码 `0x19c050` | 一致（`.abs()`） |
| 14 / 15 / 16 | `sin`/`cos`/`tan` | `0x4ebae7` / `0x4ebb35` / `0x4ebad4` | libm `sin`/`cos`/`tan` | 一致 |
| 17 | `const` | `0x4ebb54` | `movsd xmm0,[r15+8]` | 一致 |
| 18 / 19 | `(` / `)` | `0x4ebcb7` | **落到 panic 路径**（`cmp rax,0x11; ja`） | 我方把它们留在 token 流里但求值不到（真实解析器把它们**删掉**，见 §30.7） |

`GOT 槽 → 符号`（`.rela.dyn` 的 `R_X86_64_RELATIVE`/`GLOB_DAT` 解析，`NEEDED libm.so.6`）：
`0x9bc258=tan`、`0x9bc260=sin`、`0x9bc268=cos`、`0x9bc270=log10`、`0x9bc278=pow`、
`0x9bbeb8=exp`、`0x9bbec0=log`。

### 30.3 三处真偏差（已修）：`log`/`ln` 的定义域、`div`/`sqrt` 的 NaN 符号

`tools/eval_oracle.py --sweep`（真二进制直调，`0x4eb9d0`）实测：

| 表达式 | 真二进制 | 我方（修前） | 判据 |
|---|---|---|---|
| `log(0)` | **-inf** `fff0000000000000` | `NaN` `7ff8000000000000` | libm `log10` |
| `log(-0.0)` | **-inf** | `NaN` | libm `log10` |
| `log(-1)` | `NaN` `7ff8000000000000` | `NaN` | ✓ |
| `ln(0)` | **-inf** `fff0000000000000` | `NaN` | libm `log` |
| `ln(-2)` | `NaN` `fff8000000000000` | `NaN` `7ff8…` | **符号位不同** |
| `sqrt(-1)` | `NaN` `fff8000000000000` | `NaN` `7ff8…` | `sqrtsd` 给负 NaN |
| `sqrt(-0.0)` | `-0.0` | `-0.0` | ✓ |
| `0/0` | `NaN` `fff8000000000000` | `NaN` `7ff8…` | `divsd` |
| `1/0` / `1/-0.0` | `+inf` / `-inf` | 一致 | ✓ |

> ⚠️ **旧结论作废**：`docs/03` §11 曾写"`log(0) = NaN`，判据是 `v <= 0`，
> 上游 Python 给 -inf 是偏差，已按二进制修正"。**这条是反的**——
> 二进制的 `log` 就是 libm `log10`（GOT `0x9bc270`），`log10(±0) = -inf`。
> 当初"实测"用的是同一个 `analysis/emu.py`，而它把导入映射到 python 的 `math`：
> `math.log10(0)` **抛 ValueError**，被 `_safe` 兜成 `NaN` ⇒ 记下来的是 **oracle 的假象**。
> 本节的值来自 `ctypes` 直调 `libm.so.6`，与二进制里的同一份 `libm` 同一个符号。

修法（`src/math_function.rs`，全部是"去掉特判、直接用硬件/libm 指令"，反而更短）：

```rust
fn m_sqrt(x: f64) -> f64 { x.sqrt() }          // sqrtsd；负数 -> 负 NaN
fn m_div(a: f64, b: f64) -> f64 { a / b }      // divsd；0/0 -> 负 NaN，1/0 -> inf
pub(crate) fn m_log10(v: f64) -> f64 { v.log10() }
pub(crate) fn m_ln(v: f64) -> f64 { v.ln() }
```

**副作用（已重建真值）**：`fixtures/eval.tsv` 由 `tools/gen_fixtures.py` 生成，其 oracle 用的
`emu.MATH`＝python `math`，所以 3 行本来就是错的。修 `gen_fixtures.py`（`install_libm()` 换成
`ctypes` 调 `libm.so.6`）后重生成，`git diff fixtures/` **只有这 3 行**：

```diff
-log  10:…;0:…  0.0 0.0 0.0  7ff8000000000000   →   fff0000000000000   (-inf)
-ln   11:…;0:…  0.0 0.0 0.0  7ff8000000000000   →   fff0000000000000
-ln   11:…;0:… -2.0 3.0 0.0  7ff8000000000000   →   fff8000000000000   (NaN 的符号位)
```

`path.tsv`（224 例）、`parse.tsv`（37 例）**完全没变** —— 这是"libm 换法没有副作用"的正对照。

`EXP` 这条**没改**（它本来就调 libm `exp`，`exp(1000) = +inf` 一直是对的）；只有
python-math-oracle 会把 `exp` 溢出记成 NaN，真值夹具里恰好没有这种行。

### 30.4 全量对拍：求值器**零差异**（可复跑）

```bash
WS=<仓库根>; export EMU=$WS/.reference/analysis-repo
export CARGO_HOME=$WS/.reference/cargo-home CARGO_TARGET_DIR=$WS/.agent-work/cg/target15
R=$WS/.reference/replays/gw2-replays-snapshot/replays
cargo run --release --offline --example eval_oracle -- --dir $R --json $OUT/evalcases.jsonl
$WS/.reference/venv/bin/python tools/eval_oracle.py --cases $OUT/evalcases.jsonl
```

| 步 | 结果（1290 发，**解析器修好之后**） | 解析器修之前 |
|---|---|---|
| `tokens_ok`（真解析器的 tag 序列 == 我方） | **1287** | 1280 |
| `const_differ`（同一个 tag 上 CONST 的值不同） | **0** | 0 |
| `eval_ok`（真求值器 == 我方，**逐位**） | **1287** | 1280 |
| `eval_differ` | **0** | 0 |
| `tokens_differ`（**解析**层面不同，见 §30.7） | **1**（只差一处结合，两边求值都是 NaN） | 8 |
| `empty`（回放里 `corrected_function` 为空） | 2 | 2 |

⇒ **求值器（`0x4eb9d0`）在我方所有回放文本 + 真实 x 上逐位一致（1287/1287，0 差异）**；
`tokens_differ` 那 1 发是 `…-e^(999(x-20.09)) + NaN` 这种乱七八糟的文本（`NaN` 被词法器当 `n`=π），
两边都是 91 个 token、只差一处结合，**求值结果两边都是 NaN**（`fff8000000000000`）⇒ 数值上无差别。

⚠️ oracle 侧的**必须**：`analysis/emu.py` 的 `alloc()` 是只进不退的 bump allocator，
逐发跑要在每行前把 `heap_ptr` 放回堆底，否则第 ~470 行起就
`RuntimeError('emulated heap exhausted')`（第一版脚本正是这么翻车的：821 条假 `parse_err`）。
`tools/eval_oracle.py::reset_heap` 做这件事。

### 30.5 154 批 1 ulp：**不是**求值器（8/8 反证 + 不可能性证明）

从 `Y` 桶（`tools/posy_attrib.py` 的 154 批）里挑最小复现，三方对照
（`tools/eval_oracle.py --repro`，真二进制 vs 我方 vs 回放观测）：

| `corrected_function` | x | 真二进制 | 我方 | 回放 `pos_y` | 差 |
|---|---|---|---|---|---|
| `x + 35.910` | -20.911460745133482 | `402dff408986d908` | `402dff408986d908` | `402dff408986d909` | **+1 ulp** |
| `x + 11.649` | 3.3506886740401742 | `402dffd731a25e49` | 同左 | `402dffd731a25e4a` | +1 |
| `-0.29(x)^2 + 10.691` | 6.322417849086799 | `bfecd64ead4a3560` | 同左 | `bfecd64ead4a355f` | -1 |
| `sqrt(abs(175-x^2)-1) - 0.112` | 13.19090509020422 | `bfbb7268730d6e4e` | 同左 | `bfbb7268730d6e4d` | -1 |
| `-x + 2.457` | 17.455988098017656 | `c02dff7b5e34287c` | 同左 | `c02dff7b5e34287d` | +1 |
| `sin(x6cos(5)) - 10.615` | -1.0476229930439582 | `c0272f6480bfc54d` | 同左 | `c0272f6480bfc54e` | +1 |
| `1/(100(x-10)) + 6.282` | 9.99952991831176 | `c02dfb57096fa9b4` | 同左 | `c02dfb57096fa9b3` | -1 |
| `sqrt(4-4sin(x)^2) + 0.221` | 24.992373421336396 | `40019c527cf1ad1f` | 同左 | `40019c527cf1ad20` | +1 |

**8/8：真二进制 == 我方（逐位），回放比两者都差 ±1 ulp。** 差的方向有正有负，不是系统性偏置。

**不可能性证明（`x + 35.910` 这一条，纯算术，可手算复核）**：
`x = -20.911460745133482`（`c034e9557dcbefa4`）、`c = fl("35.910")`（`4041f47ae147ae14`），
两者都是 f64。`x + c` 的**精确实数值**恰好是 `obs - 1 ulp(obs)`，也就是说
**精确和本身就在 f64 网格上**（`ulp(x) = 2 ulp(obs)`）。于是：

* 改 `x`：任何邻近 f64 都差 ≥ `1 ulp(x) = 2 ulp(obs)` ⇒ 结果只能取到 `…515` / `…517` 这类**偶数跳**；
* 改 `c`：`1 ulp(c) = 4 ulp(obs)` ⇒ 跳得更远；
* 两者同时改：`Δ = (2k + 4m)·ulp(obs)`，**偶数**，而 `obs` 与 `fl(x+c)` 相差 **奇数**（1）。
* 暴力复核：`x` 扫 ±3000 ulp × `c` 扫 ±3000 ulp，**0 命中**（脚本内联在 §30.5 的推导里；
  `x+35.910` 的 `fl` 序列是 `…508 / …511 / …515 / …518 / …522`，`obs=…516` 落在**格点之间**）。

⇒ 观测值**不可能**是"任一 f64 的 x 与任一 ±3000 ulp 的常数做过一次正确舍入加法"的结果。
剩下的解释只有"这个 y 不是这条文本的一次求值"（细节来源见 §25.4 的空白：点数组的填充者未定位），
或"报文里的十进制不是最短往返"（但那与 `end_x` 逐位吻合的事实矛盾）。
**这一条仍是未知**，但"求值器最后一位不同"这条解释**已被排除**。

### 30.6 16 批序列化格式差 → **0**（`emit_*` 的 f64 改成 ryu 语义）

`src/_scaffold/protocol.rs` 新增 `emit_f64()`，`emit_function_points` / `emit_effect_info` 用它：

* `format!("{}")`（`Display`）**不能用**：整数值丢 `.0`（`15` vs `15.0`）、**永不**用指数记法
  （`2.638231208698597e-62` 打成 66 位小数）——这正是 §25.2 那 16 批的两种形态。
* `format!("{:?}")`（`Debug`）是**最短往返**，且定/指数分界与 ryu 只差**一档**：
  十进制点落在 `kk == -4`（`|v| ∈ [1e-5, 1e-4)`）时 ryu 用**定点**，`Debug` 用指数。
  实测：只换成 `{:?}` 时 16 批 → **1 批**（`-4.3453289208041886e-5` vs `-0.000043453289208041886`），
  补上 `fixed_for_small()` 后 → **0 批**。
* 非有限值写 `null`（serde_json 不接受 `NaN`/`inf`）。

护栏（`src/_scaffold/protocol.rs::emit_f64_tests`，3 条）：
`ryu_fixtures_roundtrip` 把 `fixtures/protocol*.jsonl`（**真实抓包**，ryu 写的）里
**每一个**浮点字面量重新格式化后逐字节比对；`ryu_known_divergences` 钉住 6 条已知分叉 +
`e-5` 那一档；`emit_message_uses_ryu` 钉住整条报文。

| 指标（`tools/posy_attrib.py`，`conv` 轴） | 修前 | 修后 |
|---|---|---|
| 纯序列化格式（值同、字节不同） | **16 批** | **0 批** |
| 数值末位差（`Y` 桶另一半） | 154 批 | 154 批（未动，见 §30.5） |
| 失败批（1080 分母） | 511 | 495 |

### 30.7 附带发现：**真解析器对"多余的 `)`"是跳过，不是截断**（§29.7 第 1 行的答案）

§29.7 第 1 行把"真解析器遇到没有配对的 `)` 怎么处理"标成**未知**，并说明我方 `parse` 是**截断**、
只在 `game.rs` 里绕开。`tools/eval_oracle.py` 直调 `0x4ed670` 给出了答案：

| 文本 | 真解析器 tag 序列 | 含义 |
|---|---|---|
| `x)` | `[0]` | 多余 `)` 被**丢掉** |
| `x)+1` | `[3,0,17]` | `ADD(x, 1)` —— **没有**截断，继续解析 |
| `2+(x))` | `[3,17,0]` | `ADD(2, x)` |
| `(x)+1)` | `[3,0,17]` | 尾部多余 `)` 丢掉 |
| `sin(x))` | `[14,0]` | ✓ |
| `)` / `()` | `[]` | 空 |
| `(x` | `[0]` | 缺 `)` 也照样解析 |

⇒ 规则是 **"无主的 `)` 直接被跳过，解析继续"**，不是"丢掉整段尾部"。
而且跳过之后**仍然适用隐式乘法**：`x)2 -> [MUL, VAR_X, CONST]`、`sin(x))2 -> [MUL, SIN, VAR_X, CONST]`。

**已修**（本轮，`src/math_function.rs`）：`Parser` 加一个 `depth`（`(` 进、`)` 出），
`expr` / `term` 的循环开头丢一次"无主的 `)`"。三条护栏钉在 `math_function::parser_tests`。

* **对括号平衡的文本是恒等操作**：深度 0 上还能 peek 到 `)` 的，按定义就是无主的，
  所以 1280 条平衡文本的解析一字没变（全量对拍复算：`tokens_ok` 1280 → **1287**，没有一条变坏）。
* **8 发"多右括号"逐条核对**：全 1290 发里 `#(` < `#)` 的**恰好 8 发**，其余全部平衡。
  例：`14/(1+exp(100*(x-19)^2)))+24/(1+exp(100*(x-24)^2)) - 4.323`（`(`×6 / `)`×7）——
  老实现只给 12 个 token 的 `DIV(14, …)`（整段尾部丢掉），真解析器给 **27 个 token** 的
  `SUB(ADD(…), CONST 4.323)`，修后逐 tag 相同。
* 修完还剩 **1 发**不同（见 §30.4 末），是那种"文本本身就是乱码"的输入，两边求值都是 NaN。

### 30.8 验收（本分支 `agent/eval-unicorn`，同一份 401 回放快照，三条轴分开跑）

```bash
export CARGO_HOME=$WS/.reference/cargo-home CARGO_TARGET_DIR=$WS/.agent-work/cg/target15
R=$WS/.reference/replays/gw2-replays-snapshot/replays
cargo run --release --offline -q --example replay_attrib -- --dir $R --drive fire  --policy default --json $OUT/f.jsonl
cargo run --release --offline -q --example replay_attrib -- --dir $R --drive batch --policy default --json $OUT/b.jsonl
cargo run --release --offline -q --example batch_conv --dir $R --policy strict
cargo run --release --offline -q --example replay_attrib -- --dir $R --drive conv --policy strict --json $OUT/c.jsonl
cargo run --release --offline -q --example posy_probe    -- --dir $R --json $OUT/p.jsonl
python3 tools/posy_attrib.py --conv $OUT/c.jsonl --probe $OUT/p.jsonl
cargo test --offline
```

| 指标 | 轴 | 修前 | 修后 |
|---|---|---|---|
| 内容命中 | **fire**（`Match::fire`） | 325 / 1080 | **331 / 1080** |
| 内容命中 | **batch**（`Match::fire_batch`） | 346 / 1080 | **355 / 1080** |
| 内容命中 | **batch_conv**（收敛层，驱动 B） | 595 / 1080 | **616 / 1080** |
| 种类序列 | **batch_conv**（驱动 B） | 1074 / 1080 | **1074 / 1080**（不退） |
| 内容命中 | **conv**（`replay_attrib --drive conv --policy strict`） | 569 / 1080 | **585 / 1080** |
| 纯序列化格式批 | conv | 16 | **0** |
| `cargo test --offline` | — | **1520** passed | **1538** passed（+18，见下） |
| `python3 tools/coverage_matrix.py --check` | — | OK | OK（`src` 函数数 +2，已重跑生成器） |

`+18` 的构成：
* **3** 条 `emit_f64` 护栏 × **5 个 test 二进制** = 15 —— `lib.rs` 一次，外加
  `graphwar_lan_broadcaster_v6` / `graphwar_replay_reader` / `graphwar_server_broker_handler` /
  `server_broker_request_connection` 四个集成测试用 `#[path = "../src/_scaffold/protocol.rs"]`
  把该文件编进来（所以它的 `#[cfg(test)]` 在每个二进制里都跑一遍）；
* **3** 条 `math_function::parser_tests`（括号平衡无影响 / 无主 `)` 跳过 / 真语料那一例）。

基线 1520 是我在同一棵树上 `git stash` 后**自己复算**的（不是引报告）。

### 30.9 仍未确定的（诚实清单）

| # | 事项 | 缺什么 | 档 |
|---|---|---|---|
| 1 | 154 批 1 ulp **到底是什么**（§30.5 已排除求值器/常数/取 x） | 点数组 `[x19+0x30..0x40]` 的**填充者**（§25.4 的空白）；以及回放那份构建的 `pos_x/pos_y` 是在哪一步与"曲线点"分开算的 | **未知** |
| 2 | 真解析器的**完整规则**在极端文本上的残留（`…-e^(…)+NaN` 那 1 发差一处结合） | 那类文本里 `NaN` 被词法器拆成 `n`(π)+跳过字符，真解析器的算子优先级在那种流上怎么走；影响 1/1290 发、且两边求值都是 NaN | **观测**（13 条探针 + 全量对拍） |
| 3 | `fixtures/eval.tsv` 之外，是否还有**别的**夹具被 python-math-oracle 污染 | `path.tsv` 已核对"一字未变"；其它夹具不是求值器生成的 | **推断**（`path/parse` 已做正对照） |
