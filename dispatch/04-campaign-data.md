# 04 · `CG/campaign-data`：21 关参数表落码

**优先级 P1。数据任务，不需要二进制。**

## 背景

21 关的参数**已经从二进制里解出来了**，权威表在 `docs/archive/118-campaign-data.md` §7，
并与 385 份真实回放交叉验证 **21/21 全对**。

但：`client/src/app/campaign.gd` 里还是 **PLACEHOLDER**，我们 `src/` 里也没有这份表。

## 数据形状（见 §7 的表）

每关 7 个参数：障碍模式 / `min_count` / 士兵数 / 目标数 / Bot 难度 / 回合上限 / 标题与描述。

另有存档语义（`docs/118` §3）：`campaign.save` = 单行 `level_<n>`、**无尾换行**、0 基、
语义 = 最后通关关卡、只增不减、启动时不创建。

## 要做什么

1. 在 `src/graphwar_campaign_control.rs` 里落 21 关的表（按原版 `level_info()` = `0x4ad380`
   的 21 分支 `match` 的形状写，每行注明来自 §7）；
2. 加一个可按关卡号查询的接口，并写单测：**21/21 关的 7 个参数逐项相等**；
3. 改 `client/src/app/campaign.gd`：把 PLACEHOLDER 换成从内核取（或照抄同一份表），
   保证客户端显示的关卡名/描述与 §7 一致（标题 21/21、描述 21/21 逐字节解出过）。

## 允许改的文件（**只许这些**）

- `src/graphwar_campaign_control.rs`
- `tests/`（新增或追加到既有 campaign 相关测试）
- `client/src/app/campaign.gd`
- **`docs/10-campaign.md`** 追加一节（索引表 + 与 §7 的一致性声明）

⚠️ **不许改**：`src/lib.rs`、其它 `src/*.rs`、`client/` 里别的文件、`docs/INDEX.md`。

## 验收

```text
cargo test --offline                     # 全绿；新增 21/21 参数断言
python3 tools/coverage_matrix.py --check  # OK（改了 src 要重跑生成器并提交两个生成物）
grep -c PLACEHOLDER client/src/app/campaign.gd   # == 0
```

## 汇报（≤8 行）

分支 + sha、表落在哪个函数、21/21 断言是否全过、
客户端 PLACEHOLDER 是否清空、`cargo` 条数。
