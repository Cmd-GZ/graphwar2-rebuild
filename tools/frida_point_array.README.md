# `frida_point_array` —— 在真游戏里抓点数组的填充者

`dispatch/25`（P0，运行时）交付物。两个文件：

| 文件 | 作用 |
|---|---|
| `frida_point_array.js` | Frida 探针：挂 `execute_results` / 开火队列 / `calculate_function` / 求值器，在**进程内**判定"写进去的 y 与求值器返回值是否逐位相同" |
| `frida_point_array.py` | 驱动：用 frida `spawn` 拉起真游戏 → 载入上面那个脚本 → 用 `xdotool` 点进对局并长按开火 → 把消息写成 JSONL |

结论见 [`docs/06-verification.md` §33](../docs/06-verification.md)。

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
(1000,1048)  f(x) = 输入框（然后 xdotool type "sin(x)+3"）
(1855, 50)   Fire!  ← **长按**：mousedown; sleep 2; mouseup
```

⚠ `FireButton` 在 `play_screen.tscn` 里带 `long_press_input_text = "fire"`，**单击不动**。

## 输出（JSONL，每行一条 `send()`）

| type | 内容 |
|---|---|
| `hooks` | 下断结果 + 指针槽 `0x9bac98` 前后 16 个字的运行时取值（核对"采样器是谁"） |
| `exec_enter` | `execute_results` 入口：`self`、精确调用栈、此前求值/采样计数 |
| `exec_queue` | **主证据**：队列原文、每个元素的 `kind/ptr/len`、点数组 head/tail/`yhash`、`classify` |
| `evals` / `samplers` | 求值器/采样器的抽样行：精确返回地址、入参、返回值位模式 |
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
