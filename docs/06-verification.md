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

> ⚠️ **审计 2026-11-08（读本文件前先看这两条）**
> ① **`dispatch/` 不在本仓库**：`dispatch/NN-*.md` 是当轮任务书，每次合并后都从 master 移除
> （最近一次 `ee41ff23`）。本文件 9 处「任务书：`dispatch/…`」已去掉链接、只留**轮次标识**；
> 要看原文得找回对应分支/历史，**它不是可点的证据**。
> ② **本文件是逐轮日志**：§24 起的每个数字都带轮次日期，是**当轮读数**，不是现行基线。
> 现行值只看 `fixtures/guardrails.json` 与 `docs/HANDOVER.md` §0.0.0.2
> （2026-11-08：内容 647 / 647 / 633，种类序列 1075 / 1075 / 1075，死亡 366/385，挖洞 207/294）。

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

REA 的定位、静默陷阱与强制前置检查见 `archive/90-anti-waste.md` §2.6–§2.8
（**审计 2026-11-08**：原链接指向本文件 `§2.6`，但本节只有 §22「防无用功守则（原 `90-anti-waste`）」这个落点，
`§2.6/§2.7/§2.8` 是归档件里的小节号 ⇒ 改成指向归档原文）。

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

> 任务书：`dispatch/03-bit-exact-report.md`。
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

> ⚠ **2026-11-07 深夜审计 —— 本表是 §24 当轮的读数，现行基线已不同。**
> 同一份 401 份定格快照、同一台机器、三根轴分开跑（命令见 README「对拍现在多准」）的**现行**读数：
>
> | 指标 | `fire` | `batch` | `batch_conv`（收敛层） |
> |---|---|---|---|
> | 内容命中 | **325 / 1080** | **346 / 1080** | **595 / 1080** |
> | 种类序列 | **819 / 1080** | **919 / 1080** | **1074 / 1080** |
>
> 延迟段（`batch` 轴）：死亡集合 **215 / 379**、挖洞点 **140 / 262**。
> 审计说明：原表记 `fire` 21 / 818、`batch` 21 / 918、`conv` 568 / 945（当轮值），
> 实测已变为 325 / 819、346 / 919、595 / 1074；`conv` 的 1074 与 §25 一致，`fire`/`batch` 的**内容命中**
> 是这一段涨得最多的（`+304` / `+325`）。**报数字必须带轴名**（`AGENTS.md` §C）——
> 1074 属 `batch_conv`，**不是** `batch` 轴的数。

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
>
> ⚠️ **审计更正（[`docs/153`](153-audit-downstream.md) §1-B/§1-C，2026-11-07 夜独立复算）**：
> 上一行的两个数**都不是合并后代码的实测值**。① 真实路径：在**同一提交** `8755db9e` 上复跑
> `--drive batch --policy strict` = **139/258**（134/259 复现不出来，分母也对不上 258）；
> 合并 `b49c3150` 括号修复后 = **140/262**（`--drive fire` 轴 **139/262**）。
> ② 收敛层**仍是 66/291**（两种 `conv` 驱动都测过）：`198/291` 是 `tools/hole_axis_attrib.py`
> 打印的「**改成**「挖洞点 = 爆炸点」**以后**」，属于**反事实预测**——任务 14 只改了 `src/game.rs`，
> `_scaffold/diffconv.rs` 一行没动，那条路径上的数不会变。

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

> 任务书：`dispatch/09-fx-explosion-y.md`。归因桶 = §24.4 的 `Y`
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
cargo test --offline                       # 1520 passed / 0 failed（审计：本节原记 1512，2026-11-07 复测 1520）
python3 tools/coverage_matrix.py --check   # OK
```

§26.7 的"按模式分组 + ulp 直方图"由一次性脚本从 `$OUT/conv.jsonl` + 回放
（`gzip` 的 JSONL，`FunctionInfo.corrected_function` / `FunctionPoints.end_x` / `EffectInfo.pos_y`）
算出来，口径写在上表里，**没有**新的入库工具。

## 27. `FunctionPoints.end_x` 的取值点：逐指令定死 = `pts[len-1]`（2026-11-07，`dispatch/11`）

> 任务书：`dispatch/11-endx-sample-point.md`（P1，任务书口径"失败 95 批"，
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

> 任务书：`dispatch/10-multi-shot-wiring.md`。
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
cargo test --offline                 1520 passed / 0 failed   （**历史**：当轮记 1516 = 基线 1512 + 本节新增 4 条；2026-11-07 复测 1520）
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

## 30. 把 `diffconv` 的坐标计算**并回内核** + 两条路径的逐位一致性（2026-11-07，`dispatch/16-diffconv-merge`）

> 任务书：`dispatch/16-diffconv-merge.md`（P1，纯工程债）。
> 点这个债的是 `dispatch/reports/CG-round2-2026-11-07.md` **§5**：`src/game.rs`（真实实现）与
> `src/_scaffold/diffconv.rs`（收敛层）**各有一份**爆炸点/挖洞点算法，第二轮任务 09/14 只改了
> `game.rs`、收敛层没跟上 ⇒ 同一批回放在两条路径上得出不同结论，以后每次对拍都得先怀疑
> "是驱动错了还是内核错了"。
>
> ⚠️ 编号：任务书写"用 §28"，但本分支上 **§28（多发批结构接线）与 §29（1074/918→915 复核）已经存在**，
> 所以本节用 **§30**（要 §28 的话得把既有两节整体挪号，不值）。

### 30.1 一句话结论（四条）

1. **清单 = 7 条规则、11 处调用点**（§30.2）。其中 **9 处改成调内核同一个函数**、**1 处删掉**
   （第三份采样）、**2 处保留**并标明是**假设探针**（H2：拿回放的文本重算，故意不同输入）。
   收敛层现在**不再自己算任何一个坐标**。
2. **核心验收过了，而且是逐位、全语料**：Normal 轴 **740 发 / 740 发**的点序列（逐坐标 `to_bits`）、
   `stop`、`blocked`、`killed`（含顺序）、修正文本**全同**；挖洞点 **244 / 244** 逐位相同（§30.5）。
   同一条断言在改之前是 **32 / 740**（§30.6 的正对照 ② —— 708 发的几何本来就**末位不同**）。
3. **`DiffEq` 批（515 批 / 550 发）不可比，不并入分母**：内核开火路径把 `Mode::Normal` **写死**
   （`src/game.rs::fire_functions`），DiffEq 局在内核里**没有**开火路径。这是**内核缺功能**，
   不是"近似通过"（§30.4）。收敛层的 Ode 分派只能留在收敛层，理由是"内核那份不存在"。
4. **回归全绿、且只升不降**：`fire` 内容 **325 / 1080**（未动，`src/game.rs` 一字未改）、
   `batch_conv` 种类 **1074 → 1075**、内容 **595 → 612**（逐批 **变好 17 / 变差 0**）、
   `cargo test --offline` **1525**（基线 1520，+5 条新断言）、`coverage_matrix --check` OK。

### 30.2 清单：7 条规则、11 处调用点（改前 → 改后）

| # | 位置（改前） | 它自己算的是什么 | 改后调的内核入口 |
|---|---|---|---|
| R1 | `corrected_function` 的函数体 + `unbalanced_brackets` | 修正文本：补 `)` + `± c.ddd` 后缀，**`c` 用原文解析的树** | `game::corrected_function_text`（`c` 用**去掉多余 `)` 的树**，`docs/06` §29） |
| R2 | `settle::resolve` 内部（`shot_points(..., apply_correction=true)`） | 采样串：原文 + **全精度** `c`（`shifted_tokens`） | `game::sampling_function_text`（**印出来的三位小数**文本，`docs/06` §25） |
| R3 | 同上 | 点序列：`calculate` + `SERVER_STEP` | `graphwar_shot_calculator::shot_points(…, apply_correction=false)` |
| R4 | 同上 | 停点 / 阻断 / 致死 / 命中弧长 | `settle::resolve_points_owned` |
| R5 | `fire_batch` ① 的 `others` 过滤器 | 射手排除**外加**一条 `1e-12` 位置排除 | 删掉：内核 `resolve_points_owned(owner)` **只比 id**（`docs/122` §4） |
| R6 | `explosion_point(corrected, stop, …)` | 爆炸点：印出文本在 `±end_x` 处求值 + 非有限回退 | `game::hole_point`（产出路径） |
| R7 | `fire_batch` ⑤ 的 `out.stop` | 挖洞点位置（**第一轮的老值**） | `game::hole_point`（挖洞点 ≡ 爆炸点，503/503 逐位） |

调用点（11）：

| 调用点 | 规则 | 处置 |
|---|---|---|
| `fire_batch` ① `resolve(原文, …)` | R2/R3/R4 | 改为 `sampling_text` + `shot_points(…, false)` + `resolve_points_owned` |
| `fire_batch` ① `others` 过滤 | R5 | 删（整张士兵表交内核） |
| `fire_batch` ① `corrected_function` | R1 | 改为转调内核 |
| `fire_batch` ⑤ 挖洞 `out.stop` | R7 | 改为 `hole_point` |
| `fire_batch` ⑥ `explosion_point` | R6 | 改为 `hole_point` |
| `probe_file` `resolve(原文, …)` | R2/R3/R4 | 同上（与预测器逐行同构） |
| `probe_file` `shot_points(原文, true)`（pts / `tail` / `n_points`） | R2/R3 | 改为从 `ShotOutcome::points` 取（**少算一次**） |
| `probe_file` `corrected_function`（`corrected_ours`） | R1 | 随 R1 转调内核 |
| `probe_file` `shot_points(原文, true)`（`our_n`） | R2/R3 | **删**（第三份采样；现在 `our_n == n_points`） |
| `probe_file` `shot_points(回放文本, true)`（`h2` / `h2_n`） | — | **保留**：H2 假设（拿**回放**的文本重算），输入本来就不同，不是重复计算 |

留下的三样"没有内核对应物"的东西（都写明理由）：`mode_of`（回放字符串 → 采样模式）、
`Mode` 分派（`Ode` 时采样原文、修正文本 = 原文）、`explosion_point`（吃**已印出**文本的旧签名助手，
已不在产出路径上，用 §30.5 的等价性断言钉住它与 `game::hole_point` 逐位一致）。

### 30.3 为什么"并回内核"不会把收敛层变差（`Y` 类那处仍在）

`game::execute_results` 的 ④（`EffectInfo(ExplosionEffect)`）**当前仍写 `out.stop`**，
而 ③（挖洞）已经用 `hole_point`（`docs/06` §24 的 `Y` 类、§26 的坐标来源）。收敛层现在
**③④ 都用 `hole_point`**：
* 这是照**内核的那条规则函数**走（不是照内核那份还没接线完的调用点走）；
* 若为了"和内核一模一样"把 ④ 也降回 `out.stop`，收敛层内容会掉回 `Y` 桶（§24：170 批），
  而回放侧的铁证是 **503/503 条 `DestroyObstacle` 与同批 `ExplosionEffect` 坐标逐位相同**；
* 差多少是**量化**的：全语料 **20 / 740 发**（`§30.5` 的"爆炸点相同 720 / 740"），
  这就是"内核 ④ 未接线"的剩余量，不是收敛层算错，也不能藏起来。

### 30.4 `DiffEq`：内核**没有** Ode 开火路径（内核缺口，本任务不许改 `game.rs`）

`src/game.rs::fire_functions` 里写死的是：

```rust
let points = crate::graphwar_shot_calculator::shot_points(
    &corrected, s.x0, s.y0, crate::SERVER_STEP, s.flip,
    crate::graphwar_path_calculator::Mode::Normal,   // ← 写死
    false,
);
```

⇒ 对 DiffEq 房间，内核路径按 `y = f(x)` 采（错），收敛层按 RK4 采（对，`docs/142` §3）。
两种结论不可能逐位相同，所以 **DiffEq 批不进分母**：全语料 **515 批 / 550 发**（Normal 是 565 批 / 740 发，
合计 1080 批 / 1290 发）。这是本节唯一的"做不到"，性质是**内核缺功能**：
`game.rs` 不在 `dispatch/16` 的允许改文件里，且 §29.7 第 3 条已经把它登记成**未知**
（"`Match::fire` 的按模式分，收敛层已做、`game.rs` 没做"）。要让这条也变成"逐位"，下一步得在
内核开火路径上加 `Mode` 入参 —— **另开任务**，不在本节。

### 30.5 一致性断言怎么写（可复跑）

对拍器就放在收敛层里（`src/_scaffold/diffconv.rs` 末尾）：`parity_file` / `parity_dir` / `Parity`。
两条路径对**同一批回放**跑同一批：

| 路径 | 是什么 |
|---|---|
| K | `game::Match::fire_functions`（只算几何、不改状态）+ `Match::fire_batch`（同一批真正结算一遍） |
| C | `ConvPredictor::fire_batch` |

比的**不是**"报文对上回放"，而是**中间结果逐位**：`points`（逐坐标 `to_bits`，长度也要同）、
`stop`、`blocked`、`killed`（**含顺序**）、`hits`（id/下标/弧长位）、修正文本（取 K 真正**发出去**的
`FunctionInfo.corrected_function`）；另单列**挖洞点**与**爆炸点**报文坐标。

```bash
export CARGO_HOME=<ws>/.cargo-home CARGO_TARGET_DIR=<本 worktree 私有>
R=<ws>/.reference/replays/gw2-replays-snapshot/replays
cargo run --release --offline -q --example diffconv_parity -- --dir $R --max-samples 5
```

实测（同一份 401 回放快照 / release / 私有 target）：

```text
文件 286；Normal 结算批 565；发 740；点序列/stop/blocked/killed/修正文本 逐位全同 740 / 740（100.0%）
挖洞点（两边都调 game::hole_point）：逐位相同 244 / 244
爆炸点（内核 ④ 仍用 stop，收敛层用 hole_point）：相同 720 / 740
DiffEq 批 515 / 550 发：不可比（内核写死 Mode::Normal）⇒ 不在分母
结论：Normal 轴 740 发逐位全同，没有一处不一致（退出码 0）
```

`cargo test` 里钉的是**固定样本**（5 份回放，覆盖 Normal 单发自由 / 单发撞障碍 / 撞障碍零死亡 /
**11 批多发** / DiffEq），debug 下 0.32 s：

```text
cargo test --offline --test diffconv_parity -- --nocapture
[样本] 文件 5；Normal 结算批 15；发 26；…逐位全同 26 / 26（100.0%）
[样本] 挖洞点逐位相同 17 / 17；爆炸点相同 26 / 26
[样本] DiffEq 批 3 / 6 发：不在分母
```

`GW2_PARITY_FULL=1 cargo test --offline --test diffconv_parity` 会跑全语料（同一份代码路径）。
测试里另有两条"**反向**断言"（防空转）：样本必须**真的比过**挖洞点（`destroy_compared > 0`）、
必须**真的把 DiffEq 计进不可比**（`diff_eq_batches > 0`）—— 否则"逐位相同"可能只是"没比较"。

### 30.6 正对照：证明"原样就不同"这件事是真的（不是没比较）

按 `AGENTS.md` 合并纪律 A.4，负结论要有正对照。这里做了两条**故意破坏**（改完立刻还原）：

| 对照 | 怎么破坏 | 结果 |
|---|---|---|
| ① | 把 ⑤ 挖洞点改回第一轮的 `out.stop` | `挖洞点 242 / 244`、`不一致 1 处`（`1782675586 #0`）、退出码 **1** |
| ② | 把采样改回"原文 + 全精度修正常数"（= 老 `settle::resolve` 那条路） | `逐位全同 32 / 740（4.3%）`、`不一致 1766 处`、退出码 **1** |
| — | 现状（并回内核后） | `740 / 740`、`244 / 244`、退出码 **0** |

对照 ② 同时回答了"这债到底有多贵"：**改之前，708 / 740 发 Normal 的弹道与内核差位**
（不是"理论上可能"），这也正是 §5 说的"同一批数据两条路径不同结论"。

### 30.7 回归数字（三条轴分开报，命令都能粘）

```bash
cargo run --release --offline -q --example replay_attrib -- --dir $R --drive fire  --policy default --json f.jsonl
cargo run --release --offline -q --example replay_attrib -- --dir $R --drive batch --policy default --json b.jsonl
cargo run --release --offline -q --example batch_conv    -- --dir $R --policy strict
```

| 轴 | 指标 | 改前（`CG@418319d`） | 改后 |
|---|---|---|---|
| `fire` | 内容命中 | 325 / 1080 | **325 / 1080**（未动：`src/game.rs` 一字未改） |
| `batch` | 内容命中 | 346 / 1080 | **346 / 1080**（同上） |
| `batch_conv` | 种类序列 | 1074 / 1080 | **1075 / 1080** |
| `batch_conv` | 内容命中 | 595 / 1080 | **612 / 1080** |
| `cargo test --offline` | 通过条数 | 1520 | **1525**（+3 单元 +2 集成） |

逐批翻面（用 `batch_conv --json` 的 `content_ok` / `kind_seq_ok` 对位比）：
**内容 变好 17 / 变差 0；种类 变好 1 / 变差 0**。变好的 17 批**全是 Normal**（16 个单发 + 1 个两发），
即"采样串换成内核那条（印出来的三位小数）"直接换来的。

### 30.8 这次改动对既有产物的影响（诚实清单）

| 东西 | 变化 | 为什么 |
|---|---|---|
| `diffconv::ShotOutcome` | 新增 `points` / `sampling` / `hits` 三个字段 | 对拍要拿中间结果；只加字段，报文口径不变 |
| `ShotGeom::our_n` | 现在**恒等于** `n_points` | 第三份采样（`shot_points(原文, true)`）删了。旧文档里比 `h2_n` vs `our_n` 的读法要按这条重读 |
| `probe_file` 的 `stop_*` / `killed` / `blocked` / `n_points` / `tail` / `corrected_ours` | 变得与预测器**逐位同源** | 同一批内核入口（此前探针与预测器各算一次，可能不同） |
| `explosion_point` / `unbalanced_brackets` | 仍在，语义未变，但**不在产出路径上** | `tests/diff_conv.rs` 与几个探针还在用；前者另有 `explosion_point_is_the_kernel_hole_point` 等价性断言（含"空函数早退"那条边界的说明） |
| `replaydiff::Predictor::predict` | **一字未改** | `tools/bit_exact_attrib.py --check-baseline` 钉住的冻结基线（第二轮任务 10 刻意没动） |

### 30.9 仍未确定的（诚实清单）

| # | 事项 | 缺什么 | 档 |
|---|---|---|---|
| 1 | DiffEq 局的内核开火路径（采样模式该从哪来） | `game.rs::fire_functions` 的 `Mode` 入参在二进制里怎么传（`docs/06` §29.7 第 3 条同一件事）；本任务不许改 `game.rs` | **未知** |
| 2 | `game::execute_results` ④ 何时改用 `hole_point`（`Y` 类 20 / 740 发） | 与 ③ 同一条证据（503/503）已经指向"该改"，但那是内核侧接线任务 | **推断**（证据足，未接线） |
| 3 | 两条路径"逐位相同"是否等价于"与原版逐位相同" | 本节只证明**内部一致**；与原版的一致性仍由 §24–§29 那几条轴分别计（`fire` 325 / `batch_conv` 612 就是余下的差） | **逐位**（仅对"两路径相等"这一命题） |

## 31. 事件生产者的三条路与"撞什么算撞"（2026-11-07，`dispatch/17`）

> 这一节回答第二轮报告 §7 遗留清单里的两条（"撞障碍判据"、"一次挖洞发 2 条"），
> 并把上一轮那条**"`EffectInfo` 的生产者查不动"** 就地改写成"查得动，但换的是路子"。

### 31.1 一句话（三条）

1. **`EffectInfo`(tag 44) 有且只有 2 个生产点，都在 `execute_results` 里**：
   x86 `0x4729be`（tag 构造）/ `0x4729d6`（`add_event` 发件箱）/ `0x472e35` / `0x472e4d`，
   模块 `src/graphwar_game_state_manager.rs`。上一轮找不到它们，是因为**扫错了常数**（见 §31.2）。
2. **`DestroyObstacle` 一侧是两个 tag，别混**：tag **41**（线上名 `DestroyObstacle`）的唯一生产者是
   `GraphwarObstacleManager::destroy_obstacle` = x86 **`0x3f1b30`**；tag **42**（线上名
   `QueueDestroyObstacle`，即"排一次挖洞"）的唯一生产者是 `queue_destroy_obstacle` = x86 **`0x3f20f0`**。
   两者同在 `src/graphwar_obstacle_manager.rs`。
3. **"撞什么算撞" = 掩膜单点查表**，判据逐指令在 §31.4：`mask[trunc(y*1000/50+300)*1000 + trunc(x*1000/50+500)] != 0xFF`，
   **画布外一律算撞**。二进制里没有几何判据（不是圆/环相交）。

### 31.2 为什么上一轮"查不动"：tag 的**三种** materialize 形态

`GraphwarEvent` 的线上判别值是 `0x8000000000000000 | n`。它在 x86 里有三种写法，**只扫第一种会漏掉生产者**：

| 形态 | 长什么样 | 例子 |
|---|---|---|
| ① `movabs` 直写 | `movabs rax, 0x8000…2c` | tag 42 的 `0x3f22b2` |
| ② `MAX + add` | `movabs rdx, 0x7fffffffffffffff` + `add rdx, n+1` | **tag 44 的两处生产点**：`0x4729be` + `0x4729c8 (add rax,0x2d)` |
| ③ `MAX + inc` | `movabs rdx, 0x7fffffffffffffff` + `inc rdx` | **tag 41 的生产点**：`0x3f1baf` + `0x3f1c09`…（注意中间隔了 22 条指令） |
| ④ 直写 0 形态 | 目标 tag 恰为 `0x8000…0` 时 | `calculate_shot_results` 的挖洞指令：`0x3c38d7` + `0x3c38e1 (inc rdx)` |

⇒ 上一轮只扫 ①，所以"全 `.text` 只有 2 处 `movabs 0x8000…2c` 且都是消费者"是**对的**，
但结论"生产者查不动"是**错的** —— 生产者用形态 ②。

工具已补：`tools/addr_probe.py tags [--only N]`（全 `.text` 一次扫三种形态，
并**自带正对照**：tag 42 必须命中 `0x3f20f0`、tag 41 必须命中 `0x3f1b30`，不命中就返回 1）。

```bash
../.venv/bin/python tools/addr_probe.py tags           # 全部 tag → 生产点 + 模块（panic::Location 投票）
../.venv/bin/python tools/addr_probe.py tags --only 44
```

⚠️ `--only 0` 不可用：`0x8000…0` 在依赖 crate 里是 `i64::MIN`/`Option` 哨兵，全 `.text` 有 2000+ 处，
**这个常数本身没有区分度**；挖洞指令那条记录是靠**形状**认出来的（§31.3 第 3 行）。

### 31.3 生产者清单（`x86-64 2026-08-28`，ELF 虚地址；括号里是 Ghidra 地址）

| tag | 线上名 | 构造点 | `add_event` 调用 | 模块（证据） | 等级 |
|---|---|---|---|---|---|
| **44** | `EffectInfo` | `0x4729be`（`movabs 0x7fff…`）+ `0x4729c8`（`add rax,0x2d`） | `0x4729d6` | `graphwar_game_state_manager.rs`（`locs` 23 票，函数 `execute_results`=`0x472470`） | **观测** |
| **44** | `EffectInfo` | `0x472e35` + `0x472e3f` | `0x472e4d` | 同上 | **观测** |
| **42** | `QueueDestroyObstacle` | `0x3f22b2`（`movabs 0x8000…2a`） | `0x3f22c6` | `graphwar_obstacle_manager.rs:269`（唯一锚） | **观测** |
| **41** | `DestroyObstacle` | `0x3f1baf`+`0x3f1c09`（`MAX+0x2a`） | `0x3f1c17` | `graphwar_obstacle_manager.rs`（`locs` 2 票，函数 `destroy_obstacle`=`0x3f1b30`） | **观测** |
| — | （挖洞**指令**，非线上报文） | `0x3c38d7`+`0x3c38e1`（`MAX+inc`→`0x8000…0`） | 写 `Vec`（`0x3c38e4`…`0x3c38fe`） | `graphwar_shot_calculator.rs`（`calculate_shot_results`=`0x3c2bc0`） | **观测** |

`EffectInfo` 的两个生产点**实参不同**（这是"两个生产点"而不是"同一处两次"的证据）：

| | 站点 | `pos`（`+0x08..0x18`）来源 | `effect_type`（`+0x20`） | `start_tick`（`+0x1c`） |
|---|---|---|---|---|
| A | `0x4729d6` | `[rsp+0xf0]` = 队列记录所指点表的**最后一个点**（`0x47286b`） | **常量 0**（`0x472948 mov byte [rsp+0x90], 0`） | `[管理器+0x58] + X/30 + 60` |
| B | `0x472e4d` | `[rsp+0xc0]`（16 字节） | **记录里的一个字节**（`0x472d75 → 0x472e2e`） | 记录里的一个 u32（`0x472dc2`） |

两处都从同一个全局原子计数器取 `effect_id`（`lock xadd dword [0x9c10c0], …`，`0x472930` / `0x472d9f`），
`effect_type` 在 `+0x20` 上是**一个字节**——与 `docs/16` §33.6 的 72 字节布局、以及两个消费者
（`0x4bf09b` 音频 `GraphwarAudioControl::effect_info_event`=`0x4bf090`、
`0x4e2dd4` `EffectDrawer::ready`=`0x4e2d50`，都在 `[rsi+0x20]` 上按 `effect_type` 分派）互相印证。

**跨架构旁证（arm64 未 strip）**：`GraphwarGameStateManager::execute_results` 里
`0x2c + 0x8000<<48` 的 `movz/movk` 有 4 处（`0x4a87b8`/`0x4a89e8`/`0x4a8cac`/`0x4a8e00`），
其中前两处与 x86 的两个生产点一一对应（第 3 处落在同一个内联块里，第 4 处紧跟
`strb w8,[x19,#0x120]`＝游戏状态 5，是**同一段代码的另一条臂**）。⇒ arm64 侧**不作为**"3 个生产者"的证据。

