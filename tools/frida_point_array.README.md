# `frida_point_array` —— 在真游戏里抓点数组的填充者

`dispatch/25`（P0，运行时）交付物。两个文件：

| 文件 | 作用 |
|---|---|
| `frida_point_array.js` | Frida 探针：挂 `execute_results` / 开火队列 / `calculate_function` / 求值器，在**进程内**判定"写进去的 y 与求值器返回值是否逐位相同" |
| `frida_point_array.py` | 驱动：用 frida `spawn` 拉起真游戏 → 载入上面那个脚本 → 用 `xdotool` 点进对局并长按开火 → 把消息写成 JSONL |

结论见 [`docs/06-verification.md` §35](../docs/06-verification.md)。
**独立复现那一轮（同文件 §35.8）**记下了这两支脚本被改的三处（全部**只读**、不改游戏内存）、
以及入库样本 [`../fixtures/frida_point_array_sample.jsonl`](../fixtures/frida_point_array_sample.jsonl)。

## 地址对应的构建（**必须**随版本看）

```text
构建      Graphwar II Linux 2.0.60（Steam）
二进制    .reference/game/libgraphwarrust.so
sha256    9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187   ← 与逆向目标逐字节相同
口径      RVA = ELF 虚地址 = 模块内偏移；下断 = base + RVA；Ghidra 地址 = RVA + 0x100000
```

| RVA | 函数 | FDE 范围 |
|---|---|---|
| `0x472470` | `execute_results` | `[0x472470, 0x473513)` |
| `0x4724cc` | 同上体内：开火队列预扫（`rbp`=self；队列 ptr/len/cap = `+0x38/+0x40/+0x48`，元素步长 `0x20`） | — |
| `0x3c2530` | `calculate_function`（**点数组的生产者**） | `[0x3c2530, 0x3c2bb9)` |
| `0x4eb9d0` | 求值器（`src/math_function.rs`） | `[0x4eb9d0, 0x4ebbe0)` |
| `0x9bac98` | 旧文档的"采样器函数指针槽"（运行时读出 = `base+0x3c2530`） | — |

函数边界用 `.eh_frame` 复核：

```bash
GW2_SO=.reference/game/libgraphwarrust.so \
  .reference/venv/bin/python tools/addr_probe.py fde 0x472470
```

## 跑起来（两个已踩过的坑）

1. **`/tmp` 是逐命令 tmpfs**：`Xvfb` 的 `/tmp/.X11-unix` 跨命令不可见 ⇒ 用 **TCP** 监听。
2. **Xvfb 直接起会 core dump**（`libEGL_nvidia` 串到 `swrast_dri`）⇒ 只让它看见 mesa：

```bash
mkdir -p /tmp/.X11-unix            # 同一个命令里做
export __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json
export LIBGLX_VENDOR_LIBRARY_NAME=mesa LIBGL_ALWAYS_SOFTWARE=1
Xvfb :92 -screen 0 1920x1160x24 -listen tcp -ac &
```

```bash
# 全自动：spawn 真游戏 → 挂钩 → 点 Campaign/关卡/Start → 输入 sin(x)+3 → 长按 Fire!
.reference/venv/bin/python tools/frida_point_array.py --out /abs/run.jsonl --display 127.0.0.1:92
```

`--no-clicks` 只挂探针，自己玩着点（适合挂到已经进到对局的进程上）。
`--settle/--tail` 是"启动后等多久"和"开火后再录多久"（弹道飞完才触发 `execute_results`，
**至少留 15 s**）。

## 点菜单的坐标（1920×1160，实测）

```text
(534, 629)   主菜单 Campaign
(243, 224)   关卡列表里唯一的节点
(1072,1009)  Level 0 弹窗上的 Start
(1000,1048)  f(x) = 输入框（**先 `ctrl+a` 全选**，再 `xdotool type "sin(x)+3"`）
(1855, 50)   Fire!  ← **长按**：mousedown; sleep 2; mouseup
```

