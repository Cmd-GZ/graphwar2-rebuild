# 86 · `agent/last-tier`：档位**最后一格** + `LocalPlayer` 门⑤的**命中分支**

**优先级 P2（收尾活，两件都很小）。**

## 0. 两件遗留

```
① 档位 **19 wired / 2 layout / 1 assembled / 0 stub = 22** —— 还剩 **1 格** `assembled`
   报告 §9 明写："未查是哪一格、缺什么"
② 门⑤的**命中分支没打到**：79 号给了结构性原因 + 正对照
   ——需要「**本连接第一个玩家是 Bot**」的局（纯 Bot 客户端 / 连接重建）
```

## 1. 要做什么

1. **查清最后一格**：
   ```
   · 用 `Wired::status()` 逐槽数（别抄表），点出是哪一格
   · 它现在有什么（类型/尺寸 / new / 订阅 / 发布，能给的都给）
   · 缺什么才能到 `wired`：字段布局？事件流？还是卡禁区（`src/game.rs` / `client/src/**`）？
   → 结论写进 `docs/161` 对应行，并更新 §0 的档位分布说明
   ```
2. **门⑤命中分支**（若环境允许）：
   ```
   · 造"本连接第一个玩家是 Bot"的局（69 号/80 号的起局配方现成）
   · 把门⑤的取值为真的那一次打出来：命中时做了什么、发了哪条请求
   · 若造不出来：写清"卡在哪 + 缺什么"，并把正对照贴出来（证明观测链路本身是好的）
   ```
3. 两件都要**回写文档**：`docs/161`（档位）、`docs/06`（门⑤观测，若打到）。

## 2. 硬约束

- 数字不许退：`cargo test --offline` **1753**、三轴 `647 / 647 / 637`、
  headless 1890（14 个 SceneTree 脚本）、`selftest` 714（降级见 `docs/18` §10.3.3）；
- **GSM 别碰**（卡 `src/game.rs` 禁区）——若最后一格就是它，如实写"卡禁区"，**不硬来**；
- 样本/日志放 `.agent-work/`，**不进冻结快照**；
- headless 只跑 `extends SceneTree` 的脚本；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/last-tier`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- Lang：`<WS>/.reference/game/` 三件套 + frida 17.26.1；起局见 69 号配方
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/last-tier`

## 4. 可以改的文件

- `src/_scaffold/root_wiring.rs`（档位）、`client/tools/assemble_check.gd`
- `docs/161`、`docs/06`（追加观测节）
- ❌ 不许改 `src/game.rs`、`client/src/**`、`dispatch/`

## 5. 验收

```text
① 最后一格是哪一格 + 缺什么 + 能不能做（可做/卡禁区）
② 门⑤：打到没打到；若打到，贴命中时的行为；若没打到，卡在哪 + 正对照
③ 档位分布变化（19/2/1/0 → ?）
④ cargo/三轴/headless/selftest 是否退
```

## 6. 汇报（≤8 行）

分支+sha、最后一格是谁 + 缺什么、门⑤结论、档位是否变、三轴是否退。
