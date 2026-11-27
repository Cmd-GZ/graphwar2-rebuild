extends RefCounted
class_name GwNetFrame
## 线上分帧与消息解析 —— **帧层规则在 Rust**，本文件只留"JSON ↔ Dictionary"与拼原文。
##
## 传输层（**观测**，`docs/24` §1）：`u16` **大端**长度 + JSON 负载，长度只算负载。
## 实测量到的帧头 `0x0031` = 49 字节，后面正好 49 字节 JSON。
##
## 消息形状（**观测**，`docs/24` §2）：全部是**单键对象** `{"TypeName":{字段…}}`；
## 字段只观测到字符串、整数、浮点、布尔、`null` 与嵌套小对象（`color` 的 `{r,g,b}`）。
## **没有观测到数组**。`docs/96` §4 另证：线上走的是 serde 的**外部标签**枚举
## （变体名就是键），**没有数值判别值** —— 所以补一条消息只需要名字 + 字段。
##
## **帧层在 Rust**（`src/graphwar_stream_buffer.rs`：`read_event` arm64 `0x45c51c` /
## `write_event` arm64 `0x45c818`，逐位），一律经 `GwStreamBuffer` 问。本文件留的是
## "Godot JSON ↔ Dictionary"与报文原文拼装（原版由 serde derive 生成，属 B 类）。
## 逐段判定见 `fixtures/controller_migration2_ledger.tsv` F01–F10。
##
## ⚠️ 改前**漏了一条真值**：读侧单帧上限 `0x1000`=4096（arm64 `0x45c590`，超了报
## `Message too large`）；原来只挡写侧 `0xffff`。已补（见 `decode_buffer` 的 `error`）。

# ---------------------------------------------------------------- 编码（规则在 Rust）

## 一帧 = `[hi, lo] + UTF-8 负载`。负载超过 `u16` 时返回空 `PackedByteArray`
## （调用方必须把"空"当成错误 —— 原版在这里报 `Message too long.` 而不是截断）。
static func encode(payload: String) -> PackedByteArray:
	return GwStreamBuffer.encode_frame(payload)

## 把若干条消息编码成一段字节流（等价于 `src/server.rs::to_wire`）。
static func encode_many(payloads: Array) -> PackedByteArray:
	var parts := PackedStringArray()
	for p in payloads:
		parts.append(String(p))
	return GwStreamBuffer.encode_frames(parts)

# 单帧上限**不在这里写常量**：写侧 `0xffff` = `GwStreamBuffer.max_payload()`、
# 读侧 `0x1000` = `GwStreamBuffer.max_read_len()`（逐位，见文件头）。

## 构造一条消息原文：`{"Kind":<body_json>}`，`body_json` 必须是**已写好的 JSON 正文**。
## 不让调用方传 `Dictionary` 再序列化的理由：`JSON.stringify` 会重排/加空格，而线上字节
## 要能逐字节对拍（`docs/96` §7）⇒ 像 `src/_scaffold/protocol.rs` 一样拼原文。
static func message(kind: String, body_json: String) -> String:
	return '{"%s":%s}' % [kind, body_json]

## JSON 字符串字面量（带引号）。函数原文里有 `"`/`\` 时不能裸拼。
static func json_string(s: String) -> String:
	var out := "\""
	for i in s.length():
		var c := s[i]
		match c:
			"\"": out += "\\\""
			"\\": out += "\\\\"
			"\n": out += "\\n"
			"\r": out += "\\r"
			"\t": out += "\\t"
			_: out += c
	return out + "\""

# ---------------------------------------------------------------- 切帧（规则在 Rust）

## 从缓冲区头部尽量多地切出帧（判据全在 `GwStreamBuffer.decode_one`）。
##
## 返回 `{"frames", "rest", "bad_utf8", "error"}`：
## * `frames` —— 完整帧（按到达顺序）；`rest` —— **没切走**的尾部字节（半帧留着）；
## * `bad_utf8` —— 负载不是合法 UTF-8（原版 `Frame::decode` 给 `None` ⇒ 当"对端关了"）；
## * `error` —— 其它**真错误**原文（读侧 4096 上限 ⇒ `Message too large`）。非空 ⇒ 这帧永远
##   切不出来，连接层必须断开（否则 `rest` 无限增长）。
static func decode_buffer(buf: PackedByteArray) -> Dictionary:
	var frames: Array[String] = []
	var pos := 0
	var bad := false
	var error := ""
	while pos < buf.size():
		var r := GwStreamBuffer.decode_one(buf, pos)
		var status := String(r.get("status", "failed"))
		if status == "event":
			frames.append(String(r.get("frame", "")))
			pos += int(r.get("consumed", 0))
			continue
		if status == "failed":
			if String(r.get("reason", "")) == "bad_utf8":
				bad = true
			else:
				var msg := String(r.get("message", ""))
				error = msg if msg != "" else String(r.get("reason", ""))
		break
	return {"frames": frames, "rest": buf.slice(pos), "bad_utf8": bad, "error": error}

