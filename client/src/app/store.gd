extends RefCounted
class_name GwStore
## 本地存档：**原版战役进度** + 我们自己的客户端档案。
##
## 两个文件，来源完全不同，别混：
##
## 1. `user://campaign.save` —— **原版的战役存档**（格式已确认，逐位要求，见下）。
## 2. `user://gw2_client.cfg` —— 本客户端自己的（名字/外观/内购位）。原版没有这个文件，
##    我们也不假装它是原版的（`docs/108` §7 第 19 条）。
##
## 原版 `campaign.save` 的格式（**代码 + 用户实测双重确认**，docs/124 §3.2）
## ------------------------------------------------------------------
## * 文件内容**恰好**是 `level_<n>`，**行尾不能有任何字节**（没有 `\n`、没有空格）；
## * 读法是 `split('_').last()` 然后 `parse::<u32>()`；Rust 的 `parse` **不跳空白**，
##   多一个 `\n` 就让整个解析失败、进度归零（用户实测踩过）；
## * 语义：`level_<n>` = **最后通关的那一关**（0 基）⇒ `0..=n` 都是解锁的，
##   下一关（`n+1`）是当前可玩的那一关；
## * **写**只在"解锁新关卡"时发生，而且**单调**（只有新值 > 旧值才写）；
## * **启动时不创建这个文件**（原版删掉后重启不会重建）；
## * 原版自己写出来的内容也没有尾换行。

const PATH := "user://gw2_client.cfg"

## 原版战役存档（**不是**我们发明的路径）。
const CAMPAIGN_PATH := "user://campaign.save"

const SECTION_PROFILE := "profile"
const SECTION_CAMPAIGN := "campaign"

## 玩家的外观选项数量。观测依据：`docs/92` §4.14 —— Skins 15 个按钮、
## Faces 15 个槽、Hats 27 个槽（5×3 / 5×3 / 9×3，来自 `shop_button_entry.tscn` 实例数 42）。
const SKIN_COUNT := 15
const FACE_COUNT := 15
const HAT_COUNT := 27

## 原版玩家的**默认外观** = yellow + regular_eyes + soldier_helmet。
##
## 三条独立证据（`docs/137` §6）：
##   ① **全新 `XDG_DATA_HOME`**（`user://player.config` 不存在）跑正版二进制 ⇒ 预览画的是
##      黄色圆 + 绿色士兵盔 + regular_eyes（截图 `truth/player_skins.png`）；
##   ② `player.config` 里**只**写 `skin: red` ⇒ 皮肤红、脸仍是 regular_eyes、帽子仍是
##      soldier_helmet ⇒ 原版是**逐键兜底**，不是"文件在就整份覆盖"；
##   ③ 用户提供的 App Store 图标 = 这三张图的合成（黄圆 → 白眼黑瞳 → 绿盔描边）。
##
## ⚠️ **不要写死下标**：`GwIcons.SKINS` 是 `.rodata` 顺序（含 `gray`/`target`），而原版
## 界面网格跳过那两张、顺序也不同（`docs/137` §6.3 把两套顺序都列出来了）。默认值表达的
## 是"yellow 这件皮肤"，所以这里写**名字**，下标在类加载时按名字解析。
const DEFAULT_SKIN_NAME := "yellow"
const DEFAULT_FACE_NAME := "regular_eyes"
const DEFAULT_HAT_NAME := "soldier_helmet"

static var DEFAULT_SKIN := 0
static var DEFAULT_FACE := 0
static var DEFAULT_HAT := 0

static func _static_init() -> void:
	DEFAULT_SKIN = resolve_look("skins", DEFAULT_SKIN_NAME)
	DEFAULT_FACE = resolve_look("faces", DEFAULT_FACE_NAME)
	DEFAULT_HAT = resolve_look("hats", DEFAULT_HAT_NAME)

## 名字 → `GwIcons` 表下标。找不到返回 -1（调用方自己决定兜底，别在这里瞎猜 0）。
static func resolve_look(dir: String, short_name: String) -> int:
	var table: Array = []
	match dir:
		"skins": table = GwIcons.SKINS
		"faces": table = GwIcons.FACES
		"hats": table = GwIcons.HATS
		_: return -1
	return table.find("%s/%s" % [dir, short_name])

