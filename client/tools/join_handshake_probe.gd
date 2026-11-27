extends SceneTree
## T84 诊断脚本（**不进 14 脚本 headless 门槛**，跑一次约 2 s）：确定性复现/回归
## `GwNetSession` 的"什么时候可以进对局屏"这件事。
##
## 由来（`docs/06` §131.2）：T84 之前 `client/src/net/session.gd` 里有一条**我方自造**的兜底
## `_can_join_by_idle()`：见过 `NewConnection` 之后再"静默" 0.30 s 就 `_mark_joined()` ——
## 即使 7 条状态同步只到 1 条。于是 `App` 会提前进 PlayScreen，`_sync_state_from_net()` 用的是
## **空 room**：`function_mode_index("")=1`（DiffEq ⇒ 底栏画 `dy/dx`）、`time_mode_index("")=1`
## （Timer1m ⇒ 底栏画 `00:5x` 倒计时）、实体表空（棋盘空）。真帧证据见 `docs/06` §131.2.1。
##
## 这条兜底会不会踩中取决于"某一次 `poll()` 有没有把服务端整批 47 帧读完"，是**跨轮会飘的时序**
## ⇒ 像素对拍只能证明"某一轮撞上了/没撞上"，证不了机制。本脚本把那个瞬间直接摆出来：
##
##   改前（`0631ff3`）：`A after_poll phase=joined sync=1 joined=true`   ← 1/7 条就进屏
##   改后（T84）     ：`A after_poll phase=handshaking sync=1 joined=false` ← 不进屏，等 7 条
##
## 复跑：`$GODOT --headless --path client --script res://tools/join_handshake_probe.gd`
## 判据：`A after_poll` 必须是 `handshaking`；`B after_all` 必须是 `joined` 且 7/7。

func _init() -> void:
	var sync: Array = GwWireSamples.STATE_SYNC
	print("@@PROBE STATE_SYNC 条数 = %d" % sync.size())

	# ---- 场景 A：只收到第 1 条，然后过一帧很长的 delta（llvmpipe 下的常态 0.4 s）----
	var a := GwNetSession.new()
	a.phase = "handshaking"
	a._on_frame(str(sync[0]))
	print("@@PROBE A after1 phase=%s sync=%d" % [a.phase, a.state_sync_progress()])
	a.poll(0.4)
	print("@@PROBE A after_poll phase=%s sync=%d joined=%s" % [
		a.phase, a.state_sync_progress(), str(a.is_joined())])
	print("@@PROBE A room: function_mode=%s time_mode=%s players=%d entities=%d" % [
		str(a.room.get("function_mode", "<unset>")), str(a.room.get("time_mode", "<unset>")),
		(a.room.get("players", {}) as Dictionary).size(),
		(a.room.get("entities", {}) as Dictionary).size()])
	print("@@PROBE A degenerate: function_mode_index(\"\")=%d time_mode_index(\"\")=%d" % [
		GwNetSession.function_mode_index(""), GwNetSession.time_mode_index("")])

	# ---- 场景 B：同样先只到 1 条，但紧接着把剩下 6 条喂完（正常快网会走这条路）----
	var b := GwNetSession.new()
	b.phase = "handshaking"
	b._on_frame(str(sync[0]))
	for i in range(1, sync.size()):
		b._on_frame(str(sync[i]))
	print("@@PROBE B after_all phase=%s sync=%d joined=%s function_mode=%s time_mode=%s" % [
		b.phase, b.state_sync_progress(), str(b.is_joined()),
		str(b.room.get("function_mode", "<unset>")), str(b.room.get("time_mode", "<unset>"))])
	quit(0)
