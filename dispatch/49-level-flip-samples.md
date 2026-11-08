# 49 · `agent/level-flip-samples`：补 **Level 18–20** 与 **`flip=1`** 的样本

**优先级 P1。第十轮报告 §8 第 4 条 —— 现有样本的空白。**

## 0. 空白在哪

```
现有样本：Normal / Ode 都有、单士兵为主
空白：Level 18/19/20（多士兵 / Bot）、以及 **flip=1**（翻转坐标）
```

`flip` 是"从左往右打"还是"从右往左打"的镜像开关；我们的实现里它影响
`c = y0 - f(∓x0)` 的取号与采样方向 —— **真机上一发 flip=1 的样本都没有**。

## 1. 要做什么

1. **造 flip=1 的对局**：右侧士兵往左打（campaign 里找，或用 `serve_match` 摆位）；
2. **进 Level 18–20**：多士兵 / Bot，抓完整的点数组与报文；
3. 每类至少 **3 发**样本，入库（文本 JSON）；
4. 回答：
   ```
   ① flip=1 时修正量的取号与我们一致吗（逐位）
   ② flip=1 时的采样方向（x 递增还是递减）与我们一致吗
   ③ 多士兵局里 pos 的归属（谁的位置写进报文）
   ```

## 2. 纪律

- 怎么造出 flip=1 / 进哪一关，**要写清**（否则别人复现不了）；
- 拿不到就如实写"未取得 + 为什么"；
- 样本入库（去路径用户名）。

## 3. 环境（与 40/45 号同一套）

```
<WS>/.reference/game/  三件套（sha 9a538115…）
frida 17.26.1          <WS>/.venv
Xvfb（TCP 监听）        docs/06 §35.1 的三个坑
```

- 克隆到 `<WS>/.agent-work/flips/repo`，分支 `agent/level-flip`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/flips/target`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/level-flip`

## 4. 可以改的文件

- `tools/frida_point_array.*`、`examples/`（只加不改）、`fixtures/`（样本）
- `docs/06-verification.md` / `docs/13`（追加）
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① flip=1 怎么造的（步骤/关卡）；抓到几发
② Level 18–20 各抓到几发
③ 三个问题的答案（逐位/观测）
④ 若与我们的实现不一致：差在哪、要不要修
⑤ cargo test 1638；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、flip=1 怎么造、各抓到几发、三个问题答案、是否要改实现、样本文件、`cargo` 结果。
