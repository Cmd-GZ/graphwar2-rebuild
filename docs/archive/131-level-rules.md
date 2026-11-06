# 131 · 关卡胜负判定：靶子进命中、按关卡类型分胜负、"打不全就复原"

状态：**现行**。本文回答三件事，都是同一条 bug 链上的：

1. **"发射一次就通关"的根因是什么**（§1）——不是"射击被当成胜利"，是**存活统计**把打靶关误判了；
2. **胜负条件现在怎么分**（§2）——打靶关看靶子、对战关看队伍、回合上限真的算数；
3. **靶子怎么接进命中判定**（§3）、**"打不全就全部复原"怎么落地、哪一段是推断**（§4）。

> 真值依据（全是既有结论，本文**没有**重新挖二进制）：
> [`118-campaign-data.md`](118-campaign-data.md) §7（21 关全参数：目标数 / Bot / 回合上限）、
> [`123-target-reset.md`](123-target-reset.md)（目标重置的**形状**已钉住、**判定点未知**）、
> [`120-hit-test.md`](120-hit-test.md)（`hit_test` 的契约与坐标）、
> [`84-kills-verified.md`](84-kills-verified.md) / [`86-simultaneous-turns.md`](86-simultaneous-turns.md)
> （击杀与同时回合的实测）、[`122-self-hit.md`](122-self-hit.md)（致死半径是**数据字段**）、
> [`30-kill-semantics.md`](30-kill-semantics.md)（障碍挡弹道、士兵不挡）。
> 本文**不改** `docs/INDEX.md` 与 `docs/HANDOVER.md`，也**没有**碰
> `client/src/app/app.gd` 的结算段（`report_match_result()` / `mark_completed`，另一位 agent 在改）。

证据分级沿用 `AGENTS.md`：**逐位验证** / **观测** / **推断** / **未知**。
本文的地址一律 **ELF 虚地址**（原文出处见各条注明的文档）。

---

## 0. 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 根因 | `_check_match_over()` **只数两队士兵存活数**（`alive = {0:n, 1:m}`）。打靶关**没有敌方士兵** ⇒ `alive[1] == 0` ⇒ 第一发一落地就判人类胜。**射击跟胜负无关，是存活统计误判** | 观测（旧代码 + 用户现象） |
| 靶子参与判定 | 是。靶子与士兵塞进**同一个** `hit_test` 第 3 参数（`[x,y,r]*n`），内核把它变成 `graphwar_shot_calculator::settle::Target{id: 下标}` —— **没有任何"士兵"语义过滤** | 逐位验证（`src/_scaffold/api.rs:266`/`:284`、`src/graphwar_shot_calculator/settle.rs:73`）+ 观测（端到端打中） |
| 打靶关胜利 | **全部靶子被摧毁** | 推断转观测：等价于"`Team2` 全灭"（`docs/123` §2.1/§4.5，45/45 回放）；本文端到端实测 |
| 打靶关失败 | **回合上限用尽仍未清完**（`turn_limit > 0` 时） | **推断**（`docs/123` §5.3 #1：语料 0 样本）+ 端到端实测 |
| 对战关胜负 | 对方队伍全灭 ⇒ 胜；我方全灭 ⇒ 负 | 观测（`docs/84` §1 / `docs/123` §4.5：45/45） |
| 回合上限 | **真的算**，按"结算一发 = 一个回合"计 | 逐位 + 运行时（`docs/118` §7.1 的 `turn_limit` 列，`Level 7` = 1）+ 推断（"用尽 ⇒ 判负"） |
| 空放/走火 | **不判胜**；只消耗一个回合（所以 `Level 7`（上限 1）空放一发就是失败） | 观测（本文 §5.2 ①④） |
| 打不全就复原 | 实现了，标 **推断（依据用户实测 + `docs/123` 的形状）**；**只翻 alive**、不重建实体、不碰掩膜 | 推断（形状部分逐位/观测，判定点**未知**） |
| 新文件 | `client/src/game/target_set.gd`（138 行）、`client/src/game/level_rules.gd`（183 行） | — |
| 改动 | `client/src/screens/play_screen.gd` **+231/−26**；`client/tools/selftest.gd` **+273/−0**（只新增断言） | — |
| `bash client/run.sh selftest` | **191 → 242 passed, 0 failed**（+51，**未改/未删任何既有断言**；191 = 已合入 `origin/main` 的基线） | — |
| `cargo test --offline` | **296 passed**（本任务没动 Rust；条数不变） | — |
| 回归反证 | 把 `_check_match_over()` 换回旧实现 ⇒ 新增断言里 **9 条失败**（§5.3 原文照录） | 观测 |
| 仍未定 | 重置的**判定点**、"全部复原"还是"只复原本发打掉的"、是否重置回合计数、视觉效果、`Level 3` 这种**没有写 "single shot"** 的多目标关是否也适用同一规则、靶子半径/坐标、同时回合下双方同时清场的判负 | 见 §6 |

