extends RefCounted
class_name GwReplayFile
## 回放文件的客户端读取（`docs/133`）。
##
## ## 格式（**观测**，`docs/78` §2）
##
## gzip 压缩 + 每行一个 JSON 事件：
##
## ```text
## {"event":{"FunctionInfo":{"owner_id":635,"function":"x","corrected_function":null}},"tick":1234}
## ```
##
## 内层 `{"<EventName>":{…}}` 与网络协议同构（`src/events.rs` 的 72 个名字），
## 所以**事件名就是字典的键**，字段名就是协议字段名。协议里没有的字段**原样保留在
## `data` 里**（`GwReplayFile.events[i].data` 是整个载荷字典），不丢键、不猜键名。
##
## ## 解压为什么在客户端做
##
## 内核侧（`src/replay.rs`）有同一份规则的可执行版本（`cargo test` 覆盖，另有 C ABI
## `gw2_replay_state_at` 供 ctypes 对拍）。客户端读文件走 Godot 自带的
## `PackedByteArray.decompress(..., FileAccess.COMPRESSION_GZIP)`，**不重复实现 DEFLATE**。
##
## ⚠️ 踩过的坑（`docs/133` §5.4）：`FileAccess.open_compressed()` **不能**读裸 gzip —— 它读的是
## Godot 自己的 `GDPC` 容器（魔数 `0x43454447`），对真回放一律返回
## `ERR_FILE_UNRECOGNIZED` / "File unrecognized"。能读裸 gzip 的只有
## `PackedByteArray.decompress()`（Godot 的 gzip 解压走 zlib `windowBits=15+16`）。
## 两份实现的一致性由 `docs/133` §5 的三方对拍保证（Python 参考 / Rust 内核 / 本文件）。
##
## ## 已知的脏数据（**观测**）
##
## 400 份真实回放里：
## * 2 份是**空回放**（gzip 有效、正文 0 字节）⇒ `events` 为空，`ok()` 为 false；
## * 7 份 tick 有回退（最多 2 次、单次差 ≤ 19 tick）⇒ 这里**不排序**，见
##   `GwReplayPlayer` 的游标模型；
## * 解析不了的行**跳过并计数**（`skipped_lines`），与 `tools/parse_replay.py` 的
##   `continue` 同口径 —— 一行坏掉不该让整局播不出来。

## 回放的默认目录（Godot 4 里 `user://` = `$XDG_DATA_HOME/godot/app_userdata/<项目名>/`）。
const DIR_DEFAULT := "user://replays"

## 一局回放。
var path := ""
var gzipped := false
## 按**文件顺序**（不排序）的事件：每项 `{tick:int, kind:String, data:Dictionary}`。
var events: Array[Dictionary] = []
var skipped_lines := 0
var error := ""
## 全局实体表：`player_id → 名字`（`NameInfo.entity_id` 就是 `player_id`，`docs/79` §1）。
var names := {}
## 全局实体表：`soldier_id → player_id`（`FunctionInfo.owner_id` 是**士兵**号）。
var soldier_owner := {}
## `TargetInfo.target_id → true`（靶子）。
var targets := {}

## 读一个文件。失败时返回的对象 `ok()` 为 false，`error` 写明原因（不抛异常）。
static func load_path(p: String) -> GwReplayFile:
	var out := GwReplayFile.new()
	out.path = p
	if not FileAccess.file_exists(p):
		out.error = "文件不存在：%s" % p
		return out
	var text := ""
	var head := FileAccess.get_file_as_bytes(p)
	if head.is_empty():
		out.error = "空文件"
		return out
	if head.size() >= 3 and head[0] == 0x1f and head[1] == 0x8b:
		out.gzipped = true
		var plain := _gunzip(head)
		if plain.is_empty():
			out.error = "gzip 解压失败（%d 字节）" % head.size()
			return out
		text = plain.get_string_from_utf8()
	else:
		# 明文回放（部分工具会写成未压缩）也吃：与 tools/parse_replay.py 一致。
		text = head.get_string_from_utf8()
	return out._parse_text(text)

