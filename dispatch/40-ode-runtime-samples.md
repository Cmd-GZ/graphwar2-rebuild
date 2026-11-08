# 40 · `agent/ode-runtime-samples`：补 **DiffEq/Ode 局**的运行时样本（一发都没覆盖）

**优先级 P0。第八轮报告 §8 第 2 条。** 残差里 **219 批是 DiffEq**，而 34 号那 37 发**一发 Ode 都没有**。

## 0. 为什么这是空白

```
34 号（运行时抓 (ptr,n)）      37 发，全是 Normal 局
35 号（折叠复算）            407 批里 DiffEq 219 批 / 243 发 —— 但**全是报文侧**，没有运行时观测
⇒ "DiffEq 的 pos 为什么不是最后一个积分点" 这件事，**一次真机观测都没有**
```

第八轮 §7 已把它列为未定性：

> DiffEq 那 90 批"其它" —— 回放 `pos_x` 不在我方积分网格上 ⇒
> **"回放点不对" vs "我方积分先跑偏" 不可分**（34 号 37 发里一发 Ode 都没有）

## 1. 要做什么

1. **在真游戏里进一局 DiffEq（Ode 模式）并开炮** —— 关键是**怎么造出这个模式**：
   - 查 `FunctionModeInfo`(46) / `AxisModeInfo`(45) 这些事件怎么把一局设成 `DiffEqFunction`；
   - 可行路径：本地起服务端（`examples/serve_match`）或在 campaign 里找一关（`docs/13` 的模式表）；
   - **若造不出这个模式，如实报"卡在哪"**，这本身也是有价值的信息。
2. 抓到与 34 号**同一套**数据：点数组 `(ptr, n)`、`pos/end_x` 读数、两次 `calculate_function` 的返回；
3. **回答那个二分问题**：
   ```
   DiffEq 局里，报文读的那个点 = 点数组最后一点吗？
   ⇒ 是 ⇒ "回放点不对"（与我方无关）
   ⇒ 否 ⇒ "我方积分先跑偏"（我方要修）
   ```
4. 至少 3 发样本入库。

## 2. 环境（与 34 号同一套，照抄）

```
<WS>/.reference/game/  三件套（sha 9a538115…）
frida 17.26.1          <WS>/.venv
Xvfb（TCP 监听）        docs/06 §35.1 的三个坑
脚本                  tools/frida_point_array.{js,py}（已支持 --extra-shot 等）
```

- 克隆到 `<WS>/.agent-work/odesamples/repo`，分支 `agent/ode-runtime`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/odesamples/target`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/ode-runtime`

## 3. 纪律

- **怎么造出 Ode 局要写清**（命令/存档/步骤），否则别人复现不了；
- 样本入库（文本 JSON，去路径用户名）；
- **抓不到不要编**：写清卡在哪一步 + 缺什么。

## 4. 可以改的文件

- `tools/frida_point_array.*`、`examples/`（只加不改）
- `fixtures/`（新增样本）
- `docs/06-verification.md`（追加一节，标 `观测`） / `docs/13`（若模式怎么造清楚了）

❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① Ode 局是怎么造出来的（步骤/命令）
② 抓到几发；每发的 (ptr,n)、pos、end_x
③ 那个二分问题的答案（回放点不对 / 我方积分跑偏）
④ 219 批里有几批能被这次观测解释
⑤ cargo test --offline 1628；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、Ode 局怎么造的、抓到几发、二分问题答案、
对 219 批的解释力、样本文件、`cargo` 结果。