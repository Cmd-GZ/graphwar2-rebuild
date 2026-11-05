# 144 · 联机大厅与关卡弹窗：四条**实测 bug** 的证据、修法与残差

状态：**现行**。分支 `agent/lobby-fixes`。
本文回答四件事：① 用户实测报的四条 bug 各自能指到什么证据；② 改了哪几行、新旧行为差在哪；
③ 每条加了哪些断言（都能在改动前变红）；④ 还剩什么没验。

> 前置：`134-net-client.md`（联机层三通道）、`130-popups.md` §1（**真值管线**）、
> `129-player-screen.md` §4（大厅真帧口径）、`101-gdc-contents.md` §4.5（`OptionsContainer.gdc` 真源码）、
> `132-android-build.md` §1.1（Android 包的 `.symtab` 未 strip，是模块/符号级证据的第一来源）。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级（沿用 `AGENTS.md`）：
**逐位验证** = 与字节/逐像素一致；**观测** = 能指到地址、符号、字节或帧；
**推断** = 有间接证据、缺什么写在旁边；**未知** = 没有证据。
另加一档 **自定** = 本项目自己的选择，不是复原结论。

---

## 0. 一页结论

| # | 用户看到的 | 真因 | 原版出处 | 等级 |
|---|---|---|---|---|
| 1 | 上一把创建的本地房间留在列表里 | 大厅用 `state.players`（**对局状态**）拼"我的房间"；退出对局不清 ⇒ 行永远在 | `GraphwarLanVBox::update_game_list<Iter<SocketAddr,GameInfo>>` 每帧重投影；`exit_lan_lobby` → `stop_lan_broadcaster() + stop_server_broker_handler()` | 观测（符号 + 指令）|
| 2 | Global 页点占位行提示"这一行没有房间地址" | 占位行是**我们自定**的，没有地址；原版**没有无地址的行** | 房间表的键就是 `SocketAddr`；Join → `start_client_to_remote_server(addr)` | 观测（泛型参数 + `bl`）+ 自定（占位行）|
| 3 | 别人进房间，`Players` 列不动 | 人数是**建行时拍的快照**；原版每帧从活数据源重取 | `update_game_list` 的 filter 闭包：先查 broadcaster 的 `HashMap<SocketAddr,GameInfo>`，再查 broker 的，取不到就丢掉这一行 | 观测（逐条指令）|
| 4a | 关卡弹窗"下面的部分太长" | 屏幕级少了那一行 **96 px**，面板从 956 涨到 1056、一直顶到屏底 | 真值场景 `join_level_pop_up.tscn` 的节点 dump | 观测（真值管线）|
| 4b | 坐标模式的图标背景不是白色 | 四个模式按钮**一个样式盒都没挂**，白底那格丢成面板色 | `axis_button_theme.tres`（白底）+ `game_menu_icon_theme.tres`（#1A3B2D），两份只差底色 | 观测（真值场景的 `theme` + `StyleBoxFlat.bg_color`）|

数字：客户端自检 **581 → 605**（+24，0 失败 0 跳过）；`cargo test --offline` **333**（未改 Rust）。

---

## 1. bug 1：房间不留在大厅里

### 1.1 原版的数据模型（观测）

Android 那份 `.so` 没 strip（`docs/132` §1.1 已立此先例），`.symtab` 里能直接读到模块名与完整
泛型签名。与大厅相关的两个：

```
graphwarrust::graphwar_lan_vbox::GraphwarLanVBox::update_game_list
    <std::collections::hash::map::Iter<core::net::socket_addr::SocketAddr,
                                       graphwarrust::graphwar_game_info::GameInfo>>
<graphwarrust::graphwar_lan_vbox::GraphwarLanVBox as IVBoxContainer>::process      (14 524 B)
```

⇒ **房间列表就是一张 `HashMap<SocketAddr, GameInfo>` 的投影**：每一行由"地址"标识、内容是那一条
`GameInfo`。没有地址就没有行（这条直接决定了 §2）。

`update_game_list::{{closure}}::{{closure}}`（ARM64 `0x449124`，264 B）是那张表的 **filter**，
从头读下来是：

