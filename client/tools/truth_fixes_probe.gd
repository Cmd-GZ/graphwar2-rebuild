extends SceneTree
## T42（真值修复）的**取证脚本**：证明两处 `.gd` 自造值已经换回 Rust 侧的真值、
## 且函数输入/键盘的编辑规则确实走 Rust（`src/_scaffold/function_input_rules_class.rs`）。
##
## ```text
## # 前置：cargo build --release --features gdext-bindings
## #       cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
## #       godot --headless --path client --import
## godot --headless --path client --script res://tools/truth_fixes_probe.gd
## # 期望：T42PROBE 23 passed, 0 failed
## ```
##
## 断言三组（每组都指到出处，见 `docs/06` §92 / `docs/16` §81）：
## ① `GwNetLink.connect_timeout` = **30 s**（Rust `CONNECT_TIMEOUT_SECS`，arm64 `0x48dccc mov w1,#0x1e`）
## ② `GwLanDiscovery.room_ttl_ms` = **10000 ms**（Rust `ENTRY_TTL_SECS`，arm64 `0x3de6e4 cmp x0,#0xa`）
## ③ `GwFunctionEdit` 的编辑/按键规则（`Buffer` `0x4f3210`/`0x4f44c0` + 折叠跳转表 `0x1c3d5c`），
##    并逐条核对改前 `selftest.gd` 那 9 条键盘断言的行为没变。
##
## ⚠️ 本脚本**不在** `docs/06` §49.6 那 14 个 `*_check.gd` 的 headless 门槛清单里
## （T42 的门槛读数要求"14 脚本 / 1935"，加进去会改口径），所以单列成 `*_probe.gd`。

var _checks := 0
var _fail := 0

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_fail += 1
		print("FAIL %s" % what)