### 31.4 "撞什么算撞"：判据逐指令

**独立函数**（可被 unicorn/frida 直接调用）：`GraphwarObstacleManager::is_obstacle` = x86 **`0x3f15b0`–`0x3f179c`**。

```text
0x3f15fc  movsd  xmm0, [0x19fbd0]      ; 1000.0
0x3f160c  movsd  xmm2, [0x19faa8]      ; 50.0
0x3f1618  addsd  xmm1, [0x19f838]      ; + 500.0            ← x 轴偏移
0x3f164e  addsd  xmm0, [0x19f1f0]      ; + 300.0            ← y 轴偏移
           ...   cvttsd2si → (col, row)，与 arm64 0x4254d8…0x425550 同构
0x3f166b  cmp    ecx, 0x3e7            ; col > 999 ?
0x3f1671  ja     0x3f169e              ;   ⇒ **算撞**（越界=x）
0x3f1673  cmp    edx, 0x257            ; row > 599 ?
0x3f1679  ja     0x3f169e              ;   ⇒ **算撞**（越界=y）
0x3f1697  cmp    byte ptr [rax + rdi], 0xff   ; mask[row*1000+col]
0x3f169b  setne  al                    ; ← **判据就这一条**：!= 0xFF 即"撞"
```

**对局路径里是内联的**（`is_obstacle` 没有出现在射击链的调用图上，上一轮"三方共用"的猜测据此可以关掉）：
全 `.text` 扫"`mulsd 1000 / divsd 50 / addsd 500|300`"这个三元组，只命中 **7 处**：

| 函数 | 处 | 干什么 |
|---|---|---|
| `calculate_shot_results` `0x3c2bc0` | `0x3c37ee`+`0x3c3831`（**A**）、`0x3c39b6`+`0x3c39f6`（**B**）、`0x3c4eab`+`0x3c4eeb`（**C**） | A = 挖洞指令的判据；C = 逐点**计数**（见下） |
| `GraphwarPlane::obstacle_partial_update_event` `0x4be3e0` | `0x3be425` | 画笔/擦除用的坐标换算，不是碰撞判据 |

A 处的判决（`0x3c3863`–`0x3c3892`）：

```text
0x3c3863  cmp  eax, 0x3e7     ; col > 999
0x3c3868  ja   0x3c3898       ;  ⇒ 撞
0x3c386a  cmp  ecx, 0x257     ; row > 599
0x3c3870  ja   0x3c3898       ;  ⇒ 撞
0x3c3876  imul ecx, ecx, 0x3e8
0x3c387e  add  rdi, rcx       ; idx = row*1000 + col
0x3c388e  cmp  byte ptr [rax + rdi], 0xff
0x3c3892  je   0x3c37aa       ; == 0xFF ⇒ 不撞，继续下一个点
```

⇒ **我方 `src/plane.rs::is_occupied` 的判据已经逐条对上**（越界算撞、`!= EMPTY`），这一层**不需要改**。
唯一的坑是**地址口径**：`plane.rs` 的注释把 `is_obstacle` 写成 `0x4f15b0`（Ghidra 口径），
与本节 `0x3f15b0`（ELF 口径）是同一个函数，**两个数差 `0x100000`**，读文档时不要当成两个函数。

### 31.5 "一次挖洞发 2 条"的成因

**结论：不是两条不同实参 —— 是"同一个坐标排了两次队，各自发了一条"。** 四段证据，全部可复算：

1. **报文逐位相同**：401 份回放 503 条 `DestroyObstacle`，162 组重复的 `(pos_x,pos_y,radius)`
   三字段**逐位相同**（`repr` 相等），从无 3 条以上；
2. **紧邻**：162 组里 **154 组**在消息流里**紧挨着**（下标差 1），7 组差 2，1 组差 14；
3. **一半连 tick 都相同**：`92/162` 两条的消息 tick 相同；其余差值 1…13、21、23、42、44、464
   （= 两条的**预约 tick** 不同：`tick = [管理器+0x58] + X/30 + 60`，`X` = 采样点序号，见下）；
4. **机制链**（两条指令级）：

```text
queue_destroy_obstacle 0x3f20f0   : 往 [管理器+0x40] 的 HashMap<tick, Vec<(x,y,r)>> 里追加一条
        （桶存在就 append：0x3f224b；不存在就新建：0x3f230e→0x3f21c6）
handle_tick_event      0x3f1d30   : 到这个 tick 时按 key 取出整个桶，**逐条**调 destroy_obstacle
        0x3f1d99 test rax,rax / 0x3f1db0 循环（stride 0x18）/ 0x3f1dd0 call 0x3f1b30
destroy_obstacle       0x3f1b30   : draw_circle(mask, x, y, r(=0.5), 0xFF) 擦洞
        0x3f1baa call 0x3f11e0     ; GraphwarObstacleData::draw_circle
        + push 一条 tag 41（0x3f1c17）   ⇒ **一条队列条目 = 一条线上报文**
```

⇒ "两条相同报文" ⇔ "排了两次队"。排队点在生产侧：`calculate_shot_results`（`0x3c2bc0`）里
对**每个采样点**做 §31.4 的掩膜测试，命中就往结果 `Vec` 追加一条挖洞指令
（`0x3c38e4` 写 tag、`0x3c38ee` 写 `(x,y)`、`0x3c38f4`/`0x3c38fe` 写 `(u32 记录字段, u32 点序号)`），
**不 break**，并用一个"该点已处理"的字节数组防重（`0x3c37c6 cmp byte [flags+r15],0`）。
⇒ 两条相同坐标的条目，只能来自**两个不同点序号上坐标逐位相同的点**（同一序号被标志位挡住了）。

**仍未知**（诚实登记）：为什么两个不同序号的点坐标会逐位相同。候选（都未证）：
点表尾部重复（采样在撞点处补了一个重复点）、或同一发的结果在批里出现两次。
缺的证据：那一发的**点表 + 两个点序号**——unicorn 直接调 `0x3c2bc0`（需造 0x20 字节的入参记录）
或 frida 在 `0x3c38e4` 断点抓一次 `1782640836` 的第 2 发。

### 31.6 与我方内核的差异（**这一处没改**，理由写在下面）

| 行为 | 原版（观测） | 我方（`src/graphwar_shot_calculator/settle.rs:154`） |
|---|---|---|
| 判据 | `is_occupied` 同构 | 同构 ✅ |
| 遇撞后 | **不 break**：整批的**每个**命中点各排一次队 ⇒ N 条 | `break` + **只发 1 条** `QueueDestroyObstacle` |

⇒ 我方**结构上不可能**发出第 2 条（这也解释了为什么 341 个坐标我方只发 341 条）。
**不改的原因**：要改成"每个命中点各发一条"，必须先知道上面那条未知（重复坐标从哪来），
否则会把"1 条"变成"命中点个数条"，直接动延迟段挖洞点的分母（140/262）与批内容命中（325/1080）；
在没有逐位证据的情况下这是**用回归风险换一个猜**。⇒ 登记为已知差异，等 §31.5 的证据。

### 31.7 两处顺手的更正（都在本节之前写下的东西里）

1. **`calculate_shot_results` 的 x86 地址是 `0x3c2bc0`，不是 `0x3b5770`**
   （第二轮报告 §7 与 `docs/16` §29.3(b) 的表述沿用了 arm64 的 `0x3b5770`）。
   x86 `0x3b5770` 属于 `src/graphwar_root_node.rs`。桥：`fixtures/name_to_addr.tsv`
   第 83 行（`唯一锚`，22 个双向唯一锚，函数大小 `0x308d`）。
   ⇒ 遗留清单里"`end_x` 那 103 发"那条，接手时请从 `0x3c2bc0` 起。
2. **`execute_results` 的跳表**（`0x1c1800`，直接按序读 5 项，本节点过一遍）：
   `idx0 → 0x472814`（tag ≥ 0：死亡块）、`idx1 → 0x472c61`（tag `0x8000…0`：挖洞块）、
   `idx2 → 0x472a53`（tag `0x8000…1`）、`idx3/4 → 0x4727c0`（跳过，tag `0x8000…2/3`）；
   哨兵 `0x8000…4` 在进入分派前就被拦下（`0x4727d3`）。idx = `tag - 0x8000…0 + 1`（负 tag）或 0（非负）。
   `docs/14` §12.4 那张表**是对的**（idx1 = 挖洞块），本节把它连同"为什么 idx 这么算"一起钉住。

### 31.8 复现（三套命令，全部只读语料）

```bash
export CARGO_HOME=<ws>/.reference/cargo-home CARGO_TARGET_DIR=<你的私有目录>
export PY=<ws>/.reference/venv/bin/python
R=<ws>/.reference/replays/gw2-replays-snapshot/replays

# 1) 生产者表（自带正对照；不命中就返回 1）
$PY tools/addr_probe.py tags
$PY tools/addr_probe.py tags --only 44      # 2 生产 + 2 消费者

# 2) 判据逐指令
$PY tools/addr_probe.py dis 0x3f1660 --length 0x50      # is_obstacle 的判决 + setne
$PY tools/addr_probe.py dis 0x3c3856 --length 0x40      # 内联版（A 处）的判决
$PY tools/addr_probe.py jt 0x1c1800 --owner 0x472470    # 按表序打印 5 项（顺序别丢）

# 3) 重复报文的数据面（下表的数就是它出来的：不需要编译任何东西）
$PY tools/destroy_dup_attrib.py --dir $R
```

`tools/arm64_sym.py`（本轮新增）是 arm64 侧的符号表入口（`find/mod/at/callers/bl/const/dis`），
用来给 x86 侧"这个函数是谁"提供**未 strip** 的独立证据；本节的 arm64 地址全部它给的。

### 31.9 未知清单（可直接接手）

| # | 未定 | 缺什么 | 档 |
|---|---|---|---|
| 1 | 两个不同点序号上坐标为何逐位相同（"2 条"的最终成因） | 那一发的点表 + 两个序号（unicorn 调 `0x3c2bc0` / frida 断 `0x3c38e4`） | **未知** |
| 2 | `calculate_shot_results` 里 `0x8000…1` / `…2` / `…3` 三种指令各自的语义 | `0x472a53`（idx2 块）逐指令；`…2/…3` 在 `execute_results` 里被直接跳过，说明是**本地**中间结果 | **推断** |
| 3 | `destroy_obstacle` 的 `radius` 参数是否恒为 0.5 | 语料 503 条 `radius` 全 0.5（观测）；`0x472d11` 侧是常量 `0x19f6d0=0.5`，但 `handle_tick_event` 用的是桶里存的 r | **观测**（r 来自排队入参） |
| 4 | `QueueDestroyObstacle`(42) 为什么在 401 份回放里 **0 条** | 发送侧的过滤点（`send_buffer` 的挑拣规则，`tools/arm64_sym.py find 'send_buffer'` 是入口） | **未知** |

## 32. 求值器与二进制**逐位一致**；154 批 1 ulp 残差**不在**求值器里（2026-11-07，`dispatch/15`）

> 任务书：`dispatch/15-eval-unicorn.md`（P0）——"用 unicorn 对拍 `evaluate_recursive`，解掉最后 1 ulp"。
> 本节回答三件事：**对拍台怎么搭**、**求值器到底差在哪**（答案：不再差）、
> **那 154 批 1 ulp 是什么**（答案：不是求值器，且给出反证）。
> 顺带把 §25.2/§25.5 的 16 批"纯序列化格式差"打到 **0**。
> ⚠️ 任务书说"最大 §27，你用 §28"；本提交时文件里 §28/§29 已被并行任务占了，所以本节用 **§30**
> （同号冲突按任务书约定由协调者合并）。

### 32.1 一句话结论（六条）

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
   把 x 与常数各扫 ±3000 ulp 都到不了观测值（§32.5）。
   ⇒ 任务书"H1'求值器最后一次舍入不同'"这条前提，**被证伪**（与第二轮另外 5 条同一种性质）。
6. **16 批序列化格式差 → 0**：`emit_*` 的 f64 打印改成 ryu 语义（`{:?}` + `e-5` 那一档补定点），
   `posy_attrib.py` 的"纯序列化格式"从 **16 批降到 0 批**；`fire` 内容命中 **325 → 331**、
   `batch` **346 → 355**、`batch_conv` 内容 **595 → 615**（种类序列 **1074 不退**）。

### 32.2 对拍台（地址级事实先钉死）

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
| 3 / 4 / 6 / 7 | `+ - * /` | `0x4ebb48` / `0x4ebb0d` / `0x4ebb70` / `0x4ebb4e` | `addsd` / `subsd` / `mulsd` / `divsd` | **`/` 曾不一致**（§32.3） |
| 5 | `neg` | `0x4ebb5c` | `xorpd` 符号掩码 `0x19e690` | 一致（`-x`） |
| 8 | `^` | `0x4ebb95` | `jmp [0x9bc278]` = **libm `pow`** | 一致（`powf` → `pow`） |
| 9 | `sqrt` | `0x4ebb26` | `xorps xmm0,xmm0` + `sqrtsd` | **曾不一致**（§32.3） |
| 10 | `log` | `0x4ebb82` | `jmp [0x9bc270]` = libm **`log10`** | **曾不一致**（§32.3） |
| 11 | `ln` | `0x4ebafa` | `jmp [0x9bbec0]` = libm `log` | **曾不一致**（§32.3） |
| 12 | `exp` | `0x4ebb13` | `jmp [0x9bbeb8]` = libm `exp` | 一致 |
| 13 | `abs` | `0x4ebb66` | `andpd` 掩码 `0x19c050` | 一致（`.abs()`） |
| 14 / 15 / 16 | `sin`/`cos`/`tan` | `0x4ebae7` / `0x4ebb35` / `0x4ebad4` | libm `sin`/`cos`/`tan` | 一致 |
| 17 | `const` | `0x4ebb54` | `movsd xmm0,[r15+8]` | 一致 |
| 18 / 19 | `(` / `)` | `0x4ebcb7` | **落到 panic 路径**（`cmp rax,0x11; ja`） | 我方把它们留在 token 流里但求值不到（真实解析器把它们**删掉**，见 §32.7） |

`GOT 槽 → 符号`（`.rela.dyn` 的 `R_X86_64_RELATIVE`/`GLOB_DAT` 解析，`NEEDED libm.so.6`）：
`0x9bc258=tan`、`0x9bc260=sin`、`0x9bc268=cos`、`0x9bc270=log10`、`0x9bc278=pow`、
`0x9bbeb8=exp`、`0x9bbec0=log`。

### 32.3 三处真偏差（已修）：`log`/`ln` 的定义域、`div`/`sqrt` 的 NaN 符号

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

### 32.4 全量对拍：求值器**零差异**（可复跑）

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
| `tokens_differ`（**解析**层面不同，见 §32.7） | **1**（只差一处结合，两边求值都是 NaN） | 8 |
| `empty`（回放里 `corrected_function` 为空） | 2 | 2 |

⇒ **求值器（`0x4eb9d0`）在我方所有回放文本 + 真实 x 上逐位一致（1287/1287，0 差异）**；
`tokens_differ` 那 1 发是 `…-e^(999(x-20.09)) + NaN` 这种乱七八糟的文本（`NaN` 被词法器当 `n`=π），
两边都是 91 个 token、只差一处结合，**求值结果两边都是 NaN**（`fff8000000000000`）⇒ 数值上无差别。

⚠️ oracle 侧的**必须**：`analysis/emu.py` 的 `alloc()` 是只进不退的 bump allocator，
逐发跑要在每行前把 `heap_ptr` 放回堆底，否则第 ~470 行起就
`RuntimeError('emulated heap exhausted')`（第一版脚本正是这么翻车的：821 条假 `parse_err`）。
`tools/eval_oracle.py::reset_heap` 做这件事。

### 32.5 154 批 1 ulp：**不是**求值器（8/8 反证 + 不可能性证明）

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
* 暴力复核：`x` 扫 ±3000 ulp × `c` 扫 ±3000 ulp，**0 命中**（脚本内联在 §32.5 的推导里；
  `x+35.910` 的 `fl` 序列是 `…508 / …511 / …515 / …518 / …522`，`obs=…516` 落在**格点之间**）。

⇒ 观测值**不可能**是"任一 f64 的 x 与任一 ±3000 ulp 的常数做过一次正确舍入加法"的结果。
剩下的解释只有"这个 y 不是这条文本的一次求值"（细节来源见 §25.4 的空白：点数组的填充者未定位），
或"报文里的十进制不是最短往返"（但那与 `end_x` 逐位吻合的事实矛盾）。
**这一条仍是未知**，但"求值器最后一位不同"这条解释**已被排除**。

### 32.6 16 批序列化格式差 → **0**（`emit_*` 的 f64 改成 ryu 语义）

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
| 数值末位差（`Y` 桶另一半） | 154 批 | 154 批（未动，见 §32.5） |
| 失败批（1080 分母） | 511 | 495 |

### 32.7 附带发现：**真解析器对"多余的 `)`"是跳过，不是截断**（§29.7 第 1 行的答案）

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
* 修完还剩 **1 发**不同（见 §32.4 末），是那种"文本本身就是乱码"的输入，两边求值都是 NaN。

### 32.8 验收（本分支 `agent/eval-unicorn`，同一份 401 回放快照，三条轴分开跑）

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

### 32.9 仍未确定的（诚实清单）

| # | 事项 | 缺什么 | 档 |
|---|---|---|---|
| 1 | 154 批 1 ulp **到底是什么**（§32.5 已排除求值器/常数/取 x） | 点数组 `[x19+0x30..0x40]` 的**填充者**（§25.4 的空白）；以及回放那份构建的 `pos_x/pos_y` 是在哪一步与"曲线点"分开算的 | **未知** |
| 2 | 真解析器的**完整规则**在极端文本上的残留（`…-e^(…)+NaN` 那 1 发差一处结合） | 那类文本里 `NaN` 被词法器拆成 `n`(π)+跳过字符，真解析器的算子优先级在那种流上怎么走；影响 1/1290 发、且两边求值都是 NaN | **观测**（13 条探针 + 全量对拍） |
| 3 | `fixtures/eval.tsv` 之外，是否还有**别的**夹具被 python-math-oracle 污染 | `path.tsv` 已核对"一字未变"；其它夹具不是求值器生成的 | **推断**（`path/parse` 已做正对照） |

---

## 33. 点数组的**填充者**找到了；`y` 在写入路径上**逐位未动**（2026-11-08，`dispatch/20`）

> 本节接手 §32.5 / §25.4 的空白：「`[x19+0x30..0x40]` 那个 `Vec<(f64,f64)>` 是谁 push 的」。
> 结论用的是**新的证据源**：`android-2.0.60` 的 `.symtab`（**24600 个带尺寸的符号，未 strip**，
> 名字是符号表原文）与 `windows/graphwarrust-2.0.60.dll` 的 COFF 符号表。
> 两份都是 **2.0.60**，与 arm64 逆向目标是**同一个版本**——名字不是跨版本猜的。

### 33.1 一句话（三条）

1. **填充者 = `GraphwarFunctionManager::set_function_points`**（arm64 **`0x489278`** / x86-64 **`0x456810`**），
   它把一个 `Vec<(f64,f64)>` 的**三段头**（`cap/ptr/len`）搬进 `HashMap<u32, _>` 的桶里；
   点本身由 **`GraphwarShotCalculator::calculate_function`**（arm **`0x3b5374`** / x86 **`0x3c2530`**）`push` 出来，
   经 `calculate_shot_results`（arm `0x3b5770` / x86 `0x3c2bc0`）→ `GraphwarGameStateManager::fire_functions`
   （arm `0x4a7160` / x86 `0x470eb0`，写 `self.results` 的 `+0x30..+0x40`）→ 才到 `set_function_points`。
2. **1 ulp 不在这一步**（**反证**，逐跳）：从采样器返回到 `self.results` 再到 `HashMap` 再到
   `execute_results` 取 `points[len-1]`，**每一跳都是 16/24/32 字节的搬运，没有一条 `addsd/subsd/mulsd/divsd`**。
   ⇒ 写进点数组的 `y` 与采样器返回值**逐位相同**。
3. **"同坐标不同序号"**：队列条目里两个 u32 是 `GraphwarShotResult.id`（`+0x18`）与采样序号 `X`（`+0x1c`）；
   外层循环 over `X`、内层 over **结果记录**，`flags[]` 按**结果**索引 ⇒ **每个结果记录最多排一次队**。
   ⇒ 两条坐标逐位相同的报文只能来自**两条点表逐位相同的结果记录**（不同 `id`），
   **不是** §31.5 猜的"两个不同点序号上的同坐标点"。

### 33.2 `[x19+0x30..0x40]` 到底是什么

`self.results: Vec<GraphwarShotResult>`，元素 **32 B**，布局由**写指令**反推（不猜类型）：

| 偏移 | 字段 | 证据（arm64） | 证据（x86-64） |
|---|---|---|---|
| `+0x00` | `points.cap` | `0x3b5870 stp x27, x10, [x8]` | 只在释放/扩容时用（`shl $0x4` 算 `cap*16`） |
| `+0x08` | `points.ptr` | 同上（`stp` 的第二个寄存器） | `0x3c37d9 mov 0x8(%r14,%r13,1),%rdx` → `0x3c37e2 movupd (%rdx,%r12,1)` |
| `+0x10` | `points.len` | `0x3b5884 str x9, [x8, #0x10]` | `0x3c37d2 cmp %r12,0x10(%r14,%r13,1)` |
| `+0x18` | `id: u32` | `0x3b5890 str w28, [x8, #0x18]` | `0x3c38a3 mov 0x18(%r14,%r13,1),%r14d` |
| `+0x1c` | `flag: u8` | `0x3b5894 strb w10, [x8, #0x1c]` | — |

类型名是**符号表原文**：`core::ptr::drop_in_place<alloc::vec::Vec<graphwarrust::graphwar_shot_calculator::GraphwarShotResult>>`
与 `…<alloc::vec::into_iter::IntoIter<…::FunctionResult>>`（都在 `calculate_shot_results` 的 `bl` 目标里）。
`Vec` 在本构建里的字段序是 **`(cap, ptr, len)`**（`stur q0,[x22,#0x30]` 写 16 B = cap+ptr、`stur x8,[x22,#0x40]` 写 len），
这正解释了 `execute_results` 为什么用 `+0x38` 当 ptr、`+0x40` 当 len（§32.5 写的"`+0x30..0x40`"是同一字段，只是起点）。

### 33.3 逐跳：谁 push、谁搬、有没有算术

`tools/point_array_chain.py hop --arch arm|x86` 直接复现下表。

| # | 谁 | arm64 | x86-64 | 这一跳的指令 | 有算术吗 |
|---|---|---|---|---|---|
| 1 | `calculate_function` push 点 | `0x3b5374` | `0x3c2530` | arm `stp d9, d12, [x8]`；x86 `movsd %xmm0,(%rax,%rcx,1)` + `movsd %xmm0,0x8(%rax,%rcx,1)`（push 走 GOT 槽 `0x9baca0`） | **是**（这一步就是采样器） |
| 2 | 打包成 `GraphwarShotResult` | `0x3b5770` | `0x3c2bc0` | arm `0x3b5870 stp x27,x10,[x8]` / `str x9,[x8,#0x10]` / `str w28,[x8,#0x18]` | **否**（只写 cap/ptr/len/id） |
| 3 | 整条 Vec 搬进 `self.results` | `0x4a7160` | `0x470eb0` | arm `0x4a79f8 stur q0,[x22,#0x30]` + `0x4a79f0 stur x8,[x22,#0x40]`；x86 `0x471a15/0x471a1c movups` | **否**（24 B 头） |
| 4 | 深拷贝点表 → `set_function_points` | 同上 | 同上 | arm `0x4a7ac8 ldp q0,q1 / 0x4a7ad4 stp q0,q1`（32 B/次）；x86 同形 | **否**（块拷贝） |
| 5 | 存进 `HashMap` 桶 | `0x489278` | `0x456810` | arm `0x489478 str q0,[x22]` + `0x48947c str x8,[x22,#0x10]`；x86 `0x456a2b movups (%rbx),%xmm0` + `0x456a2e movups %xmm0,(%r15)` | **否**（24 B 头） |
| 6 | 取 `points[len-1]` 当爆炸点 | `0x4a8558` | `0x472470` | arm `0x4a891c ldp d9, d8, [x8, #-0x10]`；x86 `0x4724cc/0x4724dd` 取 len×32 | **否**（只读两个 f64） |

**第 1 跳里 `y` 是怎么来的（这一跳是唯一的算术，也是唯一可能的偏差源）**：

```text
arm64 0x3b5618  bl 0x4452d4   MathFunction::evaluate_recursive(self=x21, x=d8, y=0.0, z=0.0, &mut scratch)
arm64 0x3b561c  fmov d12, d0  ; d12 = f(x)
arm64 0x3b56f4  stp  d9, d12, [x8]   ; ★ point = (±x, f(x))，y 就是求值器的返回值本身
```

* 同一函数里还有**第二条**路径：`0x3b5404 tbz w22, #0x0` 跳到 `0x3b55c8` 是 Normal（`y = f(x)`），
  不跳则走 `0x3b5504`–`0x3b5518` 的 **RK4**：`fmov d1,#6 / fdiv d1,d12,d1 / fadd d0,d9,d0` 即
  `y_{n+1} = y_n + (step/6)(k1+2k2+2k3+k4)`，点是 `stp d8, d13, [x8]`（`0x3b55b4`）。
  选哪条由 `w22`（`calculate_shot_results` 传的 `[mod+0x32]`，即**函数模式**）的最低位决定
  ——这就是 §30.4 说的"内核没有 Ode 开火路径"在二进制里的样子。
* 采样点**不在均匀 x 网格上**：`0x3b551c`–`0x3b5538` 每次用 `Vector2D::length`（`0x7ac7b4`）
  量上一段弧长，与 `h` 比，`|len-h| < 1e-3` 才收（容差字面量 `0x19fc78 = 0.001`；
  步长 `h` 来自 `fire_functions` 的 `movabs 0x3f847ae147ae147b = 0.01`）。

