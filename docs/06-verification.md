# 06 · 如何证明复原是对的

逆向最怕"看起来像"。本仓库的每条结论都标注证据等级，并说明它凭什么成立。

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

1. **`.eh_frame` 恢复函数边界** —— 编译器生成的调用帧表，比 Ghidra 的启发式可靠，
   得到 13430 个真实函数。
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
