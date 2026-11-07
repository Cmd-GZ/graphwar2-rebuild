# HANDOVER（**CG 分支独立版**）

> 这份是 **CG 分支**的交接文档，与 `master` 上的 `docs/HANDOVER.md` **相互独立**。
> `master` 那份讲"复原到什么程度"；这份讲"**派发这一侧怎么运作**"。
> 两者故意不同——`CG` 与 `master` 的全部差异只有两处：**`dispatch/` 与本文**。

## 1. CG 是什么

```
还原进度   **527 / 527 可归属函数 = 100%**（逐位 88 / 观测 439 / 推断 0）｜ 未做 0、未知模块 0
协议       **74 / 74 全字段 + 74 / 74 判别值 + 7 个线上枚举全集**（已无未知）
GDExtension 类   12 / 12 全部还原
真名       327 个函数有原版方法名 + x86 地址（fixtures/name_to_addr.tsv）
cargo      **1520 passed / 0 failed**（lib 464；`--features gdext-bindings` **1595**，lib 539）
客户端自检   **694 passed / 0 failed**（前置：`client/assets/` + `examples/serve_match`；
           缺资产是 591 / 2 / 23 —— 两个口径见 README「客户端自检的两个口径」）
```

**CG 上不写产品代码。** 所有代码改动都来自 `agent/*` 分支，经合并进 `master` 后，
再把 `master` 并回 `CG`（只带来源码，不带 `dispatch/`）。

## 2. 现在这两份清单从哪看

| 想知道 | 看哪 |
|---|---|
| 复原到什么程度、还剩什么 | **`master` 的 `docs/HANDOVER.md`** |
| 对拍的准确度（三条轴的数） | `master` 的 `docs/HANDOVER.md` §0.0.0.2 |
| 合并前/后必须做什么 | `master` 的 `AGENTS.md` §「合并纪律」 |
| 下一轮派什么活 | **本分支 `dispatch/README.md`** 的"第三轮"一节 |
| 上一轮下游报了什么 | `dispatch/reports/CG-2026-11-07.md`（第一轮）、`CG-round2-2026-11-07.md`（第二轮） |

## 3. 派发的硬规矩（写死，别再犯）

1. **分支前缀必须是 `agent/`**，不能是 `CG/` —— 远端已有 `CG`，git 不允许 `CG/xxx` 子引用。
2. **数字必须带轴名**：`fire` / `batch` / `batch_conv` 三条驱动轴，同一指标完全不同。
   报数字不写轴 = 没报；不给可复跑命令 = 当未验证。
3. **验收必须能从干净树重跑**：要写清"重建 `.so` + `--import`"这两步，
   否则会拿旧产物验出假通过（真发生过）。
4. **并行任务各用私有 `CARGO_TARGET_DIR`**；顺序做才可以共享（省磁盘，但共享会串味）。
5. **任务书里标"已知"的一律注证据档**；没标的默认当"待验证"——
   第一轮有 4 条、第二轮有 5 条前提被下游证伪。

## 4. 每轮收尾动作（照做）

```text
① 每个任务一个 agent/ 分支，各自推送
② 合并进 master 前：范围核对 + 复现报告数字 + 读关键 diff + 负结论正对照 + 污染检查
③ 合并后：四件套（cargo×2 / coverage+docrefs / 三条轴数字 / headless 脚本）
④ 把 master 并回 CG（保持"只差 dispatch + 本文"这条不变量）
⑤ 写本轮验收报告到 dispatch/reports/，把被证伪的前提单独列一节
```

## 5. 与 master 的同步

```bash
# 在 CG 上
git fetch origin
git merge origin/master            # 期望：只可能冲突 dispatch/ 与本文
git checkout HEAD -- dispatch/     # 保住任务书（master 里没有这目录）
# docs/HANDOVER.md 冲突时保留本文件（CG 版）
git commit && git push origin CG
```

合并后自检（**这条不变量必须成立**）：

```bash
git diff --name-only master..CG
# 期望只出现：dispatch/... 与 docs/HANDOVER.md
```

## 6. 当前状态（2026-11-08 凌晨）

```
master @ 已并入本次更新；cargo 1520 passed / 0 failed
任务书：01–08（第一轮，已完成并合并）
        09–14（第二轮，已完成并合并；11 是负结果）
        15–19（第三轮，待派发）
下游：第二轮报告在 dispatch/reports/CG-round2-2026-11-07.md
```