⚠ `FireButton` 在 `play_screen.tscn` 里带 `long_press_input_text = "fire"`，**单击不动**。

⚠ **点输入框只是把光标放进去**。不先 `ctrl+a` 就打字，新文本会**接在旧文本后面**
（2026-11-07 实测：想打 `x/2`，实际生效 `sin(x)+3x/2`）。所以 `CLICKS` 与 `--extra-shot`
都在打字前插了一步 `key --clearmodifiers ctrl+a`。

## 输出（JSONL，每行一条 `send()`）

| type | 内容 |
|---|---|
| `hooks` | 下断结果 + **函数指针槽 `0x9bac98` 的运行时取值**（`slot_9bac98.target_rva`，核对"采样器是谁"） |
| `exec_enter` | `execute_results` 入口：`self`、精确调用栈、此前求值/采样计数 |
| `exec_queue` | **主证据**：队列原文、每个元素的 `kind/ptr/len`、点数组 head/tail/`yhash`、`classify`；不可读的指针显式记 `skipped` |
| `evals` / `samplers` | 求值器/采样器的抽样行：精确返回地址、入参、返回值位模式。`samplers` 行**尾部多两项**：`[11]` = `calculate_function` 的返回记录 `[rax, rax+0x18)`（`{kind, ptr, len}`），`[12]` = `{xmm0, xmm1}` 全 16 字节 |
| `reallocs` | `realloc(size ≥ 16 KB)` 且调用方在 `.so` 内的调用栈（`Vec` 扩容点） |
| `beat` | 心跳：已抓多少次求值/采样/结算 |

`classify` 的字段（在游戏进程里算，避免搬几十万行回宿主）：

```text
n          点数组长度
exact      该点的 y 位模式在求值器返回值集合里出现过（**逐位相同**）
ulp1       只差 ±1 ulp
miss       都没有（正常只应有 i=0 那一个：炮口/出生点，不是一次求值）
pairExact  (x, y) **两个位模式都**命中
pairWho    这些命中来自求值器的哪个返回地址（`gw+3c2995` = `calculate_function` 体内）
```

## 2026-11-08 `dispatch/34`：两处 `(ptr, n)` 读数（`docs/06` §42）

这一轮给同一支探针加了一个**报文侧的断点**，用来判"报文读的点数组是不是就是
`calculate_function` 算出来、进队列的那一块"，以及"每发两次 `calculate_function`
（`gw+3c30c7` / `gw+3e2d3b`）返回的两条数组是否逐位相同"。

| 新增 RVA | 是什么 | 抓什么 |
|---|---|---|
| **`0x472865`** | `execute_results` 里 `movdqu -0x10(%rdx,%rcx,1),%xmm0`（`pos = pts[n-1]`） | 执行前：`rdx` = 数组基址、`rcx` = n×16、`r13` = n、`r15d` = 队列项 `+0x18` 低 32 位；那 16 字节；整条数组摘要；与队列项 / 两次 `calculate_function` 返回的比对 |

新增消息类型：

| type | 内容 |
|---|---|
| `calc_array` | 每次 `calculate_function` 返回：`seq`（全局自增）、`ptr/len/kind`、`digest_xy`/`digest_y`（FNV-1a 64，覆全部 16·len 字节 / 只算 y）、首末点，以及与最近 8 个数组的**逐点逐字节**比较结果 `vs` |
| `pos_read` | `movdqu` 那一刻的 `rdx/rcx/n/r13/r15`、`pos_bytes`、摘要、`vs_queue`（对入口扫到的队列项）、`vs_calc`（对两条数组） |
| `selftest` | 装机时的**正对照**：合成数组 相同/翻 1 bit/翻 2 bit 三档，确认比对函数报得出不同 |

驱动新增开关：