func _initialize() -> void:
	print("== GwNetConsts / GwFunctionEdit 在 ClassDB 里吗 ==")
	ok(ClassDB.class_exists("GwNetConsts"), "ClassDB 有 GwNetConsts")
	ok(ClassDB.class_exists("GwFunctionEdit"), "ClassDB 有 GwFunctionEdit")

	if ClassDB.class_exists("GwNetConsts"):
		var p: Dictionary = ClassDB.instantiate("GwNetConsts").call("provenance")
		print("     provenance = ", p)
		ok(absf(float(ClassDB.instantiate("GwNetConsts").call("connect_timeout_secs")) - 30.0) < 0.0001,
			"connect_timeout_secs = 30（实际 %s）" % str(ClassDB.instantiate("GwNetConsts").call("connect_timeout_secs")))
		ok(int(ClassDB.instantiate("GwNetConsts").call("entry_ttl_ms")) == 10000,
			"entry_ttl_ms = 10000（实际 %s）" % str(ClassDB.instantiate("GwNetConsts").call("entry_ttl_ms")))
		ok(absf(float(ClassDB.instantiate("GwNetConsts").call("broadcast_period_secs")) - 1.0) < 0.0001,
			"broadcast_period_secs = 1")

	print("== ① GwNetLink.connect_timeout 从 Rust 取 ==")
	var link := GwNetLink.new()
	print("     link.connect_timeout = ", link.connect_timeout, " s")
	ok(absf(link.connect_timeout - 30.0) < 0.0001, "GwNetLink.connect_timeout == 30 s（不再是 3 s）")

	print("== ② GwLanDiscovery.room_ttl_ms 从 Rust 取 ==")
	var lan := GwLanDiscovery.new()
	print("     lan.room_ttl_ms = ", lan.room_ttl_ms, " ms")
	ok(lan.room_ttl_ms == 10000, "GwLanDiscovery.room_ttl_ms == 10000 ms（不再是 12000 ms）")


	print("== ③ 编辑规则（GwFunctionEdit.apply_command ← Rust Buffer） ==")
	if ClassDB.class_exists("GwFunctionEdit"):
		var c = ClassDB.instantiate("GwFunctionEdit")
		var r: Dictionary = c.call("apply_command", "sin(x)", 4, "(", 128)
		ok(String(r["text"]) == "sin((x)" and int(r["caret"]) == 5, "插入 '(' @4 ⇒ sin((x), caret 5（实际 %s / %d）" % [r["text"], r["caret"]])
		r = c.call("apply_command", "sin(x)", 4, GwCommands.KEY_BACKSPACE, 128)
		ok(String(r["text"]) == "sinx)" and int(r["caret"]) == 3, "⌫ @4 ⇒ sinx), caret 3（实际 %s / %d）" % [r["text"], r["caret"]])
		r = c.call("apply_command", "sin(x)", 4, GwCommands.KEY_DELETE, 128)
		ok(String(r["text"]) == "sin()" and int(r["caret"]) == 4, "⌦ @4 ⇒ sin(), caret 4（实际 %s / %d）" % [r["text"], r["caret"]])
		r = c.call("apply_command", "sin(x)", 4, "C", 128)
		ok(String(r["text"]) == "" and int(r["caret"]) == 0, "C ⇒ 清空")
		r = c.call("apply_command", "abc", 3, "d", 3)
		ok(bool(r["rejected"]) and String(r["text"]) == "abc", "长度上限 3 ⇒ rejected 且文本不动")
		# 原版 28 条死键折叠（改前 .gd 完全没有这条）
		ok(String(c.call("key_text", 0x00E2, 0)) == "a", "â（0xE2）⇒ 折叠成 a（原版跳转表 0x1c3d5c）")
		ok(String(c.call("key_text", 0x00B2, 0)) == "2", "²（0xB2）⇒ 折叠成 2")
		ok(String(c.call("key_text", 0x0177, 0)) == "y", "ŷ（0x177）⇒ 折叠成 y")
		ok(int(c.call("provenance").get("composed_fold_count", 0)) == 28, "折叠表 28 条")

	print("== ③' GwKeyInput 行为与改前逐条相同（selftest 的 9 条断言口径） ==")
	var ev := InputEventKey.new()
	ev.keycode = KEY_S
	ev.physical_keycode = KEY_S
	ev.pressed = true
	ev.unicode = KEY_S
	ok(GwKeyInput.command_for(ev) == "s", "S ⇒ s（实际 '%s'）" % GwKeyInput.command_for(ev))
	var ev9 := InputEventKey.new()
	ev9.keycode = KEY_9
	ev9.pressed = true
	ev9.unicode = 40
	ok(GwKeyInput.command_for(ev9) == "(", "Shift+9 ⇒ (")
	var evp := InputEventKey.new()
	evp.keycode = KEY_P
	evp.pressed = true
	evp.unicode = 0x3C0
	ok(GwKeyInput.command_for(evp) == "π", "π ⇒ π")
	var evb := InputEventKey.new()
	evb.keycode = KEY_BACKSPACE
	evb.pressed = true
	ok(GwKeyInput.command_for(evb) == GwCommands.KEY_BACKSPACE, "退格 ⇒ ⌫")
	var evs := InputEventKey.new()
	evs.keycode = KEY_SPACE
	evs.pressed = true
	evs.unicode = 32
	ok(GwKeyInput.command_for(evs) == "", "空格不插入")
	var eve := InputEventKey.new()
	eve.keycode = KEY_ENTER
	eve.pressed = true
	ok(GwKeyInput.command_for(eve) == GwKeyInput.SUBMIT, "回车 ⇒ SUBMIT")
	# 新能力：死键合成字符（改前被 .gd 的 PRINTABLE 白名单丢掉）
	var eva := InputEventKey.new()
	eva.keycode = KEY_A
	eva.pressed = true
	eva.unicode = 0x00E2
	ok(GwKeyInput.command_for(eva) == "a", "â ⇒ a（改前会返回 ''，这是本轮升上来的保真度；实际 '%s'）" % GwKeyInput.command_for(eva))

	print("T42PROBE %d passed, %d failed" % [_checks - _fail, _fail])
	quit(1 if _fail > 0 else 0)