一句话：**"发射一次就通关"是存活统计的误判，不是射击被当成胜利。**
现在靶子进了 `hit_test`、胜负按 `docs/118` §7.1 的关卡类型分开判、回合上限真的算、
打不全就复原（推断）；四个端到端场景都走真实开火路径验过。

---

## 1. 根因：存活统计把打靶关判成了"对方全灭"

### 1.1 旧实现（`play_screen.gd`，改动前）

```gdscript
func _check_match_over() -> void:
	var alive := {0: 0, 1: 0}
	for s in _soldiers:
		if bool(s.alive):
			alive[int(s.team)] = int(alive[int(s.team)]) + 1
	if int(alive[0]) > 0 and int(alive[1]) > 0:
		return
	# 任一队为 0 ⇒ 结束；winner = 0 if alive[0] > 0 else 1
	_running = false
	app_node.report_match_result(winner == human_team, winner)
```

而靶子当时**只用于绘制**：`_plane.set_targets(_target_positions(...))`（`GwPlaneView` 画
`skins/target.svg`），命中判定里构造的实体数组**只来自士兵**
（`s.pos` + `GwCore.SOLDIER_HIT_RADIUS`）——靶子根本没进 `hit_test`。

### 1.2 为什么打靶关一定误判（`docs/118` §7.1 的 13 个目标关）

打靶关只有**一个玩家、没有 Bot**（`bot_tier = -1`）⇒ `_soldiers` 里**没有一个 team 1 的士兵**
⇒ `alive[1] == 0` 恒成立 ⇒ 第二轮循环条件 `alive[0] > 0 and alive[1] > 0` 为假
⇒ `_check_match_over()` **第一发就判结束**，且 `winner = 0`（人类队）⇒ `human_won = true`
⇒ `report_match_result(true, 0)` ⇒ `store.mark_completed(level_index)` ⇒ **通关**。

⇒ 三个现象一次解释完：**能不能打中靶子、打了几个，全都不影响结果。**

### 1.3 旧代码在同时回合制下也不会"立刻"结束（澄清一条容易搞错的）

同时回合制下，不是"所有人打完"那一发只会**入队**、不会调 `_resolve_shots`，所以击杀还没落地；
旧代码随后调 `_check_match_over()` 时看到的仍是旧的 alive 表。这个分支在打靶关上不成立
（13 个打靶关**全部是顺序回合**，见 §2.2），所以上面那条误判覆盖了全部打靶关。

---

## 2. 胜负条件：按关卡类型分开

### 2.1 规则表

| 关卡类型 | 判据 | 胜利 | 失败 |
|---|---|---|---|
| **打靶关**（`targets > 0`、`bot == 0`；13 关 = `Level 0/1/2/3/4/6/7/8/10/12/13/16/17`，标题号即 0 基 index） | `GwLevelRules.for_campaign()` | **全部靶子被摧毁** | 回合上限用尽仍未清完（`turn_limit > 0`） |
| **对战关**（`bot > 0`、`targets == 0`；8 关 = `Level 5/9/11/14/15/18/19/20`） | 同上 | **对方队伍全灭** | 我方队伍全灭 |
| 两者都有（`targets > 0` 且 `bot > 0`） | 同上（`Kind.BOTH`） | 目标全部达成 | 任一失败条件 |

依据：