⇒ **`y` 与"采样器返回值"逐位相同，且这就是求值器的输出**；§32.5 的"写 y 这一步"到此**排除**。

### 33.4 `②` 的正面结论：残差更可能在**另一条 pos 生产者**上

`execute_results` 里有**两处** `add_effect`（§25.4 已钉），第二处的 `pos` 来自
`GraphwarPlanePosManager::get_position`（arm **`0x3b7f58`**，名字是符号表原文）。
把它的读写两侧都扫一遍（`tools/point_array_chain.py pos`，正对照：必须命中两个已知调用点）：

```text
get_position（arm 0x3b7f58）：35 个 bl 调用点，其中与开火有关的只有两个
  0x4a72dc  in GraphwarGameStateManager::fire_functions
  0x4a8b40  in GraphwarGameStateManager::execute_results      ← 就是 §25.4 的第 ② 处 add_effect
  另有 0x3dcbe8 / 0x3dcce0 in GraphwarPathCalculator::calculate_paths（**客户端**动画路径）

set_position（arm 0x3b8160）**只有两个**调用点：
  0x3db4d8  in GraphwarLevelGenerator::place_entity
  0x3e76ec  in GraphwarRemoteEventHandler::handle_server_remote_event   ← 来自**线上**
```

⚠ 写侧里**没有一处**在弹道计算里（`calculate_function` / `calculate_shot_results` / `fire_functions` 都不写它）。

⇒ **这一张位置表不由弹道计算写入，而是由"远端事件"写入**。也就是说：
那 154 批里若有落在第 2 个 `add_effect` 上的，`pos_y` 是一个**别的机器/别的构建算出来再从网上收来**的 f64
——与"本机采样器在这条文本上的求值"本来就不是同一次运算，1 ulp 完全合法。
这与 §32.5 的不可能性证明（`obs` 不是 `fl(x+c)` 对任何 `x` 的结果）**相容**，而"求值器最后一位不同"不相容。

顺带量到一件与 §25.2 口径有关的事：`pos_x == end_x`（逐位）在 **1290 发里只有 1050 发**
（`posy_probe` 的 `obs_expl_x` vs `obs_end_x`，见 33.6）。⇒ §25.2 的"逐位相同"是**一个子集**的陈述，
不能当成"两处 add_effect 的 pos 都等于曲线末点"。
**这一条本身仍未知**：哪 154 批走的是哪一处 `add_effect`。

### 33.5 `③` 同坐标不同序号：**两条结果记录**，不是两个点序号

挖洞队列的写入点在 `calculate_shot_results`（x86 `0x3c38e4`–`0x3c38fe`，逐条 objdump 可复现）：

```text
3c3755  xor %ecx,%ecx                 ; X = 0                 ← 外层的"采样序号"
3c3770  mov 0x38(%rsp),%rcx / inc %rcx ; X += 1
3c378d  mov %rcx,0x38(%rsp)           ; ★ 本轮的 X（报文 tick 由它派生）
3c3798  xor %r13d,%r13d / xor %r15d,%r15d   ; 内层：结果记录偏移 / 下标
3c37aa  add $0x20,%r13 / inc %r15     ; 下一个结果记录（32 B/条）
3c37c6  cmpb $0x0,(%rax,%r15,1)       ; ★ flags[] 按**结果下标 r15** 索引
3c37d2  cmp 0x10(%r14,%r13,1),%r12    ; 结果.length vs X → 到尾巴就 flags[i]=1 退出
3c37de  shl $0x4,%r12 / 3c37e2 movupd (%rdx,%r12,1),%xmm0   ; ★ 取 points[X]（X 在内层共享！）
3c388e  cmpb $0xff,(%rax,%rdi,1)      ; 掩膜单点查表（§31.4）== 0xFF → 这个点没挖到，换下一条结果
3c38e4  mov %rdx,(%rax,%rcx,1)        ; 条目 +0x00 = tag 0x8000000000000000
3c38ee  movdqu %xmm0,0x8(%rax,%rcx,1) ; 条目 +0x08 = (x, y)  ← 直接来自 points[X]
3c38a3  mov 0x18(%r14,%r13,1),%r14d   ; 条目 +0x18 = **GraphwarShotResult.id**
3c38fe  mov %edx,0x1c(%rax,%rcx,1)    ; 条目 +0x1c = X
```

⇒ **`序号` 有两种可能读法，且都不是"同一结果里的两个点"**：
`+0x18` 是**结果记录 id**（`GraphwarShotResult.id`），`+0x1c` 是**采样序号 X**。
关键在于 **`flags[]` 是按结果记录索引的**（`r15` 与 `r13+=0x20` 同步走），而 `X` 在内层是**共享**的：

* 同一 `X`、两条点表逐位相同的结果 ⇒ 坐标逐位相同、`X` 相同（tick 相同）、`id` 不同 ⇒ **同坐标不同序号**；
* 这正好对上 §31.5 实测的"**92/162 连 tick 都相同**"。

⇒ **修正 §31.5 的猜想**：那里写"只能来自两个**不同点序号**上坐标逐位相同的点"，前提是
`flags[]` 按点序号索引；实测它是**按结果记录**索引的，`X` 在内层只有一个值。
所以重复坐标的来源是**同一批里两条点表逐位相同的结果记录**（不同 `id`），不是同一条曲线上的重复点。
**仍是推断的部分**：那两条记录为什么点表逐位相同（两条 `GraphwarShotResult` 天然来自
`calculate_shot_results` 对 `get_current_turn_soldiers` 的逐兵循环 + 一份 id 数组
`0x3b58ac ldr w28,[x21],#0x4`）；"同一条函数被两个兵各算一遍"这一点**未证**。

### 33.6 复现（全部只读语料/二进制）

```bash
export PY=<ws>/.reference/venv/bin/python
# ① 名字 + 地址（arm64 .symtab 原文）+ 字段偏移 + 正对照（17 条断言，必须全 OK）
$PY tools/point_array_chain.py sym
$PY tools/point_array_chain.py fields
$PY tools/point_array_chain.py check          # 退出码必须 0
# ② 逐跳判据指令
$PY tools/point_array_chain.py hop --arch arm
$PY tools/point_array_chain.py hop --arch x86
# ③ set_position / get_position 的读写两侧（arm64 全 .text 的 BL 立即数扫描，带正对照）
$PY tools/point_array_chain.py pos            # 退出码必须 0
# ④ pos_x 与 end_x 的逐位比较（1290 发）
$CARGO_TARGET_DIR/release/examples/posy_probe --dir $R --json /tmp/posy.jsonl
python3 - <<'EOF'
import json,struct
b=lambda x:struct.unpack('<Q',struct.pack('<d',float(x)))[0]
r=[json.loads(l) for l in open('/tmp/posy.jsonl')]
print(sum(1 for x in r if b(x['obs_expl_x'])==b(x['obs_end_x'])), "/", len(r))
EOF
```

**正对照（`check` 子命令就是这套）**：17 条断言里 5 条是 arm64 符号表、12 条是 x86 指令形态，
其中 3 条互为独立交叉（`0x3c2530` 里必须出现**求值器 `4eb9d0`**、`0x470eb0` 里必须出现
`0.01` 字面量与 `call 456810`/`call 472470`）。§32 已经逐位对拍过 `0x4eb9d0`，
所以"我在 `calculate_function` 里认出了求值器调用"这件事本身是可验证的，不是形状相似。

### 33.7 两处旧文档错的更正

1. **§25.4 的"x86 `0x472470` 不是这个函数（是 hashbrown 探测循环）"是错的。**
   `objdump -d --start-address=0x472470` 出来的是 `push %rbp … lea 0xe8(%rdi) → call`（= `hashbrown clear`）
   → `lea 0xe0(%rbp) → call`（= `get_module_wk`）→ `movabs 0x7fff…` → `mov 0x40(%rbp),%rsi` +
   `mov 0x38(%rbp),%rcx` + `shl $0x5`，与 arm64 `0x4a8558` **逐条对应**。
   `fixtures/name_to_addr.tsv` 的 22 个 `panic::Location` 唯一锚也是这么指的。
   ⇒ 那一行的"审计更正"应作废（当时可能把 `.text` 的 VA↔文件偏移算错了）。
2. **§25.2 的"`pos_x` 与 `end_x` 逐位相同"是子集陈述**：1290 发里 1050 发成立（33.4）。

### 33.8 验收（本分支 `agent/point-array-filler`）

`src/` **零改动**（本节只是取证），所以三轴数字与基线**逐个相同**（自己复算，不看报告）：

| 轴 | 指标 | 值 | 命令 |
|---|---|---|---|
| `fire` | 内容命中 | **331 / 1080** | `replay_attrib --drive fire --policy default --json f.jsonl` |
| `batch` | 内容命中 | **355 / 1080** | `replay_attrib --drive batch --policy default --json b.jsonl` |
| `batch_conv` | 内容命中 | **633 / 1080** | `batch_conv --policy strict` |
| `batch_conv` | 种类序列 | **1075 / 1080** | 同上 |

```text
cargo test --offline                        1559 passed / 0 failed
python3 tools/coverage_matrix.py --check    OK
python3 tools/point_array_chain.py check    17/17 OK（退出码 0）
```

延迟段（`batch` 轴）死亡集合 215/379、挖洞点 140/262 —— 未经本分支触碰；
`src/` 零改动时它们是同一棵树的同一批二进制，故未重跑，接管者按 `tools/bit_exact_attrib.py` 复核。

### 33.9 仍未确定的（诚实清单，可直接接手）

| # | 事项 | 缺什么 | 档 |
|---|---|---|---|
| 1 | 154 批 1 ulp **落在哪一处 `add_effect`** | 每条 `EffectInfo(Explosion)` 是 `execute_results` 的 ①（`points[len-1]`）还是 ②（`PlanePosManager::get_position`）产的——需要在 `0x4a89ec` / `0x4a8cb0`（arm）各下一次断点，或对拍时把两条都算出来比 | **未知** |
| 2 | `PlanePosManager` 的位置**谁先算** | 只有 `place_entity` 与 `handle_server_remote_event` 写它；后者消费的 `ServerRemoteEvent` 里那对 f64 是**哪个客户端**算的、用的哪份构建 | **未知** |
| 3 | 两条点表逐位相同的结果记录**从哪来** | `GraphwarShotResult.id` 数组（`0x3b58ac ldr w28,[x21],#0x4`）的上游：是一次开火里**逐兵**一份，还是逐函数一份 | **推断**（有地址、未证） |
| 4 | `calculate_corrected_function` / `add_offset` 是否改变求值路径 | 它产出的 `MathFunction` 与印刷文本是否**同一条 token 流**；若是"给常数加偏移再求值"，就是**双舍入**（§32.5 的奇偶性证明只排除了**一次**加法，排不掉两次） | **未知** |

> 第 4 条是本节留给下一轮最值钱的一条：§32.5 的不可能性证明假设的是**一次** `fl(x+c)`。
> 若点数组用的是"加过 offset 的 token 流"，`fl(fl(x+a)+b)` 的**双舍入**能造出那个"落在格点之间"的值，
> 而这条路径**不在** §32 的对拍范围内（那里比的是"给定文本 + 给定 x"）。

## 34. 内核开火路径补上 `Mode`：DiffEq 515 批 / 550 发**从"不可比"变成"可比"**（2026-11-08，`dispatch/21`）

> 任务书：`dispatch/21-diffeq-fire-path.md`。
> 这一节结掉的是 `dispatch/reports/CG-round3` §7 与本文 §30.4 / §30.9 第 1 条那笔账：
> `game.rs::fire_functions` 写死 `Mode::Normal` ⇒ 微分方程局（DiffEq）的开火走 `y = f(x)` 采样器
> ⇒ **515 批 / 550 发根本不是"不一致"，是没法比**。
>
> 起点 `c99c8b5`，分支 `agent/diffeq-fire-path`，私有 `CARGO_TARGET_DIR`，同一份 401 回放快照。

### 34.1 一句话（四条）

1. **二进制里模式不是"开火时的入参"**，是「本局设置」里的一个字节：`FunctionModeInfo`(46) 的处理器写它，
   **每次开火现读**，再原样塞进结算 config 的 `+0x39`，结算函数把它转给采样器的第 7 个实参
   （整条链逐指令见 §34.2）。
2. `Match` 因此长了一个字段 `function_mode: FunctionMode`（`graphwar_mode_manager` 那个，2 态、`repr(u8)`）
   + `set_function_mode*`；`fire_functions` / `execute_results` 都按 `self.sampling_mode()` 走。
3. **DiffEq 轴从"不可比"变成"可比"**：两条路径（内核 K / 收敛层 C）的逐位对拍分母
   从 **740 发（只有 Normal）** 变成 **1290 发**，**DiffEq 550 / 550 逐位全同**（§34.4）。
4. 对回放内容（才是"能不能对上原版"）：`batch` 轴 DiffEq 内容命中 **0 → 288 / 515 批（55.9%）**、
   种类序列 **359 → 515 / 515**；`fire` 轴 **0 → 277 / 515**。**Normal 三轴护栏一条没退**。

### 34.2 模式怎么传到采样器（**观测：逐指令，自己 objdump 复验**）

x86-64 `2026-08-28`（`.reference/steam/2026-08-28/libgraphwarrust.so`，vaddr）：

```text
0x470eb0  GraphwarGameStateManager::fire_functions（5456 B）
  0x471791  call 0x46e140             ; 模块容器按 16 字节 key 取「本局设置」模块
  0x4717b8  movzbl 0x32(%rax),%r14d   ; ★ 模块 +0x32 = function_mode（2 态字节）
  0x4717ef  movb $0x1,0x80(%rsp)      ; config+0x38 = 1
  0x4717f7  movabs $0x3f847ae147ae147b ; config+0x00 = 0.01（服务端步长）
  0x471806  mov %r14b,0x81(%rsp)      ; ★ config+0x39 = function_mode
  0x47180e  movw $0x1,0x82(%rsp)      ; config+0x3a = 1
  0x471825  call *0x9bacb0            ; = 0x3c2bc0 calculate_shot_results

0x3c2bc0  graphwar_shot_calculator（12 KB）
  0x3c2d18  movzbl 0x39(%rax),%eax    ; 读回 config+0x39
  0x3c2d1c  mov %al,0x198(%rsp)       ; 溢出到栈槽
  0x3c3091  movzbl 0x198(%rsp),%ecx   ; ★ 作为采样器第 7 实参（%ecx）
  0x3c30c1  call *0x9bac98            ; = 0x3c2530 弦长采样器

0x3c2530  采样器
  0x3c2541  mov %ecx,%r15d            ; 第 7 实参 = mode
  0x3c254a  mov %edx,%ebp             ; 第 6 实参 = flip（后面 test %bpl,%bpl 取反 x）
  0x3c25f5  test %r15b,%r15b
  0x3c25f8  je   0x3c2923             ; ★ mode == 0 ⇒ Normal 分支；非 0 ⇒ 落到 0x3c25fe（RK4）
```

* **极性**：`0 = NormalFunction`、`非 0 = DiffEqFunction`。三条互证：`docs/07` §5.2 的枚举判别值、
  图标索引（`fx_option.svg` = 0 / `dx_dy_option.svg` = 1，`0x4c94a3`/`0x4c9529`）、
  以及 `ctx+0x5c == 0` 才加修正常数（`0x51bbaa`）。
* **谁写这个字节**：`FunctionModeInfo`(46) 的分支 `0x3e2b1b`（`mov BYTE PTR [r15+0x32],bpl`；
  同族 45/47/48 = axis/turn/time，`docs/archive/110` §2.3）。⇒ **模式是事件的落值对象，开火时读**，
  同一局里可以变（`Match::set_function_mode` 就是这条的落地）。
* **实参排布**：`(rdi=out, rsi=tokens, xmm0/1/2=x0,y0,step, edx=flip, ecx=mode)`——与
  `src/graphwar_path_calculator.rs::calculate` 的 Rust 签名逐位同构，所以**采样算法一个字节都没改**。

**正对照（本文的两条"只此一处"结论，按 `AGENTS.md` 合并纪律 A.4 做）**：

| 结论 | 扫描 | 结果 |
|---|---|---|
| `config+0x39` 在 `0x3c2bc0` 里**只被读一次、只转给采样器一次** | `objdump … \| grep '0x198(%rsp)'` | 5 处命中：1 次写（`0x3c2d1c`）、1 次读转采样器（`0x3c3091`）、3 次是**同一栈槽后来被当 8 字节量复用**（`0x3c3cd0`/`0x3c3db1`/`0x3c3f79`，都在 `0x3c3xxx` 段） |
| 采样器函数指针槽 `0x9bac98` 只有 2 个使用者 | `objdump -d \| grep '0x9bac98'` | 恰好 2 条：`0x3c30c1`（开火结算体内）、`0x3e2d35`（客户端预览，`docs/151` §2.5 同结论） |

### 34.3 改了什么（**集中、可解释**：一个字段 + 四个新函数 + 三处函数体 + 两处接线）

| 文件 | 位置 | 改法 | 为什么在这里 |
|---|---|---|---|
| `src/game.rs` | `struct Match` | **新增字段** `function_mode: FunctionMode` | 二进制里它就挂在"本局设置"模块上，谁开火谁读（§34.2） |
| `src/game.rs` | `Match::new` | 1 行：从 `room.function_mode` 初始化 | `RoomState::function_mode` 就是 46 号事件的字符串，落地路径不变 |
| `src/game.rs` | 新增 `function_mode_of` / `sampling_mode` | 字符串→枚举、枚举→采样器 `Mode` | 全仓库只有这一份映射（`diffconv::mode_of` 改成转调） |
| `src/game.rs` | 新增 `corrected_function_text_mode` / `sampling_function_text_mode` | `Normal` = 原两个函数；`Ode` = **原文照发** | 修正常数 `c = y0 − f(∓x0)` 对 `dy/dx` 无意义（§34.5 的两条证据） |
| `src/game.rs` | `Match::fire_functions` | 采样串与采样模式都按 `self.sampling_mode()`（**原来写死 `Mode::Normal`**） | 这就是那条"不可比"的根 |
| `src/game.rs` | `Match::execute_results` | ① 发出的 `corrected_function` 按模式；③ 挖洞点 `hole_point(mode == Ode, …)`（原来直接比 `room.function_mode` 字符串） | 同一个模式，两处读取口径必须一致 |
| `src/game.rs` | 新增 `Match::function_mode` / `sampling_mode` / `set_function_mode` / `set_function_mode_wire` | getter + 46 号事件的落点（同时同步 `room.function_mode`） | 线上字符串取 `MODE_NAME_TABLE[0..2]`（**逐位**那张表），不再抄一份 match |
| `src/_scaffold/replaydiff.rs` | `Predictor` | 新增 `mode_schedule`（全文件的 46 号事件时刻表）+ `sync_mode`；`predict` 开头多一行 | 模式是**输入流**的一环，不是常量：喂到哪一批就该用那一刻的值 |
| `src/_scaffold/diffconv.rs` | `mode_of` / `corrected_function` / `sampling_text` | 全部改成**转调内核**（`docs/06` §30 的同一条纪律） | 同一条规则不许有第二份实现 |
| `src/_scaffold/diffconv.rs` | `parity_file` | 删掉 `if diff_eq { continue }`，改成**计入分母**并给内核 `set_function_mode_wire` | 这就是"不可比"那道闸 |
| `src/_scaffold/diffconv.rs` | `Parity` | 新增 `diff_eq_shots_identical`；`summary()` **分轴印**（Normal / DiffEq） | 分母 740→1290 后，混着印会把"Ode 轴刚开始比"读成"Normal 变差" |
| `examples/diffconv_parity.rs` | 打印 | **只改两行文字**（删掉"DiffEq 不可比"这句已变成假话的说明，改成两句统计） | 断言/口径/退出码一字未动 |
| `tests/diffconv_parity.rs` | 注释 + 一条断言 | 注释改口径；**新增**一条防空转断言（`diff_eq_shots > 0 && < shots`） | 原文那条 `diff_eq_batches > 0` **保留原样**且仍然成立 |
| `src/game.rs`（`mod tests`） | 新增 4 条单测 | 模式来源/极性、RK4 换采样器、原文照发、挖洞点=停点 | 自写断言只作回归锁，**不构成复原证据** |

**没动的**：`src/graphwar_path_calculator.rs`（采样算法，`Mode::Ode` 早就实现了）、`*_class.rs`、
`client/**`、`src/lib.rs`、`dispatch/`、既有测试断言。`graphwar_shot_calculator::shot_points` 签名也没变
（`apply_correction` 那条正交开关照旧传 `false`）。

### 34.4 DiffEq 轴的数字（**分轴、带轴名、命令都能粘**）

```bash
R=<快照>/replays ; OUT=<本次工作目录>
cargo run --release --offline -q --example replay_attrib -- --dir $R --drive fire  --policy default --json $OUT/f.jsonl
cargo run --release --offline -q --example replay_attrib -- --dir $R --drive batch --policy default --json $OUT/b.jsonl
cargo run --release --offline -q --example batch_conv    -- --dir $R --policy strict
cargo run --release --offline -q --example diffconv_parity -- --dir $R --max-samples 6
```

**① 三条护栏轴（改前 / 改后都是同一台机器、同一份 401 回放快照上自己跑的）**

| 轴 | 指标 | 改前（`c99c8b5`） | 改后 |
|---|---|---|---|
| `fire` | 内容命中（全 1080 批） | 331 / 1080 | **608 / 1080** |
| `fire` | 其中 Normal | 331 / 565 | 331 / 565（**未退**） |
| `fire` | 其中 **DiffEq** | **0 / 515** | **277 / 515** |
| `batch` | 内容命中（全 1080 批） | 355 / 1080 | **643 / 1080** |
| `batch` | 其中 Normal | 355 / 565 | 355 / 565（**未退**） |
| `batch` | 其中 **DiffEq** | **0 / 515** | **288 / 515**（55.9%） |
| `batch` | DiffEq 种类序列 | 359 / 515 | **515 / 515**（100%） |
| `batch_conv` | 种类序列 | 1075 / 1080 | 1075 / 1080（**未退**） |
| `batch_conv` | 内容命中 | 633 / 1080 | 633 / 1080（**未退**：收敛层本来就按模式走） |
| 延迟段（**batch 轴**） | 死亡集合 | 215 / 379 | **366 / 385**（多报 19 / 漏报 0 / 错人 0） |
| 延迟段（**batch 轴**） | 挖洞点 | 140 / 262 | **206 / 293** |
| `cargo test --offline` | 通过条数 | 1559 | **1563**（+4 单测） |
| `python3 tools/coverage_matrix.py --check` | — | OK | OK（改了 `src/` ⇒ 已重跑生成器，两个生成物一起提交） |

⚠️ 延迟段的**分母变了**（379→385、262→293），不是口径放宽：DiffEq 批原来算不出"可比对象"
（弹道整条错 ⇒ 既不杀人也撞不到障碍），现在产出了，于是进了分母。分子同时涨（+151 / +66）。

**② 两条路径的逐位对拍（`diffconv_parity`，K = 内核 / C = 收敛层）**

```text
改后：文件 286；结算批 1080（Normal 565 / DiffEq 515）；发 1290（Normal 740 / DiffEq 550）
      点序列/stop/blocked/killed/修正文本 逐位全同 1290 / 1290（100.0%）
      分轴 Normal 740 / 740（100.0%）、DiffEq 550 / 550（100.0%）
      挖洞点（两边都调 game::hole_point）：逐位相同 348 / 348
      爆炸点（内核 ④ 仍用 stop，收敛层用 hole_point）：相同 1278 / 1290
```

改前：**DiffEq 515 批 / 550 发被 `continue` 掉，分母 740 发**（§30.4 / §30.5 的记录；本次没有为
"改前"再复跑一遍旧代码，只复跑了改后）。所以这一条的正向变化是**分母**：740 → 1290，
而新加进来的 550 发**一发不差**。

**③ DiffEq 轴还剩什么（`batch` 轴，515 批，`tools/bit_exact_attrib.py` 互斥分桶）**

```text
内容命中 288 / 515（55.9%）；失败 227 批
  Y  爆炸点 y 末位差（求值器末位）      124 批（54.6%，独占 124）
  E1 end_x 差一个采样点                52 批（22.9%）
  X  爆炸点 x 末位差（pos_x ≠ end_x）   43 批（18.9%，独占 23）
  E2 end_x 真分歧（几何）               8 批（ 3.5%）
```

⇒ **真正的"几何算错"只有 8 批（1.6%）**；`end_x`（弹道终点 x）逐位相同的 **455 / 515（88.3%）**；
`pos_y` 的差 **1 ulp 占 101 批**、2 ulp 29 批——与 Normal 轴那 106 批 1 ulp 是**同一类残差**
（§32.5 已把求值器排除干净，来源仍是**未知**）。`C`（修正文本）桶**已经清零**
（改前那 2 批就是 §34.5 的括号证据）。

### 34.5 顺手钉死的一件事：DiffEq 局**连补括号都不做**

`corrected_function` 在 Normal 局是 `原文 + ")"×缺 + " ± c.ddd"`；在 DiffEq 局是**原文逐字节**。
第一版实现把"补括号"当成与模式无关的 `to_text` 性质（**推断**），结果 2 批报 `C` 类：

| 回放 | 我方（补完括号） | 回放（真值） |
|---|---|---|
| `1782760485` #0 | `…cos(2588))))))))))))))))`（补 16 个） | `…cos(2588`（**一个都没补**） |
| `1783013297` #0 | `sin(ln(abs((sin(558)cos(559)))))`（补 3 个） | `sin(ln(abs((sin(558)cos(559))` |

⇒ 微分方程局里"算修正常数 + 重印函数串"这一步**整个被跳过**（补括号是这个重印的副产物），
所以采样器吃的也是原文。这就是 `corrected_function_text_mode` / `sampling_function_text_mode`
两个 `Mode::Ode` 分支都返回 `function_text.to_string()` 的依据（**观测**：回放实读 2/2，加上
550/550 的 `corrected_function == function`）。改动前后这 2 批由 `C` 变成命中。

