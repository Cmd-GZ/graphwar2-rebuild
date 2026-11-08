# 73 · `agent/conv-follow-s3`：收敛层跟上 **S3**（批内按到达序逐发挖洞）

**优先级 P2，但**拖久了会口径分叉** —— 报告 §9-5 点名。**

## 0. 现状

```
65 号落地了 S3（`fire_functions` 批内按到达序逐发挖洞），三轴一位没退、净 2 批变好
但**收敛层 `diffconv` 没跟** ⇒ 两层口径分叉（报告 §7 明确登记"未动"）
```

## 1. 要做什么

1. 把 S3 的批内顺序语义同步到收敛层（`src/_scaffold/diffconv.rs` 那套 `ConvPredictor`）；
2. **三轴分开报**（这是硬要求）：
   ```
   fire ?/1075 ｜ batch ?/1075 ｜ **batch_conv ?/1075**
   ```
   注意 `batch_conv` 必须用 `--example batch_conv --policy strict` 跑，**不许拿 fire/batch 的数代替**；
3. 若某条轴下降 ⇒ 说明两层不能简单同构，**给分类开关 + 说明为什么**。

## 2. 硬约束

- 65 号的改动是**已验证**的（三轴未退），本任务**不许回退它**；
- 数字不许退：`cargo test --offline` 1693、headless 全脚本 0 失败；
- 交界处要有注释说明"两层为什么同构/不同构"；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/conv-follow-s3`
- 快照 401 份（**冻结，别加文件**）
- 三轴命令见 `AGENTS.md`「合并纪律 B」
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/conv-follow-s3`

## 4. 可以改的文件

- `src/_scaffold/diffconv.rs`、`src/game.rs`（只动批内顺序那一段）
- `docs/06-verification.md` 追加
- ❌ 不许改 `client/`、`dispatch/`

## 5. 验收

```text
① 改了什么（一段话）
② **三轴各自的 before/after**（fire / batch / batch_conv）
③ 若不同构：分类开关 + 理由
④ cargo test 1693；headless 0 失败
```

## 6. 汇报（≤8 行）

分支+sha、改法、三轴 before/after、是否同构及理由、`cargo` 结果。