* **目标数 / Bot 数 / 回合上限** 三列来自 `docs/118` §7.1（`S` 立即数 + `C` 闭包预设 + `R`
  385 份回放，21/21 对齐）；本实现直接读 `GwCampaign.LEVELS`（同一个数据源，`campaign.gd` 文件头有出处表）。
* **对战关 = 对方全灭**：`docs/84` §1 与 `docs/123` §4.5 —— 45/45 份出现 `EndingGame` 的回放里，
  胜方都是"某队全灭"，`Team2` 存活实体为 0。**观测**。
* **打靶关 = 靶子全清**：`docs/123` §2.1 证明靶子就是 `Team2` 里的普通实体（`0x46f310` 里发
  `TeamInfo{Team2}`，逐位验证），§4.5 又证明 45/45 的结束都是 `Team2` 全灭 ⇒
  **"靶子全清"与"Team2 全灭"在原版里是同一件事**。**观测 + 逐位验证**。
  我们的客户端没有给靶子分配队伍（靶子只在 `GwPlaneView` 里画），所以这里显式写成
  "目标全部达成"，语义等价。
* **回合上限用尽 ⇒ 失败**：`docs/118` §7.1 的 `turn_limit` 是真值（`Level 7` = 1，描述
  "Destroy your target in your first try"），但**用尽之后的判决语料里没有样本**
  （`docs/123` §5.3 #1：唯一上限为 1 的样本首发就命中）⇒ **推断**。

### 2.2 为什么 `Level 7` 的"1 回合"不会误伤

`turn_limit` 只在**打靶关**生效（判定里 `has_targets()` 才看它）。21 关里有上限的 **11** 关
**全是打靶关**（`Level 1/2/3/4/6/7/8/10/12/13/17`），有上限的对战关为 **0** 个；
而 13 个打靶关**全部是顺序回合**（`turn=0`），
所以"一个回合"没有歧义（`docs/123` §5.1 #3 的结论在这里扩到 13/13）。

### 2.3 判定顺序与 `reason` 码（`GwLevelRules.evaluate()`）

```
① 对战：我方全灭            → over, human_won=false, reason="own_wiped"
② 打靶：靶子全部被摧毁      → over, human_won=true,  reason="targets_cleared"
③ 对战：对方全灭            → over, human_won=true,  reason="enemy_wiped"
   ③' 同一刻两边都空        → over, human_won=false, reason="mutual_wipe"   ← 未观测，见 §6
④ 打靶：turn_limit>0 且 已用回合 >= 上限 → over, human_won=false, reason="turn_limit"
⑤ 其余                    → not over, reason="playing"
   连"双方都不在场"也判不了 → not over, reason="no_verdict"（旧代码在这里会随便判一个人赢）
```

"失败优先"是**我们的选择**（`BOTH` 类型才可能同时命中 ①②③）——21 关数据里**没有**
同时有靶子和 Bot 的关卡（`docs/118` §7.1），所以这条分支目前是**不可达**的，只按规则表定义好。

### 2.4 打靶关失败时"胜方"是谁

`report_match_result(human_won, winner_team)` 的签名**没有改**（`app.gd` 那段另一位 agent 在动）。
打靶关没有别的玩家队，所以 `winner_team` 取 `1 - human_team` = **1 = Team2**。
这**不是凑数**：靶子在原版里就属于 `Team2`（`docs/123` §2.1），所以"关卡没打完 ⇒ Team 2 wins"
与原版语义一致，结算框的文案也就对得上。

### 2.5 我们对旧实现的**有意**行为变更

| 场景 | 旧 | 新 | 理由 |
|---|---|---|---|
| 打靶关空放一发 | 判胜、通关 | 不判胜，只消耗一个回合 | §1 的根因 |
| 打靶关打掉部分靶子 | 判胜、通关 | 复原 + 继续 | §4 |
| 打靶关用尽回合上限 | 不可能发生（早就"胜"了） | 判负 | §2.1 |
| 一队都没有（例如回放模式没有玩家表） | `0 == 0` ⇒ 判人类胜 | `no_verdict`，不判 | 旧行为是纯粹的假阳性 |
| 只有一个队伍、也没靶子（UI 里不可达） | 判人类胜 | `no_verdict` | 同上 |

