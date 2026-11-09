# 92 · `agent/gate5-real-lobby`：门⑤ 的**真房源顺序**（真服务器抓一次"加 Bot"）

**优先级 P2，运行时活；86 号已经把"命中分支可达"证明了，本轮只差"真实顺序"这一块。**

## 0. 现状

```
86 号：自建协议服务器控帧序，4 局 2 命中 ⇒ **命中分支可达**（正面证据）
但**真服务器**在"加 Bot"时到底先发 `BotAdded` 还是先发 `PlayerInfo`、连接归属怎么算 —— **未观测**
⇒ 门⑤在**真实对局**里命中的**条件**还不清楚
```

## 1. 要做什么

1. 按 86 号的路子起局（真机 + 我们的协议服务器 / 或真 LAN 加 Bot），**抓一次"加 Bot"的报文顺序**：
   ```
   · `BotAdded` 与 `PlayerInfo` 的先后（逐条时间戳）
   · 该玩家的 `player_id` 与连接归属（是不是"本连接第一个玩家"）
   ```
2. 对照门⑤的判据（86 号已给：`0x519fb1` → 表查询），回答：
   ```
   真实对局里，什么条件下这道门会为真？为真之后行为是什么（少发哪几条请求）？
   ```
3. 若真实对局**打不到**（例如游戏本身不允许），写清"卡在哪 + 缺什么 + 86 号的自建路径已经证明了什么"。

## 2. 硬约束

- 样本/日志放 `.agent-work/`，**不进冻结快照**；
- 数字不许退：`cargo test --offline` **1753**、三轴 `647 / 647 / 637`、headless **1923**、`selftest` **716**；
- 抓不到就说抓不到，**不许用静态推测代替运行时结论**；
- 提交信息用 `-F 文件`。

## 3. 环境

```
<WS>/.reference/game/ 三件套 ｜ frida 17.26.1 ｜ 86 号的 tools/gate5_serve.py（自建协议服务器）
起局配方见 69/80 号（Xvfb + Popen+attach）
```

- 分支 `agent/gate5-real-lobby`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/gate5-real-lobby`

## 4. 验收

```text
① 加 Bot 时的报文顺序（逐条，带时间戳）
② 该玩家的 player_id / 连接归属
③ 门⑤在真实对局里为真的条件（结论 + 依据）
④ 若打不到：卡在哪 + 缺什么
⑤ 三轴/headless/selftest 是否退
```

## 5. 汇报（≤8 行）

分支+sha、报文顺序、连接归属、门⑤为真的条件、打不到的原因（若有）、三轴是否退。
