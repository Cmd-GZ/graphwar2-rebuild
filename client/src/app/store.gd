extends RefCounted
class_name GwStore
## 本地存档：玩家档案 + 战役进度。
##
## 位置：`user://gw2_client.cfg`（Godot 的 user 目录；Linux 下通常是
## `$XDG_DATA_HOME/godot/app_userdata/<项目名>/`）。**不写游戏自己的存档目录**，
## 也不读它——原版存档格式未解，见 docs/108 §7。

const PATH := "user://gw2_client.cfg"

const SECTION_PROFILE := "profile"
const SECTION_CAMPAIGN := "campaign"

## 玩家的外观选项数量。观测依据：`docs/92` §4.14 —— Skins 15 个按钮、
## Faces 15 个槽、Hats 27 个槽（5×3 / 5×3 / 9×3，来自 `shop_button_entry.tscn` 实例数 42）。
const SKIN_COUNT := 15
const FACE_COUNT := 15
const HAT_COUNT := 27

var profile := {
	"name": "user",
	"skin": 0,
	"face": 0,
	"hat": 0,
}

## 已通关的关卡 id 列表（战役解锁状态）。
var completed_levels: Array[int] = []
## 是否已"解锁皮肤"（内购模拟位；Linux 端不做内购）。
var skins_unlocked := false

func load_all() -> void:
	var cf := ConfigFile.new()
	if cf.load(PATH) != OK:
		return
	profile.name = String(cf.get_value(SECTION_PROFILE, "name", profile.name))
	profile.skin = clampi(int(cf.get_value(SECTION_PROFILE, "skin", 0)), 0, SKIN_COUNT - 1)
	profile.face = clampi(int(cf.get_value(SECTION_PROFILE, "face", 0)), 0, FACE_COUNT - 1)
	profile.hat = clampi(int(cf.get_value(SECTION_PROFILE, "hat", 0)), 0, HAT_COUNT - 1)
	skins_unlocked = bool(cf.get_value(SECTION_PROFILE, "skins_unlocked", false))
	var done: Array = cf.get_value(SECTION_CAMPAIGN, "completed", [])
	completed_levels.clear()
	for v in done:
		completed_levels.append(int(v))

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

func mark_completed(level_index: int) -> void:
	if not completed_levels.has(level_index):
		completed_levels.append(level_index)
		save_all()

func is_completed(level_index: int) -> bool:
	return completed_levels.has(level_index)