```bash
--attach                # ★ 本机必须：frida spawn 会把这游戏挂在 wait4、永不建窗口（docs/06 §42.1）
--shots N --gap S --exprs "a,b,c"   # 连打 N 发，逐发轮换表达式
--snap-dir DIR          # 每步按**窗口 id** 截图（root 全黑）
--eval-rows 0           # 关掉 evals 抽样，连发时 JSONL 不会爆
--extract raw.jsonl --extract raw2.jsonl --label series1 --label series2 \
    --fixture-out fixtures/frida_ptr_pair_sample.jsonl   # 摘成入库用的纯文本样本
```

`--extract` 的输出 = 1 行 provenance + 每发 1 行（字段见 `fixtures/frida_ptr_pair_sample.jsonl`），
**不含路径、不含用户名、不含整条数组的十六进制**。

⚠ 坐标与点击的两个新坑（§42.1）：输入框在 `y≈1090`（不是 1048）；
`mousemove X Y click 1` **一条命令**会把点击按旧指针位置派发，必须拆成两条。

## 2026-11-08 `dispatch/39`：开火那一刻喂进 `calculate_function` 的 **token 列表**（`docs/06` §26）

第三个断点组，**独立函数 + 独立开关**（并行任务 40 也在改这个文件，所以新增的东西全部收在
`TOKCFG` / `installTokenProbe()` / `recv('cfg')` 的 `tokenProbe` 分支里，主探针一行没动）：

| 新增 RVA | 是什么 | 抓什么 |
|---|---|---|
| **`0x3c2530`** | `calculate_function`（采样器）入口 | `rsi` = `&Vec<Token>`：`{cap, ptr, len}` + **16·len 字节原文**；`edx`=flip、`ecx`=mode、`xmm0/1/2`=x0/y0/step 的**位模式**；调用点 RVA |
| **`0x4ed670`** | `MathFunction::new`（解析器）= 槽 `0x9bacf8` | `rsi` = `&String` 输入文本：**UTF-8 字节 + 长度**；返回的 `Vec<Token>` 也抓（`cap/ptr/len` + 字节） |
| **`0x3c5f70`** | `calculate_function_offset_str` | onLeave 读 sret `String` = `" + 10.938"` / `" - 0.000"`（修正常数的**文本**） |

新增消息类型：

| type | 内容 |
|---|---|
| `tok_fire` | 每次 `calculate_function` 入口：`ret`（`gw+3c30c7` 或 `gw+3e2d3b`）、`flip/mode/x0/y0/step`、`vec{cap,ptr,len}`、`tok{hex,tags}`，以及 `vs_parse` —— 与最近 256 次 parse 的比对（`ptr_eq` 指针恒等 / `same_bytes` 逐字节） |
| `tok_parse` | 每次解析器调用：输入文本 `txt`/`txtHex`/`txtLen`、调用点、输出的 token 列表 |
| `tok_suffix` | 每次 `offset_str` 返回的后缀串（`text` + `hex`）+ 当时的 `x0/y0` 位模式 |

⚠ 这三个钩子**装机时就装上**（`TOKCFG.on=false` 时回调第一行就 `return`），开关由驱动
`--tokens` → `post({type:'cfg',tokenProbe:true})` 打开：这样不赌 `install()` 与 `cfg` 的先后。

驱动新增开关：

```bash
--tokens                     # 打开 token 探针（不加 = 行为与之前逐字节相同）
--tokens-report raw.jsonl    # 离线摘要模式（不给游戏）：可给多次
--replay <user://replays/<unix 秒>>   # **同一局**的回放（gzip JSONL）→ 才能做"抓到的文本 vs corrected_function"
--oracle 1                   # 再用真二进制（unicorn 直调 0x4ed670）解析一遍，与抓到的 token 逐字节比
--tokens-fixture-out fixtures/frida_token_at_fire_sample.jsonl
```

`--tokens-report` 的输出 = 1 行 provenance + 每次 `calculate_function` 一行，含
`tokens.hex`（16·len 字节原文）、`tokens_decoded`（tag + f64 位模式）、`prefix_text`（前缀写法，
**只是给人看的忠实渲染，不是 `0x4ec160` 的 `to_text`**）、`matched_parse`（那一次 parse 的文本与 hex）、
`suffix_matches`、以及 `A/B/C` 三条逐字节结论。**不含路径、不含用户名、不含整条回放的二进制**。

