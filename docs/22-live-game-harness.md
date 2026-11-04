# 22 · 让原版游戏在我们自己的 harness 里跑（达成）

这是从"拿不到玩法真值"到"原版游戏活着"的转折点。

## 1. 三个卡点，逐个拆掉

| # | 卡点 | 解法 |
|---|---|---|
| 1 | 包里只有 `.ctex`/`.import`，源 `.svg`/`.png` 不在包内 → `Resource file not found` | `tools/make_placeholders.py` 生成同名占位（149 svg + 16 png） |
| 2 | `Ad_holder.gd` 依赖 AdMob **原生插件** → `LoadAdError` 等类型未定义，脚本解析失败 | 用自己的 `Ad_holder.gd`（`extends Control` + 5 个信号）替换编译字节码 `Ad_holder.gdc` |
| 3 | **本文件的主角**：自动加载/脚本一个都不跑，引擎只打印 banner 就无限加载资源 | 游戏自带 **`project.binary`**（编译后的项目设置）覆盖了我们的 `project.godot`，删掉它 |

第 3 条是排查出来的：同样的 `project.godot` + 脚本放到空项目里能正常跑
（`PROBE_READY` 打印），放进 `godot-live` 就完全没反应。
把 `project.binary` 改名后立刻正常。

## 2. 结果（实测）

```
$ godot --headless --path godot-live
PROBE_READY
PROBE_F60 current=GraphwarRootNode:<GraphwarRootNode#60246984152>
TREE root [Window]
TREE   probe [Node]
TREE   GraphwarRootNode [GraphwarRootNode]
TREE     VBoxContainer [VBoxContainer]
TREE     IAP_holder [Control]
TREE     Ad_holder [Control]
TREE     IOS_album_holder [Control]
TREE     Android_mediastore_holder [Control]
```

**原版 `GraphwarRootNode`（Rust 写的游戏根节点）运行在我们的项目里**，
并且把它的整棵子节点建起来了。

## 3. 这意味着什么

- 之前 `docs/21` 的结论"玩法没有真值来源"**已经不成立**：
  现在原版逻辑是活的，可以用 GDScript 驱动、用 frida hook、用引擎内观察。
- 仍然缺的是"**怎么让它进入一局对战**"——根节点建好后停在哪个界面，
  以及用什么调用序列能开局，还没探明。

## 4. 复现

```bash
cd godot-live && rm -f project.binary            # 关键：让 project.godot 生效
python3 ../gw2-rebuild/tools/make_placeholders.py .
godot --headless --editor --path . --quit         # 首次导入
godot --headless --path .                         # 运行
```

`godot-live/` 由 `rsc/`（修好命名后的导出资源）+ 原版 `.so` + 我们的桩构成，
属于本地产物，已 gitignore。

## 5. 开一局真实对局（本轮达成）

### 5.1 按钮链路（靠枚举按钮摸出来的）

```
MainMenu  .../VBoxContainer2/LocalButton                    text=Battle
LanLobby  .../MarginContainer/HBoxContainer/CreateButton    text= Create Game
GameSetup .../OptionsContainer/AddBotButton                 打开 AddBotPopUp
AddBotPopUp .../VBoxContainer/OptionsContainer/AspectRatioContainer/AddBotButton   选难度，加 Bot
GameSetup .../MarginContainer/HBoxContainer/StartButton     text= Start
```

按完后 `ScreensControl` 的可见子节点从 `GameSetup` 变成 **`PlayScreen`** —— 对局开始。

**坑**：`AddBotPopUp` 打开时会挡住 Start；不加 Bot 直接 Start 不会报错、也不会切屏（静默无操作）。

### 5.2 对局里玩法函数确实在跑（hook 计数）

```bash
node gw2-rebuild/tools/hook_live_game.js
```

12 秒一局，`libgraphwarrust.so` 上的调用计数：

```
parse_function  11      eval            1       rasterize     38
point_test   21629      mask_ctor       2       life_entry    12
turn_a       1259       turn_b       1531      levelgen    1 + 1
shot_calc       0       calculate_path  0
```

⇒ **回合、生命、关卡、碰撞全部在实际执行**，而且可以用 frida 观察。
`docs/21` 里"玩法没有真值来源"的结论**正式作废**。

### 5.3 现在能做但还没做的事

- 用 `Interceptor.attach` 抓参数，问出回合/生命的语义；
- 在同一局里主动调用内部函数（活体 oracle），与我们的复刻对拍；
- `shot_calc=0 / calculate_path=0` 说明这一局 Bot 还没开火（只跑了 12 秒），
  需要更长观察窗或让 Bot 真正行动，才能覆盖开火结算。

## 6. 复现

```bash
# 手动开一局（按钮链路见 5.1），probe.gd 会自动按
godot --headless --path godot-live
# 带 hook 观察
node gw2-rebuild/tools/hook_live_game.js
```