---

## 3. 靶子怎么接进命中判定

### 3.1 借用 `hit_test` 的第 3 个参数（**先读了签名才敢借**）

`GW2Rebuild.hit_test(points, obstacles, soldiers)` 的 `soldiers` 是扁平 `[x, y, radius] * n`
（`docs/120` §1.1）。**读了内核实现**（`src/_scaffold/api.rs:266` 起）：

```rust
pub fn hit_test(points, obstacles, soldiers) -> HitOutcome {
    // 掩膜只放**障碍**
    let targets: Vec<crate::graphwar_shot_calculator::settle::Target> = soldiers.iter().enumerate()
        .map(|(i, &(x, y, radius))| crate::graphwar_shot_calculator::settle::Target { id: i as i64, x, y, radius })
        .collect();
    let out = crate::graphwar_shot_calculator::settle::resolve_points(&mask, &targets, points);   // 判定本体只此一份
    ...
}
```

⇒ **没有任何"这是士兵"的过滤**：每个三元组只是"一个带半径的靶点"，`id` 就是**数组下标**，
`resolve_points`（`src/graphwar_shot_calculator/settle.rs:73`）只做"弹道点落在半径内 ⇒ 记一次死亡"。
`docs/120` §1.2 已经把这个参数叫"致死半径"而不是"士兵半径"，本文只是把它用满：

* 士兵照旧进这个数组（半径 `GwCore.SOLDIER_HIT_RADIUS` = 0.5）；
* **靶子追加在士兵之后**（半径 `GwTargetSet.HIT_RADIUS` = 0.5）；
* 内核回的 `hits` 是**下标**，所以 `play_screen.gd::_resolve_shots()` 用一张并行的
  `owners` 表把下标翻译回"是哪个士兵 / 哪个靶子"。

这是**借用关系**，不是 `hit_test` 原本的语义（它本来只服务士兵）——写在这里以免以后有人
以为内核认识"靶子"这个概念。真值里的靶子与士兵确实是同一种东西（都是带 `radius` 的实体，
`docs/122` §1、`docs/123` §2.1），所以这条借用不算跨语义。

### 3.2 靶子半径 = 0.5（**推断**）

`docs/122` §1：致死半径是**士兵记录自带的数据字段**（条目 `+0x18`，`PosInfo.radius`
在 5300 条回放里恒 `0.5`），**不是**编译期常量。靶子的半径**没有观测**
（`docs/118` §8 #16：靶子坐标与半径都未解，运行时随机）。

⇒ 这里按 `GwTargetSet.HIT_RADIUS = 0.5` 实现（与士兵同值），并在代码与本文都标 **推断**。
将来若从回放里拿到靶子的 `PosInfo.radius`，只改这一个常量即可（`entities()` 统一用它）。

### 3.3 靶子**不**进掩膜（不挡弹道）

`hit_test` 只把 `obstacles` 光栅化进掩膜；`soldiers`（含靶子）只按半径判死、**不挡弹道**
（`docs/30` §2 的建模：障碍让弹道停下，实体只是被穿过）。所以"打中靶子"不会让弹道中断。

### 3.4 画的和判的是**同一份**靶子集合

`_targets: GwTargetSet` 同时供两处：

* `_plane.set_targets(_targets.alive_positions())` —— 只画**还活着**的靶子
  （`GwPlaneView.set_targets()` 收 `Array[Vector2]`，它只管画、没有 alive 概念；
  所以"靶子被打掉"= 数组少一项，"复原"= 多一项）；
* `_targets.entities()` —— 只含活着的靶子，交给 `hit_test`。

⇒ 与障碍（`docs/126` §4）同一条纪律：**看得见的与判得了的必须是同一批数据**，
否则就会出现"画着靶子却打不中"或"靶子没了还能被打"。

---

## 4. "打不全就全部复原"（**推断**）

### 4.1 形状部分（有证据）