```text
ldr  x8, [x20,#0x10]                 ; 借用检查（RefCell 已可变借用 ⇒ panic）
ldr  x9, [x20,#0x128]                ; LanVBox.lan_broadcaster
cbz/cmp 门                         ;
bl   GraphwarLanBroadcasterV6::get_game_info(&self+0x128, &addr)   ; ← ① LAN 表
cbnz x0, use_it                      ;
ldr  x21,[x20,#0x240]                ; LanVBox.server_broker_handler
bl   GraphwarServerBrokerHandler::get_game_info(&self+0x240, &addr); ← ② broker 表
cbz  x0, drop_row                    ;          两处都没有 ⇒ 这一行被丢掉
use_it: mpmc::Sender::send(GameInfo) ; 交给渲染侧
```

两处 `get_game_info` 都是 **HashMap 查表**（`GraphwarLanBroadcasterV6::get_game_info`，ARM64
`0x3ddce8`：`[x0+0x90]` = 桶数组、`[x0+0x78]`/`[x0+0x80]` = 控制字节、bucket stride `0x78` = 120 B，
键按 `SocketAddr` 的各字段逐项 `cmp`）。

⇒ **每行的数据是"每次迭代现取"的，不是缓存**：源里没了，行就没了；源里人数变了，行里的人数值就变了
（这条同时是 §3 的出处）。

### 1.2 什么时候源会没（观测 + 推断）

`GraphwarRootNode::process_button_pressed` 用 `input_text` 做**开码化字符串比较**（`mov/movk` 拼
8 字节字面量再 `ccmp`）。把命令键解出来以后，两条关键的：

| 命令 | 处理块 | 做了什么 |
|---|---|---|
| `exit_lan_lobby` | `0x3a9520` | `GraphwarBackend::stop_lan_broadcaster()`（`0x407068`）→ `stop_server_broker_handler()`（`0x4074f8`）→ `strb wzr,[x25+0x704]`（把屏选择清零 = 回主菜单） |
| `ok_disconnected` | `0x3a9484` | `stop_client()`（`0x406ba4`）→ `stop_server()`（`0x406c0c`） |
| `create_game` | `0x3a9770` | `start_server(port)`（`0x406c64`）→ `start_client_to_local_server()`（`0x407848`） |

`stop_lan_broadcaster` 全二进制**只有一个调用点**，就是 `exit_lan_lobby` 那一支
（`0x3a9538`）—— 也就是说：**出大厅 = 广播器停 = 自己那一行从 LAN 表里消失 = 列表里没有这一行了。**
（"LAN 表里只有自己那间"还是"自己也收别人"，本轮没证到，见 §6。）

### 1.3 我们错在哪

`client/src/screens/lan_lobby.gd` 以前这样拼"我的房间"：

```gdscript
var has_local_room: bool = app_node.state.players.size() > 0 and not app_node.online
if has_local_room and lan_rooms.is_empty():
    _list.add_child(_room_entry(String(app_node.state.room_name), "%d" % app_node.state.players.size()))
```

`state.players` 是**对局状态**，`_leave_to_main_menu()` 不清它 ⇒ 退出对局再进大厅，这一行还在。
根因是拿"对局状态"当"房间状态"。

### 1.4 改法

* `client/src/app/app.gd`：新增 `local_room`（`{}` = 没开），三个方法
  `_open_local_room()` / `close_local_room()` / `local_room_row()`。
  它的生命周期对齐原版的 `start_lan_broadcaster()` / `stop_lan_broadcaster()`：
  * `create_game` 时开；
  * `_leave_to_main_menu()`（= 出大厅 / 退出对局 / 退出 GameSetup）时收；
  * `connect_to()`（去连别人的房间）时收 —— 对应原版 `start_client_to_*` 之前的 `stop_server`。
* `client/src/screens/lan_lobby.gd`：Local 页那一行改读 `app.local_room_row()`；
  返回 `{}` 就**不画**这一行。

新旧行为：

| 场景 | 改前 | 改后 |
|---|---|---|
| Create Game → 大厅 | 1 行 | 1 行 |
| 出大厅再进来 | **1 行（残留）** | 0 行 |
| 打完一局退出 → 大厅 | **1 行（残留）** | 0 行 |

断言（`client/tools/selftest.gd::_test_lobby_fixes`）：
`开房：create_game 之后 App 有本机房间`、`开房：Local 页画出自己那一行`、
`退出大厅：本机房间被收掉`、`退出大厅再进来：Local 页没有残留房间`、
`退出对局：本机房间被收掉`、`退出对局再进大厅：Local 页没有残留房间`。

---

## 2. bug 2：Global 页的占位行点了会报"这一行没有房间地址"

