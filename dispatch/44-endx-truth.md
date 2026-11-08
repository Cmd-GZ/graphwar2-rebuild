# 44 · `agent/endx-truth`：`pos_x ≠ end_x` 的那 100 发，到底哪个字段不是"这次读"

**优先级 P0。第九轮报告 §8 第 2 条。** 目前 `end_x` 的**真身未知**。

## 0. 现状

```
报文里同时有 `FunctionPoints.end_x` 与 `EffectInfo.pos_x`
已证：两者读**同一对 (ptr,len)、同一个下标**（x86 0x472814–0x47286b）
但实测有 **100 发** `pos_x ≠ end_x`（差 <0.02 的 127 发 / 差 <1e-12 的 102 发，见 §25.8）
```

⇒ 既然"同一处读"，为什么会出现两个不同的值？三种可能：

```
(a) 两个字段是**两次读**（中间数组被改过）；
(b) 其中一个不是"这次读"，而是**早先缓存**的值；
(c) 报文里有**两处**装配点，我们只看到了一处。
```

## 1. 要做什么

1. **同时**抓两处（这是关键：单抓一处分不出来）：
   - `FunctionPoints` 的**装配处**（写 `end_x` 的地方）；
   - **`0x472865` 附近那次读**（写 `pos_x` 的地方）；
   - 在真游戏里打一炮，把两处的 `(ptr, len, 下标, 读到的 x bits)` 都记下来，看是不是同一次。
2. **用回放当裁判**：那 100 发里抽 ≥10 发，看回放的两个值分别等于我方点数组的哪一个元素
   （第 `len-1` 个？`len-2` 个？还是别的）。
3. **给结论**：(a)/(b)/(c) 是哪一种；若是 (c)，找出第二个装配点。
4. 若确认是"两次读且中间被改"，说明**原版自己在这条路径上有非确定性** —— 那要写清它改了什么。

## 2. 环境（与 34/40 号同一套）

```
<WS>/.reference/game/  三件套（sha 9a538115…）
frida 17.26.1          <WS>/.venv
Xvfb（TCP 监听）        docs/06 §35.1 的三个坑
脚本                  tools/frida_point_array.{js,py}
```

- 克隆到 `<WS>/.agent-work/endxtruth/repo`，分支 `agent/endx-truth`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/endxtruth/target`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/endx-truth`

## 3. 纪律

- **两处必须同时抓**（否则分不出来）——这是本任务的核心要求；
- 样本入库（文本 JSON，去路径用户名）；
- 抓不到就如实报"卡在哪 + 缺什么"。

## 4. 可以改的文件

- `tools/frida_point_array.*`（扩展）、`examples/`（只加不改）
- `fixtures/`（新增样本）
- `docs/06-verification.md` 追加一节（标 `观测`）

❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 两处抓到了吗（各几次）
② 两处的 (ptr,len,下标) 是否同一次读
③ 那 100 发里 ≥10 发的逐项对照（回放两值 vs 我方点数组第几个）
④ 结论：(a)/(b)/(c)，以及"原版是否有非确定性"
⑤ cargo test 1631；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、两处抓取结果、是否同一次读、10 发对照的关键两行、结论 (a)/(b)/(c)、样本文件、`cargo` 结果。