| 结论 | 依据 | 档位 |
|---|---|---|
| 复原**不是**重建实体（不新分配 id、不发 `TargetInfo`、不发 `EntityRemoved`） | 造目标的 `0x46f310` **只有 13 个调用点，全在关卡开场**；109 份含目标的回放里目标**从未**被 `EntityRemoved`（`docs/123` §2.2、§4.3 ③） | 逐位验证 + 观测 |
| 复原只能表现为 **`LifeInfo{alive:true}`** | 目标只有 `TargetInfo`/`LifeInfo` 两种状态事件；分派器 `0x3e2830` 的 70 个分支里没有一处构造 `life_info`（`docs/123` §3.4） | 逐位验证 + 推断 |
| 复原**不碰掩膜**（障碍的洞留着） | 掩膜整局只建一次、`ClearObstacles` 397 份里从不中途出现（`docs/123` §3.2 问题 5） | 观测 |
| 目标整局**不移动、不重生、不计时** | `PosInfo` 只在开场与开局快照出现（`docs/123` §5.3 #7） | 观测 |

### 4.2 判定点与条件（**推断，且必须当作推断读**）

```gdscript
# GwTargetSet.settle_shot(destroyed_this_shot)
# 本发一个都没打掉            → 不复原（走火/空放什么都不发生）
# 本发把靶子清空              → 不复原（全部达成）
# 否则                        → 复原**全部已摧毁的靶子**
```

* **判据**："本发摧毁的集合 ≠ 全部靶子 ⇒ 全部复原"。等级 **推断**：来源于
  **用户实测**（"一发没打全 ⇒ 已摧毁的目标全部复原"）与 `docs/123` §3.2 问题 2 的
  最强推断（"等价说法：结算后 `Team2` 仍有存活 ⇒ 全部复原"）。
* **判定时刻**：放在**一次射击结算结束时**（`_settle_round()` 里、判胜负之前）。
  `docs/123` §3.2 问题 1 明确这是**三种候选之一**（射击结算 / `end_turn` / 下次开火前），
  **二进制里没找到判定点**，语料里 **397 份回放 0 次部分命中**（`docs/123` §4.3 ①）
  ⇒ **没有真值可以对**。
* **为什么不写进文档当结论**：`AGENTS.md` 硬规矩 1。本文与代码注释都把它标成
  `推断（依据用户实测 + docs/123 的形状结论）`。

### 4.3 回合粒度与"顺序"这两条口径（也是推断）

* **一个回合 = 一次射击结算**。战役关 `TimeModeInfo = TimerInf`（`docs/123` §5.1 #9），
  **没有超时路径**，所以一个回合只能由"打完一发"推进；LAN 的 3 分钟超时不计回合
  （那些对局 `turn_limit` 恒为 0）。
* **同时回合制下，一轮 = 一个回合**：`_resolve_shots()` 把整批弹道一起结算，
  所以"打不全就复原"与"判胜负"都作用在**整批**上。13 个打靶关都是顺序回合，
  所以这条口径目前不影响任何一关（`docs/123` §5.1 #3）。
* **复原事件与回合边界的先后未证实**：语料的回合边界块（`TurnCountInfo` 等四条）里
  **从来不含 Life/Target 事件**（`docs/123` §4.4），所以"复原插在哪"没有样本。
  我们的客户端没有协议层，`_settle_round()` 的顺序是"先复原、再判胜负"，仅此而已。

### 4.4 一个**没有采用**的替代方案

`docs/123` §6.2 建议过逐关布尔量 `all_in_one_shot`（只有 `Level 6/8/10/13/17` 为真，
即描述里写了 "in a single shot" 的 5 关）。本文**没有**采用，而是**对所有打靶关通用**，
理由与代价都写在 §6 第 5 条：

* 用户口述是**一条通用规则**，父 agent 也按通用规则下令；
* "N 个靶子 + 有限回合"的关卡如果允许分次打完，`Level 3` 的 10 回合、
  `Level 9` 的 6 回合就都变成了宽松的计时器；
* **代价**：`Level 3`（3 个靶子、10 回合，描述里**没写** single shot）从"分次能打完"变成
  "必须一发全清"——13 个打靶关里**只有它**同时满足"多目标"与"描述没写 single shot"，
  而这一条**与原版是否一致是未知的**（0 样本）。