### 2.1 原版里根本没有"没有地址的行"（观测）

见 §1.1：列表是 `HashMap<SocketAddr, GameInfo>` 的投影，**地址是键**。所以"某一行没有地址"
在原版里是不可表达的状态 —— 那个错误提示是我们自己造出来的。

` Join ` 按下后做什么：`GraphwarLanVBox::process` 里有**唯一一处**
`bl GraphwarBackend::start_client_to_remote_server`（ARM64 `0x450d68`；全二进制共两处，
另一处在 `GraphwarRootNode::process_button_pressed` 的 `0x3a9470`，是"按 IP 加入"那条），
前面紧接着

```text
ldr  x8, [x24,#0x10] ; cbnz x8, <bail>   ; 已经在连 ⇒ 不再发起
mov  x8, #-1         ; str x8,[x24,#0x10]
add  x0, x24, #0x18  ; backend
add  x1, sp, #0xa0   ; ← 这一行的 SocketAddr
bl   start_client_to_remote_server
str  x27,[x24,#0x10] ; 复位
```

⇒ 语义是"连这一行自己的地址"，而且有一个**防重入闸门**。
（把这段与"就是 JoinButton 的处理"对应起来是**推断**：`update_game_list` 被内联进 `process`，
`JoinButton.pressed` 的闭包也在里面；我读到的是 `process` 里唯一的
`start_client_to_remote_server` 调用点 + 它前面的 SocketAddr 拷贝。**缺**：没抓到一次真机点击的
运行时调用记录。）

### 2.2 那 4 条占位行是什么

`docs/129` §4.4 的"离线真帧四行"和 `docs/92` §4.4 的"五行"**都不是静态数据**：
`libgraphwarrust.so` 的 `.rodata` 里**没有** `US Room` / `EU Room` 任何一条（本机实测
`strings -a libgraphwarrust.so | grep -c "US Room"` = 0），`Graphwar.pck` 里也没有
（`grep -aob "US Room" Graphwar.pck` 无命中）。⇒ 那些行来自**真实 broker 的推送**，
每一行都带自己的 `address`。我们的四行是**自定**的离线兜底（`docs/134` §0 已经写明
broker 地址默认空是本项目的取舍）。

### 2.3 改法

`_room_entry()` 里：`address` 为空 ⇒ `JoinButton.mouse_filter = MOUSE_FILTER_IGNORE`，
**不接 `pressed`**；按钮照画（保持真帧里那一格的外观），点它什么都不会发生。
`_on_join_pressed("")` 的提示已删除（只留一条防御性 `return`）。
有地址的行（LAN 广播 / broker）行为不变。

新旧行为：

| 场景 | 改前 | 改后 |
|---|---|---|
| 点 Global 占位行的 ` Join ` | 冒一句"这一行没有房间地址" | 无反应、无提示、不发起连接 |
| 点 LAN/broker 行的 ` Join ` | 连该行地址 | 不变 |

断言：`Global 页占位行数 = 4`、`Global 占位行的 Join 不接受点击`、
`点占位行的 Join 不弹任何提示`、`点占位行的 Join 不发起连接`。

> 形态上仍然是 4 行：现有断言 `每条房间都有 Join 按钮` 只检查"存在一个 `JoinButton` 节点"，
> 不检查可点性。**自定**：保留占位行是为了和离线真帧的矩形一致；一旦接上真实 broker
> （`GW2_BROKER_ADDR`），这 4 行会被真实房间表整页替换。

---

## 3. bug 3：`Players` 列不随人数增长

### 3.1 原版怎么更新（观测）

§1.1 的 filter 闭包已经给出答案：每次迭代都 `get_game_info(addr)` **现取**。
所以只要源里那条 `GameInfo` 的 `current_player_count` 变了，下一帧画出来的数字就变了 ——
不需要"删行重建"，也不需要额外的通知通道。

我们的错误：`_rebuild_list()` 把人数 `%d` **格式化成字符串**塞进 `Label.text`，
之后只在 `rooms_changed` 信号到达时整页重建。人数从 1 变 2 时：

* 对 LAN/broker 行：其实已经能更新（`GwLanDiscovery._same_row` / `GwBrokerClient._same`
  都把 `current_player_count` 算进"变了就发信号"），所以这两页**本来就**会刷新；
* 对**自己那间房**：数字来自 `state.players.size()` 的快照，而这条路径**没有任何信号**，
  于是永远停在建行那一刻的值 —— 这就是用户看到的"别人进来人数不动"。

