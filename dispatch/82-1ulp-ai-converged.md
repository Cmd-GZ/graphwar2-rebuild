# 82 · `agent/1ulp-ai-converged`：1 ulp 的**最后一条路** —— AI-vs-AI 的**收敛解**那一发

**优先级 P2。它很可能是死路（前面三条都没复现），但这是唯一没走完的。**

## 0. 为什么只剩这条

```
66 号  常数 token / 数组被改        → 否（21/21 0 ulp；20/20 未被改）
69 号  AI 开的那一发（人 + 1 Bot）  → 否（直接量：喂进 0x3c5f70 的 &Point 就是 start_x，3/3 0 ulp）
56 号  打印-回代链                  → 否（DiffEq 局没有常数；184/184 相同）
⇒ 语料里的分叉批是 **AI-vs-AI**，而 69 号只抓到 1 发、且是"人+1 Bot"
⇒ 差别可能在"**对局双方都是 AI**"时的那条路径（69 号建议：去 `evaluate` 钩子 或 压 `calcKeep`）
```

## 1. 要做什么

1. **造 AI-vs-AI 局**：两队各一个 Bot（难度档 3/5 都行），**人类 0 士兵**；
   若游戏不允许，就用"人 0 士兵 + 1 Bot"能造出的最接近形态，并**写明差别**；
2. **让 AI 的搜索"收敛"**（这是关键）：
   ```
   · 去/禁 evaluate 钩子，或把 `calcKeep` 压低 ⇒ 让它把候选解跑到收敛，而不是早停
   · 目标是拿到"**收敛解**"那一发，因为语料分叉批可能就是早停/收敛的差别
   ```
3. **同一次开火抓三样**（沿用 66/69 的探针）：① `0x3c2530` 入口 token 的 16 字节；
   ② `0x472865` 的 `(rdx,rcx,n)` + 16 字节；③ `EffectInfo` 的 JSON；外加
   **直接量喂进 `0x3c5f70` 的 `&Point`**（69 号的判决手段，最硬）；
4. **判决表**（照抄 69 的格式）：
   ```
   常数 token vs 印字 3 位小数 → 期望 0 ulp；若不是，找到了
   &Point vs start_x           → 期望逐位相同；若差 1 ulp，找到了
   末点与回放逐位比            → 差在哪一段（采样/命中/报文）
   ```

## 2. 允许的结论

**"仍然没复现"是一个合格结论**，只要写清：抓了几发、每发的边界距离（`c` 离 .xxx5 多远）、
覆盖面缺什么。**它比硬凑一个解释有价值。**

## 3. 硬约束

- 样本放 `.agent-work/replay-*`，**不进冻结快照**；
- 数字不许退：`cargo test --offline` 1710、三轴 647/647/637；
- 不许改 `src/game.rs` 的结算逻辑去"凑"样本；
- 提交信息用 `-F 文件`。

## 4. 环境

```
<WS>/.reference/game/ 三件套（sha 9a538115…）｜ frida 17.26.1（<WS>/.venv）
LAN+Bot 起局配方：见 69 号（一条命令起局 + attach；Xvfb 的坑见 docs/06 §35.1）
探针：tools/frida_point_array.*（66/69 已扩到抓 EffectInfo 载荷/JSON 与 &Point）
```

- 分支 `agent/1ulp-ai-converged`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/1ulp-ai-converged`

## 5. 可以改的文件

- `tools/`（探针）、`docs/06-verification.md`（追加观测节）
- ❌ 不许改 `src/`、`client/`、`fixtures/`、`dispatch/`

## 6. 验收

```text
① 局怎么造的（AI-vs-AI 还是人+Bot；差别写清）
② 抓到几发 AI 开火；每发的 `c` 离 .xxx5 的距离
③ 三样 + `&Point` 的判决表
④ 结论：复现了 / 仍没复现（+ 覆盖面缺什么）
⑤ 样本位置（确认没进冻结快照）；cargo/三轴是否退
```

## 7. 汇报（≤8 行）

分支+sha、局型、抓到几发、`&Point` 与 start_x 的比较结果、末点差在哪、结论、三轴是否退。