⇒ 实现上只有**一个**决策点（`GwTargetSet.settle_shot()`），要改成逐关开关只需给它多加
一个 `bool` 参数，不改别处。

---

## 5. 验证

### 5.1 复现命令

```bash
cd .agent-work/lvlrules/repo        # 本文的所有数字都是在这个隔离开克隆上跑的
export XDG_DATA_HOME=$PWD/../xdg/data XDG_CACHE_HOME=$PWD/../xdg/cache XDG_CONFIG_HOME=$PWD/../xdg/config
export CARGO_HOME=/home/user/MyProject/gw2/.cargo-home
export CARGO_TARGET_DIR=/home/user/MyProject/gw2/.agent-work/lvlrules/cargo-target

cargo test --offline                                   # 296 passed
cargo build --offline && cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/
"$GODOT" --headless --path client --import             # 新 class_name 必须重新扫描！
bash client/run.sh selftest                            # 242 passed, 0 failed
```

（`client/run.sh` 在本克隆里算出来的 `$ws` 少一级，所以 `GODOT=<真 Godot 路径>` 要显式给。）

### 5.2 四个端到端场景（`client/tools/selftest.gd::_test_level_rules()`）

全部走**真实开火路径**：`fire()` → `GwCore.calculate_path()`（内核 `path_calculate`）
→ `GwCore.hit_test()`（内核 `hit_test`）→ `_settle_round()` → `_check_match_over()`
→ `report_match_result()`。只有"每局随机的靶子/障碍布局"是注入的
（`PlayScreen.inject_layout()`；原版靶子坐标本来就随机，`docs/118` §8 #16），
打法是把靶子放在 `f(x)="0"` 那一发的水平线上/外——修正常数 `c = y0 − f(x0)`
（`docs/24` §6.2）会把这发钉成 `y = 士兵的 y`。

| # | 场景（关卡取 `docs/118` §7.1 的真参数） | 实测断言（节选） | 结果 |
|---|---|---|---|
| ① | **打靶关空放**：`Level 0`（1 靶、无上限、无障碍），靶子放在弹道外 `(12, 2.0)`，打 `f(x)=0`（水平线 `y=−5.1`） | `running == true`（旧实现这里通关）；结算框**不可见**；`last_result` 未被改写；靶子仍 1 个；`turns_used == 1`；`reason == "playing"` | **通过** |
| ① | **打靶关全清**：`Level 0`，靶子放在弹道线上 `(12, −5.1)` | 靶子 `alive == 0`；状态行 `"本轮命中 0 个士兵、1 个靶子"`；`running == false`；`reason == "targets_cleared"`；`last_result = {human_won: true, winner_team: 0}`；结算框可见 | **通过** |
| ② | **打掉部分 ⇒ 全部复原**：`Level 6`（2 靶、上限 6），一个靶在弹道线上、一个在 `y=2.0` | 状态行记录"打掉 1 个靶子"；**结算后 `alive == 2`**（已摧毁的复原）；`running == true`；`reason != "targets_cleared"`；`turns_used == 1`；`GwPlaneView.targets` 回到 2 项 | **通过** |
| ③ | **对战关：对方全灭**：LAN 1v1（人 + Bot，各自 `y=−5.1`），`f(x)=0` 正好穿过对方士兵 | `reason == "enemy_wiped"`；`last_result = {human_won: true, winner_team: 0}` | **通过** |
| ③ | **对战关：我方全灭**：同一条结算路径，但弹道由**对方**打出（`apply_external_shot()`，见 §5.4） | `reason == "own_wiped"`；`last_result = {human_won: false, winner_team: 1}` | **通过** |
| ④ | **`Level 7`（index 7，上限 1）空放**：1 靶、上限 1，靶子放在弹道外 | `turns_used == 1` 且靶子还在；`reason == "turn_limit"`；`human_won == false`；`winner_team == 1`（Team2）；结算框可见 | **通过** |

另外还有一组**纯逻辑**断言（不依赖内核）：13 关的类型分类（`targets` / `versus` /
`both`）、`Level 7` 的 `turn_limit == 1`、五种 `reason` 的判定、以及 `GwTargetSet`
的"全清不复原 / 部分复原 / 空放不动作 / 重复 destroy 幂等 / 半径 0.5"。

