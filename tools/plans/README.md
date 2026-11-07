# `tools/plans/*.json` —— 真游戏的**点击脚本**（`tools/gw2_drive.py --plan`）

坐标都是 **1920x1160** 窗口（`Graphwar.x86_64 --resolution 1920x1160 --position 0,0`，
Xvfb `-screen 0 1920x1160x24 -listen tcp`，`DISPLAY=127.0.0.1:92`）下的**根窗口绝对坐标**，
由截图（`import -window root`）逐屏量出来。`Graphwar II` Linux **2.0.60**。

## 量出来的界面坐标（下一轮直接用）

| 屏幕 | 元素 | 坐标 | 备注 |
|---|---|---|---|
| 主菜单 | `Campaign` | `(534, 629)` | 四项之一 |
| Campaign（21 关） | 关卡节点 | 3 行 × 7 列蛇形：`0..6` 在 `y=220`，`13..7` 在 `y=463`，`14..20` 在 `y=728`；列心 `x = 239, 478, 719, 957, 1200, 1440, 1677` | `Level 2` = `(719, 220)`；`Level 3` = `(957, 220)`；`Level 0` = `(239, 220)` |
| 关卡弹窗 | `Start` | `(1070, 1009)` | 同屏 `Cancel` 在其左 `(838, 1009)` |
| 对局界面 | `f(x) =` 输入框 | `(1000, 1048)` | |
| 对局界面 | `Fire!` | `(1855, 50)`，**必须长按**（`mousedown; sleep 2; mouseup`） | `long_press` 按钮，单击无效（`docs/06` §35.1） |
| 对局界面 | 返回箭头 | `(75, 50)` | 会弹 `Are you sure you want to exit?` |
| 退出确认 | `Yes` | `(1144, 704)` | **只有走这一步游戏才会把回放写进 `user://replays/`**；直接 kill 进程不会 |

## 两个计划的用途

* `explore_campaign.json` / `explore_level2_popup.json` —— 只是"点进去截个图"，用来量上面那张表；
  它们证明了 `campaign.save = level_20` 之后 21 关**全部可点**（`docs/10` §2 的存档格式）。
* `level2_clean.json` —— **本任务的正式脚本**：进 Level 2 → 探针 `x` → `calc_slope`（从
  `--out` 那个 JSONL 里读炮口算 `x_m/y_m`）→ `{m}*x` 连打两炮（弹道过原点 ⇒ 必撞障碍）→
  返回 + `Yes`（写回放）。
* `level2_aimed.json` —— 同一个套路但打三炮、不退出（产出 `fixtures` 里 `run2#*` 那三条）。

⚠ `calc_slope` 里的路径**必须和 `gw2_drive.py --out` 一致**（脚本是边跑边读自己写的 JSONL）。
