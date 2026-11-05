# 26 · 把 harness 当玩家用：能做什么、卡在哪

目标：在 harness 里以玩家身份开火，制造一次死亡，从而取得伤害/生命的真值。
结果：**开火成功，函数注入失败**，因此仍然没有死亡样本。

## 1. 已经能做的（实测）

| 能力 | 做法 | 证据 |
|---|---|---|
| 进对局 | 按 `LocalButton → CreateButton → AddBotButton → StartButton` | `ScreensControl` 切到 `PlayScreen` |
| 找到开火按钮 | `PlayScreen/.../RightControls/HBoxContainer/FireContainer/FireButton` | 对局开始后 `FireContainer` 有 2 个子节点（`ClipButton` + `FireButton`） |
| **真的开火** | `emit_signal("pressed")` on `FireButton` | 协议里出现 `{"FunctionFireRequest":{"player_id":229}}` |
| 找到函数输入节点 | `find_children("*", "GraphwarFunctionInput", true, false)` | 唯一一个：`PlayScreen/.../HBoxContainer2/GraphwarFunctionInput` |

## 2. 卡住的地方（诚实记录）

想让玩家提交**非空函数**，试了三种调用组合，都不生效——协议里我们的
`FunctionUpdateRequest` 始终是 `{"player_id":229,"function":""}`：

```gdscript
fi.on_text_change("0")
fi.on_text_submitted("0")
# 以及
fi.on_text_change("0"); fi.force_update_cb(); fi.on_text_submitted("0")
```

节点内部**没有** `LineEdit`/`TextEdit` 子节点（`find_children` 返回 0），
说明文字是 Rust 自绘、状态在 Rust 内部；`on_text_change` 大概是"输入框内容变了"
的回调，而不是"设置内容"的接口。

⇒ 未找到"程序化设置玩家函数"的入口。要继续得逆向 `GraphwarFunctionInput`
的内部状态（或模拟真实按键事件到它的输入处理）。

## 3. 附带收获：AI 提交的函数形态（观测）

对局里 Bot（`player_id=232`）提交的函数样本：

```
{"FunctionUpdateRequest":{"player_id":232,"function":"sin(x)"}}
{"FunctionUpdateRequest":{"player_id":232,"function":"0.00"}}
{"FunctionUpdateRequest":{"player_id":232,"function":"(-1.06*x)"}}
{"FunctionUpdateRequest":{"player_id":232,"function":"(-23.81*x)"}}
```

**多数是过原点的线性函数**（`k*x`），系数带两位小数。这与"瞄准某个目标"的假设一致：
过原点意味着 `f(0)=0`，加上修正 `c = y0 - f(x0)` 后，直线会穿过射手自身位置，
只需再选一个斜率即可扫过目标。斜率的取值规则（是否由目标位置算出）**尚未验证**。

## 4. 下一步

1. 用同一时刻的 `PosInfo` + Bot 的 `k*x` 做相关性分析，验证"斜率 = 指向目标的直线"；
2. 若能验证，就直接得到 `computer_player_manager` 的瞄准规则，可写进复刻；
3. 死亡样本仍缺——要么找到函数注入入口，要么构造一次必定命中的 Bot 对局。