### 34.6 正对照：证明"可比"不是"把比较放宽了"

| 检查 | 做法 | 结果 |
|---|---|---|
| 口径没松 | `compare_batch` / `Policy` / 投影 / 折叠 / 归一化 / leave-one-out：**一行未改**（本次没有碰 `replaydiff` 的比对层，只在 `Predictor` 里加了模式同步） | `git diff` 里比对函数零改动 |
| 提升来自哪 | 只换采样模式、文本规则不动：`fire` DiffEq 0 → **276**、`batch` 0 → **287**；再把文本规则改成"Ode 原文照发"：**+1 / +1** | 287+1=288（`batch`） |
| 不是"少比了" | `batch_conv` 的 1080 分母、`diffconv_parity` 的 1290 分母都**没变小**；`tests/diffconv_parity.rs` 新增 `diff_eq_shots > 0 && < shots` 防空转断言 | 全绿 |
| Normal 没被动 | `fire` Normal 331/565、`batch` Normal 355/565、`batch_conv` 1075/633 —— 与改前**逐数相同** | 未退 |
| 模式不是"猜"的 | `function_mode` 的取值集合与极性可指到 `docs/07` §5.2（`逐位`）+ §34.2 的两条指令 | — |

### 34.7 仍未确定的（诚实清单）

| # | 事项 | 缺什么 | 档 |
|---|---|---|---|
| 1 | DiffEq 轴 **124 批 `pos_y` 1~3 ulp** 的来源 | 与 Normal 轴 §32.5 那个空白同一个入口：点数组的**填充者**。本节只证明"不是模式选错"（把模式改回 Normal 就整条弹道错，不是末位差） | **未知** |
| 2 | DiffEq 轴 **52 批 `E1`**（`end_x` 差一个采样点）与 **8 批 `E2`** | 采样器的二分收敛在 Ode 分支上与真值差一个点；`docs/04` §10.4 说 Ode **没有**半步复检（`0x3c2840` 观测），所以不是"少了一段代码"这么简单 | **未知** |
| 3 | `graphwar_mode_manager::FunctionMode` 的**类型归属** | 二进制里那个模块的 Rust 类型名是 TypeId 哈希，`type_name` 不存在（`docs/archive/110` §2.5 第 1 条）。本字段用它是因为**语义与 2 态形状**对得上 | **推断（强）** |
| 4 | 一局里**中途改模式**对已发出的弹道有没有影响 | 语料里 **0 / 1080 批** 的"批前最后一个 46 号事件"与"第一批之前那个"不同（本次实测，见下），所以这条路径**在现有语料上不可观测**；`Predictor::mode_schedule` 按"开火时读"实现，但**没有被任何样本检验过** | **推断** |

> 第 4 条的实测（可复跑：对 403 份回放逐行扫 `FunctionModeInfo` / `CalculatingResults`）：
> **每批之前的模式 == 第一批之前的模式，0 例外**。96 份回放全文里出现过两种取值，但那些切换
> 都落在**最后一批之后**（换局的设置广播）。⇒ `Match::new` 那一次初始化就够覆盖全语料；
> `Predictor` 的时刻表是"按事件语义写对"，不是"靠它拿分"。

## 35. **运行时**抓到点数组的填充者：`calculate_function`；写进去的 y 与求值器**逐位相同**（2026-11-07，`dispatch/25`）

> 任务书：`dispatch/25-frida-point-array.md`。**本节全部是运行时观测**
> （frida 17.26.1 挂在真游戏 `Graphwar II` Linux 2.0.60 进程里），不是静态推断。
> 脚本：[`tools/frida_point_array.js`](../tools/frida_point_array.js)（下断）+ [`tools/frida_point_array.py`](../tools/frida_point_array.py)（驱动）。
>
> 前置：§25.4（点数组 8 字节布局）、§25.5 第 2 条（"填充者未知"）、§26.2（arm64 逐指令）、§32.9 第 1 行。

### 35.0 一页结论

| # | 问题 | 答案 | 档 |
|---|---|---|---|
| ① | 游戏跑起来了吗 | **跑起来了**：Linux **2.0.60**（Steam 版，`libgraphwarrust.so` sha256 `9a538115…`），Xvfb + frida 拉起，进到 campaign 对局并**开了炮** | **观测** |
| ② | 点数组的填充者 | **`calculate_function`，x86-64 ELF RVA `0x3c2530`**（FDE `[0x3c2530,0x3c2bb9)`，本构建）。它自己 `push` 出 `Vec<(f64,f64)>` 并把它当返回值交给调用方；调用方（`gw+0x3e2d3b`，`graphwar_remote_event_handler.rs` 里那个大函数 `0x3e2830`）把这个 0x20 字节记录**原样**放进开火队列 `[GSM+0x38+i*0x20]` | **观测** |
| ③ | 写进去的 y 与采样器/求值器返回值 | **逐位相同**：`3588/3588`（run5）、`4062/4062`（复跑）、`4482/4482`（run4）——`exact = n-1`、`ulp1 = 0`。**唯一不来自求值的点是第 0 点**（炮口/出生点）。⇒ **1 ulp 不在这一步** | **逐位**（真机内存 vs 真机求值器返回值） |
| ④ | 同坐标不同序号的"序号" | **未观测到样本**（缺什么见 §35.6） | **未知** |

### 35.1 怎么跑起来的（命令 + 版本 + 日志）

构建与二进制（**地址都相对这一份**）：

```text
构建      Graphwar II Linux 2.0.60（Steam）
          .reference/game/{Graphwar.x86_64, Graphwar.pck, libgraphwarrust.so}
sha256    9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187  libgraphwarrust.so
          ⇒ 与逆向目标**逐字节相同**，所以本节所有 RVA 都是"目标二进制"的地址
引擎      Godot 4.7.stable.official.5b4e0cb0f + godot-rust (API v4.6, runtime v4.7)
```

跑法（两个坑，都实测）：

1. **`/tmp` 是逐命令 tmpfs** ⇒ `Xvfb` 的 `/tmp/.X11-unix` 跨命令不可见。改成 **TCP 监听**
   （`Xvfb :92 -listen tcp -ac`），后续任何命令都能用 `DISPLAY=127.0.0.1:92` 连上。
2. **Xvfb 直接起会 core dump**（`libEGL_nvidia` → `swrast_dri` 段错误）。加
   `__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json` +
   `LIBGLX_VENDOR_LIBRARY_NAME=mesa` + `LIBGL_ALWAYS_SOFTWARE=1` 后正常（llvmpipe）。
3. Vulkan 不可用 ⇒ Godot 自己回落到 OpenGL 3（日志原文见下）。

```bash
# ① Xvfb（TCP，1920x1160 —— 与真帧管线同尺寸）
Xvfb :92 -screen 0 1920x1160x24 -listen tcp -ac &
# ② frida 拉起真游戏 + 下断 + 点菜单 + 长按开火（同一个 PID 命名空间里做完）
.reference/venv/bin/python tools/frida_point_array.py --out /abs/run.jsonl --display 127.0.0.1:92
```

游戏自己的输出（**原文**，`[stderr]`）：

```text
Initialize godot-rust (API v4.6.stable.official, runtime v4.7.stable.official, safeguards balanced)
Godot Engine v4.7.stable.official.5b4e0cb0f - https://godotengine.org
WARNING: Your video card drivers seem not to support the required Vulkan version, switching to OpenGL 3.
...
src/graphwar_function_input.rs:519: dt:0.010279712      ← 对局里每帧打的 debug 行
src/entity_drawer.rs:585: dt:0.008586611
src/effect_drawer.rs:507: dt:0.048114464
```

真帧（1920×1160 根窗口截图，逐帧看过，**不入库**，一条命令可重放）：
主菜单四项 + 右下角版本号 **`2.0.60`**；`Battle` → LAN 大厅（`Create Game`/`Join IP`）；
`Campaign` → 关卡节点 → `Level 0 / Write a function of x to hit the target` → `Start` →
对局界面（左键区数字/函数键盘、右侧 `Fire!`、底部 `f(x) =` 输入框）；输入 `sin(x)+3` 后
**长按** `Fire!` 2 s，图上出现正弦曲线并开始播放弹道。

⚠ **`Fire!` 是长按按钮**（`play_screen.tscn` 里 `FireButton.long_press_input_text = "fire"`）：
单击只按下不动，必须 `mousedown; sleep 2; mouseup`。踩过一次（前两炮"没反应"就是这个）。

### 35.2 下断地址：**本构建**逐条核对过（并更正 §25.4 的一句话）

`tools/addr_probe.py fde <rva>`（`.eh_frame`，`--so` 指到 `.reference/game/libgraphwarrust.so`）：

| RVA | FDE 范围 | 是什么 | 怎么核的 |
|---|---|---|---|
| `0x472470` | `[0x472470, 0x473513)` | `execute_results` | 序言 `push rbp/r15/r14/r13/r12/rbx` + `sub rsp,0x148`，与 §25.4 记的一致 |
| `0x4724cc` | 同上（体内） | 开火队列预扫第一条指令 `mov 0x40(%rbp),%rsi` | 停在这里读 `rbp`=self 与 `[self+0x38/+0x40/+0x48]` |
| `0x3c2530` | `[0x3c2530, 0x3c2bb9)` | `calculate_function` | 函数指针槽 `0x9bac98` **运行时**读出来 = `base+0x3c2530`（旧文档的"唯一入口"成立） |
| `0x4eb9d0` | `[0x4eb9d0, 0x4ebbe0)` | 求值器（`src/math_function.rs`） | 递归自调用点 `gw+4eba49/4eba6b/4eba91` |

📌 **更正 §25.4 的"`0x472470` 别再照着用"**：那句话对 **2026-08-28 那份 x86** 成立；
在 **Linux 2.0.60（sha256 `9a538115…`）** 这份上，`0x472470` **就是** `execute_results`，
`.eh_frame` 给的函数体 4260 B，与 arm64 的 3712 B 同量级同形状（本条 **观测**）。

### 35.3 开火队列在 x86 上的实际布局（与 arm64 记的差 8 字节）

`execute_results` 体内 `0x4724cc` 处停下，`rbp` = GSM：

```text
[self+0x38] = 队列 ptr        [self+0x40] = 队列 len(u32)     [self+0x48] = cap(u32)
元素步长 0x20（函数体里 shl $0x5），元素布局（真机内存 dump，四次抓取一致）：
  +0x00 kind(u64)      例：0x8000000000000001 / 0x8000000000000002 / 0x1000 / 0x2000
  +0x08 pts.ptr
  +0x10 len(u64)       ← 点数组长度（函数体 `mov 0x10(%rdx),%edi` 用的就是它，再 ÷30 算 start_tick）
  +0x18 = 230 (u64)    ← **四次抓都是 230**，与 len 无关，**不是 cap**（cap < len 不可能）
```

⇒ §25.4 记的 arm64 `[x19+0x30/+0x38/+0x40]` 与这里的 `+0x38/+0x40/+0x48` **差 8 字节**。
是不是同一结构的不同基址 —— **未知**（缺：arm64 那份的同一处运行时 dump）。

> ⚠ **§35.8.5 第 2 条更正**：`+0x18` **不是恒定 230**。本仓库独立复现的四发实测是
> `231 / 231 / 231 / 230`（与 len 仍然无关）。它也不是 `calculate_function` 写的
> （它的返回值只到 `+0x17`），是**入队路径**加的字段。

### 35.4 填充者 = `calculate_function`（RVA `0x3c2530`）：四条独立证据

一炮打出去，点数组有 **3221 / 3589 / 4063 / 4483 / 4847** 个点（每次不同，取决于打在哪儿），
x 步长恒 **0.00875**（世界坐标）。

> ⚠ **§35.8.5 第 1 条更正**：不是"恒 0.00875"。本仓库独立复现的一发 4943 点里，相邻 x 差是
> **三个值** `0.0075 / 0.00875 / 0.009375`（= 弦长 `0.01` 的二分格点，`0.01×3/4、7/8、15/16`）。

1. **返回值就是这个队列元素**：`calculate_function` 在 `onLeave` 时 `xmm0` 低 16 字节 =
   `{kind=0x1000, ptr=0x7f294c144860}`，而同一发在 `execute_results` 里读出的队列元素 =
   `{kind=0x1000, ptr=0x7f294c144860, len=3221, 230}` —— **ptr 同一个**。
2. **`Vec` 扩容的调用栈落在它体内**（`realloc` 探针，size ≥ 16 KB，调用方在 `.so` 内）：
   ```text
   realloc(16384) → gw+460fff ← gw+460c5a ← gw+4ebb48 ← **gw+3c2b23** ← gw+3c30c7   (第 1 次调用)
   realloc(32768) → 同上                                            ← gw+3c30c7
   realloc(65536) → 同上                                            ← gw+3c30c7
   realloc(16384) → 同上                                            ← **gw+3e2d3b** （进队列的那一次）
   ```
   > ⚠ **§35.8.4 更正**：最后一行那个括号是错的。本仓库独立复现四发，
   > 队列元素的 ptr **永远等于 `gw+3c30c7` 那条链最后一次 realloc 的返回值**，
   > `gw+3e2d3b` 那条链的返回值**没有**进队列（它落在低位堆上，ptr 不同）。
   `gw+0x3c2b23` ∈ `[0x3c2530, 0x3c2bb9)` = `calculate_function` 体内。
3. **逐点的求值调用点也在它体内**：求值器的**精确返回地址**直方图（一次开火，run5）：
   ```text
   gw+3c2995 : 18222      ← calculate_function 体内（每点一次）
   gw+3c2a21 :  7178      ← 同上（另一条采样臂）
   gw+4eba49/4eba6b/4eba91 : 50804/50804/25408   ← 求值器**自递归**
   gw+3c602f : 11
   ```
4. **写进去的每个点都来自这里**：见 §35.5 的 `pairWho` 全 = `gw+3c2995`。

调用者：一次开火里 `calculate_function` 被调 **2 次**——
`gw+0x3c30c7`（客户端/绘制侧）与 `gw+0x3e2d3b`（`src/graphwar_remote_event_handler.rs` 的
`0x3e2830` 大函数，**这一份的返回值进了开火队列**）。⇒ 一句话：
**`calculate_function` 生产点数组，`graphwar_remote_event_handler` 那个开火分支把它原样入队。**

> ⚠ **§35.8.4 更正**：加粗那句的后半截与独立复现不符。四发实测：队列里那条记录的 ptr 属于
> **`gw+0x3c30c7`** 那次调用（FDE `[0x3c2bc0,0x3c5c4d)`，`panic::Location` 归属
> `src/graphwar_shot_calculator.rs` 12 票），**不是** `gw+0x3e2d3b` 那次。
> 且 `+0x18` 那个 eightbyte 不是 `calculate_function` 写的（它的返回值只到 `+0x17`）。

### 35.5 1 ulp 判定：写进去的 y 与求值器返回值**逐位相同**

做法（全部在游戏进程里算，`xmm0` 只取低 8 字节 = 一个 f64 的位模式）：
把求值器每次返回的 y 与 (x,y) 对收进两张表；等 `execute_results` 里的点数组出现时逐点回查。

| 抓取 | 点数组 len | `exact`（y 逐位相同） | `ulp1`（差 ±1 ulp） | `miss` | `(x,y)` 双位命中 & 来自谁 |
|---|---|---|---|---|---|
| run4 | 4483 | **4482** | **0** | 1 | 4482，全 = `gw+3c2995` |
| run5 | 3589 | **3588** | **0** | 1 | 3588，全 = `gw+3c2995` |
| run_final（用交付脚本复跑） | 4063 | **4062** | **0** | 1 | 4062，全 = `gw+3c2995` |

* 三次的 `miss` **都是同一个点：`i = 0`**。它是炮口/出生点（世界坐标 `(-11.394271926113204,
  -12.600201435480562)`，下一个点 x = 前一个 + 0.00875），**不是一次求值**，所以查不到天经地义。
* ⇒ **y 从求值器到点数组这一步一个 bit 都没变**。§25.5 / §32.9 里"1 ulp 可能在这一步"的假设
  **被运行时否掉**；残留要么在求值之前（文本/常数/取 x），要么在点数组之后（取末点 → 报文）。

### 35.6 ④「同坐标不同序号的序号从哪来」：**没取到样本**

* 本轮只打通了 **campaign Level 0**（"写一条 f(x) 打到目标"）：**场上没有任何 obstacle**，
  `execute_results` 里那条 `DestroyObstacle` 分支根本不会走。
* 想取样需要**带障碍物的对局**：`Battle → Create Game` 的 LAN 大厅里 Team 2 是空的，
  `+` 按钮点了只高亮、没有增员（`Start` 于是不生效）。**缺：**大厅加机器人/第二个玩家的
  正确点击路径（或一份带 `DestroyObstacle` 的存档/关卡）。
* 现在能说的只有静态那条（**观测**，arm64）：两处 `EffectInfo` 的 id 来自 `ldxr/stxr` 的
  **全局原子计数器自增**（§25.4）。**障碍物实体 id 是不是同一个计数器 —— 未知**，没证据。

### 35.7 复跑与数据

```bash
Xvfb :92 -screen 0 1920x1160x24 -listen tcp -ac &                 # 要 mesa-only 的 EGL/GLX 变量
.reference/venv/bin/python tools/frida_point_array.py --out /abs/run.jsonl --display 127.0.0.1:92
# ↑ 自动：spawn 真游戏 → 挂钩 → 点 Campaign/关卡/Start → 输入 sin(x)+3 → 长按 Fire! → 抓 exec_queue
```

输出是 JSONL：`hooks`（下断结果 + 指针槽内容）、`exec_enter`、`exec_queue`
（队列原文 + 点数组 head/tail/`yhash` + `classify`）、`evals`、`samplers`、`reallocs`、`beat`。
`classify` 里的 `exact/ulp1/miss/pairWho` 就是 §35.5 那张表。
原始 JSONL 留在 `.agent-work/`（可丢弃，**不入库**）；本节数字都能用上面这一条命令重新生成。

### 35.8 本仓库独立复现（2026-11-07，另一台机器 · 另一个会话）

> **这一节是重跑，不是复述。** 换了台机器、从零装 frida、自己把游戏拉起来、自己打炮、自己算，
> 目的只有一个：**§35.0 那三条到底能不能独立复现出来**。
>
> 一句话：**①②③ 全部复现**（③ 还用了比 §35.5 **更严**的判据）；另外得到**一条新事实**
> （修正量被四舍五入到 3 位小数，§35.8.3）和**三处与 §35.3 / §35.4 不一致**（§35.8.4、§35.8.5）。
> ④ 依旧**未知**，卡点完全相同（没有带障碍的对局）。

#### 35.8.0 一页对照

| # | §35.0 的说法 | 本机独立复现的结果 | 一致？ | 档 |
|---|---|---|---|---|
| ① | 真游戏 2.0.60 起来，Xvfb + frida，进 campaign 对局并**开了炮** | 全部达成。版本号本机另有**两条独立来源**（主菜单右下角 `2.0.60`；`Graphwar.x86_64 --version` → `4.7.stable.official.5b4e0cb0f`） | **一致** | 观测 |
| ② | 填充者 `calculate_function`，RVA `0x3c2530`；它 `push` 出 `Vec<(f64,f64)>`；调用方 `gw+0x3e2d3b` 把 0x20 字节记录放进开火队列 | 填充者**同为 `0x3c2530`**（§35.4 的证据逐条重放成功，另加"函数指针槽 → RVA"一条）。但**进开火队列那条记录的 ptr 属于另一次调用（`gw+0x3c30c7`）**，不是 `gw+0x3e2d3b` 那次 —— §35.8.4 | **地址一致，归属不一致** | 观测 |
| ③ | 写进点数组的 y 与求值器返回值**逐位相同**：`exact=n-1`、`ulp1=0`，唯一 miss 是第 0 点 | 四次抓取**全部** `exact=n-1`、`ulp1=0`、`miss=1` 且 miss 的 `i=0`。另外用分析仓库的参考采样器 `graphwar_curve.py` **逐位重算整条曲线**，四次全中（§35.8.3） | **一致（且更强）** | 逐位 |
| ④ | 同坐标不同序号的"序号" | 同样**没拿到**：仍然只有 campaign Level 0，场上 0 个障碍物 | **一致（都未知）** | 未知 |

#### 35.8.1 环境、跑法、以及三个坑哪些真的踩到了

```text
frida         17.26.1（pip wheel frida-17.26.1-cp37-abi3-manylinux_2_5_x86_64）
frida-tools   14.13.0
python        3.14.9 —— 装进**分析仓库根的 `.venv`**，不用系统 python
Xvfb          /usr/bin/Xvfb :92 -screen 0 1920x1160x24 -listen tcp -ac -nolisten unix
游戏          把 Graphwar.x86_64 / Graphwar.pck / libgraphwarrust.so 三个**符号链接**放进一个临时目录，
              再用 `tools/frida_point_array.py --game <那个目录>` 指过去（原因见下）
libgraphwarrust.so sha256 = 9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187
              ⇒ 与逆向目标**逐字节相同**，所以本节所有 RVA 仍然通用
```

⚠ **本机没有 `.reference/game/` 这个目录**（三份文件在分析仓库根上，`libgraphwarrust.so` 是符号链接）。
`tools/frida_point_array.py` 的 `_find_repo()` 是按 `.reference/game` 找根的，找不到会**静默**回落到
脚本上一级 —— 于是 `--game` 指错、frida `spawn` 说文件不存在。**必须显式给 `--game`。**

三个坑的实测结论（**两个真，一个本机不成立**）：

| §35.1 的坑 | 本机 | 证据 |
|---|---|---|
| ① `/tmp` 是逐命令 tmpfs ⇒ Xvfb 别用 unix socket | **真的** | 另一条命令里 `ls /tmp/.X11-unix` → 不存在；不带 `-listen tcp` 起 Xvfb 直接报 `_XSERVTransmkdir: ERROR: euid != 0,directory /tmp/.X11-unix will not be created.` ⇒ 只能 TCP |
| ② 不设 mesa-only 的 EGL/GLX 变量，Xvfb 上会 core dump | **本机不成立** | 负对照：把 `__EGL_VENDOR_LIBRARY_FILENAMES` / `LIBGLX_VENDOR_LIBRARY_NAME` / `LIBGL_ALWAYS_SOFTWARE` 全 unset，游戏照样跑满 15 s（`rc=124`，不是 139），画面正常。⇒ 这条**机器相关**（本机没有 `libEGL_nvidia`）。变量照设无妨，但别写成"必须" |
| ③ Vulkan 不可用 ⇒ Godot 回落 OpenGL 3 | **真的** | 日志原文见下 |

**关于引擎版本横幅**：本机这份 2.0.60 的 stdout/stderr **都没有**打出 `Godot Engine v4.x…` 那一行
（只有 godot-rust 那一行，它给出 `runtime v4.7.stable.official`）。引擎版本 + commit 用
`./Graphwar.x86_64 --version` 拿到：`4.7.stable.official.5b4e0cb0f`，与 §35.1 记的**一字不差**。
⇒ 少一行横幅不影响任何结论，但**别把它当成"跑起来了"的必需证据**。

日志关键行（原文）：

```text
Initialize godot-rust (API v4.6.stable.official, runtime v4.7.stable.official, safeguards balanced)
ERROR: Condition "err != VK_SUCCESS" is true. Returning: ERR_CANT_CREATE
   at: _initialize_devices (drivers/vulkan/rendering_context_driver_vulkan.cpp:836)
WARNING: Your video card drivers seem not to support the required Vulkan version, switching to OpenGL 3.
     at: DisplayServerX11 (platform/linuxbsd/x11/display_server_x11.cpp:7124)
...
src/graphwar_function_input.rs:519: dt:0.010279712      ← 这三行只有进了对局才有
src/entity_drawer.rs:585: dt:0.008586611
src/effect_drawer.rs:507: dt:0.048114464
```

真帧（1920×1160 根窗口截图，**不入库**，可一条命令重放）：主菜单四项 + 右下角 `2.0.60` →
`Campaign` → 关卡节点 → `Level 0` 弹窗 `Start` → 对局界面（右侧 `Fire!`、底部 `f(x) =`）→
输入 `sin(x)+3` 后**长按** `Fire!` 2 s → 图上出现曲线并开始播放弹道。
`f(x)=` 输入框里出现 `sin(x)+3`、右下角出现 `- 5.665` —— **这个 `- 5.665` 后面很关键**（§35.8.3）。

#### 35.8.2 ② 填充者 = `calculate_function`（RVA `0x3c2530`）：**地址完全一致**

证据逐条重放（工具全部取自本仓库：`tools/addr_probe.py`、`objdump`、frida 运行时）：

| 证据 | 本机结果 |
|---|---|
| `.eh_frame` FDE | `calculate_function` = `[0x3c2530, 0x3c2bb9)`（ghidra 表给 `..0x3c2bc0`，与 §35.2 一致）；`execute_results` = `[0x472470, 0x473513)` |
| 函数指针槽 `0x9bac98` | **运行时**读出 = `base + 0x3c2530` ⇒ `gw+3c2530`（`hooks.slot_9bac98.target_rva`） |
| 两个调用点都是**间接**调用 | `3c30c1: ff 15 d1 7b 5f 00  call *0x5f7bd1(%rip)  # 9bac98`（返回地址 `3c30c7`）<br>`3e2d35: ff 15 5d 7f 5d 00  call *0x5d7f5d(%rip)  # 9bac98`（返回地址 `3e2d3b`）⇒ 两个调用方走的是**同一个函数指针槽**，不是两条 `call rel32` |
| 逐点求值调用点落在它体内 | 求值器返回地址直方图（run4，一发）：`gw+3c2995: 30128`、`gw+3c2a21: 11842`，两者都 ∈ `[0x3c2530,0x3c2bb9)`（`addr_probe.py fde` 判"NOT an FDE function start"＝在体内）；其余全是求值器**自递归** `gw+4eba49/4eba6b/4eba91` |
| 每一发里它恰好 2 次 | `samplerHist` = `{gw+3c30c7: 1, gw+3e2d3b: 1}`（四次抓取都一样） |

