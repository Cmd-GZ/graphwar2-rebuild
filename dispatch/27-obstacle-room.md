# 27 · `agent/obstacle-room`：造一个**带障碍的对局**，收「同坐标不同序号」样本

**优先级 P1。** 第四轮报告 §8 第 2 条；这条卡着一个**未知**：
"一次挖洞发 2 条、坐标相同但序号不同"——**至今没有样本**。

## 现状

```
任务 14/17 已经定性：挖洞点 = 爆炸点，生产者已定位
但"同坐标不同序号"需要**实际出现挖洞的对局**才能观测
现有 401 份回放里没有可用样本
```

## 要做什么（两条路，任选其一或都试）

1. **制造对局**：起一个本地服务端（`examples/serve_match` 或我们自己的 broker 实现），
   加机器人 / 摆障碍，打进一炮让弹道**撞到障碍**，触发 `QueueDestroyObstacle` + `DestroyObstacle`；
   把报文序列抓下来（已入库的抓包工具见 `docs/16` 的 net 部分 / `examples/`）。
2. **找现成存档**：查 `Graphwar.pck` 里有没有带障碍的 campaign 存档 / replay；
   或看客户端里"障碍"在哪些模式出现（`docs/13` 的模式表）。

**拿到样本后**要回答：

- 两条 `DestroyObstacle` 的**坐标是否真的逐位相同**；
- "序号"是什么字段（`entity_id`？`effect_id`？障碍 id？），**为什么同点两号**；
- 若序号是**服务端分配**的，说明谁分配、分配规则是什么（计数器？哈希桶？）。

## 已知（别重推）

- `DelayDestroy = 165 tick` 是抓包经验值（`docs/14` §12）；
- 挖洞点 = 爆炸点（第四轮 14 号定性，200 例全归 "d 参照点错"）；
- `QueueDestroyObstacle` = tag 42、`DestroyObstacle` = tag 41（第四轮 17 号查清，**是两个 tag**）；
- 三轴护栏见 `dispatch/README.md`（不许退）。

## 环境

- 克隆到 `<WS>/.agent-work/obsroom/repo`，分支 `agent/obstacle-room`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/obsroom/target`
- 游戏三件套在 `<WS>/.reference/game/`（`Graphwar.x86_64` / `Graphwar.pck` / `libgraphwarrust.so`，sha `9a538115…` = 目标）
- 运行时参考：`docs/06` §35.1（**Xvfb 三个坑**：`/tmp` 逐命令 tmpfs ⇒ 用 TCP 监听；EGL 必须指 mesa；Vulkan 不可用）
- frida 脚本可复用：`tools/frida_point_array.{js,py}`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/obstacle-room`

## 可以改的文件

- `examples/`（只加不改）、`tools/`、`fixtures/`（样本入库，**只文本**）
- `docs/14-turn.md` / `docs/06-verification.md` 追加一节（标 `观测` 或 `逐位`）

❌ 不许改 `src/` 的业务逻辑（若必须改，先写清证据）、`client/src/**`、`dispatch/`

## 验收

```text
① 样本怎么来的（服务端 + 机器人 / 存档 / 真机操作），给命令
② 是否拿到"同坐标不同序号"的样本（条数）
③ 序号字段是什么、为什么同点两号（有解释或明确"缺什么"）
④ 样本入库（jsonl 片段，去路径用户名）
⑤ 拿不到样本也算合格：写明卡在哪 + 缺什么
cargo test --offline 全绿（基线 1569）
```

## 汇报（≤8 行）

分支+sha、样本来源（命令）、拿到几条样本、序号字段结论、入库文件、
拿不到的话卡在哪、`cargo` 结果。