### 5.3 回归反证：把旧实现放回去，新断言会红

为了证明这组断言**不是**自证，我把 `_check_match_over()` 临时换回旧实现（其余不变）跑了一遍：

```
SELFTEST 233 passed, 9 failed
  FAIL ① 空放一枪 ⇒ 局还在（running = true，旧实现这里直接通关）
  FAIL ① 空放一枪 ⇒ **不弹结算框**
  FAIL ① 空放一枪 ⇒ **没有写胜负结果**
  FAIL ① 判定理由 = targets_cleared（实际 {  }）
  FAIL ② 部分命中 ⇒ 不通关
  FAIL ③ 对方全灭 ⇒ 判定 enemy_wiped（实际 {  }）
  FAIL ③ 我方全灭 ⇒ 判定 own_wiped（实际 {  }）
  FAIL ④ 回合上限用尽仍未清完 ⇒ 判负、理由 turn_limit（实际 {  }）
  FAIL ④ 打靶关失败 ⇒ 胜方 Team2（靶子的队伍，docs/123 §2.1；实际 { "human_won": true, "winner_team": 0 })
```

换回本文的实现后 **242 passed, 0 failed**。（"① 靶子真的能被打中"那条在旧实现下**也**通过 ——
因为靶子进 `hit_test` 是另一处改动，与旧的存活统计无关，这正好说明两条改动各自独立。）

### 5.4 自检用到的两个新入口（都只加、不改）

| 入口 | 用途 | 备注 |
|---|---|---|
| `PlayScreen.inject_layout({targets, obstacles})` | 把**每局随机**的靶子/障碍布局钉住 | 原版两者都是随机的（`docs/118` §8 #16）；不注入时行为与之前完全一样 |
| `PlayScreen.apply_external_shot(points, shooter_player)` | 把"别人打过来的一发"送进**同一条**结算路径 | 客户端还没有 AI 开火循环（对手不开火）、服务端也还没接；接上之后它就是那个回调本身 |
| `PlayScreen.level_state()` | 只读快照：`running / rules / targets / turns_used / verdict / shot_status / obstacles` | 自检与 `--dump` 诊断用，不参与逻辑 |

### 5.5 这一轮**没有**验的

* **真机**（有显示器的窗口渲染）：只在 `--headless` 下跑；靶子"消失/复原"是
  `GwPlaneView.targets` 的项数变化，**没有**像素级截图比对（`docs/121` §7 的老账）。
* **内核缺席**（`client/*.so` 不在）时整条路径：`GwCore.hit_test()` 会退到 `_approx_hits()`
  几何近似，靶子同样在这个数组里，所以胜负规则仍然成立；但本轮的 242 条是**内核在**时跑的。
* **协议层**：我们没有事件流，所以"复原要广播 `LifeInfo{alive:true}`"这条（`docs/123` §3.2
  问题 4）**没有**实现——只翻了本地 alive 位。
* **`app.gd` 的结算段**：按任务边界一个字没动，`report_match_result(human_won, winner_team)`
  的签名与调用约定不变。

---

## 6. 仍未确定

