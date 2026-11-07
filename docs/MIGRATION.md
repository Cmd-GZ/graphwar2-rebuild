# MIGRATION — 文档重组映射表（“没丢东西”的凭据）

> 本轮把 `docs/` 从“按时间堆的变更日志”重组成“按主题组织的知识库”：
> **根目录从 122 篇降到 25 篇**（含本表），其余 **125 篇全部进 `docs/archive/`**，
> 原文一字未改。本表逐份说明它**去哪儿了、为什么**。
>
> ⚠️ **审计 2026-11-08**：那个"25 篇"是**迁移当时**的数；现行根目录 **32 篇**
> （`ls docs/*.md | wc -l`）——多出来的 `144` / `147` / `149` / `150` / `151` / `152` / `153` / `154`
> 是**迁移之后**新写的文档，**不在本表里**（本表只登记"旧文件去哪了"，不登记新文件）。

## 0. 三条判定规则

| 记号 | 含义 |
|---|---|
| **① 吸收** | 事实（地址 / 表格 / 常量 / 契约 / 四档标记）已并入某篇保留文档的指定小节，**逐字搬运**，标记原样保留 |
| **② 叙事** | 只是“本轮做了什么 / 下一步计划 / 失败的尝试过程”，事实部分早在别处；提交信息里有，故不入正文 |
| **③ 归档** | 原文保留在 `docs/archive/`，只是不再占据根目录；正文不改 |

任何一份归档文档都至少满足其中一条。**没有任何一份被就地删除**——
唯一的 5 处删除是“只做转发、正文早已在 `archive/`”的空壳页（见 §3）。

## 1. 为什么不让它继续堆下去

- 122 篇 / 33 288 行里，大量是**每个 agent 交付时写的“我这一轮做了什么”**
  （`126-level-obstacles`、`127-ai-speaks`、`133-replay-player`、`141-sim-turns`…）。
  那是**提交信息**，`git log` 已经有，放在 `docs/` 里只会稀释检索。
- 同题重复与连载：`34` 与 `102` 同题两篇；`40/55/56/61/103` 是关卡生成五篇；
  `47/48/50/52/53/54/57/58/60` 是一段 RNG 追猎的连载（早已结案）。
- 上一轮（`107-docs-consolidation`）已经归并过一批，但**仍然是按时间顺序编号**，
  读的人还是得先知道历史才找得到东西。本轮改成**按主题**：一个主题只有一篇。

## 2. 逐份映射（125 篇归档 + 24 篇保留）

