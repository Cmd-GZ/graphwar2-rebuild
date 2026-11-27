extends Node
class_name GwAudio
## 音频播放层 —— **规则在 Rust**（`graphwar_audio_control`），本文件只留"引擎播放"
## （加载资源 / 建 `AudioStreamPlayer` / 推 generator 缓冲 / 发信号 / 调 `AudioServer`）。
##
## 原版 `GraphwarAudioControl`（结构体 0xf0 字节）的复原实现在 `src/graphwar_audio_control.rs`，
## 逐位证据：5 次 `load()` 的写入顺序（`+0x80..+0xc0`）、音量立即数、generator 常数、
## 静音双标志 `+0xd0/+0xd1`。逐段台账 `fixtures/controller_migration2_ledger.tsv` A01–A12。
##
## ⚠️ 改前本文件的槽位表**五条全错**（自造），改后一律从 Rust 取 —— 资源/槽/音量的真值表：
##
## | 音效 | 改前 | 真值槽 | 真值音量 | 改前的后果 |
## |---|---|---|---|---|
## | `explosion` | `0xa0` | `0x80` | -15 dB | 槽错（音量巧合相同） |
## | `squash` | `0x80` | `0x90` | -15 dB | 同上 |
## | `spark` | `0x90` | `0xa0` | -15 dB | 同上 |
## | `pop` | `0xa0` | `0xb0` | **0 dB** | 按 -15 dB 播 ⇒ **小 15 dB** |
## | `unpop` | `0x80` | `0xc0` | **-5 dB** | 按 -15 dB 播 ⇒ **小 10 dB** |
## | `humm` | 被写成占 `0xc0` | 不占 5 槽 | —（generator） | `0xc0` 真值是 `unpop.mp3` |
##
## 设计边界（保留）：本层**只认"名字"**，不硬编码"哪个事件放哪个音"（那由调用方按
## `GwAudioControl.slot_of_effect_kind(kind)` 取槽，原版是 `0x4bf090` 的四臂）；
## 缺资产不崩（记进 `missing()`，selftest 显式 SKIP）；player 懒建（原版 `0x4bf090`/`0x4bf340`）。

## 播放完成（给自检/证据用）：`play()` 真的把 stream 交给了一个 player 就发。
signal played(sound: String, slot: String)
## 静音状态变化（**两个方向**都发：我们自己切的、和外部直接改总线后被采纳的）。
signal mute_changed(muted: bool)

const SOUND_DIR := "res://assets/sounds"

## 资产目录（`GwAssets` 用同一个约定：可被 `GW2_ASSET_DIR` 覆盖）。
## 做成可变字段而不是只留常量，是为了让 selftest 能构造一个"目录不存在"的实例，
## 用来验证"缺资产 ⇒ 静默降级而不崩"。
var sound_dir := SOUND_DIR

## `humm` 走 `AudioStreamGenerator`，**不在**原版那 5 个资源槽里（`0xc0` 是 `unpop.mp3`）。
## 所以它需要一个自己的 player 键。这个串是**我方选择**（原版生成器挂在哪个 `Gd` 上
## 仍是推断，见 `graphwar_audio_control` 模块文档 §6），但"它不是 5 槽之一"是逐位结论。
const HUMM_SLOT := "humm"

## `humm` 那条路的音量 —— **推断**。
##
## `-5.0f` 在二进制里属于 `entity_removed_event` 那个槽（`+0xc0` = `unpop.mp3`，
## `graphwar_audio_control` 模块文档 §3 末段已纠正 `docs/111` §4.5 的旧归类），
## 而 generator 所在的 player 音量**没有观测**（模块文档 §6）。
## 这里保留改前的 -5.0 dB 作为我方选择，明确标推断 —— 不假装它来自某个立即数。
const HUMM_VOLUME_DB := -5.0