⚠ 两个新坑（`docs/06` §26.8）：
1. **不要用共享 Xvfb**。本轮在别人已经开着的 `:92` 上截图/点击，落到的是**别人 agent 的游戏窗口**，
   于是"主菜单"其实在别人那台机上、连点五下没开成炮。自己起 `:97` 后一次就对。
2. 点菜单坐标（1920×1160，本机实测仍然有效）：`(534,633)` Campaign → `(237,224)` 关卡节点 →
   `(1070,1010)` Start → 输入框 `(1000,1106)` → `Fire!` 长按 `(1754,45)`。
   截图必须按**窗口 id**（`xdotool search --name '^Graphwar$'`），root 全黑。

## 2026-11-08 `dispatch/44`：`FunctionPoints.end_x` 的**装配处**（`docs/06` §28）

第四个断点组，**独立函数 + 独立开关**（并行任务 45 也在改这个文件，所以新增的东西全部收在
`FPCFG` / `FPST` / `installFunctionPointsProbe()` / `recv('cfg')` 的 `fnpoints` 分支里，其余探针一行没动）：

| 新增 RVA | 是什么 | 抓什么 |
|---|---|---|
| **`0x456810`** | `FunctionPointsEvent` 的**装配**（`graphwar_function_manager.rs`） | `rdi`=self、`esi`=owner、**`rdx`=`&Vec<(f64,f64)>`**；体内 `0x4569b8 movq -0x10(%rcx,%rax,1)` = **`pts[len-1].x` → `end_x`**、`0x4569be movdqu (%rcx)` = `pts[0]` → `start_x/start_y`、`0x4569ce` = `owner_id`(u32)、判别式 `0x8000000000000025` = 37 |
| **`0x50eb20`** | `FunctionPointsEvent` → JSON 写入器 | `rsi` = `&event`，`start_x@+0`、`start_y@+8`、`end_x@+0x10`、`owner_id@+0x18` ⇒ **报文里那个 `end_x`**；同处记返回地址与调用栈 |
| **`0x472865`** | 与主探针**同一处** `movdqu`（再挂一个只读监听） | 把这一刻的 `pos` 在**每条 `calculate_function` 数组里定位到下标**（`x_idx`/`y_idx`/`xy_idx`），并与最近一次装配的 `(ptr,len)`、`end_x` 对撞 |

新增消息类型：

| type | 内容 |
|---|---|
| `fnpoints_ctor` | 每次装配：`caller`（`gw+471a8d` = 状态机侧 / `gw+3e44d9` = **target_manager** 侧）、`vec{cap,ptr,len}`、`start_pt`、`end_pt`、`end_x`、`owner_id`、与最近 8 条数组的 `same_ptr`/`same_bytes` |
| `fnpoints` | JSON 写入器那一刻：`end_x`/`start_x`/`start_y` 的**位模式**、`owner_id`、调用栈、`vs_calc`（`end_x` 在每条数组里**命中的下标**、是否等于末点/倒数第二点） |
| `pos_x_locate` | `0x472865` 那一刻：`n`、`pos_bytes`、`pos_x`、`last_fn{equal_x}`、`last_ctor{same_ptr,same_len,equal_x}`、每条数组的 `x_idx/xy_idx/same_bytes` |

驱动新增开关：

```bash
--fnpoints                      # 打开上面这组钩子（不加 = 行为与之前逐字节相同）
--campaign-level 20             # 开跑前把**工作区** XDG_DATA_HOME 的 user://campaign.save 写成 level_20
--level-node 16                 # 点第 16 关（内置 21 个实测坐标；也可给 "x,y"）
# 不给游戏，只把带 --fnpoints 的原始 JSONL 摘成入库样本（规则在代码里，可复跑）：
--endx-extract raw.jsonl --endx-extract ode.jsonl --label normal_l0 --label ode_l16 \
    --series-mode Normal,DiffEq --series-level 0,16 --series-exprs "sin(x)+3,x/2" \
    --series-exprs "y,-y,sin(x)-y/2" --endx-fixture-out fixtures/frida_endx_sample.jsonl
```

