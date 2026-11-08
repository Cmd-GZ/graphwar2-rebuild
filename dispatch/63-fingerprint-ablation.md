# 63 · `agent/fingerprint-ablation`：用 **GPU** 做特征消融 ＋ 置信区间（顺带审每个特征的取值范围）

**优先级 P0。这是本项目**唯一真正需要 GPU** 的活：数百组配置 × 6.6 亿对。**

## 0. 起点（57 号刚把天花板从 73% 抬到 78.2%）

```
51 号：72.73%（断言"天花板 ≈73%"）—— 已被 57 号推翻
57 号：+ scal（全部 IMM 立即数 xref）→ **78.18 / 89.09**；
       + stok（真字符串语义）→ 72.73（**一格不动，第三次被否**）
       根因：原 `consts` 只收 0x100..0xFFFFFFFF，`<0x100` 的小常数**整段漏掉**
```

⚠️ **一个特征的取值范围假设错了，就藏掉了 5.5 个点。** 其余 15 个特征**没人系统审过**。

## 1. 要做什么（三件，GPU 主跑）

### A. 消融 ＋ 置信区间（**GPU 主力**）

```
· 配置空间（≥300 组）：
    - 16 个特征的**留一法**（leave-one-out）
    - `scal` / `consts` / `shape` / `gram` 的**权重网格**（各 3–5 档）
    - 关键子集的**子集枚举**（例如 {scal, consts, shape, gram} 的 2^4）
· 每组都要：留出半 top-1/top-5 + **bootstrap 95% 置信区间**（≥200 次重采样）
· 目标：给出"78.18% ± ?"、**特征重要性排序**、以及"过 80% 需要什么"
```

### B. 每个特征的**取值范围审计**（这就是 5.5 点的来源）

对 16 个特征逐个问：**它的取值域假设有没有把合法值排除掉？**

```
· `consts` 已经中招：只收 0x100..0xFFFFFFFF ⇒ 漏掉 <0x100
· 其它嫌疑：直方图分类是否漏了某类指令；size 比是否只取整数；
  立即数是否只收正数；字符串分桶是否只收 ASCII ≥ 某长度；被调者多重集是否漏了 thunk…
```

做法：对每个特征，**打印目标集合与正对照集合的实际取值分布**（分位点 +
"被排除的值有多少条"），凡两边分布显著不同就标红。**这一步是 CPU 活，也很快。**

### C. 只在新口径下重出夹具（不要动旧夹具的可比性）

```
· 明确写：新夹具与旧夹具**不可直接比**（特征口径变了）
· 给出"旧→新"的映射表：多少行的 match_addr 变了、多少行的 tier 变了
```

## 2. 硬约束

- **GPU 与 CPU 必须逐条一致**（348 正对照 top-1/top-5 全对）—— 沿用 61 号的实现，
  **注意 61 号已量到：小数据 GPU 反而慢（800 行 0.7×），所以别拿小矩阵测速度**；
- **不许再引入"正对照有、目标集合没有"的特征**（51/57 都在这上面栽过）；
- 并行/显存：显存 ≤ 4 GB 切块；若 OOM 就降 batch，写清实测；
- 数字不许退：`cargo test --offline` **1671**、三轴 `fire 647/1075`、`batch 647/1075`；
- 提交信息用 `-F 文件`；**大矩阵不入库**，只入"配置表 + 结果 + 置信区间"。

## 3. 环境

- 分支 `agent/fingerprint-ablation`
- **私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- GPU 环境（61 号已装好，**别重装**）：`.reference/gpu-venv`（torch 2.6.0+cu124）、
  `.reference/dsh-cuda`（插件，link 引用别删）、`~/.dsh/cuda.json`
- 四份构建：`<WS>/.reference/steam/{2026-08-28, android-2.0.60, windows-2.0.60, windows-early2026}`
- 工具：`tools/callgraph_fingerprint.py`、61 号的 GPU 实现（`git log --grep=gpu-similarity`）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/fingerprint-ablation`

## 4. 可以改的文件

- `tools/`、`fixtures/`（新夹具 + 配置表）
- `docs/02-module-map.md` §35（口径与新数）、`docs/159`（标已按建议重做）
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 配置组数、墙钟（GPU vs CPU 16 进程）、显存峰值
② 78.18% 的 bootstrap 95% 置信区间
③ 特征重要性排序（留一法 Δtop-1，按影响排）
④ 取值范围审计：16 个特征逐个给"是否漏值"的结论（漏了多少条）
⑤ 新夹具 + 旧→新映射（多少行变了）
⑥ cargo test 1671；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤10 行）

分支+sha、配置组数/墙钟/显存、置信区间、特征重要性前三、取值范围审计命中几处、映射行数、`cargo` 结果。
