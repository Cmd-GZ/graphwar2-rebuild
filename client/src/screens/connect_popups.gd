extends GwScreen
class_name GwConnectPopup
## JoinIpPopUp / ConnectingPopUp / DisconnectedPopUp —— 三个"连接"弹窗。
##
## **节点树全部来自原版 `.tscn`**（三份都与 `recovered-src/` **逐字节相同**）：
##
##   client/scenes/screens/join_ip_pop_up.tscn      172 行 / 31 节点  "Ip:" + LineEdit + Cancel/Join
##   client/scenes/screens/connecting_pop_up.tscn   159 行 / 28 节点  "Connecting..." + Cancel
##   client/scenes/screens/disconnected_pop_up.tscn 160 行 / 28 节点  "You have been disconnected." + Ok
##
## 1920×1160 下量到的矩形（绝对像素，真机真帧）：
##
##   面板外框 (243,386) 1434×383   ← 三列比例 **1:6:1**（`stretch_ratio = 6.0`）
##   文案行   y 481..576（面板内 95 高）
##   Cancel   (853,676) 194×73     ← 面板中段、按钮行居中
##   返回键   (0,0) 146×96        ← 根节点上的浮层
##
## 细节（都是原版 `.tscn` 的原样，不是我们挑的）：
##   * 文案与按钮外面都套一层 `IpPopUpMargin.gd` 的 `MarginContainer`（**四边各 10 px**，
##     字号 = 宽/40）；
##   * 按钮行两侧各一个 `[h=3,v=3]` 弹性空格，两个按钮之间**没有**额外空格；
##   * `ConnectingPopUp` / `DisconnectedPopUp` 在 Cancel/Ok 右边还有一个**空**的
##     `IpPopUpMargin` 壳（20 px 宽）—— 它让按钮组比正中偏左 10 px，原版如此；
##   * `JoinIpPopUp` 的 "Ip:" 那个 MarginContainer **不参与弹性**（没有 `size_flags`），
##     右边的 LineEdit 外壳才是 `[h=3]`；
##   * `DisconnectedPopUp` 的 `MarginContainer` 带 `z_index = 5`（另外两份没有）。
##
## 本文件**不再造任何节点**（手抄的 `build()` 与 4 个骨架小工具已删，docs/06 §97），
## 只剩三件**我方控制器**的事（原版这三件事在 Rust `GraphwarPlayScreenControl`/网络层里）：
##   ① 把 `JoinIpIpLineEdit` 的 `text_submitted` 接到 `join_join_ip` 命令；
##   ② `current_text()` —— `app.gd::_submit_join_ip()` 与 selftest 都靠它读输入框；
##   ③ `on_enter()` 把焦点给输入框（原版点进 JoinIpPopUp 就能直接打字）。
##
## 观测（`docs/92` §4.7）：地址**必须**是 `host:port`；只写 IP 时原版毫无反应。
## `App._submit_join_ip()` 照抄了这条校验。真正的 TCP 连接/握手属于 Rust 侧
## （`docs/33`、`src/_scaffold/protocol.rs`）。

## 原版 `.tscn` 里输入框的节点名（只有 `join_ip_pop_up.tscn` 有）。
const LINE_EDIT_NAME := "JoinIpIpLineEdit"

var _edit: LineEdit

func _ready() -> void:
	font_divisor = 40        # IpPopUpMargin.gd：宽/40
	super()

func build() -> void:
	var edit := find_child(LINE_EDIT_NAME, true, false) as LineEdit
	if edit == null:
		return                       # connecting / disconnected 两份没有输入框
	_edit = edit
	# 回车 = 点 Join（原版按钮 `input_text = "join_join_ip"`，同一条命令）
	if not _edit.text_submitted.is_connected(_on_text_submitted):
		_edit.text_submitted.connect(_on_text_submitted)

func _on_text_submitted(_text: String) -> void:
	var a := app()
	if a != null:
		a.command(GwCommands.JOIN_JOIN_IP)

# ---------------------------------------------------------------- 状态

func current_text() -> String:
	return _edit.text if _edit != null else ""

func on_enter() -> void:
	if _edit != null:
		_edit.grab_focus()