⚠ 三个本轮新踩的坑（`docs/06` §28.7）：

1. **本机是 niri/Wayland 会话**：`WAYLAND_DISPLAY` 存在时 Godot 走 `DisplayServerWayland`，
   **不在 Xvfb 上建 X11 窗口**（`xdotool search` 一直返回 `root`，整轮 0 发）。驱动现在启动前把它摘掉。
2. **Xvfb 要用后台作业起**（同一条命令里 `(Xvfb … &)` 起的会随命令退出被杀，
   下一条命令里游戏报 `X11 Display is not available`）。
3. 战役第 16 关弹窗的 `Start` 在 `(1072,1009)`、关卡节点 `(691,751)`（1920×1160）——
   `--level-node` 用的是内置坐标表，别手敲。

## 2026-11-08 `dispatch/48`：两装配点**同一发**同时抓 + 找"改长度"的写入者（`docs/06` §30）

一组**独立开关**（默认全关：不给这些开关时行为与之前**逐字节相同**；新逻辑都在
`installDivergeProbe()` 里，主探针 / token 探针 / FunctionPoints 探针一行没动）。

```bash
--diverge                       # 打开分叉探针（下面 5 类消息）
--dv-mam                        # 实验：item 刚 push 之后把它所在页设成陷阱（MemoryAccessMonitor，每页一次）
--div-extract <raw.jsonl>       # 不给游戏：把 --diverge 的原始 JSONL 摘成入库样本（可给多次）
--div-fixture-out <out.jsonl>   # --div-extract 的输出（纯文本；配 --label / --series-level）
```

消息（都是 `send()` 的一行）：

| `type` | 是什么 |
|---|---|
| `dv_ctor` | `0x456810`（`end_x` 装配点）入口：喂进去的 `Vec` 的 `(cap,ptr,len)` + **整条数组的字节副本**的摘要/首末点（两个调用点 `gw+471a8d` / `gw+3e44d9` 都抓） |
| `dv_pos` | `0x472865` 入口：读到的 `(rdx,n,pos 16 B)`，并与**每一个** `dv_ctor` 快照**逐点逐字节**比（`prefix_diff` / `firstDiff` / `pos` 在快照里命中的下标） |
| `dv_csr` / `dv_csr_enter` | `0x3c2bc0` `calculate_shot_results` 返回时：sret 的 4 个字、结果 `Vec` 的每一项（`kind/ptr/len/owner`；ptr 按 `+0xc<<32 \| +0x8` 拼，见 `0x4727e1`） |
| `dv_site` | 9 个"存储点"断点：`0x3c2d5e/6a/77`（push）、`0x3c30d7/e7`（采样器返回后存 ptr/len）、**`0x3c3bf7`/`0x3c3c2c`（★ 把 `len` 夹小）**、`0x3c50d5/eb`（拷进结果 Vec） |
| `dv_mam*` | `--dv-mam` 的页级访问探针（`MemoryAccessMonitor` **每页只报一次**；在 `onAccess` 里重武装会退化成同一条指令反复触发，别照抄） |

入库样本：[`fixtures/frida_array_divergence_sample.jsonl`](../fixtures/frida_array_divergence_sample.jsonl)
（10 发，`docs/06` §30）。

## 2026-11-09 `dispatch/66`：**三样同抓**（①采样器入口的 token ②`pts[n-1]` ③`EffectInfo` 的 JSON）

第五个断点组 + 一个"回合感知"驱动开关（默认全关：不给开关时行为与之前**逐字节相同**；
新逻辑全部收在 `EFFCFG` / `EFFST` / `installEffectProbe()` / `eff_payload` / `eff_json` 两条消息里）。

