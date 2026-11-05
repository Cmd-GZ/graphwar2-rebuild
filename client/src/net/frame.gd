extends RefCounted
class_name GwNetFrame
## 线上分帧与消息解析 —— `src/protocol.rs` 的 GDScript 对应物。
##
## 传输层（**观测**，`docs/24` §1）：`u16` **大端**长度 + JSON 负载，长度只算负载。
## 实测量到的帧头 `0x0031` = 49 字节，后面正好 49 字节 JSON。
##
## 消息形状（**观测**，`docs/24` §2）：全部是**单键对象** `{"TypeName":{字段…}}`；
## 字段只观测到字符串、整数、浮点、布尔、`null` 与嵌套小对象（`color` 的 `{r,g,b}`）。
## **没有观测到数组**。`docs/96` §4 另证：线上走的是 serde 的**外部标签**枚举
## （变体名就是键），**没有数值判别值** —— 所以补一条消息只需要名字 + 字段。
##
## 本文件只做三件事：编码一帧、从缓冲区切帧、把一帧解成 `{kind, body, text}`。
## 业务语义（状态机、房间表）在 `link.gd` / `session.gd` / `discovery.gd` / `broker.gd`。

## `u16` 长度字段的上限。原版同一个写入函数 `0x3d89e0` 里有 `cmp $0xffff`，
## 超了记 `Failed to write to tcp_stream. Message too long.`（`docs/115` §1）。
const MAX_PAYLOAD := 65535

# ---------------------------------------------------------------- 编码

## 一帧 = `[hi, lo] + UTF-8 负载`。负载超过 `u16` 时返回空 `PackedByteArray`
## （调用方必须把"空"当成错误 —— 原版在这里报 Message too long 而不是截断）。
static func encode(payload: String) -> PackedByteArray:
	var body := payload.to_utf8_buffer()
	var n := body.size()
	if n > MAX_PAYLOAD:
		return PackedByteArray()
	var out := PackedByteArray()
	out.append((n >> 8) & 0xFF)
	out.append(n & 0xFF)
	out.append_array(body)
	return out

## 把若干条消息编码成一段字节流（等价于 `src/server.rs::to_wire`）。
static func encode_many(payloads: Array) -> PackedByteArray:
	var out := PackedByteArray()
	for p in payloads:
		out.append_array(encode(String(p)))
	return out

## 构造一条消息原文：`{"Kind":<body_json>}`。`body_json` 必须是**已经写好的 JSON 正文**。
##
## 为什么不让调用方传 `Dictionary` 再序列化：`JSON.stringify` 会重排/加空格，
## 而线上字节要能逐字节对拍（`docs/96` §7 的"逐位验证"档）。所以我们像
## `src/protocol.rs` 一样**拼原文**，不做"解析再序列化"。
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

# ---------------------------------------------------------------- 切帧

## 从缓冲区头部尽量多地切出帧。
##
## 返回 `{"frames": Array[String], "rest": PackedByteArray, "bad_utf8": bool}`：
## * `frames` —— 已经完整的帧（按到达顺序）；
## * `rest` —— **没切走**的尾部字节（半帧必须留着，下一批数据接上）；
## * `bad_utf8` —— 某帧负载不是合法 UTF-8。原版 `Frame::decode` 在这里返回 `None`，
##   调用方（`read_frame`）当成"对端关了"处理，所以这里也只报一个标志位，由连接层断开。
static func decode_buffer(buf: PackedByteArray) -> Dictionary:
	var frames: Array[String] = []
	var pos := 0
	var bad := false
	while buf.size() - pos >= 2:
		var n := (int(buf[pos]) << 8) | int(buf[pos + 1])
		if buf.size() - pos - 2 < n:
			break
		var payload := buf.slice(pos + 2, pos + 2 + n)
		pos += 2 + n
		var text := payload.get_string_from_utf8()
		if text.to_utf8_buffer() != payload:
			bad = true
			break
		frames.append(text)
	return {"frames": frames, "rest": buf.slice(pos), "bad_utf8": bad}

# ---------------------------------------------------------------- 解消息

## 解一帧：返回 `{"kind": String, "body": Dictionary, "text": String}`，解不出返回 `{}`。
##
## 判据照 `src/protocol.rs::parse_message`：外层必须是**恰好一个**键的对象，
## 它的值必须也是对象（`ClearObstacles` 的空体 `{}` 也算对象）。花括号配平由 JSON
## 解析器负责 —— 半截帧在切帧阶段就已经被挡掉了。
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

# ---------------------------------------------------------------- 取字段
#
# 原版是**文本查找**（`src/protocol.rs::get_str/get_int`），这里用 JSON 解析后的字典。
# 差异只有一个：Godot 的 JSON 把所有数字都读成 float，所以 `as_int` 要显式取整。

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