## 解 gzip（裸 gzip，**不是** Godot 的 `GDPC` 容器，见文件头的坑）。
##
## 分配尺寸用 gzip 尾部 4 字节 `ISIZE`（RFC 1952 §2.3.1：未压缩大小 mod 2^32）——
## 精确，不用猜。多 member 的拼接回放（真实语料里没见到）ISIZE 只算最后一个 member，
## 所以解不出来时按 4×/16× 再试，都不行才算失败。
static func _gunzip(bytes: PackedByteArray) -> PackedByteArray:
	var n := bytes.size()
	if n < 18:
		return PackedByteArray()
	var hint := int(bytes[n - 4]) | (int(bytes[n - 3]) << 8) \
		| (int(bytes[n - 2]) << 16) | (int(bytes[n - 1]) << 24)
	for factor in [1, 4, 16]:
		var size := maxi(hint * factor, 1 << 16)
		var plain := bytes.decompress(size, FileAccess.COMPRESSION_GZIP)
		if plain.size() > 0:
			return plain
	return PackedByteArray()

## 直接吃一段明文 JSONL（自检用；等价于 `load_path` 的明文分支）。
static func parse_text(text: String) -> GwReplayFile:
	return GwReplayFile.new()._parse_text(text)

func _parse_text(text: String) -> GwReplayFile:
	for raw in text.split("\n"):
		var line := raw.strip_edges()
		if line.is_empty():
			continue
		var parsed = JSON.parse_string(line)
		if typeof(parsed) != TYPE_DICTIONARY:
			skipped_lines += 1
			continue
		var tick_v = parsed.get("tick")
		if typeof(tick_v) != TYPE_FLOAT and typeof(tick_v) != TYPE_INT:
			skipped_lines += 1
			continue
		var body = parsed.get("event")
		if typeof(body) != TYPE_DICTIONARY or (body as Dictionary).is_empty():
			skipped_lines += 1
			continue
		# 外层事件的单键对象：`{"FunctionInfo":{…}}` ⇒ 键就是事件名
		var kind := String((body as Dictionary).keys()[0])
		var data = (body as Dictionary)[kind]
		if typeof(data) != TYPE_DICTIONARY:
			data = {}
		events.append({"tick": int(tick_v), "kind": kind, "data": data})
	_collect_globals()
	if events.is_empty():
		error = "没有可播放的事件（空回放？）"
	return self

## 两张全局表 + 靶子集合，在**整份文件**上收集（`docs/79` §1 的链路）。
##
## 与 `tools/replay_state.py` 的预扫描同口径：名字/士兵归属是静态事实，
## 不该随 tick 变化；位置/队伍/生死才是逐 tick 的。
func _collect_globals() -> void:
	for ev in events:
		var data: Dictionary = ev.data
		match String(ev.kind):
			"NameInfo":
				if data.has("entity_id") and data.has("name"):
					names[int(data.entity_id)] = String(data.name)
			"SoldierInfo":
				if data.has("soldier_id") and data.has("player_id"):
					soldier_owner[int(data.soldier_id)] = int(data.player_id)
			"TargetInfo":
				if data.has("target_id"):
					targets[int(data.target_id)] = true

func ok() -> bool:
	return error == "" and not events.is_empty()

## `[第一条, 最后一条]` 事件的 tick（按文件顺序取极值，不排序）。
func tick_range() -> Vector2i:
	if events.is_empty():
		return Vector2i.ZERO
	var lo: int = int(events[0].tick)
	var hi: int = lo
	for ev in events:
		var t := int(ev.tick)
		lo = mini(lo, t)
		hi = maxi(hi, t)
	return Vector2i(lo, hi)

## 事件名 → 条数（诊断用）。
func kind_counts() -> Dictionary:
	var out := {}
	for ev in events:
		var k := String(ev.kind)
		out[k] = int(out.get(k, 0)) + 1
	return out

# ---------------------------------------------------------------- 目录列举

## 列出 `dir` 里的回放文件（**不按扩展名过滤**：真实回放的文件名是纯 unix 时间戳，
## 没有扩展名，`docs/78` §1）。返回每项 `{title, size, mtime, path}`，新的在前。
static func list_dir(dir: String) -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	var d := DirAccess.open(dir)
	if d == null:
		return out
	var files := d.get_files()
	files.sort()
	files.reverse()
	for f in files:
		var full := dir.path_join(f)
		out.append({
			"title": f,
			"size": int(FileAccess.get_file_as_bytes(full).size()),
			"mtime": int(FileAccess.get_modified_time(full)),
			"path": full,
		})
	return out