# ---------------------------------------------------------------- 解消息

## 解一帧：`{"kind", "body", "text"}`，解不出返回 `{}`。判据照 `_scaffold/protocol.rs::parse_message`：
## 外层**恰好一个**键的对象、它的值也是对象（`ClearObstacles` 的空体 `{}` 也算）。
## 留在 .gd 的理由见台账 F09：入参已是引擎解析出的文本、返回值是 `Dictionary`，
## 真正的"事件 → 结构体"反序列化在原版是 serde（arm64 `0x4691a4`）。
static func parse_message(text: String) -> Dictionary:
	var t := text.strip_edges()
	if t == "" or not t.begins_with("{"):
		return {}
	var parsed = JSON.parse_string(t)
	if typeof(parsed) != TYPE_DICTIONARY:
		return {}
	var d: Dictionary = parsed
	if d.size() != 1:
		return {}
	var kind := String(d.keys()[0])
	var body = d[kind]
	if typeof(body) != TYPE_DICTIONARY:
		return {}
	return {"kind": kind, "body": body, "text": t}

# ---------------------------------------------------------------- 取字段（Variant 转换，留 .gd）
#
# 原版是**文本查找**（`_scaffold/protocol.rs::get_str/get_int`），这里用解析后的字典；
# 唯一差异：Godot 把数字都读成 float，所以 `as_int` 显式取整。台账 F10。

static func has(msg: Dictionary, key: String) -> bool:
	var body: Dictionary = msg.get("body", {})
	return body.has(key)

static func as_str(msg: Dictionary, key: String, def := "") -> String:
	return body_str(msg.get("body", {}), key, def)

static func as_int(msg: Dictionary, key: String, def := 0) -> int:
	return body_int(msg.get("body", {}), key, def)

static func as_float(msg: Dictionary, key: String, def := 0.0) -> float:
	return body_float(msg.get("body", {}), key, def)

## 布尔：`null` 与缺失都返回 `def`（`RankInfo.rank` 观测到 `null`，`locked` 是布尔）。
static func as_bool(msg: Dictionary, key: String, def := false) -> bool:
	return body_bool(msg.get("body", {}), key, def)

## 嵌套对象（唯一观测到的用法：`GameInfo` 的 `{"game_info":{…}}`、`color` 的 `{r,g,b}`）。
static func as_dict(msg: Dictionary, key: String) -> Dictionary:
	return body_dict(msg.get("body", {}), key)

# --- 直接对"消息体"取字段（会话层手里只有 body 时用这一组）---------------

static func body_str(body: Dictionary, key: String, def := "") -> String:
	var v = body.get(key, null)
	if v == null:
		return def
	if typeof(v) == TYPE_STRING:
		return String(v)
	return str(v)

static func body_int(body: Dictionary, key: String, def := 0) -> int:
	var v = body.get(key, null)
	if v == null:
		return def
	if typeof(v) == TYPE_STRING:
		return int(String(v)) if String(v).is_valid_int() else def
	return int(round(float(v)))

static func body_float(body: Dictionary, key: String, def := 0.0) -> float:
	var v = body.get(key, null)
	if v == null:
		return def
	if typeof(v) == TYPE_STRING:
		return float(String(v)) if String(v).is_valid_float() else def
	return float(v)

static func body_bool(body: Dictionary, key: String, def := false) -> bool:
	var v = body.get(key, null)
	if typeof(v) == TYPE_BOOL:
		return bool(v)
	return def

static func body_dict(body: Dictionary, key: String) -> Dictionary:
	var v = body.get(key, null)
	if typeof(v) == TYPE_DICTIONARY:
		return v
	return {}