| # | 问题 | 状态 | 缺什么 |
|---|---|---|---|
| 1 | 复原的**判定点**（射击结算 / `end_turn` / 下次开火前） | **未知**（本文取"射击结算结束时"） | 二进制里的判定点，或一份**部分命中**的回放（语料 0/397） |
| 2 | "全部复原"还是"只复原本发打掉的" | **未知**（实现取"全部已摧毁的"，单发场景下两者等价） | 跨回合累积的样本 |
| 3 | 复原会不会**重置回合计数** | **未知**（实现：不重置） | `docs/123` §5.3 #3，同上缺样本 |
| 4 | 复原有没有**视觉效果**（火花） | **未知**（实现：无） | 同上 |
| 5 | `Level 3`（3 靶、10 回合，描述里**没写** "in a single shot"）是否也适用"打不全就复原" | **未知** ⇒ 本实现按**通用规则** | 该关的一次部分命中回放 |
| 6 | 靶子的**致死半径** | **推断 0.5**（`docs/122` 证明半径是数据字段，靶子值未观测） | 回放里的 `PosInfo.radius`（靶子的） |
| 7 | 靶子的**坐标**分布 | 我们的自定值（`_target_positions()`，右侧半区） | `docs/118` §8 #16：原版随机，本来就没有固定坐标 |
| 8 | 同时回合制下**双方同时把对方清空**怎么判 | **未观测**（实现：判人类负，`mutual_wipe`） | `docs/123` §5.3 #5 同类 |
| 9 | 对手会不会开火 | **不会**（客户端没有 AI 开火循环，既有缺口，不是本次引入） | AI/服务端接线（另一位 agent 的线） |
| 10 | `BOTH` 类型（既有靶子又有 Bot） | **21 关数据里不存在**，规则按 §2.1 的表格实现 | 自定义关/服务端下发的关卡 |
| 11 | 打靶关失败时结算框的文案（"Team 2 wins"） | 语义上对（靶子属于 Team2），但**没有原版真帧** | 一次"回合用尽"的原版录像 |
| 12 | 回放模式（没有玩家表）现在的行为 | 从"第一发判人类胜"改成 `no_verdict`（不判） | 回放播放器整体未做（`docs/00-status` §4.2 第 1 项） |

---

## 7. 证据分级汇总

| 结论 | 档位 | 出处 |
|---|---|---|
| 旧实现只看两队士兵存活数 ⇒ 打靶关必误判 | **观测**（代码 + 用户现象） | §1；`git diff` 的旧函数体 |
| `hit_test` 的第 3 参数没有"士兵"语义（就是半径实体表） | **逐位验证** | `src/_scaffold/api.rs:266`/`:284`、`src/graphwar_shot_calculator/settle.rs:73` |
| 靶子被打中会让 `alive` 翻 false、且能驱动胜负 | **观测**（端到端跑通） | §5.2 ①③ |
| 对战关胜负 = 队伍全灭 | **观测**（45/45） | `docs/84` §1、`docs/123` §4.5 |
| 打靶关胜利 = 靶子全清（≡ Team2 全灭） | **观测 + 逐位验证** | `docs/123` §2.1/§4.5 |
| 每关目标数 / Bot / 回合上限 | **逐位验证 + 运行时**（21/21） | `docs/118` §7.1；`client/src/app/campaign.gd` |
| 回合上限用尽 ⇒ **判负** | **推断** | `docs/123` §5.3 #1（0 样本）；本文 §2.1 |
| 打不全就全部复原 | **推断**（依据用户实测 + 形状结论） | `docs/123` §3.2 问题 2；本文 §4 |
| 复原只翻 alive、不重建、不碰掩膜 | **逐位验证 + 观测** | `docs/123` §2.2、§4.3、§3.2 问题 5 |
| 靶子半径 = 0.5 | **推断** | `docs/122` §1；`docs/118` §8 #16 |
| 13 个打靶关都是顺序回合 | **观测**（21/21） | `docs/123` §4.2；`campaign.gd` 的 `turn` 列 |

---

## 8. 复现（只想看结论时跑这两条）

```bash
# ① 四个端到端场景 + 规则单测
bash client/run.sh selftest | tail -3          # SELFTEST 242 passed, 0 failed

# ② 回归反证：把 _check_match_over 换回旧实现（alive 统计）再跑，会红 9 条
#    （旧函数体见 §1.1；跑完记得换回来）
```

`git diff` 锚点（逐文件）：

| 文件 | 变化 |
|---|---|
| `client/src/game/target_set.gd` | **新增** 138 行：靶子集合 + `settle_shot()`（推断的那条规则） |
| `client/src/game/level_rules.gd` | **新增** 183 行：类型分类 + `evaluate()`（胜负条件） |
| `client/src/screens/play_screen.gd` | **+231/−26**：靶子进 `hit_test`、`_settle_round()`、`_check_match_over()` 重写、三个诊断/测试入口 |
| `client/tools/selftest.gd` | **+273/−0**：只新增断言（既有 191 条一条没动） |
