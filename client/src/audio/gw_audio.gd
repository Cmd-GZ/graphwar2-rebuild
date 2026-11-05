extends Node
class_name GwAudio
## 音频播放层 —— 原版 `graphwar_audio_control.rs`（节点类 `SoundHandler`）的最小复刻。
##
## 原版事实（`docs/111`，逐条带地址）与本文的对应
## ===============================================
## | 原版 | 地址/值 | 本文 |
## |---|---|---|
## | 5 个 mp3 走 Godot `load()`，`set_stream` + `play()` | `0x4bf550`（5 次 `load`）、`0x4c01b9/0x4c01d8` | `SOUNDS` 里 `kind="mp3"`，`_ensure_player()` + `play()` |
## | `humm.wav` 由 Rust `FileAccess::get_file_as_bytes` 读、hound 解析 | `0x3eaa00` @`0x4bf681`、`0x50a080` @`0x4bf6c3` | `start_humm()`：`FileAccess.get_file_as_bytes` + 本文的 RIFF 解析 |
## | `AudioStreamGenerator` 16000 Hz / 0.1 s + `push_buffer` | `0x4c016a`/`0x4c018f`/`0x4c0a5d` | `GEN_MIX_RATE`/`GEN_BUFFER_LENGTH`/`_push_generator_frames()` |
## | PCM 归一化 `32767.0f` | `0x4c08f6` | `PCM_NORMALIZE` |
## | 四个 player 臂 `-15.0f` | `0x1bb818`/`0x1b93f4`…（`docs/111` §4.5 表） | `SLOT_VOLUME_DB["0x80"/"0x90"/"0xa0"]` |
## | 第五个槽 `-5.0f`（humm 那一路） | `0x1b9ba0` @`0x4bf4cf`（函数 `0x4bf480`） | `SLOT_VOLUME_DB["0xc0"]` |
## | `0xb0` 槽没设音量（Godot 默认 `0 dB`） | `0x4bf39e` 一支 | `SLOT_VOLUME_DB["0xb0"]` |
## | **静音 = `AudioServer.set_bus_mute(get_bus_index("Master"), …)`** | `0x4c13a4` / `0x4c13c7` | `set_muted()` —— **绝不**逐 player 调 `volume_db` |
##
## 设计边界（有意为之）
## ==================
## * **播放层只认"名字"**，不认游戏事件。事件↔音效的映射在原版里**仍未确定**
##   （`docs/111` §5.1；本轮的补充证据见 `docs/128` §3：`0x4bf090` 是
##   `effect_info_event`（tag 44）的处理函数，按 `event[+0x20]`（= `effect_type`）四臂分派，
##   但**哪个 effect_type 落哪个音效仍然没有证据**）。
##   ⇒ 本层不硬编码任何"事件→音效"关系；`App` 那边的几条接线在 `docs/128` §4 里
##   逐条标了"**自定/推断，非原版**"。
## * **缺资产不崩**：加载失败只记进 `missing_sounds()`（并 `push_warning`），
##   `play()` 返回 false；`selftest` 用 `loaded()`/`missing()` 显式 SKIP 并报原因。
## * 节点只在需要时建 player（原版也是懒建：`0x4bf090`/`0x4bf340`/`0x4bf480` 里
##   `0xa0`/`0xb0`/`0xc0` 槽都是"取/建"合一的辅助函数）。

## 播放完成（给自检/证据用）：`play()` 真的把 stream 交给了一个 player 就发。
signal played(sound: String, slot: String)
## 静音状态变化（**两个方向**都发：我们自己切的、和外部直接改总线后被采纳的）。
signal mute_changed(muted: bool)

const SOUND_DIR := "res://assets/sounds"

## 资产目录（`GwAssets` 用同一个约定：可被 `GW2_ASSET_DIR` 覆盖）。
## 做成可变字段而不是只留常量，是为了让 selftest 能构造一个"目录不存在"的实例，
## 用来验证"缺资产 ⇒ 静默降级而不崩"（`docs/128` §6.2）。
var sound_dir := SOUND_DIR

