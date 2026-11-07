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

## 24. 整局对拍**剩余失败**的归因（2026-11-07，403 份定格快照；纯分析）

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

### 24.2 语料与基线：任务书的 5 个数**全部复现**

| 项 | 任务书 | 本轮实测 | 档 |
|---|---|---|---|
| 语料文件 | 401（上一次定格） | **403**（`ls \| wc -l`；其中 **2 份** `Replay::load` 失败、**119 份**没有结算批） | **观测** |
| 有结算批的回放 | — | **282** | **观测** |
| 结算批总数 | — | **1080** | **观测** |
| 结算批内容命中 | 21 / 1080 | **21 / 1080** | **观测** |
| 消息种类序列命中 | 818 / 1080 | **818 / 1080** | **观测** |
| 完全一致的回放 | 1 / 401 | **1 / 403** | **观测** |
| 延迟段死亡集合命中 | 211 / 379 | **211 / 379**（多报 13 / 漏报 154 / 错人 1） | **观测** |
| 延迟段挖洞点命中 | 1 / 259 | **1 / 259** | **观测** |

⇒ 差异只有**语料条数**（401 → 403）：批数与命中数**一位没变**，所以任务书的 5 个数都可直接沿用。
复现命令见 §24.9；`tools/bit_exact_attrib.py --check-baseline` 会把上表的 7 个数字逐项比对并
打印 `全部一致`。

### 24.3 归因表 A：**任务书基线**（`examples/replay_diff` 的驱动，默认 `Policy`）

失败 **1059** 批（= 1080 − 21）。分桶规则与优先级见 `tools/bit_exact_attrib.py` 头部表。

| 类别 | 批数 | 占失败 | 独占 | 根因（指到地址/文档） | 可修性 |
|---|---|---|---|---|---|
| `C` `corrected_function` 文本 | 369 | 34.8% | 15 | `docs/142` §2.2（DiffEq 不加后缀 / Normal 补括号）；旧驱动没接线 | **已修，未接线** |
| `Y2` 爆炸点 y 中等差（1e-6~1e-3） | 332 | 31.4% | **332** | `docs/142` §2.3：原版拿**印刷文本**（常数 3 位小数）回代求 y；旧驱动用停点 | **已修，未接线** |
| `D` 死效块数量（死亡集合） | 132 | 12.5% | **132** | `docs/16` §37（`execute_results` 查表、查不到不发）；实际差的是**死亡集合** | 需先修几何 |
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
生产点**（§24.6），与 `docs/16` §32.3(b) 记的"`execute_results` 里有两处 `EffectInfo` 生产点"一致。
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
在任何挖洞/爆炸之前"）；那个结论仍以 `docs/16` §37.6 的 arm64 指令级证据为准。
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
   跳表 —— 与 `docs/16` §37.6 的 72 字节布局 `+0x20 = effect_type` 吻合）。
   同一现象也出现在 `function_active_event`（tag 36）：**movabs 站点 0 处**。
   ⇒ 这两个变体的 tag 在这份 x86-64 构建里**不是 movabs 出来的**（可能是
   `mov dword [mem],0x…` + `mov dword [mem+4],0x80000000` 的双半写法，扫过 ±0x40 字节窗口没找到）。
   **等级：未知。** 想接手的下一步：从 `0x4bf09b` 的消费者反推 payload 布局 →
   用 frida 在 `EffectManager::add_effect`（`docs/16` §32.3(b) 的 arm64 `0x4a8c94`）断点抓
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