**两个调用方是谁**：用 `tools/addr_probe.py locs`（`panic::Location` 投票）核了一遍 —— 这是 §35.4 没做的一步：

| 返回地址 | 所在函数（FDE） | `locs` 投票 |
|---|---|---|
| `gw+3c30c7` | `[0x3c2bc0, 0x3c5c4d)` | `src/graphwar_shot_calculator.rs` ×12、`src/graphwar_obstacle_manager.rs` ×2 |
| `gw+3e2d3b` | `[0x3e2830, 0x3e5909)` | `src/graphwar_remote_event_handler.rs` ×89（其余 7 个模块各 1–5 票） |

⇒ §35.4 说 `0x3e2830` 是 `graphwar_remote_event_handler.rs` 那个大函数 —— **对**（89 票）。
但**不是它那次调用的返回值进了队列**，见 §35.8.4。

#### 35.8.3 ③ `exact` / `ulp1`：**一致**，判据还能更严

先把 §35.5 的口径原样重放（全部在游戏进程内算，`xmm0` 低 8 字节 = 一个 f64 的位模式）。
本机四次抓取，**每一条都对上 §35.5 的形状**：

| 抓取 | 对局里实际生效的 `f(x)` | 点数组 len | 队列 tag | `exact` | `ulp1` | `miss` | miss 的 `i` | `pairWho` |
|---|---|---|---|---|---|---|---|---|
| run1 | `sin(x)+3` | 4943 | `0x2000` | **4942** | **0** | 1 | **0** | `{gw+3c2995: 4942}` |
| run2#0 | `sin(x)+3` | 3548 | `0x1000` | **3547** | **0** | 1 | **0** | `{gw+3c2995: 3547}` |
| run2#1 | `sin(x)+3x/2` | 929 | `0x400` | **928** | **0** | 1 | **0** | `{gw+3c2995: 928}` |
| run4#0 | `sin(x)+3` | 5921 | `0x2000` | **5920** | **0** | 1 | **0** | `{gw+3c2995: 5920}` |

对照 §35.5 那三行（4483 / 3589 / 4063 点，都是 `exact=n-1`、`ulp1=0`、`miss=1`、`pairWho` 全 `gw+3c2995`）：
**口径与结论完全一致**。唯一 miss 也确实是 `i=0`（炮口/出生点）—— 本机还多一条正向证据：
`calculate_function` **入口**的 `xmm0`（第一个 f64 参数）位模式与数组第 0 点的 `x` **逐位相同**，
说明第 0 点是**调用方传进去的起点**，不是求值出来的。

**为什么"逐位相同"是结构上必然的**（从 `graphwar_curve.py` 的实现 + `gw+3c2995` 的位置看出来，**推断**）：
`calculate_path` 每接受一个点，写进数组的 `y` 就是**二分探针那次调用** `f(mid)` 的返回值本身，
不存在"再算一遍"的步骤；而每个点的 `(x,y)` 双位命中都落在 `gw+3c2995`（探针那一次），
`gw+3c2a21`（半步校验 `f(mid2)`）的值**一次都没进数组** —— 与 §35.5 的 `pairWho` 完全吻合。

**更严的一条：不用 frida 的判定，直接拿分析仓库的参考采样器逐位重算整条曲线**：

```text
起点 = 该发数组第 0 点 (x0,y0)；函数文本 = 该发真正生效的表达式；step = 0.01（弦长）
calculate_path( f(x) + c , x0, y0, 0.01 )   逐点比较 8 字节位模式
```

| 抓取 | len | 点数组实测 `c = y0-f(x0)`（原值） | 与实测逐位相同的常数 | 逐位相同 / 总数 | 参考采样器总点数 |
|---|---|---|---|---|---|
| run1 | 4943 | `-5.665409572276467` | **`-5.665`** | **4943 / 4943** | 4943（= 实测） |
| run2#0 | 3548 | `3.9445763390886914` | **`3.945`** | **3548 / 3548** | 3548（= 实测） |
| run2#1 | 929 | `12.527594625625152` | **`12.528`** | **929 / 929** | 929（= 实测） |
| run4#0 | 5921 | `-5.956945340257709` | **`-5.957`** | **5921 / 5921** | 5921（= 实测） |

⇒ 两条独立结论：

1. **点数组 = 参考采样器从同一起点、同一常数算出来的那条曲线，逐位、连长度也一致。**
   这比"y 在求值器返回值集合里出现过"强得多（后者只是集合成员，不排除撞值）。
2. **新事实（§35.0 / §35.5 都没写）：真正用于开火的那个常数是 `round(y0 - f(x0), 3)`，不是原值。**
   四发全中，而且 run2#0 这条**排除了截断**：`c = 3.9445763…`，用 `3.945` 逐位全中、
   用 `3.944`（截断）**3548 个点里只有第 0 点对上**。
   这个数正是**游戏 UI 右下角印出来的那个数**（`- 5.665` / `+ 3.945` / `+ 12.528` / `- 5.957`）。
   * 档：**观测**（四发、逐位；用 `c = 原值` 时四发**各差 n-1 个点**，差量恒等于 `|c_raw - round3(c_raw)|`）。
   * **机制未知**：是"UI 把 3 位小数的常数拼进表达式文本"，还是"采样前先量化"——**缺**：把修正量写入
     采样输入的那条指令（在 `src/graphwar_shot_calculator.rs` 里）。
   * 影响：现有 `graphwar_curve.shot_points()` 用的是**原值** `correction()`，与真机**不符**；
     这是 **1e-4 量级**的差（不是 1 ulp），但它会让**整条曲线每一个点都不等**。
     ⇒ 与 §32 / §25 的 1 ulp 悬案**不是同一件事**（量级差约 8 个数量级），但"取末点 → 报文"之前
     这一段现在已经**没有**未解释的差。

#### 35.8.4 队列里那条 0x20 字节记录到底是谁写进去的（**与 §35.4 末句不一致**）

先补一件 §35.4 **没能留下证据**的事：仓库里的 `tools/frida_point_array.js` 原来**不记录返回值**，
只记 `xmm0` 的低 8 字节 —— 也就是说"返回值就是这个队列元素（ptr 同一个）"这条**无法用入库脚本重放**。
本轮给探针**加了两处纯读取**（不改内存，见 §35.8.7），于是能直接读到 `calculate_function` 的返回值：

```text
onLeave 时 rax == rdi（= 调用方给的隐藏 sret 指针）⇒ 这个记录是**留在内存里**返回的，读 [rax] 前 0x18 字节：
    +0x00 kind(u64)   +0x08 ptr(u64)   +0x10 len(u64)
xmm0 低 16 字节 = {kind, ptr}（与 §35.4 第 1 条的说法一致）
```

四次抓取，**队列里那条点数组记录的 ptr，永远等于"返回地址 `gw+3c30c7` 那一次调用"的返回值**：

| 抓取 | 队列点数组元素 `{kind, ptr, len}` | `gw+3c30c7` 那次的返回 | `gw+3e2d3b` 那次的返回 | 队列 = 哪一次 |
|---|---|---|---|---|
| run1 | `0x2000, 0x7fae841410b0, 4943` | 未记录指针；`realloc` 链最后一次返回 **`0x7fae841410b0`** | `realloc` 链最后一次返回 `0x2a6bf5f0` | **`gw+3c30c7`** |
| run2#0 | `0x1000, 0x7f0404142e10, 3548` | `0x1000, 0x7f0404142e10, 3548` | `0x1000, 0x1a27baf0, 3548` | **`gw+3c30c7`** |
| run2#1 | `0x400, 0x7f0404144240, 929` | `0x400, 0x7f0404144240, 929` | `0x400, 0x1b2a0d00, 929` | **`gw+3c30c7`** |
| run4#0 | `0x2000, 0x7f0514602090, 5921` | `0x2000, 0x7f0514602090, 5921` | `0x2000, 0x2ab2b500, 5921` | **`gw+3c30c7`** |

`Vec` 扩容链也**独立**指向同一个结论（run1，`realloc` 的调用方栈）：

```text
16384/32768/65536/131072 → gw+460fff ← gw+460c5a ← gw+4ebb0d ← gw+3c2b23 ← gw+3c30c7   最后一次返 7fae841410b0 = 队列 ptr
16384/32768/65536/131072 → 同上                                        ← gw+3e2d3b   最后一次返 2a6bf5f0     ≠ 队列 ptr
```

⇒ **§35.4 末句"`gw+0x3e2d3b`（`graphwar_remote_event_handler`）这一份的返回值进了开火队列"与本次四次观测不符。**
按观测应该说：**进队列的是 `graphwar_shot_calculator.rs` 那条路径（返回地址 `gw+0x3c30c7`）的返回值**；
`remote_event_handler` 那次也算了同样的一条曲线（同 kind、同 len），但它的 `Vec` 落在**低位堆**上、
ptr 不同、没进队列。**它算的那条曲线拿去干什么了 —— 未知**（缺：对 `gw+0x3e2d3b` 前后指令的读点观测）。

顺带一条（**观测**）：队列元素第 4 个 eightbyte（`+0x18`）**不是** `calculate_function` 写的 ——
它的 sret 缓冲只写到 `+0x17`（前三个 eightbyte 是 `{kind, ptr, len}`），`+0x18` 在**返回值里**是
调用方那段栈上的残留，而且**与调用点相关**（三次抓取完全一致）：

```text
gw+3c30c7 那条路：0x00007f04000000e7 / 0x00007f04000000e7 / 0x00007f05000000e6   （低 32 位 = 231/231/230）
gw+3e2d3b 那条路：0x8000000000000025 / 0x8000000000000025 / 0x8000000000000025   （低 32 位 = 37）
```

而**入队后**的队列元素 `+0x18` 是 `0x00000000000000e7 / …e7 / …e7 / …e6`（低 32 位 `231/231/231/230`）——
**低 32 位与上面 `gw+3c30c7` 那条路的残留逐次相同**，高 32 位是 0。
⇒ 入队路径复制的是 `{kind, ptr, len}` + 一个有值的小 u32（`230/231`），那个 u32 从哪来 —— **未知**
（缺：写队列元素的那几条指令）。**别把它当成"恒定 230"**（§35.8.5 第 2 条）。

#### 35.8.5 与 §35.3 / §35.4 不一致的两处（就地记下，不改旧文）

1. **"x 步长恒 `0.00875`"（§35.4 开头）不成立。**
   本机 run1 的 4943 个点里，相邻 `x` 差只有**三个值**：

   ```text
   0.007500000  × 2831      0.008750000  × 1439      0.009375000  × 672      （合计 4942 = n-1）
   ```

   三个值正是 `0.01 × (3/4, 7/8, 15/16)` —— 也就是**弦长 0.01 的二分格点**（`calculate_path` 对弦长做二分，
   接受条件 `|chord - step| < 0.001`，dx 落在 `x + step·(1-2^-k)` 这种格上）。
   ⇒ 正确说法是：**弦长恒 `0.01`，x 步长在这三个格点里挑**，不是"恒 0.00875"。
   §35.5 里"下一个点 x = 前一个 + 0.00875"同理，只对一部分点成立。
   （四发的点数组都能被 `calculate_path(..., step=0.01)` 逐位复现 ⇒ 这条是**逐位**级，不是推断。）

2. **"`+0x18` = 230（`u64`），四次抓都是 230"（§35.3）不成立。**
   本机队列元素实测 `+0x18 = 231 / 231 / 231 / 230`（四发）。它**仍然与 len 无关**（三次 231 对应
   4943 / 3548 / 929 点），但**不是一个跨运行恒定的值**。是什么 —— **未知**（缺：写它的那条指令；
   猜"某种 tick 计数"没有任何证据）。

#### 35.8.6 还没解决的

* ④（同坐标不同序号的"序号"）**仍然没有样本**，卡点与 §35.6 完全相同：只打通了 campaign Level 0，
  场上没有 obstacle；LAN 大厅加不出第二个玩家。**缺**：带 `DestroyObstacle` 的对局
  （大厅加机器人/第二个玩家的正确点击路径，或一份带障碍的存档/关卡）。
  * 本轮多看到一条**线索**（不入结论）：开火队列里除点数组外还有 1 个元素，`kind = 0x8000000000000002`，
    `+0x08` 是一个随发变化的 f64，`+0x10/+0x14` 是**两个 u32**（四次抓是 `(231,232) / (231,232) / (231,232) / (230,232)`）。
    这形态很像"两处 `EffectInfo` 的 id 来自同一个原子计数器"，但**这里没有障碍物** ⇒ 它跟 ④ 是不是
    一回事 —— **未知**。
* 修正量"四舍五入到 3 位"是**哪一步**做的（UI 拼字符串？采样前量化？）—— **未知**（§35.8.3）。
* 两次 `calculate_function` 的**第二个 f64 参数**（`xmm1`）是一个与本次开火有关、两次调用**完全相同**的数：
  `sin(x)+3` 两发是 `25.003612808975177` / `25.006206494882143`，`sin(x)+3x/2` 是 `1.065487808975671`。
  四次都能对上"**末点 x + 末点那一步的 dx**"（即**第一个被平面边界拒掉的候选点**的 x）⇒ **推断**：
  它是调用方**先算好**的"曲线离开平面处的 x"。机制 **未知**（缺：调用方的反汇编）。`rdx = rcx = 0`（四次一样）。

#### 35.8.7 样本入库 + 复跑

**入库的样本**（本节唯一可长期引用的原始数据）：

```text
fixtures/frida_point_array_sample.jsonl      5 行 / ≈69 KB
 第 1 行  provenance：构建 sha256、引擎、frida 用法、点击/长按流程、怎么截的、原始条数
 第 2–5 行 四次开火（run1 / run2#0 / run2#1 / run4#0），每行含
            · 队列原文（两个 0x20 元素，逐字节）+ qptr/qlen/qcap
            · 点数组 tag/ptr/len/lenOff + 头 8 尾 8 点 + classify（原样）+ 完整 hex 的 sha256
            · 点坐标：最短那发（929 点）**全留**，其余保留**前 48 + 后 8** 点
            · calculate_function 两次调用的：返回地址、rax、xmm0 全 16 B、xmm1、起点 x、返回记录、调用栈
            · 求值器返回地址直方图（本发增量 + 累计）
            · Vec realloc 扩容链
            · `derived_by_parent_repo_oracle`：**派生、非观测** —— 参考采样器对该发给出的
              `c_raw / round3 / trunc3` 与逐位命中数（§35.8.3 那张表就是这里算的）
```

**原始条数**（供核对"截了哪一段"）：三个进程、四次开火，原始 JSONL 分别是
**50 行 / 2.70 MB**、**69 行 / 3.85 MB**、**68 行 / 5.62 MB**（大头是 `beat` 心跳与点数组的完整 hex）。
入库文件把 `evals`/`samplers` 的批量行、`beat`、以及大数组的 158 KB hex 全部丢掉
（hex 用 sha256 + 首尾点代替），保留的是**支撑 ①②③ 的最小充分集**。
文件里**没有任何文件系统路径、没有用户名**；里面所有地址都是**每次运行随机**的 ASLR 值，
只有 `gw+<RVA>` 与 `slot_9bac98.target_rva` 有长期意义。

复跑（与 §35.7 同一条命令，只多一个 `--game`）：

```bash
Xvfb :92 -screen 0 1920x1160x24 -listen tcp -ac -nolisten unix &
<分析仓库根>/.venv/bin/python tools/frida_point_array.py \
    --game <含 Graphwar.x86_64/.pck/libgraphwarrust.so 的目录> \
    --data-dir <可写目录> --out <绝对路径>/run.jsonl --display 127.0.0.1:92 \
    --settle 12 --tail 40 [--extra-shot]
```

**本轮为了让上面几条能被重放，改了 `tools/frida_point_array.*` 三处（全部只读，不改游戏内存）**：

| 改动 | 为什么必须改 |
|---|---|
| `hooks` 增加 `slot_9bac98`（运行时读函数指针槽，并换算成 `gw+<RVA>`） | README 本来就这么写，但代码里**没有**；而 §35.4 的"唯一入口"和两个 `call *0x9bac98(%rip)` 都要它自证 |
| `calculate_function` 的 `onLeave` 增加返回记录 `[rax, rax+0x18)` 与 `xmm0/xmm1` 全 16 B | §35.4 第 1 条（ptr 同一个）**无法用原脚本重放**：原脚本只记 `xmm0` 低 8 字节，JSONL 里从来没有指针 |
| 队列元素的 dump 判据从 `pv.compare(base) > 0` 改成"该地址落在**可读映射**里" | 原判据假设点数组一定在模块基址之上。**实测一次（run3）整条点数组位于 `0x41679010`（glibc brk 堆），被静默跳过、`classify` 一条都不出** —— 这种"静默零结果"最危险。改后可读性判据恢复正常；被跳过的元素现在也会**显式记一条 `skipped`** |

另外 `--extra-shot` 原来会在**旧文本后面接着打**（单击输入框只是放光标），
实测想打 `x/2`、实际生效的是 `sin(x)+3x/2`（run2#1 的表达式就是这么发现的）。
现在点完输入框先 `ctrl+a` 全选再打字。**这一条也解释了为什么 run2#1 的 `f(x)` 不是 `x/2`。**

## 36. 「爆炸点 `pos_y` 来自网络」= **证伪**；1 ulp 残差按来源拆成三份（2026-11-08，`dispatch/26`）

> 本节接手第四轮报告 §3 ③ / §33.4 / §33.9 的三条未知：
> 「那 154 批 1 ulp 会不会是**别的机器算完、从网上收来的 f64**」。
> 结论：**不是**。两条互相独立的证据（一条逐指令、一条载荷统计，都不依赖对方的假设），
> 外加一条**定量**的反证：残差率与"有没有远端"无关。
>
> 起点 `CG@0ea7729`，分支 `agent/pos-y-from-network`，私有 `CARGO_TARGET_DIR`，同一份 401 回放快照。

### 36.1 一句话（五条）

1. **证伪。** 爆炸点 `(pos_x, pos_y)` 是**客户端自己点数组的末元素**（站点 A，`0x472865`→`0x47286b`），
   不是点位表里那个由线上写入的位置 ⇒ 「`pos_y` 是网络字段」**不成立**。
2. **载荷级判别**（新增、可复跑）：`EffectInfo` 的四种 `effect_type` 里，只有**非爆炸**的三种
   `pos` 逐位等于某个实体的 `PosInfo.pos`（`DeathEffect` 519/536 = 96.8%、`SparkEffect` 54/54、
   `AshEffect` 363/363 = 100%，**这三类就是正对照**）；`ExplosionEffect` 只有 **55/1290 = 4.3%**
   （重合的都是"弹道正好落在某个士兵身上"）。⇒ 两种生产者**从载荷上就能分开**。
3. **定量反证**：末位残差率在**战役单机**（`GameNameInfo = "Level N"`，无远端）是
   **175/609 = 28.7%**，在**联机房间**是 **81/332 = 24.4%** —— 有没有远端**没有影响**。
   「另一台机器算出来的数」解释不了单机里那 175 批。
4. **1 ulp 不是"同一份代码差一位"这一件事**：按可指认的量拆开（`Normal` 740 发），
   498 发逐位相同、**43 发是我方最后采样点的 `x` 差 1–2 ulp（y 是它的下游）**、
   **46 发是回放自己那条点的 `(x,y)` 不自洽**、153 发两个都不是。
   ⇒ **不许再把 `Y` 桶当成一个机制**（§25.3 的三条否证仍然有效，但分母被本节拆了）。
5. **不变的东西**：三轴护栏一个没退（§36.8），`src/` **零改动**。

### 36.2 静态：`ExplosionEffect` 只能是站点 A（逐指令，自己 objdump 复验）

`EffectType` 的 **0 = `ExplosionEffect`**（`docs/07` §5.2：x86 `VARIANTS` 顺序 + arm64
`visit_str` 两套独立证据）。而 tag 44 的两个生产点（§31.3）里：

| 站点 | `effect_type` 怎么写 | 指令 |
|---|---|---|
| **A** `0x4729d6` | **常量 0** | `0x472948  mov byte ptr [rsp + 0x84], 0` |
| B `0x472e4d` | 记录里的一个字节 | `0x472d75  movzx eax, byte ptr [rcx + r8*8 - 4]` → `0x472e2e mov byte ptr [rsp + 0x90], al` |

⇒ 只有 A 能产出 `effect_type = 0`；B 产出的是另外三种（载荷证据见 §36.3）。

**A 的 `pos` 从哪来**（这是本节的核心两条指令）：

```text
0x472859  mov   rcx, r13
0x47285c  shl   rcx, 4                    ; r13 = 点数，步长 16 B = 一个 (f64,f64) 点
0x472860  mov   rdx, qword ptr [rsp + 8]  ; 点数组 ptr
0x472865  movdqu xmm0, xmmword ptr [rdx + rcx - 0x10]   ; ★ points[len-1]
0x47286b  movdqa xmmword ptr [rsp + 0xf0], xmm0         ; ← 后面写进 EffectInfo 的 pos
```

点数组的填充链已经钉死（§33.3 六跳、`dispatch/25` 真机四证）：`calculate_function` → … →
`self.results` → `HashMap` → A 读末元素，**每一跳都是 16/24/32 B 搬运，零算术**。
⇒ **`pos` 的数值在生产它的那个进程里就地算出来**，没有任何反序列化路径参与。

⚠️ 这一条**不能**独自证伪"服务端下发"：若 `execute_results` 跑在**权威端**（联机房的房主）
而回放录在**另一个客户端**，值仍然"过了线"。所以还需要 §36.3 / §36.4 那两条。

### 36.3 载荷级判别：点位表 vs 点数组（**新判据**，带正对照）

**为什么这是一个判据**：点位表（`GraphwarPlanePosManager`）的写侧**只有两处**
（§33.4，`set_position` 全 `.text` 两个 `bl`）：`LevelGenerator::place_entity` 与
`RemoteEventHandler::handle_server_remote_event`——**后者就是线上入口**。
⇒ 如果爆炸点走了"服务端下发"这条路，它就必须**落在点位表上**；落在点位表上就意味着
`(pos_x,pos_y)` 逐位等于**某个实体的 `PosInfo`**。

```bash
$PY tools/posy_source_attrib.py --dir $R         # 退出码 0 = 正对照全部成立
```

| `effect_type` | `pos` 逐位等于某个 `PosInfo.pos` | 条数 | 判读 |
|---|---|---|---|
| `ExplosionEffect` | **55 / 1290 = 4.3%** | 1290 | 站点 A：客户端点数组 |
| `DeathEffect` | **519 / 536 = 96.8%** | 536 | 站点 B：点位表（**有**线上写侧） |
| `SparkEffect` | **54 / 54 = 100%** | 54 | 站点 B |
| `AshEffect` | **363 / 363 = 100%** | 363 | 站点 B |

**正对照（`AGENTS.md` 合并纪律 A.4）**：如果这个比法不灵，非爆炸的那三类也不会命中。
它们全部 ≥96.8% ⇒ **比法有效**，那么 `ExplosionEffect` 的 4.3% 就是一个**有意义的负结论**。
（4.3% 里的 55 条是巧合：弹道端点正好落在某个士兵的 `PosInfo` 上，即"打中了"。）

⇒ **"点位表 = 线上写的那张表"这条路确实存在，而爆炸点不在上面。**

### 36.4 第二条独立证据：残差率与"有没有远端"无关

回放类别由 `GameNameInfo` 决定（**不猜**）：

* `campaign`：出现过 `Level N`（战役关卡，单机/AI；`TimeModeInfo = TimerInf`、场上有 `target`）；
* `online`：出现过房间名（`US Room 1` / `EU Room 3` / `2v2` …）；
* `unknown`：只有默认名 `Graphwar Game`。

```bash
$PY tools/posy_source_attrib.py --dir $R --conv conv.jsonl      # conv.jsonl 见 §36.7
```

| 回放类别 | 份数 | 结算批 | 末位差批 | 末位差率 |
|---|---|---|---|---|
| `campaign`（无远端） | 205 | 609 | **175** | **28.7%** |
| `online`（有远端） | 151 | 332 | **81** | **24.4%** |
| `unknown` | 45 | 139 | 35 | 25.2% |

**读法**：如果那一批 `pos_y` 是"另一台机器算出来的 f64"，那么**只有联机回放**才该有残差，
单机回放（同一台机器、同一个进程、同一份代码）应当 0。实测**反过来了**：单机 28.7% > 联机 24.4%。
⇒ 「外来值」不是解释；残差在**任何**回放里都以 ~1/4 的比例出现。

### 36.5 数值侧：`pos_x` 能逐位相同、`pos_y` 差 1 ulp 的**分解**

`Normal` 局（`y = f(x) + c`，740 发；`DiffEq` 局 `y` 来自 RK4 积分，本节的"回代"判据对它不适用）：

| 组 | 判据 | 条数 | 说明 |
|---|---|---|---|
| 逐位相同 | 我方值 = 回放值 | **498**（67.3%） | — |
| 我方 `x` 差 | 回放 `y == f(回放的 x)`，而我方 `x` 差 1–2 ulp | **43**（5.8%） | **`y` 的差是 `x` 的下游** |
| 回放 `(x,y)` 不自洽 | 我方 `x` 能算出回放的 `y`，但回放的 `y ≠ f(回放的 x)` | **46**（6.2%） | 原版那条点的两个坐标**不是同一个 `x` 上算的** |
| 都不解释 | 三个候选 `x` 都不给回放值 | 153（20.7%） | 几何/文本层面（极端函数占多数） |

判据工具（**新增**，只加不改）：`examples/posy_mech.rs`

```bash
$CARGO_TARGET_DIR/release/examples/posy_mech --dir $R --json mech.jsonl
```

