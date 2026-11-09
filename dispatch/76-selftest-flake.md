# 76 · `agent/selftest-flake`：修 `selftest` 的**不确定性**（它现在不是可靠护栏）

**优先级 P0。上游实测：同一条命令反复跑，会出 714/0 → 704/4 → 697/1。**

## 0. 现象（我这边抓到的原文）

```
跑两次都"通过"的间隙里出现过：
  SELFTEST 704 passed, 4 failed, 1 skipped
  SELFTEST 697 passed, 1 failed, 1 skipped
  FAIL 真服务端：开局那一批到了（障碍 0、SoldierInfo 4）
干净重跑又回到 714 / 0 / 1 skipped
```

⇒ 不是回归，是**时序/端口 flake**。这很严重：**`selftest 714/0` 是本项目的护栏之一，
护栏不确定就等于没有护栏。**

## 1. 要做什么

1. **先把 flake 稳定复现**：写一个循环跑 `bash client/run.sh selftest` N=10 次，
   统计通过/失败、失败项名字、以及失败时 `serve_match` 的端口与启动日志；
2. **定位根因**（以下都是嫌疑，逐个排除）：
   ```
   · 端口竞争：serve_match 起在随机端口，客户端 connect 太早 ⇒ 失败
   · 上一个用例的 serve_match 还没退出 ⇒ 下一个用例连到旧服务端
   · 等待用的是固定 sleep 而不是"等到握手完成"
   · 用例之间有共享状态（重放目录、local_ip.config、user:// 文件）
   ```
3. **修成确定性的**：优先"等握手/等就绪信号 + 唯一端口 + 显式 teardown"，**不要加大 sleep**；
4. **验收**：连续跑 **10 次全绿**（贴 10 行输出），并且失败时能给出**可诊断的输出**
   （端口、serve_match 的 stderr、哪一步超时）。

## 2. 硬约束

- **不许把失败改成跳过**（那是把护栏拆了）；
- 不许改断言判据；只许改"等什么、什么时候起、什么时候退"；
- 数字不许退：`cargo test --offline` **1710**、三轴 `fire 647 / batch 647`、
  headless 14 个 SceneTree 脚本 **1827 条 / 0 failed**；
- headless 只跑 `extends SceneTree` 的脚本（`*_check.gd`），别拿 helper 喂 `--script`；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/selftest-flake`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 相关：`client/tools/selftest.gd`、`client/run.sh`、`examples/serve_match.rs`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/selftest-flake`

## 4. 可以改的文件

- `client/tools/selftest.gd`、`client/run.sh`、`examples/serve_match.rs`（只加不改判据）
- `docs/18-build.md`（把"自检两个口径"补一条"确定性"说明）
- ❌ 不许改 `src/`、`client/src/**`、`dispatch/`

## 5. 验收

```text
① flake 复现率（10 次里几次失败、失败项）
② 根因（一段话 + 证据：端口/日志/时序）
③ 怎么修的（等什么、怎么 teardown）
④ **连续 10 次全绿**的输出
⑤ cargo test 1710；三轴与 headless 有没有退
```

## 6. 汇报（≤8 行）

分支+sha、复现率、根因、修法、10 次连跑结果、`cargo`/三轴结果。