## `docs/111` §4.5：音量是**逐 player 的静态常量**，不是玩家可调项。
const SLOT_VOLUME_DB := {
	"0x80": -15.0,   # match 臂 3（`0x4bf233` → `0x4bf244`）
	"0x90": -15.0,   # match 臂 4（`0x4bf279` → `0x4bf28a`）
	"0xa0": -15.0,   # match 臂 1 / 2（`0x4bf199`、`0x4bf1e6`）
	"0xb0": 0.0,     # 原版这一支**没调** `set_volume_db` ⇒ Godot 默认 0 dB（`0x4bf39e`）
	"0xc0": -5.0,    # 第五个槽（`0x4bf480`，`0x1b9ba0`）
}

## `docs/111` §4.4(b)：generator 的三个常数，全是立即数。
const GEN_MIX_RATE := 16000.0      # `0x1b93f4` = 16000.0f
const GEN_BUFFER_LENGTH := 0.1     # `0x1b9144`… 实为 `0x1bb144` = 0.1f
const PCM_NORMALIZE := 32767.0     # `0x1b9a58` = 32767.0f

## 音效表。`slot` 只是"默认落在哪个 player 槽"；`evidence` 写明这一行的依据，
## **没有一行声称"原版用它响应某个游戏事件"**（那条映射未知，见文件头）。
const SOUNDS := {
	"explosion": {
		"file": "explosion.mp3", "kind": "mp3", "slot": "0xa0",
		"volume_db": -15.0,
		"origin": "res://sounds/explosion.mp3",
		"evidence": "docs/111 §4.4(a) 第 1 个 load()；音量见 §4.5 的 -15.0f 四臂",
	},
	"squash": {
		"file": "squash.mp3", "kind": "mp3", "slot": "0x80",
		"volume_db": -15.0,
		"origin": "res://sounds/squash.mp3",
		"evidence": "docs/111 §4.4(a) 第 2 个 load()；音量见 §4.5",
	},
	"spark": {
		"file": "spark.mp3", "kind": "mp3", "slot": "0x90",
		"volume_db": -15.0,
		"origin": "res://sounds/spark.mp3",
		"evidence": "docs/111 §4.4(a) 第 3 个 load()；音量见 §4.5",
	},
	"pop": {
		"file": "pop.mp3", "kind": "mp3", "slot": "0xa0",
		"volume_db": -15.0,
		"origin": "res://sounds/pop.mp3",
		"evidence": "docs/111 §4.4(a) 第 4 个 load()；音量见 §4.5",
	},
	"unpop": {
		"file": "unpop.mp3", "kind": "mp3", "slot": "0x80",
		"volume_db": -15.0,
		"origin": "res://sounds/unpop.mp3",
		"evidence": "docs/111 §4.4(a) 第 5 个 load()；音量见 §4.5",
	},
	"tear": {
		"file": "tear.mp3", "kind": "mp3", "slot": "0x90",
		"volume_db": -15.0,
		"origin": "res://sounds/tear.mp3",
		"evidence": "**原版没有加载它**（docs/111 §5.2 的负结果，全二进制无 res://sounds/tear.mp3）；"
			+ "这里按「可用但未接线」提供，槽位/音量是照 -15 dB 四臂取的，属**推断**",
	},
	"humm": {
		"file": "humm.wav", "kind": "generator", "slot": "0xc0",
		"volume_db": -5.0,
		"origin": "res://sounds/humm.wav",
		"evidence": "docs/111 §4.3/§4.4(b)：Rust 自己读 + hound 解析 + AudioStreamGenerator 16000Hz/0.1s；"
			+ "音量 -5.0f 见 §4.5 的第五个槽",
	},
	"humm_2": {
		"file": "humm_2.wav", "kind": "wav", "slot": "0xa0",
		"volume_db": -15.0,
		"origin": "res://sounds/humm_2.wav",
		"evidence": "**原版没有引用它**（docs/111 §5.2）；按 -15 dB 提供，槽位属**推断**",
	},
	"humm_b": {
		"file": "humm_b.wav", "kind": "wav", "slot": "0x80",
		"volume_db": -15.0,
		"origin": "res://sounds/humm_b.wav",
		"evidence": "**原版没有引用它**（docs/111 §5.2）；PCM 与 humm.wav 的 data 块逐字节相同",
	},
	"shepard_2": {
		"file": "shepard_2.wav", "kind": "wav", "slot": "0x90",
		"volume_db": -15.0,
		"origin": "res://sounds/shepard_2.wav",
		"evidence": "**原版没有引用它**（docs/111 §5.2，uid 在 105 个 .gd + 31 个 .tscn 里零引用）；"
			+ "16-bit 单声道 16000 Hz 20.009 s",
	},
}