它对每发算同一份打印文本在**三个不同 `x`** 上的值（回放 `end_x` / 我方采样停点 /
**回放自己的 `pos_x`**），以及对**所有常数各挪 ±1 ulp**的值，逐位比 `pos_y`：

| 候选 | 逐位命中（`Normal` 740 发） | 结论 |
|---|---|---|
| `eval(文本, end_x)` | 532 | 不是我们现在交付的那个（见下） |
| `eval(文本, 我方采样停点)` | **544** | **= 现行交付口径**（`game::hole_point` 用 `stop.0`） |
| `eval(文本, 回放的 pos_x)` | 541 | 回放那条点在多数情况下是自洽的 |
| 常数各 **+1 ulp** / **−1 ulp**（在两种 `x` 上） | 79 / 61 / 75 / 68 | **否掉**"常数整体偏 1 ulp" |

**由此得到三条可检验的判据**（全部已在上面跑过，不是设想）：

* **M1「取哪个 `x`」**：把交付口径从 `stop.0` 换成 `end_x` 会**变差**（544→532）；
  换回放的 `pos_x` 也不会更好（541）。⇒ `x` 的取法**基本已经对了**，
  余下 43 发是"我方最后采样点的 `x` 差 1–2 ulp"，那是**采样器几何**的账，不是求值器的。
* **M2「常数 `c` 的最后一位」**：±1 ulp 整体平移**只命中 10%**（75/68）⇒ 不是系统性偏移，
  只可能是**逐个值不同**的 residue（十进制→double 的那一步）。**判据**：反汇编
  `calculate_corrected_function` / `add_offset`（arm64 `.symtab` 有名字），
  看偏移量是 `f64` 本身还是"3 位小数文本再解析"；再把 `c` 的十进制→double 结果逐位对一遍。
* **M3「libm 最后一位」**：二进制**自己不实现** `exp/log/sin/cos/tan/pow/log10`
  （`objdump -T libgraphwarrust.so` 里是 `UND (GLIBC_2.2.5)`），glibc 的 libm 按 CPU
  走 **IFUNC**（FMA/AVX 变体）⇒ **同一份二进制换台机器，超越函数的最后一位会变**。
  **判据**：把求值器的 `exp` 换成另一个 IFUNC 变体，只应影响含超越函数的发。
  **今天测到的是反证**：把 `Normal` 740 发按"函数里有没有超越函数"拆开，
  "被 `x` 取法解释"的比例是 **75.1%（纯 `+-*/`）vs 76.6%（含超越函数）**——一样 ⇒
  **M3 不是主因**（它最多解释一个子集）。
* **M4「x87 双舍入 / FMA 收缩」**：对**一次加法**这一类（`fn = "x"`、`"-x"`）
  **在数学上不可能**产生差异：两个 double 的和用 64 位尾数的 x87 也能**精确**表示
  （再舍到 double 是一次正确舍入），FMA 又需要乘法。⇒ 那 23 发"纯 `+-*/`、`x` 逐位相同、
  仍差 1 ulp"（含 `fn="x"` 的 4 发）**不是浮点环境问题**，只能来自**被加的那个常数**。

### 36.6 所以下一步改什么（/ 为什么不该改）

| # | 建议 | 依据 | 本轮做没做 |
|---|---|---|---|
| 1 | **不要**把爆炸点改到点位表（站点 B）上 | §36.3：站点 B 的 `pos` 是实体位置，爆炸点不是 | 不做（会砸掉 1290 发） |
| 2 | **不要**再用"求值器最后一位"解释 `Y` 桶 | §32.5 已证 + §36.5 的分组 | — |
| 3 | 该改的是**采样器最后一点的 `x`**（43 发 = 5.8%）与**偏移常数的十进制→double**（M2） | §36.5 | 未改：`hole_point`/采样器不在本任务允许清单里（只允许 `src/game.rs` 一小段），且改之前要先拿到 M2 的反汇编证据 |
| 4 | `Y` 桶的门槛值**不要**再当成"一个 bug 的进度条" | 它至少是三份（§36.5 表） | 文档层面已改：本节 |

### 36.7 复现（全部只读语料/二进制）

```bash
export CARGO_HOME=<ws>/.reference/cargo-home CARGO_TARGET_DIR=<你的私有目录>
export PY=<ws>/.reference/venv/bin/python
R=<ws>/.reference/replays/gw2-replays-snapshot/replays

# ① 静态两条指令（站点 A 的 pos = points[len-1]；effect_type = 常量 0）
$PY tools/addr_probe.py dis 0x472840 --length 0x40
$PY tools/addr_probe.py dis 0x472940 --length 0x18
$PY tools/addr_probe.py dis 0x472d70 --length 0x18     # 站点 B 的 effect_type 来自记录字节

# ② 载荷判别 + 类别拆分（退出码 0 = 正对照成立）
$CARGO_TARGET_DIR/release/examples/replay_attrib --dir $R --drive conv --policy strict --json conv.jsonl
$PY tools/posy_source_attrib.py --dir $R --conv conv.jsonl

# ③ 机制判别（三个 x + 常数 ±1 ulp）
$CARGO_TARGET_DIR/release/examples/posy_mech --dir $R --json mech.jsonl

# ④ libm 是外部的（不是游戏自己实现的）
objdump -T <ws>/.reference/steam/2026-08-28/libgraphwarrust.so | grep -E 'UND.*(exp|log|sin|cos|tan|pow)'
```

### 36.8 验收（本分支，同一棵树自己复算）

| 轴 | 指标 | 值 | 命令 |
|---|---|---|---|
| `fire` | 内容命中 | **608 / 1080** | `replay_attrib --drive fire --policy default` |
| `batch` | 内容命中 | **643 / 1080** | `replay_attrib --drive batch --policy default` |
| `batch_conv` | 种类序列 / 内容 | **1075 / 1080 · 633 / 1080** | `batch_conv --policy strict` |
| 延迟段（`batch`） | 死亡集合 / 挖洞点 | **366 / 385 · 206 / 293** | `$PY tools/bit_exact_attrib.py --json b.jsonl` |
| `cargo test --offline` | 通过/失败 | **1569 / 0**（1 ignored） | — |
| `coverage_matrix --check` | — | **OK**（527 个函数、逐位 88 / 观测 439） | — |

⚠️ 本任务书写的是"挖洞点 205/293"，**实测 206/293**（与第四轮报告 §2 一致）。
按 `AGENTS.md` §0.6 记在这里：**不是本节改出来的**，是任务书的数字早了一步。

### 36.9 仍未确定的（诚实清单）

| # | 未定 | 缺什么 | 档 |
|---|---|---|---|
| 1 | 那 46 发"回放 `(x,y)` 不自洽"的成因 | 原版那条点在**拼装**时用哪个 `x` 算 `y`（`calculate_function` 里 `stp d9,d12` 之前有没有对 `x` 做过第二次舍入） | **推断**（判据已给出） |
| 2 | 偏移常数 `c` 的十进制→double 是否逐位正确（M2） | `calculate_corrected_function` / `add_offset` 的逐指令 | **未知** |
| 3 | 153 发"三个 `x` 都不解释" | 多数是极端函数（`999999999999999x`）；要逐发定位得先有 §36.9#1 的答案 | **未知** |
| 4 | 联机回放的 `EffectInfo` 是不是**权威端**算的（房主 vs 加入者） | 一份"同一发、两个客户端各录一份"的回放对（本快照里没有） | **未知**（但不影响本节结论：单机里也有 28.7%） |

## 37. **真机造了一个带障碍的对局**：一炮只产生 1 条挖洞指令；"两条报文"来自**两个 ObstacleManager 实例**（2026-11-07，`dispatch/27`）

> 任务书：`dispatch/27-obstacle-room.md`（P1）。**本节全部是运行时观测**：
> frida 17.26.1 挂在真游戏 `Graphwar II` Linux **2.0.60** 进程里（`libgraphwarrust.so` sha256
> `9a538115…`，= 逆向目标那份），Xvfb + xdotool 驱动，**在 Campaign Level 2（中心单障碍）里打了 9 炮，其中 8 炮撞到障碍**。
> 脚本：[`tools/obstacle_room.js`](../tools/obstacle_room.js)（下断）+
> [`tools/gw2_drive.py`](../tools/gw2_drive.py)（驱动）+ [`tools/obstacle_room_extract.py`](../tools/obstacle_room_extract.py)（对齐入库）。
> 样本：[`fixtures/obstacle_destroy_pairs.jsonl`](../fixtures/obstacle_destroy_pairs.jsonl)（8 条，文本）。

### 37.0 一页结论

| # | 问题 | 答案 | 档 |
|---|---|---|---|
| ① | 带障碍的对局造出来了吗 | **造出来了**：Campaign `Level 2` = `AddObstacle (0,0) r∈{3.13, 4.41, 4.52}`（三局三值，`docs/10` §7 记的"中心单障碍 r∈[3,8)"对上）。9 炮里 **8 炮撞到障碍**（第 9 炮是按设计"可能打空"的探针），撞到的 8 炮**全部**触发挖洞 | **观测** |
| ② | 两条 `DestroyObstacle` 的坐标是否逐位相同 | **8 组里 7 组逐位相同；1 组 y 差 1 ulp**（`bfbf092aa99f64ae` vs `…af`）。x 与 radius（0.5）**8/8 逐位相同** | **观测（逐位）** |
| ③ | "序号"是什么字段 | 是挖洞指令记录 `+0x18` 那个 u64 的**高 32 位 = 采样点序号**；低 32 位 = **射手的 soldier entity_id**（本组 230/231，与点数组记录 `+0x18` 同一个值）。**线上 `DestroyObstacle`(tag 41) 报文里没有这个字段**（只有 `pos_x/pos_y/radius`） | **观测** |
| ④ | 为什么"同点两号" | **不存在两个号**：一炮只产生 **1 条**挖洞指令、**1 个**点序号。两条报文来自**两个 `ObstacleManager` 实例各排一次队**（服务端 `execute_results` 的 tag-42 生产点 + 客户端 `graphwar_remote_event_handler` 的 tag-42 消费点），各自桶里都只有 **1 条** | **观测** |
| ⑤ | 谁分配序号 | **不是分配的**：它就是点数组的下标，由 `calculate_shot_results` 在撞点处写出（`0x3c38fe mov [rax+rcx+0x1c], edx`），恒等于**被截断点数组的最后一个下标**（`len == idx+1`，8/8） | **观测（逐位）** |

⇒ **§31.5 / §31.9 第 1 条那条"两个不同点序号上坐标为何逐位相同"的候选解释被运行时否掉**：
那份语料里的重复报文不是"两个采样点"，而是**同一次挖洞被两条路径各发了一次**。详见 §37.5。

### 37.1 怎么造出来的（可复跑）

§35.6 记的卡点是"Campaign Level 0 场上没有障碍 / LAN 大厅加不了机器人"。本轮**绕开大厅**：

1. **解锁关卡**：`user://` 已经重定向到一个可写目录，直接写
   `XDG_DATA_HOME/godot/app_userdata/Graphwar/campaign.save` 内容为 `level_20`
   （格式见 `docs/10` §2），进游戏后 Campaign 界面上 21 关全部可点。
2. **进 Level 2**（`docs/10` §7 表：中心单障碍 / 1 士兵 / 1 靶 / 5 回合）：Campaign → `Level 2` → `Start`。
3. **瞄准**：障碍恒在原点，士兵位置**每局随机**。实测弹道是 `y = f(x) + c`，`c` 由"曲线过炮口"
   定（UI 上输入框右边那个 `+ 2.221` 就是 `c`，本轮截图逐个对过）。于是：
   **先用 `f(x) = x` 打一发探针**，从点数组首点读出炮口 `(x_m, y_m)`，再取
   `f(x) = (y_m/x_m)·x ⇒ c = 0`，弹道**必过原点** ⇒ 必撞障碍。按这套瞄的 5 炮 **5/5 命中**；
   另外 3 炮是探针（`x` / `x/2`），命中 2 炮、打空 1 炮 —— 合计 9 炮 8 中。
   （`tools/gw2_drive.py` 的 `calc_slope` / `type_tpl` 两个动作就是干这个的。）
4. **抓**：frida 下 7 个断点（见 §37.2），xdotool 长按 `Fire!`（`long_press` 按钮，单击无效，§35.1），
   每炮等 13 s，最后点返回箭头 + `Yes` 让游戏**优雅退出**（这样它才会把回放落到 `user://replays/<ts>`）。

```bash
export PY=<ws>/.reference/venv/bin/python
cd <ws>/.agent-work/cg/wt-27                       # 本任务的工作区（可丢弃）
printf 'level_20' > gamedata27/data/godot/app_userdata/Graphwar/campaign.save

# 一局：进 Level 2 → 探针 → 按探针算斜率 → 连打 3 炮 → 退出（写回放）
$PY tools/gw2_drive.py --js tools/obstacle_room.js \
    --plan tools/plans/level2_clean.json \
    --shots art/level2e --out art/level2e_run.jsonl

# 对齐成可入库样本 + 自洽性闸门（5 条恒等式，不过就返回 1）
$PY tools/obstacle_room_extract.py --frida art/level2e_run.jsonl --level "Level 2" \
    --fired '["x","0.030708*x","0.030708*x"]' \
    --replay gamedata27/data/godot/app_userdata/Graphwar/replays/<ts> \
    --run-base 2 --out fixtures/obstacle_destroy_pairs.jsonl --append
$PY tools/obstacle_room_extract.py --check --out fixtures/obstacle_destroy_pairs.jsonl
# → [check] 8 条样本，0 条不满足恒等式
```

⚠ 两个新踩的坑（都比 §35.1 那三条更靠前）：

* **Xvfb 自己也会撞 `libEGL_nvidia`**（不是只有游戏会）：不设
  `__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json` +
  `LIBGLX_VENDOR_LIBRARY_NAME=mesa` + `LIBGL_ALWAYS_SOFTWARE=1` 时，`Xvfb` **直接段错误退出**
  （`_XSERVTransmkdir` 之后 `libEGL_nvidia → swrast_dri`）。这三个变量要给 **Xvfb 进程**也加上。
* **回放只在优雅退出时落盘**：直接 `kill` 进程 ⇒ `user://replays/` 一个文件都没有；
  点返回箭头 → `Yes` 之后立刻出现（本轮 `1794066262`，1517 B）。踩了两次才发现。

### 37.2 下断的 7 个点（全部本构建逐条核对过）

地址 = **x86-64 ELF 虚地址（RVA）**；`base` 用 `Process.enumerateModules()` 找 `libgraphwarrust`。

| RVA | 是什么 | 实参（逐指令核过） |
|---|---|---|
| `0x3c2bc0` | `calculate_shot_results`（FDE `[0x3c2bc0,0x3c5c4d)`） | `rdi` = 返回 `Vec<Record0x20>` 的**隐式返回指针**（`0x3c2bd1 mov [rsp+0x1b8],rdi`）；`rsi` = 那发的结构体 |
| `0x3c38e4` | 同上体内**写挖洞指令**的第一条指令 | 现场 `[rsp+0x70]`=(x,y)、`r14`=记录字段、`[rsp+0x38]`=序号 |
| `0x3f20f0` | `queue_destroy_obstacle` | `rdi`=&mut self；`rsi`=&(x,y,u64)；`edx`=tick；`xmm0`=radius |
| `0x3f1b30` | `destroy_obstacle`（= 发一条 tag 41） | `rdi`=&mut self；`rsi`=&(x,y)；`xmm0`=radius |
| `0x3f1d99` | `handle_tick_event` 内：`rax` = 这个 tick 桶里的条目数（`rbx`=self） | — |
| `0x3f11e0` | `GraphwarObstacleData::draw_circle` | `rdi`=掩膜数据；`edx`=颜色（`0` 建障碍 / `0xff` 挖洞）；`xmm0/xmm1/xmm2`=(x,y,r) |
| `0x4724cc` | `execute_results` 内队列预扫 | `rbp`=GSM；队列 `{ptr@+0x38, ?@+0x40, ?@+0x48}`，步长 `0x20`（本节实测 `+0x40`=len、`+0x48`=cap） |

### 37.3 一炮的完整记录（`fixtures/obstacle_destroy_pairs.jsonl` 的 `run3#shot3`，原文数字）

```text
障碍（add 时就地观测 + 回放里的报文一致）：AddObstacle {pos_x:0.0, pos_y:0.0, radius:4.516640192000594}
炮口 = 士兵 230 的 PosInfo        = FunctionPoints.start_x/start_y = (-5.626074133450276, -0.1727650897248889)
点数组（kind 0x1000）：len=180，首点 = 炮口，末点 = (-3.947949133450215, -0.12123362198998919)
                      (x 步长 0.009375，**整数组没有重复点**：dupScan = 0 组)
挖洞指令（结果 Vec 里**唯一**一条 tag 0x8000…0）：
       x_bits=c00f95665af3858e  y_bits=bfbf092aa99f64ae
       +0x18 = (u32 230, u32 179)   ← (射手 entity_id, 采样点序号) == (len-1)
第 1 次排队（server_side，调用点 gw+472d1e = execute_results 的挖洞块）
       tick=2856  x/y 与上面**逐位相同**  rec_field=230  rec_index=179  radius=0.5
第 2 次排队（client_side，调用点 gw+3e36bf = graphwar_remote_event_handler(0x3e2830) 里的 tag-42 消费点）
       tick=2856（**就是报文里那个 tick**）  x 逐位相同，**y = …af（差 1 ulp）**，第 3 槽位是 radius(=0.5) 不是 u64
到期：两个管理器各自的桶里都只有 **1** 条（`0x3f1d99` 处 `rax=1`）
发报文：`destroy_obstacle` 各调一次（`bt[0]=gw+3f1dd5` = handle_tick_event 的循环体）⇒ 两条 (x,y,r)
回放（线上一侧）：**只有 1 条** `DestroyObstacle {pos_x:-3.947949133450215, pos_y:-0.1212336219899892, radius:0.5}` @tick 2856
                  同批 `EffectInfo{effect_id:236, ExplosionEffect, pos 与它逐位相同}` @tick 2793
                  `FunctionPoints{owner_id:230, end_x:-3.947949133450215}` @tick 2788
```

三条**结构**证据（都来自上面这份原文）：

1. **"序号"是下标，不是被分配的**：`point_index == 点数组 len − 1`，8/8 成立；
   同一发的点数组**没有重复点**（`dupScan` 组数 0/8）⇒ 序号不是"两个相同坐标的不同下标"。
2. **两次排队来自两个对象，不是一个桶里两条**：两个 `self` 是不同指针（一个在 `.so` 堆上，一个是进程静态低地址）；
   `handle_tick_event` 处两边 `rax`（桶内条目数）**都是 1**（6 次观测，`fixtures` 里 3 条带
   `tick_buckets`）。⇒ §31.5 第 4 条记的"`handle_tick_event` 逐条调 `destroy_obstacle`"是**对的**，
   但它解释的不是本现象：这里每条路径的桶里只有一条。
3. **`pos` 的来源**：挖洞指令的 (x,y) 就是**点数组末点**（`[rsp+0x70]` 在 `0x3c37e8` 处由
   `points[cursor]` 写入，逐位相同 8/8）。⇒ 一炮被障碍挡住时，弹道在撞点处被截断，
   末点既是挖洞点、也是 `ExplosionEffect` 的点（`§12.1` 的 503/503 是这条的推论）。

### 37.4 顺带把 `tick` 式子钉住（§12.6 第 2 条、§13 第 3 条的两个"未知"）

`tick = [管理器+0x58] + 点序号/30 + 60`（§12.4 的式子）。三发带回放的样本**全部对上**，
而且把 `[管理器+0x58]` 落实了：

| 发 | 点序号 | 观测 tick | `FunctionPoints` 报文 tick | 反推 `[管理器+0x58]` |
|---|---|---|---|---|
| run3#1 | 164 | 740 | 672 | **675** = 672+3 |
| run3#2 | 121 | 1762 | 1695 | **1698** = 1695+3 |
| run3#3 | 179 | 2856 | 2788 | **2791** = 2788+3 |

⇒ `tick = (开炮那一 tick + 3) + floor(点序号/30) + 60`，三发**同一个 +3 偏移**
（`tools/obstacle_room_extract.py --check` 的第 ⑤ 条恒等式就是这条）。
另有两条旁证：

* 客户端侧排队的 `edx` **就是报文里那个 tick**（不是本地重算），所以两边 tick 相同；
* `DelayDestroy` **不是常数**：本组三发从开炮到挖洞分别隔 68 / 67 / 68 tick
  （= 3 + 序号/30 + 60，序号小 ⇒ 墙就在炮口旁边）；同一机制在更长的弹道上会更大
  （`run2` 那三发是 114~? tick 量级）。⇒ §12.6 里"`DelayDestroy = 165` 是抓包经验值"要按
  **"`60 + 3 + 点序号/30`"** 重读，不该当成常数。
* 掩膜侧的自洽：连打三炮（同一条过原点的直线）时，撞点离原点依次是
  `3.1155 → 2.6073 → 2.0896`（`run2`）与 `4.5213 → 4.4938 → 3.9498`（`run3`），
  相邻差 ≈ **0.508 / 0.517** —— 正是报文里 `radius = 0.5` 那个洞的半径 ⇒
  "挖洞点 = 掩膜上被刷掉的那个圆心"闭环了。

### 37.5 修正本仓库的三条旧结论（都是"就地改正"，标清改了什么）

| 旧说法 | 现在 | 依据 |
|---|---|---|
| §31.5 / §31.9#1：两条相同报文"只能来自两个不同点序号上坐标逐位相同的点" | **否**。一炮只出 1 条指令、1 个序号；重复来自**两条路径各自排队**（服务端 `execute_results` + 客户端 tag-42 消费点）。§31.5 自己的机制链（`queue_destroy_obstacle` 追加桶 → `handle_tick_event` 逐条发）**没变**，只是"桶里为什么有两条"的答案换成了"因为有两个桶/两个管理器，各自一条" | §37.3 的结构证据 1/2 |
| §12.4 / §13：`u64` 字段"语义未知" | **已定**：`(u32 射手 soldier entity_id, u32 采样点序号)` | `rec_field == 点数组记录 +0x18 == 230/231`，`rec_index == len-1`（8/8） |
| §35.6 ④"没取到样本" | **取到了**（8 组）。同时**新增一个未知**：8 组里 1 组的 y 差 1 ulp（§37.6 第 2 条） | `fixtures` |

⚠ 一条**没有**被我改动的东西：`src/graphwar_shot_calculator/settle.rs:154` 现在仍是
"遇撞 `break` + 只发 1 条"（§31.6 登记的已知差异）。本节的观测**支持"一炮只挖一个洞"**
（点数组只到撞点为止、结果 Vec 里只有 1 条 tag 0x8000…0），也就是**我方"1 条"是对的**；
"原版会发 2 条"是**同一次挖洞被两条路径各发一次**，不是"每个命中点各发一条"。
⇒ 若要让报文条数也对上，正确做法是**建模两个 `ObstacleManager`**（服务端/客户端各一个），
不是把 `break` 去掉。这条是**推断**（客户端那一侧在线上局里到底谁发给谁还没观测），
证据缺口见 §37.6 第 1 条。

### 37.6 仍未确定（诚实清单）与缺什么

| # | 事项 | 缺什么 | 档 |
|---|---|---|---|
| 1 | 401 份**线上**回放里 162 组重复报文，是否同一机制（两管理器各一条） | **线上一侧的抓包**：本轮只做了 campaign（同一进程既是服务端又是客户端，`user://` 回放里**只有 1 条** tag 41）。要做：`examples/serve_broker` + 两个客户端，或在大厅里加机器人（`dispatch/25` 卡在这里） | **推断（强）** |
| 2 | 1/8 那发 **y 差 1 ulp** 从哪来 | tag-42 报文载荷的**序列化点**（`(x,y,r,tick)` 写文本的那一处）。观测到的事实是：生产侧 `ae…`、客户端侧与线上都是 `af…`；`EffectInfo` 也是 `af…`。候选：文本回代 / `ExplosionEffect` 那一支的重算（§12.1 的"拿印出来的文本重算 y"）。**证据缺口**：在 `add_event` 之前把 tag-42 载荷 dump 出来 | **未知** |
| 3 | `y = f(x) + c` 还是 `y = f(x − x_m) + y_m` | 两模型在**线性** f 上等价（本轮 8 发全是直线，分不开）。缺：一发非线性 f（如 `sin`）的点数组首几点 | **推断** |
| 4 | 客户端侧那个 `ObstacleManager` 的**归属模块** | `self` 落在进程静态低地址（不是 `.so` 堆），本轮只按调用点 `gw+3e36bf`（`0x3e2830` 大函数 = `graphwar_remote_event_handler`）定性 | **推断** |

### 37.7 数据与复跑

```bash
# 自洽性闸门（5 条恒等式；不满足返回 1）
../.reference/venv/bin/python tools/obstacle_room_extract.py --check --out fixtures/obstacle_destroy_pairs.jsonl

# 逐条看那 8 组"两条报文"的位模式
../.reference/venv/bin/python - <<'PY'
import json
for l in open('fixtures/obstacle_destroy_pairs.jsonl'):
    r=json.loads(l); v=r['verdict']
    print(r['sample'], 'idx=%d'%r['destroy_instruction']['point_index'],
          'x_eq=%s y_eq=%s y_ulp=%s'%(v['x_bits_equal'],v['y_bits_equal'],v['y_ulp_diff']))
PY
```

样本 `fixtures/obstacle_destroy_pairs.jsonl` 是**文本**、20 KB、已去掉路径与指针
（`self` 只留 `server_side`/`client_side` 两个角色标签，由**调用点**判定，不看地址）。
原始 frida JSONL、截图、`user://` 都在 `.agent-work/` 里（**可丢弃、不入库**）：
本节每个数字都能用 §37.1 那三条命令重新生成。

## 38. 名字判据的自检口径：`rust_sym` + 普查 + 正对照（2026-11）

> 背景与结论见 `docs/02` §39。本节只写**怎么验**。
> 一句话：名字判据（哪些符号算 `graphwarrust` 自己的函数）改动之后，**不能只看
> 「重跑一遍还是这个数」**，必须同时给三条互不依赖的证据。

