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
