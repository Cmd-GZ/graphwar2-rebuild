# 25 · `agent/frida-point-array`：在**真游戏**里抓点数组的填充者（运行时）

**优先级 P0（若你能跑起真游戏；跑不起来就如实说，不要硬做）。**

## 0. 为什么需要运行时

任务 20 是**静态**追"谁往点数组 `[x19+0x30..0x40]` 里写"。静态已经**卡住两次**：
`execute_results`（arm64 `0x4a8558`）里只找到 `mem::take` 式的**清空写**，找不到填充者。
而这一处同时卡着两条悬案：**1 ulp 残差**与**"同坐标不同序号"**。

⇒ **在真游戏里断点抓一次，比继续静态翻十倍快。**

## 1. 已知（别重推）

```
execute_results        arm64 0x4a8558，体 3712 B ⇒ 到 0x4a93d8
ExplosionEffect 那处    0x4a89ec；pos 取**点数组末元素**（0x4a891c ldp d9,d8,[ptr+len*16-0x10]）
另一处                  0x4a8cb0；pos 来自 0x3b7f58 返回的 Option<(f64,f64)>
点数组字段              [x19+0x30 / +0x38 / +0x40]（ptr / len / cap）
采样器                  0x3c2530（x86）= calculate_function；arm64 0x3b5374
                       **零 call rel32**，唯一入口是函数指针槽 0x9bac98
求值器                  0x4eb9d0（x86）；**已逐位排除**，1 ulp 不在它那儿
```

## 2. 要做什么

**目标顺序（能到哪步算哪步，每步都记）**

1. **让游戏跑起来**：Steam 版 `Graphwar II`（Windows 或 Linux 均可；
   Linux 那份我们有未 strip 的 Android 版做符号参考，Windows 有 939 个真名的 DLL —— **Windows 更好调试**）。
2. **断点**：在 `execute_results` 的点数组写入处下断（`[x19+0x30]` 的写入点 / `memcpy` / `Vec::extend`）。
   推荐下在**清空写之后**，观察谁在之后填数据。
3. **答案要三样**：
   - 填充者是谁（模块 + 函数 + 地址）；
   - 写进去的 `y` 与**采样器返回值**是否逐位相同（`1 ulp` 就在这一步或不在）；
   - "同坐标不同序号"：同一批里两条 `DestroyObstacle` 的坐标相同、序号不同 —— **序号从哪来**。
4. **抓取脚本要交**：`tools/frida_point_array.js`（可复跑），并写清下断地址来自哪个构建（**地址必须带构建版本**）。

## 3. 环境

- 分支 `agent/frida-point-array`（⚠️ **不是 `CG/`**，推不上去）
- 工具包：`dist/gw2-toolkit.tar.zst`（含 frida 相关说明与两个 `.so`）
- frida：`pip install frida-tools`（或工具包里的 venv）
- ⚠️ **`/tmp` 是逐命令 tmpfs**，临时产物放工作区
- ⚠️ 如果你**没有**能跑这个游戏的机器/环境：**直接回报"做不了 + 缺什么"**，
  不要用静态推测冒充运行时结论（`AGENTS.md` §1 第一条）

## 4. 可以改的文件

- `tools/`（新增 frida 脚本与说明）
- `docs/06-verification.md` 或 `docs/14-turn.md` 追加一节（**必须写明是运行时观测，标 `观测`**）

❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 游戏是否真的跑起来了（截图/日志/版本号）——没跑起来就到此为止
② 填充者（有/无 + 地址 + 模块）或"缺什么"
③ 1 ulp：写进去的 y vs 采样器返回值，逐位对照（若取到样本）
④ 序号来源（若取到样本）
⑤ tools/frida_point_array.js 可复跑，注明地址对应的构建
cargo test --offline 全绿（基线 1559）
```

## 6. 汇报（≤10 行）

分支+sha、**游戏跑起来了吗（哪一版、怎么跑的）**、填充者（有/无 + 地址）、
1 ulp 是否在这一步、序号来源、脚本位置、`cargo` 结果。
**没跑起来就只写"做不了 + 缺什么"，那也算合格交付。**