### 38.1 三条必跑的检查

| # | 验什么 | 命令 | 通过标准 |
|---|---|---|---|
| 1 | **判据本身**：结构解析对不对（不是子串） | `.venv/bin/python tools/name_crate_audit.py` | 三段都自洽：`sym_direct + sym_impl_head + sym_nested_impl == game_total`；目标构建 0 个名字；`keys_coarse_only` 里的键 `lands_addr` 全 0 |
| 2 | **交付物不受损**：改判据没有改错已交付的名字→地址 | `.venv/bin/python tools/name_bridge.py build` + `git diff --stat fixtures/name_to_addr.tsv` | 带地址的行数不变（325 个地址），diff 只有「去掉胶水假名字 + 新增列」 |
| 3 | **正对照**：随机抽已知真值，逐条查入口/锚归属/两侧一致 | `.venv/bin/python tools/name_accuracy_check.py --sample 30` | 入口 100%、锚归属/溯源 100%、两侧一致 100%（不一致的必须能对上 `docs/02` §39.5 的冲突清单） |

### 38.2 可重复性（这条曾经是坏的）

`name_bridge.py` 的消去法是顺序敏感的贪心，旧版按 `set` 迭代 ⇒ 输出随 `PYTHONHASHSEED` 变
（同一命令三种答案）。任何「确切数」都必须先过这一关：

```bash
for s in 0 1 3 7; do PYTHONHASHSEED=$s $PY tools/name_bridge.py build --out /tmp/n$s.tsv; done
md5sum /tmp/n{0,1,3,7}.tsv      # 四份必须相同
```

`fixtures/sym_match.tsv` 也一样：重建后必须与入库一致（`md5 = db252b8d135eb879f6a77e26b06becc9`）。

### 38.3 夹具各自的「宇宙」不同——别拿一个数去对另一个

| 夹具 | 宇宙 | 行数 | 判据 |
|---|---|---|---|
| `name_to_addr.tsv` | Android 的**游戏自有**符号（`rust_sym.is_game_name`） | 743 | 名字表 |
| `windows_name_to_addr.tsv` | Windows 的**游戏自有**符号 | 551 | 名字表 |
| `sym_match.tsv` | Android 里**提到**本 crate 的符号（`mentions_crate`，含编译器胶水） | 1,300 | **配对表**（§35），胶水也是真函数 |
| `name_crate_audit.tsv` | 三口径并排（prefix_only / unified / coarse） | 4 行 + 逐键清单 | 普查 |

`sym_match.tsv` 的 1,300 是**刻意的**：它的任务是「没有 Location 锚的函数怎么配对」，
把 330 个胶水函数踢掉会让 §35 的分母与 `tests/sym_match.rs` 的宇宙断言同时失效；
而它的**名字键**仍然只从游戏自有符号取（`parse_name` 里的 crate 闸），两者不要混为一谈。

---
## 39. `fire` 轴的**口径变更**：多发批改走一次 `fire_batch`（2026-11-08）

> 起因：`S`（多发批结构）桶 126 批被当成"内核还没修好的结构错"。交叉核对否掉了这个读法：
> **130 个多发批里有 128 批在 `batch` 轴上种类序列是 OK 的**（§28 接线后）。也就是说
> `S` 不是内核错，而是 `fire` **驱动**的表达能力不足 —— 一个结算批里有 N≥2 发时，
> 逐发调 `fire` 天然造不出"一批一次广播"的窗口。
> 本节只改**驱动的调用方式**，内核（`src/game.rs` 的结算逻辑）一行未动。

### 39.1 判定行：按**批内发数**选驱动

`src/_scaffold/replaydiff.rs` → `Predictor::predict`：

```rust
let wire: Vec<String> = match batch.shots.len() {
    0 => Vec::new(),
    1 => self.model.fire(s.owner, &s.text, s.x0, s.y0, s.flip, 0),   // 单发批：口径不变
    _ => self.model.fire_batch(&subs, 0),                            // ★ 多发批：一次结算一次广播
};
```

判据是 **`batch.shots.len()`（批内发数）**，不是"整份回放里有没有多发射击"：
同一份回放里单发批与多发批可以混着出现（130/1080 是多发批），按文件判会连带改掉单发批。

单发批**逐位不变**是有理由的，不是巧合：`Match::fire(...)` 的实现就是
`self.fire_batch(&[Submission { .. }], now_ms)` —— 单元素特例，不是第二条结算路径。
实测印证：单发批种类序列 **947 → 947**，900+ 批一条没动（见 39.3 的分母）。

### 39.2 `fire` 轴 before/after（同一快照、同一二进制，只换驱动）

| 指标 | before（逐发 `fire`） | after（批内 ≥2 发一次 `fire_batch`） | 差 |
|---|---|---|---|
| 内容命中（1080 批） | **608** | **643** | **+35** |
| 消息种类序列（1080 批） | **947** | **1075** | **+128** |
| ├ 多发批（130 批） | 0 | **128** | +128 |
| └ 单发批（950 批） | 947 | **947** | **0** |
| 死亡集合 | 366 / 385 | 366 / 385 | 0 |
| 挖洞点 | 205 / 293 | **206** / 293 | +1 |

复跑：

```bash
cargo run --release --offline -q --example replay_attrib -- \
    --dir <快照> --drive fire --policy default --json f.jsonl
python3 tools/bit_exact_attrib.py --json f.jsonl
```

### 39.3 逐桶 before/after：**为什么"其它桶"会涨**，以及它为什么不是回归

先给违反直觉的那一半：**其它桶确实涨了。**

| 桶 | before | after | 差 | 涨的每一批从哪来 |
|---|---|---|---|---|
| `PASS` | 608 | **643** | +35 | 原 `S` 里内容本来就对的那 35 批 |
| `S` 多发批结构 | **126** | **0** | **−126** | 全部重归因（见下） |
| `SD` 结构+死效 | **4** | **0** | **−4** | 同上 |
| `D` 死效块 | 3 | 5 | +2 | 原地 3 + 原 `SD` 2 |
| `C` corrected_function | 14 | 14 | 0 | — |
| `E1` end_x 小差 | 91 | 127 | +36 | 原地 91 + 原 `S` 36 |
| `E2` end_x 真分歧 | 8 | 11 | +3 | 原地 8 + 原 `S` 2 + 原 `SD` 1 |
| `X` 爆炸点 x | 66 | 91 | +25 | 原地 66 + 原 `S` 25 |
| `Y` 爆炸点 y | 160 | 189 | +29 | 原地 160 + 原 `S` 28 + 原 `SD` 1 |
| 失败合计 | 472 | **437** | **−35** | |

**任务书的字面要求（"其它桶一个都不许涨"）在数学上不可能满足**，原因在 `tools/bit_exact_attrib.py`
的判桶优先级：`kind_seq_ok == false` 时先走结构分支，命中 `S`/`SD` 就**不再看内容**。
一旦窗口结构被修对，这批就不再被结构判据截走，它**底下的内容差**必然落到内容桶。
`S` 桶 126 批里只有 **35 批**（表格"独占"列）内容本来是全对的 —— 这 35 批才是 S→PASS 的全部。

所以本节改用一个**更强、且能真正证伪的判据**：

> **① 原来非 `S`/`SD` 的六个桶，成员集合逐批不变；② `PASS → 失败` 的批数为 0；③ 失败总数下降。**

逐批迁移矩阵（`before` 行 × `after` 列，共 1080 批，来自 `classify()` 原样复用）：

| before \ after | PASS | D | C | E1 | E2 | X | Y | 合计 |
|---|---|---|---|---|---|---|---|---|
| `PASS` | **608** | 0 | 0 | 0 | 0 | 0 | 0 | 608 |
| `S` | 35 | 0 | 0 | 36 | 2 | 25 | 28 | 126 |
| `SD` | 0 | 2 | 0 | 0 | 1 | 0 | 1 | 4 |
| `D` | 0 | **3** | 0 | 0 | 0 | 0 | 0 | 3 |
| `C` | 0 | 0 | **14** | 0 | 0 | 0 | 0 | 14 |
| `E1` | 0 | 0 | 0 | **91** | 0 | 0 | 0 | 91 |
| `E2` | 0 | 0 | 0 | 0 | **8** | 0 | 0 | 8 |
| `X` | 0 | 0 | 0 | 0 | 0 | **66** | 0 | 66 |
| `Y` | 0 | 0 | 0 | 0 | 0 | 0 | **160** | 160 |

**只有 `S` 与 `SD` 两行散开，其它七行全是纯对角。** 也就是说：

* 608 个原本 `PASS` 的批**一个都没变坏**；
* `D`/`C`/`E1`/`E2`/`X`/`Y` 六桶的**原有成员 3 / 14 / 91 / 8 / 66 / 160 逐批原地不动**；
* 六个桶涨出来的 95 批 = `S` 的 91 批 + `SD` 的 4 批，每一批都能指到来源（上表逐格可加）。

⇒ **判定：这是一次纯重归因，不是回归。** 但它同时说明一件事，必须留给下一轮：
`S` 那 126 批里**只有 35 批的内容是真的对的**，剩下 91 批掩盖着 `E1`/`Y`/`X`/`E2` 级的真内容差。
"消掉 S 桶能白拿 126 批"这个预期是错的，实际白拿 **35 批**。

### 39.4 `batch` 轴：**一点没变**

`--drive batch` 不经过 `Predictor`（它在 `examples/replay_attrib.rs` 里自建 `Match` 直接调
`fire_batch`），所以这次改动对它应当是恒等的。实测逐字节核对：

```bash
cmp b_before.jsonl b_after.jsonl && echo IDENTICAL
# md5 两侧同为 af9b77f16ef8ca0309d54643564a17ba
```

内容 **643 / 1080**、种类序列 **1075 / 1080**、死亡 366/385、挖洞 206/293 —— 一项没动。
`--check-baseline --axis batch` 全绿。

### 39.5 顺带得到的一个恒等式：`fire` 轴 ≡ `batch` 轴（本快照）

改完之后 `f_after.jsonl` 与 `b_after.jsonl` **逐字节相同**。这不是抄错：

* 两条轴现在都"一个批一次结算一次广播"，**唯一**剩下的差别是**模式时刻表** ——
  `Predictor` 按 `FunctionModeInfo` 逐批更新模式（§33.3），`--drive batch` 用建局时的模式不动；
* 而这份快照的 282 份回放里，`mode` 列（`Normal`/`DiffEq`）在**首个结算批之后没有任何跳变**
  ⇒ 该项差值为 **0**，两轴输出相同。

⇒ 以后 `fire` 与 `batch` 的差 = "局中改模式"这一项的**当前值（=0）**，可以当探针用；
真出现非零差，说明语料里出现了局中改模式的回放，而不是驱动 bug。

### 39.6 影响面与登记

| 对象 | 影响 | 处理 |
|---|---|---|
| `replay_diff::run_file` | 共用 `Predictor` ⇒ 第一分歧统计同步变化 | **必须一起变**：否则 `replay_attrib --drive fire` 与 `replay_diff` 的"逐位同口径"就成假话 |
| `examples/replay_attrib.rs` 注释 | `--drive fire` 释义、与 `--drive batch` 的剩余差别 | 已同步更新 |
| `examples/sim_turns.rs:130` 注释 | 仍写着 `false` = "`replaydiff::Predictor` 的驱动方式（每发一个窗口）" | **已过时**；`sim_turns.rs` 不在本轮可改范围，**登记待改** |
| `fixtures/guardrails.json` `fire` 轴 | `content_ok 608→643`、`kind_seq_ok 947→1075`、`destroy_ok 205→206` | 已用 `--update-baseline --axis fire` 写回，`--check-baseline` 全绿 |
| `docs/coverage-matrix.tsv` · `docs/149-coverage-matrix.md` | `src/` 变了 ⇒ 只有 `src_tree_sha256` 那一行变（**没有任何数字变**） | 按 `tools/coverage_matrix.py` 的提示重新生成，`--check` 回到 OK |
| `tests/fire_batch_driver.rs` | 新增 4 条（`cargo test` 1569 → **1573**） | 见 39.7 |
| 内核 `src/game.rs` / `*_class.rs` / `client/` / `dispatch/` | **未改一行** | — |

### 39.7 护栏与测试（为什么这条口径能被锁住）

`--check-baseline --axis fire` 只在**有定格快照的机器**上才有意义（快照在
`.agent-work/` 里，按仓库约定是可丢弃的）。所以另外加了 `tests/fire_batch_driver.rs`，
**自带合成回放**（`PosInfo` + 一个结算批 + `GameStateInfo` 三件套），没有语料也能跑：

| 测试 | 断言 | 旧驱动（逐发） |
|---|---|---|
| `single_shot_batch_has_exactly_three_game_states` | 1 发 → `GameStateInfo` = 3 | 通过（口径不变，正是这一点要被证明） |
| `multi_shot_batch_is_one_broadcast_not_n_windows` | N 发 → 仍然是 3（不是 3N） | **红**：`(2, 6)` |
| `multi_shot_batch_keeps_the_grouped_prefix` | `FunctionInfo`/`FunctionPoints` 各 N 条且**连排** | **红**：首个到末个 `FunctionInfo` 跨 15 条 |
| `criterion_is_the_batch_not_the_file` | 混排 `[1,2,1]` → 每个批都是 3 | **红**：`[(1,3),(2,6),(1,3)]` |

**正对照已做**：把 `Predictor::predict` 临时换回逐发实现，上面后三条**确实变红**
（`left: [(2, 6)]` / `right: [(2, 3)]`），换回按批实现即绿 —— 说明这四条锁的是真实行为，
不是"写了个永远为真的断言"。同时 `[(1,3),(2,6),(1,3)]` 这组数也直接证明
**旧口径的破坏只发生在多发批上**，单发批一条没动。

证据档位：本节所有数字都是 `逐位`（同一份定格快照、同一个二进制、**只换驱动调用方式**），
分桶复用 `tools/bit_exact_attrib.py` 的 `classify()`，没有第二份判据。

### 39.7 主会话独立复核（合并前，2026-11-08）

合并在即，我自己重跑了一遍并**逐批比对**新旧两次 fire 轴产物（`r5` 与 `r6` JSONL）：

```text
PASS → 失败 = 0            没有一批从对变成错（这是本改动最要紧的判据）
失败 → PASS = 35           净收益只有 35，不是 126
batch 轴新旧 JSONL 逐字节相同（md5 一致）⇒ 原口径一点没被影响
```

⇒ 与本节 §39.3 的迁移矩阵一致：S 的 126 批里只有 **35** 批内容本来能对，
另外 **91** 批只是从"结构桶"迁到"内容桶"，**并没有变好**。

**方法论记一笔**：任务书里我写的是"其它桶一个都不许涨"，那是**错的**——
判桶是优先级制，结构一修对，被遮住的内容差必然浮出来。
正确的判据是这三条：**PASS→失败 = 0 / 原有成员逐批不变 / 新增成员逐批可追溯来源**。
以后改口径的任务书都按这个写。

## 40. 爆炸点坐标：`pos` 与 `end_x` **不是同一个数**；1 ulp 残差不是"回代"能修的（2026-11-08，`agent/explosion-pos`）

> 任务：找"原版算爆炸点的那个表达式"，攻 `Y`（160 批 / 33.9%）与 `X`（66 批 / 14.0%）。
> **结论：表达式没找到**；本节交付的是**把假设空间砍掉一大半**的判定，以及三条能复跑的否证。
> 反汇编口径同 §26/§36：x86-64 目标 `2026-08-28/libgraphwarrust.so`（stripped，地址 = 虚地址）
> ＋ arm64 Android 2.0.60（未 strip，地址 = `st_value`）。

### 40.1 一句话（四条）

1. **现象普遍，且是"两条路径"**（§39.2）：全语料 1290 发里 `pos_x` 与**同一发的** `end_x`
   逐位相同 **1050**（81.4%），差 **1~3 ulp 的 66 发**，差 ~一个采样步的 79 发、其余 95 发。
   `pos_y` 与**我方点数组末元素**的 `y` 逐位相同的只有 **581 / 757**（76.8%）——
   也就是说"末元素"这一侧 `y` 也在变，**`x` 与 `y` 的偏差彼此独立**。
2. **`Y` 桶不是一个机制**（§39.3）：160 批里 **114 批是 DiffEq（RK4 积分）**、**46 批是 Normal**。
   46 批 Normal 里，`eval(回放自己的 corrected_function, 回放自己的 pos_x)` 逐位命中 **0 / 46**，
   `−1 / +1 ulp` 各 28 / 18 ⇒ **回放那条 `(pos_x, pos_y)` 自己就不自洽**（不是同一个 `x` 上算的）。
3. **"用印刷版修正文本在 `x = end_x` 处回代" 是错的**（§39.4，实测）：把 `EffectInfo(Explosion)`
   从 `out.stop` 换成 `hole_point`，`fire` 内容命中 **608 → 601**（丢 7 批、得 **0** 批）。
   §36.5 的 M1 提示（`eval(文本, 停点)` 命中 544 > 498）**在整批口径下不成立** ——
   那 544 是逐发的，落到"整批只差这一个字段"的 160 批上命中率是 **0**。
4. **点数组的拷贝链仍是逐位搬运**（§39.5，两套构建各自复验）：`end_x` 与 `pos` 读的是
   **同一对 `(ptr,len)`、同一个下标**。所以两者在报文层**不可能**不同 ⇒ 残差只能来自
   "**被读的那条数组在两处之间变了**"或"**存在两条数组**"。Frida 夹具直接抓到
   **每发两次 `calculate_function`（两个不同返回地址、两块不同缓冲、点数相同）**，这是留给下一轮的唯一硬线索。

### 40.2 现象：`pos` vs `end_x`（观测，不依赖我方实现）

`examples/replay_endx_probe.rs` 跑遍 401 份快照（1290 发可比），按"`|end_x − 我方末点 x| ≤ 4 ulp`"
筛出**没有被障碍截断**的 825 发（其余是 settle 截断，与本议题无关）：

| 关系（825 发，未截断） | 条数 |
|---|---|
| `pos_x` 与 `end_x` **逐位相同** | 757 |
| `pos_x = end_x ± 1 ulp` | 53 |
| `pos_x = end_x ± 2..8 ulp` | 15 |
| `pos_y` 与**我方点数组末元素 `.y`** 逐位相同 | 581 |
| `pos_y` 差 1 ulp / 2 ulp / 其它小量 | 105 / 22 / 49 |

两个独立事实：

* **`end_x` ≡ 我方末点 `x`**（825 发里 823 发差 0 ulp）⇒ 采样器几何、`settle` 前的点列都对。
* **`pos_y` 却只有 77% 与末点 `y` 相同**，而 `pos_x` 有 92% 与末点 `x` 相同 ——
  **同一个点元素的两个分量表现出不同的偏差率**（23% vs 8%）。单点同源无法解释这个不均衡。

### 40.3 `Y` 桶按模式二分，且 Normal 那一半自相矛盾

`examples/posy_mech.rs`（既有）产出 `mech.jsonl`，按 `tools/bit_exact_attrib.py` 的 `classify`
把 `Y` 桶 160 批逐批对齐：

| 模式 | 批数 | `eval(回放文本, 回放 pos_x)` 逐位命中 |
|---|---|---|
| `DiffEq`（RK4） | **114** | 不适用（`y` 来自积分，`NaN`） |
| `Normal` | **46** | **0 / 46**（`−1 ulp` 28 批、`+1 ulp` 18 批） |

⇒ ① 旧口径把 `Y` 当"求值器末位"是错的（114/160 根本没有 `f(x)`）；
② 剩下 46 批**连回放自己那条 `(x,y)` 都不自洽**，所以"换一个 `x` 回代"这条路已经封死
（`end_x` / 我方停点 / 回放 `pos_x` 三个候选全部 0 命中；常数各 ±1 ulp 只命中 6~9 批）。

⚠ **一个必须点明的口径反转**：`Y` 桶的分桶判据就是"`pos_x` **没有**差"，所以那 160 批里
`pos_x` 与 `end_x` 是**逐位相同**的 —— "`pos` 与 `end_x` 是两条路径"这句话**只在 `X` 桶成立**。
`Y` 桶的悖论更硬：**同一个 `x`、同一个常数、同一个求值器，却差 1 ulp**。
（`DiffEq` 那 114 批的 `y` 是积分累加值，理论上输入差 1 ulp 就可以只动 `y` 不动 `x`；
`Normal` 那 46 批的 `y = f(x) + c` 是**单次正确舍入**，做不到 —— 见下面这一例。）

手工复算一例（`1782676871#3`，函数 `1/(6-x)`、修正文本 `1/(6-x) - 6.861`、flip 发）：

```text
pos_x = -5.9542382857185725   ⇒ 求值用的 x = +5.9542382857185725（flip 取反，逐位精确）
6 - x        = 0.0457617142814275        （Sterbenz：减法精确）
1/(6-x)      = 21.852328211529706        （divsd，正确舍入）
f - 6.861    = 14.991328211529705        ← 我方、也是"精确值"的最近 double
回放 pos_y   = 14.991328211529703        ← 比它**低 1 ulp**
```

`1/(6-x) − 6.861` 的**精确值**是 `14.99132821152970524068…`，最近 double 就是 `…705`。
把这 1 ulp 归因成 `x` 差多少：`f'(x) = 1/(6−x)² = 477.6`，`Δy = 1 ulp = 1.78e-15`
⇒ `Δx ≈ 3.7e-18`，只有 `x` 的 ulp（8.9e-16）的 **0.4%**。
⇒ **在一个"单点单次正确舍入求值"的模型里，这个 1 ulp 不可能出现**；
它要么是求值器里有一步不是正确舍入（FMA 收缩 / 扩展精度 / 近似倒数），要么 `y` 不是这条 `x` 上算的。
两种都**没有证据**，故本节不结论（缺什么见 §39.7）。

### 40.4 实测：`hole_point` 回代 = 退步

§36.5 的 M1 说"`eval(文本, 我方停点)` 命中 544 > 现在的 498"，据此把 ④ 的坐标换成
`hole_point`（`src/game.rs::execute_results`，与 ③ 挖洞点共用一份，顺带满足"挖洞点 ≡ 爆炸点"）：

```text
fire 驱动，同一份 401 快照
  before   内容命中 608 / 1080   Y 160   X 66   Y2 0
  after    内容命中 601 / 1080   Y 160   X 66   Y2 7      ← 丢 7、得 0
```

⇒ **逐个发的 544 不等于整批的收益**：`Y` 桶的定义是"整批只差 `pos_y` 这一个字段"，
在那 160 批上回代命中率是 **0**。改动已回退。

### 40.5 两套构建各自复验：`end_x` 与 `pos` 读的是同一对 `(ptr,len)`

* **x86-64（目标构建）`execute_results`**：队列项 32 B/项，`+8/+0xc` = 指针两半、
  `+0x10` = 点数、`+0x18` = 一个 u32：

```text
0x4727e1  mov r15d,[rax+0x8]          ; 指针低半
0x4727e5  mov r12d,[rax+0xc]          ; 指针高半
0x472814  shl r12,0x20 / 0x472818 or r12,r15 / 0x47281b mov [rsp+8],r12
0x472820  mov r13,[rax+0x10]          ; n
0x472854  test r13,r13 / je skip
0x472859  mov rcx,r13 / shl rcx,4
0x472860  mov rdx,[rsp+8] / 0x472865 movdqu xmm0,[rdx+rcx-0x10]   ; ★ pos = pts[n-1]
0x47286b  movdqa [rsp+0xf0],xmm0
```

* **arm64 `fire_functions`**（`0x4a7160`）给 `set_function_points` 的那条：

```text
4a7a24  ldr  x9,[x28],#0x20     ; tag
4a7a28  tbnz x9,#0x3f, skip     ; 跳过 bit63 的"事件项"
4a7a2c  ldr  x24,[x8,#0x10]     ; n  ← 同一个字段
4a7a50  ldr  x23,[x8,#0x8]      ; ptr ← 同一个字段
4a7ac8/4a7af0                   ; 纯 16 B memcpy，零算术
4a7b28  stp  x24,x21,[x29,#-0x68] ; (n, dst)
4a7b3c  bl   set_function_points  ; 里面 end_x = pts[n-1].x（§27.2）
```

⇒ 报文构造层两者必然相等。所以 §39.2 的 66 + 245 条差**不可能**在这一层产生。

### 40.6 新线索：**每发两次采样**（Frida 夹具，观测）

`fixtures/frida_point_array_sample.jsonl`（§35 的真机样本，4 发）里每发都有
`calculate_function_calls: [seq1, seq2]`：

| 发 | seq1 返回地址 | seq1 返回的 Vec | seq2 返回地址 | seq2 返回的 Vec |
|---|---|---|---|---|
| run1 `sin(x)+3` | `gw+3c30c7` | cap `0x2000`, ptr `7fae841410b0`, **len 4943** | `gw+3e2d3b` | cap `0x2000`, ptr `2416b6…`, **len 4943** |
| run2 `sin(x)+3` | `gw+3c30c7` | `0x1000`, `7f0404142e10`, **3548** | `gw+3e2d3b` | `0x1000`, `1a27baf0`, **3548** |
| run2 `sin(x)+3x/2` | `gw+3c30c7` | `0x400`, `7f0404144240`, **929** | `gw+3e2d3b` | `0x400`, `…`, **929** |

