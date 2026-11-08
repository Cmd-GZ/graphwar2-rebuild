# 55 · `agent/overlap-scenario`：造出「**结算与开火重叠**」的对局

**优先级 P0。第十一轮报告 §10 第 2 条 —— 48 号说这是唯一还能解释那 100 发的场景。**

## 0. 为什么只剩这一条

```
48 号（10 发）: `end_x == pos_x` **10/10**、`prefix_diff` 恒 0 ⇒ 没抓到分叉
但两装配点**真的不同**（prefix_diff 1519/2309/2309 点）；报文**永远取 A**（10/10）
⇒ §28.4 的"end_x 站 A、pos 站 B"机制**被否掉**
⇒ 剩下唯一能解释"同一下标读到不同值"的：**两个装配点之间有东西改写了数组**
   （例如下一发已经开始结算、或上一发还没读就被覆盖）
```

## 1. 要做什么

**故意制造"多发生命周期重叠"**：

1. 让两发**几乎同时**飞（同时回合模式，两个玩家/Bot 同一帧开火）；
2. 或在**结算仍在进行**时立刻开下一发（连发/快速点击）；
3. 在两处装配点（`0x4569b8` 写 `end_x` / `0x472865` 读 `pos`）**同时下断**，
   记录 `(ptr, len, 下标, 值 bits, 时间戳)`；
4. 目标：抓到**至少一次** `end_x ≠ pos_x`，并给出"两次之间谁写了那块内存"。

**若造不出重叠**（例如同一局强制串行）：如实写"造不出 + 缺什么"，并给出
"为什么原版这条路径天然串行"的证据（那也是有价值的负结论）。

## 2. 纪律

- **两处必须同时抓**；
- 样本入库（文本 JSON，去路径用户名）；
- 抓不到就如实报。

## 3. 环境（与 34/40/44/48 同一套）

```
<WS>/.reference/game/  三件套（sha 9a538115…）
frida 17.26.1          <WS>/.venv
Xvfb（TCP 监听）        docs/06 §35.1 的三个坑
同时回合               `TurnMode=1`（50 号解出：`TurnData[+0x58]`）
```

- 分支 `agent/overlap-scenario`
- **私有** `CARGO_TARGET_DIR`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/overlap-scenario`

## 4. 可以改的文件

- `tools/frida_point_array.*`、`examples/`、`fixtures/`（样本）
- `docs/06-verification.md` 追加
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 怎么造的重叠（步骤/模式/时序）
② 抓到几次 end_x ≠ pos_x（几次、几发）
③ 那两次之间的写入者（地址 + 值）
④ 若造不出：为什么天然串行（证据）
⑤ cargo test 1659；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、怎么造的、抓到几次分叉、写入者地址、或"造不出+原因"、`cargo` 结果。