## 加载失败的音效名（去重保序）。空 = 全部就绪。
var _missing: Array[String] = []
## 名字 → AudioStream（只装成功的）
var _streams := {}
## 槽位 → AudioStreamPlayer（懒建）
var _players := {}
## humm 的 PCM（Vector2 帧，已归一化）与播放游标
var _humm_frames := PackedVector2Array()
var _humm_cursor := 0
var _humm_rate := 0
var _humm_error := ""

## 内部认定的静音状态（与 `AudioServer` 的 Master 总线保持同步，见 `adopt_bus()`）
var _muted := false

func _ready() -> void:
	name = "Audio"
	_load_all()
	_muted = bus_muted()
	set_process(_humm_active())

# ------------------------------------------------------------------ 加载

func _load_all() -> void:
	for sound_name in SOUNDS:
		var spec: Dictionary = SOUNDS[sound_name]
		var path: String = sound_dir.path_join(String(spec.file))
		if not FileAccess.file_exists(path):
			_missing.append(sound_name)
			# **静默降级**：不 push_error（缺资产的常见原因是用户没提取），
			# 但一定要留下可查的痕迹 —— selftest 会把它变成显式 SKIP。
			print("[gw2/audio] 缺资产，跳过：%s（%s）" % [sound_name, path])
			continue
		if String(spec.kind) == "generator":
			# humm 那条路**不是** ResourceLoader 加载：原版自己读字节 + hound 解析。
			# 这里同样只缓存字节/PCM，真正建 generator 是在 start_humm()。
			_humm_frames = _decode_wav_frames(path)
			if _humm_frames.is_empty():
				_missing.append(sound_name)
				print("[gw2/audio] humm.wav 解析失败：%s" % _humm_error)
			continue
		var res := load(path)
		if res is AudioStream:
			_streams[sound_name] = res
		else:
			_missing.append(sound_name)
			print("[gw2/audio] 资产不是 AudioStream，跳过：%s（%s）" % [sound_name, path])