## 音效表：**只留"资产"这一半**（文件名 / origin / kind / evidence）。
##
## `slot` 字段只给**原版没有引用过**的那四条（tear / humm_2 / humm_b / shepard_2）——
## 它们是"我方提供、档位推断"，所以槽位只能是我的选择；
## 有原版证据的五条（explosion/squash/spark/pop/unpop）**不写槽**，一律按 `origin`
## 路径去 Rust 的 `SoundSlot` 表里匹配（见 `slot_of()`）。
const SOUNDS := {
	"explosion": {
		"file": "explosion.mp3", "kind": "mp3",
		"origin": "res://sounds/explosion.mp3",
		"evidence": "docs/111 §4.4(a) 第 1 个 load()；槽 +0x80、音量 -15.0f",
	},
	"squash": {
		"file": "squash.mp3", "kind": "mp3",
		"origin": "res://sounds/squash.mp3",
		"evidence": "docs/111 §4.4(a) 第 2 个 load()；槽 +0x90、音量 -15.0f",
	},
	"spark": {
		"file": "spark.mp3", "kind": "mp3",
		"origin": "res://sounds/spark.mp3",
		"evidence": "docs/111 §4.4(a) 第 3 个 load()；槽 +0xa0、音量 -15.0f",
	},
	"pop": {
		"file": "pop.mp3", "kind": "mp3",
		"origin": "res://sounds/pop.mp3",
		"evidence": "docs/111 §4.4(a) 第 4 个 load()；槽 +0xb0、音量 **0 dB**（soldier_info 那一路）",
	},
	"unpop": {
		"file": "unpop.mp3", "kind": "mp3",
		"origin": "res://sounds/unpop.mp3",
		"evidence": "docs/111 §4.4(a) 第 5 个 load()；槽 +0xc0、音量 **-5.0f**（entity_removed 那一路）",
	},
	"tear": {
		"file": "tear.mp3", "kind": "mp3", "slot": "0x90",
		"origin": "res://sounds/tear.mp3",
		"evidence": "**原版没有加载它**（docs/111 §5.2）；按「可用但未接线」提供，槽/音量属**推断**",
	},
	"humm": {
		"file": "humm.wav", "kind": "generator",
		"origin": "res://sounds/humm.wav",
		"evidence": "docs/111 §4.3/§4.4(b)：自读字节 + hound 解析 + AudioStreamGenerator；不占 5 槽",
	},
	"humm_2": {
		"file": "humm_2.wav", "kind": "wav", "slot": "0xa0",
		"origin": "res://sounds/humm_2.wav",
		"evidence": "**原版没有引用它**（docs/111 §5.2）；槽位属**推断**",
	},
	"humm_b": {
		"file": "humm_b.wav", "kind": "wav", "slot": "0x80",
		"origin": "res://sounds/humm_b.wav",
		"evidence": "**原版没有引用它**（docs/111 §5.2）；PCM 与 humm.wav 的 data 块逐字节相同",
	},
	"shepard_2": {
		"file": "shepard_2.wav", "kind": "wav", "slot": "0x90",
		"origin": "res://sounds/shepard_2.wav",
		"evidence": "**原版没有引用它**（docs/111 §5.2：105 个 .gd + 31 个 .tscn 零引用）",
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

## 用户静音（原版 `self + 0xd0`）与内部认定的状态
var _muted := false
## 隐式静音（原版 `self + 0xd1`）。有效静音 = 两个标志相或 —— **规则在 Rust**
## （`GwAudioControl.effective_mute` ← `orr w8, w19, w22`）。本文件只负责把它写到总线上。
var _hidden_muted := false

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

## 读一个 WAV → `Vector2` 帧（**已归一化** `÷32767`）。
##
## 解析与归一化**都在 Rust**：`GwAudioControl.decode_wav` 是原版那一步 `hound`（`0x50a080`）
## 的等价物，归一化常数 `32767.0f` 来自 `0x4c08f6`（逐位）。本文件不再碰 RIFF 布局。
func _decode_wav_frames(path: String) -> PackedVector2Array:
	_humm_rate = 0
	var res := GwAudioControl.decode_wav(FileAccess.get_file_as_bytes(path))
	if not bool(res.get("ok", false)):
		_humm_error = String(res.get("error", "unknown"))
		return PackedVector2Array()
	_humm_rate = int(res.get("rate", 0))
	var frames = res.get("frames", null)
	if frames is PackedVector2Array:
		return frames
	return PackedVector2Array()

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
##
## 判据：先按 `origin` 路径去 Rust 的 5 个 `SoundSlot` 里匹配（逐位表）；匹配不到的是
## 原版没引用过的资产，退回表里写明的 `slot`（**推断**）；generator 走自己的键。
func slot_of(sound: String) -> String:
	var spec: Dictionary = SOUNDS.get(sound, {})
	if String(spec.get("kind", "")) == "generator":
		return HUMM_SLOT
	var origin := String(spec.get("origin", ""))
	for slot in GwAudioControl.slot_names():
		if String(GwAudioControl.slot_path(slot)) == origin:
			return String(slot)
	return String(spec.get("slot", "0xa0"))

## 某槽的音量（dB）。**值从 Rust 取**（`SoundSlot::volume_db`，逐位立即数）。
func volume_db_of(slot_name: String) -> float:
	return float(GwAudioControl.slot_volume_db(slot_name))

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
	player.volume_db = volume_db_of(use_slot)
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
	p.volume_db = volume_db_of(slot)
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
## 三个常数（16000 / 0.1 / 32767）与一帧预算 1600 全部从 Rust 取。
func start_humm() -> bool:
	if _humm_frames.is_empty():
		return false
	var player := _ensure_player(HUMM_SLOT) as AudioStreamPlayer
	if player == null:
		return false
	if _humm_active():
		return true
	var gen := AudioStreamGenerator.new()
	gen.mix_rate = float(GwAudioControl.hum_mix_rate())          # 0x467a0000 = 16000.0f
	gen.buffer_length = float(GwAudioControl.hum_buffer_length())  # 0x3dcccccd = 0.1f
	player.stream = gen
	player.volume_db = HUMM_VOLUME_DB
	player.play()
	_humm_cursor = 0
	set_process(true)
	played.emit("humm", HUMM_SLOT)
	return true

func stop_humm() -> void:
	if _players.has(HUMM_SLOT):
		(_players[HUMM_SLOT] as AudioStreamPlayer).stop()
	set_process(_humm_active())

func _humm_active() -> bool:
	if not _players.has(HUMM_SLOT):
		return false
	return (_players[HUMM_SLOT] as AudioStreamPlayer).playing

func _process(_delta: float) -> void:
	_push_generator_frames()
	set_process(_humm_active())

## 把 PCM 推进 generator 缓冲（循环播放）。可用帧数用 `get_frames_available()` 问 —— 与原版
## `0x4c05d3` 的 `get_frames_available()` 同一语义。单次预算 = `mix_rate × buffer_length` = 1600。
func _push_generator_frames() -> void:
	if not _humm_active():
		return
	var player: AudioStreamPlayer = _players[HUMM_SLOT]
	var playback := player.get_stream_playback() as AudioStreamGeneratorPlayback
	if playback == null:
		return
	var avail := playback.get_frames_available()
	if avail <= 0:
		return
	var total := _humm_frames.size()
	var chunk := PackedVector2Array()
	chunk.resize(mini(avail, int(GwAudioControl.hum_frames_per_buffer())))
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

## 把"有效静音"写到 Master 总线。
##
## **有效静音 = 用户静音 ∥ 隐式静音** —— 这条规则在 Rust（`GwAudioControl.effective_mute`
## ← `orr w8, w19, w22`）：原版写总线时用的是 `arg | 另一个标志`，所以两个标志任一为真都静音。
func _apply_mute() -> bool:
	var effective := bool(GwAudioControl.effective_mute(_muted, _hidden_muted))
	var bus := master_bus()
	if bus >= 0:
		AudioServer.set_bus_mute(bus, effective)
	return effective

## 设置用户静音（原版 `set_muted` 写 `+0xd0`）。返回这次调用是否真的改变了状态。
func set_muted(muted: bool) -> bool:
	var changed := (_muted != muted) or (bus_muted() != muted)
	_muted = muted
	_apply_mute()
	if changed:
		mute_changed.emit(_muted)
	return changed

## 设置隐式静音（原版 `set_hidden_muted` 写 `+0xd1`）。
##
## 改前本文件**没有这一位**（只跟踪一个标志），所以"别的代码让总线静音"这条只能靠
## `adopt_bus()` 反推。现在两个标志分开记，判据在 Rust。
## ⚠️ 客户端目前**没有**调用方（原版何时置这一位未观测）；保留它是为了让规则完整。
func set_hidden_muted(hidden: bool) -> bool:
	var changed := _hidden_muted != hidden
	_hidden_muted = hidden
	_apply_mute()
	if changed:
		mute_changed.emit(bool(GwAudioControl.effective_mute(_muted, _hidden_muted)))
	return changed

func is_muted() -> bool:
	return _muted

func is_hidden_muted() -> bool:
	return _hidden_muted

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
