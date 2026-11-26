extends GwScreen
## ConfirmPlayExitPopUp —— 退出对局确认。
##
## **节点树全部来自原版 `.tscn`**：`client/scenes/screens/confirm_exit_pop_up.tscn` 与
## `recovered-src/confirm_exit_pop_up.tscn` **逐字节相同**（176 行 / 31 节点）。
## 1920×1160 下量到的矩形（绝对像素，来自真机真帧）：
##
##   VBoxContainer 的三条竖带  1920×382 / 1920×383 / 1920×382（sep 4）
##   面板外框 (482,386) 956×383     ← HBoxContainer2 的**中段**，三列比例 1:2:1
##   文案 Label (492,485) 936×67    ← 水平居中 + 垂直居中
##   No 按钮  (687,665) 154×73
##   Yes 按钮 (1061,665) 171×73     ← 两个按钮**分开**、整行居中
##   返回键   (0,0) 146×96         ← **挂在弹窗根节点上**的浮层，不在面板里
##
## 用户实测的四处差异全部由原版结构决定，现在都由 `.tscn` 承载：
##   ① 面板 1:2:1 三列（不是按内容最小尺寸 560×220 居中）；
##   ② 文案 Label 的 h/v alignment 都居中，外面套 `IpPopUpMargin.gd`（10 px 内边距，字号 = 宽/40）；
##   ③ No/Yes 在**面板中段**，左右各一个弹性空格夹着；
##   ④ 返回键在根节点上的 `MarginContainer2`（`MarginContainerPopUpBack.gd`，桌面 margin 全 0）。
##
## 两个按钮是**原生 `GraphwarButton`**，`input_text = cancel_exit_play` / `exit_play`，
## 由 `client/src/main.gd::_instantiate_screens()` 的 `GwButton.bind_native()` 接到 `App.command`。
## 本文件**不再造任何节点**（手抄树 + 4 个骨架小工具已删，docs/06 §97），也没有别的控制器逻辑：
## 「点哪颗弹框」属于**对局屏**（`input_text = confirm_exit_play`，见 `play_screen.gd`）。
##
## 观测（`docs/92` §4.11）：对局里 `ClipButton` 单击**不弹**框，`LongPressButton` 单击**弹**框
## —— `ClipButton` 只是动画层（`.tscn` 里它本来就是那个带 `AnimationRect` 的裁剪层）。

func _ready() -> void:
	# `IpPopUpMargin.gd`：内边距 10，字号 = 屏幕宽/40（文案与两个按钮都吃这个字号）。
	font_divisor = 40
	super()