* 两次调用 **`x_arg_bits` 相同**（同一个 `x0`），**点数相同**，**缓冲不同**；
* `queue.items[1].f08` 的指针**逐位等于 seq1` （`7fae841410b0`）⇒ 观测那一刻队列里放的是 seq1 的数组；
* `queue` 有 **2 项**：`items[0]` tag `0x8000000000000002`（带一个 f64 与两个 u32 tick），
  `items[1]` = 上面那条"点数项"（`{cap@0, ptr@8, n@0x10, tick@0x18}`）。

⇒ 两处报文若分别吃**两条数组**（或者队列项在 `fire_functions`（tick 231）与
`execute_results`（同批稍后）之间被第二趟采样**覆盖**），1 ulp 级差异就有了来源，
而且天然解释"`x` 与 `y` 偏差率不同"。**这一跳没证到**（seq2 的调用者 `gw+3e2d3b`
落在 Godot 回调链里，未定位到模块），所以本节不下结论。

### 40.6b 残差与"越界停"强相关（观测，进一步缩小范围）

把 `Y`/`X` 两桶的 1290 发按"这条弹道是不是**越界停**（`|pos_x| > 24.9` 或 `|pos_y| > 14.9`）"统计：

| 组 | 发数 | 越界停 | 占比 |
|---|---|---|---|
| `Y` · `DiffEq` | 114 | 101 | **89%** |
| `Y` · `Normal` | 46 | 31 | 67% |
| `X` · `DiffEq` | 37 | 28 | 76% |
| `X` · `Normal` | 29 | 14 | 48% |
| **全部 `DiffEq`**（基线） | 515 | 376 | 73% |
| **全部 `Normal`**（基线） | 565 | 320 | 57% |

⇒ 残差**偏向**落在"最后一个点是被越界检查挡下来的那一支"（89% vs 73%），
但不是全部。下一轮的地毯式搜索应该**只看这一支**：`pts[len-1]` 入列之后、报文之前，
有没有一步会重写它。（`Y` 的 `DiffEq` 那一半 `|y| > 14.9` 的有 71/114 —— y 越界比 x 越界更集中。）

### 40.7 还缺什么（下一轮的直接入口）

| # | 缺的证据 | 怎么拿 |
|---|---|---|
| 1 | seq2（`gw+3e2d3b`）**是哪条路径**、它的数组去哪了 | x86 目标在该返回地址附近 dump 调用者链；或在 `execute_results` 的 `movdqu` 处下断点，抓 `rdx/rcx` 与 `fire_functions` 那次 clone 的 `(ptr, n)` 比一比 |
| 2 | 队列项在两次报文之间**有没有被就地改写** | 同一断点前后各读一次 `queue.items[1]` 的 `(ptr,n)` 与 `pts[n-1]` 的 16 B |
| 3 | `Y` 那 114 批 DiffEq 的 1 ulp 是 RK4 哪一步 | 需要一名 DiffEq 发的**整条点列**（真机抓 `calculate_function` 的返回数组，像 §35 那样抓 **DiffEq 局**；现有 4 发全是 Normal） |

复现（本节全部数字）：

```bash
export CARGO_HOME=<ws>/.cargo-home CARGO_TARGET_DIR=<ws>/.agent-work/explpos/target
R=<ws>/.agent-work/replay-snapshot
cargo run --release --offline --example replay_attrib -- --dir $R --drive fire --policy default --json f.jsonl
$PY tools/bit_exact_attrib.py --json f.jsonl --check-baseline --axis fire
cargo run --release --offline --example posy_mech  -- --dir $R --json mech.jsonl     # 既有探针
$PY tools/explpos_attrib.py --attrib f.jsonl --mech mech.jsonl   # ★ 本节 39.3 / 39.6b 两张表
```

`tools/explpos_attrib.py` 是本节新增（**只加不改**）：两份 JSONL 单看各缺一半
（`f.jsonl` 有桶没模式，`mech.jsonl` 有模式没桶），脚本按 `(file, batch)` 对齐后直接打表，
所以以上每个数都能一条命令重算。

### 40.8 护栏

`src/` 最终**零改动**（§39.4 的改动已回退），两轴逐项与 `fixtures/guardrails.json` 一致：

| 轴 | 内容命中 | 种类序列 | 死亡集合 | 挖洞点 |
|---|---|---|---|---|
| `fire` | 608 / 1080 | 947 | 366 / 385 | 205 / 293 |
| `batch` | 643 / 1080 | 1075 | 366 / 385 | 206 / 293 |

## 41. `Ode` 局的**停止判据**：五个出口逐指令核对完毕，与实现逐条同构（2026-11-08，`dispatch/29`）

> 任务书：`dispatch/29-ode-stop-condition.md`。
> 起点 `1877049`，分支 `agent/ode-stop`，私有 `CARGO_TARGET_DIR`，同一份 401 回放快照。
> **编号说明**：任务书写"最大 §37、用 §38"，但本分支已有 §38（名字判据自检口径），
> 所以本任务用 **§39**；并行任务如果也占了 §39，请协调者合并时重排。

### 41.0 一页结论（五条）

1. **原版 `Ode` 的停止条件查到了，而且只有五个出口**（越界 `|x|>25` / 越界 `|y|>15` /
   候选点 NaN / 二分 100 次不收敛 / 点数 `(200/step)` 用尽），**五个全部在"收敛之后"才判**；
   `src/graphwar_path_calculator.rs::calculate` 的 `Mode::Ode` 分支**逐条同构**（§39.1/§39.2），
   本次**没有**需要修的判据 —— 既没改阈值，也没改结构。
2. **"我方一路走到边界"在语料上不成立**：`Ode` 515 批里**一批都没有**（0 发 / 0 批），
   `Normal` 那 11 批全部来自同一份回放的同一类异常（§39.3）。任务书 §0 那句
   "我方 stop = −24.9928"是 `examples/replay_endx_probe` 的输出，**那个工具只调采样器、
   不调掩膜**；管线口径（`settle::resolve_points_owned`）在同一批上给的是
   **−20.75531942288392，与回放逐位相同**。
3. 那个样本（`1782764906` 批 #0）**真正的原因是空函数串**：`function == ""` ⇒
   `graphwar_shot_calculator::shot_points` 提前 `return Vec::new()` ⇒ 整条弹道消失 ⇒
   `end_x = 0.0`。反事实测量（§39.4）：改 1 行，`batch` 轴 **643 → 647**、`fire` 轴 **608 → 612**
   （⚠️ **审计 2026-11-08**：`fire` 那一列是 §39 改口径**之前**的轴——多发批还按逐发驱动；§39 之后
   `fire` 与 `batch` 在本快照上逐字节相同，所以现行 `fire` 也是 **647**，不是 612。见 `guardrails.json`），
   `Normal` **一条不动**。那一行在 `src/graphwar_shot_calculator.rs`，**不在本任务允许改的
   文件清单里**，所以本分支**交付时没有落地**。
   **✅ 审计 2026-11-08 更正：已由协调者落地** —— `f95a2a95`「fix(shot): Ode 局的空函数文本
   不再提前返回空点列」，闸门正是 `mode != Mode::Ode`；`fixtures/guardrails.json` 现行
   `batch`/`fire` 内容命中 = **647** 就是这一行的结果。**本段原写"没有落地"已过期。**
4. 顺带查清：回放自己的 `FunctionPoints.end_x` 与它的 `EffectInfo(Explosion).pos_x`
   **差一个采样点**（`pos = end + 1`）的批，`Ode` **52 发 / 52 批**、`Normal` **79 发** ——
   这就是归因表的 `E1` 桶（`Ode` 侧 52 批，与 `bit_exact_attrib.py` 的数完全一致）。
   **机制未知**（§39.5），但可以确定它**不是**"弹道停早了"：这些批里我方停点与回放的
   **爆炸点**是逐位相同的。
5. 交付物：`examples/ode_stop_census.rs`（逐发普查）、`examples/ode_stop_trace.rs`
   （停止原因追踪，**自校验**：与 `calculate` 逐位比）、`tools/ode_stop_census.py`（汇总 ①）、
   `tests/ode_stop.rs`（6 条回归锁）。

### 41.1 原版 `Ode` 的停止条件（**观测：逐指令，x86 + arm64 双证**）

采样器 = `GraphwarShotCalculator::calculate_function`
（**arm64 未 strip 的符号名**：`_ZN12graphwarrust24graphwar_shot_calculator22GraphwarShotCalculator18calculate_function…`
@ `0x3b5374`；x86 同函数 `0x3c2530`，`docs/06` §34.2 已把它接进内核）。

Ode 分支入口：x86 `0x3c25f5 test %r15b,%r15b` / `0x3c25f8 je 0x3c2923`（**不跳** ⇒ `0x3c25fe` 起）。

| 出口 | x86（`libgraphwarrust.so` 2026-08-28） | arm64（`android-2.0.60`） | 判据 |
|---|---|---|---|
| 二分 100 次不收敛 | `0x3c264c mov $0x64,%ebx`；`0x3c27xx` 每轮 `dec %ebx`；`0x3c282f jne` / `0x3c2835 jmp 0x3c2b5d` | `0x3b5458 b.eq 0x3b5704` | 100 次都不满足 `\|chord−step\| < 0.001` |
| 候选点 NaN | `0x3c287d ucomisd cand_y,px` + `0x3c2881 jp 0x3c2b5d` | `0x3b5570 fcmp d13,d8` + `0x3b5574 fccmp …vc` + `0x3b5580 b.gt` | `cand_y` 或 `px` 任一为 NaN |
| x 越界 | `0x3c2887`取 `mid` → `0x3c2890 andpd` → `0x3c2898 ucomisd [0x19fbc8]` → `0x3c28a0 ja` | `0x3b556c fabs d0,d10` + `0x3b5574`（25） | `\|mid\| > 25` |
| y 越界 | `0x3c28a6`取 `cand_y` → `0x3c28b2 andpd` → `0x3c28b4 ucomisd [0x19fd40]` → `0x3c28bc ja` | `0x3b5564 fabs d1,d13` + `0x3b557c`（15） | `\|cand_y\| > 15` |
| 点数用尽 | `0x3c2915 cmp %r13d,%r12d` / `0x3c2918 jne 0x3c260d` / `0x3c291e jmp 0x3c2b5d` | `0x3b55ac cmp w22,w23` / `0x3b55c0 b.ne` | `count == n` |

常量全部是**从 `.rodata` 逐字节读出来的**（不是从反编译器的注释抄的）：

| 地址 | 值 | 用途 |
|---|---|---|
| `0x19fbc8` | `25.0` | x 硬边界（arm64 里是 `fmov d2,#25.0`，`0x3b5560`） |
| `0x19fd40` | `15.0` | y 硬边界（arm64 `fmov d0,#15.0`，`0x3b5578`） |
| `0x19fc78` | `0.001` | 弦长收敛容差（`LENGTH_TOL`） |
| `0x19f8c0` | `200.0` | 点数预算的分子（arm64 `mov x8,#0x4069000000000000`，`0x3b53cc`） |
| `0x19fbc0` / `0x19f7e0` | `−2^31` / `2^31−1` | `as i32` 的饱和夹（`0x3c25d1 maxsd` / `0x3c25d9 minsd`） |
| `0x19f098` / `0x19f6d0` | `6.0` / `0.5` | RK4 的 `/6` 与二分中点（`0x3c2784` / `0x3c269d`） |
| `0x19e690` | `-0.0` | flip 的 x 取反掩码（`0x3c25b5` / `0x3c2869`） |

点数预算（`0x3c25bd..0x3c25f1`；arm64 `0x3b53cc..0x3b5400`）：

```text
n = (200.0 / step)                # 0x19f8c0 / step
  → cvttsd2si（NaN 得 0，±∞ 饱和到 i32 边界）
  → max(1, n)                     # 0x3c25e8 cmp $0x2 + 0x3c25f1 cmovge
```

⇒ `step = 0.01`（服务端）时预算 **20000**。**没有别的上限**：没有时间上限、没有弧长上限、
没有"导数爆炸"判据。**"撞到东西"不在这个函数里** —— 采样器只吃函数树
（`rdi=out, rsi=tokens, xmm0/1/2=x0,y0,step, edx=flip, ecx=mode`），障碍截断在调用方
（`settle::resolve_points_owned` 那一层，对应 `docs/06` §24 的"掩膜口径"）。

**顺序**（这一点决定了"什么时候停"的语义）：五个出口里前四个都在**收敛之后**才判
（x86 `0x3c27d0` 起算弦长 → `0x3c27f2 ja 0x3c2840` 收敛 → 才到 `0x3c2881/0x3c2898/0x3c28b4`），
所以**不会**因为"候选点在界外"就提前回头缩半步：只要收敛就把这个候选点丢掉、整条曲线结束。

### 41.2 我方实现逐条同构（附**正对照**）

| 原版 | `src/graphwar_path_calculator.rs` | 一致 |
|---|---|---|
| `hi = x + step` 每个外层迭代重设（`0x3c263d`） | `let mut hi = x + step;` 在 `loop` 内 | ✓ |
| `mid = (lo+hi)*0.5`（`0x3c269d`） | `(lo + hi) * HALF` | ✓ |
| `h = mid − x`（`0x3c26aa`） | `let h = mid - x;` | ✓ |
| k1 在外层 hoist（`0x3c262f`，不随 mid 变） | 直接式，等价 | ✓ |
| `yn = y + (h/6)(k1+2k2+2k3+k4)`（`0x3c2780..0x3c27b8`，结合序 `2k2+k1` → `+2k3` → `+k4` → `*(h/6)` → `+y`） | 同一结合序（所以逐位相同） | ✓ |
| `chord = sqrt(h² + (yn−y)²)`（`0x3c27ce call *0x9baca8`，即 `sqrt(a*a+b*b)`） | `hypot(h, yn - y)` = `m_hypot` | ✓ |
| 收敛：`!(0.001 > \|chord−step\|)` 才调整（`0x3c27f2`） | 同 | ✓ |
| Ode **没有**半步复检 | 只有 `Mode::Normal` 走 `mid2` | ✓ |
| `count == n` 才停（`0x3c2915`） | `if count == n { return }` | ✓ |

**正对照（证明"逐条同构"不是自说自话）**：同一份语料，把**管线口径**的停点与回放的
`FunctionPoints.end_x` 逐发比较（`tools/ode_stop_census.py`）——

```text
Ode    停点逐位相同 490 / 550 发（462 / 515 批）
Normal 停点逐位相同 619 / 740 发（480 / 565 批）
```

另外：`Ode` 里 **12 发**的弹道是"y 全程不变"的直线，**12 发的管线口径停点与回放全部逐位相同**
（`tools/ode_stop_census.py`：`flat_y == true` 的 12 发，`stop` 与 `end_x` 差 0）——
其中 **10 发**停在 `|x| = 25` 的硬边界上（`24.99136449816757` / `24.993300784265678` /
`24.9949902094923` / `24.99509111560907` / `24.99552816315917` / `24.998112977661386` …），
2 发停在障碍上（`1782763601` #4 停在 `-19.99277505242003`、`1782764906` #0 停在
`-20.75531942288392`，掩膜命中点分别是第 375 / 3787 个采样点）。
⇒ 边界值 25 与"撞到障碍就停"都**不是我们猜的**，都是被测出来的；而 #0 那一批仍然失败，
原因在 §39.4（空函数串让整条弹道在管线里消失），**与停止判据无关**。

### 41.3 ① 普查：`Ode` vs `Normal` 的"走到边界"批数

口径（`tools/ode_stop_census.py` 的表头有完整定义）：**我方没有被障碍挡住**（`hit_mask = None`）、
一路采到 `|x| = 25`，而回放停在我方点列的**中间**某点、**且该点上我方 y 与回放 `pos_y` 逐位相同**。

| 轴 | 分母 | 内容命中（`batch` 驱动） | "走到边界"（严格） | 放宽：前缀相同但回放更早停 | 粗口径：x 幅度大 >0.5 |
|---|---|---|---|---|---|
| `Ode` | 550 发 / 515 批 | **288 / 515** | **0 发 / 0 批** | 2 发 / 2 批 | 77 发 / 72 批 |
| `Normal` | 740 发 / 565 批 | **355 / 565** | 11 发 / 11 批 | 29 发 / 29 批 | 221 发 / 196 批 |

* **严格列 `Ode` = 0** ⇒ 任务书 §1 那条假设（"如果走到边界是普遍现象，就是停止判据整体错"）
  **被证伪**：`Ode` 一批都没有。
* `Normal` 那 11 批**全部来自同一份回放 `1783267628`（#15/#18/#20/…）**，而且回放的
  `end_x` 等于**开火者自己的 x**（我方点列的第 0 点） ⇒ 那是"这一发在回放里等于没走"，
  另一类异常，**不是**走到边界。
* **粗口径列不能当缺陷数用**：它只比"x 位移"，弹道本身就错的批也会命中
  （`Ode` 77 → 严格 0，就是这个差别）。任务书 §0 引用的 `-24.9928` 属于这一列的成因：
  `replay_endx_probe` **绕过了掩膜**，只跑采样器。
* 回放自身 `end_x` 与 `pos_x` 的关系（投到我方点列上）：`end == pos` `Ode` 494 发 /
  `Normal` 659 发；**`pos = end + 1`**（回放的终点比它自己的爆炸点退一个采样点）
  `Ode` **52 发** / `Normal` **79 发**；亚步差 4 / 2 发。见 §39.5。

### 41.4 样本 `1782764906` 批 #0 的真因：**空函数串**（不是停止判据）

任务书 §0 的复现样本：

```text
回放 end_x = -20.75531942288392 ；pos_y = 2.4950410243592103
probe(只跑采样器) stop = -24.99281942288328     ← 任务书看到的就是这一行
管线口径（采样器 + 掩膜）stop = -20.75531942288392  ← 与回放**逐位相同**
```

那一发 `function == ""`（`FunctionInfo{function:"", corrected_function:""}`，tick 26987）。
空串 ⇒ `parse("")` 得空 token 树 ⇒ `eval(空, …) = 0` ⇒ `dy/dx = 0` ⇒ **一条 y = y0 的水平直线**。
回放的 `end_x` 正是这条直线撞上障碍 #4（`AddObstacle(-22.76708968732609, -0.6655441843576924, r=3.7426175957326127)`）
的那个采样点：我方点列第 **3787** 点 = `-20.75531942288392`（Δ = 0），
精确圆判据的第一个内部点是第 3788 点（掩膜比解析圆小一个像素级）。

**但管线里 `end_x = 0.0`**，因为 `graphwar_shot_calculator::shot_points` 对空 token 树
提前返回空 `Vec`（那条早退在"空函数 = 没有弹道"的旧假设下写的）。全语料 **1291 发**里
`function == ""` 的有 **6 发**（5 份回放）。

**反事实测量**（本分支实测，改完即还原；**后来由协调者在 `f95a2a95` 正式落地**，见下）：把早退收窄成
`if toks.is_empty() && mode != Mode::Ode { return Vec::new(); }` ——

| 轴 | 改前 | 改后 | 其中 `Ode` | 其中 `Normal` |
|---|---|---|---|---|
| `batch`（内容命中） | 643 / 1080 | **647** | 288 → **292**（/515） | 355 → **355**（/565，未退） |
| `fire`（内容命中） | 608 / 1080 ⚠️**旧口径** | **612** ⚠️ | 277 → **281**（/515） | 331 → **331**（未退） |

变干净的 4 批：`1782764906` #0 / #1 / #5、`1783342151` #2（全是 `DiffEq`）。

⚠️ ~~**没有在本分支落地**~~ **审计 2026-11-08：已落地**（`f95a2a95`，闸门 `mode != Mode::Ode`；
`guardrails.json` 的 batch/fire = 647 即其结果）。原文保留如下，记录"交付时为什么没落地"：
`src/graphwar_shot_calculator.rs` 不在 `dispatch/29` §4 的允许清单里
（只许动 `src/graphwar_path_calculator.rs` 的 Ode 停止判据）。落地位置与判据都写在这里，
请协调者按 `AGENTS.md` 的分支纪律另开一刀；`Normal` 侧不动的那一行靠 `mode != Mode::Ode`
这个闸保证（它只放行 `Ode`，`Normal` 的早退逐字保留）。
**这条不改变 `Ode` 的停止判据**，改的是"空函数串该不该有弹道"。

### 41.5 顺带查清的两件事（都是**结论**，但机制仍未知）

**(a) 回放自己的 `end_x` 比它的 `pos_x` 退一个采样点。**
`Ode` 52 发 / 52 批、`Normal` 79 发 / 70 批（这批就是归因表的 `E1` 桶：`tools/bit_exact_attrib.py`
给出的 `Ode` 52 批与此**逐数相同**）。典型一串（`1782760054` 批 #0，owner 482830，`f ≡ −0.8`）：

```text
回放 FunctionPoints.end_x =  22.288479589977186   ← 我方点列第 4650 点（逐位相同）
回放 EffectInfo(Explosion).pos_x = 22.295979589977183
回放 pos_y = -14.999734885497316
我方点列第 4651 点 = (22.295979589977186, -14.999734885497313)   ← 与 **pos** 逐位相同
```

⇒ 回放的**爆炸点**是我方弹道的最后一点，回放的 `end_x` 却比它退一个点。
**这不是"我们停早了"**（我方弹道更长的那一截在回放里根本没有对应点），也不是
"末位差"（差的是**整步** 0.009375）。**机制未知**：候选解释有
（① `end_x` 与 `pos` 走两条不同的写路径；② 采样器把"越界候选"附在数组尾部、
`end_x` 取 `len−2`；③ 客户端预览采样与服务器采样的最后一个点不同），
**缺的证据是**：x86 `GraphwarShotCalculator::calculate_shot_results`（`0x3c2bc0`，12 KB）
里是谁写 `FunctionPoints.end_x`、写的是数组哪一项。本次没有逐条核完那个函数。

**(b) 三个"我方多一个点"的样本，逐指令证明采样器不可能在那里停。**
`1782926595` 批 #1（`222(-y+( 7 ))`）、`1782926882` 批 #0（`222(-y+(13+…))`）、
`1783255470` 批 #8（`2(-y+(sin(1)))`）：这三发在末尾都进入"y 恰好等于目标值、`dy/dx` 恒 0"
的直线段（我方 `y` = `7.0` / `13.0` / `0.8414709848078906` = `sin(1)`，**与回放 `pos_y` 逐位相同**），
此时每一步的 `chord` 恒等于 `h`、4 次二分就收敛，`|mid|` 与 `|y|` 都离边界还有 0.017 以上 ⇒
**五个出口一个都命中不了**，采样器不可能在回放那个点上停。而回放确实停了（差一整步 0.009375）。
⇒ 与 (a) 同源或同族，**未知**；`docs/06` §34.7 第 2 条（`E1` "差一个采样点"）就是这笔账。

### 41.6 仍未确定的（诚实清单）

| # | 事项 | 缺什么证据 | 档 |
|---|---|---|---|
| 1 | 回放 `end_x` 与 `pos_x` 差一个采样点的**机制**（§39.5a，`Ode` 52 批 / `Normal` 79 发） | `calculate_shot_results`（x86 `0x3c2bc0`）里 `FunctionPoints.end_x` 的写入点没有逐条核 | **未知** |
| 2 | §39.5b 那三发"回放比我们少一个点" | 同上；或需要 arm64/Steam 那一版的采样器逐条再核一遍（本次只核了五个出口与常量） | **未知** |
| 3 | 空函数串在原版里是否**总是**一条水平直线 | 只有 6 发样本（5 份回放），且都是 `DiffEq` 局；`Normal` 局的空函数串**没有样本** | **推断（强）** |
| 4 | 掩膜（`plane::rasterize`）与解析圆判据的像素级差 | 本次只观察到"掩膜比解析圆大一个采样点"（样本批 #0：掩膜命中 3787、解析圆 3788），未做系统性对拍 | **观测（单例）** |

### 41.7 复现（全部只读语料/二进制）

```bash
export CARGO_HOME=<WS>/.reference/cargo-home CARGO_TARGET_DIR=<WS>/.agent-work/<you>/target
R=<WS>/.reference/replays/gw2-replays-snapshot/replays

# ① 逐发普查 + 汇总（本节的 ① 表）
cargo run --release --offline -q --example ode_stop_census -- --dir $R --json $OUT/census.jsonl
cargo run --release --offline -q --example replay_attrib   -- --dir $R --drive batch --policy default --json $OUT/b.jsonl
python3 tools/ode_stop_census.py --census $OUT/census.jsonl --attrib $OUT/b.jsonl

# ② 停止原因追踪（自校验：与 calculate 逐位比）
cargo run --release --offline -q --example ode_stop_trace -- --dir $R --file 1782764906 --batch 0 --tail 4
cargo run --release --offline -q --example ode_stop_trace -- --text "" --x0 20 --y0 0 --ode --tail 3

# ③ 停止判据的回归锁（6 条）
cargo test --offline --test ode_stop

# ④ 逐指令（x86 / arm64）
objdump -d --start-address=0x3c25e0 --stop-address=0x3c2940 <WS>/.reference/steam/2026-08-28/libgraphwarrust.so
llvm-objdump -d --start-address=0x3b5558 --stop-address=0x3b55c8 <WS>/.reference/steam/android-2.0.60/libgraphwarrust.so
```

### 41.8 验收（本分支实测）

| 项 | 结果 |
|---|---|
| ① 走到边界（严格）：`Ode` / `Normal` | **0 批 / 11 批**（`Normal` 的 11 批同一份回放、回放 `end_x` = 开火点，另一类异常） |
| ② 原版 `Ode` 停止条件 | 五个出口 + 常量 + 预算公式，x86 与 arm64 **双证**（§39.1） |
| ③ `Ode` 内容命中 before → after | `batch` 288 → **288**（未改代码）；反事实 288 → **292**（§39.4，未入库） |
| ③ `Normal` 是否退 | `batch` 355 / `fire` 331，**一条未退** |
| ④ `cargo test --offline` | **1575 通过 / 0 失败**（基线 1569 + 本任务 6 条） |
| ④ `coverage_matrix.py --check` | OK（`src/` 一行未改） |
| ④ `check_doc_refs.py` | rc=0 |
| 三轴（`fixtures/guardrails.json` 未改） | `fire` 608/947 ｜ `batch` 643/1075 ｜ `batch_conv` 633/1075 ｜ 死亡 366/385 ｜ 挖洞 206/293 |

⚠️ `fire` 轴的数**口径正在被别的任务改**（`agent/fire-batch`：多发批改走 `fire_batch`），
本节引用的是 `CG@1877049` 上的实测值，**只作本次改动的对照**，不要当最新基线。