## 读一个 16-bit PCM WAV 的样本 → `Vector2` 帧（归一化 `÷32767`）。
##
## 原版这一步是 hound（`0x50a080`）做的；我们只支持原版实际会遇到的形状
## （PCM / 16-bit / 单声道 16000 Hz），其余一律判失败并报原因，不猜。
func _decode_wav_frames(path: String) -> PackedVector2Array:
	_humm_rate = 0
	var bytes := FileAccess.get_file_as_bytes(path)
	var frames := PackedVector2Array()
	if bytes.size() < 12 or bytes.slice(0, 4) != "RIFF".to_ascii_buffer() \
			or bytes.slice(8, 12) != "WAVE".to_ascii_buffer():
		_humm_error = "不是 RIFF/WAVE"
		return frames
	var off := 12
	var fmt_ok := false
	var channels := 0
	var bits := 0
	var data_off := -1
	var data_len := 0
	while off + 8 <= bytes.size():
		var id := bytes.slice(off, off + 4)
		var sz := bytes.decode_u32(off + 4)
		var body := off + 8
		if body + sz > bytes.size():
			_humm_error = "块 %s 越界（%d @%d）" % [id.get_string_from_ascii(), sz, body]
			return frames
		if id == "fmt ".to_ascii_buffer():
			var fmt := bytes.decode_u16(body)
			channels = bytes.decode_u16(body + 2)
			_humm_rate = bytes.decode_u32(body + 4)
			bits = bytes.decode_u16(body + 14)
			fmt_ok = (fmt == 1 and bits == 16)
		elif id == "data".to_ascii_buffer():
			data_off = body
			data_len = sz
		off = body + sz + (sz & 1)
	if not fmt_ok:
		_humm_error = "只支持 PCM 16-bit（channels=%d bits=%d）" % [channels, bits]
		return frames
	if data_off < 0 or data_len <= 0:
		_humm_error = "没有 data 块"
		return frames
	if channels != 1:
		# 原版的 humm.wav 是单声道 16-bit（docs/111 §1.3 的 RIFF 头）；
		# 多声道也不难，但没有证据说原版支持，这里显式拒绝而不是猜。
		_humm_error = "只支持单声道（channels=%d）" % channels
		return frames
	var sample_count := data_len / 2
	frames.resize(sample_count)
	for i in sample_count:
		var v := float(bytes.decode_s16(data_off + i * 2)) / PCM_NORMALIZE
		frames[i] = Vector2(v, v)        # Godot 的 generator 帧就是 (L, R)
	return frames

# ------------------------------------------------------------------ 播放

## 是否有这个音效且真的加载成功。
func loaded(sound: String) -> bool:
	if String(SOUNDS.get(sound, {}).get("kind", "")) == "generator":
		return not _humm_frames.is_empty()
	return _streams.has(sound)

## 缺资产/加载失败的音效名（selftest 用它决定 SKIP 与原因文本）。
func missing() -> Array[String]:
	return _missing.duplicate()

func loaded_names() -> Array[String]:
	var out: Array[String] = []
	for n in SOUNDS:
		if loaded(n):
			out.append(n)
	return out

## 指定名字在 **哪个 player 槽** 播（原版槽位语义：同一时刻一个槽只能有一条流）。
func slot_of(sound: String) -> String:
	return String(SOUNDS.get(sound, {}).get("slot", "0xa0"))

## 播放一个音效。缺资产 ⇒ 返回 false（不崩、不报错到控制台之外）。
func play(sound: String, slot := "") -> bool:
	if not SOUNDS.has(sound):
		push_warning("[gw2/audio] 未知音效：%s" % sound)
		return false
	if not loaded(sound):
		return false
	var use_slot := slot if slot != "" else slot_of(sound)
	if String(SOUNDS[sound].kind) == "generator":
		return start_humm()
	var player := _ensure_player(use_slot)
	if player == null:
		return false
	player.stream = _streams[sound]
	# 音量按**槽位**（原版的音量是槽的属性，不是音效的属性）。
	var db: float = float(SLOT_VOLUME_DB.get(use_slot, -15.0))
	player.volume_db = db
	player.play()
	played.emit(sound, use_slot)
	return true

## 取/建一个 player（原版 `0x4bf090`/`0x4bf340`/`0x4bf480` 都是"取/建"合一）。
func _ensure_player(slot: String) -> AudioStreamPlayer:
	if _players.has(slot):
		return _players[slot]
	var p := AudioStreamPlayer.new()
	p.name = "Sound_%s" % slot
	p.bus = "Master"
	p.volume_db = float(SLOT_VOLUME_DB.get(slot, -15.0))
	add_child(p)
	_players[slot] = p
	return p

func stop_all() -> void:
	for slot in _players:
		(_players[slot] as AudioStreamPlayer).stop()
	stop_humm()

func is_playing(sound: String) -> bool:
	if String(SOUNDS.get(sound, {}).get("kind", "")) == "generator":
		return _humm_active()
	var slot := slot_of(sound)
	if not _players.has(slot):
		return false
	var p: AudioStreamPlayer = _players[slot]
	return p.playing and p.stream == _streams.get(sound)

# ------------------------------------------------ humm：AudioStreamGenerator

