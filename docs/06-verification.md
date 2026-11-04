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
| 函数边界 | ✅ `.eh_frame` 恢复出 13430 个 | 秒级 |
| 模块归属 | ✅ 首批 56 个模块 / 483 个函数 | 秒级（依赖上面的索引） |
| Ghidra 12.1.4 反编译 | ✅ 已装并验证 | **首次全量分析约 19 分钟** |
| REA 的 MCP 原生分析 | ⚠️ 受客户端超时限制 | 见下 |

**为什么 MCP 侧的 Ghidra 目前不好用**：导入 + 全量 auto-analysis 约 19 分钟，
而 DeepSeek Harness 的 MCP 调用默认 60 秒超时。REA 自身可以放宽
（`ghidra.startup_timeout_ms` 已设到 40 分钟），但**客户端**的截止时间在注册配置里，
需要给 `rea-mcp` 条目加 `toolCallTimeoutMs` 并重启宿主。

因此当前的重活走 REA CLI（无客户端截止时间），MCP 侧只做秒级的无 provider 操作。

**另一个负结果**：这个 `.so` **没有 `.dep-v0`（cargo-auditable）段**
（`inspect_rust_binary` 已确认），所以无法从依赖清单反推 crate 结构。