### 3.2 改法

* `lan_lobby.gd` 加 `_process()` → `_sync_list()`：**每帧**重新算一遍"应该画什么"
  （对齐原版 `process` 的节奏）。键序列（地址）没变就只改两个 `Label` 的文本，
  变了才回退到 `_rebuild_list()`。这样滚轮位置/动效不会因为人数变化被重置。
* 自己那间房的人数改成 `App.local_room_row()` 里**实时读** `state.players.size()`
  （不是建表时拍快照）。

新旧行为：

| 场景 | 改前 | 改后 |
|---|---|---|
| 房主大厅里，人数 1 → 2（无信号） | 仍显示 1 | 下一帧显示 2 |
| 同一行人数变化 | 整页重建（行节点换掉） | 只改文本，行节点不变 |

断言：`人数：刚开房时 Players = 1`、`人数：别人加入后 Players 列变 2`、
`人数：只改文本，不重建行（同一行节点还在）`。

> **仍是推断的部分**：原版"别人加入 → 服务端那份 `GameInfo` 被改写"这一环，我们只证到
> "列表每帧从 `GameInfo` 里取"。`GameInfo` 本身由谁在什么时候更新（服务端回 `NewConnection`
> 之后哪条路径改 `current_player_count`）本轮**没有查**，因为我们的客户端目前不跑服务端
> （`docs/134` §0：广播循环在 `src/`，本轮不动）。自检因此把"人数变了"直接模拟在
> `state.players` 上，验的是**界面侧的活投影**，不是服务端协议。

---

## 4. bug 4：关卡弹窗（`join_level_pop_up.tscn`）

### 4.1 量法（沿用 `docs/130` §1 的真值管线）

正版 `Graphwar.pck` + `libgraphwarrust.so` 喂 stock `Godot_v4.7.2`。本轮**没有**改那条管线，
只是把它复制到 `.agent-work/lobbyfix/probe/harness/`（另一个 agent 的 `probe/` 没动），
加了一个只读探针 `probe.gd`：实例化 `res://join_level_pop_up.tscn`，把每个 Control 的
`position/size/custom_minimum_size/size_flags/stretch_ratio`、每个 Button 的
`theme` 资源路径与五个 `StyleBoxFlat` 的 `bg_color / border / corner_radius` 全打出来。

窗口 1920×1160（`--resolution`；实测与 X screen 无关，Godot 会按 pck 的 `project.binary` 建
1920×1160 的画布）。**逐节点真值**：

```text
MarginContainer(满屏) → VBoxContainer(sep 4)
  MarginContainer2   1920×96                       ← 返回键行
  HBoxContainer2     1920×956  EXPAND ratio 10
    MarginContainer    239  EXPAND ratio 1
    MarginContainer2  1434  EXPAND ratio 6         ← 3:6:3 落在 239/1434/239
      Panel            1434×956                    ← 与下一个是**兄弟**，不是 PanelContainer
      VBoxContainer    1434×956
        LevelTitleLabel          1434×90   V-SHRINK-CENTER
        HBoxContainer            1434×535  EXPAND ratio 2
          MarginContainer2   64  (ratio 1)
          MarginContainer  1297  (ratio 20) → ScrollContainer 1277×515（内边距 10）
          MarginContainer3   65  (ratio 1)
        OptionsContainer         1434×226  EXPAND ratio 0.5, cms=(0,225.25)
          MarginContainer 235 + 4×AspectRatioContainer 236 + MarginContainer2 236
            AxisModeButton     226×226   theme=axis_button_theme.tres
            FunctionModeButton 226×226   theme=game_menu_icon_theme.tres
            TurnModeButton     226×226   theme=game_menu_icon_theme.tres
            TimeModeButton     226×226   theme=game_menu_icon_theme.tres   icon=<none>
        HBoxContainer3           1434×93
          MarginContainer3 485 + MarginContainer(Cancel 214) + MarginContainer2(Start 238) + MarginContainer4 485
            Cancel 194×73 / Start 218×73（字号 48 = 1920/40，容器内边距 10）
  HBoxContainer3     1920×96   ← **combined_min=(0,0)、无子节点，却占 96**
  AdContainer        1920×0
```

