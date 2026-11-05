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

## 原版玩家的**默认外观**（`docs/92` §4.14 / §4.10 观测）：yellow 皮肤 + regular_eyes + soldier_helmet。
## 这三个下标 = `GwIcons` 表里的位置，不是随手取的 0：
##   skin_7 = skins/yellow、face_2 = faces/regular_eyes、hat_1 = hats/soldier_helmet。
## （0 号分别是 blue / no_face / no_hat，其中后两个是**全透明**图，预览会变成一颗光头。）
const DEFAULT_SKIN := 7
const DEFAULT_FACE := 2
const DEFAULT_HAT := 1

var profile := {
	"name": "user",
	"skin": DEFAULT_SKIN,
	"face": DEFAULT_FACE,
	"hat": DEFAULT_HAT,
}

## 已通关的关卡 id 列表（本客户端自己记的一份，仅给"打勾"提示用）。
var completed_levels: Array[int] = []
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
	if cf.load(PATH) == OK:
		profile.name = String(cf.get_value(SECTION_PROFILE, "name", profile.name))
		profile.skin = clampi(int(cf.get_value(SECTION_PROFILE, "skin", DEFAULT_SKIN)), 0, SKIN_COUNT - 1)
		profile.face = clampi(int(cf.get_value(SECTION_PROFILE, "face", DEFAULT_FACE)), 0, FACE_COUNT - 1)
		profile.hat = clampi(int(cf.get_value(SECTION_PROFILE, "hat", DEFAULT_HAT)), 0, HAT_COUNT - 1)
		skins_unlocked = bool(cf.get_value(SECTION_PROFILE, "skins_unlocked", false))
		var done: Array = cf.get_value(SECTION_CAMPAIGN, "completed", [])
		completed_levels.clear()
		for v in done:
			completed_levels.append(int(v))
	# 原版存档：**只读，不创建**（原版删掉后重启不会重建，docs/124 §3.2）。
	campaign_progress = read_campaign_progress()

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