var profile := {
	"name": "user",
	"skin": DEFAULT_SKIN,
	"face": DEFAULT_FACE,
	"hat": DEFAULT_HAT,
}

## 已通关的关卡 id 列表（本客户端自己记的一份，仅给"打勾"提示用）。
var completed_levels: Array[int] = []

## 静音开关（`docs/128` §5）。
##
## 这是**原版 `user://player.config` 里那 5 个字段之一**（`name/skin/face/hat/muted`，
## `docs/118` §3.2）。本项目把它存在**自己的** `gw2_client.cfg` 里（`[profile] muted`），
## **不写**原版那个文件：原版写者的字段顺序、`muted` 的取值编码（`1`/`true`?）都只证到
## "分隔符是 `: `、每行以 `\n` 结尾"（`docs/118` §3.2.1 判**推断**），
## 拿未验证的编码去覆盖用户真机的 `player.config` 会把原版设置弄坏。
## ⇒ 我们**读**它（`read_player_config()`，容错解析），**不写**它。
var muted := false
## 是否已"解锁皮肤"（内购模拟位；Linux 端不做内购）。
var skins_unlocked := false

## 原版 `campaign.save` 里的那个 n。`-1` = 没有存档 / 解析失败（两种都当"什么都还没通"）。
var campaign_progress := -1
## 可注入的路径，给单测用（默认就是原版的 `user://campaign.save`）。
var campaign_path := CAMPAIGN_PATH
## 本次会话里是否**成功读到**过原版存档（单测/诊断用）。
var campaign_loaded := false

## 战役总关数（`docs/92` §4.2：21 关，`level_0..level_20`）。
const LEVEL_COUNT := 21

func load_all() -> void:
	var cf := ConfigFile.new()
	var have_own := cf.load(PATH) == OK
	if have_own:
		profile.name = String(cf.get_value(SECTION_PROFILE, "name", profile.name))
		profile.skin = clampi(int(cf.get_value(SECTION_PROFILE, "skin", DEFAULT_SKIN)), 0, SKIN_COUNT - 1)
		profile.face = clampi(int(cf.get_value(SECTION_PROFILE, "face", DEFAULT_FACE)), 0, FACE_COUNT - 1)
		profile.hat = clampi(int(cf.get_value(SECTION_PROFILE, "hat", DEFAULT_HAT)), 0, HAT_COUNT - 1)
		skins_unlocked = bool(cf.get_value(SECTION_PROFILE, "skins_unlocked", false))
		muted = bool(cf.get_value(SECTION_PROFILE, "muted", false))
		var done: Array = cf.get_value(SECTION_CAMPAIGN, "completed", [])
		completed_levels.clear()
		for v in done:
			completed_levels.append(int(v))
	# 我们自己还没有档案（第一次跑本客户端）⇒ 把**原版** `player.config` 套上来。
	# 有了我们自己的档案就不再套：用户在本客户端里的选择优先（两种情况都写进 docs/137 §6）。
	if not have_own:
		apply_original_player_config()
	# 原版存档：**只读，不创建**（原版删掉后重启不会重建，docs/124 §3.2）。
	campaign_progress = read_campaign_progress()

## 把原版 `user://player.config` 的三项外观套到 `profile` 上（**只读，不写回**）。
##
## 实测语义（正版二进制三组实验，`docs/137` §6.2）：
##   * 文件不存在 ⇒ 三项全用默认；
##   * 只写 `skin: red` ⇒ 皮肤 red、脸/帽仍默认 ⇒ **逐键兜底**；
##   * 值是**名字**（`asian` / `asian_hat` 这种），不是下标 ⇒ 按名字解析成我们的表下标；
##   * 未知名字（实验 `skin: bogus`）在原版下会让那一层**不画**（皮肤整个消失）。
##     我们这里保留默认值并告警 —— 那是坏配置下的原版行为，不照抄（理由见 docs/137 §6.4）。
func apply_original_player_config() -> void:
	if not FileAccess.file_exists(PLAYER_CONFIG_PATH):
		return
	var f := FileAccess.open(PLAYER_CONFIG_PATH, FileAccess.READ)
	if f == null:
		push_warning("[gw2] 读不到 %s" % PLAYER_CONFIG_PATH)
		return
	var kv := parse_player_config(f.get_as_text())
	if kv.has("name") and String(kv["name"]) != "":
		profile.name = String(kv["name"])
	# 逐键：键在就解析，键不在就保持默认（原版实测语义，docs/137 §6.2）。
	for pair in [["skin", "skins"], ["face", "faces"], ["hat", "hats"]]:
		var key := String(pair[0])
		var dir := String(pair[1])
		if not kv.has(key):
			continue
		var idx := resolve_look(dir, String(kv[key]))
		if idx < 0:
			push_warning("[gw2] player.config 里 %s=%s 不认识，保留默认值" % [key, kv[key]])
			continue
		profile[key] = idx