**样式盒（白底的出处）**：`AxisModeButton` 的 `normal/hover/pressed/disabled` 四个态全是
`bg=(1,1,1,1) + border=(0,0,0,1)×3px + radius=10`；另外三个是
`bg=(0.102,0.2314,0.1765) = #1A3B2D +` 同样的黑边与圆角。两份 `.tres` 除底色外逐字段相同 ——
正好是 `client/src/ui/theme_factory.gd::mode_icon_stylebox(on_white)` 已经实现的那一对
（`docs/129` §4.5 在**大厅房间行**里用过）。

### 4.2 4a "下面的部分太长"

像素口径（沿中间列找纯黑边框带，阈值 `sum(dark)>250`）：

| | 面板上沿 | 模式行上沿 | 模式行下沿 | Cancel/Start | 面板下沿 |
|---|---|---|---|---|---|
| 真值 | 100–102 | 733–735 | 956–958 | 973–975 / 1043–1045 | **1053–1055** |
| 改前 | 100–102 | 807–810 | ~1039–1041 | 1071–1072 / 1140–1142 | **1153–1155** |
| 改后 | 100–102 | 722–724 | 954–957 | **973–975 / 1043–1045** | **1053–1055** |

改前面板高 1056（一直顶到 y=1155），真值 956。差的 100 = **屏幕级那行 96 + 一个分隔 4**。
所以修法不是调比例，而是**把那一行补上**：`vbox` 的第三个孩子是一个空的 `HBoxContainer3`，
`custom_minimum_size.y = 屏宽 × 0.05`（1920 下 96，与返回键同高），在 `apply_font_scale()` 里设。

顺带按真值修了三处（同一批 dump 里读出来的，不修就白量）：

1. `Panel` + `VBoxContainer` 改成**兄弟**（以前是 `PanelContainer`，stylebox 的
   `content_margin` 会再内缩 3 px，真值两者是同一个 1434×956 矩形）；
2. 说明栏补上 1:20:1 的两侧垫片 + 中间那层 10 px 内边距（真值 64 / 1297 / 65）；
3. `OptionsContainer` 改成 `EXPAND + ratio 0.5`，`custom_minimum_size.y = 0.25 × 父高`
   —— 公式来自**真源码**（`OptionsContainer.gd`：`self.custom_minimum_size.y = 0.25 * parent_size.y`，
   `docs/101` §4.5 有全文）。

**已知残差 14 px**：真值场景里那格 `custom_minimum_size.y` 是 **225.25 = 0.25 × 901**，
而父容器（面板 VBox）当时已经是 956。901 是 ad 条折叠（`docs/130` §1.2 的 `visible=false→true`）
之前那一次 `resized` 触发的 `call_deferred("set_minimum_size")` 留下的值，之后没有新的
`resized`，循环就停了 —— 也就是**原版这一次没收敛**。我们按公式求值 ⇒ 239（模式按钮 235×235，
真值 226×226）。上表里改后模式行上沿 722 vs 真值 733、下沿 954 vs 956，就是这 14 px 的两端分摊。
这是**推断**：没有别的证据能区分"225 是设计常量"与"225 是布局时序残留"；按源码实现公式是可辩护的
选择，写在这里以便下一个人推翻。

### 4.3 4b "坐标模式的图标背景不是白色"

改前四个按钮**一个样式盒都没挂** ⇒ 全部落回 app 主题的 `Button/normal`（= #1A3B2D），
白底那格就没了（真帧改前实测 `axis` 像素 = `(26,59,45)`，即面板色）。
改后：`GwThemeFactory.mode_icon_stylebox(i == 0)` 挂到四个态上 ⇒ 第一格
`(255,255,255)`，另外三格 `(26,59,45)`，与真值场景逐像素一致（`docs/130` 的同一对 `.tres`）。

断言：`弹窗：屏幕级 VBox 有 4 行`、`弹窗：底部那行高 = 返回键高`、
`弹窗：面板下沿离屏底 = 返回键高 + 两个分隔`（视口 ≥ 800 px 时才量像素，headless 下会溢出）、
`弹窗：四个模式按钮都是正方`、`弹窗：四个模式按钮撑满 1/6 格`（同上）、
`弹窗：坐标模式白底、其余三格面板色`、`弹窗：表示坐标模式的那一格背景是白色`、
`弹窗：OptionsContainer.min.y = 0.25 × 父高`、`弹窗：OptionsContainer 竖直 EXPAND`。

---

## 5. 动了哪些文件

