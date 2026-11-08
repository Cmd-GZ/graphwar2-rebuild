# 52 · `agent/unicorn-matrix`：把解析器/求值器的**差分矩阵**放大一个量级

**优先级 P1（纯算力：unicorn 逐指令 emulation，单核慢、但可以并行多进程 —— 你们 16C32T 正合适）。**

## 0. 现状

上一轮修"无主 `)`"时用的手段很有效：

```
unicorn 直调真二进制 `0x4ed670`（解析器）/ `0x4eb9d0`（求值器）
349 条矩阵 + 随机不平衡串 → 0 不一致
真语料 117 条风险形状文本   → 0 不一致（其中 103 条**旧解析确实错**）
```

但 349 条矩阵只覆盖**很窄**的表达式空间。解析器是我们整条链的地基，值得压一遍。

## 1. 要做什么

1. 把矩阵放大到 **≥5,000 条**，覆盖：
   ```
   括号不平衡（多 ')' / 少 ')' / 交错）｜隐式乘法｜一元负号与 '^' 的优先
   '^' 左结合｜log/ln/sqrt/exp/abs 的连写｜数字形态（.5 / 5. / 1e3 / 1E-3）
   空格 ｜ 空串 ｜ 超长串（>1 KB）｜ 非法字符 ｜ 'y' 与 "y'" 的切词
   ```
2. **并行跑**（每核一个 unicorn worker），记录每条的 token 串；与本项目 `math_function::parse` 逐条比；
3. **不一致的全列出来**（输入 → 真值 token 串 → 我方 token 串），并按模式分类；
4. **能修的就修**（`src/math_function.rs`），修完重跑矩阵；
5. **把矩阵与结果入库**（文本），供以后回归。

## 2. 硬约束

- **不许改既有断言**让它通过；不一致就是不一致，先报再修；
- 数字不许退：`cargo test --offline` **1638**、三轴 `fire 647/1075`、`batch 647/1075`、
  死亡 `366/385`、挖洞 `207/294`；
- ⚠️ **护栏不敏感**（上一轮教训）：解析器改动可能**一个轴都不动** —— 别拿轴绿当"改对了"的证据，
  证据是矩阵本身；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/unicorn-matrix`
- **私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- unicorn 与 python：本项目 `.venv`（`pip install unicorn capstone`）
- 目标二进制：`<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`（sha `9a538115…`）
- 既有工具可复用：`tools/nan_c_probe.py`（unicorn 直调三个真函数的写法）、`examples/parse_dump.rs`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/unicorn-matrix`

## 4. 可以改的文件

- `tools/`（矩阵生成 + 比对）、`fixtures/`（矩阵与结果，**文本**）
- `src/math_function.rs`（修不一致）
- `docs/03-function-language.md`（追加不一致清单与结论）
- ❌ 不许改 `src/game.rs`、`client/`、`dispatch/`

## 5. 验收

```text
① 矩阵规模（几条）、并行跑了多久
② 不一致几条（逐条列出，按模式分类）
③ 修了几条、修完复跑结果
④ 矩阵与结果入库位置
⑤ cargo test 1638；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、矩阵规模与耗时、不一致条数与分类、修了几条、复跑结果、入库位置、`cargo` 结果。

## 6b. 并行要求（补）

**必须真的吃满机器**：unicorn 是单核活，矩阵 ≥5,000 条请按 `--jobs N`（N ≥ 16）分片并发，
汇报时给出墙钟时间与加速比。若有 GPU 可用（RTX 4060）也只在"表达式生成/结果比对"这类
可向量化的部分考虑，**不要为用 GPU 而用 GPU**。