| 原文件 | 去向 | 判定 | 说明 |
|---|---|---|---|
| `01-architecture.md` | 01-architecture.md | 保留 | 原文保留 |
| `02-module-map.md` | 02-module-map.md | 保留+扩写 | 新增 §15 = 原 112 的结论与地址总表 |
| `03-function-language.md` | 03-function-language.md | 保留+扩写 | 新增 §12 = 原 119 的 tag 表与格式规则 |
| `04-path-sampling.md` | 04-path-sampling.md | 保留+扩写 | 新增 §9 = 原 91 的复检规则与残留未知 |
| `05-shot-pipeline.md` | 05-shot-pipeline.md | 保留+扩写 | 新增 §7–§11 = 原 15 的端到端串讲、§12 = 原 138 |
| `06-verification.md` | 06-verification.md | 保留+扩写 | 新增 §18–§23 = 原 49/88/16/136+142/90/25+37+89 |
| `07-protocol.md` | 07-protocol.md | 保留+扩写 | 新增 §6–§9 = 原 96/114/43+46/31 |
| `08-plane.md` | 08-plane.md | 保留+扩写 | 新增 §9–§10 = 原 38/39 |
| `09-network.md` | 09-network.md | 新建 | 吸收 24/32/33/36/62/115（原 24 的事件目录改由 07 承载） |
| `10-campaign.md` | 10-campaign.md | 新建 | 吸收 118/131/102/105/125/34 |
| `11-levelgen.md` | 11-levelgen.md | 新建 | 吸收 55/56/61/103/51/40 |
| `12-rng.md` | 12-rng.md | 新建 | 吸收 53/54/57/58/47(+59/60)/113/50/48/52/40 |
| `13-ai.md` | 13-ai.md | 新建（主体 = AI.md） | 吸收 AI/97/99/110/95/100/87/94/98/109/127/79/80/85/93/12 |
| `14-turn.md` | 14-turn.md | 新建 | 吸收 35/41/42/44/45/86/141/117 |
| `15-target-life.md` | 15-target-life.md | 新建 | 吸收 65/30/122/123/83/84 |
| `16-client.md` | 16-client.md | 新建 | 吸收 108/92/116/120/121/137/124/129/130/145/126/128/78/133/134/139/135/140/143 |
| `17-resources.md` | 17-resources.md | 保留+扩写 | 保留 17 正文，新增 §2–§6 = 原 111 |
| `18-build.md` | 18-build.md | 保留 | 原文保留（原 19 早已并入 §6） |
| `19-android.md` | 19-android.md | 新建 | 吸收 132（941 行 → 结论/链路/平台对照/构建） |
| `20-source-recovery.md` | 20-source-recovery.md | 新建 | 吸收 146/104/101 |
| `21-guide-manual.md` | 21-guide-manual.md | 新建 | 吸收 148/106（矛盾台账住 §3） |
| `HANDOVER.md` | HANDOVER.md | 保留 | 引用改成新文档编号 |
| `INDEX.md` | INDEX.md | 重写 | 一屏索引 |
| `00-status.md` | 00-status.md | 重写 | 加入 §0 诚实声明、缺口表、文档地图 |
| `AI.md` | 13-ai.md §1–§14 | ① 吸收 | AI 的唯一权威，整篇成为 13 的主体 |
| `12-ai.md` | 02-module-map.md §13.2 + 13-ai.md | ② 叙事/已过时 | 自述「已过时」，只保留模块级负结果（已并入 02 §13.2） |
| `15-end-to-end.md` | 05-shot-pipeline.md §7–§11 | ① 吸收 | 端到端串讲原样搬运（去掉「如果要继续」的计划段） |
| `16-reproduce.md` | 06-verification.md §20 | ① 吸收 | 复现手册 |
| `24-network-protocol.md` | 09-network.md §1 + 07-protocol.md | ① 吸收 + ③ 归档 | 传输/帧/29 条真实样本进 09；事件枚举以 96/114 为准 |
| `25-live-collision.md` | 06-verification.md §23 | ① 吸收 | 结论吸收，步长勘误原样保留 |
| `30-kill-semantics.md` | 15-target-life.md §1 | ① 吸收 | 穿过即死的两次实测 |
| `31-event-generation.md` | 07-protocol.md §9 | ① 吸收 | 事件顺序 |
| `32-netcode-implementation.md` | 09-network.md §2 | ① 吸收 | 帧编解码能力表 |
| `33-server-handshake.md` | 09-network.md §3 | ① 吸收 | 握手 7 条 |
| `34-color-assignment.md` | 10-campaign.md §10 | ② 叙事 + 已被 102 取代 | 只到「规则未定」，结论以 102 为准 |
| `35-tick-clock.md` | 14-turn.md §1 | ① 吸收 | 60 Hz，113 个样本 |
| `36-match-server.md` | 09-network.md §4 | ① 吸收 | Match 组成 + 提取陷阱 |
| `37-paired-replay.md` | 06-verification.md §23 | ① 吸收 | 结论吸收，步长勘误原样保留 |
| `38-dynamic-mask.md` | 08-plane.md §9 | ① 吸收 | 否证型结论（负结果也是事实） |
| `39-punch-hole.md` | 08-plane.md §10 | ① 吸收 | 挖洞机制 |
| `40-level-generator.md` | 11-levelgen.md §7 + 12-rng.md §9 | ② 叙事（已被推翻） | OS 熵结论已推翻，只留作演进记录 |
| `41-turn-state-machine.md` | 14-turn.md §2 | ① 吸收 | 6 状态 7 事件 |
| `42-end-of-turn.md` | 14-turn.md §3 | ① 吸收 | 回合截止 |
| `43-turn-message-sequence.md` | 07-protocol.md §8 | ① 吸收 | 一回合报文顺序 |
| `44-destroy-delay.md` | 14-turn.md §4 | ① 吸收 | ≈165 tick |
| `45-turn-count-and-ending.md` | 14-turn.md §5 | ① 吸收 | 回合计数与结束 |
| `46-full-turn.md` | 07-protocol.md §8 | ① 吸收 | 12 条完整序列 |
| `47-turn-order-random.md` | 12-rng.md §6 | ① 吸收 | 出场顺序判定（已吸收 59/60） |
| `48-rng-replay-attempt.md` | 12-rng.md §9 | ② 叙事（方法错） | hook 粒度错误，保留为教训 |
| `49-verification-checklist.md` | 06-verification.md §18 | ① 吸收 | 验收清单 |
| `50-rng-source-reopened.md` | 12-rng.md §9 | ① 吸收 | 推翻过程与教训 |
| `51-obstacle-emit-chain.md` | 11-levelgen.md §6 | ① 吸收 | 生成→发送调用链 |
| `52-rng-hunt-status.md` | 12-rng.md §9 | ② 叙事（已被 53 超越） | 调用链部分已在 11 §6 / 12 |
| `53-rng-found.md` | 12-rng.md §2–§3 | ① 吸收 | ChaCha 定位与状态可读 |
| `54-rng-replay-verified.md` | 12-rng.md §5 | ① 吸收 | 随机流→障碍坐标命中 |
| `55-level-generator-closed.md` | 11-levelgen.md §2 | ① 吸收 | 公式闭合 |
| `56-levelgen-replay-complete.md` | 11-levelgen.md §3 | ① 吸收 | 13/13 逐位 |
| `57-seed-source.md` | 12-rng.md §3 | ① 吸收 | 惰性播种 |
| `58-rng-capability-summary.md` | 12-rng.md §4 | ① 吸收 | 能力清单 |
| `61-levelgen-rust.md` | 11-levelgen.md §5 | ① 吸收 | Rust 实现与边界 |
| `62-server-loop.md` | 09-network.md §5 | ① 吸收 | 整局服务端循环 |
| `65-life-rules-confirmed.md` | 15-target-life.md §1–§2 | ① 吸收 | 生命规则（已吸收 27/63） |
| `78-replay-files.md` | 16-client.md §10 | ① 吸收 | 回放文件位置与格式 |
| `79-replay-ai-corpus.md` | 13-ai.md §24 | ① 吸收 | 按等级分离 AI 函数 |
| `80-limit-vs-length.md` | 13-ai.md §24 | ① 吸收 | 超上限发射的解释 |
| `83-physics-verified.md` | 15-target-life.md §5 | ① 吸收 | 物理验证 + 步长勘误 |
| `84-kills-verified.md` | 15-target-life.md §5 | ① 吸收 | 击杀验证 + 步长勘误 |
| `85-statistical-check.md` | 13-ai.md §24 | ① 吸收 | 统计对照 |
| `86-simultaneous-turns.md` | 14-turn.md §6 | ① 吸收 | 同时回合制的结算顺序 |
| `87-function-assembly.md` | 13-ai.md §20 | ① 吸收 | 装配/补齐 |
| `88-coverage-matrix.md` | 06-verification.md §19 | ① 吸收 | 覆盖面矩阵 |
| `89-real-step.md` | 06-verification.md §23 + 04 §9 | ② 叙事（结论已并入） | 只有「步长 0.01」和「二分修复」两条仍现行 |
| `90-anti-waste.md` | 06-verification.md §22 | ① 吸收 | 失败清单与对策 |
| `91-bisection-loop.md` | 04-path-sampling.md §9 | ① 吸收 | 二分复检的逐条指令 |
| `92-ui-inventory.md` | 16-client.md §2 | ① 吸收 | 15/15 界面运行时台账 |
| `93-ai-time-budget.md` | 13-ai.md §10 | ① 吸收 | 25 s 墙钟（AI.md §8.5 已成正文） |
| `94-var-mode-trim.md` | 13-ai.md §21 | ① 吸收 | 生成时区分模式 |
| `95-elite5.md` | 13-ai.md §18 | ① 吸收 | 精英 5 不存在，按比例 1/5 |
| `96-protocol-coverage.md` | 07-protocol.md §6 | ① 吸收 | 73/73 覆盖 |
| `97-two-phase-scoring.md` | 13-ai.md §15 | ① 吸收 | 两阶段评分判据 |
| `98-shooter-selection.md` | 13-ai.md §9 | ① 吸收 | AI 不选射手（AI.md §8.4 已成正文） |
| `99-generation-dynamics.md` | 13-ai.md §16 | ① 吸收 | 父代/重评分/变体 B |
| `100-runtime-ai-checks.md` | 13-ai.md §19 | ① 吸收 | 运行时证实三条 |
| `101-gdc-contents.md` | 20-source-recovery.md §6 | ① 吸收 | .gdc 常量池 / 布局助手数值 |
| `102-color-assignment.md` | 10-campaign.md §10 | ① 吸收 | 队色表 + 线性探测 |
| `103-disk-levelgen.md` | 11-levelgen.md §1 + §4 | ① 吸收 | 圆盘式的定性（负结果） |
| `104-recovered-project.md` | 20-source-recovery.md §5 | ① 吸收 | GDRE 恢复与对拍 |
| `105-team-sides.md` | 10-campaign.md §10 | ① 吸收 | 队伍固定在 x 两侧 |
| `106-guide-crosscheck.md` | 21-guide-manual.md §6 | ① 吸收 | GW1 条目对账 |
| `107-docs-consolidation.md` | MIGRATION.md | ② 叙事 + 被本文取代 | 上一轮的文档清理报告；本轮改成按主题组织 |
| `108-client-rebuild.md` | 16-client.md §1 + §14 | ① 吸收 | 客户端台账 + 自定规则登记 |
| `109-kernel-turn-aigen.md` | 13-ai.md §23 + 14-turn.md §8 | ① 吸收 | 轮转 + 按士兵进化 |
| `110-gen0-and-variantb.md` | 13-ai.md §17 | ① 吸收 | 第 0 代 / 变体 B 触发 |
| `111-audio.md` | 17-resources.md §2–§6 | ① 吸收 | 音频清单/判据/播放链路 |
| `112-meta-managers.md` | 02-module-map.md §15 | ① 吸收 | 对局元信息八模块 |
| `113-hashmap-determinism.md` | 12-rng.md §7 | ① 吸收 | HashMap 迭代序实证 |
| `114-event-tags.md` | 07-protocol.md §7 | ① 吸收 | 72 条判别值全表 |
| `115-broker.md` | 09-network.md §6 | ① 吸收 | broker 协议与实现 |
| `116-kernel-api.md` | 16-client.md §3 | ① 吸收 | GDExtension FFI 接缝 |
| `117-server-wiring.md` | 14-turn.md §8 | ① 吸收 | 服务端接线 |
| `118-campaign-data.md` | 10-campaign.md §1–§8 | ① 吸收 | 21 关数据/存档/解锁/生成链 |
| `119-token-to-text.md` | 03-function-language.md §12 | ① 吸收 | token → 文本 |
| `120-hit-test.md` | 16-client.md §4 | ① 吸收 | hit_test 契约 |
| `121-visual-fidelity.md` | 16-client.md §5 | ① 吸收 | 启动/底色/图标 |
| `122-self-hit.md` | 15-target-life.md §3 | ① 吸收 | 按 id 排除 |
| `123-target-reset.md` | 15-target-life.md §4 | ① 吸收 | 靶子形态与复原规则 |
| `124-ui-geometry.md` | 16-client.md §7 | ① 吸收 | 视口/主题/文案 |
| `125-playscreen.md` | 10-campaign.md §11 | ① 吸收 | PlayScreen/JoinLevelPopUp 对齐 |
| `126-level-obstacles.md` | 16-client.md §8 | ① 吸收 | 障碍绘制与命中 |
| `127-ai-speaks.md` | 13-ai.md §22 | ① 吸收 | AI 函数上线 |
| `128-audio-client.md` | 16-client.md §9 | ① 吸收 | 音频播放层 |
| `129-player-screen.md` | 16-client.md §7 | ① 吸收 | PlayerScreen 对齐 |
| `130-popups.md` | 16-client.md §7 | ① 吸收 | 七个弹窗 |
| `132-android-build.md` | 19-android.md | ① 吸收 | 三平台/广告/内购/构建 |
| `133-replay-player.md` | 16-client.md §10 | ① 吸收 | 回放播放器 |
| `134-net-client.md` | 16-client.md §11 | ① 吸收 | 客户端联机层 |
| `135-fire-fx.md` | 16-client.md §12 | ① 吸收 | 开火表现 |
| `136-replay-diff.md` | 06-verification.md §21 | ① 吸收 | 对拍口径（数字已被 142 取代） |
| `137-pixel-fidelity.md` | 16-client.md §6 | ① 吸收 | 像素保真 |
| `138-ode-api.md` | 05-shot-pipeline.md §12 | ① 吸收 | Ode 模式 API |
| `139-play-online.md` | 16-client.md §11 | ① 吸收 | PlayScreen 接联机 |
| `140-plane-visual.md` | 16-client.md §12 | ① 吸收 | 准备圈/弹道画法 |
| `141-sim-turns.md` | 14-turn.md §7 | ① 吸收 | 三段流程与广播分组 |
| `142-diff-converge.md` | 06-verification.md §21 | ① 吸收 | 收敛原因与四条修正 |
| `143-play-flow.md` | 16-client.md §13 | ① 吸收 | 六条硬伤 |
| `145-tab-fix.md` | 16-client.md §7 | ① 吸收 | 动态合成底色 + 弹窗比例 |
| `146-source-recovery.md` | 20-source-recovery.md §1–§4 | ① 吸收 | 源码面回收 |
| `148-guide-manual.md` | 21-guide-manual.md §1–§5 | ① 吸收 | 手册元文档 |
| `131-level-rules.md` | 10-campaign.md §9 | ① 吸收 | 胜负判定 + 打不全就复原 |
| `09-report-decision.md` | 02-module-map.md §13 | ③ 归档 | 2026-11 上一轮已归档（报表/决策模块观测），正文在 archive |
| `10-lifecycle.md` | 02-module-map.md §13 | ③ 归档 | 2026-11 上一轮已归档（生命周期），正文在 archive |
| `11-dispatch.md` | 06-verification.md §12 | ③ 归档 | 2026-11 上一轮已归档（函数指针派发），已并入 06 §12 |
| `13-turns.md` | 14-turn.md + 02-module-map.md §13.3 | ③ 归档 | 2026-11 已归档；根目录只剩转发页，本轮删掉转发页 |
| `14-target.md` | 15-target-life.md + 02-module-map.md §13.4 | ③ 归档 | 同上（转发页已删） |
| `19-godot-verify.md` | 18-build.md §6 | ③ 归档 | 已并入 18 §6 |
| `20-live-oracle.md` | 06-verification.md §13 | ③ 归档 | 已并入 06 §13 |
| `21-oracle-limits.md` | 06-verification.md §14 | ③ 归档 | 已并入 06 §14 |
| `22-live-game-harness.md` | 06-verification.md §14 | ③ 归档 | 转发页已删；正文在 archive |
| `23-live-verification.md` | 06-verification.md §15 | ③ 归档 | 已并入 06 §15 |
| `26-harness-control.md` | 06-verification.md §15.3 | ③ 归档 | 转发页已删；正文在 archive |
| `27-death-flow.md` | 15-target-life.md §1–§2 | ③ 归档 | 已并入 65 → 本轮进 15 |
| `28-turn-order.md` | 12-rng.md §6 | ③ 归档 | 出场顺序；已并入 47 → 本轮进 12 §6 |
| `29-ai-probe.md` | 13-ai.md | ③ 归档 | AI 定量刻画；已被 AI.md 系列取代 |
| `59-turnorder-negative-and-two-rngs.md` | 12-rng.md §6 | ③ 归档 | 已并入 47 → 本轮进 12 §6 |
| `60-turnorder-verdict.md` | 12-rng.md §6 | ③ 归档 | 转发页已删；正文在 archive |
| `63-life-rules-status.md` | 15-target-life.md §1–§2 | ③ 归档 | 已并入 65 → 本轮进 15 |
| `64-join-real-server-attempt.md` | 09-network.md §5.5 | ③ 归档 | 连真服务器没打通；结论留在 09 §5.5 |
| `81-replay-validation.md` | 06-verification.md | ③ 归档 | 旧版 replay 验证（步长 0.5 口径），已被 06 §21 取代 |
| `82-stop-test.md` | 06-verification.md §23 | ③ 归档 | 结论已被 83 推翻（提取器 bug），保留为教训 |

