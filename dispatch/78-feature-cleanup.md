# 78 · `agent/feature-cleanup`：按 71 号的留一法**删掉负收益特征**，把特征表变小变准

**优先级 P1。71 号给了数，但没人动手清理。**

## 0. 依据（71 号实测，别重推）

```
REF（88.15%）逐个拿掉，留出半 Δ：
  gram   −16.63 点（命脉）｜ scal −2.49 点
  size    0.0 ｜ consts 0.0        ← 纯冗余
24 个特征里 **7 个拿掉后 top-1 反而涨**：
  ncall +7 ｜ caller_a +6 ｜ caller +5 ｜ icall +3 ｜ 其余 3 个小幅
⇒ **"加特征"平均负收益**；现在这张表该瘦身了
```

## 1. 要做什么

1. **把"负收益/零收益"的特征从默认特征集里摘掉**（保留代码路径，加开关，便于复现旧数）；
2. **重出夹具**，并给三张表：
   ```
   ① 每个被删特征：单独拿掉后的 Δ（引 71 号的数）+ 一起拿掉后的 Δ
   ② 删后 vs 删前：留出半 top-1 / top-5 / 95% CI（必须**不变或更好**）
   ③ 特征表规模：从 N 列降到 M 列（显存/墙钟顺带报）
   ```
3. **写清"为什么删"**：不是"为了简洁"，是"负边际"——文档里要能一句话说清；
4. **守住两条纪律**：
   ```
   · 被删的特征**代码不删**（能一键复原），只从默认集里摘（否则旧结论无法复现）
   · 每删一个都要报"两边覆盖率"，防止删掉的是"偏斜源"（strs 那类错误）
   ```

## 2. 硬约束

- 不许改 `gram` / `scal` 的编码（那是命脉，改了就不是清理了）；
- 留出半**不许降**（降了说明删错了，回退并报）；
- 数字不许退：`cargo test --offline` 1710、三轴 647/647/637；
- 提交信息用 `-F 文件`；大表入库按 `fixtures/` 规则（≤8 MB）。

## 3. 环境

- 分支 `agent/feature-cleanup`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- GPU 可用（3,000 组 216 s 已实测）；**报 GPU 必须给两个口径**（内部计算时间 / 进程墙钟）
- 工具：`tools/callgraph_fingerprint.py`（含 71 号的张量实现）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/feature-cleanup`

## 4. 可以改的文件

- `tools/callgraph_fingerprint.py`、`fixtures/`（新夹具 + 配置表）
- `docs/02-module-map.md` §35（补"特征表瘦身"小节）
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 删了哪些（逐条：单删 Δ、合删 Δ、两边覆盖率）
② 删后留出半 top-1/top-5/CI（与 76.4% / 88.15% 对照）
③ 特征表规模 N → M；显存/墙钟变化
④ 旧数还能一键复现（开关）
⑤ cargo test 1710；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、删了哪几个、删后准确率与 CI、表规模与显存变化、旧数能否复现、`cargo` 结果。
