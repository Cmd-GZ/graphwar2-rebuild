# 48 · `agent/array-divergence`：抓"两装配点数组**真的分叉**"的那一刻 + 定位改 `Vec.len` 的点

**优先级 P0。第十轮报告 §8 第 2、3 条，两个问题同一片区域，一起做。**

## 0. 现状（44/45 号已把范围缩小）

```
44 号：end_x 真身 = 0x4569b8（pts[len-1].x）；12 发双抓证明 end_x 与 pos_x
      **同下标、不同对象、不同时刻** —— 但**没抓到分叉本身**
45 号：某处把点数组的 Vec.len 从 **5098 改成 1528**，只证明了区间
      （**采样之后、0x472865 之前**），**没定位到那个写入者**
```

## 1. 要做什么

1. **抓分叉那一刻**：在 `0x4569b8`（end_x 写点）与 `0x472865`（pos_x 读点）**两处同时下断**，
   抓 `(ptr, len, 下标, 值 bits)` 四元组；要求抓到**至少一次** `end_x ≠ pos_x` 的真实分叉，
   并给出"两次之间数组被谁改过"的证据（或证明是两次读同一数组但 `len` 变了）；
2. **定位改 `len` 的写入者**：在 `Vec.len` 被改的瞬间断（watchpoint / 在 45 号给的区间内扫 `mov [x+0x10], r??`）；
   给出**写入者地址 + 当时的新旧值 + 触发条件**；
3. 回答：这个截断是**原版故意**（例如"只广播前 N 个点"）还是**副作用**（例如 `truncate` 复用缓冲）。

## 2. 纪律

- **两处必须同时抓**（44 号已证明只抓一处分不出来）；
- 样本入库（文本 JSON，去路径用户名）；
- 抓不到就如实报"卡在哪 + 缺什么"。

## 3. 环境（与 34/40/44/45 同一套）

```
<WS>/.reference/game/  三件套（sha 9a538115…）
frida 17.26.1          <WS>/.venv
Xvfb（TCP 监听）        docs/06 §35.1 的三个坑
```

- 克隆到 `<WS>/.agent-work/arrdiv/repo`，分支 `agent/array-divergence`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/arrdiv/target`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/array-divergence`

## 4. 可以改的文件

- `tools/frida_point_array.*`（扩展）、`examples/`（只加不改）、`fixtures/`（样本）
- `docs/06-verification.md` 追加一节（标 `观测`）
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 抓到几次分叉（几次、几发）
② 分叉时刻的四元组（end_x 侧 vs pos_x 侧）
③ 改 len 的写入者地址 + 新旧值 + 触发条件
④ 是"故意截断"还是"副作用"（结论 + 证据）
⑤ cargo test 1638；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、抓到几次分叉、四元组关键两行、改 len 的地址、故意/副作用结论、样本文件、`cargo` 结果。