## `docs/111` §4.4(b) 那条路：generator(16000 Hz, 0.1 s 缓冲) + `push_buffer`。
##
## 与 mp3 的区别是**结构性**的：mp3 是"`set_stream` + `play()` 一次性"，
## 而 humm 每帧要往缓冲里推 PCM（`0x4c05d3` 取可用帧数 → `0x4c0a5d` push）。
func start_humm() -> bool:
	if _humm_frames.is_empty():
		return false
	var player := _ensure_player("0xc0") as AudioStreamPlayer
	if player == null:
		return false
	if _humm_active():
		return true
	var gen := AudioStreamGenerator.new()
	gen.mix_rate = GEN_MIX_RATE            # 原版硬编码 16000.0f（WAV 本身也是 16000）
	gen.buffer_length = GEN_BUFFER_LENGTH  # 0.1f
	player.stream = gen
	player.volume_db = float(SLOT_VOLUME_DB["0xc0"])
	player.play()
	_humm_cursor = 0
	set_process(true)
	played.emit("humm", "0xc0")
	return true

func stop_humm() -> void:
	if _players.has("0xc0"):
		(_players["0xc0"] as AudioStreamPlayer).stop()
	set_process(_humm_active())

func _humm_active() -> bool:
	if not _players.has("0xc0"):
		return false
	return (_players["0xc0"] as AudioStreamPlayer).playing

func _process(_delta: float) -> void:
	_push_generator_frames()
	set_process(_humm_active())

## 把 PCM 推进 generator 缓冲（循环播放）。可用帧数用 `get_frames_available()` 问 —— 与原版
## `0x4c05d3` 的 `get_frames_available()` 同一语义。
func _push_generator_frames() -> void:
	if not _humm_active():
		return
	var player: AudioStreamPlayer = _players["0xc0"]
	var playback := player.get_stream_playback() as AudioStreamGeneratorPlayback
	if playback == null:
		return
	var avail := playback.get_frames_available()
	if avail <= 0:
		return
	var total := _humm_frames.size()
	var chunk := PackedVector2Array()
	chunk.resize(mini(avail, 2048))
	var fill := 0
	while fill < chunk.size():
		chunk[fill] = _humm_frames[_humm_cursor]
		_humm_cursor = (_humm_cursor + 1) % total
		fill += 1
	playback.push_buffer(chunk)

# ------------------------------------------------------------------ 静音

## Master 总线下标（原版 `0x4dd070` = `AudioServer::get_bus_index("Master")`）。
func master_bus() -> int:
	return AudioServer.get_bus_index("Master")

func bus_muted() -> bool:
	var bus := master_bus()
	if bus < 0:
		return false
	return AudioServer.is_bus_mute(bus)

## 设置静音。**只动 Master 总线** —— 原版 `0x4c13c7` 的 `set_bus_mute`，不是逐 player 调音量。
## 返回这次调用是否真的改变了状态。
func set_muted(muted: bool) -> bool:
	var changed := (_muted != muted) or (bus_muted() != muted)
	_muted = muted
	var bus := master_bus()
	if bus >= 0:
		AudioServer.set_bus_mute(bus, muted)
	if changed:
		mute_changed.emit(muted)
	return changed

func is_muted() -> bool:
	return _muted

func toggle_mute() -> bool:
	set_muted(not _muted)
	return _muted

## 把**总线的实际状态**采纳为内部状态（"双向同步"的另一半）：
## 别的代码（原版语义的 `play_screen.toggle_mute`）直接写了 `set_bus_mute` 时，
## 我们在这里把变化接回来，并让 `mute_changed` 冒出去给 `App` 落盘。
func adopt_bus() -> bool:
	var bus_state := bus_muted()
	if bus_state == _muted:
		return false
	_muted = bus_state
	mute_changed.emit(_muted)
	return true

## 原版 `0x4c11d0` 的有效静音是 `arg | self[0xd1]` —— 第二个标志位的语义未知
## （`docs/111` §5.3）。这里只实现已知的那一半（`arg`），并把未知写进文档。