# ---------------------------------------------------------------- 原版 campaign.save

## 解析原版存档的**全部**内容（`level_<n>`，其它任何形状都算解析失败 → -1）。
##
## 刻意做成"逐字符校验"而不是 `is_valid_int()`：要复刻的是 Rust
## `split('_').last().parse::<u32>()` 的语义 ——
##   * 先按 '_' 切、**只解析最后一段** ⇒ 前缀里的任意字符都不影响（`" level_0"` 也是 0）；
##   * 最后一段必须是纯 ASCII 十进制数字：`parse::<u32>()` **不跳空白**，
##     所以 `"20\n"` / `"20 "` / `" 20"` 全部失败（尾换行是用户实测踩过的坑）；
##   * 超出 u32 范围失败。
## 这条严格性有用户实测背书：多一个 `\n` 进度就归零（docs/124 §3.2）。
static func parse_campaign_save(raw: String) -> int:
	var idx := raw.rfind("_")
	if idx < 0:
		return -1
	var tail := raw.substr(idx + 1)
	if tail.is_empty():
		return -1
	# 长度先挡一下：u32 最多 10 位十进制数字。
	if tail.length() > 10:
		return -1
	for i in tail.length():
		var c := tail.unicode_at(i)
		if c < 0x30 or c > 0x39:
			return -1
	var v := int(tail)
	if v < 0 or v > 4294967295:
		return -1
	return v

## 读 `campaign.save`。文件不存在 ⇒ -1，并且**绝不创建它**。
func read_campaign_progress() -> int:
	campaign_loaded = false
	if not FileAccess.file_exists(campaign_path):
		return -1
	var f := FileAccess.open(campaign_path, FileAccess.READ)
	if f == null:
		return -1
	var raw := f.get_as_text()
	f.close()
	campaign_loaded = true
	return parse_campaign_save(raw)

## 单调写：只有 `level_index > campaign_progress` 才写。返回是否真的写了。
## 写入内容**没有尾换行**（原版自己写出来的也没有）。
func save_campaign_progress(level_index: int) -> bool:
	if level_index <= campaign_progress:
		return false
	if level_index < 0:
		return false
	var f := FileAccess.open(campaign_path, FileAccess.WRITE)
	if f == null:
		push_warning("[gw2] 战役存档写入失败：%s" % campaign_path)
		return false
	f.store_string("level_%d" % level_index)
	f.close()
	campaign_progress = level_index
	return true

## 当前可玩到的最靠后一关（含）。
##
## `campaign_progress = n` 表示 0..=n 都通了 ⇒ 解锁 `0..=n`；再往下一关（n+1）是
## "当前要打的那一关"，也必须可点。没有存档时至少 0 号可点。
## ⚠️ `+1` 这一步是**推断**：文件只给了"最后通关的那一关"，"下一关自动可玩"这一点
## 没有直接观测，但它是唯一同时满足"`level_20` ⇒ 21 关全解锁"与"没有存档也能玩第 0 关"
## 的规则（docs/124 §3.2 证据分级）。
func highest_unlocked_level() -> int:
	return clampi(campaign_progress + 1, 0, LEVEL_COUNT - 1)

func is_level_unlocked(level_index: int) -> bool:
	return level_index <= highest_unlocked_level()