| 文件 | 改了什么 |
|---|---|
| `client/src/screens/lan_lobby.gd` | 每帧重投影（`_process`/`_sync_list`/`_wanted_rows`/`_same_keys`）；自己那间房改读 `App.local_room_row()`；占位行的 ` Join ` 不可点；删掉"没有房间地址"提示 |
| `client/src/screens/join_level_pop_up.gd` | 补屏幕级 96 那行；`Panel`+`VBox` 改兄弟；说明栏 1:20:1；`OptionsContainer` EXPAND/ratio/公式；四个模式按钮挂 `mode_icon_stylebox` |
| `client/src/app/app.gd` | 新增 `local_room` + `_open_local_room`/`close_local_room`/`local_room_row`；`create_game` 开、`_leave_to_main_menu` 与 `connect_to` 收 |
| `client/tools/selftest.gd` | 新增 `_test_lobby_fixes`（24 条断言）；既有断言一条没动 |
| `docs/144-lobby-fixes.md` | 本文 |

`client/src/net/` **没有改**：`GwLanDiscovery` / `GwBrokerClient` 的"内容一变就发
`rooms_changed`"本来就对（`_same_row` / `_same` 都包含 `current_player_count`）；
本轮修的是**视图侧的活投影**，不是采集侧。

复现本文的测量：

```bash
# 真值（需要正版 Graphwar.pck + libgraphwarrust.so）
mkdir -p .agent-work/lobbyfix/probe/harness/pck
cp Graphwar.pck .agent-work/lobbyfix/probe/harness/pck/
cp libgraphwarrust.so .agent-work/lobbyfix/probe/harness/
Xvfb :93 -screen 0 1920x1160x24 & 
DISPLAY=:93 tools/Godot_v4.7.2-stable_linux.x86_64 \
  --path .agent-work/lobbyfix/probe/harness --rendering-driver opengl3 \
  res://probe.tscn -- --scene=res://join_level_pop_up.tscn

# 我们自己的（含逐节点 GEOM 输出）
DISPLAY=:93 tools/Godot_v4.7.2-stable_linux.x86_64 --path client \
  --rendering-driver opengl3 --resolution 1920x1160 -- --shots=<outdir>

# Rust 出处（Android 未 strip 的 .so，逐符号 + aarch64 反汇编）
readelf -sW .reference/x/arm64/lib/arm64-v8a/libgraphwarrust.so | c++filt | grep lan_vbox
llvm-objdump -d .reference/x/arm64/lib/arm64-v8a/libgraphwarrust.so | less
```

---

## 6. 仍未知

1. **`GraphwarLanBroadcasterV6` 的表里到底有谁**：`get_game_info(addr)` 按地址查表是观测；
   "表里既放自己那间、也收别人的广播"是**推断**（`set_room_info` + 按地址查的多条表结构）。
   缺：一次多客户端的运行时观测，或 `GraphwarLanBroadcasterV6::process`（ARM64 `0x3de310`，
   2 040 B）的完整读解。
2. **`GameInfo.current_player_count` 在服务端侧由谁改写**（§3 的推断部分）。我们的客户端不跑
   服务端，这条只能在 `src/`（本轮不动）或真机抓包里验。
3. **Join 的防重入闸门语义**（§2.1 的 `[x24,#0x10]`）：观测到"非零就不发起"，但那个字段
   什么时候复位、失败后要不要等超时，**未知**。
4. **`OptionsContainer` 的 225 vs 239**（§4.2）：见那里的残差说明，**推断**。
5. **屏幕级 `HBoxContainer3` 那 96 px 从哪来**：真值里它 `combined_min=(0,0)`、没有子节点、
   也没有 `EXPAND`，却拿到 96 —— 高度像是原版那次布局分配出来的**剩余量**，机制**未知**。
   我们用一个 `custom_minimum_size.y = 屏宽×0.05` 的空行复现同一个矩形（推断：与返回键同高）。
6. **`TimeModeButton` 的图标**：真值场景里它 `icon=<none>`（第 4 格是空的），字号 96；我们
   按 `GwModes.time_icon(level.time)` 画了 `turn_inf`。真机运行时那格到底由谁填、填什么，
   本轮没验（`join_level_pop_up.tscn` 单独实例化时 Rust 控制器不跑，静态值就是空）。
7. **大厅真帧的 4/5 条公网房间**：§2.2 只证到"不是静态数据、来自 broker"，具体几次观测
   各是几条（`docs/92` 5 条、`docs/129` 4 条）没法复核，因为都依赖当时那台 broker。