| 新增 RVA | 是什么 | 抓什么 |
|---|---|---|
| **`0x4729d6`** | `execute_results` 里**站点 A**（Explosion）往事件队列 push 的 `call *0x9bac80`；上一句 `0x4729d1 lea rsi,[rsp+0x70]` | `rsi` = **载荷原文 0x28 字节**：判别式 `0x…2c`(44) @+0、`pos_x` @+8、`pos_y` @+0x10、`effect_id` @+0x18、`start_tick` @+0x1c、`effect_type` @+0x20（=0 Explosion） |
| **`0x472e4d`** | 同形态的**站点 B**（Death/Spark/Ash） | 同上 |
| **`0x50f990`** | `EffectInfo` → **JSON 的写入器**（派发：`0x504640` 的 62 项跳表 `0x1c48d0` **第 44 项** = `0x50476e` → `jmp 0x50f990`） | `rsi` = `&载荷`（`pos_x@+0`/`pos_y@+8`/`effect_id@+0x10`/`start_tick@+0x14`）；**onLeave 读输出 String 的新增段 = 这条 `EffectInfo` 的 JSON 文本** |

消息：

| `type` | 是什么 |
|---|---|
| `eff_payload` | 装配点那一瞬：`payload_hex`（0x28 B）+ `pos_x_bits`/`pos_y_bits`（内存序）+ `effect_id`/`start_tick`/`effect_type` |
| `eff_json` | 写入器退出时：`len_before`/`len_after`、`text`（JSON 文本）、`text_hex`、同一刻的载荷位模式 |

驱动新增开关：

```bash
--triple                 # = tokenProbe + fnpoints + **effProbe**（三样一次开）
--round-aware            # 回合感知：按下 Fire! 后**等 tok_fire**，没等到就重试（--fire-retry/--fire-wait）
--triple-report <raw.jsonl> --triple-fixture-out <out.jsonl> [--oracle 1]   # 离线：摘样本 + 每发 A/B/C 判定
```

`--triple-report` 的输出 = 1 行 provenance + 每发 1 行（`fire`/`suffix`/`pos_read`/`effect`/`oracle`/`judge`），
**不含路径、用户名、整条数组的十六进制**；判据键见 `docs/06` §38。

⚠ 三个本轮新踩的坑（`docs/06` §38.6）：

1. **`ctrl+a` 不总能进全选**：焦点还没落到输入框时它发给的是窗口 ⇒ 新表达式**接在旧文本后面**。
   本轮抓到的 `parse_text` 里出现过 `2+1exp(x)/5-1`（想打 `sin(x)*2+1`）。`shot_actions` 现在
   在 `ctrl+a` 后补一条 `key Delete`。**判据只认 `parse_text`（实际喂进解析器的那串）**，不认我打算打的。
2. **`frida` 钩子里 `rdi/rsi` 是调用者保存**：`onLeave` 读到的**不是**入口的值。
   `eff_json` 第一版就是因此在 `len_before=-1`、`text=null` 上空的 —— 必须在 `onEnter` 抄下来。
3. **一次开火是两次 `calculate_function`**（`gw+3c30c7` 然后 `gw+3e2d3b`），两次的间隔
   **就是弹道飞行的时间**（实测 3–15 s；第二次在 `execute_results` 之前那一刻）⇒ 两发的调用
   序列会**交错**（第 N 发的第二次调用落在第 N+1 次尝试的窗口里）。所以：
   · 切发要**按调用点**（见到 `gw+3c30c7` 就开新的一组），不要按时间/按"计数有没有涨"；
   · 驱动判"这一发开出去了没有"要用 **`pos_read`**（结果真的执行了），不能用 `tok_fire`
     —— 用 `tok_fire` 会把上一发的第二次调用当成这一发生效（本轮实测：8 次尝试里只有 4–5 发
     真的开了出去，日志却报 8 次"生效"）。
   · 打字**不会**调 `0x3c2530`（输入框里那条曲线走的是 `0x3c5f70` 里的 parse+eval，不是采样器），
     所以"只有一次调用、没有第二次"= **那一发还在飞**（或者压根没开出去）。