func save_all() -> void:
	var cf := ConfigFile.new()
	cf.set_value(SECTION_PROFILE, "name", profile.name)
	cf.set_value(SECTION_PROFILE, "skin", profile.skin)
	cf.set_value(SECTION_PROFILE, "face", profile.face)
	cf.set_value(SECTION_PROFILE, "hat", profile.hat)
	cf.set_value(SECTION_PROFILE, "skins_unlocked", skins_unlocked)
	cf.set_value(SECTION_PROFILE, "muted", muted)
	cf.set_value(SECTION_CAMPAIGN, "completed", completed_levels)
	var err := cf.save(PATH)
	if err != OK:
		push_warning("[gw2] 存档写入失败：%s" % PATH)

## 通关一关：写本客户端档案（打勾提示）+ 单调写原版的 `campaign.save`。
func mark_completed(level_index: int) -> void:
	save_campaign_progress(level_index)
	if not completed_levels.has(level_index):
		completed_levels.append(level_index)
		save_all()

func is_completed(level_index: int) -> bool:
	return completed_levels.has(level_index)

# ------------------------------------------------------- 原版 player.config（格式参考）
#
# 原版 `user://player.config`（`.rodata 0x1bfe12`，长度 0x14 = 20）的形状是**逐位验证/观测**的
# （`docs/118` §3.2）：
#
#   * 纯文本，`FileAccess::store_string`（不是 ConfigFile / store_var / JSON / 二进制）；
#   * **每行 `key: value`，没有 `[section]` 头**（`0x417700` 起可见 `fol…": "` 的立即数、
#     `0x4176d9` 写 `'\n'`；`0x416911` 用长度 1 的 `":"` 切）；
#   * 5 个键：`name` / `skin` / `face` / `hat` / `muted`（用户真机文件转述 + 协议字段一一对应）。
#
# 我们只**读**它：`read_player_config()` 把 `key: value` 行解成字典，容错（坏行跳过）。
# 写回原版文件是**故意不做**的（值的编码没有证据），见 `muted` 字段的注释。

const PLAYER_CONFIG_PATH := "user://player.config"

## 解析 `key: value` 文本（每行一条、无 section 头）。返回 {key: value(String)}。
## 容错规则（都写明了，因为这是一份"没有逐位验证过 match 分支"的格式）：
##   * 空行、以及 `#` 开头的注释行跳过；
##   * 没有 `:` 的行跳过（不猜）；
##   * key/value 两端去空白；**值里允许再有 `:`**（只按第一个 `:` 切）。
static func parse_player_config(text: String) -> Dictionary:
	var out := {}
	for line in text.split("\n"):
		var t := line.strip_edges()
		if t.is_empty() or t.begins_with("#"):
			continue
		var idx := t.find(":")
		if idx <= 0:
			continue
		var key := t.substr(0, idx).strip_edges()
		var value := t.substr(idx + 1).strip_edges()
		if key.is_empty():
			continue
		out[key] = value
	return out

## 反向：把字典写成原版的 `key: value` 行（每行一个 `\n`，**没有** section 头）。
## 仅用于单测/格式自证；不写原版文件。
static func format_player_config(fields: Dictionary) -> String:
	var keys := fields.keys()
	keys.sort()
	var lines: Array[String] = []
	for k in keys:
		lines.append("%s: %s" % [String(k), String(fields[k])])
	if lines.is_empty():
		return ""
	return "\n".join(lines) + "\n"

## 读原版 `player.config`（存在才读）。返回 {} 表示"没有这个文件"。
func read_player_config() -> Dictionary:
	if not FileAccess.file_exists(PLAYER_CONFIG_PATH):
		return {}
	var f := FileAccess.open(PLAYER_CONFIG_PATH, FileAccess.READ)
	if f == null:
		return {}
	var raw := f.get_as_text()
	f.close()
	return parse_player_config(raw)

## 原版文件里的静音值 → bool。**取值编码未定**（`docs/118` §3.2.1 判推断），
## 所以这里只接受几种常见写法，遇到不认识的**返回 null**（不猜）。
static func muted_from_config(value: String):
	var v := value.strip_edges().to_lower()
	if v in ["1", "true", "yes", "on"]:
		return true
	if v in ["0", "false", "no", "off"]:
		return false
	return null
