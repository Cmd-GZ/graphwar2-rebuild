# 14 · `agent/hole-axis`：挖洞点轴定性（200 例"点位不存在"）

**优先级 P2。这是机制问题，不是数值问题。**

## 现象（下游实测，`dispatch/reports/CG-2026-11-07.md` §7）

```text
延迟段 271 批里，**200 例**我方算出的挖洞点，在该回放的任何 `DestroyObstacle` 报文里都不存在。
```

⇒ 不是"差一点"，是**我们认为该挖的洞，回放里根本没挖**。

## 要做什么（**先定性，再谈修**）

1. 把 200 例分类：

```text
a) 我方多挖了（回放没挖，我方挖了）
b) 我方挖的位置对但时间不对
c) 我方把"不该挖的障碍"当成可挖
d) 其它
```

2. 每类挑 3 个样本，列出：回放的弹道停点、我方停点、回放 `DestroyObstacle` 的实际坐标与 tick；
3. **回到二进制**：`QueueDestroyObstacle` / `DestroyObstacle` 的入队条件
   （`docs/14` §11 有 `ObstacleManager::queue_destroy_obstacle` 的调用位置；`DelayDestroy` = **165 tick**）；
4. 若能定出条件，落地修正；定不出就**列出缺什么证据**。

## 允许改的文件

- `src/game.rs`（结算里挖洞那段）**或** `src/plane.rs`（掩膜/挖洞）
- `tests/`、`examples/`
- `docs/` 对应主题追加（`docs/14` 或 `docs/08-plane.md`）

⚠️ 不许碰 `*_class.rs`、`client/`。

## 验收

```text
200 例的分类计数（四类加起来 == 200）
每类 3 个样本的具体数字（坐标 + tick）
延迟段挖洞点命中：before 1 / 259 → after 必须给出数字
cargo test --offline 全绿（1512）；coverage --check OK
```

## 汇报（≤8 行）

分支+sha、**四类各多少例**、主因是哪一类（一句话）、
二进制里的入队条件（若有）、before/after 命中数、缺什么证据。