## 3. 被删除的 5 个“转发页”（正文早已在 `archive/`）

这 5 个根目录文件只剩一句“本文件已并入 X，正文移入 archive/Y”，正文在上一轮就已归档。
保留它们只是为了避免旧引用变死链；本轮把它们删掉，**旧引用统一由本表解析**：

| 删除的根目录文件 | 正文所在 | 现在读哪里 |
|---|---|---|
| `docs/13-turns.md` | `archive/13-turns.md` | `14-turn.md` + `02-module-map.md` §13.3 |
| `docs/14-target.md` | `archive/14-target.md` | `15-target-life.md` + `02-module-map.md` §13.4 |
| `docs/22-live-game-harness.md` | `archive/22-live-game-harness.md` | `06-verification.md` §14 |
| `docs/26-harness-control.md` | `archive/26-harness-control.md` | `06-verification.md` §15 |
| `docs/60-turnorder-verdict.md` | `archive/60-turnorder-verdict.md` | `12-rng.md` §6 |

## 4. 其他归档内容

- `docs/archive/ai/`（12 篇）：2026-10 的 AI 逐轮记录，早已被 `AI.md` → 本轮 `13-ai.md` 取代。
  逐篇结论的合并关系见 `13-ai.md` §1–§14。
- `docs/guide/`（6 篇）：**本轮未动**，它是另一套读者（想赢的人）的手册，由
  `21-guide-manual.md` 作元文档与矛盾台账。

## 5. 怎么核对“没丢”

1. 任意抽一篇归档文档，把它的结论/常量/地址拿到本表查“去向”，再在目标文档里搜同一个字符串；
2. 四档标记（**逐位 / 观测 / 推断 / 未知**）在搬运时**原样保留**，不做升级或降级；
3. 如果两篇文档说法冲突，**不自行裁定**，而是记进 `21-guide-manual.md` §3 的矛盾台账
   （或本篇 §6 的补记），写明“说法 A（出处）/ 说法 B（出处）/ 尚未裁定”。

## 6. 本轮新增登记的矛盾（合并时发现的）

| # | 说法 A | 说法 B | 状态 |
|---|---|---|---|
| 1 | `RankInfo` 的 serde 原文写 “2 elements”（`112` §5.4） | `rank_manager` 实际写了 **3 个 dword**（`112` §5.4） | **尚未裁定**（`02-module-map.md` §15 原样记录） |
| 2 | `AI.md` 六档难度表 | `118` §12.4 从关卡立即数解出的 Bot 难度值 | **尚未裁定**（`10-campaign.md` §6 与 `13-ai.md` §1 各自保留原文，不圆） |
| 3 | 用户口述“准备圈 4 段” | 逐像素量得 **3 段**（`140` §3.4） | 以量测为准，口述差异记在 `16-client.md` §12 |
| 4 | 用户口述“光圈在函数非空时闭合” | 用户后续实测“点 Fire 之后才闭合”（`140` §0 #11） | 以**后一次实测**为准，已在 `16-client.md` §12 标注 |
